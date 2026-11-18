// 屏幕适配：手机屏的比例跟着用户的手机走，宽度跟着屏上的窗口走。
// 比例：向日葵 把画面等比缩放、居中铺在手机上，底部工具栏压在画面上。手机屏的高宽比 = 手机屏幕的高宽比扣掉上下各 sunloginInset，
// 画面正好落在工具栏和灵动岛之间（17 Pro Max 上是 540×950）。手机屏幕从哪来见 PhoneModels。
// 宽度：平时用基准宽度，越窄字越大，最窄到 2 倍高清的门槛（宽 540、高 800，实测）。App 限定了最小尺寸、基准宽度铺不满时，
// 手机屏换成刚好装得下它的一档，窗口照样铺满，整屏等比缩小（字跟着小，手机上可以双指放大）。
// 不按 App 预设：放窗口时先按基准宽度铺，App 卡在自己的最小尺寸上，读回来的大小就是它要的，据此选档。
// 档位建屏时一次登记好，宽高比都一样：换档时 向日葵 不用重新推流、点击位置照样准，画面平滑缩放；
// 换档只是选一档（帮手进程切，约 0.4 秒，屏不掉线、编号不变）。实测见记忆 project-phone-screen
import CoreGraphics

public struct ScreenFit: Equatable, Sendable, CustomStringConvertible {
    /// Snug 自己的面板和手势阈值按这个宽度设计（17 Pro Max 上的基准宽度）
    public static let designWidth = 540
    /// 2 倍高清的门槛（实测，和像素上限无关）：宽不小于 540、高不小于 800 点，系统才把 2 倍模式标成桌面可用
    public static let minWidth = 540
    public static let minHeight = 800
    /// 最宽一档：字只剩一半，不比 向日葵 原版看主屏大了，再宽没有意义。更宽的 App 用这一档、靠左放
    public static let maxWidth = 1080
    /// 相邻两档差 10 点：装下窗口后最多多缩 2%
    public static let step = 10
    /// 高宽比的范围：最方到正方形（折叠屏内屏扣掉工具栏后比这还方），最长到 2（像素上限按它定）
    public static let ratioRange: ClosedRange<Double> = 1...2
    /// 像素上限：最宽一档按最长比例的 2 倍，所有比例共用。换比例时屏幕身份不变、不多出 ICC 文件（实测）
    public static let maxPixels = CGSize(width: maxWidth * 2, height: Int(Double(maxWidth) * ratioRange.upperBound) * 2)
    /// 画面上下各让出的高度（点）。向日葵 底部工具栏 17 Pro Max 实测约 268 像素（89 点）；取 91 让 17 Pro Max 正好是
    /// 真机上调好的 540×950（取 89 是 540×955），多出的 2 点当余量
    static let sunloginInset: Double = 91
    /// 不知道逻辑宽度时（安卓、自己填的）按这个估：安卓手机默认显示大小下常见 360–412dp
    static let assumedPoints: Double = 400
    /// 基准宽度下底部小横条的高度（点）
    static let baseHomeBarHeight: CGFloat = 28

    /// 高宽比
    public let ratio: Double
    /// 基准宽度：App 资源库、能铺满的窗口都用它
    public let baseWidth: Int
    /// 登记的档位：基准宽度到最宽一档，每 10 点一档
    public let widths: [Int]

    /// 没选过手机时的比例（540×950）
    public static let standard = ScreenFit(phone: PhoneChoice.standard.screen)

    /// 按用户的手机算比例
    public init(phone: PhoneScreen) {
        let points = phone.points.map(Double.init) ?? Self.assumedPoints
        self.init(ratio: Double(phone.height) / Double(phone.width) - 2 * Self.sunloginInset / points)
    }

    init(ratio: Double) {
        let ratio = ratio.clamped(to: Self.ratioRange)
        self.ratio = ratio
        // 矮的屏 540 宽时高不到 800，没有 2 倍：基准加宽到够高的最窄一档（比例不小于 1，800 宽一定够）
        let all = stride(from: Self.minWidth, through: Self.maxWidth, by: Self.step)
        baseWidth = all.first { (Double($0) * ratio).rounded() >= Double(Self.minHeight) } ?? Self.maxWidth
        widths = Array(stride(from: baseWidth, through: Self.maxWidth, by: Self.step))
    }

    /// 这一档的逻辑尺寸（点）
    public func size(width: Int) -> CGSize {
        CGSize(width: CGFloat(width), height: (Double(width) * ratio).rounded())
    }

    /// 日志里的样子：基准尺寸
    public var description: String {
        "\(baseWidth)×\(Int(size(width: baseWidth).height))"
    }

    /// 窗口该用哪一档。window 是按 area（current 这一档的可放区域）铺过之后读回的实际大小：
    /// 装得下就还是 current；App 不让缩到那么小，就按超出的比例往上找装得下的最窄一档，超过最宽一档就用最宽一档。
    /// 按比例放大是够的：可放区域里只有菜单栏不跟着缩放，屏越宽它占的比例越小。
    /// 先乘后除：540 × (580 / 540) 在浮点下是 580.0000000000001，向上取整会多进一档
    public func width(toFit window: CGSize, in area: CGSize, at current: Int) -> Int {
        let tolerance: CGFloat = 2
        guard window.width > area.width + tolerance || window.height > area.height + tolerance else { return current }
        let base = CGFloat(current)
        let needed = max(base * window.width / area.width, base * window.height / area.height)
        return widths.first { CGFloat($0) >= needed.rounded(.up) } ?? Self.maxWidth
    }

    /// 屏上的点比设计宽度小了几倍：Snug 自己的面板和手势阈值按它放大，在手机上看着、划着一样
    public static func scale(screenWidth: CGFloat) -> CGFloat {
        screenWidth / CGFloat(designWidth)
    }

    /// 底部小横条的高度（这一档的点）：跟着缩放，手指点着一样大。放上来的窗口让出这一条
    public static func homeBarHeight(screenWidth: CGFloat) -> CGFloat {
        (baseHomeBarHeight * scale(screenWidth: screenWidth)).rounded()
    }
}

extension TouchRegion {
    /// 手势翻译管整块手机屏，阈值按这一档比基准宽度宽几倍放大（和小横条一样，在手机上划着手感不变）。
    /// closeButton：手机屏上那个窗口的关闭按钮，点它时交给 Snug 先放回原位再关
    public init(phoneScreen bounds: CGRect, closeButton: CloseButton? = nil) {
        self.init(frame: bounds, scale: ScreenFit.scale(screenWidth: bounds.width), closeButton: closeButton)
    }
}
