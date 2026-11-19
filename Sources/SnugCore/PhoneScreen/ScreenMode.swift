// 一块屏的显示模式，和帮手进程要切到哪个模式。
// 加屏、撤屏前记下别的屏的模式，换完把被 macOS 拨动的设回去（见 PhoneDesk 的 Surroundings）
import Foundation

public struct ScreenMode: Equatable, Sendable, CustomStringConvertible {
    /// 系统给这一档的编号（CGDisplayModeGetIODisplayModeID，SInt32），只在这块屏里有意义：可以是 0，系统保留段读出来是负数。
    /// 屏幕组合变了会重排，属性相同的也可能有好几档：只用来在属性相同的几档里挑回原来那一档
    public var ioModeID: Int32
    /// 点数
    public var width: Int
    public var height: Int
    public var pixelWidth: Int
    public var pixelHeight: Int
    public var refreshRate: Double

    public init(ioModeID: Int32, width: Int, height: Int, pixelWidth: Int, pixelHeight: Int, refreshRate: Double) {
        self.ioModeID = ioModeID
        self.width = width
        self.height = height
        self.pixelWidth = pixelWidth
        self.pixelHeight = pixelHeight
        self.refreshRate = refreshRate
    }

    /// 是不是同一个设置：系统按分辨率、缩放、刷新率记每块屏的模式，不记编号
    public func isSame(as other: ScreenMode) -> Bool {
        width == other.width && height == other.height && pixelWidth == other.pixelWidth
            && pixelHeight == other.pixelHeight && abs(refreshRate - other.refreshRate) < 0.01
    }

    public var description: String {
        let scale = pixelWidth == width ? " " : pixelWidth == 2 * width ? "（2 倍）" : "（\(pixelWidth)x\(pixelHeight) 像素）"
        let hz = refreshRate.rounded() == refreshRate ? String(Int(refreshRate)) : String(format: "%.2f", refreshRate)
        return "\(width)x\(height)\(scale)\(hz)Hz"
    }
}

/// 帮手进程（Snug --set-mode / --restore-mode）要把屏切到哪个模式
public enum ModeTarget: Equatable, Sendable, CustomStringConvertible {
    /// 手机屏换档：width × height（点）的 2 倍模式，只对本次登录生效
    case hiDPI(width: Int, height: Int)
    /// 别的屏：设回这个模式。permanently 时连永久配置一起写，否则只对本次登录生效
    case exact(ScreenMode, permanently: Bool)
    /// 别的屏：把旋转设回这个角度（度，CGDisplayRotation 的方向：顺时针为正，只认 0/90/180/270）。
    /// 和模式分开设：换屏幕组合时 macOS 可能只把旋转拨回 0°，模式的宽高看着没变；也可能先拨模式再拨旋转，
    /// 所以设回时先设模式、再设旋转。permanently 的含义同 exact
    case rotation(degrees: Double, permanently: Bool)

    /// 交给帮手的命令行参数，LaunchMode 按同一套解析回来
    public func arguments(display: DisplayID) -> [String] {
        switch self {
        case .hiDPI(let width, let height):
            ["--set-mode", "\(display)", "\(width)", "\(height)"]
        case .exact(let m, let permanently):
            // 编号按无符号写：负数在命令行上是单横线开头，会被当成系统塞进来的参数过滤掉
            ["--restore-mode", "\(display)", "\(UInt32(bitPattern: m.ioModeID))", "\(m.width)", "\(m.height)", "\(m.pixelWidth)", "\(m.pixelHeight)",
             "\(m.refreshRate)", permanently ? Self.permanent : Self.session]
        case .rotation(let degrees, let permanently):
            ["--restore-rotation", "\(display)", "\(degrees)", permanently ? Self.permanent : Self.session]
        }
    }

    static let permanent = "permanent"
    static let session = "session"

    public var description: String {
        switch self {
        case .hiDPI(let width, let height): "2 倍 \(width)x\(height)"
        case .exact(let mode, let permanently): "\(mode)（模式 \(mode.ioModeID)，\(permanently ? "连永久配置" : "只对本次登录")）"
        case .rotation(let degrees, let permanently): "旋转 \(degrees)°（\(permanently ? "连永久配置" : "只对本次登录")）"
        }
    }
}

/// 手机屏以外的一块屏此刻的样子
public struct OtherDisplay: Equatable, Sendable {
    public var id: DisplayID
    /// 在「显示器」设置里的名字，日志用
    public var name: String
    public var mode: ScreenMode
    /// 旋转角度（度，CGDisplayRotation：顺时针为正）。竖屏是 90° 或 270°，横屏是 0°
    public var rotation: Double
    /// 睡着了（Mac 锁屏后几秒显示器就睡）：这时改不了模式
    public var asleep: Bool

    public init(id: DisplayID, name: String, mode: ScreenMode, rotation: Double = 0, asleep: Bool = false) {
        self.id = id
        self.name = name
        self.mode = mode
        self.rotation = rotation
        self.asleep = asleep
    }
}
