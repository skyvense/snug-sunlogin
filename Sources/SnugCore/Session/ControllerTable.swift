// 控制端表格：从 向日葵 窗口的界面树里认出谁在控制本机。AX 遍历在 App 层的 SunloginAccessibility，它只照原样抄树，
// 认哪一页、哪张表、哪几个字都在这里，所以能拿实测的界面树单独测。
// - 只认本机被控页里的表：和「本机被控中」标题直接挂在同一个元素下的那张（向日葵 4.46.0 实测）。
//   点开同账号的另一台设备时，详情页里也有一张一模一样的表，列的是控制那台设备的人：同一个账号的两台 Mac，
//   手机连其中一台，另一台的 向日葵 停在它的详情页，就被当成本机被控、加了手机屏（issue #12）。
//   左边栏顶上也有一张「本机被控中」小卡片，本机被控时一直在，所以只看窗口里有没有这几个字不够：
//   本机被电脑连着、底下还停着别的设备的详情页时，那页的手机会混进来
// - 向日葵 只在被控时才显示这张表，没有「无人」的空态：没被控时界面上什么都不显示，
//   首页、错误页的行，以及远程协助、副屏模式的行，也都不是这里要找的控制端。
//   所以只有认出控制端才算证据，没认出不等于没人（issue #1：主窗口停在首页时，正常远程 60 秒后被误判断开）
import Foundation

/// 向日葵 窗口里的一个界面元素：辅助功能树照原样抄下来，只分出认控制端用得上的几种
public struct AXTreeElement: Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        /// 静态文字
        case text(String)
        case table
        case row
        case other
    }

    public var kind: Kind
    public var children: [AXTreeElement]

    public init(_ kind: Kind, _ children: [AXTreeElement] = []) {
        self.kind = kind
        self.children = children
    }
}

public enum ControllerTable: Equatable, Sendable, CustomStringConvertible {
    /// 没授权、向日葵 没在运行、没有窗口、遍历超限、读得太慢
    case unreadable
    /// 窗口读得到，但没认出控制端行
    case unrecognized
    /// 认出的控制端名称：按表格里的顺序、去重，不为空
    case shown([String])

    /// 本机被控页的标题
    static let pageTitle = "本机被控中"
    /// 控制端行带的连接方式。只认「远程控制」，远程协助、副屏模式的行不算
    static let controlMark = "远程控制"
    /// 控制端行里的连接时长，比如「连接时长 00:10」，不是名字
    static let durationPrefix = "连接时长"

    /// windows：向日葵 每个窗口的界面树
    public init(windows: [AXTreeElement]) {
        self.init(rows: windows.flatMap(Self.localTables).flatMap(\.rows).map(\.texts))
    }

    /// 和「本机被控中」标题直接挂在同一个元素下的表格
    private static func localTables(in element: AXTreeElement) -> [AXTreeElement] {
        let here = element.children.contains { $0.kind == .text(pageTitle) } ? element.children.filter { $0.kind == .table } : []
        return here + element.children.flatMap(localTables)
    }

    /// rows：每一行里的全部静态文字，按界面上的顺序
    init(rows: [[String]]) {
        var names: [String] = []
        for texts in rows where texts.contains(Self.controlMark) {
            let name = texts.first { $0 != Self.controlMark && !$0.hasPrefix(Self.durationPrefix) && !$0.isEmpty }
            if let name, !names.contains(name) { names.append(name) }
        }
        self = names.isEmpty ? .unrecognized : .shown(names)
    }

    /// 认出的控制端；读不到、没认出时为空
    public var names: [String] {
        if case .shown(let names) = self { names } else { [] }
    }

    public var description: String {
        switch self {
        case .unreadable: "读不到"
        case .unrecognized: "未识别"
        case .shown(let names): names.joined(separator: ",")
        }
    }
}

private extension AXTreeElement {
    /// 里面的行（含自己）
    var rows: [AXTreeElement] { (kind == .row ? [self] : []) + children.flatMap(\.rows) }

    /// 里面的全部静态文字，按界面上的顺序
    var texts: [String] {
        let own: [String] = if case .text(let text) = kind { [text] } else { [] }
        return own + children.flatMap(\.texts)
    }
}
