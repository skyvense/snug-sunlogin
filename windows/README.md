# Snug for Sunlogin — Windows 版（spike / v0.1，Rust）

macOS 版的 Windows 对照版。同一仓库、双平台：macOS 代码仍在仓库根目录
（`Sources/`），Windows 代码全部在 `windows/` 下，版本线分开。

## 本分支的范围（机主 2026-11-19 定）

**只做 spike：验证向日葵手机端能不能用一块 Windows 虚拟屏。手势翻译不做。**

- 虚拟屏：复用开源的 Virtual Display Driver（VDD，已签名；由同捆的
  `setup-virtual-display.ps1` 用官方驱动包 + nefcon 安装），不自研驱动
- 应用本体（`SnugWindows.exe`，Rust + Win32）：
  - 枚举显示器，找到 VDD 虚拟屏
  - 「创建手机屏」：把所选型号的分辨率写进 VDD 配置（`C:\VirtualDisplayDriver\vdd_settings.xml`，
    只动 monitors/count 与手机屏这一条分辨率，原文备份 `.snug-pristine.bak`）、
    重启驱动设备、把虚拟屏设成该模式（200% 缩放时逻辑尺寸即一半，与 mac 版一致）。
    型号库与 mac 版共用同一份 `Assets/phone-models.json`（编译时嵌入），分辨率按型号比例缩放：
    短边上限 1080（默认 iPhone 17 Pro Max = 1080×2346@60）；界面下拉换型号，
    屏在用时会先按旧规格拆净再按新规格重建
  - 「移除手机屏」：恢复配置之外再禁用 VDD 设备（光改配置删不掉这块屏——驱动启用着就一直挂着），并以枚举结果验证它真的消失；下次「创建手机屏」会先把设备启用回来
  - 「主控连入时自动建屏」（默认开）：后台每 3 秒查进程命令行，带 `--mod=desktopagent`
    的向日葵桌面代理在 = 有主控正在桌面控制本机 → 自动创建手机屏；连续两次查不到
    = 断开 → 自动移除（只拆自动建的，手动建的不碰；会话中手动移除不会被抢回）。
    传文件/看摄像头走别的代理，不会触发。型号与开关存 `%LOCALAPPDATA%\SnugWindows\settings.txt`
  - 「测试窗口」：铺满虚拟屏的动/静对照画面（大字帧号），专为对照 mac 版串流卡顿
  - 「启动器」（App 资源库）：对着 mac 版的行为——盖满虚拟屏，顶部搜索框，
    「运行中 / 全部」两页：运行中列正在跑的 App（图标 + 窗口标题），点一下把它的窗口
    全屏到虚拟屏；全部是开始菜单里的所有 App（图标网格，运行中的带珊瑚色圆点），
    点一下打开、等窗口出现、自动全屏过来。关掉资源库后虚拟屏左上角留一个悬浮
    「App」键（右键可收起），点它重开资源库——手机端就靠它回家
  - 全程日志：界面日志框 + `%LOCALAPPDATA%\SnugWindows\snug-windows.log`

## 怎么跑

普通用户不用装 Rust：去 GitHub Actions 的 `Windows App (Rust)` 工作流下载
`SnugSunlogin-Windows-Rust` 产物（exe + `setup-virtual-display.ps1` + `SPIKE.md`），
按 `SPIKE.md` 的步骤测。

本地构建（需要 Rust 工具链）：

```powershell
cd windows
cargo build --release --target x86_64-pc-windows-msvc
# 产物：target\x86_64-pc-windows-msvc\release\SnugWindows.exe
```

exe 带 requireAdministrator 清单（改配置、重启设备需要），双击会弹 UAC，属预期。

## 说明

- 本应用不调用向日葵的任何 API：虚拟屏对向日葵被控端就是一块普通显示器，
  手机端选屏、串流都走它自己的链路——这正是 spike 要验证的。
- 同一需求曾有 C#（WPF / WinForms）两稿，均已弃用：机主 2026-11-19 定 Windows 版用 Rust 重写。
