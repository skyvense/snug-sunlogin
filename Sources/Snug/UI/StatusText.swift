// 状态文案：一个标题 + 一个短说明。把 AppState 翻成菜单栏和设置窗口共用的状态，也给手机屏上的提示用；
// 手机屏列表里窗口、App 的几处说法也在这里。只用短词，书面、克制
import SnugCore
import SwiftUI

struct StatusText: Equatable {
    var headline: String
    var detail: String?

    init(headline: String, detail: String?) {
        self.headline = headline
        self.detail = detail
    }

    @MainActor
    init(_ s: AppState) {
        headline = Self.headline(s)
        detail = Self.detail(s)
    }

    @MainActor
    private static func headline(_ s: AppState) -> String {
        if s.quitting { return s.tr("正在退出", "Quitting") }
        if !s.accessibilityTrusted { return s.tr("需要\(PermissionName.accessibility)权限", "\(PermissionName.accessibility) permission required") }
        switch s.connection {
        case .idle: return s.tr("等待连接", "Waiting for connection")
        case .reconnecting: return s.tr("连接中断", "Connection lost")
        case .connected: return s.controllers.isEmpty ? s.tr("远程中", "In session") : "\(s.controllers.joined(separator: s.tr("、", ", "))) · " + s.tr("远程中", "In session")
        }
    }

    @MainActor
    private static func detail(_ s: AppState) -> String? {
        if s.quitting { return s.tr("正在还原窗口", "Restoring windows") }
        if !s.accessibilityTrusted { return s.tr("用于移动窗口与转换手势", "Used to move windows and translate gestures") }
        if !s.config.phoneScreen { return s.tr("手机屏已关闭", "Phone screen is off") }
        if s.connection == .idle { return nil }
        if s.gesturesFailed { return s.tr("手势转换未启用", "Gesture translation is not active") }
        switch s.phone.screen {
        case .preparing: return s.tr("正在准备手机屏", "Preparing the phone screen")
        case .failed: return s.tr("手机屏创建失败", "Phone screen creation failed")
        case .ready:
            guard s.phoneInUse else { return s.tr("手机屏已就绪", "Phone screen ready") }
            return s.phone.placed.map { s.tr("手机屏", "Phone screen") + " · \($0.window.app)" } ?? s.tr("手机屏使用中", "Phone screen in use")
        case .off:
            if s.phone.dismissed { return s.tr("本次远程已退出手机屏", "Phone screen dismissed for this session") }
            return ControllerKind.wantsPhoneScreen(s.controllers) ? nil : s.tr("电脑控制端，不提供手机屏", "Desktop controller — no phone screen")
        }
    }
}

extension AppState {
    /// 状态的颜色，菜单栏的圆点和设置页顶部的图标共用：绿色远程中；橙色要处理（缺权限、连接中断）；灰色等待连接、手机屏关着
    var statusColor: Color {
        guard accessibilityTrusted else { return .orange }
        guard config.phoneScreen else { return .gray }
        switch connection {
        case .connected: return .green
        case .reconnecting: return .orange
        case .idle: return .gray
        }
    }
}

extension LoginItemIssue {
    /// 设置页的问题行：出了什么事，去哪里处理
    var text: StatusText {
        switch self {
        case .needsApproval: StatusText(headline: Lang.tr("开机启动待允许", "Launch at login needs approval"), detail: Lang.tr("系统设置 › 通用 › 登录项", "System Settings › General › Login Items"))
        case .notInApplications: StatusText(headline: Lang.tr("开机启动未生效", "Launch at login is not active"), detail: Lang.tr("需位于「应用程序」", "Snug must be in the Applications folder"))
        case .failed: StatusText(headline: Lang.tr("开机启动未生效", "Launch at login is not active"), detail: Lang.tr("请稍后重试", "Please try again later"))
        }
    }
}

extension PhoneChoice {
    /// 名字：机型全名，自己填的叫「自定义」（分辨率另外写，见 sizeText）
    var title: String { model ?? Lang.tr("自定义", "Custom") }
    /// 自己填的分辨率：打开自定义输入时先填上它
    var customScreen: PhoneScreen? { model == nil ? screen : nil }
}

extension PhoneScreen {
    var sizeText: String { "\(width) × \(height)" }
    /// 自定义输入下面的说明（设置窗口、手机屏共用）
    static var customHint: String { Lang.tr("屏幕分辨率或比例，如 1080 × 2400、9 × 20", "Resolution or aspect ratio, e.g. 1080 × 2400 or 9 × 20") }
}

extension PhoneDesk.Notice {
    /// 手机屏列表顶部的提示：出了什么事，和接下来怎么办。不出现技术词
    var text: StatusText {
        switch self {
        case .windowGone(let app): StatusText(headline: Lang.tr("\(app) 窗口已关闭", "\(app)'s window was closed"), detail: Lang.tr("请另选一个窗口", "Pick another window"))
        case .windowUnresponsive(let app): StatusText(headline: Lang.tr("\(app) 无响应", "\(app) is not responding"), detail: Lang.tr("可能正忙，请稍后再试", "It may be busy — try again shortly"))
        case .recordUnwritable: StatusText(headline: Lang.tr("无法保存原位置", "Couldn't save the original position"), detail: Lang.tr("为确保能放回原处，窗口未移动", "The window was left in place so it can be restored"))
        case .screenNotReady: StatusText(headline: Lang.tr("手机屏未就绪", "Phone screen not ready"), detail: Lang.tr("屏幕正在调整，请稍后再试", "The screen is adjusting — try again shortly"))
        case .openFailed(let app): StatusText(headline: Lang.tr("无法打开 \(app)", "Couldn't open \(app)"), detail: Lang.tr("请稍后再试", "Please try again later"))
        case .noNewWindow(let app): StatusText(headline: Lang.tr("\(app) 无法新建窗口", "\(app) can't create a new window"), detail: Lang.tr("可点 App 打开已有窗口", "Tap the app to open an existing window"))
        case .noWindow(let app): StatusText(headline: Lang.tr("未找到 \(app) 的窗口", "No window found for \(app)"), detail: Lang.tr("App 已打开，但没有可显示的窗口", "The app is open but has no window to show"))
        case .quitStalled(let app, _): StatusText(headline: Lang.tr("\(app) 未退出", "\(app) didn't quit"), detail: Lang.tr("强制退出将丢失未存储的内容", "Force quitting will lose unsaved changes"))
        case .reshaped: StatusText(headline: Lang.tr("屏幕比例已调整", "Screen ratio adjusted"), detail: Lang.tr("在 向日葵 中切到其他屏幕再切回，点按位置才准确", "In Sunlogin, switch to another screen and back so taps land accurately"))
        case .reshapeFailed: StatusText(headline: Lang.tr("屏幕比例未能调整", "Screen ratio couldn't be adjusted"), detail: Lang.tr("下次连接时生效", "It will take effect on the next connection"))
        case .zoomLeftBehind(let app, let steps): StatusText(headline: Lang.tr("\(app) 窗口已关闭", "\(app)'s window was closed"), detail: Lang.tr("再打开时字号若偏大，按 \(steps) 次 ⌘- 复原", "If text looks too large when reopened, press ⌘- \(steps) times to restore"))
        }
    }
}

extension ScreenshotNotice {
    /// 手机屏上截图、裁剪之后的提示
    var text: StatusText {
        switch self {
        case .cropped: StatusText(headline: Lang.tr("已复制选区", "Selection copied"), detail: nil)
        case .needsPermission: StatusText(headline: Lang.tr("需要\(PermissionName.screenRecording)权限", "\(PermissionName.screenRecording) permission required"), detail: Lang.tr("请在 Mac 上的 Snug 设置中授权", "Grant it in Snug's settings on this Mac"))
        case .windowGone: StatusText(headline: Lang.tr("窗口已关闭", "The window was closed"), detail: Lang.tr("未截图", "No screenshot taken"))
        case .failed: StatusText(headline: Lang.tr("未能截图", "Couldn't take the screenshot"), detail: Lang.tr("请稍后再试", "Please try again later"))
        case .copyFailed: StatusText(headline: Lang.tr("未能写入剪贴板", "Couldn't write to the clipboard"), detail: Lang.tr("请稍后再试", "Please try again later"))
        case .cropFailed: StatusText(headline: Lang.tr("未能裁剪", "Couldn't crop"), detail: Lang.tr("剪贴板中仍是整张截图", "The clipboard still holds the full screenshot"))
        }
    }

    var symbol: String { self == .cropped ? "checkmark.circle.fill" : "exclamationmark.triangle.fill" }
}

extension WindowInfo {
    /// 列表里显示的标题
    var displayTitle: String { title.isEmpty ? Lang.tr("无标题", "Untitled") : title }
}

extension RunningApp {
    /// 开着几个窗口
    var windowSummary: String { windows.isEmpty ? Lang.tr("没有窗口", "No windows") : (Lang.english ? "\(windows.count) window\(windows.count == 1 ? "" : "s")" : "\(windows.count) 个窗口") }
}
