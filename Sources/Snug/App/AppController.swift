// AppController：把 向日葵 的观测、会话层和手机屏接在一起。
// 每 0.5 秒一拍：感知器给出观测 → 会话层判断连没连、是不是同一次远程 → 算出「要不要手机屏」交给 PhoneDesk。
// 手机屏在时开手势翻译，范围跟着手机屏走。全仓唯一写配置的地方；界面只读 state，意图见 AppController+Intents
import AppKit
import SnugCore
import SnugSystem

/// 显式标 Sendable（主线程隔离的类本来就是）：意图协议要求 Sendable，而遵循写在另一个文件里，
/// 不在这里标的话编译器会警告「Sendable 须在类所在的文件里声明」
@MainActor
final class AppController: Sendable {
    static let tickInterval: TimeInterval = 0.5
    /// 每隔这么久核对一次手机屏上的窗口还在不在
    static let verifyInterval: TimeInterval = 2

    let state = AppState()
    let updater: Updater
    let desk: PhoneDesk
    /// 手机屏 App 资源库的统一数据源。界面碰得到的系统入口只有 desk 和它
    let catalog: AppCatalog
    /// 手机屏上的截图（小横条的相机键）
    let screenshots = Screenshots(system: ScreenshotService())
    /// 用户从菜单或设置里点「退出」（由 AppDelegate 接到退出流程上）
    var onQuitRequested: (() -> Void)?

    let store: ConfigStore
    /// --preview-phone：不等手机连上就要手机屏
    private let previewPhone: Bool
    private let sensor = SunloginSensor()
    private var session = RemoteSession()
    private let touch = TouchTap()
    private var watcher: DisplayWatcher?
    private var tickTimer: Timer?
    private var lastTickAt: Date?
    private var lastVerifyAt = Date.distantPast
    /// 单调时钟的起点：系统改时间不会让「已经过了多久」算错
    private let anchor = (date: Date(), uptime: ProcessInfo.processInfo.systemUptime)

    init(paths: SnugPaths, previewPhone: Bool = false) {
        self.previewPhone = previewPhone
        store = ConfigStore(url: paths.config)
        let phones = Self.loadPhones()
        // 选过的机型按机型库现在的数值来（机型库改过数值时跟着改），建屏前就定下来
        let phone = phones.resolve(store.config.phone)
        let helper = Bundle.main.executableURL ?? URL(fileURLWithPath: CommandLine.arguments[0])
        let windows = WindowService()
        let apps = AppService()
        desk = PhoneDesk(displays: PhoneDisplayService(helper: helper), windows: windows, apps: apps, paths: paths,
                         defaultZoom: store.config.zoomSteps, fit: ScreenFit(phone: phone.screen))
        catalog = AppCatalog(apps: apps, windows: windows)
        state.config = store.config
        state.phones = phones
        state.accessibilityTrusted = AX.isTrusted
        updater = Updater(state: state)
        AX.configureTimeout()
        if phone != store.config.phone {
            let old = store.config.phone.screen
            changeConfig("手机型号 \(phone.title) 按机型库更新：\(old.sizeText) → \(phone.screen.sizeText)") { $0.phone = phone }
        }
    }

    var now: Date { anchor.date.addingTimeInterval(ProcessInfo.processInfo.systemUptime - anchor.uptime) }

    /// 机型库随 App 发（Contents/Resources）。读不出来只是没有型号可选，还能自己填分辨率，手机屏照常按配置里的比例
    private static func loadPhones() -> PhoneCatalog {
        guard let url = Bundle.main.url(forResource: "phone-models", withExtension: "json") else {
            Log.error("App 里没有机型库 phone-models.json（打包漏了），手机型号只能自己填")
            return .empty
        }
        do {
            let phones = try PhoneCatalog(contentsOf: url)
            Log.info("机型库：\(phones.brands.count) 个品牌、\(phones.models.count) 款机型")
            return phones
        } catch {
            Log.error("机型库读不出来（\(url.path)）：\(error)；手机型号只能自己填")
            return .empty
        }
    }

    /// 开始监控。有权限的话先把上次没放回去的窗口放回去
    func start() {
        desk.onChange = { [weak self] phone in self?.phoneChanged(phone) }
        desk.onGone = { [weak self] gone in self?.catalog.forget(gone) }
        touch.onCloseWindow = { [weak self] window in self?.desk.closePlaced(window: window) }
        screenshots.onChange = { [weak self] overlay in self?.screenshotChanged(overlay) }
        desk.onTopologyChange = { [weak self] in
            guard let self else { return }
            session.noteSelfInducedChange(at: now)
        }
        let watcher = DisplayWatcher { [weak self] in
            self?.desk.displaysChanged()
            self?.syncGestures()
        }
        watcher.start()
        self.watcher = watcher
        sensor.start()
        if state.accessibilityTrusted { desk.recover() }
        let timer = Timer(timeInterval: Self.tickInterval, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tick() }
        }
        // 加进 .common：菜单展开、窗口拖动期间也照常走
        RunLoop.main.add(timer, forMode: .common)
        tickTimer = timer
        tick()
        Log.info("开始监控（手机屏\(store.config.phoneScreen ? "已开" : "已关")，手势\(store.config.gestures ? "已开" : "已关")"
                 + "\(previewPhone ? "，预览模式：不等手机直接加手机屏" : "")）")
    }

    /// 停止监控（退出收尾前调用）
    func stopMonitoring() {
        tickTimer?.invalidate()
        tickTimer = nil
        watcher?.stop()
        sensor.stop()
    }

    // MARK: 每拍

    private func tick() {
        let now = now
        if let last = lastTickAt, now.timeIntervalSince(last) > 1.5 {
            Log.warn(String(format: "主线程卡了 %.1fs", now.timeIntervalSince(last)))
        }
        lastTickAt = now
        refreshTrust()
        screenshots.expire()
        let sample = sensor.sample(now: Date())
        let transitions = session.update(sample.signals, now: now)
        for t in transitions { Log.info("会话：\(Self.describe(t))") }
        // 远程结束就是不要手机屏了。下一次在同一拍里开始时 wantsPhoneScreen 前后都是真，不先说一声「不要」，
        // 上一次在手机上退出的手机屏会带进这一次。同一拍里先「不要」再「要」不会撤屏重建（PhoneDesk 排队时读的是最新结论）
        if transitions.contains(.ended) { desk.setWanted(false) }
        desk.setWanted(wantsPhoneScreen)
        desk.setRemote(session.inEpisode)
        if now.timeIntervalSince(lastVerifyAt) >= Self.verifyInterval {
            lastVerifyAt = now
            desk.verifyPlacement()
            desk.checkReturns()
            desk.settleDisplays()
            refreshScreenRecording()
        }
        state.update(\.connection, session.isConnected ? .connected : (session.inEpisode ? .reconnecting : .idle))
        state.update(\.controllers, session.controllers)
        // 在推哪块屏只能按一路推流推断：几台同时连着时各推各的、日志混在一起，当作不知道（手机屏上的列表照旧刷新）
        state.update(\.streamedDisplay, session.controllers.count > 1 ? nil : sample.streamingDisplay)
    }

    /// 要不要手机屏：开着总开关、有权限，这次远程的控制端里有不是电脑的（见 RemoteSession.wantsPhoneScreen）
    private var wantsPhoneScreen: Bool {
        guard store.config.phoneScreen, state.accessibilityTrusted else { return false }
        return previewPhone || session.wantsPhoneScreen(now: now)
    }

    /// 权限是用户在系统设置里给的，随时会变：刚给上时把上次没放回去的窗口放回去
    private func refreshTrust() {
        let trusted = AX.isTrusted
        guard trusted != state.accessibilityTrusted else { return }
        state.accessibilityTrusted = trusted
        Log.info("辅助功能权限：\(trusted ? "已授权" : "被收回")")
        if trusted { desk.recover() }
    }

    /// 录屏权限也是用户在系统设置里给的：开着截图键时每两秒对一次（不跟着每拍去读 TCC），变了记一笔
    func refreshScreenRecording() {
        guard store.config.screenshot else { return }
        let trusted = screenshots.trusted
        guard trusted != state.screenRecordingTrusted else { return }
        state.screenRecordingTrusted = trusted
        Log.info("录屏权限：\(trusted ? "已授权" : "未授权")")
    }

    // MARK: 手机屏

    private func phoneChanged(_ phone: PhoneDesk.State) {
        state.phone = phone
        // 手机屏没了：浮着的截图、裁剪中的一并收掉，不然下次连上还盖着旧截图
        if phone.display == nil { screenshots.dismiss() }
        syncGestures()
    }

    private func screenshotChanged(_ overlay: ScreenshotOverlay?) {
        state.screenshotOverlay = overlay
        syncGestures()
    }

    /// 手势翻译只在手机屏在、开着手势、并且没在裁剪截图时运行（裁剪时的拖动是在画选区，不能翻译成滚动）；
    /// 范围跟着手机屏（换档、屏幕重排时 DisplayWatcher 也会调到这里）
    func syncGestures() {
        // 窗口露着时才拦它的关闭按钮：列表盖着时那个位置是列表
        let closeButton = state.phone.listVisible ? nil : state.phone.closeButton
        let translating = store.config.gestures && state.screenshotOverlay?.isCropping != true
        let region = translating
            ? state.phone.display.map { TouchRegion(phoneScreen: CGDisplayBounds($0), closeButton: closeButton) }
            : nil
        state.update(\.gesturesFailed, !touch.setRegion(region))
    }

    static func describe(_ t: SessionTransition) -> String {
        switch t {
        case .began(let c): "开始（控制端 \(names(c))）"
        case .resumed(let c): "重连上，仍是同一次远程（控制端 \(names(c))）"
        case .controllersChanged(let c): "控制端变为 \(names(c))"
        case .interrupted: "断开，等 \(Int(SessionEpisode.reconnectWindow))s 看会不会重连"
        case .ended: "结束"
        }
    }

    private static func names(_ controllers: [String]) -> String {
        controllers.isEmpty ? "读不到名字" : controllers.joined(separator: "、")
    }
}
