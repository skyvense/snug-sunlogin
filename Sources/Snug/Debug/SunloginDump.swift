// --dump-sunlogin：打印从 向日葵 读到的东西和此刻的屏幕。只读，不点 向日葵、不碰屏幕
import CoreGraphics
import Foundation
import SnugCore
import SnugSystem

@MainActor
enum SunloginDump {
    static func run() -> Int32 {
        print("覆盖窗口：\(SunloginApp.overlayVisible() ? "在屏" : "没有")")
        printDisplays()
        guard AX.isTrusted else {
            print("没有辅助功能权限，读不到 向日葵 的界面")
            return 1
        }
        guard let pid = SunloginApp.pid() else {
            print("向日葵 没有在运行")
            return 1
        }
        guard let windows = SunloginAccessibility.windows(pid: pid) else {
            print("界面：读不到（向日葵 没有窗口、没响应，或界面超出遍历上限）→ 控制端按手机处理")
            return 0
        }
        // 打出界面树里的文字、表格和行：没认出控制端时，看 向日葵 停在哪一页
        print("界面：")
        for window in windows { printTree(window, depth: 1) }
        guard case .shown(let names) = ControllerTable(windows: windows) else {
            print("控制端：未识别（只认本机被控页里「本机被控中」那张表：远程中出现这一行，多半是 向日葵 停在首页、错误页、别的设备的详情页，或收起了被控页；"
                + "上面确实有本机被控页和它的表，才是认不出 向日葵 的界面）→ 按手机处理")
            return 0
        }
        print("控制端：")
        for name in names { print("  \(name)：\(ControllerKind(name: name) == .computer ? "电脑" : "手机、平板或认不出")") }
        print(ControllerKind.wantsPhoneScreen(names) ? "→ 有一台不是电脑，加手机屏" : "→ 全是电脑，不加")
        return 0
    }

    /// 只打文字、表格和行，别的元素不占一层缩进
    private static func printTree(_ element: AXTreeElement, depth: Int) {
        let label: String? = switch element.kind {
        case .text(let text): text.isEmpty ? nil : "「\(text)」"
        case .table: "表格"
        case .row: "行"
        case .other: nil
        }
        if let label { print(String(repeating: "  ", count: depth) + label) }
        for child in element.children { printTree(child, depth: label == nil ? depth : depth + 1) }
    }

    private static func printDisplays() {
        var count: UInt32 = 0
        CGGetOnlineDisplayList(0, nil, &count)
        var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
        CGGetOnlineDisplayList(count, &ids, &count)
        print("屏幕：")
        for id in ids.prefix(Int(count)) {
            let mine = CGDisplayVendorNumber(id) == PhoneScreenSpec.vendorID ? "（Snug 手机屏）" : ""
            print("  #\(id) \(Screens.name(of: id))\(mine) \(DisplayMode.describe(id))")
        }
    }
}
