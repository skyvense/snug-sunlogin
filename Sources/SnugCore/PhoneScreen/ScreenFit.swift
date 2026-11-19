// 屏幕适配：手机屏的比例跟着用户的手机走，宽度跟着屏上的窗口走。
// 比例：向日葵 在手机上全屏显示画面，工具栏是浮在画面上的覆盖层、不占画面高度，所以不扣：
// 手机屏的高宽比 = 手机屏幕的完整高宽比（17 Pro Max 上是 540×1173）。手机屏幕从哪来见 PhoneModels。
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
    /// 高宽比的范围：最方到正方形（折叠屏内屏比这还方），最长到 2.25（现代手机全屏比例：19.5:9 约 2.17、20:9 约 2.22；像素上限按它定）
    public static let ratioRange: ClosedRange<Double> = 1...2.25
    /// 像素上限：最宽一档按最长比例的 2 倍，所有比例共用。换比例时屏幕身份不变、不多出 ICC 文件（实测）
    /// 高度按 size() 的吸附口径取（2430 点吸附到 2432），不然最长比例那一档的像素高会超出上限、模式登记不进去
    public static let maxPixels = CGSize(width: maxWidth * 2, height: Int((Double(maxWidth) * ratioRange.upperBound / 8).rounded() * 8) * 2)
    /// 画面上下各让出的高度（点）。向日葵 全屏显示、工具栏浮在画面上不占高度，所以是 0
    /// （2026-11-18 实机纠正）。原版 UU 客户端上下有黑边和工具栏才要扣，那是 UU 的界面
    static let sunloginInset: Double = 0
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

    /// 没选过手机时的比例（540×1173）
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
    /// 实验（抓屏卡顿 A/B）：宽高都吸附到 8 的倍数，2 倍下像素就是 16 的倍数（540×1173 → 544×1176 点 = 1088×2352 像素）。
    /// 依据：手机远控物理屏（3840×2160、2560×1440、1440×2560）两轴都能被 16 整除、串流流畅；虚拟屏原先
    /// 1080×2346 两轴都不整除，且 Windows 主控看虚拟屏流畅、只有手机会话卡——怀疑手机硬解对非对齐尺寸走慢路径。
    /// 像素总量几乎不变（2.53MP → 2.56MP），单独验证「对齐」这一个变量。
    public func size(width: Int) -> CGSize {
        let w = (Double(width) / 8).rounded() * 8
        let h = (Double(width) * ratio / 8).rounded() * 8
        return CGSize(width: w, height: h)
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
