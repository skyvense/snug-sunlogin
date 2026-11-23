---
name: project-sunlogin-port
description: 向日葵移植（fork）的进度与实机结论：身份核对、全屏比例定版、旋转还原、手势点击失效排查、公司证书与 GitHub Actions 发版
metadata:
  node_type: memory
  type: project
  modified: 2026-11-19T08:21:00.000Z
---

本仓库是上游 Snug 的向日葵（Sunlogin）移植 fork，与上游自己的记忆文件（project-product、project-release 等）分开记：那些写的是原作者的环境（个人证书 29898YBGWD、gh 账号 zephyr4123、国内源），**不适用于本 fork**，别混用。

- **仓库与定位**：github.com/skyvense/snug-sunlogin（公开，默认分支 `main`），本地 `~/workspace/snug-sunlogin`。机主 2026-11-19 先说这是公司（Oray）要实现的产品，**同日更正：不是公司官方产品**——是他作为 Oray 的一名技术人员、以个人身份用 AI（Muse 免费版）把原项目重新打磨的个人项目，打算开源；README、PORTING 已按更正口径表述（非官方产品），以后引用一律按更正后的口径。签名用 Oray 的 Developer ID（见签名一节）。移植细节与逐项状态见仓库 `PORTING.md`。
- **身份层已定版（2026-11-18 实机核对）**：推流和注入输入都是 `AweSun_Desktop`（AweSun.app 里 Helpers 下的独立进程）；被控悬浮窗是主程序 `AweSun` 的窗口、图层 1001，`overlayVisible` 认 owner=AweSun 且图层 ≥ 100；bundle id `com.oray.sunlogin.macclient`；图标换成向日葵金橙版双屏叠片，README、About、仓库描述都标了原作者出处。控制端表格**未适配**：仍按 UU 的界面结构读，向日葵读不到，未知控制端等 2 秒按手机处理（副作用：电脑连电脑也会升起手机屏，v0.1 可接受）。
- **手机屏比例定版（2026-11-18，机主实机纠正）**：向日葵手机端全屏显示、工具栏悬浮覆盖在画面上，`ScreenFit.sunloginInset` 91 → 0；比例上限 2 → 2.25（连带像素上限 4320 → 4860）；默认屏 540×1173（iPhone 17 Pro Max 完整比例），不再是扣过的 540×950。
- **竖屏旋转保存/还原（2026-11-18）**：别的屏的模式记录加了 `rotation`（`CGDisplayRotation` 读），加/撤屏 settle 时先还原模式、再还原旋转，远程结束写永久配置时也带上。两个教训：`CGConfigureDisplayRotation` 这个 API 不存在（编译才发现）；设旋转走私有框架 MonitorPanel 的 `MPDisplay.setOrientation:`，必须经 C 桥（`SnugSetDisplayRotation`）运行时查找调用——Swift 里直引类符号会链接失败。macOS 26 上 MPDisplay 是否还在、还原是否真生效，**未实机证实**。
- **测试期望重写（commit 85a5874，已闭环）**：全屏比例与 AweSun_Desktop 两处定版后，旧测试 42 处期望过期（捕获日志夹具还写着候选进程路径、毫米期望还按 4320 上限、iPhone 8 铺屏基准宽变了等），已按新定版值重写。2026-11-19 主干 CI（macos-26 runner）跑 `scripts/snug check` 全绿（run 44482961476，341 条测试全过），不必再等机主本机复跑。口径要分清：CI 证明的是代码与期望一致；定版值本身是机主实机纠正的，CI 不独立证明值对。日志夹具改名 `AweSun_Desktop` 一项另有独立依据（发现报告实测的进程名）。
- **手势根因补充（2026-11-22 晚，机主提供向日葵产品知识）**：向日葵有防输入冲突功能且**关不掉**——被控端正在动作时，主控输入全部丢弃。这给手势失效补上了设计层解释：手势翻译靠在本机补发事件（惯性滚动还要 60fps 连发），在向日葵眼里就是「被控正在动作」，防冲突一触发，手机后续输入（包括同一次手势的抬起、惯性中用来停住的点击）被向日葵自己丢掉，扣住的按下等不到抬起、翻译链断裂；关手势后不再有本机注入，输入立即恢复——与 11-18 症状严丝合缝，也解释了 UU 下无此问题、11-19 轻量实测（短手势在判定窗口内完成）一切正常。修复前景随之改变：监听自身的加固（超时自愈、停用先清状态）治不了这一层；重新打开手势的前置 spike 是先摸清向日葵按什么判定「被控在动作」（事件来源标记 / 它自己的注入账本 / HID 层）——若补发事件能对齐它的注入通道标记才有戏，否则手势在向日葵下是设计层死结。
- **手势点击失效排查（2026-11-18 → 19）**：现象是远程后虚拟屏点击全部失效、本地控制也受影响；关手势或退出 Snug 即恢复，定位到全机唯一的事件监听（TouchTap）。加了有上限的诊断日志（commit 2b5f291，每次监听前 150 条，**留在代码里别删**）。2026-11-19 诊断日志实锤管线正常：事件全来自 AweSun_Desktop（pid 96664），按下扣住 → 抬起补发按下+当前、单指拖动译成滚动，区域坐标也对。11-18 那次失效没有逐事件证据，日志里仅有的异常是撤屏/退出重排窗口时监听两次被系统以「回调超时」停用（另有主线程卡 2.2s、向日葵会话自身断连重连两次）——只能记为嫌疑，**未实锤**。再犯时的取证纪律：先点几下、拖一下让诊断日志记下来，别急着开关手机屏（重排会把证据冲掉），然后退出发日志。
- **HDR 与主屏空白条（未结）**：加/撤屏后物理屏 HDR 状态变了、主屏左缘出现空白条。嫌疑：模式记录（DisplayMode.ModeInfo）只有分辨率/缩放/刷新率、没有 HDR 字段，还原时可能套到同规格的非 HDR 模式。待机主提供 `~/Library/Logs/SnugSunlogin.log` 里加/撤屏前后的模式记录定案。
- **签名与发版（2026-11-19）**：机主钥匙串里有 Oray 的 Developer ID Application（团队 ZBNMDRTU32，SHA-1 以 970F4A72 开头），`scripts/snug dist` 会自动选中它（钥匙串里多于一张 Developer ID 时脚本要求用 `MAC_SIGNING_IDENTITY` 指定）。这张证书有个坑（2026-11-19 定位并解决）：**证书本体在 System 钥匙串、私钥在 login 钥匙串**（CSR 在本机生成、私钥落 login；.cer 装进了 System）。全量 `find-identity` 能跨钥匙串配对，所以本机签名一直可用；但按单个钥匙串查不到、钥匙串访问里也看不到它、导出 .p12 置灰。已用 `security find-certificate`（看输出的 `keychain:` 字段定位）从 System 导出 PEM 再 import 进 login，两者团聚，login 里凑齐 6 个身份、.p12 导出恢复正常。教训：钥匙串访问只显示单个钥匙串，定位证书存放位置用 find-certificate，别逐个钥匙串猜。还差公证档案：要用 Oray 账号的 App Store Connect API key 跑 `store-credentials`，再在 `.machine.env` 填 `MAC_NOTARY_PROFILE`（未完成，key 可能要账号管理员开）。证书概念别混：Apple Development（调试）、Apple Distribution（App Store）都不能给 dist 用，只有 Developer ID Application 能签名 + 公证做站外分发；「7 天过期」只是免费 Personal Team 的 provisioning 限制，付费 Developer ID 没有。`.cer` 只有公钥、放 GitHub 没用（私钥在钥匙串里，脚本从本机钥匙串找证书）；`.p12`、`.p8` 是真秘密，绝不进仓库。
- **GitHub Actions（2026-11-19 已合入默认分支）**：`ci.yml`（PR / 主干推送跑 check，无签名无 secrets）+ `release.yml`（推 `v*` 标签触发：核对标签与 VERSION → 证书 secrets 解码进临时钥匙串且搜索列表只留它 → ASC key 存公证档案 → dist → dmg + sha256 挂同名 Release；runner 用 macos-26，macos-14 已被 GitHub 退役）。两个 workflow 在仓库 Actions 里均为 active；`release` Environment 已建好、Required reviewers 是机主本人；五个 Environment secrets 已加齐并经 API 核对名字无误（2026-11-19，值不可见、只能机主本人录入）。**首跑已成功（2026-11-19）**：沿用版本号 2.5.0（机主拍板；本地克隆自带的同名上游标签只存在于本地、远端没有，删本地标签后重新打在 fork 代码上），run 44481640843 经机主审批后全程绿，DMG + `.sha256` 已挂到 v2.5.0 Release。**DMG 命名（机主 2026-11-19 定）：`Snug-Sunlogin-<版本>.dmg`**，与上游的 `Snug-<版本>.dmg` 区分；scripts/snug、release.yml、README 已同步改名，v2.5.0 的 Release 资产也已改名（dmg 内容不变、sha256 按新文件名重签）；`.pack` 与 App 本体名（Snug.app）不改。之后机主指示把 `feat/sunlogin-port` 合入 `main`（合并提交 8b9d0dc），CI 的 push 触发已补上 main；仓库默认分支已切到 main（2026-11-19，机主确认）。README 同日重写：个人项目口径（Oray 技术人员以个人身份用 AI 打磨、非官方产品）、使用 Muse（免费版）移植、已在 Mac 实测可用、路线图含对接向日葵开放 API（对接后不再需要手动选择手机屏）。
- **dist-adhoc 临时包（分支 `feat/adhoc-dmg`，本地、未推）**：没有 Developer ID 时打 ad-hoc dmg 的兜底子命令。钥匙串里已有 Developer ID 后它会拒绝运行，这条路已作废，分支可删。
- **机主环境**：Mac mini M4 / macOS 26.6.2 / 向日葵 16.8.1；主屏 DELL U2720QM 竖屏（逻辑 1440×2560，旋转 270°），副屏 G27M7Pro 横屏（2560×1440@160，HDR）；虚拟屏被系统排在两块物理屏之间（原点 -540,0）。本机（Linux）编不了 Swift，所有构建、测试、实机结论都以机主 Mac 上跑出的结果为准。
- **检查更新修复与 2.5.1（2026-11-19）**：原 fork 检查更新必失败——默认更新源 `.tencent` 指向占位地址 `example.invalid`（fork 没有 COS），且 Release 里没有 update.json 与 .pack，GitHub 源也是空的。修复 commit 0a88829：UpdateSource 只留 GitHub 且设为默认（存量配置的旧值经未知源回退自动落到 GitHub），release.yml 在 dist 后跑 `snug-release feed` 并把 .pack + update.json 一并挂 Release。本想重跑 2.5.0 补资产，机主指出应升 2.5.1——已发布版本不重打覆盖（仓库 ship 本来就拒绝改已发版本）、且 App 本体的默认源修复必须装进新版本才生效；重跑已取消（run 44488812453 cancelled）。**2.5.1 已发布**：标签说明即应用内更新日志（feed 从标签取），run 44490954113 经机主审批全程绿；已验证 Release 四件资产齐全、`releases/latest/download/update.json` 拉取正常（version 2.5.1、package.file = Snug-2.5.1.pack、releases[0] 说明无误）。存量 2.5.0 需手动把更新源切到 GitHub 一次即可应用内升 2.5.1，实机自更新待机主验证。
- **串流卡顿排查（2026-11-19 晚，机主实测连环排除）**：现象定性为纯串流问题（无操作也慢一拍、偶尔冻屏，输入翻译层排除）。机主实测：Windows 主控看虚拟屏流畅（Mac 抓屏链无罪）；手机远控其他 4K 物理屏流畅（手机解码/网络/像素量排除，4K 8.3MP ≫ 虚拟屏 2.53MP）。像素对齐假说（1080×2346 两轴不整除 16、物理屏全对齐）已用实验分支 `feat/aligned-pixels`（基于 v2.5.1 单变量，544×1176 点 = 1088×2352 像素）实测**证伪**——一样卡，分支留作记录、勿合并。未结：手机会话 × 虚拟屏的组合差异（向日葵会话内部的帧率/编码协商不可见）；待机主数据：虚拟屏放全屏视频时手机端是否流畅（分「低变化内容被编码器跳帧」与「恒定流赤字」）、手机会话统计（fps/分辨率/延迟/P2P 或转发）、卡时 AweSun_Desktop 与 WindowServer 的 CPU。Snug 侧未试的最后一个旋钮：main 上的 30Hz 实验（53ed7a5，未发布）。
- **提交身份（2026-11-19）**：本机 git 曾用占位邮箱 `Zhang <zhang@local>`，GitHub 按邮箱认人，机主的 35 个提交没算进 Contributors（榜上只有上游四人）。机主拍板：**不改写历史**，往后算数。仓库本地 git 邮箱已改为机主的 GitHub noreply `2735848+skyvense@users.noreply.github.com`（账号 ID 已核对），自本条起的新提交都会记到 skyvense 名下。
- **Windows 版 spike 与后台进程撞车（2026-11-19 晚）**：机主指令「手势翻译不要，建 windows 分支、写 app 测试」后，本机后台的 proactivity 研究进程把主会话的指令当成自己的工单，在 `feat/windows` 分支自行写了一套 WPF 实现并提交（未推送）——经运行记录逐项取证确认，无第二人、无外部访问。机主定规矩：**后台进程只调研、不实现**，已记入主会话的主动偏好。随后机主拍板 Windows 版**用 Rust 重写**：`feat/windows-app` 分支 `windows/` 下 Rust + Win32 实现（枚举显示器、VDD 配置字符串手术 + pnputil 重启设备、1080×2346 设模式、动/静对照测试窗口），`cargo check`（windows-msvc 目标）零警告通过，vdd 配置逻辑 5 个单测在本机以独立 harness 验证全过；CI（windows-app.yml）在 windows-latest 构建并产出可下载 exe。C# 两稿（feat/windows 的 WPF 稿、场外 WinForms 稿）弃用，feat/windows 分支留置不推。待机主在 Windows 机器上按 windows/SPIKE.md 实测四个未知数。
- **Windows 版补上 App 资源库（2026-11-19 晚，机主追加）**：机主问 mac 版那个启动器在 Windows 上有没有，答没有后当场补齐——照 mac 版 PhoneHomeView 的行为：盖满虚拟屏、顶部搜索、「运行中 / 全部」两页（运行中列窗口、点一下全屏过去；全部是开始菜单 .lnk 网格、运行中带圆点、点一下打开并等窗口出现后自动全屏），关掉后虚拟屏左上角留悬浮「App」键重开。实现：`windows/src/apps.rs`（EnumWindows 枚举、开始菜单扫描、SHGetFileInfo 图标、SetWindowPos+最大化安置）+ `launcher.rs`（自绘网格/列表、图标缓存、悬浮键）；cargo check 零警告；SPIKE 加第五个未知数 U5 待机主实测。同晚机主下令删掉两份 C# 草稿：`feat/windows` 分支已删，本机场外 WinForms 稿移入回收站（可恢复）。
- **Windows 虚拟屏搞通脚本（2026-11-20）**：机主实测确认 VDD 的 winget 包已装但设备节点没建出来——`Get-PnpDevice -Class Display` 只有向日葵自带的 `OrayIddDriver Device`（ROOT\DISPLAY\0000，OK）和 Intel Arc，没有 Virtual Display Driver；即「包进了驱动仓库、Root\MttVDD 节点缺失」。应机主要求（不管装没装过其他 Idd 都要能搞通）新增 `windows/scripts/setup-virtual-display.ps1`（commit 9fd02a1，CI run 44665843305 绿、已进产物）：自动提权后按层处理——查 VC++ 运行时与内存完整性（只报不改）→ 盘点全部显示适配器（Oray 等别人的 Idd 只看不动，设备一律按硬件 ID 认、不看本地化名字）→ 驱动包不在仓库就从官方 GitHub 发布下 Driver zip 并 pnputil 装入 → 节点缺失先重试 pnputil、再用内嵌 C# 走 SetupAPI 程序化创建（SetupDiCreateDeviceInfo + DIF_REGISTERDEVICE + UpdateDriverForPlugAndPlayDevices，devcon install 等价做法）→ 配置手术与 app 的 vdd.rs 同口径（count≥1、1080×2346，原文件备份 .setup-script.bak）→ pnputil 重启设备（不行则禁用→启用）并轮询 EnumDisplayDevices 验证虚拟屏真出现在桌面、顺带设 1080×2346@60；全程日志 `setup-virtual-display.log`，兜底指引是 VDD Control 的 Install。本机验证：pwsh 解析零语法错误、内嵌 C# 编译通过、文件 UTF-8 BOM（PS 5.1 防 GBK 乱码的老坑）；**真机未验证**——待机主跑完回日志，重点看程序化建节点这步在他的机器上是否一次成功。install-vdd.ps1 保留（只管装包，不管节点）。
- **虚拟屏脚本真机首跑与第二轮（2026-11-20 中午）**：机主在 NATETHINKBOOK（Win11 家庭版 Build 26300、内存完整性开启）跑第一版：前三段全通——脚本自动下载官方 25.7.23 的 Driver Only 包、pnputil 装入仓库成功（oem274.inf，winget 那次根本没装进去）；卡点在第四段绑驱动：节点注册步骤无错，但 UpdateDriverForPlugAndPlayDevices 报 **-536870389 = 0xE000020B = ERROR_NO_SUCH_DEVINST**（已查 setupapi.h 与多方资料核实：该 API 按硬件 ID 全局找设备，没找到刚建的节点）。另第一版 Find-VddDevice 逐设备取属性，他机器上设备条目极多（光显示器残留 30+），一次查找近 20 秒、十次轮询无输出，观感像卡死（机主在轮询中途发来日志）。INF 原文已下载核对：硬件 ID `Root\MttVDD` 与 `MttVDD` 双条目、UMDF/IddCx、类 Display，脚本用的 ID 没错。第二轮已推（commit 2620389）：查找改实例 ID 字符串直查；第四段先报节点现状，没绑驱动走补绑阶梯（pnputil /install → /scan-devices → 新增 C# DiInstallDevice 按实例直绑，仓库枚举匹配 Description/Provider）；等待循环全打进度；显示器盘点只列在用的。本机复验：pwsh 解析 OK、内嵌 C# 编译 OK、BOM 在。**待机主重跑回日志**：重点看节点在第二轮开局是否已被找到（验证首跑节点其实建出来了）、补绑阶梯哪一步生效。
- **虚拟屏脚本第二轮真机结果（2026-11-20 12:01）**：v2 开局盘点即见 **两个 Virtual Display Driver（ROOT\MTTVDD\0000 与 0001）均状态 OK、问题码 0**——驱动已绑上（首跑注册节点后 PnP 后台异步装完，UpdateDriver 的 0xE000020B 是虚惊）；多出的 0001 疑为首版脚本被重跑多建的节点。配置确认无误（1080×2346 已在、备份已做）。**新卡点在最后一层：设备 OK 但不点亮显示器**——桌面枚举里 Oray 占着 DISPLAY15–24、VDD 一个显示输出都没有；脚本对 0000 重启、禁用→启用各一轮，60 秒仍无虚拟屏。脚本自身一处误判已定位：DEVPKEY_Device_DriverInfPath 对该 UMDF 设备返回空，第四段据此误启补绑阶梯（A/B 无效、C 的 DiInstallDevice 未枚举到匹配驱动），无害但说明触发条件应以「状态 OK+问题码 0 = 已绑」为准。机主已自行重启电脑验证（Idd 首次安装常需重启挂 IndirectKmd 管线）。若重启后仍无屏：下一步摘掉多余的 0001 节点只留一个、并查驱动事件日志。
- **虚拟屏脚本第三轮（2026-11-20 13:35）**：重启后复跑（12:07 那轮）确认：0001 冗余节点重启后消失、只剩 0000，设备 OK/问题码 0，但重启与禁用→启用仍无虚拟屏——「健康但不点亮」，重启因素排除。**关键发现**：脚本与 app（vdd.rs）内置模板是 `<logging>/<colour>/<cursor>/<edid>` 分节结构，而 25.7.23 驱动包自带默认配置是 `<options>` 单节——两套 schema；机上现行配置全文从未见过（历次日志只验了 count 与 1080×2346 在不在）。v3（已推）：模板换随包原文；第五段做 A/B（先把现行配置全文打进日志、备份 .pre-v3.bak、换随包结构+1080×2346+logging=true、重启验证）；第六段失败转储证据（VDD 目录与驱动日志、WUDFRd/WUDFHost、CodeIntegrity 与 DriverFrameworks 事件）；修第四段误判（OK+0 即已绑）。若 A/B 出屏，app 的 vdd.rs 模板要同步改随包结构——那是产品侧的真修复。若仍不出屏，看证据再走：内存完整性拦 DLL（CodeIntegrity 事件）或 Build 26300（Canary）IddCx 兼容。
- **参考实现调研与脚本第四轮（2026-11-20 13:42）**：机主指路 `rizetngan/snug-for-windows`（成熟实现 v0.2.47，UU 向，本地克隆在 ~/workspace/ref/snug-for-windows）。读其 `Install-SnugDriver.ps1` 与 `VddDriverClient.cs` 后三点定案：① **配置 schema 之争终结**——他们用的正是我们模板的分节结构且产品可用，驱动包 zip 里的 `<options>` 配置是过时打包，v3 的 A/B 前提作废（幸未跑）；② **驱动真活着的信号是命名管道 `MTTVirtualDisplayPipe`**（PING 用 UTF-16LE 写、期望 ASCII PONG；25.7.23 已知会连通但不回 PONG，此时以管道连通+显示器在场为准），其就绪判定还要求 DriverInfPath 非空——机上该值为空与「安装未真正完成」互证；③ 建节点用 **nefcon**（Nefarius）：三件套先放 `C:\VirtualDisplayDriver`，`nefcon install <inf> Root\MttVDD --no-duplicates`，节点按会话由 SYSTEM 计划任务创建/移除（我们的脚本场景保留常驻即可）。他们还带自定义 EDID（user_edid.bin，CustomEdid=true，监视器标识 SNG00F0）供自家枚举认屏，我们暂不需要。v4 已推：管道活性探针 + nefcon 重建（官方 v1.21.1 包，SHA256 28292F…与参考实现钉住值一致，已本地验过）+ 模板换回分节结构 + options 老结构自动换回。待机主跑 v4 回日志：先看开局管道通不通、nefcon 重建后管道与 InfPath 是否转正。
- **Windows 虚拟屏搞通（2026-11-20 13:48，机主实测日志）**：v4 一跑到底——开局管道不通、InfPath 为空（僵尸设备实锤）；nefcon remove 没认出 SetupAPI 建的节点（报 No devices found，退出码 1，节点没摘掉），但 **`nefcon install` 成功把安装真正做完**（退出码 3010 需重启），**管道随即连通**（25.7.23 的无响应行为）、设备 InfPath 仍空但驱动已活；第五段把机上现行配置（原来是驱动包的 `<options>` 结构 + app 手术加的 1080×2346，底子不是我们的模板）换成参考实现的分节结构；第六段重启设备后 **2 秒内虚拟屏出现：\\.\DISPLAY15，SetMode 1080×2346@60 返回 0 并复核生效**。根因定案：**设备安装从未真正完成**（winget 只放了文件、SetupAPI 路径绑驱动失败、PnP 异步装了个半成品），nefcon 的 install 把它收尾。次要事实：机上配置一直是 `<options>` 结构，换分节结构后出屏——两者谁决定性以管道先通为准，主因记安装完成，配置结构存疑待观察（重启后若复发先查配置）。**遗留**：3010 的重启待机主方便时做；重启后虚拟屏应常驻（根设备持久化），复发再跑脚本即可。**产品侧待办**：Windows app 的驱动安装（现 install-vdd.ps1 走 winget）应改为脚本验证过的路径（官方包 + nefcon），等机主拍板再动 app。下一步回到 SPIKE 的 U1–U5：SnugWindows 刷新显示器 → 创建手机屏 → 向日葵手机端选屏串流。
- **SnugWindows 代码复核（2026-11-20 下午，机主要求"一并确认"，对照搞通结论逐文件核）**：vdd.rs 模板本就是分节结构 ✓、字符串手术对两种结构都安全 ✓；display.rs 枚举/设模式无问题 ✓。修掉四处（commit e458b18 + ddf7c4f）：① devices.rs 找 VDD 原来只认友好名/IddSample，补实例 ID 的 MttVDD 匹配；② 枚举加 -PresentOnly，免得挑中已消失的幽灵设备（他机器上真出现过 0001 幽灵节点）；③ 创建手机屏原来固定等 1.8 秒枚举一次，慢机器会误报失败，改轮询等虚拟屏出现（上限 15 秒）；④ 「安装 VDD」原来走 install-vdd.ps1 的 winget 路径——正是真机证明装不完驱动、且其成功判定只看设备名会在僵尸状态误报"已装好"的那条路——改为打开 setup-virtual-display.ps1，install-vdd.ps1 已删除，workflow/README/SPIKE 同步。未做（留待产品化时）：app 内直接查 MTTVirtualDisplayPipe 活性（现阶段靠搞通脚本兜底）。
- **主窗口 UI 打磨（2026-11-20 下午，机主"你先把 UI 搞搞好"，commits a1b33eb 起，CI run 44688142177 绿）**：① app.manifest 补 Common-Controls 6.0 依赖——此前未声明，按钮/列表全是经典（Win2000 风）外观，这是最根本的一处；② 字体从 DEFAULT_GUI_FONT 换成微软雅黑 UI（状态行 12 磅半粗、其余 10 磅），日志框 Consolas 等宽；③ 状态行按级别配色（就绪绿 #1F7A3D、待处理橙 #9C6A00，WM_CTLCOLORSTATIC 实现）；④ 显示器列表从 ListBox 整行字符串改为 ListView 报表分列（显示器/适配器/分辨率/刷新率/位置/备注，整行选中+双缓冲）；⑤ 布局函数统一摆位：窗口可缩放（WM_SIZE 重排、最小 600×540@96dpi）、坐标按窗口 DPI 缩放、初始窗口按桌面 DPI 定尺寸、WM_DPICHANGED 更新系数（字体不重建，单屏机无感）；⑥ 日志新内容自动滚到底。编译踩了三轮 windows 0.59 类型坑（CreateFontW 的 weight 是裸 i32、charset 等是类型化常量、pitch/family 是 u32——最后下 crate 源码查实）。**未真机看过**：视觉效果待机主下次打开新产物时验收；启动器（手机侧全屏 UI）本轮没动。
- **图标 + 排版对齐 Mac 版（2026-11-20 下午，机主指令，commits fbf5db0/88ae3e8，CI run 44689148793 绿）**：图标——从 Assets/AppIcon/AppIcon-1024.png 生成 9 尺寸 app.ico（windows/assets/），build.rs 改用 winresource（构建依赖）把图标（资源 ID 1）与 app.manifest 一起嵌（取代手动 /MANIFESTINPUT，那条路嵌不了图标）；运行时类图标 + WM_SETICON 补小图标，状态区左侧 SS_ICON 显示同款图标。**产物实证**：下载 CI 产物验过——exe 内含 requireAdministrator/PerMonitorV2/Common-Controls 清单文本与 256px 图标 PNG 数据，exe 608 KB。排版——对照 Mac 设置页（SettingsWindow.swift/StatusText.swift）：状态区改 Mac 式两行（粗标题 + 灰色短说明，标题按状态着色），功能改分组框三组：「手机屏」整宽在上（创建是大按钮、组内一行短说明）、「测试」「驱动」并排在下，文案学 Mac 的短说明风格。Mac 独有的手势/截图/字号/开机启动按 Windows 既定范围不引入。Cargo.lock 本机无 cargo 未更新（CI 构建自动解析，不用 --locked，无碍）。待机主真机验收观感。
- **Windows 版「移除手机屏」删不掉（2026-11-20 15:40 机主截图报障）**：日志显示移除只删配置+重启设备，配置里还没找到手机屏分辨率、状态文件也无 original_count，却照样报「移除完成」。结构性结论：MttVDD 只要设备启用就一直挂屏（count 恢复 1 也还有一块），光改配置永远删不掉——参考实现按会话建/删设备节点也是这个道理。修复（commit 20737a4，CI 44697168390 绿）：移除改为禁用 VDD 设备并以枚举结果验证消失（还在就明说不报完成），创建时先 pnputil 启用回来；移除时先打印「配置现状：count+分辨率一览」（与手术同一解析口径）取证——配置里为何没有手机屏条目仍未定论（zip 原件/脚本模板/App 模板三种写法口径一致，排除格式不符），等机主复测日志。另：截图确认新 UI（图标/绿色状态行/三分组/ListView）在真机渲染与设计一致。
- **v2.5.2（2026-11-20，机主指令）**：触控手势在向日葵下有异常，本版起强制关闭——AppConfig 默认改 false、解码不读存量值（存量开着的自动关）、设置页开关禁用并注明「向日葵下暂不可用」；引擎本来就只读 config.gestures，一处收口全链路生效。测试同步：默认值断言改手势关、新增「存量 true 解码为 false」一条。main CI 全绿后打注记标签 v2.5.2（标签说明即更新日志），Release run 44693512629 经机主批准后成功，四件资产（DMG、sha256、pack、update.json）齐全已验。
- **同步规矩（2026-11-20，机主指令）**：main 往 feat/windows-app 单向同步成常态——main 上有新提交就随手合入 Windows 分支（本次为首次：合入 2.5.2 手势强制关闭等 3 个提交，唯一冲突就是本文件两边都加记录，两边都保留），防止两边越岔越远将来合不动。同步由主会话执行，不走后台进程（后台只调研的老规矩）。

### 2026-11-20 16:2x Windows 自动建屏 + 型号预选（机主拍板方案，主会话实现）
- 机主纠正感知路线：不用 history.log，盯进程最靠谱——Windows 上桌面主控连入时向日葵拉起带 `--mod=desktopagent` 的桌面代理进程，它在不在即会话在不在（传文件/摄像头走别的 mod，不误触发；此点待 U6 实测确认）。
- 实现（feat/windows-app，提交 7bbf14f + 3472b0f + PostMessageW 修复）：phones.rs（机型库 include_str! 仓库根 Assets/phone-models.json——单一数据源；scaled_resolution 短边上限 1080、偶数高，17 Pro Max 恰为 1080×2346；current_spec 全局）+ vdd/display/创建流程改读 current_spec，删 PHONE_* 常量；settings.rs（%LOCALAPPDATA%\SnugWindows\settings.txt：phone_model/auto_create）；session.rs（后台线程 3 秒轮询 Get-CimInstance 查命令行标记，消失连续 2 次才发 END，PostMessageW 边沿通知 UI 线程）；主窗口：手机屏组加高到 148（型号下拉 CBS_DROPDOWNLIST + 「主控连入时自动建屏」BS_AUTOCHECKBOX 默认开），换型号时屏在用先按旧规格拆净再重建；自动只拆自己建的屏（screen_auto_owned），手动建/拆优先；状态行加「等待主控连入」（自动开且无主控且无屏时）。CI 44700377692 绿，产物字节验：desktopagent、机型名、settings.txt、中文 UI 串都在 exe 里（742,912 B）。
- 待机主实测（SPIKE 新增 U6）：自动建/拆、传文件不触发、手动优先、换型号尺寸跟随；与先前待复测的移除修复（20737a4）同批验。

### 2026-11-20 16:2x 悬浮 App 键改图标异形窗（机主在手机上实测时提的要求）
- 旧悬浮键是 76×76 白色圆角方块 + 珊瑚描边 + 「App」文字。机主要求：大一点、做成图标样式的异形窗、点击要有效果。
- 实现（launcher.rs）：边长 104；SetWindowRgn 把窗口轮廓设成图标双屏叠片（含右下阴影偏移）的并集，轮廓外不绘制也不接点击；绘制改为 GDI 手绘——底铺深金阴影色、两块屏各自竖向金橙渐变（后屏 #FFE066→#FFC21A、前屏 #FFB03A→#FF8A00）+ 深色描边分层，去掉文字；按压效果：LBUTTONDOWN 缩到 92% 且亮度 80%（SetCapture 跟踪），LBUTTONUP 且落点在轮廓内（PtInRegion）才打开资源库，滑出松手取消，WM_CAPTURECHANGED 兜底清按压态；右键收起不变。CI 44701653042 绿。待机主真机看效果（形状边缘锯齿、渐变观感、按压手感）。

### 2026-11-20 16:5x 连入时资源库自动打开（机主真机测出缺失）
- 机主实测反馈：主控连上时启动器没有自动打开到虚拟屏。已补：WM_SESSION_START 在自动建屏就绪后调 open_launcher_on_virtual()（按枚举找虚拟屏 rect，launcher 已开则先 close 再 open 确保落位；无虚拟屏则不开并记日志）；WM_SESSION_END 自动拆屏前先 launcher::close_all() 关资源库与悬浮键；on_auto_toggled 中途开开关且主控在连时同样补开。勾选文案改为「主控连入时自动建屏并打开资源库（断开自动移除）」。CI 44705142425 绿。待机主复测：连入后手机应直接看到 App 资源库首页。

### 2026-11-20 16:5x 自动/手动改为单选（机主要求）
- 原「主控连入时自动建屏」勾选框换成两个 BS_AUTORADIOBUTTON 单选：自动（连入建屏并打开资源库，断开删屏）/ 手动（只认创建/移除按钮），并排放在型号行下方；apply_auto_setting 统一处理切换副作用（切到自动且主控在连则补建屏+资源库）；settings 格式不变。CI 44705522455 绿。

### 2026-11-20 17:0x 「点窗必达虚拟屏」核查与加固（机主重申规格）
- 规格（机主原话）：只要有虚拟屏，启动器里点的任何窗口都打开在虚拟屏，已开的应用点击也自动移过去。核查发现两处弱点：①fullscreen_on 全程忽略返回值、日志无条件报「已全屏」，移动失败无声；②Launch 等新窗口时抢第一个新窗口，常抢到闪屏/加载窗，真窗口留在物理屏。
- 加固（apps.rs/launcher.rs）：fullscreen_on 改为落位验证——还原→SetWindowPos→最大化→等 150ms→GetWindowRect 判中心点在目标屏内，没落位重试一次，返回 bool；三条路径（运行中 Focus、全部里已运行、新打开）按结果如实记日志（没到位会写明）。Launch 等待改稳定落锤：名字/标题对上的立即认；对不上的取面积最大的新窗口、稳定 1.5 秒才认，到点有候选兜底。CI 44706525206 绿。已知边界：全部页「已运行」判定仍靠名字/标题启发式（.lnk 名与 exe 名对不上的冷门应用可能重复打开），未做 .lnk 目标解析（COM），待实测撞上再说。

### 2026-11-20 17:1x 虚拟屏缩放 + 启动器图标/多窗口（机主要求）
- 缩放：新模块 scale.rs 走 CCD 未公开 GET/SET_SOURCE_DPI_SCALE（type -3/-4，Firefox/Wine/SetDPI 同款）；阶梯 12 档，相对步数以推荐档为 0 点；GET 后三 i32 字段序两说，用 min<=cur<=max 不变式判定（b==c 时两读法等价，不变式可精确区分）；当前%由该屏 DC 的 LOGPIXELSX 换算，设置后读回验证。建屏就绪后 apply_phone_scale 抬到至少 250%（只升不降），原始步数记 vdd-state 的 scale_rel_before（只记第一次），移除时在禁用设备前先 set_rel 还原。CI 44707938910 绿。未验证：真机上 VDD 屏的推荐档与实际生效幅度，等机主看日志「缩放 X% → 250%」。
- 启动器：图标放大（行 48→56/行高 88，磁贴 64→88/高 188）；「运行中」改按 App 分组（键=exe 路径退回程序名）：多窗口行尾「N 窗 ▾」展开钮，展开子行缩进小图标逐窗可选（点=全屏该窗）、子行行尾 ✕ 关该窗（WM_CLOSE 后 pump 350ms 重扫列表对账）；单窗口分组行行尾直接 ✕；多窗口分组行无 ✕（关窗在子行）。绘制与命中共用 close_btn_rect/expand_chip_rect。CI 44708156219、44708267916 绿。SPIKE 新增 U7（缩放）、U5 补多窗口/关窗项。

### 2026-11-20 17:2x 合入 main + v2.6.0 双平台发版（机主指令）
- 合并检查：main 零独有提交、windows 分支领先 53 提交、merge-tree 无冲突 → 快进合并已完成并推送，main CI（44709202550，mac 构建与测试）绿。合并前发现分组提交漏了命中接线两处编辑（滞留工作区），已补提交 015ff30 并验 CI 绿才合并。
- release.yml 新增 windows-dist 任务：needs dist（等 mac 建好 Release）、不绑 Environment（无签名 secrets、免二次批准），按 windows-app.yml 同款配方构建，zip 内含 SnugWindows.exe + setup-virtual-display.ps1 + windows/README.md，附 .sha256，gh release upload --clobber 挂同一 Release。VERSION 升 2.6.0（a7b762f），注释标签 v2.6.0 已推，Release run 44709754420 处于 waiting——等机主在 Actions 里批准 release Environment；批准后 mac DMG 四件套 + Windows zip 应同挂 v2.6.0 Release，待验。
- 结果（2026-11-20 17:3x）：机主批准后 Release run 44709754420 全绿（mac dist 4m4s、windows-dist 1m16s）；v2.6.0 Release 资产六件齐全已验：Snug-Sunlogin-2.6.0.dmg + .sha256、Snug-2.6.0.pack、update.json、Snug-Sunlogin-Windows-2.6.0.zip（381,778 B）+ .sha256。根 README 已同步改为双平台（简介/实测/下载安装/系统要求/工作原理/构建，提交于 main）。

### 2026-11-20 18:1x 自动拆屏失效根因：会话查询自匹配（已修）
- 机主实测：自动模式断开后不拆屏，日志无「主控已断开」；且机主自查进程列表确认 desktopagent 断开后确实退出。根因在查询本身：session.rs 的脚本经 -Command 传给 powershell.exe，标记串 --mod=desktopagent 就在查询进程自己的命令行里，Get-CimInstance 全量枚举每次都查到它自己 → agent_running 恒为真 → 会话被永远判为连着，END 永不触发（连入侧 START 也因此在程序启动数秒后虚假触发）。修复：Where-Object 加 $_.ProcessId -ne $PID 排除查询进程自己；边沿事件补写文件日志。CI 双绿（44716614620/44716634876）。待机主真机复测：不连手机启动应显示「等待主控连入」且不自动建屏；连入建屏；断开约 10 秒内日志出现「主控已断开」并拆屏。

### 2026-11-20 18:4x 同事机器装不上驱动：证书 + 选错设备 + 双开（已修）
- 同事证据链经逐条核对全部属实：① 驱动自签名证书（CN=Virtual Display Driver）不在受信任存储时 pnputil /add-driver 直接拒绝，而 setup-virtual-display.ps1 原先没有任何证书导入逻辑（官方包靠 installCert.bat 做这步）；② 应用 find_vdd 按友好名 "virtual display" 模糊匹配 + is_virtual 按适配器名 contains 匹配，在装了 GameViewer/AskLink 等别家虚拟适配器的机器上会先命中别人的设备去启用/重启/禁用；③ 同机双开两个实例。
- 修复：脚本在 pnputil 前从驱动包 .cat 提取签名证书（Get-AuthenticodeSignature）导入 LocalMachine Root + TrustedPublisher，不依赖包附带 .cer；find_vdd 改为只认实例 ID 带 MTTVDD 或硬件 ID 精确等于 Root\MttVDD/MttVDD（枚举附带 DEVPKEY_Device_HardwareIds），is_virtual 只认适配器名以 "Virtual Display Driver" 开头或带 MttVDD（INF 的 DeviceDesc 实测就是该字符串）；main 加 CreateMutexW 单实例互斥（windows 0.59 里该函数不在 Threading 路径，直接声明 kernel32 导入；第二个实例写日志退出）。pnputil 退出码 50 未做硬中止：本就启用时也可能非 0，成败仍以枚举验证为准。
- 教训：同一消息里并行发多个 edit + 依赖它们的 commit exec，exec 可能在 edit 落盘前执行——出现过「提交只落了导入行、函数体留在工作区」的半成品提交，后以补提交修复。依赖顺序的工具调用不要同批并行。

### 2026-11-20 19:1x Mac 应用中心打开 App 一直转圈（已修，待真机验证）
- 机主报：应用中心打开 App 后一直转圈，像找不到窗口，乱点一阵又移过来了。代码取证：PhoneDesk.awaitWindow 只轮询 shownWindows，而 shownWindows = AX 标准窗口 ∩ CGWindowList 在屏清单（.optionOnScreenOnly）——窗口最小化或开在别的桌面空间时永远不在清单里，只能等满超时（冷启动 30 秒/唤回 10 秒）；用户乱点把窗口弄回当前屏/取消最小化才被下一轮轮询捞到，与体感完全吻合。place() 本来就处理最小化还原与跨屏挪位，卡的只是发现环节。
- 修复：awaitWindow 等过 2 秒宽限期（hiddenGrace）后改查新增的 standardWindows（App 全部标准窗口，没最小化的优先），捞到没见过的直接走 place，并写日志「捞到不在屏上的 X」。协议/实现/假件/回归测试齐备，main CI 全绿（44724979482，342 项）。此修复在 v2.6.2 标签之后提交，未进 2.6.2。

### 2026-11-20 19:5x Windows 版补版本号与自动更新（已实现，待发版）
- 此前 Windows exe 无任何版本标识、无更新机制。已实现：仓库根 VERSION 为单一来源，build.rs 读入后以 SNUG_VERSION 编进程序并用 winresource 写 exe FileVersion/ProductVersion 字符串；主窗口标题改为「Snug for Sunlogin — Windows 版 vX.Y.Z」，启动写文件日志。
- 新增 windows/src/update.rs：启动静默查一次 GitHub 最新 Release（PowerShell Invoke-RestMethod 解析，只回一行「版本|zip|sha」），驱动组加「检查更新」按钮可手动查；有新版 MessageBox 确认后下载 zip、按官方 .sha256 用 Get-FileHash 验包、Expand-Archive 解压到 %TEMP%\SnugWindowsUpdate，写 swap.ps1（等旧进程退出→覆盖 SnugWindows.exe 与 setup-virtual-display.ps1→重新拉起），主进程拉起脚本（DETACHED_PROCESS，子进程继承管理员权限）后自发 WM_CLOSE 退出。版本号按数字段比较。CI 绿（44730288988）。
- 注意自举问题：此功能在 v2.6.3 标签之后提交，2.6.3 不含它；且现装的旧版没有更新器，用户需再手动下载一次含更新器的版本，之后才能自动更新。

### 2026-11-20 20:0x Windows 配置向导（已实现，CI 绿，待发版）
- 新增 windows/src/guide.rs 独立窗口，三步带通：第 1 步检查（精确认 MttVDD 设备 + 当前虚拟屏/手机尺寸状态）→ 第 2 步安装（在本窗口内跑 setup-virtual-display.ps1 -NoPause，stdout 实时滚进正文，结束自动复查；失败给日志路径 + 重新安装）→ 第 3 步验证（调主程序 create_phone_screen 后轮询 20 秒，确认虚拟屏出现且 is_phone_mode，通过显示分辨率收官）。复用主窗口 mk_control（改 pub(crate)）与 create_phone_screen（改 pub(crate)）。
- 三个入口：① 命令行 --guide（也认误写的 --guild）；② 正常启动时后台查一次驱动，缺失才 MessageBox 问要不要开向导（每次启动最多问一次）；③ 主窗口驱动组「配置向导」按钮（与「检查更新」并排，原整行改为两半）。驱动组提示文案改为「没屏先点『配置向导』一步步装；驱动只装一次」。
- 编译教训（windows 0.59）：SendMessageW 的 wparam/lparam 是 Option 形式；EnableWindow 不在 WindowsAndMessaging 常用路径，按钮禁用改走 GetWindowLongW/SetWindowLongW 改 WS_DISABLED 位（guide.rs set_enabled）。

### 2026-11-20 21:2x 界面与向导逻辑按机主反馈重排（已实现，CI 绿）
- 主窗口删掉整个「驱动」组（安装 VDD/检查更新/配置向导按钮 + 提示全撤；更新仍靠启动静默查，驱动的事全归向导）。布局新顺序：状态区 → 显示器列表 → 操作行（刷新/创建/删除依次排开，仅手动模式出现，自动模式整行让掉、布局随模式切换即时重排）→ 手机屏组（只剩型号 + 自动/手动 + 说明）→ 测试组（改整宽）→ 日志。
- 启动流程：状态区先挂「驱动检测中…」，每次启动都查 MttVDD（spawn_driver_check 回 WM_DRIVER_CHECKED），在就刷新状态，不在直接开向导——不再有 MessageBox 确认。--guide 仍可直接开。
- 向导扩为五步走完闭环：检查 → 安装 → 创建验证 → 删除验证（remove_phone_screen 后轮询确认虚拟屏消失）→ 测试（重新创建 + testwin::open 把测试画面铺满虚拟屏，手机连进来看串流收官）。创建验证按 verify_for 分流（Create→删除步，Test→开测试窗）。
- 测试窗口闪烁/不顺修复：窗口加 WS_CLIPCHILDREN（父窗口每帧 BitBlt 原先盖住顶部按钮逼其自绘）、WM_ERASEBKGND 返回 1 不擦背景、双缓冲后备位图改为缓存复用（原先每帧新建销毁 1080×2346 约 10MB 位图）。待机主真机确认按钮不再闪、动画更顺。
- 教训重犯一次：同一消息块里并行发多个 muse.edit 会互相覆盖（快照式写入），WM_COMMAND 的一处编辑曾被同批后续编辑冲掉，靠 grep 才发现；依赖型编辑必须逐条发、逐条验。

### 2026-11-20 21:5x 主窗口与向导现代化主题（已实现，CI 绿）
- 机主嫌原生 Win32 整窗系统灰丑。改法（不换框架，全靠自绘）：窗口底色 #F5F5F7；原生分组框撤掉，父窗口 WM_PAINT 手绘白色圆角卡片（矩形由 layout 写入状态），组名改粗体静态标签；五个按钮改 BS_OWNERDRAW 自绘（main.rs draw_button，主窗口与向导共用：创建/向导动作钮=品牌金橙+深棕字，删除=珊瑚红+白字，其余=白底描边，圆角 12，按下变深，禁用灰）；静态文字/单选/日志/列表配色走 WM_CTLCOLOR* 与 LVM 设色统一（状态色保留绿/橙，提示次灰 rgb(110,110,115)，正文 rgb(29,29,31)）。
- 向导闪烁与自绘的冲突已解：自绘按钮不能再翻 BS_DEFPUSHBUTTON 样式位，闪烁相位改走按钮 GWLP_USERDATA（0/1），draw_button 读到 1 时主按钮变深填充+3px 边。
- windows 0.59 名称位置教训（均查本地 crate 源码证实）：DRAWITEMSTRUCT 在 UI::Controls、InvalidateRect 与 DrawTextW 在 Graphics::Gdi、EnableWindow 在 UI::Input::KeyboardAndMouse、HDC 在 Gdi 不在 Foundation。

### 2026-11-20 22:1x 驱动与 nefcon 随包打包（机主要求，CI 绿）
- 背景：安装脚本三处要从 GitHub 下载（驱动包×2、nefcon×1），国内网络屡屡失败。
- 做法：钉住版本入仓库 windows/assets/（VDD 官方 25.7.23 驱动四件套 276K + nefcon v1.21.1 x64 nefconc.exe 1.1M，哈希与本机已验证副本一致）；release.yml 的 windows-dist 把它们打进 zip 的 vdd/、nefcon/ 子目录；脚本改随包优先——脚本目录排进 INF 磁盘搜索首位、Get-Nefcon 先查随包 nefcon\nefconc.exe；Ensure-PackageDir 本就有「INF 目录齐全即返回」的短路，随包天然命中；随包缺失才回退在线下载（同事单跑 raw 脚本的路径保留）。
- 顺带修自动更新的潜在 bug：发布 zip 里有一层 Snug-Sunlogin-Windows-<版本> 目录，旧 swap 脚本在解压根上找 exe 会扑空；改为递归定位 exe 所在目录后整套替换（含 vdd/nefcon 随包目录）。

### 待办（机主 2026-11-20 22:28 提出，后面再调）
- Windows 版被 iPhone 控制时，虚拟屏画面四角被手机圆角屏裁掉：左上角的悬浮启动器（App）按钮、启动器右上角的关闭按钮都会被圆角遮住一部分。后续把这两个角上的控件往内收（加安全边距）再调。

### 2026-11-20 22:4x 启动器数据层换真源 + 窗口尺寸恢复（已实现，CI 绿，在 v2.6.4 标签之后）
- 「全部」列表感觉不对的根源：旧实现只扫开始菜单 .lnk。现改为 .lnk（IShellLink 解析出目标 exe）∪ shell AppsFolder 枚举（开始菜单「所有应用」真身：桌面项解析名=exe 路径，打包/UWP 项=带 ! 的 AUMID，走 IApplicationActivationManager 激活、图标按 AppsFolder PIDL 取）；Cargo 需 Win32_System_Com + Win32_UI_Shell_Common 两个特性（后者门控 Common 类型与 IShellFolder 方法，查 crate 源码 cfg 确认）。
- 最近使用：读 %APPDATA%\Microsoft\Windows\Recent 的 .lnk 按 mtime 倒序，只留现存 exe 目标，上限 12；对上已安装列表的复用原条目。启动器「全部」无搜索时最近置顶（副文案「最近使用」，下面不重复），搜索时平铺。
- 运行中判定改 exe 精确匹配优先、名字/标题启发式兜底（UWP 窗口挂 ApplicationFrameHost 只能靠标题）；点已开 App 不再误开新实例，启动等待也先按 exe 认新窗口。
- 窗口尺寸恢复（机主 22:44 提出）：连入（WM_SESSION_START）先给所有运行中窗口拍 WINDOWPLACEMENT 快照（普通矩形+最大化态，首次为准）；fullscreen_on 动窗前逐个登记 touched。断开自动拆屏与手动「移除手机屏」前先 restore_saved_places：touched 的无条件还原，普查的只还原此刻还停在虚拟屏上的；还原后清表并记日志个数。手动保留屏（不拆）时不还原。

### 2026-11-20 23:0x v2.6.4 已发布（机主批准，六件资产齐全，已下载验包）
- Release run 44754225493 成功；资产：DMG + .sha256 + .pack + update.json + Snug-Sunlogin-Windows-2.6.4.zip + .zip.sha256。
- Windows zip 已实际下载校验：sha256 对得上；包内 exe/脚本/README 之外带 nefcon\nefconc.exe 与 vdd\ 四件套——随包驱动正式随版落地。
- 注意版本线：启动器真源列表/最近使用/窗口尺寸恢复三项在标签之后，只进 2.6.5。

### 2026-11-21 awesun-cli 远程实测（NateThinkbook，本实例经 CLI 全程操作）
- 通道全通：desktop 截图/点击/拖拽/打字、UAC 同意、锁屏 PIN 解锁、cmd exec（会话偶发掉线，rcmd.sh 连+跑同链+重试可绕；大输出会堵通道）、file 上传（--remote 传目录不是文件路径！）/下载（Snug 占用中的日志下载失败，用 PS Get-Content -Tail 读）。
- 新构建（CI run 44756821400 产物，含启动器真源+窗口恢复）装到 C:\Users\skyve\SnugTest\ 运行验证：标题 v2.6.4、现代化 UI、手动按钮行、驱动检测、设置持久化（Fold8 外屏）均正常。旧构建在 OneDrive\Desktop\SnugWindows.exe（615424 字节）未动。注意：应用内日志横幅仍印 v0.1.0（写死字符串）；失败提示仍写「点安装 VDD 排查」但新界面已无此按钮——两处文案待修。
- 旧构建 11:21:58 被本实例 CLI 连入触发完整自动建屏（1080×1706）+开资源库，U6 核心链路实证。新构建启动器日志：运行中 9 窗口、全部 247 个 App（AppsFolder 合并生效）、最近使用 0 个——Recent 文件夹里全是文件快捷方式（mp4/zip/log），无 App 启动记录；真「最近使用 App」需改读 UserAssist 注册表。
- **建屏回归（重要）**：11:46 本实例断开 desktop 会话后，向日葵在会话结束时把虚拟屏从显示拓扑摘除（VDD 设备与 VDD by MTT 监视器仍 OK，但无活动显示路径；主屏同时被向日葵改回、控制台被重新锁定）。此后新构建自动建屏 ×3 + 手动移除（禁用设备）→建屏 ×1 全部失败：设备启用/重启 pnputil 均退出 0，15 秒轮询枚举不到虚拟屏。11:21 那次成功在此事件之前。待验证：重启机器后是否恢复；或改建屏实现（MTT 管道挂载／监视器设备循环）。连带结论：断开时屏幕可能已被向日葵摘掉，Snug 的自动拆屏/窗口恢复（以 has_virtual 为门）赶不上——11:46 的 END 日志为「自动=是/自动建=否/虚拟屏在=否」未执行恢复；窗口被 Windows 自动挪回主屏，未丢失。
- 会话边沿特性：CLI 快速断开+重连（3 秒）不产生 END/START（宿主 agent 有宽限期，合并为一段）；断开约 10 秒后 Snug 检出 END。

## 2026-11-21 午间：重启后重跑完整测试（Zhang 拍板「重启 ThinkBook，我重跑完整测试」）
- 本实例经 CLI cmd 会话下发 shutdown /r 重启 NateThinkbook（12:22），设备掉线后约 1 分钟回在线；desktop 会话重连未再要访问密码；锁屏 PIN 由 Zhang 在聊天中再次提供（只用未存），上滑→点 PIN 框→输 PIN→回车解锁成功（第一次输 PIN 未点框焦点，回锁屏；第二次成功）。
- **重启未修复建屏（假设证伪）**：解锁后经 cmd 启动 SnugTest 新构建，UAC 点 Yes，12:33:19 启动（Fold8 外屏 1080×1706，自动模式开）。12:33:21 检出 desktopagent（会话在本实例连入中）触发自动建屏：配置已有 1080×1706、写盘 OK → 启用 VDD 设备 pnputil 退出 50（already enabled）→ 重启设备退出 0 → 15 秒轮询仍只枚举到 1 块屏（DISPLAY1 Intel Arc 1920×1080@60），12:33:48 报「重启后仍没枚举到虚拟屏／虚拟屏没就绪，资源库暂不打开」。与 11:46 后失败签名完全一致，干净重启不恢复。
- 现场取证（失败后）：vdd_settings.xml 中 count=1、1080×1706 在列；Display 类 ROOT\DISPLAY\0001（Virtual Display Driver，oem274.inf）状态 Started，即 Snug 重启的正是它；另有 ROOT\MTTVDD\0000 残留节点 Stopped 且无绑定驱动（首版脚本 SetupAPI 建的半成品，无害）；命名管道 \\.\pipe\MTTVirtualDisplayPipe 存在——驱动进程活着。结论：设备/驱动/管道/配置均正常，唯独重启设备不再产生显示路径；下一步改建屏实现（优先试管道命令挂载，参考 rizetngan 实现的 VddDriverClient；或监视器设备循环/CCD 挂载），不要再让用户反复重启。
- 因建屏失败，后续步骤（Explorer 移入虚拟屏→断开验窗口恢复/自动拆屏）无屏可测，本轮未跑。机器现状：Snug 新构建在跑、自动模式、虚拟屏未建；本实例 desktop 会话已断开（控制台按宿主惯例回锁）。

## 2026-11-21 下午：fix/vdd-stable-create 分支——根因定案 CCD 拓扑，建屏稳定化完成
- Zhang 指令「你建个分支做这个事情，看看你能不能把虚拟屏调稳定」。分支 fix/vdd-stable-create（自 main@91103c2），提交：a0bca01（devices: nefcon_install + cycle_device 抽出）、f1dbb7d（建屏三级阶梯）、d9d16d8（断开还原与拆屏解耦 + 过期文案修正）、d536215（新增 ccd.rs 拓扑激活级）、78d195f（WIN32_ERROR u32 编译修正）。CI run 44868407610 绿，产物装 ThinkBook 的 C:\Users\skyve\SnugTest2\ 实测（SnugTest 旧候选与桌面旧构建未动）。
- **根因定案（推翻「管道挂载」方向）**：设置→显示亲见——虚拟屏在场但多屏模式是「仅在 1 上显示」，2 号屏灰置。向日葵每次会话结束把 VDD 显示路径摘出活动拓扑并存盘（连源/目标模式索引都清成 0xFFFFFFFF），重启不恢复；设备 Started、监视器 Started、管道在、配置对，桌面枚举就是没有它。设备层三招（pnputil 重启/nefcon 重装/禁用启用循环）对此全数无效（12:46–12:47 三级全跑仍失败）；手动在设置里选「扩展这些显示器」立刻恢复（12:50 DISPLAY20），随后手动移除/重建一级即成（DISPLAY22）——证明持久化拓扑就是病灶。DisplaySwitch /extend 无效（不激活未列入当前拓扑的路径）。
- **修法**：新增 windows/src/ccd.rs——QueryDisplayConfig(QDC_ALL_PATHS) 按 MTT1337/「VDD by MTT」认 VDD 路径，未激活则补模式（源模式按当前机型规格摆桌面右缘；目标模式 pixelRate=w*h*hz、同步频率、PROGRESSIVE）后 SetDisplayConfig(APPLY|USE_SUPPLIED|ALLOW_CHANGES|SAVE_TO_DATABASE)；作为阶梯第二级（重启设备→**拓扑激活**→nefcon→禁用启用→再补一次拓扑激活）。成功只认枚举，不认返回码叙事。
- **实测三轮（2026-11-21 12:59–13:07，远程全程）**：①12:59 中毒态自动建屏：一级失败→CCD 查得「激活=否、源/目标模式索引 4294967295」→补模式→SetDisplayConfig 返回 0→DISPLAY26 1080×1706@60 (1920,0) 就绪+启动器自动开（247 App）。②13:00:40 断开：新解耦逻辑生效——先关启动器+还原（还原计数 0：被移上虚拟屏的 Firefox 是普查快照窗且断开时虚拟矩形已随宿主摘屏消失，按设计跳过，Windows 已自动挪回主屏无丢失），日志「虚拟屏已被向日葵摘除，跳过拆屏」。③13:01 重连：宿主自己把路径挂回（DISPLAY26 在场），Snug 未认领（自动建=否），断开时正确不拆。④13:06 重连：又是中毒态→CCD 第二次独立成功（DISPLAY27）；13:07 断开同②。两种重连形态（宿主自挂回 / 中毒）都处理正确。
- 遗留：模式索引仍有效的「仅清激活位」分支未单独实测（宿主摘屏签名实测就是连模式一起清，已覆盖实测分支）；窗口恢复对普查窗仍依赖断开瞬间虚拟屏在场（可改进：会话内记住最后虚拟矩形，未做）；缩放日志仍「100% → 100%」（U7，提高到 250% 未生效，与本分支无关的既存问题）。
- 分支未合 main、未发版，等机主定：v2.6.5 候选还包含标签后的启动器真源列表/最近使用/窗口尺寸恢复，一并待发。

## 2026-11-21 下午 · 启动器按使用频率排序 + 运行中加「常用 · 未打开」grid（分支 fix/vdd-stable-create，未合未发）

- 机主要求：①「全部」按使用频率倒排，常用软件（Office 系列、计算器等）靠前；②「运行中」tab 下面加一栏没打开过的常用 App 图标 grid；③在他电脑上跑起来测试、截图给他。
- 数据层（apps.rs）：读 HKCU UserAssist\{GUID}\Count（值名 ROT13；72 字节格式次数在偏移 4、末次 FILETIME 在偏移 60；16 字节格式在偏移 8；跳过 UEME_ 前缀；多 GUID 合并同键求和/取最大）。坑：注册表值数据可达 1612 字节，首版 256 字节缓冲撞 ERROR_MORE_DATA 直接 break，只读出 2 条键——改为 8192 缓冲且 MORE_DATA 跳过不中断后读出 1000+ 条。
- 匹配顺序：lnk 原始路径 → exe 原始路径 → aumid → 名称 → lnk 文件名键（lnk:）→ exe 主干（stem:）→ 名称主干。实测 UserAssist 的 exe 条目是已知文件夹 GUID 前缀（{1AC14E77-…}\cmd.exe=14），不是全路径；同 exe 多快捷方式会经 stem 冒名共享次数（cmd 家族七个快捷方式一度齐刷 14 次霸榜）——加同 exe 去重：组内有人有自有记录（Exact/LnkStem）时其余 Stem 继承清零，全组只有 Stem 时只留字典序第一个。
- 排序：score = 启动次数 + 常用加成 100；同分按末次运行倒序、再名称升序。常用识别 is_common_app()：exe 主干/显示名（含中文：计算器、记事本、画图、截图工具、文件资源管理器、设置、微信、企业微信、钉钉、飞书、腾讯会议、向日葵）/AUMID 命中清单（Office+WPS、系统工具、浏览器、通讯、VS 系、播放器等）。
- 界面（launcher.rs）：「全部」整体改频率序（原「最近使用」置顶摘除——Recent 文件夹只有文件快捷方式、恒 0）；「运行中」无搜索词时在运行列表后追加 Section 标题「常用 · 未打开」+ CommonTile（常用且未运行者，按 score 排，上限 18，点击走既有 Launch 流程）。布局泛化为 layout_items/content_height，绘制/命中/滚动统一消费（行 88 / 标题 54 / 瓦片 188）。
- 实测（ThinkBook SnugTest2，构建 b55f681，自动模式）：日志 1064 条键；前八 = Notepad 34、Chrome 11、Edge 4、AweSun 3、Anaconda Powershell 1、Windows PowerShell 1、Firefox 1、WeChat 0（全带常用标）。截图核验：运行中 tab 见常用 grid（Notepad/Chrome/Edge/AweSun/…/Excel/Word/腾讯会议），全部 tab 前排即频率序，计算器在常用包内（第 5 行）。注意：UserAssist 记的是启动次数，常开型应用（微信/浏览器）天然低分，靠常用加成留在前排——这是数据特性不是 bug。
- 截图方法备忘（远程截虚拟屏上的窗口）：awesun 截图只拍主屏；把启动器窗口移来主屏要过三关——①脚本里 GetClassNameW 必须带 CharSet.Unicode，否则类名只回首字母、永远 NOT-FOUND；②Snug 是提权进程，非提权脚本 SetWindowPos 被 UIPI 静默无效，须 Run 对话框里 Start-Process -Verb RunAs 提权后同级移动（坐标走脚本进程的 DPI 虚拟化空间，1030×1030 落点约 (430,25)）；③启动器开关是 toggle，点前先看日志定状态，点后别再乱点。移窗脚本 mv3b.ps1 在本机 ~/workspace/sunlogin-cli/，结果写 mv3result.txt 走文件通道回读。
- 待机主：与 CCD 修复同分支一并等合并/发版决定（机主 13:31 定「先不合，我自己再测测」不变）。

## 2026-11-21 傍晚 · 启动器页眉放大（同分支续）

- 机主看完频率版截图后点名：最上面一行放大美化，随后明确指「运行中/全部」两个按钮要大、搜索框不用那么长。
- 改动（launcher.rs）：页眉 64→96 改白带 + 底边分隔线；标题字 38→54；页签最终定格 168×60、字 40 号（金色滑块分段控件）；关闭钮圆径随行高到 60；搜索框去 WS_BORDER 方框，改绘制侧圆角描边（search_rect 与创建位置同源）、加「搜索 App」提示词、宽度封顶 320。
- 实测（ThinkBook SnugTest2，构建 7f4fbf5）：移窗截图核验——页签明显大、搜索框短、标题与关闭钮同步放大，见主会话 17:00 截图。分支仍未合未发，状态不变。

## 2026-11-21 晚：v2.6.5 正式发布（CCD 修复 + 启动器三项改进）

- 机主在 ThinkBook 上自测 SnugTest2 的分支构建后拍板（20:37「测了下还行，打正式release包吧」），解除 13:31「先不合」的决定。
- 合并与发版：fix/vdd-stable-create 快进合入 main（91103c2 → 5eeeee9），版本提交 146b475「版本升到 2.6.5」；main CI run 44904895347 绿；打附注标签 v2.6.5，Release run 44905062598 经机主批准 release 环境后于 20:45:47 发布（非草稿）。feat/windows-app 已同步快进到 main 并推送。
- 验包（主会话逐个下载实测）：六资产齐全——Snug-Sunlogin-2.6.5.dmg、.dmg.sha256、Snug-2.6.5.pack、update.json、Snug-Sunlogin-Windows-2.6.5.zip、.zip.sha256；`sha256sum -c` 对 DMG 与 Windows zip 均 OK；zip 内含离线全套（SnugWindows.exe 1,003,008 字节、setup-virtual-display.ps1、README、vdd/ 四件、nefcon/nefconc.exe）。Mac 签名/公证未在 Linux 侧另验（沿用 2.6.4 同口径，只验 sha256）。
- 本版内容：建屏 CCD 拓扑激活修复（断开后建不回来的根治）、启动器真实名单 + 窗口尺寸恢复、「全部」按使用频率倒排 + 常用加成、「运行中」底部「常用 · 未打开」grid、页眉放大（页签 168×60、搜索框封顶 320）。
- 截图方法补充（同日晚间先行验证）：截断之谜了结——此前移窗截图里行尾 ✕ 被切是移窗缩放的假象（虚拟屏 175% 与主屏 100% 缩放不一致，窗口按 1080 布局、可见仅约 1035）。改用 CCD 脚本 setprim2.ps1（SnugTest2 目录）把虚拟屏临时设为主显示器，远程画面即切到手机屏，截得 1080×1706 原生图：行尾 ✕ 完整、页眉各元素无截断；拍完已恢复原拓扑（主屏为主、虚拟屏在右），启动器关闭、悬浮按钮在位。
- 遗留不变：缩放提高 250% 未生效（U7，日志仍 100%→100%）；窗口恢复仍依赖断开瞬间虚拟矩形（可改进：会话内记住最后虚拟矩形）；启动器圆角屏内边距待调（机主 2026-11-20 定「后面再调」）；常用 grid 个别长名第二行略被压（17:22 已报机主，后面一起收拾）；启动器 open() 用缓存 vrect、会话中途切主屏会出现幻影位置（真实用户不切主屏，未修）。

## 2026-11-22 · Windows 高级模式（分支 feat/advanced-mode，未合并）

- 机主要求（2026-11-21 22:54）：主窗口左上角图标菜单（系统菜单）加「高级模式」，两个测试（测试窗口/启动器）挪进去，主区测试组删除、日志加高；高级模式里加勾选项「连入时仅输出虚拟屏（关闭物理屏）」，向日葵断开/退出必须恢复物理屏。另修：日志底色非白、自动/手动单选钮底色发灰。
- 实现：settings.only_virtual 持久化；vdd-state.txt 增 physical_off 标记（与内存双记，防删屏清状态文件时丢标记）；ccd::disable_physicals 关屏前校验 VDD 激活、应用不存盘（存盘拓扑即安全网）；关屏时先把 VDD 源位置移到 (0,0) 顶替主屏，否则 SetDisplayConfig 报 87。恢复入口三处：WM_SESSION_END、WM_DESTROY、启动自检。
- 单选钮灰带定案（连错三招才定位）：主题控件向父窗口要身下背景走 WM_PRINTCLIENT（DrawThemeParentBackground），不是 WM_ERASEBKGND；父窗口不处理时控件拿到系统按钮面灰 #F0F0F0。CTLCOLORBTN 改白、ERASEBKGND 按卡片构图擦、启动后 RedrawWindow 全体重擦均无效；补 WM_PRINTCLIENT 与 ERASEBKGND 共用 paint_main_background 后像素实测 (255,255,255)、截图无灰带。顺带删掉 main_wnd_proc 里重复的死 WM_ERASEBKGND 分支。日志非白同因：只读 Edit 走 WM_CTLCOLORSTATIC 而非 CTLCOLEDIT，已修。
- 恢复兜底（强杀实测逼出来的）：强杀进程后重启，启动自检恢复失败——向日葵断开时把物理屏路径的源/目标模式索引也一并抹掉（与 VDD 同病），路径手术报「源模式缺失且无处可借」。因关屏从不存盘，存盘拓扑里物理屏必然开着：restore_physicals 改为手术未生效时整体套用存盘拓扑（SDC_USE_DATABASE_CURRENT）。实测两轮（10:57、11:06）均为：手术失败→套存盘拓扑返回 0→复查有激活路径→恢复成功、physical_off 清零。
- 实测清单（ThinkBook SnugTest3，构建 96f2020，exe sha256 56fa0b95…，哈希在机上 certutil 核对一致）：菜单三项齐全；测试窗口、启动器均可从菜单打开（日志为据）；会话连着时勾选立即关物理屏（日志 10:56:45 全序）；连入自动关屏两轮；断开恢复（断开时向日葵先行恢复、Snug 复查无操作并清标记）；强杀→重启自检恢复（兜底路径）两轮通过；单选钮/日志底色像素与截图核验为白；测试组移除后日志区加高生效。
- 远程操作备忘：系统菜单序列必须在一次调用内连做（图标→悬停→点项），跨调用菜单会自己关；子菜单第三项 y≈0.304（点 0.326 会穿透到下面的手机型号下拉框）；awesun 文件下载 --local 传目标目录、上传 --local 传源文件；--override 本次对 70 字节文本回读校验一致，exe 仍按「先删后传、不带 override」。
- 状态：分支已推送，**未合并、未发版**，等机主决定。ThinkBook 现状：SnugTest3 最新构建运行中（自动模式、高级模式关），物理屏正常，虚拟屏待下次连入由 CCD 阶梯重建。

## 2026-11-22 中午 · 机主实测反馈两修（同分支续，未合并）

- 机主手机连入实测（11:08 起）：开黑屏模式（仅输出虚拟屏）后虚拟屏上资源库没打开、主窗口排版因 DPI 很乱。其会话日志取证：[11:08:51] 资源库打开（屏在 (1920,0)）、[11:08:52] 关屏把虚拟屏源移到 (0,0)——窗口不一定跟着走，且其后还有两个绘制层面的坑（见下）。
- 修复一（资源库对齐+重绘，9bd3ea1、04ea6a1）：turn_off_physicals 成功后调 launcher::move_to 按当前虚拟屏矩形钉回位置（列表或悬浮按钮）。但首轮复测发现窗口位置对了、页眉的页签与关闭钮仍消失：像素扫描证实橙/珊瑚色零命中。根因：窗口创建瞬间线程 DPI 上下文与目标屏不一致时系统按比例缩放初始尺寸（1080 被撑成约 1890），内容按错宽度画出；而窗口类没注册 CS_HREDRAW/VREDRAW、launcher_proc 也没有 WM_SIZE 处理，move_to 改小尺寸后系统不整窗重绘，旧布局被裁着留下（搜索框因 search_rect 有 500 封顶在两种宽度下一致，反而看不出问题）。修法：move_to 与 open() 在 SetWindowPos 后各补 InvalidateRect 整窗重绘。复测（12:34:47 日志「资源库已按虚拟屏新位置重新对齐」+截图）：页眉页签/关闭钮/行尾按钮/常用 grid 全部归位。
- 修复二（主窗口 DPI，b0784ce）：原 WM_DPICHANGED 只改 s.dpi 与窗口尺寸，注释自认「字体仍是创建时磅数」——黑屏模式下主窗口被挪到 175% 虚拟屏正是这条路，字号与控件框对不上、列表列宽还停在创建 DPI。改法：四套字体存进 MainState，DPICHANGED 时按新 DPI 重建、apply_fonts 重发全部控件、init_list_columns 重算列宽、再 relayout。复测截图：主窗口在虚拟屏上列全、字与框比例正常。
- 修复二带出的崩溃（584cbc9）：断开时拓扑变化触发 DPICHANGED，apply_fonts 在 borrow_mut 里 SendMessageW(WM_SETFONT)，控件同步重绘使父窗口重入 WM_CTLCOLORSTATIC 借 MAIN → RefCell already mutably borrowed，在 extern "system" 回调里 panic = 进程中止（12:36:19 实测崩，幸物理屏已先恢复、physical_off=0）。改法：借用内只换字体存句柄，释放后按快照发 WM_SETFONT 与列宽。同路径复测（12:40:31）：断开恢复完成、进程存活、无新 PANIC。
- 现状：分支头 584cbc9（exe sha256 6c837ed3…，已装 ThinkBook SnugTest3 运行中），仍未合并未发版，等机主决定。机主设置保留其原样（only_virtual=1、Galaxy Z Flip8）。
- 附带发现：ThinkBook 系统时钟今天多次跳变（日志时间戳出现逆序，如 12:35 的行排在 12:34 之前），排查时序时别全信时间戳；另 12:33 前后有一段旧构建（v2.6.4/测试版 v0.1.0 字样）的日志混入同一日志文件，来源未查明（疑似桌面快捷方式的旧副本被启动过），不影响当前结论。

## 2026-11-22 下午 · 资源库连入置顶（055d37f，同分支续）

- 机主要求：向日葵刚连上时虚拟屏上的资源库窗口要置顶。实现：launcher::raise()（SetWindowPos HWND_TOP + SetForegroundWindow），在 open() 成功后与 move_to() 末尾各调一次；只做一次性提位不挂 TOPMOST（点选 App 后资源库会关，长期置顶反而挡被打开的窗口）。构建已部署 ThinkBook SnugTest3（sha256 b55da642…）。
- 验证（部分）：会话 12 连入后资源库在虚拟屏全屏置顶，桌面上 firefox/chrome/explorer（8 窗）/Settings 等都在运行列表里、无一盖住资源库（截图 report-launcher-topmost.png）。更严的「应用全屏在虚拟屏时重连、资源库重新盖上」复测没跑完：awesun-cli 桌面会话自 13:07 起频繁秒断、命令通道间歇无响应（本日十余轮会话+拓扑翻转后主控端状态劣化），非 Snug 问题——Snug 日志显示每轮连入/关屏/对齐/断开恢复都正确执行。
- 测试坑（重要）：未批的 UAC 提示会挂在安全桌面上，后果连环——桌面串流秒断、且 Snug 的关物理屏 SetDisplayConfig 返回 5（访问被拒，应用正确地保持原样并打了警告）。本日 13:07/13:09 两次关屏失败皆源于此（kill.ps1 的 RunAs 叠了两个没批）。教训：RunAs 脚本只在有活会话能立刻批时才发；发完必须确认批掉。
- 遗留现场：为拍虚拟屏原生截图用 setprim2 把主屏换到虚拟屏（phone 布局，已存盘），恢复脚本 setprim2 -layout orig 的 UAC 还挂在 ThinkBook 控制台上没批成（串流秒断批不了）——已请机主在本机点「是」恢复原布局（物理屏回主屏）；若点了「否」需另行处理。当前拓扑：虚拟屏 (0,0) 主屏、物理屏 (-1920,0)，双屏都开着，Snug 运行中，设置仍是机主的（only_virtual 开、Flip8）。

## 2026-11-22 下午续 · 资源库打开慢修复（1e023c7，同分支续）

- 机主反馈：点悬浮 App 图标后图标消失很久、列表迟迟不来，问是不是列表刷新慢。证实：home_proc 点击直接调 open()，open() 先 hide_home()，然后在 UI 线程同步跑四件套——running_windows、installed_apps（开始菜单遍历 + 每个 .lnk 的 COM 目标解析 + AppsFolder 枚举）、recent_apps、usage_map（UserAssist 1064 键）——跑完才 CreateWindowExW。图标先没、窗口后到，空白期就是全量扫描的一两秒。
- 修法：installed/recent/usage/eff 打包成 AppsData 缓存（Mutex 静态）：首次打开全量扫填缓存，之后打开只现扫 running（快且必须新鲜），数据克隆缓存（日志实测备数据 0 ms）；缓存超 120 秒在打开时顺手后台刷新（本次用旧数据、下次是新）；主窗口 WM_CREATE 预热一次。诊断日志（扫描完成/频率前 8）移进 scan_apps_data，open 只留一行摘要带来源与耗时。另把 hide_home() 挪到窗口建成之后——图标不再先于列表消失，打开失败时图标也留着。
- 实测（ThinkBook，exe sha256 36a3db73…）：启动 14:04:58 预热、14:05:00 连入自动打开即「数据来自缓存，备数据 0 ms」；14:05:57 点悬浮图标重开同样 0 ms、约 1 秒内截图已见列表全屏。原全量扫描在后台线程约 2 秒量级（246 个 App）。
- 部署插曲（取证后才定论）：SnugWindows-new.exe 这个文件名经 awesun 文件通道上传后在被控端「不存在」（同文件换名 SnugWindows-test9.exe 上传/回读哈希都正常，原因未明，疑似主控端文件服务对该路径的负缓存）；今后部署一律换新文件名并回读验哈希。deploy3.ps1（杀进程+setprim2 恢复 orig 布局+换 exe+启动，一次提权）已把主屏布局恢复原样（物理 (0,0) 主屏、虚拟屏 (1920,0)），14:06 断开后状态核验干净（physical_off=0）。
- 注意：机主 13:55 前后自己在用这台机器（型号已由他切到 Galaxy Z Fold8 外屏 1080×1706、only_virtual 仍开），测试勿动其设置。

## 2026-11-22 下午续二 · 多语言（中/英）+ 机主放行发版

- 机主 14:26 拍板发版，并要求加多语言：主窗口右上角语言选择（简体中文 / English），界面、日志、向导都要多语言。范围：Windows 版（Mac 版未动）。
- 实现：新增 lang.rs——全局 AtomicBool + tr(zh, en) 取词；带插值的走 tr_fmt! 宏（format! 要求字面量格式串，宏内按语言分发到两条 format!，参数只写一遍、命名捕获 {name} 两边都要保留——ccd 复查日志曾因英文模板把 {still_on} 写成 {} 编译失败一次）。settings 增 english=0/1 持久化，启动即按它设语言。主窗口右上角 CBS_DROPDOWNLIST（ID_LANG=1010），切换即时生效：静态文案/标题/列表列头/型号下拉（display_name 英文替换品牌名：三星→Samsung、内屏→Inner screen 等，存的 id 仍是中文原名与 Mac 共用）/系统菜单（GetSystemMenu 还原后重装高级菜单）/列表刷新全量重绘；启动器与向导在打开/绘制时取词，下次打开即新语言。覆盖：main 161 对、launcher 31、guide 45、后端 ccd/vdd/devices/display/scale/session/testwin/update 约 100；apps.rs 的中文是常用 App 匹配键（数据）不译。英文文案收短一轮（单选/标签按中文宽度排版，长句会被裁）：Model、Auto: create on connect, remove on disconnect、Manual: use the buttons above。
- 实测（ThinkBook SnugTest3，构建 087a5af，exe sha256 89d4101f…，机上哈希核对一致）：下拉点选 English → 主窗口标题/状态/列头/型号（Samsung Galaxy Z Fold8 Outer screen）/单选/菜单（Advanced 子菜单三项）全英文，新日志行英文；重启后按 english=1 从首行起英文；--guide 打开向导全英文（标题/五步/按钮 Next/Close）；连入后启动器全英文（Search apps、Running/All、8 win、Common · Not open）；切回后 settings 落盘 english=0、界面与启动器恢复中文。强杀恢复兜底顺带在野复测一次：phase7 强杀关屏中的旧实例，新实例启动自检走存盘拓扑恢复成功。
- 现场注意：接手时 only_virtual 已是关——机主上午自测后自己关掉的（14:04 日志还是开、14:42 变关，期间只有他在用），测试全程未动其设置，最终原样交回（only_virtual=0、english=0、Fold8 外屏、自动模式）。awesun-cli 的鼠标 click 对系统菜单项不可靠（点了不关菜单），菜单类操作改走 settings 文件 + 会话重连验证。

## 2026-11-22 下午 · Mac 版多语言（分支 feat/mac-i18n，未合并）

- 机主要求：Mac 版也加多语言，跟 Windows 一样，并加系统自适应 + 手工切换。实现与 Windows 同构：config.json 增 language（system/zh/en，默认 system，容错解码）；Lang 取词（选 zh/en 用它，system 时看 Locale.preferredLanguages 首项是否 zh）；AppState.english 可观察镜像，SwiftUI 界面走 state.tr 改语言即重画，非界面（菜单/状态文案/更新失败原因）走 Lang.tr 静态镜像，AppController 在启动与 changeConfig 后统一 apply。
- 切换入口：设置窗口通用页新增「界面语言」行（Picker：跟随系统/简体中文/English，后两者用各自语言写）。手机屏面板的文案是建时定的：PhoneScreenWindows.follow 跟踪 state.english，update 里语言变了就 tearDown+build 重建（与换屏同一条路）；小横条无障碍名也随语言。
- 覆盖：设置页、菜单栏、StatusText 全部状态/提示、PermissionName（权限名按语言取系统叫法，PermissionFlow 传 en/zh-Hans）、欢迎页、关于页（含致谢 usage）、更新界面与 UpdateProblem、手机屏（首页/卡片/搜索/型号页/小部件/裁剪/截图浮层）。不译的：日志与 changeConfig 的 what 描述（Mac 无界面日志窗，属内部诊断）、AppBundle 错误细节（只进日志）、「Snug 手机屏」虚拟屏名（屏的真名，感知可能按名匹配，作数据）、App 匹配类字符串（NewWindowMenu/ControllerKind 等，同 Windows apps.rs 的处理）。
- CI：run 44998440895 绿（swift build + 全部测试）。
- 界面验证走编译机渲染（机主问起才想起 App 自带 --render-*）：Renderer 增 SNUG_RENDER_LANG 环境变量选语言（AppState.english 默认取 Lang 当前值，样例状态自动跟随）；ci.yml 增手动触发 job「渲染界面截图（中英）」，macos-26 上构建后对 6 个界面各渲中英两套、传 artifact。run 45005498416 两 job 全绿，下载 112 张 PNG 逐张看过关键图：英文设置页（含 Language 行显示 Follow System）、英文手机屏列表（Running/All、1 window/2 windows 单复数、Minimized）、英文欢迎页、中文设置页（界面语言行显示跟随系统）均正确；图里残留的中文是样例数据（App 名、窗口标题）和虚拟屏真名「Snug 手机屏」，非界面文案。注意：新工作流文件不在默认分支时 gh workflow run 报 404（dispatch 只能用默认分支上已有的工作流名），所以渲染 job 挂在既有 ci.yml 上、用 workflow_dispatch 触发、--ref 指分支即可（用的是分支上的 ci.yml 内容）。
- **首装向导留下的屏首次断开不删（2026-11-23 用户报告，已定位+已修，待真机实测）**：根因是所有权标记 `screen_auto_owned` 只在「会话连入自动建屏」和「切到自动模式时补建」两条路上置位，而首装向导第 3 步（验证建屏）和第 5 步（测试画面）直接调 `create_phone_screen()` 从不登记——向导走完留下一块屏，连入时见屏已在又跳过自动建屏，断开时判「不是自动建的」不拆；用户切手动删掉再切回自动后，下一轮连入才头一回走上自动建屏那条路，于是「第二次就好了」。附带同病：标记纯内存，程序中途重启（更新/崩溃）同样丢所有权。修复在分支 `fix/wizard-screen-owner`（提交 b62baa6 + 8ceedbb）：①vdd-state 新增 `screen_auto_owned` 落盘（与 physical_off 同理），`set_screen_owner` 写穿、启动按落盘值恢复；②向导两处建屏后调 `adopt_created_screen()` 按当前模式认领（自动→归会话管，手动→归用户）。CI Windows 构建绿（run 45102405856），测试 exe 已放 ThinkBook `SnugTest3\SnugWindows-fix1.exe`（sha256 e9543781…，机上 certutil 验过）。**实测被堵**：ThinkBook 锁屏（PIN 未存）、试 RunAs 留下一个挂在锁屏安全桌面上的 consent.exe（PID 31224）——机主下次解锁若看到 PowerShell 的 UAC 提示点「否」即可；旧版 Snug（PID 17432）未动、机器状态无变化。解锁后可继续远程实测（脚本 fix1-payload.ps1 已备：自动点定向导五步+验 vdd-state），或机主手动两分钟验完。
- **已发版 v2.7.1（2026-11-22 晚，机主批）**：feat/mac-i18n 快进合并 main（19080fe），VERSION 2.7.1，main CI 绿，tag v2.7.1；Release run 45006218482 两 job 全绿，六个资产逐个下载验 sha256 均 OK。feat/windows-app 已同步 main。机主未再提 mini 真机测，以 CI 渲染验证为准。
- README 截图一节（b117903）：docs/images/ 放四张图——Windows 主窗口（ThinkBook 实机窗口级截图）、Windows 资源库（11-21 实机图）、Mac 设置与 Mac 资源库（CI 渲染图）。
- ThinkBook 窗口级截图方法（capwin3.ps1，在 sunlogin-cli 工作区）：枚举进程顶层窗口 + CopyFromScreen；**必须先 SetProcessDpiAwarenessContext(-4)**，否则 GetWindowRect 是虚拟化坐标、截出来残缺。PrintWindow 对本 App 返回 false，只能 CopyFromScreen（窗口被遮挡会截到上层窗口）。awesun-cli 文件传输语法：`file transfer <session> -type up|down -local <路径> -remote <目录(up)/文件(down)> [-override]`（rfile.sh transfer 后面直接跟 flags）。注意：运行中的 SnugWindows 是提权进程，cmd 通道的 taskkill/WM_CLOSE 都被 UIPI 挡（Access denied）；桌面会话 Alt+F4 若焦点在桌面会弹关机对话框——别乱试。ThinkBook 上 SnugTest3\SnugWindows.exe 已换成 2.7.0 发布版 exe（哈希验过），旧测试版改名 .bak；但进程仍是旧构建（标题 v2.6.5，代码与发布版只差 VERSION 字符串），下次重启 App 即为发布版。
