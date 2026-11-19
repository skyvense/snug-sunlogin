# Snug 图标资源

| 文件 | 用途 |
|---|---|
| `AppIcon.svg` | App 图标矢量源文件（1024 画布，已含 macOS 圆角底板与阴影） |
| `AppIcon-1024.png` | App 图标 1024×1024 PNG |
| `AppIcon.icns` | macOS 应用图标，放进 `.app/Contents/Resources/`，并在 Info.plist 设 `CFBundleIconFile = AppIcon` |

界面里的其它图标（菜单栏、设置页、手机屏）一律用 SF Symbols，不另外绘制。

## 设计说明

方案「双屏叠片」沿用原版 Snug 的构图：横屏 = 被控电脑，竖屏 = 手机，交叠处深色 = 分辨率对齐。
向日葵版只换配色为葵花金橙，与原版的青蓝区分：`#FFC21A → #FF9E00`（电脑，金黄）、
`#FF7A1A → #F25C05`（手机，橙）、`#D94800`（交叠，深橙）；底板由冷灰蓝改为微暖米白
（`#FFFFFF → #FAF1E6`），阴影随之改暖棕。原版配色见上游仓库。
