<div align="center">

<img src="Assets/AppIcon/AppIcon-1024.png" width="128" alt="Snug 图标">

# Snug · 向日葵版

**让 Mac 在手机上拥有原生级的操作体验 —— 经向日葵远程控制**

竖屏原生显示 · 原生手势 · App 资源库 · 适配所有手机

macOS 14+ · 基于 [Snug](https://github.com/zephyr4123/snug)（UU远程 版）移植 · MIT

</div>

> ⚠️ **移植初版，未经实机验证。** 这份代码在没有 macOS 的环境里完成移植，尚未编译、未上真机。
> 适配点、候选值与定版步骤全部写在 [PORTING.md](PORTING.md)，动手前请先读它。

> **来源说明：本项目从原作者的代码修改而来。** 原项目是 [zephyr4123](https://github.com/zephyr4123) 的
> [Snug](https://github.com/zephyr4123/snug)（为 UU远程 打造，其前身为 AoyamaShou 的
> [UUyc-companion](https://github.com/aoyamashou/UUyc-companion)）。本分支在其代码基础上把远程通道
> 适配为向日葵，遵循 MIT 许可证，原作者的版权声明保留于 [LICENSE](LICENSE)，逐项改动见 [PORTING.md](PORTING.md)。

## 这是什么

原版 Snug 为网易 UU远程 打造：手机远程连上 Mac 时，它给 Mac 加一块与手机同比例的竖屏，
窗口在竖屏里铺满整屏、字号放大，单指滑动变滚动、长按出右键菜单——整块桌面等比缩进手机的憋屈没有了。

本分支把远程通道换成 **向日葵远程控制（AweSun）**，其余体验不变。Snug 不调用远程软件的任何接口，
只观察它在本机的三路信号（覆盖窗口、系统推流日志、控制端列表）来判断「手机连进来了」，
所以移植集中在一处身份层；虚拟屏、窗口搬运、手势翻译、字号记忆与还原记录全部复用原版实现。

## 系统要求

- 被控端：macOS 14+，已安装向日葵 Mac 版（AweSun）
- 控制端：手机装向日葵 App（iPhone / 安卓均可，原版的机型库通用）
- 构建：Xcode + Swift 6

## 构建与安装（在 Mac 上）

```
git clone <本仓库> && cd snug-sunlogin
scripts/snug check     # 构建并运行全部测试
scripts/snug install   # 构建、安装并打开
```

首次打开按欢迎页授予「辅助功能」权限，之后常驻菜单栏。手机用向日葵连入后，
在向日葵的屏幕列表中选择「Snug 手机屏」即可。

## 定版前必做的实机核对

1. 向日葵被控端是否枚举并能切换到虚拟显示器（整个方案的前提）
2. 推流 / 注入输入的真实进程名（现候选值 `AweSun`）——在被控 Mac 上跑
   `scripts/discover-sunlogin.sh --during` 一次抓齐，改 `Sources/SnugCore/Session/Sunlogin.swift` 一行
3. 手机屏比例扣减值按向日葵手机端工具栏校准

细节与逐项对照表见 [PORTING.md](PORTING.md)；工作原理见 [docs/how-it-works.md](docs/how-it-works.md)
（上游文档，信号细节以 PORTING 为准）。

## 与原版的关系

- 本分支与原版可同机共存：App bundle id、数据目录、日志文件均已隔离
- 更新源已改指本分支，不会被原版更新包覆盖
- 与向日葵及其开发商贝锐无关，非官方软件

## 许可证

MIT © 2026 AoyamaShou，© 2026 zephyr4123（见 [LICENSE](LICENSE)）。本分支为移植作品，
界面与演示素材来自上游仓库。
