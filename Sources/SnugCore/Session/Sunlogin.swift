// 向日葵（AweSun）的身份。各处认向日葵（感知、手势只接管它发来的操作、App 资源库里排除它）都认这里。
//
// 这是从 Snug 的 UU远程 版移植到向日葵时唯一集中改动的身份文件。移植说明见仓库根目录 PORTING.md。
import Foundation

public enum Sunlogin {
    /// 向日葵 Mac 主程序（AweSun.app）。bundle id 已由公开资料核对：
    /// 安装收据 com.oray.sunlogin.macclient、Homebrew cask `awesun` 均为此值
    public static let bundleID = "com.oray.sunlogin.macclient"
    /// 向日葵全家桶的 bundle id 前缀（贝锐 Oray）：企业版、控制端等变体的 bundle id 与主程序不同，按前缀认，免得逐个收集
    public static let bundleIDPrefix = "com.oray."

    /// 是不是向日葵（主程序、企业版、控制端都算）
    public static func isSunlogin(_ bundleID: String) -> Bool {
        bundleID == Sunlogin.bundleID || bundleID.hasPrefix(bundleIDPrefix)
    }
    /// 被远程时在本机推流、注入鼠标键盘的进程（按可执行文件名认）。
    ///
    /// 已实机核对（2026-11-18 发现报告，Mac mini M4 / 向日葵 16.8.1）：系统日志里的
    /// ScreenCaptureKit 推流事件全部来自 AweSun 的桌面代理 AweSun_Desktop
    /// （Contents/Helpers/AweSun_Desktop.app，launchd 名 com.oray.awesun.desktopagent），
    /// 不是主程序 AweSun。日志判定与手势接管的事件来源都认这个名字。
    public static let serverProcess = "AweSun_Desktop"
    /// 放覆盖窗口的是主程序 AweSun：被控时它在屏上放一个图层 1001 的悬浮窗
    /// （2026-11-18 发现报告的窗口清单）。主程序自己的普通窗口在图层 0，
    /// 所以覆盖判定还要看图层，见 overlayWindowMinimumLayer 与 SunloginApp.overlayVisible。
    /// 已核对（2026-11-24 问题机诊断日志）：未被控时也有一个图层 1001 的 0x0 占位窗口常驻在屏，
    /// 只认图层会误报在场；判定还要求窗口有真实面积，见 SunloginApp.overlayVisible。
    public static let overlayWindowOwner = "AweSun"
    /// 覆盖窗口的最低图层：普通窗口为 0，实测被控悬浮窗为 1001，取 100 分隔
    public static let overlayWindowMinimumLayer = 100
}
