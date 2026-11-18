// --probe-screens：按规格建实验屏，打印系统给它们列出的模式，屏上铺校准图，按 Ctrl-C 撤掉（实测见 docs/multi-screen.md 第 7 节）。
// 校准图：四边每 10 点一条线、每 50 点一条黄线，数字是离那条边多远（点）。在手机、平板、电脑上截 向日葵 的画面，
// 看每条边最先露出的是哪条线，就知道 向日葵 压住了多少。九个靶心用来测点得准不准：点下去标出落点，同时打印到终端。
// 切模式照正式版的做法交给帮手子进程（Snug --set-mode）：亲自切过模式的进程，虚拟屏释放后要拖很久才下线。
// main=on 的那块建好后设成主屏（V11），撤之前先按记下的排布摆回去，再释放
import AppKit
import SnugCore
import SnugSystem

@MainActor
final class ScreenProbe: NSObject, NSApplicationDelegate {
    private let screens: [ProbeScreen]
    private var probes: [ProbeDisplay] = []
    private var panels: [DisplayID: NSPanel] = [:]
    private var watcher: DisplayWatcher?
    private var signals: [DispatchSourceSignal] = []
    private var pendingReport: Task<Void, Never>?
    private var shuttingDown = false
    /// 设主屏之前各屏的原点；没设过主屏是 nil
    private var savedOrigins: [DisplayID: CGPoint]?

    init(screens: [ProbeScreen]) { self.screens = screens }

    func applicationDidFinishLaunching(_ notification: Notification) {
        for sig in [SIGINT, SIGTERM] {
            signal(sig, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
            source.setEventHandler { [weak self] in MainActor.assumeIsolated { self?.shutdown() } }
            source.resume()
            signals.append(source)
        }
        Task { await start() }
    }

    private func start() async {
        // 一开始就盯着屏幕变化：后面每建一块屏，系统都可能把前面的屏挪个位置，校准图要跟着挪
        watcher = DisplayWatcher { [weak self] in self?.displaysChanged() }
        watcher?.start()
        say("开始前的屏幕：\(Self.allDisplays())")
        for screen in screens {
            guard let probe = ProbeDisplay(screen) else {
                say("「\(screen.name)」建不出来（serial=\(screen.serial)，同身份的屏可能还在）")
                continue
            }
            probes.append(probe)
            let started = Self.now
            guard await Self.poll(timeout: 6, every: 0.1, until: { probe.isOnline }) else {
                say("#\(probe.id)「\(screen.name)」建好 6 秒没上线（serial=\(screen.serial)）")
                continue
            }
            say(String(format: "#\(probe.id)「\(screen.name)」上线（%.1fs），上线时 \(DisplayMode.describe(probe.id))", Self.now - started))
            // 刚上线时模式还在变，等它定下来再切
            try? await Task.sleep(for: .seconds(0.8))
            let first = screen.modes[0]
            if DisplayMode.hiDPIWidth(probe.id) != first.width {
                let status = await Self.setMode(probe.id, first)
                say("#\(probe.id) 切到 \(first) 的 2 倍：帮手退出码 \(status.map(String.init) ?? "起不来")，现在 \(DisplayMode.describe(probe.id))")
            }
            report(probe)
            if screen.pattern { showPattern(probe) }
        }
        if let main = probes.first(where: { $0.screen.main && $0.isOnline }) {
            let before = DisplayArrangement.origins()
            savedOrigins = before
            let id = main.id
            let err = await Self.offMain { DisplayArrangement.makeMain(id) }
            say("#\(id) 设成主屏：\(err == .success ? "成功" : "失败（CGError=\(err.rawValue)）")，屏幕：\(Self.allDisplays())")
        }
        try? await Task.sleep(for: .seconds(0.5))
        placeAllPatterns()
        say("就绪，屏幕：\(Self.allDisplays())。按 Ctrl-C 撤掉实验屏")
    }

    /// 登记的每一档：系统列没列出它的 2 倍模式、能不能当桌面用；1 倍的同尺寸模式也列一下
    private func report(_ probe: ProbeDisplay) {
        let listed = DisplayMode.listed(probe.id)
        var lines = ["#\(probe.id)「\(probe.screen.name)」serial=\(probe.screen.serial) 像素上限 \(probe.screen.maxPixels)，系统列出 \(listed.count) 个模式"]
        if listed.isEmpty { lines.append("  读不到模式（本进程读到的模式表可能是过期的）") }
        for size in probe.screen.modes {
            let same = listed.filter { $0.mode.width == size.width && $0.mode.height == size.height }
            func verdict(_ scale: Int) -> String {
                guard let hit = same.first(where: { $0.mode.pixelWidth == scale * size.width }) else { return "没列出" }
                return hit.usable ? "桌面可用" : "列出、不能当桌面用"
            }
            lines.append("  \(size)：2 倍 \(verdict(2))；1 倍 \(verdict(1))")
        }
        say(lines.joined(separator: "\n"))
    }

    // MARK: 校准图

    private func showPattern(_ probe: ProbeDisplay) {
        let panel = NSPanel(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.level = NSWindow.Level(rawValue: NSWindow.Level.statusBar.rawValue + 1)
        panel.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle, .fullScreenAuxiliary]
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        panel.backgroundColor = .black
        panel.contentView = ProbePatternView(probe: probe, onTap: { [weak self] point, target in
            self?.say(String(format: "#\(probe.id) 点按 (%.0f, %.0f)，离最近的靶心 (%.0f, %.0f) 差 (%+.0f, %+.0f)",
                             point.x, point.y, target.x, target.y, point.x - target.x, point.y - target.y))
        })
        panels[probe.id] = panel
        panel.orderFrontRegardless()
        placePattern(probe.id)
    }

    /// 摆到那块屏上，并核对窗口确实落在那块屏：落错了打印出来（先上屏再设位置，免得上屏时被系统挪回主屏）
    private func placePattern(_ id: DisplayID) {
        guard let panel = panels[id] else { return }
        guard let screen = Screens.screen(for: id) else {
            say("#\(id) 的校准图：界面还不认得这块屏，等屏幕变化后再摆")
            return
        }
        panel.setFrame(screen.frame, display: true)
        panel.contentView?.needsDisplay = true
        let landed = (panel.screen?.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value
        if landed != id || panel.frame != screen.frame {
            say("#\(id) 的校准图没摆对：想要 \(screen.frame)，实际 \(panel.frame)，落在 #\(landed.map(String.init) ?? "无")")
        }
    }

    private func placeAllPatterns() {
        for probe in probes { placePattern(probe.id) }
    }

    /// 屏幕一变（向日葵 改模式、加撤屏），系统会连着回调好几次：停 0.5 秒再打印一次此刻的样子
    private func displaysChanged() {
        guard !shuttingDown else { return }
        pendingReport?.cancel()
        pendingReport = Task {
            try? await Task.sleep(for: .seconds(0.5))
            guard !Task.isCancelled else { return }
            placeAllPatterns()
            say("屏幕变了：\(Self.allDisplays())")
        }
    }

    // MARK: 撤掉

    private func shutdown() {
        guard !shuttingDown else { return }
        shuttingDown = true
        say("撤掉实验屏…")
        Task {
            panels.values.forEach { $0.close() }
            panels = [:]
            if let saved = savedOrigins {
                let err = await Self.offMain { DisplayArrangement.restore(saved) }
                say("排布摆回设主屏之前：\(err == .success ? "成功" : "失败（CGError=\(err.rawValue)）")，屏幕：\(Self.allDisplays())")
                try? await Task.sleep(for: .seconds(1))
            }
            for probe in probes {
                // 建好不到 2.5 秒就释放会让 WindowServer 卡死（原作者实测）
                let age = Self.now - probe.bornAt
                if age < PhoneScreenSpec.minimumLifetime { try? await Task.sleep(for: .seconds(PhoneScreenSpec.minimumLifetime - age)) }
                probe.release()
            }
            let gone = await Self.poll(timeout: 10, every: 0.1) { self.probes.allSatisfy { !$0.isOnline } }
            say(gone ? "实验屏都已下线，屏幕：\(Self.allDisplays())" : "有实验屏释放 10 秒后仍在线")
            exit(gone ? 0 : 1)
        }
    }

    // MARK: 小工具

    private func say(_ text: String) {
        let stamp = Self.clock.string(from: Date())
        FileHandle.standardOutput.write(Data("\(stamp) \(text)\n".utf8))
    }

    private static let clock: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss.SSS"
        return f
    }()

    private static var now: TimeInterval { ProcessInfo.processInfo.systemUptime }

    private static func poll(timeout: TimeInterval, every: Double, until done: () -> Bool) async -> Bool {
        let deadline = now + timeout
        while !done() {
            guard now < deadline else { return false }
            try? await Task.sleep(for: .seconds(every))
        }
        return true
    }

    private static func allDisplays() -> String {
        var count: UInt32 = 0
        CGGetOnlineDisplayList(0, nil, &count)
        var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
        CGGetOnlineDisplayList(count, &ids, &count)
        // 带上排布位置：系统按身份记住每块屏摆在哪，换了名字还摆回原处说明它认得这块屏
        return ids.prefix(Int(count)).map { id in
            let origin = CGDisplayBounds(id).origin
            return "#\(id)「\(Screens.name(of: id))」\(CGDisplayIsMain(id) != 0 ? "主屏 " : "")\(DisplayMode.describe(id)) @(\(Int(origin.x)),\(Int(origin.y)))"
        }.joined(separator: " | ")
    }

    /// 屏幕配置事务不在主线程上做
    private static func offMain(_ work: @escaping @Sendable () -> CGError) async -> CGError {
        await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async { continuation.resume(returning: work()) }
        }
    }

    private static func setMode(_ id: DisplayID, _ size: ProbeScreen.Size) async -> Int32? {
        guard let executable = Bundle.main.executableURL else { return nil }
        let process = Process()
        process.executableURL = executable
        process.arguments = ModeTarget.hiDPI(width: size.width, height: size.height).arguments(display: id)
        return await withCheckedContinuation { continuation in
            process.terminationHandler = { continuation.resume(returning: $0.terminationStatus) }
            do {
                try process.run()
            } catch {
                process.terminationHandler = nil
                continuation.resume(returning: nil)
            }
        }
    }
}

/// 校准图：左上原点，单位是点
private final class ProbePatternView: NSView {
    private let probe: ProbeDisplay
    private let onTap: (CGPoint, CGPoint) -> Void
    private var taps: [CGPoint] = []

    init(probe: ProbeDisplay, onTap: @escaping (CGPoint, CGPoint) -> Void) {
        self.probe = probe
        self.onTap = onTap
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("不从 nib 建") }

    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    private var targets: [CGPoint] {
        let w = bounds.width, h = bounds.height
        return [1, 2, 3].flatMap { i in [1, 2, 3].map { j in CGPoint(x: (w * CGFloat(i) / 4).rounded(), y: (h * CGFloat(j) / 4).rounded()) } }
    }

    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        taps = Array((taps + [point]).suffix(6))
        let nearest = targets.min { hypot($0.x - point.x, $0.y - point.y) < hypot($1.x - point.x, $1.y - point.y) } ?? point
        onTap(point, nearest)
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor(white: 0.12, alpha: 1).setFill()
        bounds.fill()
        let w = bounds.width, h = bounds.height
        // 每条边往里画到 200 点（屏太小就到四分之一）
        let depth = min(200, Int(min(w, h) / 4))
        for d in stride(from: 0, through: depth, by: 10) {
            let major = d % 50 == 0
            (d == 0 ? NSColor.systemRed : major ? NSColor.systemYellow : NSColor(white: 0.55, alpha: 1)).setFill()
            let t: CGFloat = major ? 2 : 1
            let v = CGFloat(d)
            NSRect(x: 0, y: v, width: w, height: t).fill()              // 上
            NSRect(x: 0, y: h - v - t, width: w, height: t).fill()      // 下
            NSRect(x: v, y: 0, width: t, height: h).fill()              // 左
            NSRect(x: w - v - t, y: 0, width: t, height: h).fill()      // 右
            guard d % 20 == 0, d > 0 else { continue }
            let label = "\(d)"
            for x in [w * 0.3, w * 0.7] {
                text(label, at: CGPoint(x: x, y: v + 2), size: 9)
                text(label, at: CGPoint(x: x, y: h - v - 13), size: 9)
            }
            for y in [h * 0.3, h * 0.7] {
                text(label, at: CGPoint(x: v + 3, y: y), size: 9)
                text(label, at: CGPoint(x: w - v - 18, y: y), size: 9)
            }
        }
        for target in targets {
            NSColor.systemGreen.setFill()
            NSRect(x: target.x - 12, y: target.y - 0.5, width: 24, height: 1).fill()
            NSRect(x: target.x - 0.5, y: target.y - 12, width: 1, height: 24).fill()
            text("\(Int(target.x)),\(Int(target.y))", at: CGPoint(x: target.x + 4, y: target.y + 4), size: 9)
        }
        for (i, tap) in taps.enumerated() {
            (i == taps.count - 1 ? NSColor.systemRed : NSColor.systemRed.withAlphaComponent(0.4)).setFill()
            NSBezierPath(ovalIn: NSRect(x: tap.x - 5, y: tap.y - 5, width: 10, height: 10)).fill()
        }
        let mode = DisplayMode.current(probe.id)
        var info = [probe.screen.name,
                    "\(Int(w)) × \(Int(h)) 点" + (mode.map { " · \($0.pixelWidth) × \($0.pixelHeight) 像素" } ?? ""),
                    "#\(probe.id) · serial \(probe.screen.serial)",
                    "线：离那条边多少点（黄线每 50）· 点任意处标出落点"]
        if let last = taps.last { info.append(String(format: "上次落点 (%.0f, %.0f)", last.x, last.y)) }
        for (i, line) in info.enumerated() {
            text(line, at: CGPoint(x: w / 2 - 150, y: h / 2 - 50 + CGFloat(i) * 20), size: i == 0 ? 15 : 12)
        }
    }

    private func text(_ string: String, at point: CGPoint, size: CGFloat) {
        let attributes: [NSAttributedString.Key: Any] = [.font: NSFont.monospacedSystemFont(ofSize: size, weight: .medium),
                                                        .foregroundColor: NSColor.white]
        NSAttributedString(string: string, attributes: attributes).draw(at: point)
    }
}
