# Snug 向日葵版 · 移植说明（PORTING）

本仓库是 [Snug](https://github.com/zephyr4123/snug)（为 UU远程 打造的 Mac 移动端工作区，MIT）的移植分支，
把远程通道从 **UU远程** 换成 **向日葵远程控制（AweSun，贝锐）**：手机用向日葵连上 Mac 后，同样多出一块
竖屏「Snug 手机屏」，窗口铺满、字号放大、手势按 iOS 习惯翻译。基线版本：上游 Snug 2.5.0（commit `9710d59`）。

> ⚠️ **当前状态：移植初版，未经实机验证。** 移植环境没有 macOS / Xcode，这份代码**尚未编译过**。
> 第一步请在 Mac 上跑 `scripts/snug check` 看构建与测试结果，再按文末清单逐项核对。
> 凡标注「待实机核对」的，都是有候选值、但必须在真机上确认后才能定版的点。

## 架构为什么可移植

Snug 不接入远程软件的任何接口，只观察它在本机留下的三路信号（覆盖窗口 / 推流日志 / 控制端表格），
其余能力——`CGVirtualDisplay` 建竖屏、辅助功能搬窗口、事件监听翻译手势、字号记忆、还原记录——
都与远程软件无关，全部原样复用。与 UU 的耦合集中在身份层一处（原作者的设计），移植面很小：

| # | 耦合点 | UU远程（原版） | 向日葵版 | 状态 |
| :- | :--- | :--- | :--- | :--- |
| 1 | 身份常量 `Sunlogin.swift` | `com.netease.uuremote` / 进程 `UURemoteServer` | `com.oray.sunlogin.macclient` / 进程 `AweSun` | bundle id 已由公开资料核对（安装收据、Homebrew cask `awesun`）；**进程名是候选值，待实机核对** |
| 2 | 推流判定 `CaptureLogParser` | 按进程名过滤系统日志里的 ScreenCaptureKit / VideoToolbox 事件 | 关键字不变，只换进程名（跟随 #1 常量） | 若向日葵不用 ScreenCaptureKit 推流，此路信号缺席，会话判定降级（见下） |
| 3 | 手势接管 `SunloginEventSource` | 只接管来源进程名为 `UURemoteServer` 的注入事件 | 跟随 #1 常量 | 待实机核对注入进程名 |
| 4 | 覆盖窗口 `SunloginApp.overlayVisible` | owner 为 `UURemoteServer` 的在屏窗口 | 跟随 #1 常量 | 向日葵被控时是否有覆盖窗口，待实机核对 |
| 5 | 控制端表格 `ControllerTable` | 用辅助功能读 UU 主窗口「本机被控中」列表，区分手机/电脑控制端 | **未适配**：解析的仍是 UU 的界面结构，向日葵界面不同，预计读不到 | 读不到时按上游既有降级：未知控制端等 2 秒按手机处理。副作用：电脑连电脑的向日葵会话也会升起手机屏（v0.1 可接受） |
| 6 | 手机屏比例扣减 `ScreenFit.sunloginInset` | 上下各扣 91 点（UU 手机端工具栏实测） | 数值暂沿用 91 | **待校准**：向日葵手机端工具栏高度不同，按真机黑边/遮挡情况改这一个数 |
| 7 | 「换比例后要在远程里重选屏幕」提示 | UU 只在选中屏幕时按当时尺寸起推 | 提示文案已改名为向日葵 | 向日葵若能跟随尺寸变化，此提示多余但无害；待实机确认 |
| 8 | App 资源库排除远程软件自身 | 按 bundle id 排除 UU | 自动跟随 #1，排除向日葵 | 已改 |
| 9 | 改名与隔离 | 类型/文件以 UU 命名；数据目录 `Application Support/Snug` | 类型与文件改名 `Sunlogin*`；数据目录 `SnugSunlogin`、日志 `SnugSunlogin.log`、App bundle id 改为 `io.github.skyvense.snugsunlogin` | 已改：可与原版 Snug 同机共存，授权与数据互不干扰 |
| 10 | 更新源 `UpdateSource` | 作者腾讯云 / 上游 GitHub | 腾讯云一项改为失效地址（`example.invalid`），GitHub 改指 `skyvense/snug-sunlogin`（占位） | 已改：移植版绝不会被上游原版的更新包覆盖 |
| 11 | 旧版伴侣冲突检测 `LegacyApp` | 检测 UU 时代的 `com.corndog.uudisplay` | 保持原样 | 在本分支永不触发，属休眠代码，后续可删 |

代码与文档里凡由改名产生的「向日葵会如何如何」的描述（尤其是注释中关于推流、重选屏幕的），
在实机核对前都应当作**从 UU 继承的假设**，不是已证实的向日葵行为。

## 在 Mac 上定版的步骤

前置：macOS 14+，装好 Xcode 与向日葵 Mac 版（AweSun），手机装向日葵 App。

1. **先构建**：`scripts/snug check`（构建 + 全部测试）。移植只动了常量与命名，预期可直接通过；
   若有编译错误，多半是改名遗漏，按报错修。
2. **抓事实**：未连接时跑 `scripts/discover-sunlogin.sh` 存基线；手机连上 Mac 开始被控后跑
   `scripts/discover-sunlogin.sh --during`。报告里看三件事：
   - 推流日志的来源进程名 → 与 `Sunlogin.serverProcess` 不一致就改这一行；
   - 在屏窗口清单里向日葵一方的窗口（覆盖窗口是否存在、owner 是谁）；
   - AweSun 包内可执行文件清单（注入输入的进程多半在其中）。
3. **看控制端表格**：被控时运行移植版的 `--dump-sunlogin`，它会打印向日葵主窗口的辅助功能界面树。
   若要适配 #5，照这棵树改 `ControllerTable` 的识别规则（UU 版规则在 git 历史里）。
4. **端到端验**：手机连入 → 向日葵的屏幕列表里应出现「Snug 手机屏」→ 切过去 →
   在 App 资源库点开一个窗口，应铺满整屏 → 单指滑动应滚动而非拖选 → 断开后窗口归位、字号复原。
   **前置未知数**：向日葵 Mac 被控端是否枚举并允许切换到虚拟显示器（多屏支持），这是整个方案成立的前提，
   第一次连接时最先确认这一点。
5. **校准比例**：画面有黑边或被工具栏遮挡时，调 `ScreenFit.sunloginInset`（#6）。

## 已知风险与限制（继承自上游，另加向日葵特有项）

- `CGVirtualDisplay` 是 macOS 私有接口，系统大版本更新后可能失效（上游已知限制）。
- 向日葵更新后，进程名、日志、界面结构都可能变，信号随之失效——与上游对 UU 的风险同构。
- 向日葵的推流若走自研捕获而非 ScreenCaptureKit，第 2 路信号（推流日志）不存在：
  会话判定退化为覆盖窗口 + 进程在场两路，断开判定会变钝，`ConnectionTracker` 的注释里写了后果与对策。
- 本分支与向日葵及其开发商贝锐无关，非官方软件。

## 许可证与致谢

沿用上游 MIT 许可证（见 `LICENSE`），版权归原作者 AoyamaShou 与 zephyr4123 所有。
原版说明见上游仓库；本仓库 `docs/` 仍为上游文档，描述的工作原理与分层同样适用于本分支，
其中 UU 特有的信号细节以本文件为准。
