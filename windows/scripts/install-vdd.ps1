#Requires -RunAsAdministrator
# 装 Virtual Display Driver（VDD）：spike 的前置步骤，只需跑一次。
# 失败就直接报错退出，不静默跳过；每一步的判定都打印出来。
$ErrorActionPreference = 'Stop'

# 1. winget 在不在
if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
    Write-Host '没找到 winget：先从 Microsoft Store 装「应用安装程序」，或去 GitHub 的 VirtualDrivers/Virtual-Display-Driver 仓库下最新版安装包手动装。'
    Read-Host '按回车关闭'
    exit 1
}

# 2. 安装（已签名开源驱动，机主拍板复用，不自研驱动）
winget install --id VirtualDrivers.Virtual-Display-Driver -e --accept-source-agreements --accept-package-agreements
if ($LASTEXITCODE -ne 0) {
    Write-Host "winget 退出码 $LASTEXITCODE，安装可能失败，看上面的输出。"
    Read-Host '按回车关闭'
    exit $LASTEXITCODE
}

# 3. 装后自检：设备管理器里应能看到这块适配器
$dev = Get-PnpDevice -Class Display | Where-Object { $_.FriendlyName -like '*Virtual Display*' }
if ($dev) {
    Write-Host "已检测到虚拟显示适配器：$($dev.FriendlyName)（状态 $($dev.Status)）"
} else {
    Write-Host '还没枚举到虚拟显示适配器：重启一次电脑再打开 SnugWindows 看状态。'
}
Write-Host '回到 SnugWindows 点「创建手机屏」：它会把 1080×2346 写进 VDD 配置并重启驱动设备。'
Read-Host '按回车关闭'
