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
| 1 | 身份常量 `Sunlogin.swift` | `com.netease.uuremote` / 进程 `UURemoteServer` | `com.oray.sunlogin.macclient` / 推流进程 `AweSun_Desktop` | ✅ 已实机核对（2026-11-18）：推流日志全部来自 AweSun_Desktop（`Contents/Helpers/AweSun_Desktop.app`）；覆盖窗口是主程序 AweSun 放的，单独认 `overlayWindowOwner` |
| 2 | 推流判定 `CaptureLogParser` | 按进程名过滤系统日志里的 ScreenCaptureKit / VideoToolbox 事件 | 关键字不变，只换进程名（跟随 #1 常量） | ✅ 已实机核对：向日葵确实走 ScreenCaptureKit，25 秒采样与 10 分钟回看都抓到了 AweSun_Desktop 的 `SCDisplay/SCStream` 事件 |
| 3 | 手势接管 `SunloginEventSource` | 只接管来源进程名为 `UURemoteServer` 的注入事件 | 跟随 #1 常量（AweSun_Desktop） | ✅ 已实机核对（2026-11-19）：诊断日志确认注入事件全部来自 AweSun_Desktop，点击（按下扣住→抬起补发）与单指拖动（译成滚动）均按预期工作。遗留观察：2026-11-18 曾出现一次点击全部失效，当时日志仅有的异常是撤屏/退出重排窗口时监听两次被系统以「回调超时」停用（另有主线程卡 2.2s、向日葵会话自身断连重连两次）；无逐事件记录，未能实锤，诊断日志（每次监听前 150 条）已留在代码里，再犯可取证 |
| 4 | 覆盖窗口 `SunloginApp.overlayVisible` | owner 为 `UURemoteServer` 的在屏窗口 | owner 为主程序 `AweSun` 且图层 ≥ 100 的在屏窗口（实测被控悬浮窗图层 1001） | ⚠️ 部分核对：被控时存在；未被控时是否也在屏待基线报告确认，在的话这一路会误报 |
| 5 | 控制端表格 `ControllerTable` | 用辅助功能读 UU 主窗口「本机被控中」列表，区分手机/电脑控制端 | **未适配**：解析的仍是 UU 的界面结构，向日葵界面不同，预计读不到 | 读不到时按上游既有降级：未知控制端等 2 秒按手机处理。副作用：电脑连电脑的向日葵会话也会升起手机屏（v0.1 可接受） |
| 6 | 手机屏比例扣减 `ScreenFit.sunloginInset` | 上下各扣 91 点（UU 手机端工具栏实测） | **0（不扣）** | ✅ 已定版（2026-11-18 机主实机纠正）：向日葵手机端全屏显示、工具栏是浮在画面上的覆盖层、不占高度，虚拟屏直接用手机屏幕的完整比例；连带把比例上限从 2 提到 2.25（现代手机全屏比例约 2.17–2.22），默认屏由 540×950 变为 540×1173，相关单测已按新数重算（待 Mac 上 `scripts/snug check` 跑绿确认） |
| 7 | 「换比例后要在远程里重选屏幕」提示 | UU 只在选中屏幕时按当时尺寸起推 | 提示文案已改名为向日葵 | 向日葵若能跟随尺寸变化，此提示多余但无害；待实机确认 |
| 8 | App 资源库排除远程软件自身 | 按 bundle id 排除 UU | 自动跟随 #1，排除向日葵 | 已改 |
| 9 | 改名与隔离 | 类型/文件以 UU 命名；数据目录 `Application Support/Snug` | 类型与文件改名 `Sunlogin*`；数据目录 `SnugSunlogin`、日志 `SnugSunlogin.log`、App bundle id 改为 `io.github.skyvense.snugsunlogin` | 已改：可与原版 Snug 同机共存，授权与数据互不干扰 |
| 10 | 更新源 `UpdateSource` | 作者腾讯云 / 上游 GitHub | 腾讯云一项改为失效地址（`example.invalid`），GitHub 改指 `skyvense/snug-sunlogin`（占位） | 已改：移植版绝不会被上游原版的更新包覆盖 |
| 11 | 旧版伴侣冲突检测 `LegacyApp` | 检测 UU 时代的 `com.corndog.uudisplay` | 保持原样 | 在本分支永不触发，属休眠代码，后续可删 |
| 12 | 竖屏旋转的保存与还原 `DisplayMode.rotation` / `ModeTarget.rotation` | 上游只记别的屏的模式（分辨率/缩放/刷新率），不记旋转 | 加屏、撤屏前记下每块别的屏的旋转（`CGDisplayRotation`），换完先设回模式、再设回旋转（私有框架 MonitorPanel 的 `MPDisplay.setOrientation`，帮手进程 `--restore-rotation`，下发后轮询 `CGDisplayRotation` 最多 10 秒）；远程结束写永久配置时，旋转非 0° 的屏也一起写 | **已修正两版**：初版误用了不存在的 `CGConfigureDisplayRotation`（公开 API 只有读旋转、没有写），2026-11-18 实机 `scripts/snug check` 编译报错后改走 displayplacer 同款的 MonitorPanel 私有框架；第一版又在链接期报 Undefined symbols（直接引用 MPDisplay 类符号，而该框架不在链接集合里），已改为桥接层 C 函数运行时按类名查找；MPDisplay 在 macOS 26 上是否还在、设旋转后模式是否跟着变，待实机加/撤屏确认；设不回去只记日志、不卡撤屏 |

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
5. **核对比例**：扣减已定为 0（向日葵全屏、工具栏悬浮）；画面仍有黑边时先核对所选机型与真机是否一致。
6. **验竖屏旋转**（一横一竖多屏，#12）：加手机屏前后、撤屏后，竖屏应一直保持 90°/270°。
   若撤屏后竖屏被转回横向，看日志里「加屏/撤屏前别的屏」一行记的旋转、以及设回旋转的帮手退出码；
   旋转设不回去只记日志，不会卡住撤屏。

## 已知风险与限制（继承自上游，另加向日葵特有项）

- `CGVirtualDisplay` 是 macOS 私有接口，系统大版本更新后可能失效（上游已知限制）。
- 向日葵更新后，进程名、日志、界面结构都可能变，信号随之失效——与上游对 UU 的风险同构。
- 向日葵的推流若走自研捕获而非 ScreenCaptureKit，第 2 路信号（推流日志）不存在：
  会话判定退化为覆盖窗口 + 进程在场两路，断开判定会变钝，`ConnectionTracker` 的注释里写了后果与对策。
- 本分支是作者（贝锐的一名技术人员）以个人身份维护的个人项目，非向日葵官方产品。

## 实机记录（2026-11-18，Mac mini M4 / macOS 26.6.2 / 向日葵 16.8.1）

测试机显示器：一横一竖的异形双屏——DELL U2720QM 竖置（旋转 270°，逻辑 1440×2560@60）为**主屏**，
G27M7Pro 横置（2560×1440@160，HDR）。发现报告确认：手机屏成功创建（1080×1900，540×950 点），
位置被系统排在主屏左侧（坐标 -540,0），即两块物理屏之间；推流进程为 AweSun_Desktop（见 #1/#2）。

试用反馈的四个现象与当前判断：

1. **虚拟屏插在两块物理屏之间**：新建显示器的落位由 macOS 排列算法决定，Snug 不指定位置；
   对窗口搬运无影响（走全局坐标），但观感与鼠标跨屏路径会变。是否要主动把手机屏排到最外侧，待定。
2. **物理屏 HDR 状态被修改**：显示器集合变化时系统会重套模式；Snug 的模式记录/恢复只按
   分辨率、缩放、刷新率匹配（见 `DisplayMode.swift` 的 `ModeInfo`），**模式身份里没有 HDR**，
   同规格下可能选到非 HDR 的那一档。待取 `~/Library/Logs/SnugSunlogin.log` 里的模式记录证实后修。
3. **主屏最左侧出现空白**：与 #2 同族嫌疑（主屏被套到错误缩放模式时画面偏移/留边），
   也待日志里的主屏模式记录确认。
4. **手机上画面偏大、不像 iPhone 17 Pro Max 的尺寸**：报告显示手机屏是默认机型（17 Pro Max）
   按原版 UU 的 91 点扣减算出的 540×950；机主纠正向日葵是全屏显示、工具栏悬浮覆盖，
   已把扣减改为 0、比例上限提到 2.25，默认屏变为 540×1173（17 Pro Max 的完整比例），待复测。
5. **向日葵抓虚拟屏感觉卡（2026-11-19 机主反馈，调查中）**：抓屏链路是 WindowServer 合成 →
   ScreenCaptureKit 按**帧缓冲像素**抓取 → 编码推流。Snug 侧相关参数（`PhoneDisplayService.settings`）：
   hiDPI 写死开，540 档实际帧缓冲 1080×2346 ≈ 253 万像素 @60fps，是点数的 4 倍——第一嫌疑；
   刷新率写死 60Hz，管线饱和时帧排队会加重粘滞感；档位最高到 1080 点（2160×4860 像素），
   窗口装不下自动升档时像素量再翻 4 倍。物理尺寸（按像素上限以 220ppi 折算，约 56×122cm）
   与像素上限只是名义值/天花板，基本可排除。待做的零代码 A/B：①向日葵切物理竖屏（1440×2560，
   像素量更大）滚同样内容——物理屏流畅而虚拟屏卡，则问题在虚拟屏路径而非像素量；
   ②手机端画质切流畅——变好说明推流像素/码率占大头；③卡时看活动监视器里 WindowServer 与
   AweSun_Desktop 的 CPU。代码侧实验（未做）：hiDPI 关（1×，像素降到 1/4，代价是文字锐度）
   或刷新率 30Hz 的对比版。

## GitHub Actions 发版

两个 workflow（`.github/workflows/`，runner 用 `macos-26`，和本机 macOS 26 工具链同代）：

- `ci.yml`：PR 和默认分支推送时跑 `scripts/snug check`（构建全部产物 + 全部测试）。不签名、不用 secrets，fork 的 PR 也能跑。
- `release.yml`：推 `v<版本>` 标签触发（也可在 Actions 页手动选已有标签重跑）。绑 `release` Environment：把 Developer ID 证书导入临时钥匙串 → 用 App Store Connect API key 存公证档案 → `scripts/snug dist` → 把 dmg 和 `.sha256` 挂到同名 GitHub Release。第一步先核对标签与 `VERSION` 一致，对不上直接拦下；Release 已存在时改为覆盖上传附件。

一次性设置（仓库 Settings，只做一次）：

1. **Environments** → 新建 `release` → 勾 **Required reviewers**（选自己）：每次发版要先人工批准，任务才碰得到下面的 secrets。
2. 在 `release` Environment 的 **secrets** 里加五个（不要写进仓库文件，也不要放普通变量）：
   - `DEVELOPER_ID_P12_BASE64`：钥匙串访问「我的证书」里选中 Developer ID Application 那张证书、连私钥导出 `.p12`，再 `base64 -i cert.p12 -o cert.b64`，把文本贴进来；
   - `DEVELOPER_ID_P12_PASSWORD`：导出 `.p12` 时设的密码；
   - `ASC_API_KEY_P8_BASE64`：App Store Connect API key 的 `.p8` 文件的 base64；
   - `ASC_API_KEY_ID`、`ASC_API_ISSUER_ID`：该 key 的 Key ID 与 Issuer ID（App Store Connect → 用户和访问 → 集成）。
3. 签名证书是公司（Oray）的：`.p12` 和 `.p8` 谁拿到都能冒充公司签名，只放这一个 Environment，不另存仓库、网盘或聊天记录。

发版流程：改 `VERSION` 并提交 → `git tag -a v<版本> -m "…"` 并推送标签 → 在 Actions 里批准 `release` 环境 → 跑完到 Releases 页核对 dmg 能正常下载、双击打开不被 Gatekeeper 拦。应用内更新仍走原来的更新源，GitHub 这条只管下载包。

## 检查更新修复与 2.5.1（2026-11-19）

原 fork 的「检查更新」是坏的：默认更新源指向从未配置的占位地址，且 Release 里没有应用内更新需要的清单与更新包。修复（commit 0a88829）：更新源只保留 GitHub、默认即 GitHub（存量配置里的旧值会走已有的未知源回退，自动落到 GitHub）；release 流程在打包后生成更新清单（update.json）并把更新包（.pack）一并挂到 Release。已发布的版本不重打——修复随热修复版 **2.5.1** 发出（run 44490954113，机主审批后全程绿）。服务端已验证：v2.5.1 Release 四件资产齐全（DMG、.sha256、.pack、update.json），应用实际读取的 `releases/latest/download/update.json` 可拉取，版本 2.5.1、包名与发布说明无误。存量 2.5.0 安装需在设置里把更新源切到 GitHub 一次，之后即可应用内升级到 2.5.1（默认源的修复随 2.5.1 本体生效）。

## 许可证与致谢

沿用上游 MIT 许可证（见 `LICENSE`），版权归原作者 AoyamaShou 与 zephyr4123 所有。
原版说明见上游仓库；本仓库 `docs/` 仍为上游文档，描述的工作原理与分层同样适用于本分支，
其中 UU 特有的信号细节以本文件为准。
