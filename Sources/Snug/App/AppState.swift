// 发布给界面的状态。界面只读它，改动一律走意图协议（由 AppController 实现）。
// 菜单栏、设置窗口、手机屏上的列表读的是同一份状态：任何入口改了，所有界面一起变
import CoreGraphics
import Observation
import SnugCore

enum ConnectionPhase: Equatable {
    case idle
    case connected
    /// 断开了，还在重连窗口里（这段时间内再连上算同一次远程，手机屏留着）
    case reconnecting
}

@MainActor
@Observable
final class AppState {
    var config = AppConfig()
    /// 机型库（启动时读一次）：设置和手机屏上选手机型号用
    var phones = PhoneCatalog.empty
    var connection = ConnectionPhase.idle
    /// 此刻同时连着的控制端
    var controllers: [String] = []
    /// 向日葵 此刻在推的屏
    var streamedDisplay: DisplayID?
    var phone = PhoneDesk.State()
    /// 手势翻译要开却没开起来（建事件监听失败）
    var gesturesFailed = false
    var accessibilityTrusted = false
    /// 录屏权限：手机屏上的截图要它，和辅助功能分开授权（开了截图键才去读）
    var screenRecordingTrusted = false
    var launchAtLogin = false
    /// 上一次打开开机启动没成
    var loginItemIssue: LoginItemIssue?
    /// 上一次改设置没能写进 config.json
    var configSaveFailed = false
    /// 旧版「UU远程伴侣」正在运行
    var legacyAppRunning = false
    /// 正在退出收尾
    var quitting = false
    /// 应用内更新：查到了什么、进行到哪（Updater 写）
    var update = UpdateState()
    /// 手机屏上浮着的截图界面：缩略图、裁剪或一句结果（Screenshots 写）
    var screenshotOverlay: ScreenshotOverlay?

    init() {}

    /// 向日葵 正在推手机屏，也就是用户此刻正在手机屏上
    var phoneInUse: Bool {
        guard let display = phone.display else { return false }
        return streamedDisplay == display
    }

    /// 只在值真的变了时才写，免得每拍都让界面重算
    func update<T: Equatable>(_ keyPath: ReferenceWritableKeyPath<AppState, T>, _ value: T) {
        if self[keyPath: keyPath] != value { self[keyPath: keyPath] = value }
    }
}

enum LoginItemIssue: Equatable {
    /// 用户在「登录项」里关过 Snug，要在那里允许
    case needsApproval
    /// Snug 不在「应用程序」里
    case notInApplications
    case failed
}

/// 设置窗口、菜单栏能发起的意图。AppController 是唯一实现（渲染 PNG 时用一个什么都不做的替身）。
/// 几个意图协议都标 Sendable：Swift 6.2 把 intents.setPhone 这种方法引用存进视图属性时要求它可跨隔离域传递
@MainActor
protocol AppIntents: AnyObject, Sendable {
    func setPhoneScreen(_ on: Bool)
    func setGestures(_ on: Bool)
    func setZoomSteps(_ steps: Int)
    func setLaunchAtLogin(_ on: Bool)
    /// 开机启动待允许：直接打开系统设置的「登录项」
    func openLoginItemsSettings()
    func markWelcomed()
    func quitLegacyApp()
    func quit()
    func checkForUpdates()
    func installUpdate()
    func setUpdateSource(_ source: UpdateSource)
    /// 自动更新走不通时，打开所选更新源的下载地址
    func openDownloadPage()
    /// 换手机型号：手机屏在就马上换成它的比例
    func setPhone(_ phone: PhoneChoice)
    /// 指示条上的相机键（截图）
    func setScreenshot(_ on: Bool)
}

/// 手机屏上的 App 资源库和小横条能发起的意图
@MainActor
protocol PhoneIntents: AnyObject, Sendable {
    func pull(_ window: WindowInfo)
    /// 点一个 App：在跑且有窗口就放最近用过的那个，否则打开它、等新窗口出来再放（见 AppList.action）
    func open(_ app: AppInfo)
    func newWindow(_ app: RunningApp)
    func quit(_ app: RunningApp)
    func forceQuit(pid: Int32, app: String)
    func close(_ window: WindowInfo)
    func dismissNotice()
    func showList()
    func returnToPlaced()
    func putBack()
    func zoom(by delta: Int)
    func setPhone(_ phone: PhoneChoice)
    /// 退出手机屏：这次远程不再显示（见 PhoneDesk.dismissScreen）
    func dismissScreen()
    /// 截铺满手机屏的窗口到剪贴板（见 Screenshots）
    func takeScreenshot()
    func openCrop()
    func cancelCrop()
    /// selection 是图片里的比例（0…1，见 CropGeometry）
    func finishCrop(_ selection: CGRect)
}

/// 打开各个窗口
@MainActor
protocol Navigation: AnyObject, Sendable {
    func openSettings()
    func openWelcome()
    func openAbout()
    func openUpdate()
}
