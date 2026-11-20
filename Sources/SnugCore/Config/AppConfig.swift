// 用户设置（config.json）。App 层的 AppController 是唯一写入者
import Foundation

public struct AppConfig: Codable, Equatable, Sendable {
    /// 总开关：用手机远程时加一块手机屏
    public var phoneScreen = true
    /// 手机屏上把 向日葵 发来的单指操作翻译成 iPhone 的手势
    /// （在向日葵下有异常：暂时强制关闭——默认关、解码时不读存量值、设置页也开启不了；修好再恢复）
    public var gestures = false
    /// 手机屏底部小横条左边放相机键：截当前窗口到剪贴板。要另授「录屏与系统录音」权限，默认关
    public var screenshot = false
    /// 窗口放上手机屏时，字号默认放大几级（每个 App 改过之后按它自己记住的级数）
    public var zoomSteps = 2
    /// 用户的手机：手机屏的比例按它算（见 ScreenFit）
    public var phone = PhoneChoice.standard
    /// 是否已看过首次启动的欢迎页
    public var welcomed = false
    /// 从哪检查和下载更新。本 fork 只有 GitHub 一个源（上游的腾讯云国内源没有配置）
    public var updateSource = UpdateSource.github
    /// 上次运行的版本：和这次不一样说明刚换了版本，启动时弹「关于 Snug」
    public var lastVersion: Version?

    public static let zoomStepsRange = 0...4

    public init() {}

    private enum CodingKeys: String, CodingKey {
        case phoneScreen, gestures, screenshot, zoomSteps, phone, welcomed, updateSource, lastVersion
    }

    // 每一项都有默认值：以后加字段、或者手改漏了某项，都不会让整份配置被判为损坏
    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = AppConfig()
        phoneScreen = try c.decodeIfPresent(Bool.self, forKey: .phoneScreen) ?? d.phoneScreen
        gestures = false // 强制关闭（见属性注释）：存量配置里开着的也不再生效
        screenshot = try c.decodeIfPresent(Bool.self, forKey: .screenshot) ?? d.screenshot
        zoomSteps = try c.decodeIfPresent(Int.self, forKey: .zoomSteps).map { $0.clamped(to: Self.zoomStepsRange) } ?? d.zoomSteps
        // 手改坏了的（缺字段、不是竖握、不是正数）用默认值，不连累别的设置
        phone = (try? c.decodeIfPresent(PhoneChoice.self, forKey: .phone)).flatMap { $0.screen.isValid ? $0 : nil } ?? d.phone
        welcomed = try c.decodeIfPresent(Bool.self, forKey: .welcomed) ?? d.welcomed
        // 不认识的更新源（更新的版本加的、降级回来）用默认值
        updateSource = (try? c.decodeIfPresent(UpdateSource.self, forKey: .updateSource)) ?? d.updateSource
        lastVersion = try? c.decodeIfPresent(Version.self, forKey: .lastVersion)
    }
}
