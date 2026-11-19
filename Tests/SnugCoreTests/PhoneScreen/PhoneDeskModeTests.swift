import CoreGraphics
import Foundation
import Testing
@testable import SnugCore

// 加屏、撤屏不动别的屏的模式（issue #13）。macOS 按「这组屏幕」各记一份模式，屏幕组合一变就把别的屏拨回它给新组合记的那份：
// 远程期间主屏的模式被临时改过（报告人那里是 30Hz，谁改的未确认），系统记在「没有手机屏的那组」名下；撤手机屏时组合变回去，主屏就被拨回去

private let redmi60 = ScreenMode(ioModeID: 12, width: 2560, height: 1440, pixelWidth: 2560, pixelHeight: 1440, refreshRate: 60)
private let redmi30 = ScreenMode(ioModeID: 13, width: 2560, height: 1440, pixelWidth: 2560, pixelHeight: 1440, refreshRate: 30)
private let hp = ScreenMode(ioModeID: 5, width: 1080, height: 1920, pixelWidth: 1080, pixelHeight: 1920, refreshRate: 60)
/// 同一块竖屏被转回横向后的样子（宽高互换）
private let hpLandscape = ScreenMode(ioModeID: 6, width: 1920, height: 1080, pixelWidth: 1920, pixelHeight: 1080, refreshRate: 60)
private let builtIn = ScreenMode(ioModeID: 54, width: 1512, height: 982, pixelWidth: 3024, pixelHeight: 1964, refreshRate: 120)

@Suite("加屏、撤屏不动别的屏 PhoneDesk")
@MainActor
struct PhoneDeskModeTests {
    @Test("远程结束后撤屏，macOS 把别的屏拨回它给这组屏幕记的模式（远程期间临时改的 30Hz）：先设回撤屏前的样子、连永久配置一起写，再放回窗口；没被拨动的不碰")
    func retireKeepsOtherModes() async throws {
        let rig = try Rig()
        rig.displays.others = [1: redmi60, 3: hp]
        await rig.ready()
        rig.displays.onRetire = { [unowned displays = rig.displays] in displays.others[1] = redmi30 }
        rig.desk.setWanted(false)
        await rig.desk.idle()
        #expect(rig.journal.all == ["snapshot", "retire 101", "mode 1 \(redmi60) 永久", "restore"])
        #expect(rig.displays.others == [1: redmi60, 3: hp])
    }

    @Test("撤屏时竖屏被转回横向：模式和旋转一起设回撤屏前的样子（先模式、再旋转），连永久配置一起写")
    func retireKeepsRotation() async throws {
        let rig = try Rig()
        rig.displays.others = [1: hp]
        rig.displays.rotations = [1: 90]
        await rig.ready()
        rig.displays.onRetire = { [unowned displays = rig.displays] in
            displays.others[1] = hpLandscape
            displays.rotations[1] = 0
        }
        rig.desk.setWanted(false)
        await rig.desk.idle()
        #expect(rig.journal.all == ["snapshot", "retire 101", "mode 1 \(hp) 永久", "rotation 1 90.0 永久", "restore"])
        #expect(rig.displays.others == [1: hp])
        #expect(rig.displays.rotations == [1: 90])
    }

    @Test("只有旋转被拨动、模式没变时：只设旋转，不碰模式（假系统里单独拨旋转，隔离这条路径）")
    func retireKeepsRotationOnly() async throws {
        let rig = try Rig()
        rig.displays.others = [1: redmi60]
        rig.displays.rotations = [1: 90]
        await rig.ready()
        rig.displays.onRetire = { [unowned displays = rig.displays] in displays.rotations[1] = 0 }
        rig.desk.setWanted(false)
        await rig.desk.idle()
        #expect(rig.journal.all == ["snapshot", "retire 101", "rotation 1 90.0 永久", "restore"])
        #expect(rig.displays.rotations == [1: 90])
    }

    @Test("显示器睡着时（Mac 锁屏）撤的屏要等唤醒才下线：下线前不动，下线那一刻 macOS 才拨动别的屏，这时再设回、放回窗口")
    func retireWaitsUntilOffline() async throws {
        let rig = try Rig()
        rig.displays.others = [1: redmi60]
        await rig.ready()
        rig.displays.staysOnline = true
        rig.displays.onRetire = { [unowned displays = rig.displays] in displays.others[1] = redmi30 }
        rig.desk.setWanted(false)
        await rig.desk.idle()
        #expect(rig.journal.all == ["snapshot", "retire 101"])
        #expect(rig.desk.state.screen == .off)
        rig.desk.settleDisplays()
        await rig.desk.idle()
        #expect(rig.journal.all == ["snapshot", "retire 101"])

        rig.displays.goOffline(101)
        rig.desk.settleDisplays()
        await rig.desk.idle()
        #expect(rig.journal.all == ["snapshot", "retire 101", "mode 1 \(redmi60) 永久", "restore"])
        rig.desk.settleDisplays()
        await rig.desk.idle()
        #expect(rig.journal.all.filter { $0.hasPrefix("mode") }.count == 1)
    }

    @Test("旧屏还没下线又加了屏：不再等它，它下线时不按旧的样子去设")
    func newScreenSupersedesRetiring() async throws {
        let rig = try Rig()
        rig.displays.others = [1: redmi60]
        await rig.ready()
        rig.displays.staysOnline = true
        rig.desk.setWanted(false)
        await rig.desk.idle()
        rig.desk.setWanted(true)
        await rig.desk.idle()
        rig.displays.onRetire = { [unowned displays = rig.displays] in displays.others[1] = redmi30 }
        rig.displays.goOffline(101)
        rig.desk.settleDisplays()
        await rig.desk.idle()
        #expect(!rig.journal.all.contains { $0.hasPrefix("mode") })
    }

    @Test("远程中撤屏（手机上退出手机屏）：撤屏前可能是远程用的临时模式，只对本次登录设回；远程结束、屏都醒着时，把别的屏此刻的模式写进永久配置")
    func retireDuringRemotePersistsAfterwards() async throws {
        let rig = try Rig()
        rig.displays.others = [1: redmi30, 3: hp]
        rig.desk.setRemote(true)
        await rig.ready()
        rig.displays.onRetire = { [unowned displays = rig.displays] in displays.others[1] = redmi60 }
        rig.desk.dismissScreen()
        await rig.desk.idle()
        #expect(rig.journal.all.contains("mode 1 \(redmi30)"))
        #expect(!rig.journal.all.contains { $0.hasSuffix("永久") })
        rig.desk.settleDisplays()
        await rig.desk.idle()
        #expect(!rig.journal.all.contains { $0.hasSuffix("永久") })

        // 远程结束，向日葵 把主屏还原成 60Hz（只对本次登录）；屏还睡着时等着
        rig.journal.clear()
        rig.displays.others[1] = redmi60
        rig.displays.asleep = [1, 3]
        rig.desk.setRemote(false)
        rig.desk.settleDisplays()
        await rig.desk.idle()
        #expect(rig.journal.all.isEmpty)
        rig.displays.asleep = []
        rig.desk.settleDisplays()
        await rig.desk.idle()
        #expect(rig.journal.all == ["mode 1 \(redmi60) 永久", "mode 3 \(hp) 永久"])
        rig.desk.settleDisplays()
        await rig.desk.idle()
        #expect(rig.journal.all.count == 2)
    }

    @Test("加屏时也一样：别的屏被拨回这组屏幕上次记的模式，加好后设回加屏前的样子；远程中可能是临时模式，只对本次登录设回")
    func createKeepsOtherModes() async throws {
        let rig = try Rig()
        rig.displays.others = [1: redmi30, 3: hp]
        rig.displays.onCreate = { [unowned displays = rig.displays] in displays.others[1] = redmi60 }
        rig.desk.setWanted(true)
        await rig.desk.idle()
        #expect(rig.journal.all == ["snapshot", "create", "mode 1 \(redmi30)", "restore"])
        #expect(rig.displays.others == [1: redmi30, 3: hp])
    }

    @Test("系统按分辨率、缩放、刷新率记模式，不记编号：编号重排了、属性没变不算拨动")
    func renumberedIsSame() async throws {
        let rig = try Rig()
        rig.displays.others = [1: redmi60]
        await rig.ready()
        var renumbered = redmi60
        renumbered.ioModeID = 99
        rig.displays.onRetire = { [unowned displays = rig.displays] in displays.others[1] = renumbered }
        rig.desk.setWanted(false)
        await rig.desk.idle()
        #expect(!rig.journal.all.contains { $0.hasPrefix("mode") })
        #expect(redmi60.isSame(as: renumbered))
        #expect(!redmi60.isSame(as: redmi30))
        var hiDPI = redmi60
        hiDPI.width = 1280
        hiDPI.height = 720
        #expect(!redmi60.isSame(as: hiDPI))
    }

    @Test("这期间拔掉的屏、新接上的屏都不碰；都没变就什么都不做")
    func onlyScreensPresentThroughout() async throws {
        let rig = try Rig()
        rig.displays.others = [1: redmi60, 3: hp]
        await rig.ready()
        rig.displays.onRetire = { [unowned displays = rig.displays] in
            displays.others[3] = nil
            displays.others[4] = builtIn
        }
        rig.desk.setWanted(false)
        await rig.desk.idle()
        #expect(!rig.journal.all.contains { $0.hasPrefix("mode") })

        let calm = try Rig()
        calm.displays.others = [1: redmi60]
        calm.desk.setWanted(true)
        await calm.desk.idle()
        calm.desk.setWanted(false)
        await calm.desk.idle()
        #expect(!calm.journal.all.contains { $0.hasPrefix("mode") })
    }

    @Test("设不回去（帮手失败）只记日志，照样撤完屏")
    func modeFailureDoesNotBlock() async throws {
        let rig = try Rig()
        rig.displays.others = [1: redmi60]
        rig.displays.modeFails = true
        await rig.ready()
        rig.displays.onRetire = { [unowned displays = rig.displays] in displays.others[1] = redmi30 }
        rig.desk.setWanted(false)
        await rig.desk.idle()
        #expect(rig.journal.all.contains("mode 1 \(redmi60) 永久"))
        #expect(rig.desk.state.screen == .off)
    }

    @Test("模式在日志里写成点数、几倍、刷新率")
    func describesMode() {
        #expect("\(redmi30)" == "2560x1440 30Hz")
        #expect("\(builtIn)" == "1512x982（2 倍）120Hz")
        #expect("\(ScreenMode(ioModeID: 1, width: 1920, height: 1080, pixelWidth: 1920, pixelHeight: 1080, refreshRate: 59.94))" == "1920x1080 59.94Hz")
    }
}
