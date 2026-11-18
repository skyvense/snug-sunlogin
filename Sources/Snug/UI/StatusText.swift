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
        if s.quitting { return "正在退出" }
        if !s.accessibilityTrusted { return "需要\(PermissionName.accessibility)权限" }
        switch s.connection {
        case .idle: return "等待连接"
        case .reconnecting: return "连接中断"
        case .connected: return s.controllers.isEmpty ? "远程中" : "\(s.controllers.joined(separator: "、")) · 远程中"
        }
    }

    @MainActor
    private static func detail(_ s: AppState) -> String? {
        if s.quitting { return "正在还原窗口" }
        if !s.accessibilityTrusted { return "用于移动窗口与转换手势" }
        if !s.config.phoneScreen { return "手机屏已关闭" }
        if s.connection == .idle { return nil }
        if s.gesturesFailed { return "手势转换未启用" }
        switch s.phone.screen {
        case .preparing: return "正在准备手机屏"
        case .failed: return "手机屏创建失败"
        case .ready:
            guard s.phoneInUse else { return "手机屏已就绪" }
            return s.phone.placed.map { "手机屏 · \($0.window.app)" } ?? "手机屏使用中"
        case .off:
            if s.phone.dismissed { return "本次远程已退出手机屏" }
            return ControllerKind.wantsPhoneScreen(s.controllers) ? nil : "电脑控制端，不提供手机屏"
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
        case .needsApproval: StatusText(headline: "开机启动待允许", detail: "系统设置 › 通用 › 登录项")
        case .notInApplications: StatusText(headline: "开机启动未生效", detail: "需位于「应用程序」")
        case .failed: StatusText(headline: "开机启动未生效", detail: "请稍后重试")
        }
    }
}

extension PhoneChoice {
    /// 名字：机型全名，自己填的叫「自定义」（分辨率另外写，见 sizeText）
    var title: String { model ?? "自定义" }
    /// 自己填的分辨率：打开自定义输入时先填上它
    var customScreen: PhoneScreen? { model == nil ? screen : nil }
}

extension PhoneScreen {
    var sizeText: String { "\(width) × \(height)" }
    /// 自定义输入下面的说明（设置窗口、手机屏共用）
    static let customHint = "屏幕分辨率或比例，如 1080 × 2400、9 × 20"
}

extension PhoneDesk.Notice {
    /// 手机屏列表顶部的提示：出了什么事，和接下来怎么办。不出现技术词
    var text: StatusText {
        switch self {
        case .windowGone(let app): StatusText(headline: "\(app) 窗口已关闭", detail: "请另选一个窗口")
        case .windowUnresponsive(let app): StatusText(headline: "\(app) 无响应", detail: "可能正忙，请稍后再试")
        case .recordUnwritable: StatusText(headline: "无法保存原位置", detail: "为确保能放回原处，窗口未移动")
        case .screenNotReady: StatusText(headline: "手机屏未就绪", detail: "屏幕正在调整，请稍后再试")
        case .openFailed(let app): StatusText(headline: "无法打开 \(app)", detail: "请稍后再试")
        case .noNewWindow(let app): StatusText(headline: "\(app) 无法新建窗口", detail: "可点 App 打开已有窗口")
        case .noWindow(let app): StatusText(headline: "未找到 \(app) 的窗口", detail: "App 已打开，但没有可显示的窗口")
        case .quitStalled(let app, _): StatusText(headline: "\(app) 未退出", detail: "强制退出将丢失未存储的内容")
        case .reshaped: StatusText(headline: "屏幕比例已调整", detail: "在 向日葵 中切到其他屏幕再切回，点按位置才准确")
        case .reshapeFailed: StatusText(headline: "屏幕比例未能调整", detail: "下次连接时生效")
        case .zoomLeftBehind(let app, let steps): StatusText(headline: "\(app) 窗口已关闭", detail: "再打开时字号若偏大，按 \(steps) 次 ⌘- 复原")
        }
    }
}

extension ScreenshotNotice {
    /// 手机屏上截图、裁剪之后的提示
    var text: StatusText {
        switch self {
        case .cropped: StatusText(headline: "已复制选区", detail: nil)
        case .needsPermission: StatusText(headline: "需要\(PermissionName.screenRecording)权限", detail: "请在 Mac 上的 Snug 设置中授权")
        case .windowGone: StatusText(headline: "窗口已关闭", detail: "未截图")
        case .failed: StatusText(headline: "未能截图", detail: "请稍后再试")
        case .copyFailed: StatusText(headline: "未能写入剪贴板", detail: "请稍后再试")
        case .cropFailed: StatusText(headline: "未能裁剪", detail: "剪贴板中仍是整张截图")
        }
    }

    var symbol: String { self == .cropped ? "checkmark.circle.fill" : "exclamationmark.triangle.fill" }
}

extension WindowInfo {
    /// 列表里显示的标题
    var displayTitle: String { title.isEmpty ? "无标题" : title }
}

extension RunningApp {
    /// 开着几个窗口
    var windowSummary: String { windows.isEmpty ? "没有窗口" : "\(windows.count) 个窗口" }
}
