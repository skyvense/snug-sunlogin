# 装 Virtual Display Driver（VDD）：spike 的前置步骤，只需跑一次。
# 不要求管理员身份启动：这个包在 winget 里是「用户作用域」，
# 从管理员上下文装会被 winget 拒绝（实测报错 -1978335107）。
# 策略：在什么上下文就用什么打法——
#   管理员上下文：先试 --scope machine；不行再用 runas /trustlevel 降权跑普通安装；
#   普通上下文：直接普通安装（安装包自己会弹 UAC）。
# 每一步都打印判定，装完以「设备管理器里能枚举到虚拟显示适配器」为准。
$ErrorActionPreference = 'Continue'

function Test-Vdd {
    Get-PnpDevice -Class Display -ErrorAction SilentlyContinue |
        Where-Object { $_.FriendlyName -like '*Virtual Display*' }
}

if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
    Write-Host '没找到 winget：先从 Microsoft Store 装「应用安装程序」，或去 GitHub 的 VirtualDrivers/Virtual-Display-Driver 仓库下最新版安装包手动装。'
    Read-Host '按回车关闭'
    exit 1
}

if (Test-Vdd) {
    Write-Host '虚拟显示适配器已经装好了，不用重装。'
    Read-Host '按回车关闭'
    exit 0
}

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
Write-Host "当前上下文：$(if ($isAdmin) { '管理员' } else { '普通用户' })"

$installed = $false
if ($isAdmin) {
    Write-Host '== 尝试 1：管理员上下文 + --scope machine =='
    winget install --id VirtualDrivers.Virtual-Display-Driver -e --scope machine --accept-source-agreements --accept-package-agreements
    Write-Host "winget 退出码 $LASTEXITCODE"
    if (Test-Vdd) { $installed = $true }
}

if (-not $installed -and $isAdmin) {
    Write-Host '== 尝试 2：降权到普通用户上下文再装（会另开一个 winget 窗口，看着它装完）=='
    & runas.exe /trustlevel:0x20000 'winget install --id VirtualDrivers.Virtual-Display-Driver -e --accept-source-agreements --accept-package-agreements'
    Write-Host '等虚拟显示适配器出现（最多等 90 秒）…'
    for ($i = 0; $i -lt 30 -and -not $installed; $i++) {
        Start-Sleep -Seconds 3
        if (Test-Vdd) { $installed = $true }
    }
}

if (-not $installed -and -not $isAdmin) {
    Write-Host '== 普通上下文：直接安装 =='
    winget install --id VirtualDrivers.Virtual-Display-Driver -e --accept-source-agreements --accept-package-agreements
    Write-Host "winget 退出码 $LASTEXITCODE"
    if (Test-Vdd) { $installed = $true }
}

$dev = Test-Vdd
if ($dev) {
    Write-Host "已检测到虚拟显示适配器：$($dev.FriendlyName)（状态 $($dev.Status)）"
    Write-Host '回到 SnugWindows 点「创建手机屏」：它会把 1080×2346 写进 VDD 配置并重启驱动设备。'
} else {
    Write-Host '还没枚举到虚拟显示适配器。依次排查：'
    Write-Host '1) 如果刚才另开的 winget 窗口还在装，等它结束再打开 SnugWindows 看状态；'
    Write-Host '2) 驱动刚装好有时要重启一次电脑才会出现，重启后打开 SnugWindows 点「刷新显示器」；'
    Write-Host '3) 腾讯电脑管家等安全软件可能拦截驱动安装，有拦截提示要放行；'
    Write-Host '4) 手动兜底：开一个「非管理员」的 PowerShell，运行：'
    Write-Host '   winget install --id VirtualDrivers.Virtual-Display-Driver -e'
}
Read-Host '按回车关闭'
