// 通过辅助功能接口把 向日葵 的窗口照原样抄成界面树，交给 ControllerTable 认控制端（认哪一页、哪张表都在那里，这里不解释）。
// 只读，不点 向日葵 的任何东西，不为了读表格把 向日葵 的窗口弹出来或切页面：读不到、没认出都照常远程，只是少了一路在场证据和控制端名字（读不到按手机处理）。
// 没有窗口时读不到。
// AX 调用会阻塞（向日葵 卡顿时每次最多等 1 秒），所以这里的函数都是同步的，只在后台队列调用，绝不在主线程。
// 进程号由调用方在主线程查好传进来，后台只碰 AX，不碰 AppKit
import ApplicationServices
import SnugCore
import SnugSystem

enum SunloginAccessibility {
    /// AX 树有环，跳过环之外再限制深度和节点数；超限按「读不到」处理，不拿半截结果当真
    static let maxDepth = 25
    static let maxNodes = 2048

    /// 向日葵 每个窗口的界面树。没有窗口或超出上限时返回 nil
    static func windows(pid: pid_t) -> [AXTreeElement]? {
        let wins = windowElements(pid: pid)
        guard !wins.isEmpty else { return nil }
        var walk = Walk()
        return try? wins.compactMap { try snapshot($0, walk: &walk) }
    }

    // MARK: AX 基础（薄封装在 SnugSystem 的 AX）

    private static func role(_ el: AXUIElement) -> String? { AX.string(el, kAXRoleAttribute) }
    private static func value(_ el: AXUIElement) -> String? { AX.string(el, kAXValueAttribute) }

    /// 窗口列表之外再读主窗口和焦点窗口：别的 App 占着全屏 Space 时，向日葵 的 AXWindows 可能是空的，
    /// 主窗口引用却还有效。按 AX 对象去重，不激活 向日葵、不切 Space（思路来自社区 PR #10，作者 Rongzheng Liu）
    private static func windowElements(pid: pid_t) -> [AXUIElement] {
        let app = AX.application(pid)
        var result = (AX.attribute(app, kAXWindowsAttribute) as? [AXUIElement]) ?? []
        for key in [kAXMainWindowAttribute, kAXFocusedWindowAttribute] {
            guard let raw = AX.attribute(app, key), CFGetTypeID(raw) == AXUIElementGetTypeID() else { continue }
            let window = raw as! AXUIElement
            guard role(window) == kAXWindowRole, !result.contains(where: { CFEqual($0, window) }) else { continue }
            result.append(window)
        }
        return result
    }

    private struct TooLarge: Error {}

    private struct Walk {
        /// 从窗口到当前元素的这一串：再碰到其中一个就是环
        var path = Set<AXUIElement>()
        var count = 0
    }

    /// 先序抄下来。只跳过环，不按对象去重：同一个元素挂在两个父元素下就两处都抄，
    /// 「标题和表格挂在同一个元素下」这层关系不会因为先在别处抄过而丢掉。超过深度或节点上限时抛错
    private static func snapshot(_ node: AXUIElement, walk: inout Walk) throws -> AXTreeElement? {
        guard !walk.path.contains(node) else { return nil }
        walk.count += 1
        guard walk.path.count < maxDepth, walk.count <= maxNodes else { throw TooLarge() }
        walk.path.insert(node)
        defer { walk.path.remove(node) }
        let children = try AX.children(node).compactMap { try snapshot($0, walk: &walk) }
        return AXTreeElement(kind(node), children)
    }

    private static func kind(_ node: AXUIElement) -> AXTreeElement.Kind {
        switch role(node) {
        case kAXStaticTextRole: value(node).map(AXTreeElement.Kind.text) ?? .other
        case kAXTableRole: .table
        case kAXRowRole: .row
        default: .other
        }
    }
}
