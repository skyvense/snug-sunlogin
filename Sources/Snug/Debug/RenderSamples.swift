// --render-* 用的样例状态。窗口标题和设备名都是通用的，不带任何个人信息
import SwiftUI
import SnugCore

@MainActor
struct RenderSample {
    let name: String
    var dark = false
    /// nil：按内容自适应
    var size: CGSize?
    let view: () -> AnyView
}

@MainActor
enum RenderSamples {
    private static let stub = RenderStub()

    static func samples(for target: RenderTarget) -> [RenderSample] {
        switch target {
        case .settings: settings
        case .welcome: welcome
        case .about: about
        case .legacy: [RenderSample(name: "legacy") { AnyView(LegacyNoticeView(quitLegacy: {}, keepBoth: {})) }]
        case .phone: phone
        case .update: update
        }
    }

    // MARK: 设置

    private static var settings: [RenderSample] {
        [
            setting("settings-idle") { _ in },
            setting("settings-ready") { s in
                connected(s)
            },
            setting("settings-in-use-dark", dark: true) { s in
                connected(s)
                s.streamedDisplay = 14
                s.phone.placed = PhoneDesk.Placed(window: windows[0], zoom: 2)
            },
            setting("settings-custom-phone") { s in
                connected(s)
                s.config.phone = PhoneChoice(model: nil, screen: PhoneScreen(width: 1080, height: 2400))
            },
            setting("settings-screenshot") { s in
                connected(s)
                s.config.screenshot = true
            },
            setting("settings-screenshot-granted-dark", dark: true) { s in
                s.config.screenshot = true
                s.screenRecordingTrusted = true
            },
            setting("settings-update-available") { s in s.update.check = .available(sampleFeed) },
            setting("settings-update-too-old") { s in s.update.problem = .tooOld },
            setting("settings-trouble") { s in
                s.accessibilityTrusted = false
                s.legacyAppRunning = true
                s.configSaveFailed = true
                s.loginItemIssue = .needsApproval
            },
        ]
    }

    private static func setting(_ name: String, dark: Bool = false, _ configure: @escaping @MainActor (AppState) -> Void) -> RenderSample {
        RenderSample(name: name, dark: dark) {
            let s = AppState()
            s.accessibilityTrusted = true
            s.launchAtLogin = true
            s.phones = phones
            configure(s)
            return AnyView(SettingsView(state: s, intents: stub, navigation: stub))
        }
    }

    private static func connected(_ s: AppState) {
        s.connection = .connected
        s.controllers = ["iPhone 17 Pro Max"]
        s.phone.screen = .ready(14)
    }

    // MARK: 关于与更新

    private static let sampleCurrent = Version(major: 2, minor: 1, patch: 0)

    /// 样例的 update.json：比当前新两版
    private static let sampleFeed: UpdateFeed = {
        let note = { (patch: Int, date: String, message: String) in
            ReleaseNote(version: Version(major: 2, minor: 1, patch: patch), date: date, tagMessage: message)
        }
        let package = UpdatePackage(file: "Snug-2.1.2.pack", chunks: [], files: [])
        return UpdateFeed(version: Version(major: 2, minor: 1, patch: 2), releases: [
            note(2, "2026-12-08", """
                Snug 2.1.2：窗口列表按最近使用排序

                改进
                - 窗口列表按最近使用排序，常用的窗口排在前面
                - 「正在打开」提示出现得更快

                修复
                - 个别 App 放上手机屏后字号没有放大
                """),
            note(1, "2026-11-28", """
                Snug 2.1.1：修复更新后偶尔显示未授权

                修复
                - 更新后个别情况下辅助功能显示为未授权，实际仍有效

                感谢社区用户反馈。
                """),
            note(0, "2026-11-20", "Snug 2.1.0：应用内更新"),
        ], package: package)
    }()

    private static var about: [RenderSample] {
        [
            aboutSample("about") { $0.check = .upToDate },
            aboutSample("about-dark", dark: true) { $0.check = .upToDate },
            aboutSample("about-available") { $0.check = .available(sampleFeed) },
            aboutSample("about-downloading-dark", dark: true) { u in
                u.check = .available(sampleFeed)
                u.phase = .downloading(0.42)
            },
            aboutSample("about-unreachable") { $0.problem = .unreachable(.github) },
            aboutSample("about-cannot-replace") { u in
                u.check = .available(sampleFeed)
                u.problem = .cannotReplace
            },
            aboutSample("about-too-old") { $0.problem = .tooOld },
        ]
    }

    private static func aboutSample(_ name: String, dark: Bool = false, _ configure: @escaping (inout UpdateState) -> Void) -> RenderSample {
        RenderSample(name: name, dark: dark) {
            let s = AppState()
            configure(&s.update)
            return AnyView(AboutView(state: s, intents: stub, navigation: stub))
        }
    }

    private static var update: [RenderSample] {
        [
            updateSample("update") { _ in },
            updateSample("update-dark", dark: true) { _ in },
            updateSample("update-downloading") { $0.phase = .downloading(0.62) },
            updateSample("update-installing-dark", dark: true) { $0.phase = .installing },
            updateSample("update-corrupt") { $0.problem = .corrupt },
            updateSample("update-cannot-replace") { $0.problem = .cannotReplace },
            updateSample("update-one-behind", current: Version(major: 2, minor: 1, patch: 1)) { _ in },
        ]
    }

    private static func updateSample(_ name: String, dark: Bool = false, current: Version = sampleCurrent,
                                     _ configure: @escaping (inout UpdateState) -> Void) -> RenderSample {
        RenderSample(name: name, dark: dark) {
            let s = AppState()
            s.update.check = .available(sampleFeed)
            configure(&s.update)
            return AnyView(UpdateView(state: s, current: current, intents: stub, close: {}))
        }
    }

    // MARK: 欢迎页

    private static var welcome: [RenderSample] {
        [false, true].map { granted in
            RenderSample(name: granted ? "welcome-granted" : "welcome") {
                let s = AppState()
                s.accessibilityTrusted = granted
                return AnyView(WelcomeView(state: s, onDone: {}))
            }
        }
    }

    // MARK: 手机屏

    static let windows = [
        WindowInfo(key: WindowKey(pid: 1, id: 1), app: "Ghostty", bundleID: "com.mitchellh.ghostty", title: "~/projects/snug — zsh"),
        WindowInfo(key: WindowKey(pid: 2, id: 2), app: "Safari", bundleID: "com.apple.Safari", title: "Swift 文档 — Concurrency"),
        WindowInfo(key: WindowKey(pid: 3, id: 3), app: "备忘录", bundleID: "com.apple.Notes", title: "周末计划", minimized: true),
        WindowInfo(key: WindowKey(pid: 5, id: 5), app: "终端", bundleID: "com.apple.Terminal", title: "ssh build-server"),
        WindowInfo(key: WindowKey(pid: 1, id: 7), app: "Ghostty", bundleID: "com.mitchellh.ghostty", title: "~ — htop"),
    ]

    private static func app(_ bundleID: String, _ name: String) -> AppInfo { AppInfo(bundleID: bundleID, name: name, path: nil) }

    private static let installed: [InstalledApp] = [
        (app("com.apple.AppStore", "App Store"), "App Store"), (app("com.apple.calculator", "计算器"), "Calculator"),
        (app("com.apple.iCal", "日历"), "Calendar"), (app("com.apple.finder", "访达"), "Finder"),
        (app("com.mitchellh.ghostty", "Ghostty"), "Ghostty"), (app("com.apple.mail", "邮件"), "Mail"),
        (app("com.apple.Maps", "地图"), "Maps"), (app("com.apple.Music", "音乐"), "Music"),
        (app("com.apple.Notes", "备忘录"), "Notes"), (app("com.apple.Photos", "照片"), "Photos"),
        (app("com.apple.Preview", "预览"), "Preview"), (app("com.apple.reminders", "提醒事项"), "Reminders"),
        (app("com.apple.Safari", "Safari"), "Safari"), (app("com.apple.systempreferences", "系统设置"), "System Settings"),
        (app("com.apple.Terminal", "终端"), "Terminal"), (app("com.apple.TextEdit", "文本编辑"), "TextEdit"),
    ].map { InstalledApp($0.0, english: $0.1) }

    private static let running = [
        RunningApp(pid: 1, app: app("com.mitchellh.ghostty", "Ghostty"), windows: [windows[0], windows[4]]),
        RunningApp(pid: 2, app: app("com.apple.Safari", "Safari"), windows: [windows[1]]),
        RunningApp(pid: 3, app: app("com.apple.Notes", "备忘录"), windows: [windows[2]]),
        RunningApp(pid: 4, app: app("com.apple.finder", "访达"), windows: []),
        RunningApp(pid: 5, app: app("com.apple.Terminal", "终端"), windows: [windows[3]]),
    ]

    /// 样例机型库：几个品牌各几台（真的机型库在 Assets/phone-models.json）
    private static let phones: PhoneCatalog = {
        let json = """
        {"brands": [
          {"name": "Apple", "models": [
            {"name": "iPhone 17 Pro Max", "year": 2025, "screen": [1320, 2868], "points": 440},
            {"name": "iPhone 17 Pro", "year": 2025, "screen": [1206, 2622], "points": 402},
            {"name": "iPhone 17", "year": 2025, "screen": [1206, 2622], "points": 402},
            {"name": "iPhone Air", "year": 2025, "screen": [1260, 2736], "points": 420}
          ]},
          {"name": "华为", "aliases": ["HUAWEI"], "models": [
            {"name": "Mate 70 Pro", "year": 2024, "screen": [1260, 2832]},
            {"name": "Pura 80 Ultra", "year": 2025, "screen": [1276, 2848]}
          ]},
          {"name": "小米", "aliases": ["Xiaomi"], "models": [
            {"name": "17 Pro Max", "year": 2025, "screen": [1200, 2608]},
            {"name": "15 Ultra", "year": 2025, "screen": [1440, 3200]}
          ]}
        ]}
        """
        return (try? PhoneCatalog(json: Data(json.utf8))) ?? .empty
    }()

    private static var phone: [RenderSample] {
        let placed = PhoneDesk.Placed(window: windows[0], zoom: 2)
        return [
            phoneSample("phone-list") { _, _ in },
            phoneSample("phone-placed") { s, _ in s.phone.placed = placed },
            phoneSample("phone-placed-dark", dark: true) { s, _ in s.phone.placed = placed },
            phoneSample("phone-opening") { s, _ in s.phone.work = .opening(running[2].app) },
            phoneSample("phone-placing") { s, _ in s.phone.work = .placing(windows[1]) },
            phoneSample("phone-quitting") { s, _ in s.phone.quitting = [2] },
            phoneSample("phone-confirm-quit", confirming: .quit(2)) { _, _ in },
            phoneSample("phone-confirm-close", confirming: .close(windows[4].key)) { s, _ in s.phone.placed = placed },
            phoneSample("phone-confirm-dismiss", confirming: .dismissScreen) { s, _ in s.phone.placed = placed },
            phoneSample("phone-dismissing-dark", dark: true) { s, _ in s.phone.work = .dismissing },
            phoneSample("phone-closing-dark", dark: true) { s, _ in
                s.phone.placed = placed
                s.phone.closing = [windows[4].key]
            },
            phoneSample("phone-notice") { s, _ in
                s.phone.placed = placed
                s.phone.notice = .windowUnresponsive(app: windows[1].app)
            },
            phoneSample("phone-quit-stalled-dark", dark: true) { s, _ in s.phone.notice = .quitStalled(app: "文本编辑", pid: 9) },
            phoneSample("phone-all", tab: .all) { _, _ in },
            phoneSample("phone-all-dark", dark: true, tab: .all) { s, _ in s.phone.placed = placed },
            phoneSample("phone-search") { _, pages in pages.open(.search) },
            phoneSample("phone-search-results") { _, pages in
                pages.open(.search)
                pages.query = "s"
            },
            phoneSample("phone-search-none-dark", dark: true) { _, pages in
                pages.open(.search)
                pages.query = "xyz"
            },
            phoneSample("phone-model") { _, pages in pages.open(.phoneModel) },
            phoneSample("phone-model-custom-dark", dark: true) { s, pages in
                s.config.phone = PhoneChoice(model: nil, screen: PhoneScreen(width: 1080, height: 2400))
                pages.open(.phoneModel)
            },
            phoneSample("phone-model-search") { _, pages in
                pages.open(.phoneModel)
                pages.query = "17 pro"
            },
            phoneSample("phone-model-none-dark", dark: true) { _, pages in
                pages.open(.phoneModel)
                pages.query = "nokia"
            },
            phoneSample("phone-reshaping") { s, _ in
                s.config.phone = PhoneChoice(phones.models[4])
                s.phone.work = .reshaping
            },
            phoneSample("phone-reshaped") { s, _ in s.phone.notice = .reshaped },
            phoneSample("phone-empty", running: []) { _, _ in },
            phoneSample("phone-loading", running: [], loaded: false) { _, _ in },
        ] + screenshot
    }

    // MARK: 截图

    private static var screenshot: [RenderSample] {
        let fit = ScreenFit.standard
        let base = fit.size(width: fit.baseWidth)
        let wide = fit.size(width: fit.widths[min(3, fit.widths.count - 1)])
        let crop = { (size: CGSize) in
            AnyView(CropView(image: sampleShot, pixelSize: sampleShotSize, onCancel: {}, onDone: { _ in })
                .frame(width: size.width, height: size.height))
        }
        return [
            RenderSample(name: "phone-screenshot-thumbnail") {
                AnyView(ScreenshotThumbnail(image: sampleShot, scale: 1) {}.padding(12))
            },
            RenderSample(name: "phone-screenshot-notice-dark", dark: true) {
                AnyView(ScreenshotNoticeView(notice: .needsPermission, scale: 1).padding(12))
            },
            RenderSample(name: "phone-screenshot-crop", size: base) { crop(base) },
            RenderSample(name: "phone-screenshot-crop-wide", size: wide) { crop(wide) },
        ]
    }

    private static let sampleShotSize = CGSize(width: 1080, height: 1794)

    /// 一张假的窗口截图：标题栏 + 几行字的灰条
    private static let sampleShot = NSImage(size: sampleShotSize, flipped: true) { rect in
        NSColor.white.setFill()
        rect.fill()
        NSColor(white: 0.92, alpha: 1).setFill()
        NSRect(x: 0, y: 0, width: rect.width, height: 112).fill()
        for (i, color) in [NSColor.systemRed, .systemYellow, .systemGreen].enumerated() {
            color.setFill()
            NSBezierPath(ovalIn: NSRect(x: 40 + CGFloat(i) * 56, y: 38, width: 36, height: 36)).fill()
        }
        NSColor(white: 0.8, alpha: 1).setFill()
        for row in 0..<14 {
            let width = rect.width - 160 - CGFloat(row % 3) * 180
            NSBezierPath(roundedRect: NSRect(x: 80, y: 200 + CGFloat(row) * 110, width: width, height: 36), xRadius: 18, yRadius: 18).fill()
        }
        return true
    }

    private static func phoneSample(_ name: String, dark: Bool = false, tab: PhoneHomeView.Tab = .running,
                                    confirming: PhoneHomeView.Confirm? = nil, running list: [RunningApp] = running, loaded: Bool = true,
                                    _ configure: @escaping @MainActor (AppState, PhonePages) -> Void) -> RenderSample {
        let size = ScreenFit.standard.size(width: ScreenFit.standard.baseWidth)
        return RenderSample(name: name, dark: dark, size: size) {
            let s = AppState()
            s.phone.screen = .ready(14)
            s.phones = phones
            let pages = PhonePages()
            configure(s, pages)
            let catalog = AppCatalog(sample: list, installed: installed, loaded: loaded)
            return AnyView(PhoneHomeView(state: s, catalog: catalog, pages: pages, intents: stub, tab: tab, confirming: confirming)
                .frame(width: size.width, height: size.height))
        }
    }
}
