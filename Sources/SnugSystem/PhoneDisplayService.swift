// 手机屏的建、撤、换档、换比例（CGVirtualDisplay 私有接口）。实测过的做法（见记忆 project-phone-screen）：
// - 建屏时把 ScreenFit 的每一档都登记成模式，hiDPI 打开，maxPixels 框住所有比例：每一档都有 2 倍模式
// - 换比例在线换模式表（约 0.2 秒，屏不掉线、编号不变），系统随即跳到最宽一档，再切回基准宽度
// - 系统上线时用它记着的那一档（第一次是最宽的），所以上线后切回基准宽度
// - 换档交给帮手进程（主程序亲自切会让释放拖 40 秒以上）；把被 macOS 拨动的别的屏设回去也交给它
// - 释放后通常 0.1 秒下线；显示器睡着（Mac 锁屏）时要等唤醒才下线，撤屏后的善后由 PhoneDesk 等它真下线再做
// - 建好不上线就换备用身份
// - 在主线程建和释放；释放时不能留局部强引用，否则不会真正释放
import AppKit
import CGVirtualDisplayBridge
import SnugCore

@MainActor
public final class PhoneDisplayService: PhoneDisplays {
    static let onlineTimeout: TimeInterval = 6
    static let offlineTimeout: TimeInterval = 10
    /// 帮手切模式最多等这么久（正常不到一秒）
    nonisolated static let helperTimeout: TimeInterval = 10
    /// 切完等界面（NSScreen）认到新尺寸最多这么久：可放区域按它算，认到之前算出来的还是旧尺寸
    static let screenUpdateTimeout: TimeInterval = 2
    /// 换了模式表等系统列出新档位最多这么久
    static let modesTimeout: TimeInterval = 3

    private var displays: [DisplayID: CGVirtualDisplay] = [:]
    /// 每块屏登记的是哪个比例的档位：换档时按它算高
    private var fits: [DisplayID: ScreenFit] = [:]
    /// 帮手进程：就是 Snug 自己，带 --set-mode / --restore-mode 运行
    private let helper: URL

    public init(helper: URL) { self.helper = helper }

    public func create(_ fit: ScreenFit) async throws -> DisplayID {
        var madeAny = false
        for serial in PhoneScreenSpec.serials {
            guard let display = make(serial: serial, fit: fit) else {
                Log.warn("手机屏（serial=\(serial)）建不出来，换备用身份")
                continue
            }
            madeAny = true
            let id = display.displayID
            displays[id] = display
            fits[id] = fit
            guard await wait(id, online: true, timeout: Self.onlineTimeout) else {
                Log.warn("手机屏 #\(id)（serial=\(serial)）建好 \(Int(Self.onlineTimeout)) 秒没上线，换备用身份")
                _ = await retire(id)   // 没上线的屏，下没下线都换下一个身份
                continue
            }
            // 刚上线时模式还在变，等它定下来再看是哪一档
            try? await Task.sleep(for: .seconds(0.8))
            if width(id) != fit.baseWidth { await setWidth(id, fit.baseWidth) }
            return id
        }
        throw madeAny ? PhoneScreenError.neverOnline : PhoneScreenError.createFailed
    }

    public func retire(_ id: DisplayID) async -> Bool {
        guard displays[id] != nil else {
            Log.warn("要撤的手机屏 #\(id) 不是本进程建的")
            return !Self.isOnline(id)
        }
        autoreleasepool { displays[id] = nil }
        fits[id] = nil
        guard await wait(id, online: false, timeout: Self.offlineTimeout) else {
            Log.warn("手机屏 #\(id) 释放 \(Int(Self.offlineTimeout)) 秒后仍在线")
            return false
        }
        return true
    }

    public func isOnline(_ id: DisplayID) -> Bool { Self.isOnline(id) }

    public func reshape(_ id: DisplayID, to fit: ScreenFit) async -> Bool {
        guard let display = displays[id] else {
            Log.warn("要换比例的手机屏 #\(id) 不是本进程建的")
            return false
        }
        let from = DisplayMode.describe(id)
        guard autoreleasepool(invoking: { display.apply(Self.settings(fit)) }) else {
            Log.warn("手机屏 #\(id) 换比例时 apply 失败，还是原来的档位（\(from)）")
            return false
        }
        fits[id] = fit
        let started = ProcessInfo.processInfo.systemUptime
        // 系统列出新档位要一会儿：列出来之前帮手找不到这一档。等不到也照样去切，切不成 setWidth 会记下来
        let base = fit.size(width: fit.baseWidth)
        let listed = await poll(timeout: Self.modesTimeout, every: 0.1) {
            DisplayMode.hasHiDPI(id, width: Int(base.width), height: Int(base.height))
        }
        if !listed {
            Log.warn("手机屏 #\(id) 换了模式表，\(Int(Self.modesTimeout)) 秒内系统没列出 \(fit) 的 2 倍模式，照样去切")
        }
        await setWidth(id, fit.baseWidth)
        let took = String(format: "%.1f", ProcessInfo.processInfo.systemUptime - started)
        Log.info("手机屏 #\(id) 换成 \(fit) 的档位（\(took)s，原来 \(from)），现在 \(DisplayMode.describe(id))")
        return true
    }

    public func fit(_ id: DisplayID) -> ScreenFit? { fits[id] }

    public func width(_ id: DisplayID) -> Int? {
        DisplayMode.hiDPIWidth(id)
    }

    public func setWidth(_ id: DisplayID, _ width: Int) async {
        guard let fit = fits[id] else {
            Log.warn("要换档的手机屏 #\(id) 不是本进程建的")
            return
        }
        let from = DisplayMode.describe(id)
        let size = fit.size(width: width)
        let status = await runHelper(id, .hiDPI(width: width, height: Int(size.height)))
        guard status == 0 else {
            Log.warn("手机屏 #\(id) 换不到 \(width) 宽（帮手退出码 \(status.map(String.init) ?? "启动失败")），现在 \(DisplayMode.describe(id))")
            return
        }
        let switched = await poll(timeout: Self.screenUpdateTimeout, every: 0.05) {
            self.width(id) == width && Screens.screen(for: id)?.frame.width == CGFloat(width)
        }
        if switched {
            Log.info("手机屏 #\(id) 换到 \(width) 宽（原来 \(from)）")
        } else {
            Log.warn("手机屏 #\(id) 帮手说换到了 \(width) 宽，\(Int(Self.screenUpdateTimeout)) 秒内界面没认到，现在 \(DisplayMode.describe(id))")
        }
    }

    public func bounds(_ id: DisplayID) -> CGRect? {
        // CGDisplayBounds 和辅助功能用的是同一套全局坐标（左上原点）；屏不在时是空框
        let rect = CGDisplayBounds(id)
        return rect.isEmpty ? nil : rect
    }

    public func windowArea(_ id: DisplayID) -> CGRect? {
        guard let screen = Screens.screen(for: id) else { return nil }
        var area = Screens.axRect(fromAppKit: screen.visibleFrame)
        area.size.height -= ScreenFit.homeBarHeight(screenWidth: screen.frame.width)
        return area
    }

    public func mainArea() -> CGRect? {
        Screens.primary.map { Screens.axRect(fromAppKit: $0.visibleFrame) }
    }

    public func otherDisplays() -> [OtherDisplay] {
        Self.onlineIDs().filter { displays[$0] == nil }.compactMap { id in
            DisplayMode.current(id).map { OtherDisplay(id: id, name: Screens.name(of: id), mode: $0, rotation: DisplayMode.rotation(id), asleep: CGDisplayIsAsleep(id) != 0) }
        }
    }

    /// 成功时帮手自己记一行（「[帮手] 切屏幕 …：已切换」）
    public func setMode(_ id: DisplayID, _ mode: ScreenMode, permanently: Bool) async {
        let status = await runHelper(id, .exact(mode, permanently: permanently))
        if status != 0 {
            Log.warn("「\(Screens.name(of: id))」设不回 \(mode)（帮手退出码 \(status.map(String.init) ?? "启动失败")），现在 \(DisplayMode.describe(id))")
        }
    }

    /// 旋转设不回去只记日志：模式已经设回去了，不能因为旋转卡住整条撤屏
    public func setRotation(_ id: DisplayID, _ degrees: Double, permanently: Bool) async {
        let status = await runHelper(id, .rotation(degrees: degrees, permanently: permanently))
        if status != 0 {
            Log.warn("「\(Screens.name(of: id))」旋转设不回 \(degrees)°（帮手退出码 \(status.map(String.init) ?? "启动失败")），现在 \(DisplayMode.rotation(id))°")
        }
    }

    // MARK: 内部

    private func make(serial: UInt32, fit: ScreenFit) -> CGVirtualDisplay? {
        let descriptor = CGVirtualDisplayDescriptor()
        descriptor.name = PhoneScreenSpec.name
        descriptor.maxPixelsWide = UInt32(ScreenFit.maxPixels.width)
        descriptor.maxPixelsHigh = UInt32(ScreenFit.maxPixels.height)
        descriptor.sizeInMillimeters = CGSize(width: PhoneScreenSpec.millimeters.width, height: PhoneScreenSpec.millimeters.height)
        descriptor.vendorID = PhoneScreenSpec.vendorID
        descriptor.productID = PhoneScreenSpec.productID
        descriptor.serialNum = serial
        guard let display = autoreleasepool(invoking: { CGVirtualDisplay(descriptor: descriptor) }) else { return nil }
        guard autoreleasepool(invoking: { display.apply(Self.settings(fit)) }) else {
            Log.warn("手机屏（serial=\(serial)）apply 失败")
            return nil
        }
        return display
    }

    /// fit 的每一档都登记成模式，打开 2 倍
    private static func settings(_ fit: ScreenFit) -> CGVirtualDisplaySettings {
        let settings = CGVirtualDisplaySettings()
        settings.hiDPI = true
        settings.modes = fit.widths.map { width in
            let size = fit.size(width: width)
            return CGVirtualDisplayMode(width: UInt(size.width), height: UInt(size.height), refreshRate: 60)
        }
        return settings
    }

    private func wait(_ id: DisplayID, online: Bool, timeout: TimeInterval) async -> Bool {
        await poll(timeout: timeout, every: 0.25) { Self.isOnline(id) == online }
    }

    /// 每隔 every 秒看一次，done 成立就返回 true；到了 timeout 还不成立返回 false
    private func poll(timeout: TimeInterval, every: Double, until done: () -> Bool) async -> Bool {
        let deadline = ProcessInfo.processInfo.systemUptime + timeout
        while !done() {
            guard ProcessInfo.processInfo.systemUptime < deadline else { return false }
            try? await Task.sleep(for: .seconds(every))
        }
        return true
    }

    private static func isOnline(_ id: DisplayID) -> Bool { onlineIDs().contains(id) }

    private static func onlineIDs() -> [DisplayID] {
        var count: UInt32 = 0
        guard CGGetOnlineDisplayList(0, nil, &count) == .success, count > 0 else { return [] }
        var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
        guard CGGetOnlineDisplayList(count, &ids, &count) == .success else { return [] }
        return Array(ids.prefix(Int(count)))
    }

    /// 起帮手进程切模式，等它退出；返回退出码，起不来返回 nil。
    /// 最多等 helperTimeout：帮手卡在屏幕配置里不退出，就结束它（换档本来就是尽力而为），不能让建屏和整条调度队一直等
    private func runHelper(_ id: DisplayID, _ target: ModeTarget) async -> Int32? {
        let process = Process()
        process.executableURL = helper
        process.arguments = target.arguments(display: id)
        return await withCheckedContinuation { continuation in
            process.terminationHandler = { continuation.resume(returning: $0.terminationStatus) }
            do {
                try process.run()
            } catch {
                process.terminationHandler = nil
                Log.error("帮手进程起不来（\(helper.path)）：\(error)")
                continuation.resume(returning: nil)
                return
            }
            // 只有 terminationHandler 会 resume：结束进程后它照样回调
            DispatchQueue.global().asyncAfter(deadline: .now() + Self.helperTimeout) {
                guard process.isRunning else { return }
                Log.error("帮手进程 \(Int(Self.helperTimeout))s 没退出（屏 #\(id)），结束它")
                process.terminate()
            }
        }
    }
}
