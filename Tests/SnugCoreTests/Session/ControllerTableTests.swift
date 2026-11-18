import Foundation
import Testing
@testable import SnugCore

/// 行文字是照 向日葵 的界面构造的样本（大部分取自 PR #2，作者 Feng GAO）；「远程控制 / 远程协助 / 副屏模式」取自 向日葵 4.46.0 的界面文案；
/// 界面树照 向日葵 4.46.0 实测的结构构造
@Suite("控制端表格 ControllerTable")
struct ControllerTableTests {
    @Test("没认出控制端行就是未识别，不是没人：没有行、首页或错误页的行、远程协助或副屏的行、有标记没名字", arguments: [
        [[String]](),
        [["我的设备", "MacBook Pro"], ["无法连接至服务器", "错误码：2003"]],
        [["iPad", "副屏模式", "连接时长 00:10"], ["iPhone", "远程协助"]],
        [["远程控制", "连接时长 00:10", ""]],
    ])
    func unrecognized(rows: [[String]]) {
        #expect(ControllerTable(rows: rows) == .unrecognized)
    }

    @Test("认出控制端：只认带「远程控制」的行，名字跳过标记、连接时长和空文字，按顺序去重")
    func shown() {
        #expect(ControllerTable(rows: [
            ["我的设备", "MacBook Pro"],
            ["", "连接时长 00:10", "iPhone", "远程控制"],
            ["iPhone", "远程控制"],
            ["iPad", "远程控制", "连接时长 00:20"],
        ]) == .shown(["iPhone", "iPad"]))
    }

    // 照 向日葵 4.46.0 实测的界面树（11-16，手机连本机时读的）：本机被控页叠在窗口最上层，标题和控制端表格直接挂在窗口下；
    // 左边栏顶上另有一张「本机被控中」小卡片，本机被控时一直在，旁边没有表格
    private static func text(_ s: String) -> AXTreeElement { AXTreeElement(.text(s)) }
    private static func row(_ texts: String...) -> AXTreeElement {
        AXTreeElement(.row, [AXTreeElement(.other, [AXTreeElement(.other)] + texts.map(text) + [AXTreeElement(.other)])])
    }
    private static func table(_ rows: AXTreeElement...) -> AXTreeElement { AXTreeElement(.table, rows + [AXTreeElement(.other)]) }
    private static let sidebarCard = AXTreeElement(.other, [AXTreeElement(.other), text("本机被控中"), text("当前有 1 台设备正在控制此电脑"), text("我的设备"), text("全部设备")])
    private static func localPage(_ rows: AXTreeElement...) -> [AXTreeElement] {
        [AXTreeElement(.other), text("本机被控中"), text("00:00:04"), text("当前有 \(rows.count) 台设备正在控制此电脑"), AXTreeElement(.other), text("本账号设备"), AXTreeElement(.table, rows)]
    }
    /// 设备详情页：点开同账号的另一台设备时右边显示的页面，那台设备被控时也列出控制端，行的样子和本机被控页一样（issue #12）
    private static func otherDevicePage(_ rows: AXTreeElement...) -> AXTreeElement {
        AXTreeElement(.other, [text("MacBook Pro"), text("被控中"), text("当前有 \(rows.count) 台设备正在控制此电脑"), text("本账号设备"), AXTreeElement(.table, rows)])
    }
    private static func window(sidebar: [AXTreeElement], main: [AXTreeElement], overlay: [AXTreeElement] = []) -> AXTreeElement {
        AXTreeElement(.other, [AXTreeElement(.other, [AXTreeElement(.other, sidebar), AXTreeElement(.other, main)])] + overlay + [AXTreeElement(.other, [text("全部设备")])])
    }

    @Test("本机被控：只认和「本机被控中」标题平级的那张表（实测的界面树）")
    func localControlledPage() {
        let tree = Self.window(sidebar: [Self.sidebarCard], main: [Self.text("电脑"), Self.text("问剑新葡京"), Self.text("本机")],
                               overlay: Self.localPage(Self.row("iPhone", "远程控制", "连接时长 00:00:04")))
        #expect(ControllerTable(windows: [tree]) == .shown(["iPhone"]))
    }

    @Test("同账号的另一台 Mac 被手机连着、本机没人连：向日葵 停在那台的设备详情页，表里的控制端不是本机的（issue #12）")
    func otherDeviceIsNotLocal() {
        let tree = Self.window(sidebar: [Self.text("我的设备"), Self.text("全部设备")],
                               main: [Self.otherDevicePage(Self.row("iPhone Air", "远程控制", "连接时长 00:01:10"))])
        #expect(ControllerTable(windows: [tree]) == .unrecognized)
    }

    @Test("本机被电脑连着、底下还停着另一台被手机连着的设备详情页：只认本机的，不把那台的手机算进来")
    func localPageOverOtherDevice() {
        let tree = Self.window(sidebar: [Self.sidebarCard],
                               main: [Self.otherDevicePage(Self.row("iPhone Air", "远程控制", "连接时长 00:01:10"))],
                               overlay: Self.localPage(Self.row("MacBook Air", "远程控制", "连接时长 00:00:20")))
        #expect(ControllerTable(windows: [tree]) == .shown(["MacBook Air"]))
    }

    @Test("被控页收起后只剩左边栏的小卡片、没有表：未识别；没有窗口内容也是未识别")
    func collapsedPage() {
        #expect(ControllerTable(windows: [Self.window(sidebar: [Self.sidebarCard], main: [Self.text("电脑")])]) == .unrecognized)
        #expect(ControllerTable(windows: []) == .unrecognized)
    }

    @Test("names 只在认出时有内容；日志描述三态各不相同")
    func namesAndDescription() {
        #expect(ControllerTable.unreadable.names == [])
        #expect(ControllerTable.unrecognized.names == [])
        #expect(ControllerTable.shown(["iPhone", "iPad"]).names == ["iPhone", "iPad"])
        #expect(ControllerTable.unreadable.description == "读不到")
        #expect(ControllerTable.unrecognized.description == "未识别")
        #expect(ControllerTable.shown(["iPhone", "iPad"]).description == "iPhone,iPad")
    }
}
