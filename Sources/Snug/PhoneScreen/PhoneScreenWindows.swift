// 手机屏上 Snug 自己的面板：盖满整块屏的 App 资源库，窗口放上来后底部的小横条；
// 开着截图键时还有截图后浮在小横条上方的缩略图或提示，以及裁剪时盖满整块屏的裁剪界面。
// 跟着 PhoneDesk 的状态走：手机屏在才建，列表该露就露、该收就收，屏幕一变（含换档）就重新摆位置。
// 面板不抢焦点（点它不会激活 Snug），所有桌面都在，盖在这块屏的菜单栏之上；
// 只在搜索、选手机型号时临时接收键盘，用完就还回去，手机上的键盘输入平时留给放上来的窗口
import AppKit
import SnugCore
import SnugSystem
import SwiftUI

@MainActor
final class PhoneScreenWindows {
    private let state: AppState
    private let catalog: AppCatalog
    private let pages = PhonePages()
    private let intents: any PhoneIntents
    private var display: DisplayID?
    /// 面板建出来时的界面语言（改语言要重建，见 update）
    private var builtEnglish = false
    private var list: PhonePanel?
    private var homeBar: PhonePanel?
    private var homeBarView: HomeBarView?
    /// 截图后浮在小横条上方的那一块（缩略图或提示），和它此刻显示的是什么
    private var overlay: PhonePanel?
    private var overlayShown: OverlayShown?
    private var cropPanel: PhonePanel?
    private var screenObserver: (any NSObjectProtocol)?

    init(state: AppState, catalog: AppCatalog, intents: any PhoneIntents) {
        self.state = state
        self.catalog = catalog
        self.intents = intents
        screenObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.update() }
        }
        follow()
    }

    private func follow() {
        withObservationTracking {
            _ = state.phone
            _ = state.english
            _ = state.streamedDisplay
            _ = state.screenshotOverlay
            _ = state.config.screenshot
            _ = pages.active
        } onChange: { [weak self] in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    self?.update()
                    self?.follow()
                }
            }
        }
    }

    private func update() {
        let phone = state.phone
        guard let id = phone.display, let screen = Screens.screen(for: id) else { return tearDown() }
        if id != display {
            tearDown()
            build(on: id)
        } else if builtEnglish != state.english {
            // 语言变了：面板里的文案是建时定的，拆掉重建才换得过来（和换屏同一条路）
            tearDown()
            build(on: id)
        }
        guard let list, let homeBar else { return }
        list.setFrame(screen.frame, display: true)
        homeBar.setFrame(homeBarFrame(on: screen), display: true)
        homeBarView?.scale = ScreenFit.scale(screenWidth: screen.frame.width)
        homeBarView?.showsCamera = state.config.screenshot

        let showList = phone.listVisible || phone.placed == nil
        // 手机在看这块屏。不知道 向日葵 在推哪块屏（推流日志没读到）时当作在看
        let watching = state.streamedDisplay == nil || state.phoneInUse
        if showList { list.orderFrontRegardless() } else { list.orderOut(nil) }
        // 手机离开了就关掉搜索、手机型号页，交还键盘：不然在主屏上打的字会进到看不见的输入框
        if !showList || !watching { pages.close() }
        // 只在打开页面那一下拿键盘：之后用户点了别的窗口，不能每次状态一变就抢回来
        let entering = pages.active && !list.acceptsKeys
        list.acceptsKeys = pages.active
        if entering { list.makeKey() }
        // 列表每轮要把所有 App 的窗口读一遍，还和搬窗口共用一条队：手机没在看手机屏就不刷
        if showList, watching {
            catalog.startRefreshing()
        } else {
            catalog.stopRefreshing()
        }
        catalog.setPaused(phone.work != nil)
        let cropping = state.screenshotOverlay?.isCropping == true
        if phone.placed != nil, !showList, !cropping { homeBar.orderFrontRegardless() } else { homeBar.orderOut(nil) }
        updateOverlay(on: screen, hidden: showList)
        updateCrop(on: screen)
    }

    /// 截图后的浮层：缩略图靠左、正在相机键上方（点它进裁剪），提示居中（不拦点击）。
    /// 面板只有内容那么大，不挡窗口底部别的地方；回到 App 资源库、裁剪时不显示。
    /// 不显示时连面板一起拆掉：缩略图拿着解码后的整张截图，不留着占内存
    private func updateOverlay(on screen: NSScreen, hidden: Bool) {
        guard !hidden, let shot = state.screenshotOverlay, !shot.isCropping else {
            overlay?.orderOut(nil)
            overlay = nil
            overlayShown = nil
            return
        }
        let scale = ScreenFit.scale(screenWidth: screen.frame.width)
        let panel = overlay ?? PhonePanel(content: NSView(), opaque: false)
        overlay = panel
        let preview = if case .preview = shot { true } else { false }
        let shown = OverlayShown(overlay: shot, scale: scale)
        if shown != overlayShown {
            overlayShown = shown
            if case .preview(let image) = shot {
                panel.contentView = FirstMouseHostingView(rootView: ScreenshotThumbnail(
                    image: NSImage(data: image.png) ?? NSImage(), scale: scale) { [weak self] in self?.intents.openCrop() })
            } else if case .notice(let notice) = shot {
                panel.contentView = NSHostingView(rootView: ScreenshotNoticeView(notice: notice, scale: scale))
            }
            panel.ignoresMouseEvents = !preview
        }
        let size = panel.contentView?.fittingSize ?? .zero
        let bar = homeBarFrame(on: screen)
        let x = preview ? bar.minX + 12 * scale : bar.midX - size.width / 2
        panel.setFrame(NSRect(origin: CGPoint(x: x, y: bar.maxY + 8 * scale), size: size), display: true)
        panel.orderFrontRegardless()
    }

    /// 裁剪界面：盖满手机屏。进裁剪时建（图片只解码这一次），出裁剪就拆
    private func updateCrop(on screen: NSScreen) {
        guard case .cropping(let image) = state.screenshotOverlay else {
            cropPanel?.orderOut(nil)
            cropPanel = nil
            return
        }
        let panel = cropPanel ?? PhonePanel(content: FirstMouseHostingView(rootView: CropView(
            image: NSImage(data: image.png) ?? NSImage(), pixelSize: image.size,
            onCancel: { [weak self] in self?.intents.cancelCrop() },
            onDone: { [weak self] selection in self?.intents.finishCrop(selection) })))
        cropPanel = panel
        panel.setFrame(screen.frame, display: true)
        panel.orderFrontRegardless()
    }

    private func build(on id: DisplayID) {
        display = id
        builtEnglish = state.english
        list = PhonePanel(content: FirstMouseHostingView(rootView: PhoneHomeView(state: state, catalog: catalog, pages: pages, intents: intents)))
        let bar = HomeBarView()
        bar.onActivate = { [weak self] in self?.intents.showList() }
        bar.onCapture = { [weak self] in self?.intents.takeScreenshot() }
        homeBar = PhonePanel(content: bar)
        homeBarView = bar
    }

    private func tearDown() {
        catalog.reset()
        pages.close()
        list?.orderOut(nil)
        homeBar?.orderOut(nil)
        overlay?.orderOut(nil)
        cropPanel?.orderOut(nil)
        list = nil
        homeBar = nil
        homeBarView = nil
        overlay = nil
        overlayShown = nil
        cropPanel = nil
        display = nil
    }

    /// 可见区底部那一条：放上来的窗口正好让出这一条（高度跟着档位缩放，见 ScreenFit.homeBarHeight）
    private func homeBarFrame(on screen: NSScreen) -> NSRect {
        let visible = screen.visibleFrame
        return NSRect(x: visible.minX, y: visible.minY, width: visible.width,
                      height: ScreenFit.homeBarHeight(screenWidth: screen.frame.width))
    }

    /// 浮层上显示的截图结果和当时的倍率：换了才重建视图
    private struct OverlayShown: Equatable {
        let overlay: ScreenshotOverlay
        let scale: CGFloat
    }
}

/// 不抢焦点、所有桌面都在、盖住菜单栏的面板。截图的浮层是圆角卡片，面板透明（opaque 传 false）
final class PhonePanel: NSPanel {
    init(content: NSView, opaque: Bool = true) {
        super.init(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        // 菜单栏是 24，菜单栏右边的图标是 25：再高一级才盖得住
        level = NSWindow.Level(rawValue: NSWindow.Level.statusBar.rawValue + 1)
        collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle, .fullScreenAuxiliary]
        hidesOnDeactivate = false
        isReleasedWhenClosed = false
        hasShadow = false
        isOpaque = opaque
        backgroundColor = opaque ? PhoneStyle.backgroundNSColor : .clear
        contentView = content
    }

    /// 平时点面板不让它变成键盘焦点：手机上的键盘输入要留给放上来的窗口；搜索、选手机型号时才打开
    var acceptsKeys = false {
        didSet {
            // 交还键盘：收起再摆回去才会让出焦点（resignKey 只是通知，调了也还是焦点）
            guard !acceptsKeys, isKeyWindow, isVisible else { return }
            orderOut(nil)
            orderFrontRegardless()
        }
    }

    override var canBecomeKey: Bool { acceptsKeys }
    override var canBecomeMain: Bool { false }
}

/// 面板不是键盘焦点，第一下点击也要直接生效
final class FirstMouseHostingView<Content: View>: NSHostingView<Content> {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

/// 底部小横条：点一下，或者往上滑，回到 App 资源库。中间是 SF Symbols 的 chevron.compact.up；
/// 开着截图键时左边多一个相机键（camera.viewfinder），点左边这一段是截图。
/// 手机屏换到更宽的一档时跟着放大（scale），在手机上看着、点着、滑着都和基准宽度一样
final class HomeBarView: NSView {
    /// 基准宽度下往上滑过这么多点算数
    static let swipeDistance: CGFloat = 12

    var onActivate: (() -> Void)?
    var onCapture: (() -> Void)?
    /// 这一档比基准宽度宽几倍（ScreenFit.scale）
    var scale: CGFloat = 1 {
        didSet { if scale != oldValue { updateIcons() } }
    }
    /// 设置里开着「截图」
    var showsCamera = false {
        didSet { camera.isHidden = !showsCamera }
    }
    private let grabber = NSImageView()
    private let camera = NSImageView()
    private var swipe: CGFloat = 0

    override init(frame: NSRect) {
        super.init(frame: frame)
        updateIcons()
        grabber.contentTintColor = .secondaryLabelColor
        camera.contentTintColor = .secondaryLabelColor
        camera.imageScaling = .scaleNone
        camera.isHidden = true
        addSubview(camera)
        grabber.translatesAutoresizingMaskIntoConstraints = false
        addSubview(grabber)
        NSLayoutConstraint.activate([
            grabber.centerXAnchor.constraint(equalTo: centerXAnchor),
            grabber.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("不从 nib 加载") }

    private func updateIcons() {
        grabber.image = NSImage(systemSymbolName: "chevron.compact.up", accessibilityDescription: Lang.tr("App 资源库", "App Library"))?
            .withSymbolConfiguration(.init(pointSize: 22 * scale, weight: .semibold))
        camera.image = NSImage(systemSymbolName: "camera.viewfinder", accessibilityDescription: Lang.tr("截图", "Screenshot"))?
            .withSymbolConfiguration(.init(pointSize: 20 * scale, weight: .semibold))
        needsLayout = true
    }

    /// 相机键的点击范围：左边整条高、一个手指宽（图标只有二十几点）。图标摆在它正中
    private var cameraArea: NSRect { NSRect(x: 0, y: 0, width: PhoneStyle.touchTarget * scale, height: bounds.height) }

    override func layout() {
        super.layout()
        camera.frame = cameraArea
    }

    override var isOpaque: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    /// 图标只是装饰，点哪儿都由横条自己接：子视图不接面板的第一下点击（acceptsFirstMouse 默认 false）
    override func hitTest(_ point: NSPoint) -> NSView? {
        super.hitTest(point) == nil ? nil : self
    }

    override func draw(_ dirtyRect: NSRect) {
        PhoneStyle.backgroundNSColor.setFill()
        bounds.fill()
    }

    /// 接住按下，抬起才会送到这里
    override func mouseDown(with event: NSEvent) {}

    override func mouseUp(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        guard bounds.contains(point) else { return }
        if showsCamera, cameraArea.contains(point) { onCapture?() } else { onActivate?() }
    }

    /// 手势翻译把单指上滑变成滚动：手指往上走，滚动量是负的
    override func scrollWheel(with event: NSEvent) {
        if event.phase == .began { swipe = 0 }
        swipe += event.scrollingDeltaY
        if swipe <= -Self.swipeDistance * scale {
            swipe = 0
            onActivate?()
        }
    }
}
