# 搞通 Windows 虚拟屏：盘点 + 修复 + 验证，一条龙。
#
# 背景：Virtual Display Driver（VDD / MttVDD）的 winget 包「装成功」也可能只是把
# 驱动文件放进了驱动仓库，真正的虚拟显示设备节点（Root\MttVDD）没有被创建出来，
# 于是设备管理器里看不到适配器、桌面上也不会多出虚拟屏。本脚本按层排查并逐层补：
#   1) 环境：VC++ 运行时（驱动加载依赖 vcruntime140.dll）、内存完整性状态；
#   2) 盘点：所有显示适配器（含别人家的 Idd 驱动，只看不动）、当前真实枚举到的显示器；
#   3) 驱动包：不在驱动仓库就从官方 GitHub 发布下载并 pnputil 装入；
#   4) 设备节点：不存在就用 SetupAPI 程序化创建（devcon install 的等价做法）；
#   5) 配置：保证 vdd_settings.xml 里有 1080×2346@60 且 monitors/count ≥ 1；
#   6) 生效验证：重启 VDD 设备，轮询确认虚拟屏真的出现在桌面上，并把分辨率设到手机屏尺寸。
# 不管机器上装没装过其他 Idd 驱动（向日葵的 OrayIddDriver 等）都能跑：别人的驱动只盘点、不碰。
#
# 用法：双击或在 PowerShell 里运行本脚本（会自动提权）；-NoPause 结尾不暂停。
# 全程日志写到脚本同目录的 setup-virtual-display.log，排障时把这个文件发出来即可。
param([switch]$NoPause)

$ErrorActionPreference = 'Continue'
$script:LogPath = Join-Path $PSScriptRoot 'setup-virtual-display.log'
$script:ExitCode = 0

function Log([string]$msg) {
    $line = "[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $msg
    Write-Host $line
    Add-Content -Path $script:LogPath -Value $line -Encoding UTF8
}

function Section([string]$title) {
    Log ''
    Log "===== $title ====="
}

# ---- 提权：建设备节点、装驱动、重启设备都要管理员 ----
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host '需要管理员权限，正在以管理员身份重新启动…（弹出的 UAC 请点「是」）'
    $argList = "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`""
    if ($NoPause) { $argList += ' -NoPause' }
    Start-Process powershell.exe -Verb RunAs -ArgumentList $argList
    exit
}

Log 'Snug for Sunlogin — Windows 虚拟屏一键搞通脚本'
Log "机器：$env:COMPUTERNAME，用户：$env:USERNAME"

# ---- C# 助手：显示器枚举/改分辨率（user32）+ 程序化建设备节点（setupapi/newdev） ----
if (-not ('SnugWinDisplay' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class SnugWinDisplay
{
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DISPLAY_DEVICE
    {
        public int cb;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string DeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceString;
        public int StateFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceID;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceKey;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DEVMODE
    {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
        public short dmSpecVersion;
        public short dmDriverVersion;
        public short dmSize;
        public short dmDriverExtra;
        public int dmFields;
        public int dmPositionX;
        public int dmPositionY;
        public int dmDisplayOrientation;
        public int dmDisplayFixedOutput;
        public short dmColor;
        public short dmDuplex;
        public short dmYResolution;
        public short dmTTOption;
        public short dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
        public short dmLogPixels;
        public int dmBitsPerPel;
        public int dmPelsWidth;
        public int dmPelsHeight;
        public int dmDisplayFlags;
        public int dmDisplayFrequency;
        public int dmICMMethod;
        public int dmICMIntent;
        public int dmMediaType;
        public int dmDitherType;
        public int dmReserved1;
        public int dmReserved2;
        public int dmPanningWidth;
        public int dmPanningHeight;
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern bool EnumDisplayDevices(string lpDevice, uint iDevNum, ref DISPLAY_DEVICE lpDisplayDevice, uint dwFlags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern bool EnumDisplaySettings(string lpszDeviceName, int iModeNum, ref DEVMODE lpDevMode);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern int ChangeDisplaySettingsEx(string lpszDeviceName, ref DEVMODE lpDevMode, IntPtr hwnd, int dwflags, IntPtr lParam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern int ChangeDisplaySettingsEx(string lpszDeviceName, IntPtr lpDevMode, IntPtr hwnd, int dwflags, IntPtr lParam);

    const int ENUM_CURRENT_SETTINGS = -1;
    const int CDS_UPDATEREGISTRY = 0x00000001;
    const int DM_PELSWIDTH = 0x00080000;
    const int DM_PELSHEIGHT = 0x00100000;
    const int DM_DISPLAYFREQUENCY = 0x00400000;

    // 每行一个显示器：DeviceName \t DeviceString \t attached(0/1) \t primary(0/1) \t WxH@Hz
    public static string[] ListDisplays()
    {
        var lines = new List<string>();
        for (uint i = 0; ; i++)
        {
            var dd = new DISPLAY_DEVICE();
            dd.cb = Marshal.SizeOf(typeof(DISPLAY_DEVICE));
            if (!EnumDisplayDevices(null, i, ref dd, 0)) break;
            bool attached = (dd.StateFlags & 0x1) != 0;
            bool primary = (dd.StateFlags & 0x4) != 0;
            string mode = "";
            if (attached)
            {
                var dm = new DEVMODE();
                dm.dmSize = (short)Marshal.SizeOf(typeof(DEVMODE));
                if (EnumDisplaySettings(dd.DeviceName, ENUM_CURRENT_SETTINGS, ref dm))
                    mode = dm.dmPelsWidth + "x" + dm.dmPelsHeight + "@" + dm.dmDisplayFrequency;
            }
            lines.Add(dd.DeviceName + "\t" + dd.DeviceString + "\t" + (attached ? "1" : "0") + "\t" + (primary ? "1" : "0") + "\t" + mode);
        }
        return lines.ToArray();
    }

    // 与 app 里 display.rs 的 set_mode 同口径：写注册表 + 再发一次空调用提交。
    public static int SetMode(string deviceName, int width, int height, int hz)
    {
        var dm = new DEVMODE();
        dm.dmSize = (short)Marshal.SizeOf(typeof(DEVMODE));
        dm.dmPelsWidth = width;
        dm.dmPelsHeight = height;
        dm.dmDisplayFrequency = hz;
        dm.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY;
        int ret = ChangeDisplaySettingsEx(deviceName, ref dm, IntPtr.Zero, CDS_UPDATEREGISTRY, IntPtr.Zero);
        if (ret == 0)
            ChangeDisplaySettingsEx(deviceName, IntPtr.Zero, IntPtr.Zero, 0, IntPtr.Zero);
        return ret;
    }
}

public static class SnugWinDevNode
{
    [StructLayout(LayoutKind.Sequential)]
    public struct SP_DEVINFO_DATA
    {
        public uint cbSize;
        public Guid ClassGuid;
        public uint DevInst;
        public IntPtr Reserved;
    }

    [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern IntPtr SetupDiCreateDeviceInfoList(ref Guid ClassGuid, IntPtr hwndParent);

    [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool SetupDiCreateDeviceInfo(IntPtr DeviceInfoSet, string DeviceName, ref Guid ClassGuid, string DeviceDescription, IntPtr hwndParent, uint CreationFlags, ref SP_DEVINFO_DATA DeviceInfoData);

    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiSetDeviceRegistryProperty(IntPtr DeviceInfoSet, ref SP_DEVINFO_DATA DeviceInfoData, uint Property, byte[] PropertyBuffer, uint PropertyBufferSize);

    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiCallClassInstaller(uint InstallFunction, IntPtr DeviceInfoSet, ref SP_DEVINFO_DATA DeviceInfoData);

    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiDestroyDeviceInfoList(IntPtr DeviceInfoSet);

    [DllImport("newdev.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool UpdateDriverForPlugAndPlayDevices(IntPtr hwndParent, string HardwareId, string FullInfPath, uint InstallFlags, out bool bRebootRequired);

    const uint DICD_GENERATE_ID = 0x00000001;
    const uint SPDRP_HARDWAREID = 0x00000001;
    const uint DIF_REGISTERDEVICE = 0x00000019;
    const uint INSTALLFLAG_FORCE = 0x00000001;

    // 等价于 devcon install <inf> Root\MttVDD：建根设备节点并绑定驱动。
    public static string CreateRootDevice(string deviceName, string hardwareId, string infPath, string description)
    {
        Guid classGuid = new Guid("{4d36e968-e325-11ce-bfc1-08002be10318}"); // 显示类
        IntPtr set = SetupDiCreateDeviceInfoList(ref classGuid, IntPtr.Zero);
        if (set == IntPtr.Zero || set == new IntPtr(-1))
            return "SetupDiCreateDeviceInfoList 失败，Win32 错误 " + Marshal.GetLastWin32Error();
        try
        {
            var data = new SP_DEVINFO_DATA();
            data.cbSize = (uint)Marshal.SizeOf(typeof(SP_DEVINFO_DATA));
            if (!SetupDiCreateDeviceInfo(set, deviceName, ref classGuid, description, IntPtr.Zero, DICD_GENERATE_ID, ref data))
                return "SetupDiCreateDeviceInfo 失败，Win32 错误 " + Marshal.GetLastWin32Error();
            byte[] buf = System.Text.Encoding.Unicode.GetBytes(hardwareId + "\0\0");
            if (!SetupDiSetDeviceRegistryProperty(set, ref data, SPDRP_HARDWAREID, buf, (uint)buf.Length))
                return "SetupDiSetDeviceRegistryProperty 失败，Win32 错误 " + Marshal.GetLastWin32Error();
            if (!SetupDiCallClassInstaller(DIF_REGISTERDEVICE, set, ref data))
                return "DIF_REGISTERDEVICE 失败，Win32 错误 " + Marshal.GetLastWin32Error();
        }
        finally
        {
            SetupDiDestroyDeviceInfoList(set);
        }
        bool reboot;
        if (!UpdateDriverForPlugAndPlayDevices(IntPtr.Zero, hardwareId, infPath, INSTALLFLAG_FORCE, out reboot))
            return "UpdateDriverForPlugAndPlayDevices 失败，Win32 错误 " + Marshal.GetLastWin32Error();
        return reboot ? "OK（需要重启）" : "OK";
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct SP_DRVINFO_DATA
    {
        public uint cbSize;
        public uint DriverType;
        public UIntPtr Reserved;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string Description;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string MfgName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string ProviderName;
        public long DriverDate;
        public ulong DriverVersion;
    }

    [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool SetupDiOpenDeviceInfo(IntPtr DeviceInfoSet, string DeviceInstanceId, IntPtr hwndParent, uint OpenFlags, ref SP_DEVINFO_DATA DeviceInfoData);

    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiBuildDriverInfoList(IntPtr DeviceInfoSet, ref SP_DEVINFO_DATA DeviceInfoData, uint DriverType);

    [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool SetupDiEnumDriverInfo(IntPtr DeviceInfoSet, ref SP_DEVINFO_DATA DeviceInfoData, uint DriverType, uint MemberIndex, ref SP_DRVINFO_DATA DriverInfoData);

    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiDestroyDriverInfoList(IntPtr DeviceInfoSet, ref SP_DEVINFO_DATA DeviceInfoData, uint DriverType);

    [DllImport("newdev.dll", SetLastError = true)]
    static extern bool DiInstallDevice(IntPtr hwndParent, IntPtr DeviceInfoSet, ref SP_DEVINFO_DATA DeviceInfoData, ref SP_DRVINFO_DATA DriverInfoData, uint Flags, out bool NeedReboot);

    const uint SPDIT_CLASSDRIVER = 0x00000002;

    // 给已存在的设备实例绑上驱动仓库里匹配它的驱动（DiInstallDevice）。
    // UpdateDriverForPlugAndPlayDevices 靠硬件 ID 全局找设备，刚建好的节点它可能
    // 找不到（实测报 ERROR_NO_SUCH_DEVINST 0xE000020B）；这条路按实例 ID 打开
    // 设备、从仓库枚举出匹配的驱动直接装，不绕那个弯。
    public static string InstallStagedDriver(string instanceId)
    {
        Guid classGuid = new Guid("{4d36e968-e325-11ce-bfc1-08002be10318}"); // 显示类（节点就是按这个类建的）
        IntPtr set = SetupDiCreateDeviceInfoList(ref classGuid, IntPtr.Zero);
        if (set == IntPtr.Zero || set == new IntPtr(-1))
            return "SetupDiCreateDeviceInfoList 失败，Win32 错误 " + Marshal.GetLastWin32Error();
        try
        {
            var dev = new SP_DEVINFO_DATA();
            dev.cbSize = (uint)Marshal.SizeOf(typeof(SP_DEVINFO_DATA));
            if (!SetupDiOpenDeviceInfo(set, instanceId, IntPtr.Zero, 0, ref dev))
                return "SetupDiOpenDeviceInfo 失败（实例打不开），Win32 错误 " + Marshal.GetLastWin32Error();
            if (!SetupDiBuildDriverInfoList(set, ref dev, SPDIT_CLASSDRIVER))
                return "SetupDiBuildDriverInfoList 失败，Win32 错误 " + Marshal.GetLastWin32Error();
            try
            {
                for (uint i = 0; ; i++)
                {
                    var drv = new SP_DRVINFO_DATA();
                    drv.cbSize = (uint)Marshal.SizeOf(typeof(SP_DRVINFO_DATA));
                    if (!SetupDiEnumDriverInfo(set, ref dev, SPDIT_CLASSDRIVER, i, ref drv)) break;
                    bool match = (drv.Description != null && drv.Description.IndexOf("Virtual Display", StringComparison.OrdinalIgnoreCase) >= 0)
                        || (drv.ProviderName != null && drv.ProviderName.IndexOf("MikeTheTech", StringComparison.OrdinalIgnoreCase) >= 0);
                    if (match)
                    {
                        bool reboot;
                        if (!DiInstallDevice(IntPtr.Zero, set, ref dev, ref drv, 0, out reboot))
                            return "DiInstallDevice 失败，Win32 错误 " + Marshal.GetLastWin32Error();
                        return reboot ? "OK（需要重启）" : "OK";
                    }
                }
                return "驱动仓库里没枚举到匹配的 Virtual Display Driver（驱动包可能没装好）";
            }
            finally { SetupDiDestroyDriverInfoList(set, ref dev, SPDIT_CLASSDRIVER); }
        }
        finally { SetupDiDestroyDeviceInfoList(set); }
    }
}
'@
}

function Get-DisplayList {
    [SnugWinDisplay]::ListDisplays() | ForEach-Object {
        $p = $_ -split "`t"
        [pscustomobject]@{
            Name     = $p[0]
            Desc     = $p[1]
            Attached = ($p[2] -eq '1')
            Primary  = ($p[3] -eq '1')
            Mode     = $p[4]
        }
    }
}

function Test-IsVirtual([object]$d) {
    return ($d.Desc -match 'Virtual|MttVDD|Idd|Indirect') -or ($d.Name -match 'Virtual|MttVDD')
}

function Get-ProblemCode([string]$instanceId) {
    try {
        $p = Get-PnpDeviceProperty -InstanceId $instanceId -KeyName 'DEVPKEY_Device_ProblemCode' -ErrorAction Stop
        return $p.Data
    } catch { return $null }
}

function Get-HardwareIds([string]$instanceId) {
    try {
        $p = Get-PnpDeviceProperty -InstanceId $instanceId -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction Stop
        return @($p.Data)
    } catch { return @() }
}

# 找 VDD 设备。第一版在这里逐个设备取属性，机器上设备条目一多就慢得像卡死；
# 现在先用实例 ID 字符串匹配（不取属性，秒回），最后才对显示类里 ROOT 开头的
# 少数设备做硬件 ID 核对。
function Find-VddDevice {
    $d = Get-PnpDevice -InstanceId 'ROOT\MttVDD\0000' -ErrorAction SilentlyContinue
    if ($d) { return $d }
    $d = Get-PnpDevice -Class Display -ErrorAction SilentlyContinue |
        Where-Object { $_.InstanceId -match 'MttVDD' } | Select-Object -First 1
    if ($d) { return $d }
    $d = Get-PnpDevice -ErrorAction SilentlyContinue |
        Where-Object { $_.InstanceId -match 'MttVDD' } | Select-Object -First 1
    if ($d) { return $d }
    foreach ($dev in (Get-PnpDevice -Class Display -ErrorAction SilentlyContinue |
            Where-Object { $_.InstanceId -like 'ROOT\*' })) {
        if ((Get-HardwareIds $dev.InstanceId) -contains 'Root\MttVDD') { return $dev }
    }
    return $null
}

function Get-DriverInfPath([string]$instanceId) {
    try {
        $p = Get-PnpDeviceProperty -InstanceId $instanceId -KeyName 'DEVPKEY_Device_DriverInfPath' -ErrorAction Stop
        return $p.Data
    } catch { return $null }
}

# ================= 一、环境 =================
Section '一、环境'
$os = Get-CimInstance Win32_OperatingSystem
Log "系统：$($os.Caption)（Build $($os.BuildNumber)）"
if (Test-Path "$env:SystemRoot\System32\vcruntime140.dll") {
Log 'VC++ 运行时：vcruntime140.dll 在'
} else {
Log 'VC++ 运行时：缺 vcruntime140.dll——VDD 驱动依赖它，缺了会加载失败。尝试用 winget 安装 VC++ 2015-2022 x64…'
winget install --id Microsoft.VCRedist.2015+.x64 -e --accept-source-agreements --accept-package-agreements 2>&1 |
ForEach-Object { Log " winget: $_"}
}
$hvci = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity' -Name Enabled -ErrorAction SilentlyContinue).Enabled
if ($hvci -eq 1) { Log '内存完整性（内核隔离）：开启。若后面出现签名类问题码 52，先考虑它（本脚本只报告、不替你关）'}
elseif ($hvci -eq 0) { Log '内存完整性（内核隔离）：关闭'}
else { Log '内存完整性（内核隔离）：状态未知'}

# ================= 二、盘点 =================
Section '二、盘点现有显示设备（别人的 Idd 驱动只看不动）'
foreach ($a in (Get-PnpDevice -Class Display -ErrorAction SilentlyContinue)) {
$pc = Get-ProblemCode $a.InstanceId
Log "适配器：$($a.FriendlyName) | 状态 $($a.Status) | 问题码 $pc | $($a.InstanceId)"
if ($a.InstanceId -notlike 'PCI\*' -and $a.FriendlyName -notmatch 'Intel|AMD|NVIDIA|GeForce|Radeon') {
Log ' ↑ 虚拟/Idd 类适配器（不是物理显卡），只盘点、不动它'
}
}
$allMonitors = Get-PnpDevice -Class Monitor -ErrorAction SilentlyContinue
$okMonitors = @($allMonitors | Where-Object { $_.Status -eq 'OK' })
foreach ($mo in $okMonitors) {
Log "显示器（在用）：$($mo.FriendlyName) | $($mo.InstanceId)"
}
Log "显示器条目共 $($allMonitors.Count) 个，其中状态 OK 的 $($okMonitors.Count) 个（其余是历史残留记录，不逐条打了）"
foreach ($d in (Get-DisplayList)) {
Log "桌面显示：$($d.Name) | $($d.Desc) | 已接入=$($d.Attached) 主屏=$($d.Primary) | 当前 $($d.Mode)"
}

# ================= 三、驱动包 =================
Section '三、VDD 驱动包（MttVDD.inf）'
$infPath = $null
$staged = Get-ChildItem "$env:SystemRoot\INF\oem*.inf" -ErrorAction SilentlyContinue |
Select-String -List -Pattern 'MttVDD' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($staged) {
$infPath = $staged.Path
Log "驱动包已在驱动仓库里：$infPath"
}
$diskInf = $null
# 随包优先：发布 zip 里带了钉住版本的驱动（vdd 子目录），脚本目录排第一——
# 找到就不用再去 GitHub 下载（国内网络下那步最容易失败）
foreach ($root in @($PSScriptRoot, 'C:\VirtualDisplayDriver', "$env:LOCALAPPDATA\Microsoft\WinGet\Packages", $env:ProgramFiles, "$env:USERPROFILE\Downloads")) {
if (-not $diskInf -and (Test-Path $root)) {
$diskInf = Get-ChildItem $root -Recurse -Filter 'MttVDD.inf' -Depth 4 -ErrorAction SilentlyContinue |
Select-Object -First 1
}
}
if ($diskInf) {
Log "磁盘上找到原始 INF：$($diskInf.FullName)"
$infPath = $diskInf.FullName
}
if (-not $infPath) {
Log '驱动仓库和磁盘上都没有 MttVDD.inf：从官方 GitHub 发布下载驱动包…'
try {
$rel = Invoke-RestMethod 'https://api.github.com/repos/VirtualDrivers/Virtual-Display-Driver/releases/latest' -Headers @{ 'User-Agent' = 'SnugSetup'} -TimeoutSec 30
Log "官方最新版：$($rel.tag_name)"
$asset = $rel.assets | Where-Object { $_.name -match '\.zip$' -and $_.name -notmatch 'Control'} |
Sort-Object { if ($_.name -match 'Driver') { 0} else { 1}} | Select-Object -First 1
if ($asset) {
Log "下载 $($asset.name)…"
$zip = Join-Path $env:TEMP 'vdd-driver.zip'
$dst = Join-Path $env:TEMP 'vdd-driver'
Invoke-WebRequest $asset.browser_download_url -OutFile $zip -TimeoutSec 300
if (Test-Path $dst) { Remove-Item $dst -Recurse -Force}
Expand-Archive $zip -DestinationPath $dst -Force
$found = Get-ChildItem $dst -Recurse -Filter 'MttVDD.inf' | Select-Object -First 1
if ($found) { $infPath = $found.FullName; Log "已解压，INF 在 $infPath"}
} else {
Log '发布里没找到合适的驱动 zip（资产列表见日志上一行版本号，可手动去 Releases 看）'
}
} catch {
Log "下载失败：$($_.Exception.Message)"
}
}
if ($infPath) {
# 证书先行：驱动包是自签名（CN=Virtual Display Driver），证书链不受系统信任时
# pnputil 会报「在不受信任提供程序信任的根证书中终止」直接拒绝（2026-11-20
# 同事机器实测）。官方包靠 installCert.bat 把证书导入 Root + TrustedPublisher；
# 这里从驱动包自己的 .cat 里提取签名证书导入，效果相同且不依赖包里附带 .cer。
$infDir = Split-Path $infPath -Parent
$cat = Get-ChildItem $infDir -Filter '*.cat' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($cat) {
try {
$sig = Get-AuthenticodeSignature $cat.FullName
if ($sig.SignerCertificate) {
$cer = Join-Path $env:TEMP 'Virtual_Display_Driver.cer'
[IO.File]::WriteAllBytes($cer, $sig.SignerCertificate.Export('Cert'))
Log "导入驱动签名证书（$($sig.SignerCertificate.Subject)）到 Root + TrustedPublisher…"
certutil.exe -addstore -f Root $cer 2>&1 | ForEach-Object { Log " certutil(Root): $_" }
certutil.exe -addstore -f TrustedPublisher $cer 2>&1 | ForEach-Object { Log " certutil(TP): $_" }
} else {
Log '警告：从 .cat 里没提取到签名证书，pnputil 可能因证书不受信任而失败'
}
} catch {
Log "证书导入失败：$($_.Exception.Message)（pnputil 可能因此装不上驱动）"
}
} else {
Log '警告：INF 同目录没有 .cat 文件，跳过证书导入'
}
Log '执行 pnputil /add-driver 把驱动包装入仓库…'
pnputil.exe /add-driver $infPath /install 2>&1 | ForEach-Object { Log " pnputil: $_"}
} else {
Log '拿不到 MttVDD.inf，无法继续。手动办法：浏览器打开'
Log ' https://github.com/VirtualDrivers/Virtual-Display-Driver/releases'
Log '下载最新版的驱动 zip 解压（或含 VDD Control 的 zip），把解压目录发我看，或解压后重跑本脚本（它会自己找到 INF）。'
$script:ExitCode = 1
}

# ================= 四、驱动活性与设备重建 =================
function Test-VddPipe {
try {
$pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', 'MTTVirtualDisplayPipe', [System.IO.Pipes.PipeDirection]::InOut)
$pipe.Connect(700)
$bytes = [Text.Encoding]::Unicode.GetBytes('PING')
$pipe.Write($bytes, 0, $bytes.Length)
$pipe.Flush()
$buf = New-Object byte[] 64
$resp = ''
try {
$pipe.ReadTimeout = 700
$n = $pipe.Read($buf, 0, 64)
if ($n -gt 0) { $resp = [Text.Encoding]::ASCII.GetString($buf, 0, $n)}
} catch {}
$pipe.Dispose()
if ($resp) { return "管道连通，响应：$resp"}
return '管道连通（无响应属 25.7.23 已知行为）'
} catch { return $null}
}

function Get-Nefcon {
# 随包优先：发布 zip 里带了 nefcon 本体（nefcon 子目录，v1.21.1 x64），有就不用下载
$bundledNefcon = Join-Path $PSScriptRoot 'nefcon\nefconc.exe'
if (Test-Path $bundledNefcon) { Log "nefcon 就绪（随包）：$bundledNefcon"; return $bundledNefcon }
$url = 'https://github.com/nefarius/nefcon/releases/download/v1.21.1/nefcon_v1.21.1.zip'
$zip = Join-Path $env:TEMP 'nefcon_v1.21.1.zip'
$dst = Join-Path $env:TEMP 'nefcon-1.21.1'
try {
if (-not (Test-Path $zip)) { Invoke-WebRequest $url -OutFile $zip -TimeoutSec 120}
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash
if ($hash -ne '28292F4B1417484DB6AD23C47FFC18EA99D3DE3BDB3BBBE60C8ED11703A3780A') {
Log "nefcon 包哈希对不上（$hash），不用它"
return $null
}
if (Test-Path $dst) { Remove-Item $dst -Recurse -Force}
Expand-Archive $zip -DestinationPath $dst -Force
$exe = Join-Path $dst 'x64\nefconc.exe'
if (Test-Path $exe) { Log "nefcon 就绪（SHA256 已验，与参考实现钉住的哈希一致）：$exe"; return $exe}
Log 'nefcon 解压后没找到 x64\nefconc.exe'
} catch { Log "下载 nefcon 失败：$($_.Exception.Message)"}
return $null
}

function Ensure-PackageDir {
if ($infPath) {
$d = Split-Path $infPath -Parent
if ((Test-Path (Join-Path $d 'MttVDD.dll')) -and (Test-Path (Join-Path $d 'mttvdd.cat'))) { return $d}
}
try {
$rel = Invoke-RestMethod 'https://api.github.com/repos/VirtualDrivers/Virtual-Display-Driver/releases/latest' -Headers @{ 'User-Agent' = 'SnugSetup'} -TimeoutSec 30
$asset = $rel.assets | Where-Object { $_.name -match '\.zip$' -and $_.name -notmatch 'Control'} |
Sort-Object { if ($_.name -match 'Driver') { 0} else { 1}} | Select-Object -First 1
$zip = Join-Path $env:TEMP 'vdd-pkg.zip'
$dst = Join-Path $env:TEMP 'vdd-pkg'
Invoke-WebRequest $asset.browser_download_url -OutFile $zip -TimeoutSec 300
if (Test-Path $dst) { Remove-Item $dst -Recurse -Force}
Expand-Archive $zip -DestinationPath $dst -Force
$inf = Get-ChildItem $dst -Recurse -Filter 'MttVDD.inf' | Select-Object -First 1
if ($inf) { return $inf.Directory.FullName}
} catch { Log "重新下载驱动包失败：$($_.Exception.Message)"}
return $null
}

$vdd = $null
if ($script:ExitCode -eq 0) {
Section '四、驱动活性检查与设备重建（nefcon）'
$pipe0 = Test-VddPipe
if ($pipe0) { Log "驱动活性：$pipe0"} else { Log '驱动活性：MTTVirtualDisplayPipe 连不上——驱动 DLL 没在工作（设备管理器显示 OK 只是反射器的状态）'}
$vdd = Find-VddDevice
if ($vdd) {
Log "设备现状：$($vdd.FriendlyName) | $($vdd.InstanceId) | 状态 $($vdd.Status) | 问题码 $(Get-ProblemCode $vdd.InstanceId) | 驱动 INF $(Get-DriverInfPath $vdd.InstanceId)"
}
$needRebuild = (-not $vdd) -or (-not $pipe0)
if ($needRebuild) {
Log '按参考实现（snug-for-windows，已验证可行）的做法，用 nefcon 重建设备节点：'
$nef = Get-Nefcon
$pkg = Ensure-PackageDir
if ($nef -and $pkg) {
if (-not (Test-Path 'C:\VirtualDisplayDriver')) { New-Item -ItemType Directory -Force 'C:\VirtualDisplayDriver' | Out-Null}
Copy-Item (Join-Path $pkg 'MttVDD.inf') 'C:\VirtualDisplayDriver\' -Force
Copy-Item (Join-Path $pkg 'MttVDD.dll') 'C:\VirtualDisplayDriver\' -Force
Copy-Item (Join-Path $pkg 'mttvdd.cat') 'C:\VirtualDisplayDriver\' -Force
Log '驱动三件套已放到 C:\VirtualDisplayDriver（参考实现的布局，nefcon 从这里装）'
$rootInf = 'C:\VirtualDisplayDriver\MttVDD.inf'
if ($vdd) {
Log '先摘掉现有的半成品节点…'
& $nef remove 'Root\MttVDD' 2>&1 | ForEach-Object { Log " nefcon: $_"}
Log "nefcon remove 退出码 $LASTEXITCODE"
for ($i = 0; $i -lt 20; $i++) {
$vdd = Find-VddDevice
if (-not $vdd) { break}
Start-Sleep -Milliseconds 500
}
Log "摘除后设备：$(if ($vdd) { '还在'} else { '已消失'})"
}
Log '用 nefcon 建节点并装驱动…'
& $nef install $rootInf 'Root\MttVDD' '--no-duplicates' 2>&1 | ForEach-Object { Log " nefcon: $_"}
Log "nefcon install 退出码 $LASTEXITCODE（3010 表示需要重启）"
$pipe1 = $null
for ($i = 1; $i -le 10; $i++) {
Start-Sleep -Seconds 3
$vdd = Find-VddDevice
$pipe1 = Test-VddPipe
if ($vdd -and $pipe1) { break}
Log " 重建后第 $i 次检查：设备 $(if ($vdd) { '在'} else { '不在'})，管道 $(if ($pipe1) { '通'} else { '不通'})"
}
if ($vdd) { Log "重建后设备：状态 $($vdd.Status) | 问题码 $(Get-ProblemCode $vdd.InstanceId) | 驱动 INF $(Get-DriverInfPath $vdd.InstanceId)"}
if ($pipe1) { Log "重建后活性：$pipe1"} else { Log '重建后管道仍不通——驱动 DLL 还是没起来，第六段会把系统事件一并转储'}
} else {
Log 'nefcon 或驱动包没就位，跳过重建（细节见上面几行）'
}
}
if (-not $vdd) {
Log '设备节点不在。把本日志发我；兜底仍是官方 VDD Control 的 Install 按钮。'
$script:ExitCode = 1
}
}

# ================= 五、配置 =================
$script:DefaultSettingsXml = @'
<?xml version='1.0' encoding='utf-8'?>
<vdd_settings>
<monitors>
<count>1</count>
</monitors>
<gpu>
<friendlyname>default</friendlyname>
</gpu>
<global>
<g_refresh_rate>60</g_refresh_rate>
<g_refresh_rate>90</g_refresh_rate>
<g_refresh_rate>120</g_refresh_rate>
<g_refresh_rate>144</g_refresh_rate>
</global>
<resolutions>
<resolution>
<width>1920</width>
<height>1080</height>
<refresh_rate>60</refresh_rate>
</resolution>
<resolution>
<width>2560</width>
<height>1440</height>
<refresh_rate>60</refresh_rate>
</resolution>
<resolution>
<width>3840</width>
<height>2160</height>
<refresh_rate>60</refresh_rate>
</resolution>
<resolution>
<width>1080</width>
<height>2346</height>
<refresh_rate>60</refresh_rate>
</resolution>
</resolutions>
<logging>
<SendLogsThroughPipe>true</SendLogsThroughPipe>
<logging>true</logging>
<debuglogging>false</debuglogging>
</logging>
<colour>
<SDR10bit>false</SDR10bit>
<HDRPlus>false</HDRPlus>
<ColourFormat>RGB</ColourFormat>
</colour>
<cursor>
<HardwareCursor>true</HardwareCursor>
<CursorMaxX>128</CursorMaxX>
<CursorMaxY>128</CursorMaxY>
<AlphaCursorSupport>true</AlphaCursorSupport>
<XorCursorSupportLevel>2</XorCursorSupportLevel>
</cursor>
<edid>
<CustomEdid>false</CustomEdid>
<PreventSpoof>false</PreventSpoof>
<EdidCeaOverride>false</EdidCeaOverride>
</edid>
</vdd_settings>
'@

function Ensure-VddSettings {
$dir = 'C:\VirtualDisplayDriver'
$path = Join-Path $dir 'vdd_settings.xml'
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }
$enc = New-Object Text.UTF8Encoding $false
if (-not (Test-Path $path)) {
[IO.File]::WriteAllText($path, $script:DefaultSettingsXml, $enc)
Log "配置文件不存在，已按参考实现同款结构新建：$path（分节结构、含 1080×2346、logging=true）"
return
}
$bak = "$path.setup-script.bak"
if (-not (Test-Path $bak)) { Copy-Item $path $bak; Log "原配置已备份为 $bak" }
$cur = [IO.File]::ReadAllText($path)
Log '—— 当前 vdd_settings.xml 全文 ——'
foreach ($ln in ($cur -split "`r?`n")) { Log "  | $ln" }
Log '—— 全文结束 ——'
if ($cur.Contains('<options>')) {
$bak4 = "$path.pre-v4.bak"
if (-not (Test-Path $bak4)) { Copy-Item $path $bak4; Log "原配置已另备份为 $bak4" }
[IO.File]::WriteAllText($path, $script:DefaultSettingsXml, $enc)
Log '当前配置是驱动包 zip 里的 <options> 老结构，已换成参考实现同款的分节结构（现行驱动认的是这套）'
return
}
$text = $cur
$orig = $cur
$m = [regex]::Match($text, '(?s)<monitors>.*?</monitors>')
if ($m.Success) {
$seg = $m.Value
$cm = [regex]::Match($seg, '<count>\s*(\d+)\s*</count>')
if ($cm.Success) {
if ([int]$cm.Groups[1].Value -lt 1) {
$seg = $seg -replace '<count>\s*\d+\s*</count>', '<count>1</count>'
$text = $text.Substring(0, $m.Index) + $seg + $text.Substring($m.Index + $m.Length)
Log 'monitors/count 原本小于 1，已改为 1'
}
} else {
$seg = $seg -replace '<monitors>', '<monitors><count>1</count>'
$text = $text.Substring(0, $m.Index) + $seg + $text.Substring($m.Index + $m.Length)
Log 'monitors 里没有 count，已补 <count>1</count>'
}
} else {
$text = $text -replace '(<vdd_settings[^>]*>)', '$1<monitors><count>1</count></monitors>'
Log '没有 monitors 节点，已补上'
}
if (-not ($text -match '<width>1080</width>' -and $text -match '<height>2346</height>')) {
$block = "<resolution>`n <width>1080</width>`n <height>2346</height>`n <refresh_rate>60</refresh_rate>`n </resolution>`n "
if ($text.Contains('</resolutions>')) {
$text = $text.Replace('</resolutions>', $block + '</resolutions>')
} elseif ($text.Contains('</vdd_settings>')) {
$text = $text.Replace('</vdd_settings>', "<resolutions>`n $block</resolutions>`n</vdd_settings>")
}
Log '已往配置加入 1080×2346@60 分辨率'
} else {
Log '配置里已有 1080×2346 分辨率'
}
if ($text.Contains('<logging>false</logging>')) {
$text = $text.Replace('<logging>false</logging>', '<logging>true</logging>')
Log '已打开驱动 logging（它会往 C:\VirtualDisplayDriver 写自己的日志，出问题时是第一手证据）'
}
if ($text -ne $orig) {
[IO.File]::WriteAllText($path, $text, $enc)
Log "配置已写入 $path"
} else {
Log '配置无需修改'
}
}
# ================= 六、生效验证 =================
if ($vdd -and $script:ExitCode -eq 0) {
Section '五、配置 vdd_settings.xml'
Ensure-VddSettings

Section '六、重启 VDD 设备并验证虚拟屏'
pnputil.exe /restart-device $vdd.InstanceId 2>&1 | ForEach-Object { Log " pnputil: $_"}
Log '等虚拟屏出现（最多 30 秒，每 10 秒报一次进度）…'
$target = $null
for ($i = 0; $i -lt 15 -and -not $target; $i++) {
Start-Sleep -Seconds 2
$target = Get-DisplayList | Where-Object { $_.Attached -and (Test-IsVirtual $_)} |
Select-Object -First 1
if (-not $target -and ($i + 1) % 5 -eq 0) { Log " 已等 $(($i + 1) * 2) 秒，还没出现，继续等…"}
}
if (-not $target) {
Log '直接重启没等到虚拟屏：改用禁用→启用再试一轮…'
pnputil.exe /disable-device $vdd.InstanceId 2>&1 | ForEach-Object { Log " pnputil: $_"}
Start-Sleep -Seconds 2
pnputil.exe /enable-device $vdd.InstanceId 2>&1 | ForEach-Object { Log " pnputil: $_"}
Log '再等虚拟屏出现（最多 30 秒）…'
for ($i = 0; $i -lt 15 -and -not $target; $i++) {
Start-Sleep -Seconds 2
$target = Get-DisplayList | Where-Object { $_.Attached -and (Test-IsVirtual $_)} |
Select-Object -First 1
if (-not $target -and ($i + 1) % 5 -eq 0) { Log " 已等 $(($i + 1) * 2) 秒，还没出现，继续等…"}
}
}
if ($target) {
Log "虚拟屏已出现：$($target.Name)（$($target.Desc)），当前分辨率 $($target.Mode)"
if ($target.Mode -notmatch '^1080x2346') {
$rc = [SnugWinDisplay]::SetMode($target.Name, 1080, 2346, 60)
Log "设置 1080×2346@60 返回码 $rc（0=成功）"
Start-Sleep -Seconds 1
$now = Get-DisplayList | Where-Object { $_.Name -eq $target.Name} | Select-Object -First 1
Log "设置后分辨率：$($now.Mode)"
}
Log '虚拟屏搞通了。回到 SnugWindows 点「刷新显示器」应能看到这块屏，再点「创建手机屏」走后续测试。'
} else {
$script:ExitCode = 1
$vddNow = Find-VddDevice
if ($vddNow) {
Log "设备现状：状态 $($vddNow.Status)，问题码 $(Get-ProblemCode $vddNow.InstanceId)"
}
Log '—— 取证：驱动自己的日志与系统事件（下一轮分析就靠这些）——'
try {
$allFiles = Get-ChildItem 'C:\VirtualDisplayDriver' -Recurse -File -ErrorAction SilentlyContinue
foreach ($f in $allFiles) { Log " VDD 目录文件：$($f.FullName) | $($f.Length) 字节 | 改于 $($f.LastWriteTime)" }
foreach ($f in ($allFiles | Where-Object { ($_.Extension -eq '.txt' -or $_.Extension -eq '.log') -and $_.Length -lt 131072 })) {
Log "—— $($f.FullName) 内容（尾 120 行）——"
foreach ($ln in (Get-Content $f.FullName -Tail 120 -ErrorAction SilentlyContinue)) { Log "  | $ln" }
}
} catch { Log "读 VDD 目录失败：$($_.Exception.Message)" }
try {
$svc = Get-Service WUDFRd -ErrorAction Stop
Log "WUDFRd 服务（UMDF 反射器）：状态 $($svc.Status)，启动类型 $($svc.StartType)"
} catch { Log "读 WUDFRd 服务失败：$($_.Exception.Message)" }
try {
$hostProcs = Get-Process WUDFHost -ErrorAction Stop
Log "WUDFHost 进程在跑：PID $(($hostProcs | ForEach-Object { $_.Id }) -join ', ')"
} catch { Log 'WUDFHost 进程没在跑（UMDF 宿主不在，驱动不可能真正工作）' }
foreach ($logName in @('Microsoft-Windows-CodeIntegrity/Operational', 'Microsoft-Windows-DriverFrameworks-UserMode/Operational')) {
try {
$evs = @(Get-WinEvent -FilterHashtable @{ LogName = $logName; StartTime = (Get-Date).AddMinutes(-30) } -ErrorAction Stop |
Where-Object { $_.Message -match 'MttVDD|Virtual Display' } | Select-Object -First 10)
foreach ($e in $evs) { Log "事件[$logName] $($e.TimeCreated) ID=$($e.Id) 级别=$($e.LevelDisplayName)：$(($e.Message -split "`n")[0])" }
if ($evs.Count -eq 0) { Log "事件[$logName]：近 30 分钟没有提到 MttVDD 的记录" }
} catch { Log "事件[$logName] 读不到：$($_.Exception.Message)" }
}
Log '随包配置 + logging 已就位、设备也重启过，虚拟屏还是没出现。把本日志发我：里面有当前配置全文、驱动自己的日志和系统事件，下一轮按证据走（重点怀疑：内存完整性拦了驱动 DLL，或 Insider 版本 26300 的 IddCx 兼容问题）。'
}
}

# ================= 总结 =================
Section '总结'
if ($script:ExitCode -eq 0) {
Log '结果：虚拟屏已搞通（细节见上）。'
} else {
Log '结果：还没搞通，卡点见上面标/的那段。把日志发我接着查。'
}
Log "日志文件：$script:LogPath"
if (-not $NoPause) { Read-Host '按回车关闭'}
exit $script:ExitCode
