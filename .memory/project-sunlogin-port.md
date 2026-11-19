---
name: project-sunlogin-port
description: 向日葵移植（fork）的进度与实机结论：身份核对、全屏比例定版、旋转还原、手势点击失效排查、公司证书与 GitHub Actions 发版
metadata:
  node_type: memory
  type: project
  modified: 2026-11-19T07:41:00.000Z
---

本仓库是上游 Snug 的向日葵（Sunlogin）移植 fork，与上游自己的记忆文件（project-product、project-release 等）分开记：那些写的是原作者的环境（个人证书 29898YBGWD、gh 账号 zephyr4123、国内源），**不适用于本 fork**，别混用。

- **仓库与定位**：github.com/skyvense/snug-sunlogin（公开，默认分支 `feat/sunlogin-port`），本地 `~/workspace/snug-sunlogin`。机主 2026-11-19 说明：这是公司（Oray）要实现的产品、打算开源，签名用公司 Developer ID 是正当工作用途。移植细节与逐项状态见仓库 `PORTING.md`。
- **身份层已定版（2026-11-18 实机核对）**：推流和注入输入都是 `AweSun_Desktop`（AweSun.app 里 Helpers 下的独立进程）；被控悬浮窗是主程序 `AweSun` 的窗口、图层 1001，`overlayVisible` 认 owner=AweSun 且图层 ≥ 100；bundle id `com.oray.sunlogin.macclient`；图标换成向日葵金橙版双屏叠片，README、About、仓库描述都标了原作者出处。控制端表格**未适配**：仍按 UU 的界面结构读，向日葵读不到，未知控制端等 2 秒按手机处理（副作用：电脑连电脑也会升起手机屏，v0.1 可接受）。
- **手机屏比例定版（2026-11-18，机主实机纠正）**：向日葵手机端全屏显示、工具栏悬浮覆盖在画面上，`ScreenFit.sunloginInset` 91 → 0；比例上限 2 → 2.25（连带像素上限 4320 → 4860）；默认屏 540×1173（iPhone 17 Pro Max 完整比例），不再是扣过的 540×950。
- **竖屏旋转保存/还原（2026-11-18）**：别的屏的模式记录加了 `rotation`（`CGDisplayRotation` 读），加/撤屏 settle 时先还原模式、再还原旋转，远程结束写永久配置时也带上。两个教训：`CGConfigureDisplayRotation` 这个 API 不存在（编译才发现）；设旋转走私有框架 MonitorPanel 的 `MPDisplay.setOrientation:`，必须经 C 桥（`SnugSetDisplayRotation`）运行时查找调用——Swift 里直引类符号会链接失败。macOS 26 上 MPDisplay 是否还在、还原是否真生效，**未实机证实**。
- **测试期望重写（commit 85a5874）**：全屏比例与 AweSun_Desktop 两处定版后，旧测试 42 处期望过期（捕获日志夹具还写着候选进程路径、毫米期望还按 4320 上限、iPhone 8 铺屏基准宽变了等），已按新定版值重写。注意：改期望对齐代码不等于证明代码对，**Mac 上复跑 `scripts/snug check` 的结果还没回报，别当已全绿**。只有日志夹具改名 `AweSun_Desktop` 一项有独立依据（发现报告实测的进程名）。
- **手势点击失效排查（2026-11-18 → 19）**：现象是远程后虚拟屏点击全部失效、本地控制也受影响；关手势或退出 Snug 即恢复，定位到全机唯一的事件监听（TouchTap）。加了有上限的诊断日志（commit 2b5f291，每次监听前 150 条，**留在代码里别删**）。2026-11-19 诊断日志实锤管线正常：事件全来自 AweSun_Desktop（pid 96664），按下扣住 → 抬起补发按下+当前、单指拖动译成滚动，区域坐标也对。11-18 那次失效没有逐事件证据，日志里仅有的异常是撤屏/退出重排窗口时监听两次被系统以「回调超时」停用（另有主线程卡 2.2s、向日葵会话自身断连重连两次）——只能记为嫌疑，**未实锤**。再犯时的取证纪律：先点几下、拖一下让诊断日志记下来，别急着开关手机屏（重排会把证据冲掉），然后退出发日志。
- **HDR 与主屏空白条（未结）**：加/撤屏后物理屏 HDR 状态变了、主屏左缘出现空白条。嫌疑：模式记录（DisplayMode.ModeInfo）只有分辨率/缩放/刷新率、没有 HDR 字段，还原时可能套到同规格的非 HDR 模式。待机主提供 `~/Library/Logs/SnugSunlogin.log` 里加/撤屏前后的模式记录定案。
- **签名与发版（2026-11-19）**：机主钥匙串里有 Oray 的 Developer ID Application（团队 ZBNMDRTU32，SHA-1 以 970F4A72 开头），`scripts/snug dist` 会自动选中它（钥匙串里多于一张 Developer ID 时脚本要求用 `MAC_SIGNING_IDENTITY` 指定）。这张证书有个坑（2026-11-19 定位并解决）：**证书本体在 System 钥匙串、私钥在 login 钥匙串**（CSR 在本机生成、私钥落 login；.cer 装进了 System）。全量 `find-identity` 能跨钥匙串配对，所以本机签名一直可用；但按单个钥匙串查不到、钥匙串访问里也看不到它、导出 .p12 置灰。已用 `security find-certificate`（看输出的 `keychain:` 字段定位）从 System 导出 PEM 再 import 进 login，两者团聚，login 里凑齐 6 个身份、.p12 导出恢复正常。教训：钥匙串访问只显示单个钥匙串，定位证书存放位置用 find-certificate，别逐个钥匙串猜。还差公证档案：要用 Oray 账号的 App Store Connect API key 跑 `store-credentials`，再在 `.machine.env` 填 `MAC_NOTARY_PROFILE`（未完成，key 可能要账号管理员开）。证书概念别混：Apple Development（调试）、Apple Distribution（App Store）都不能给 dist 用，只有 Developer ID Application 能签名 + 公证做站外分发；「7 天过期」只是免费 Personal Team 的 provisioning 限制，付费 Developer ID 没有。`.cer` 只有公钥、放 GitHub 没用（私钥在钥匙串里，脚本从本机钥匙串找证书）；`.p12`、`.p8` 是真秘密，绝不进仓库。
- **GitHub Actions（2026-11-19 已合入默认分支）**：`ci.yml`（PR / 默认分支推送跑 check，无签名无 secrets）+ `release.yml`（推 `v*` 标签触发：核对标签与 VERSION → 证书 secrets 解码进临时钥匙串且搜索列表只留它 → ASC key 存公证档案 → dist → dmg + sha256 挂同名 Release；runner 用 macos-26，macos-14 已被 GitHub 退役）。分支 `feat/github-actions` 已合入 `feat/sunlogin-port`（c2dfe1e），两个 workflow 在仓库 Actions 里均为 active；`release` Environment 已建好、Required reviewers 是机主本人。五个 Environment secrets 已加齐并经 API 核对名字无误（2026-11-19，值不可见、只能机主本人录入）。待办只剩：定首发版本号（VERSION 现仍是上游的 2.5.0）、打第一个标签实跑验证（在那之前 workflow 只能算写好了，不能算验过）。
- **dist-adhoc 临时包（分支 `feat/adhoc-dmg`，本地、未推）**：没有 Developer ID 时打 ad-hoc dmg 的兜底子命令。钥匙串里已有 Developer ID 后它会拒绝运行，这条路已作废，分支可删。
- **机主环境**：Mac mini M4 / macOS 26.6.2 / 向日葵 16.8.1；主屏 DELL U2720QM 竖屏（逻辑 1440×2560，旋转 270°），副屏 G27M7Pro 横屏（2560×1440@160，HDR）；虚拟屏被系统排在两块物理屏之间（原点 -540,0）。本机（Linux）编不了 Swift，所有构建、测试、实机结论都以机主 Mac 上跑出的结果为准。
