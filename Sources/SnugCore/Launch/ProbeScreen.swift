// 实测用的实验屏（--probe-screens，见 docs/multi-screen.md 第 7 节）：按给的尺寸建屏，看系统和 向日葵 怎么对待它。
// 一份规格一块屏，写成逗号分隔的 键=值：modes=1180x820+1000x695,name=…,serial=…,cap=4320x2160,pattern=off,main=on。
// 只用实验身份（999_000_001 起）：实验过的身份会被系统记住模式和像素上限，不能拿正式的或以后要正式用的身份来试；
// 第一块默认就是以前实验用过的 999_000_001，不多出 ICC 文件
import Foundation

public struct ProbeScreen: Equatable, Sendable {
    public struct Size: Equatable, Sendable, CustomStringConvertible {
        public var width: Int
        public var height: Int

        public init(_ width: Int, _ height: Int) {
            self.width = width
            self.height = height
        }

        public var description: String { "\(width)×\(height)" }

        /// 「1180x820」：两个正整数
        init?(_ text: Substring) {
            let parts = text.split(separator: "x", omittingEmptySubsequences: false)
            guard parts.count == 2, let w = Int(parts[0]), let h = Int(parts[1]), w > 0, h > 0 else { return nil }
            self.init(w, h)
        }
    }

    public static let firstSerial: UInt32 = 999_000_001
    public static let baseName = "Snug 实验屏"

    /// 「显示器」设置和 向日葵 屏幕列表里的名字
    public var name: String
    public var serial: UInt32
    /// 都按 2 倍登记（点）；建好后切到第一档
    public var modes: [Size]
    /// 像素上限
    public var maxPixels: Size
    /// 屏上铺校准图（四边标尺、点按落点）。测程序坞、菜单栏时关掉，免得盖住
    public var pattern: Bool
    /// 建好后设成主屏，撤之前把原来的排布设回去（V11）。最多一块
    public var main: Bool

    public init(name: String, serial: UInt32, modes: [Size], maxPixels: Size, pattern: Bool, main: Bool = false) {
        self.name = name
        self.serial = serial
        self.modes = modes
        self.maxPixels = maxPixels
        self.pattern = pattern
        self.main = main
    }

    /// index 是第几份规格（0 起）：没写名字和身份的按它排
    init?(spec: String, index: Int) {
        var fields: [Substring: Substring] = [:]
        for pair in spec.split(separator: ",") {
            guard let eq = pair.firstIndex(of: "=") else { return nil }
            let key = pair[..<eq], value = pair[pair.index(after: eq)...]
            guard ["modes", "name", "serial", "cap", "pattern", "main"].contains(key), fields[key] == nil else { return nil }
            fields[key] = value
        }
        guard let rawModes = fields["modes"], !rawModes.isEmpty else { return nil }
        let modes = rawModes.split(separator: "+", omittingEmptySubsequences: false).map(Size.init)
        guard !modes.contains(nil) else { return nil }
        self.modes = modes.compactMap { $0 }
        let needed = Size(self.modes.map { $0.width * 2 }.max()!, self.modes.map { $0.height * 2 }.max()!)
        if let rawCap = fields["cap"] {
            // 框不住最大那一档的 2 倍，系统就不会给它 2 倍模式，这份实验就白做了
            guard let cap = Size(rawCap), cap.width >= needed.width, cap.height >= needed.height else { return nil }
            maxPixels = cap
        } else {
            maxPixels = needed
        }
        if let rawSerial = fields["serial"] {
            guard let serial = UInt32(rawSerial), serial > 0 else { return nil }
            self.serial = serial
        } else {
            serial = Self.firstSerial + UInt32(index)
        }
        name = fields["name"].map(String.init) ?? (index == 0 ? Self.baseName : "\(Self.baseName) \(index + 1)")
        guard let pattern = Self.flag(fields["pattern"], default: true), let main = Self.flag(fields["main"], default: false) else { return nil }
        self.pattern = pattern
        self.main = main
    }

    /// on / off；没写用默认值，写了别的返回 nil
    private static func flag(_ value: Substring?, default fallback: Bool) -> Bool? {
        switch value {
        case nil: fallback
        case "on": true
        case "off": false
        default: nil
        }
    }

    /// 解析 --probe-screens 后面的每一份规格；身份不能重复（同一身份同时只能有一块屏在），主屏只能有一块
    static func parse(_ specs: [String]) throws -> [ProbeScreen] {
        var screens: [ProbeScreen] = []
        for (index, spec) in specs.enumerated() {
            guard let screen = ProbeScreen(spec: spec, index: index), !screens.contains(where: { $0.serial == screen.serial }),
                  !(screen.main && screens.contains(where: \.main)) else {
                throw LaunchModeError.invalidProbeScreen(spec)
            }
            screens.append(screen)
        }
        return screens
    }
}
