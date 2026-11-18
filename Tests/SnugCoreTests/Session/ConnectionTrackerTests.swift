import Foundation
import Testing
@testable import SnugCore

/// 时间都写成「距 t0 的秒数」，断言里的数字能直接对上 1.5s 防抖、5s 静默期

private let none = SunloginSignals(overlayVisible: false, controllers: .unrecognized, captureActive: false)
private let overlay = SunloginSignals(overlayVisible: true, controllers: .unreadable, captureActive: nil)
private func capture(_ on: Bool?, _ table: ControllerTable = .unreadable) -> SunloginSignals {
    SunloginSignals(overlayVisible: false, controllers: table, captureActive: on)
}

@Suite("连接判定 ConnectionTracker")
struct ConnectionTrackerTests {
    @Test("常量：防抖 1.5s、自扰静默期 5s")
    func constants() {
        #expect(ConnectionTracker.disconnectDebounce == 1.5)
        #expect(ConnectionTracker.quietWindow == 5)
    }

    @Test("首连不做确认延迟：第一次看到在场就连入，带上认出的全部控制端")
    func immediateConnect() {
        var t = ConnectionTracker()
        #expect(t.update(none, now: at(0)) == nil)
        #expect(t.update(table("iPhone", "iPad"), now: at(0.5)) == .connected(controllers: ["iPhone", "iPad"]))
        #expect(t.isConnected)
        #expect(t.controllers == ["iPhone", "iPad"])

        var o = ConnectionTracker()
        #expect(o.update(overlay, now: at(0)) == .connected(controllers: []))
        var c = ConnectionTracker()
        #expect(c.update(capture(true), now: at(0)) == .connected(controllers: []))
    }

    @Test("断开防抖：最后一次在场之后超过 1.5s 才判断开")
    func debounce() {
        var t = ConnectionTracker()
        _ = t.update(table("iPhone"), now: at(0))
        #expect(t.update(none, now: at(0.5)) == nil)
        #expect(t.update(none, now: at(1.0)) == nil)
        #expect(t.update(none, now: at(1.5)) == nil)
        #expect(t.update(none, now: at(1.6)) == .disconnected)
        #expect(!t.isConnected)
        #expect(t.controllers.isEmpty)
        // 断开后再出现立刻连入
        #expect(t.update(overlay, now: at(1.7)) == .connected(controllers: []))
    }

    @Test("中途短暂缺席（<1.5s）不断开")
    func shortGap() {
        var t = ConnectionTracker()
        _ = t.update(overlay, now: at(0))
        #expect(t.update(none, now: at(1.0)) == nil)
        #expect(t.update(overlay, now: at(1.4)) == nil)
        #expect(t.update(none, now: at(2.8)) == nil)
        #expect(t.update(none, now: at(2.95)) == .disconnected)
    }

    @Test("自扰静默期：本工具引起扰动后 5s 内不判断开，过了才判")
    func quietWindow() {
        var t = ConnectionTracker()
        _ = t.update(table("iPhone"), now: at(0))
        t.noteSelfInducedChange(at: at(0.2))
        #expect(t.update(none, now: at(2.0)) == nil)   // 防抖已满足，但还在静默期
        #expect(t.update(none, now: at(5.2)) == nil)   // 0.2 + 5 = 5.2，还没「过」
        #expect(t.update(none, now: at(5.3)) == .disconnected)
    }

    @Test("静默期不拖慢连入")
    func quietDoesNotDelayConnect() {
        var t = ConnectionTracker()
        t.noteSelfInducedChange(at: at(0))
        #expect(t.update(overlay, now: at(0.1)) == .connected(controllers: []))
    }

    @Test("认出控制端后表格读不到、只剩推流：不断开、名字保留；停流后从最后一次在场起 1.5s 判断开")
    func captureAloneAfterNameKeepsName() {
        var t = ConnectionTracker()
        _ = t.update(capture(true, .shown(["iPhone"])), now: at(0))
        for s in stride(from: 0.5, through: 10.0, by: 0.5) {
            #expect(t.update(capture(true), now: at(s)) == nil)
        }
        #expect(t.controllers == ["iPhone"])
        #expect(t.update(capture(false), now: at(10.5)) == nil)
        #expect(t.update(capture(false), now: at(11.5)) == nil)   // 距最后在场 10.0 恰 1.5
        #expect(t.update(capture(false), now: at(11.6)) == .disconnected)
    }

    @Test("推流在就一直连着：表格读不到、没认出控制端都不算缺席，不设时长上限；停流后照常 1.5s 判断开",
          arguments: [ControllerTable.unreadable, .unrecognized])
    func captureKeepsConnection(table: ControllerTable) {
        var t = ConnectionTracker()
        #expect(t.update(capture(true, table), now: at(0)) == .connected(controllers: []))
        for s in [0.5, 59.5, 60.1, 300] {
            #expect(t.update(capture(true, table), now: at(s)) == nil)
        }
        #expect(t.isConnected)
        #expect(t.update(capture(false, table), now: at(300.5)) == nil)
        #expect(t.update(capture(false, table), now: at(301.5)) == nil)   // 距最后在场 300 恰 1.5
        #expect(t.update(capture(false, table), now: at(301.6)) == .disconnected)
    }

    @Test("认出过控制端、之后没认出（主窗口切走了），推流还在：不断开，名字保留")
    func tableGoesUnrecognizedMidSession() {
        var t = ConnectionTracker()
        #expect(t.update(capture(true, .shown(["iPhone"])), now: at(0)) == .connected(controllers: ["iPhone"]))
        for s in stride(from: 0.5, through: 120.0, by: 0.5) {
            #expect(t.update(capture(true, .unrecognized), now: at(s)) == nil)
        }
        #expect(t.isConnected)
        #expect(t.controllers == ["iPhone"])
    }

    @Test("控制端：先没有后出现、或换人，发 controllersChanged；读不到、没认出时保留原来的")
    func controllerChanges() {
        var t = ConnectionTracker()
        #expect(t.update(overlay, now: at(0)) == .connected(controllers: []))
        #expect(t.update(SunloginSignals(overlayVisible: true, controllers: .shown(["iPad"]), captureActive: nil), now: at(0.5)) == .controllersChanged(["iPad"]))
        #expect(t.update(SunloginSignals(overlayVisible: true, controllers: .shown(["iPad"]), captureActive: nil), now: at(1.0)) == nil)
        #expect(t.update(SunloginSignals(overlayVisible: true, controllers: .shown(["iPhone"]), captureActive: nil), now: at(1.5)) == .controllersChanged(["iPhone"]))
        #expect(t.update(SunloginSignals(overlayVisible: true, controllers: .unrecognized, captureActive: nil), now: at(2.0)) == nil)
        #expect(t.update(SunloginSignals(overlayVisible: true, controllers: .unreadable, captureActive: nil), now: at(2.5)) == nil)
        #expect(t.controllers == ["iPhone"])
    }

    @Test("几台同时连着（issue #3）：有人加入、离开都发 controllersChanged；只是表格里换了顺序不算变")
    func severalControllers() {
        var t = ConnectionTracker()
        #expect(t.update(table("MacBook Pro"), now: at(0)) == .connected(controllers: ["MacBook Pro"]))
        #expect(t.update(table("MacBook Pro", "iPhone"), now: at(0.5)) == .controllersChanged(["MacBook Pro", "iPhone"]))
        #expect(t.update(table("iPhone", "MacBook Pro"), now: at(1.0)) == nil)
        #expect(t.controllers == ["MacBook Pro", "iPhone"])
        #expect(t.update(table("MacBook Pro"), now: at(1.5)) == .controllersChanged(["MacBook Pro"]))
        #expect(t.controllers == ["MacBook Pro"])
    }

    @Test("信号快照写进日志用的描述：区分读不到 / 未识别 / 具体名字")
    func snapshotDescription() {
        #expect(SunloginSignals(overlayVisible: true, controllers: .unreadable, captureActive: nil).description == "overlay=是 控制端=读不到 capture=未知")
        #expect(SunloginSignals(overlayVisible: false, controllers: .unrecognized, captureActive: false).description == "overlay=否 控制端=未识别 capture=否")
        #expect(SunloginSignals(overlayVisible: false, controllers: .shown(["iPhone", "iPad"]), captureActive: true).description == "overlay=否 控制端=iPhone,iPad capture=是")
    }

    @Test("在场只认正面证据：覆盖窗口、认出控制端、推流进行中，任一路就算；读不到、没认出都不算")
    func presenceNeedsPositiveEvidence() {
        #expect(SunloginSignals(overlayVisible: true, controllers: .unreadable, captureActive: nil).isPresent)
        #expect(SunloginSignals(overlayVisible: false, controllers: .shown(["iPhone"]), captureActive: nil).isPresent)
        #expect(SunloginSignals(overlayVisible: false, controllers: .unrecognized, captureActive: true).isPresent)
        for table in [ControllerTable.unreadable, .unrecognized] {
            for captureActive in [nil, false] as [Bool?] {
                #expect(!SunloginSignals(overlayVisible: false, controllers: table, captureActive: captureActive).isPresent)
            }
        }
    }
}
