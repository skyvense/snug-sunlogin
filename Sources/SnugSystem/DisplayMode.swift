// 显示模式：读此刻是哪一档，以及帮手进程里真正去切（手机屏换档、把被 macOS 拨动的别的屏设回去）。
// 主程序不亲自切模式：亲自切过模式的进程，这块虚拟屏释放后要拖 40 秒以上才下线（两次实测）；
// 交给短命的帮手进程（Snug --set-mode / --restore-mode）切完就退出，释放照样 0.1 秒下线
import CoreGraphics
import SnugCore

public enum DisplayMode {
    public enum Outcome: Equatable, CustomStringConvertible {
        case switched
        /// 这块屏没有这个模式
        case unavailable
        case failed(CGError)

        public var description: String {
            switch self {
            case .switched: "已切换"
            case .unavailable: "没有这个模式"
            case .failed(let err): "切换失败（CGError=\(err.rawValue)）"
            }
        }
    }

    /// 此刻的模式；屏不在了返回 nil
    public static func current(_ id: DisplayID) -> ScreenMode? {
        CGDisplayCopyDisplayMode(id).map(ScreenMode.init)
    }

    /// 此刻的旋转角度（度，顺时针为正；屏不在了返回 0）
    public static func rotation(_ id: DisplayID) -> Double {
        CGDisplayRotation(id)
    }

    /// 此刻 2 倍模式的宽度（点）；不是 2 倍、或屏不在了返回 nil
    public static func hiDPIWidth(_ id: DisplayID) -> Int? {
        guard let mode = current(id), mode.pixelWidth == 2 * mode.width else { return nil }
        return mode.width
    }

    public static func describe(_ id: DisplayID) -> String {
        current(id).map { "\($0)" } ?? "无模式"
    }

    /// 系统列出的全部模式（含同尺寸的 1 倍、2 倍），以及每一档能不能当桌面用。实测新尺寸的屏时看（--probe-screens）
    public static func listed(_ id: DisplayID) -> [(mode: ScreenMode, usable: Bool)] {
        let options = [kCGDisplayShowDuplicateLowResolutionModes: true] as CFDictionary
        let modes = (CGDisplayCopyAllDisplayModes(id, options) as? [CGDisplayMode]) ?? []
        return modes.map { (ScreenMode($0), $0.isUsableForDesktopGUI()) }
    }

    /// 系统列出了 width×height 的 2 倍模式没有（换了模式表要等一会儿才列出来）
    public static func hasHiDPI(_ id: DisplayID, width: Int, height: Int) -> Bool {
        find(.hiDPI(width: width, height: height), on: id) != nil
    }

    private static func find(_ target: ModeTarget, on id: DisplayID) -> CGDisplayMode? {
        let options = [kCGDisplayShowDuplicateLowResolutionModes: true] as CFDictionary
        let modes = (CGDisplayCopyAllDisplayModes(id, options) as? [CGDisplayMode]) ?? []
        switch target {
        case .hiDPI(let width, let height):
            return modes.first { $0.width == width && $0.height == height && $0.pixelWidth == 2 * width }
        case .rotation: return nil // 旋转不按模式找：apply 开头已分流
        case .exact(let mode, _):
            // 属性相同的可能有好几档：挑回原来那个编号，编号重排了就挑能当桌面用的那一档
            let same = modes.filter { ScreenMode($0).isSame(as: mode) }
            return same.first { $0.ioDisplayModeID == mode.ioModeID } ?? same.first { $0.isUsableForDesktopGUI() } ?? same.first
        }
    }

    /// 帮手进程里调用：把屏切到 target。手机屏换档只对本次登录生效；设回别的屏按 target 要求，
    /// 连永久配置时写的是 WindowServer 的永久配置（屏幕组合一变，它会把套用来的旧模式存进去：日志「Saving configuration: session, permanent store」）
    public static func apply(_ target: ModeTarget, to id: DisplayID) -> Outcome {
        if case .rotation(let degrees, let permanently) = target {
            return applyRotation(degrees, to: id, permanently: permanently)
        }
        guard let mode = find(target, on: id) else { return .unavailable }
        let scope: CGConfigureOption = switch target {
        case .hiDPI: .forSession
        case .exact(_, let permanently): permanently ? .permanently : .forSession
        case .rotation: .forSession // 走不到：上面已分流到 applyRotation
        }
        var config: CGDisplayConfigRef?
        let begin = CGBeginDisplayConfiguration(&config)
        guard begin == .success else { return .failed(begin) }
        let staged = CGConfigureDisplayWithDisplayMode(config, id, mode, nil)
        guard staged == .success else {
            CGCancelDisplayConfiguration(config)
            return .failed(staged)
        }
        let done = CGCompleteDisplayConfiguration(config, scope)
        if done != .success { CGCancelDisplayConfiguration(config) }
        return done == .success ? .switched : .failed(done)
    }

    /// 帮手进程里调用：把屏的旋转设到 degrees（度，只认 0/90/180/270）。和 apply 分开调：
    /// 设回时先设模式、再设旋转（模式一换，旋转可能跟着被拨回 0°）
    public static func applyRotation(_ degrees: Double, to id: DisplayID, permanently: Bool) -> Outcome {
        var config: CGDisplayConfigRef?
        let begin = CGBeginDisplayConfiguration(&config)
        guard begin == .success else { return .failed(begin) }
        let staged = CGConfigureDisplayRotation(config, id, degrees)
        guard staged == .success else {
            CGCancelDisplayConfiguration(config)
            return .failed(staged)
        }
        let done = CGCompleteDisplayConfiguration(config, permanently ? .permanently : .forSession)
        if done != .success { CGCancelDisplayConfiguration(config) }
        return done == .success ? .switched : .failed(done)
    }
}

extension ScreenMode {
    init(_ mode: CGDisplayMode) {
        self.init(ioModeID: mode.ioDisplayModeID, width: mode.width, height: mode.height,
                  pixelWidth: mode.pixelWidth, pixelHeight: mode.pixelHeight, refreshRate: mode.refreshRate)
    }
}
