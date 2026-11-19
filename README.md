<div align="center">

<img src="Assets/AppIcon/AppIcon-1024.png" width="128" alt="Snug 图标">

# Snug · 向日葵版

**让手机远程连上 Mac 时，拥有一块为手机而生的屏幕**

竖屏原生显示 · 原生手势 · App 资源库 · 适配所有手机

macOS 14+ · 向日葵远程控制 · MIT

</div>

## 这是什么

Snug · 向日葵版是一款 macOS 菜单栏应用：当你用向日葵手机 App 远程连接一台 Mac 时，它在被控 Mac 上创建一块与手机同比例的竖屏（虚拟显示器）。窗口在竖屏里铺满整屏、字号放大，单指滑动变成滚动、长按出右键菜单——不再把整块横屏桌面等比缩进手机的小屏幕里。

## 关于本项目

本项目由向日葵（贝锐 Oray）的一名技术人员以**个人身份**维护：看到原 Snug 项目后，使用 **Muse（免费版）**将其移植到向日葵远程控制并重新打磨。本项目为个人作品，**非向日葵官方产品**。原项目作者与许可证见文末「致谢与许可证」。

## 实测状态

**已在真实 Mac 上测试可用。** 在 Mac mini（M4）/ macOS 26 / 向日葵 16.8.1 环境下实测：手机连入后虚拟屏自动升起、窗口铺满、手势滚动与点击、长按右键均工作正常。正式发布包经 Apple Developer ID 签名与公证，可直接安装使用。

## 下载与安装

1. 从 [Releases](https://github.com/skyvense/snug-sunlogin/releases) 页面下载最新版 `Snug-<版本>.dmg`
2. 打开 DMG，把 Snug 拖进「应用程序」
3. 首次打开时按欢迎页授予「辅助功能」权限，之后常驻菜单栏
4. 手机用向日葵连入这台 Mac，在向日葵的屏幕列表里选择「Snug 手机屏」即可

## 系统要求

- 被控端：macOS 14+，已安装向日葵 Mac 版（AweSun）
- 控制端：手机向日葵 App（iPhone / Android 均可）

## 工作原理

当前版本的 Snug 不调用向日葵的任何接口。它只观察被控 Mac 本机的三路信号——向日葵的悬浮窗、系统推流日志、控制端列表——来判断「有手机连进来了」，然后升起虚拟屏并接管手势翻译。因此它与向日葵客户端的具体版本解耦，客户端升级一般不影响使用。细节见 [docs/how-it-works.md](docs/how-it-works.md)，移植的逐项适配记录见 [PORTING.md](PORTING.md)。

## 路线图

- **对接向日葵开放 API**：后续版本考虑直接对接向日葵官方开放 API。届时连接建立后即可自动准备好手机屏，不再需要用户在屏幕列表里手动选择手机屏。
- **控制端识别适配**：进一步适配向日葵的控制端列表结构（当前对未知控制端按手机处理）。

## 从源码构建

```
git clone https://github.com/skyvense/snug-sunlogin && cd snug-sunlogin
scripts/snug check     # 构建并运行全部测试
scripts/snug install   # 构建、安装并打开
scripts/snug dist      # 打签名 + 公证的 DMG（需要 Developer ID 证书与公证档案）
```

发版走 GitHub Actions：推送 `v*` 标签、在仓库的 `release` 环境审批后，自动产出签名公证的 DMG 并挂到对应 Release。

## 与原版的关系

- 本项目与原版 Snug 可同机共存：App bundle id、数据目录、日志文件均已隔离，更新源独立，不会被原版更新包覆盖
- 原版仓库：[zephyr4123/snug](https://github.com/zephyr4123/snug)（UU远程版）

## 致谢与许可证

MIT © 2026 AoyamaShou，© 2026 zephyr4123（见 [LICENSE](LICENSE)）。

本项目基于 [zephyr4123](https://github.com/zephyr4123) 的 [Snug](https://github.com/zephyr4123/snug) 移植，其前身为 AoyamaShou 的 [UUyc-companion](https://github.com/aoyamashou/UUyc-companion)。原作者版权声明保留于 LICENSE，逐项改动见 PORTING.md。
