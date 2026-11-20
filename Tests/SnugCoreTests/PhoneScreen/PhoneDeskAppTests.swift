import CoreGraphics
import Foundation
import Testing
@testable import SnugCore

private let chat = AppInfo(bundleID: "com.openai.codex", name: "ChatGPT", path: "/Applications/ChatGPT.app")
private let ghosttyApp = AppInfo(bundleID: "com.mitchellh.ghostty", name: "Ghostty", path: "/Applications/Ghostty.app")
private let notesApp = AppInfo(bundleID: "com.apple.Notes", name: "备忘录", path: "/System/Applications/Notes.app")
private let safariApp = AppInfo(bundleID: "com.apple.Safari", name: "Safari", path: "/Applications/Safari.app")
private let chatHome = CGRect(x: 0, y: 33, width: 1512, height: 886)
/// Ghostty 新窗口：铺满主屏（已有全屏窗口时是原生全屏），退出全屏后是默认大小
private let ghosttyNew = CGRect(x: 0, y: 33, width: 1512, height: 949)
private let ghosttyNewWindowed = CGRect(x: 0, y: 350, width: 800, height: 632)

/// 只留下状态里「正在做什么」的变化，连续相同的算一次
@MainActor
private func recordWork(_ desk: PhoneDesk) -> () -> [PhoneDesk.Work?] {
    var works: [PhoneDesk.Work?] = []
    desk.onChange = { state in if works.last != .some(state.work) { works.append(state.work) } }
    return { works }
}

@Suite("App 资源库的动作 PhoneDesk")
@MainActor
struct PhoneDeskAppTests {
    // MARK: 打开、新建窗口

    @Test("启动没在跑的 App：等它的窗口出现在屏上（1.2 秒后才画出来），放上手机屏；先是「正在打开」，窗口出来转成「正在放」")
    func launch() async throws {
        let rig = try Rig()
        await rig.ready()
        rig.apps.willSpawn(chat, .init(id: 30, frame: chatHome, delay: 1.2))
        let works = recordWork(rig.desk)
        let started = rig.timeline.now
        rig.desk.open(chat)
        await rig.desk.idle()
        let pid = try #require(await rig.apps.pid(of: chat))
        let window = WindowInfo(key: WindowKey(pid: pid, id: 30), app: chat.name, bundleID: chat.bundleID, title: "")
        #expect(rig.journal.all == ["open com.openai.codex", "move 30 \(phoneArea.short) ✎", "focus 30"])
        #expect(rig.desk.state.placed?.window == window)
        #expect(rig.record?.original == chatHome)
        #expect(works() == [.opening(chat), .placing(window), nil])
        #expect(rig.timeline.now - started >= 1.2)
    }

    @Test("唤回在跑但没窗口的 App：App 重新显示原来那个窗口（编号不变）也认得出来")
    func reopenSameWindow() async throws {
        let rig = try Rig()
        await rig.ready()
        let key = WindowKey(pid: 600, id: 31)
        rig.apps.run(chat, pid: 600)
        rig.windows.add(key, frame: chatHome, hidden: true)
        rig.apps.willReshow(chat, id: 31)
        rig.desk.open(chat)
        await rig.desk.idle()
        #expect(rig.journal.all == ["open com.openai.codex", "move 31 \(phoneArea.short) ✎", "focus 31"])
        #expect(rig.desk.state.placed?.window.key == key)
    }

    @Test("唤回时窗口是最小化的：在屏上永远等不到，过宽限期直接捞出来放上手机屏，不等满 10 秒")
    func reopenMinimizedWindow() async throws {
        let rig = try Rig()
        await rig.ready()
        let key = WindowKey(pid: 600, id: 31)
        rig.apps.run(chat, pid: 600)
        rig.windows.add(key, frame: chatHome, minimized: true)
        let started = rig.timeline.now
        rig.desk.open(chat)
        await rig.desk.idle()
        #expect(rig.desk.state.placed?.window.key == key)
        #expect(rig.timeline.now - started >= PhoneDesk.hiddenGrace)
        #expect(rig.timeline.now - started < PhoneDesk.windowWait)
    }

    @Test("新建窗口：新窗口放上手机屏、按默认级数放大字号；已经开着的窗口不会被误认成新窗口")
    func newWindow() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(ghosttyApp, pid: 10, windows: [ghostty])
        rig.apps.willSpawn(ghosttyApp, .init(id: 40, frame: ghosttyNew, delay: 0.1))
        rig.desk.newWindow(app)
        await rig.desk.idle()
        #expect(rig.journal.all == ["newwin 10", "move 40 \(phoneArea.short) ✎", "focus 40", "zoom 40 +2 ✎"])
        #expect(rig.windows.window(ghostty.key)?.frame == ghosttyHome)
        #expect(rig.record?.original == ghosttyNew)
    }

    @Test("同一个 App 开着两个进程：新建窗口认点的那个进程原有的窗口，不把它们当成新窗口")
    func newWindowInChosenProcess() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(ghosttyApp, pid: 10, windows: [ghostty])
        rig.apps.run(ghosttyApp, pid: 20)
        rig.apps.willSpawn(ghosttyApp, .init(id: 40, frame: ghosttyNew, delay: 0.3))
        rig.desk.newWindow(app)
        await rig.desk.idle()
        #expect(rig.desk.state.placed?.window.key == WindowKey(pid: 10, id: 40))
        #expect(rig.windows.window(ghostty.key)?.frame == ghosttyHome)
    }

    @Test("新窗口是原生全屏的（App 已有全屏窗口时新窗口也全屏）：先退出全屏再挪，原位记退出全屏后的位置，还原时恢复全屏")
    func newFullScreenWindow() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(ghosttyApp, pid: 10, windows: [ghostty])
        rig.apps.willSpawn(ghosttyApp, .init(id: 40, frame: ghosttyNew, fullScreen: true, windowed: ghosttyNewWindowed))
        rig.desk.newWindow(app)
        await rig.desk.idle()
        #expect(Array(rig.journal.all.prefix(3)) == ["newwin 10", "fullscreen 40 off ✎", "move 40 \(phoneArea.short) ✎"])
        #expect(rig.record?.original == ghosttyNewWindowed)
        #expect(rig.record?.wasFullScreen == true)
    }

    @Test("新窗口开在手机屏上（App 记得上次在手机屏上的位置）：原位改记成主屏居中，屏撤了也放得回去")
    func bornOnPhone() async throws {
        let rig = try Rig()
        await rig.ready()
        let onPhone = CGRect(x: FakeDisplays.origin.x, y: 68, width: 540, height: 488)
        rig.apps.willSpawn(chat, .init(id: 30, frame: onPhone))
        rig.desk.open(chat)
        await rig.desk.idle()
        let main = FakeDisplays.main
        #expect(rig.record?.original == CGRect(x: (main.midX - 270).rounded(), y: (main.midY - 244).rounded(), width: 540, height: 488))
    }

    @Test("菜单里没有「新建窗口」：提示没法新建窗口，什么都不动")
    func noNewWindow() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(notesApp, pid: 13)
        rig.desk.newWindow(app)
        await rig.desk.idle()
        #expect(rig.journal.all == ["newwin 13"])
        #expect(rig.desk.state.notice == .noNewWindow(app: "备忘录"))
        #expect(rig.desk.state.work == nil)
    }

    @Test("打不开（启动失败）：提示，什么都不动")
    func openFailed() async throws {
        let rig = try Rig()
        await rig.ready()
        rig.apps.failLaunches()
        rig.desk.open(chat)
        await rig.desk.idle()
        #expect(rig.journal.all == ["open com.openai.codex"])
        #expect(rig.desk.state.notice == .openFailed(app: "ChatGPT"))
    }

    @Test("启动后又退了（崩溃）：马上提示打不开，不白等 30 秒")
    func launchThenCrash() async throws {
        let rig = try Rig()
        await rig.ready()
        rig.apps.willCrash(chat, after: 2)
        let started = rig.timeline.now
        rig.desk.open(chat)
        await rig.desk.idle()
        #expect(rig.desk.state.notice == .openFailed(app: "ChatGPT"))
        #expect(rig.timeline.now - started < 3)
    }

    @Test("等满了也没有可显示的窗口：冷启动等 30 秒、唤回等 10 秒，然后提示未找到窗口")
    func noWindow() async throws {
        let rig = try Rig()
        await rig.ready()
        var started = rig.timeline.now
        rig.desk.open(chat)
        await rig.desk.idle()
        #expect(rig.desk.state.notice == .noWindow(app: "ChatGPT"))
        #expect(rig.timeline.now - started >= PhoneDesk.launchWait)
        #expect(rig.timeline.now - started < PhoneDesk.launchWait + 1)
        // 现在它在跑了：再点是唤回，只等 10 秒
        started = rig.timeline.now
        rig.desk.open(chat)
        await rig.desk.idle()
        #expect(rig.timeline.now - started >= PhoneDesk.windowWait)
        #expect(rig.timeline.now - started < PhoneDesk.windowWait + 1)
        #expect(rig.journal.all == ["open com.openai.codex", "open com.openai.codex"])
    }

    @Test("等新窗口时远程结束了：不再等，马上撤屏，也不留提示")
    func abandonWaitWhenUnwanted() async throws {
        let rig = try Rig()
        await rig.ready()
        let started = rig.timeline.now
        rig.desk.open(chat)
        rig.desk.setWanted(false)
        await rig.desk.idle()
        #expect(rig.desk.state.screen == .off)
        #expect(rig.desk.state.notice == nil)
        #expect(rig.timeline.now - started < PhoneDesk.windowWait)
    }

    @Test("手机屏还没好：不打开，提示手机屏未就绪")
    func openWithoutScreen() async throws {
        let rig = try Rig()
        rig.desk.open(chat)
        await rig.desk.idle()
        #expect(rig.journal.all.isEmpty)
        #expect(rig.desk.state.notice == .screenNotReady)
    }

    // MARK: 退出 App

    @Test("退出：手机屏上放着它的窗口就先还原（缩回字号、回原位）再请它退出；退完不再盯，记录销账")
    func quitPlaced() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(ghosttyApp, pid: 10, windows: [ghostty])
        await rig.pull(ghostty)
        rig.journal.clear()
        rig.desk.quit(app)
        #expect(rig.desk.state.quitting == [10])
        await rig.desk.idle()
        #expect(rig.journal.all == ["zoom 7 -2 ✎", "move 7 \(ghosttyHome.short) ✎", "quit 10"])
        #expect(rig.desk.state.quitting.isEmpty)
        #expect(rig.desk.state.placed == nil)
        #expect(rig.record == nil)
        #expect(await rig.apps.isRunning(pid: 10) == false)
    }

    @Test("退出时问「要存储吗」：把挂着 sheet 的那个窗口放上手机屏，一直等到答完；存了就退出")
    func quitAsksToSave() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(notesApp, pid: 13, windows: [notes])
        rig.apps.onQuit(notesApp, .asks(notes.key.id, answer: .save(after: 20)))
        let started = rig.timeline.now
        rig.desk.quit(app)
        await rig.desk.idle()
        #expect(Array(rig.journal.all.prefix(3)) == ["quit 13", "minimize 10 off ✎", "move 10 \(phoneArea.short) ✎"])
        #expect(rig.timeline.now - started >= 20)
        #expect(await rig.apps.isRunning(pid: 13) == false)
        #expect(rig.desk.state.quitting.isEmpty)
        #expect(rig.desk.state.notice == nil)
    }

    @Test("几个文稿没存时单独弹「要先检查吗」对话框：放上手机屏，不调字号（按键会被对话框挡下）")
    func quitAsksInDialog() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(ghosttyApp, pid: 10, windows: [ghostty])
        rig.apps.onQuit(ghosttyApp, .asks(50, dialog: true, answer: .save(after: 3)))
        rig.desk.quit(app)
        await rig.desk.idle()
        #expect(rig.journal.all.contains("move 50 \(phoneArea.short) ✎"))
        #expect(!rig.journal.all.contains { $0.hasPrefix("zoom 50") })
        #expect(await rig.apps.isRunning(pid: 10) == false)
    }

    @Test("对话框里点了取消：App 还在，不再盯、不提示")
    func quitCanceled() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(notesApp, pid: 13, windows: [notes])
        rig.apps.onQuit(notesApp, .asks(notes.key.id, answer: .cancel(after: 5)))
        rig.desk.quit(app)
        await rig.desk.idle()
        #expect(await rig.apps.isRunning(pid: 13))
        #expect(rig.desk.state.quitting.isEmpty)
        #expect(rig.desk.state.notice == nil)
        #expect(rig.journal.all.filter { $0.hasPrefix("quit") } == ["quit 13"])
    }

    @Test("退出卡住：等满 10 秒还没退、也没有对话框，提示可以强制退出；强制退出后 App 没了")
    func quitStalls() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(chat, pid: 600)
        rig.apps.onQuit(chat, .stalls)
        let started = rig.timeline.now
        rig.desk.quit(app)
        await rig.desk.idle()
        #expect(rig.desk.state.notice == .quitStalled(app: "ChatGPT", pid: 600))
        #expect(rig.timeline.now - started >= PhoneDesk.quitWait)
        #expect(rig.desk.state.quitting.isEmpty)
        rig.desk.forceQuit(pid: 600, app: "ChatGPT")
        await rig.desk.idle()
        #expect(rig.journal.all == ["quit 600", "force 600"])
        #expect(rig.desk.state.notice == nil)
        #expect(await rig.apps.isRunning(pid: 600) == false)
    }

    @Test("退出请求没送到：不白等 10 秒，马上提示无响应")
    func quitRefused() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(chat, pid: 600)
        rig.apps.onQuit(chat, .refuses)
        let started = rig.timeline.now
        rig.desk.quit(app)
        await rig.desk.idle()
        #expect(rig.desk.state.notice == .windowUnresponsive(app: "ChatGPT"))
        #expect(rig.desk.state.quitting.isEmpty)
        #expect(rig.timeline.now - started < PhoneDesk.quitWait)
    }

    @Test("强制退出请求没送到：提示无响应，不悄悄算了")
    func forceQuitRefused() async throws {
        let rig = try Rig()
        await rig.ready()
        rig.apps.run(chat, pid: 600)
        rig.apps.refuseForceQuit()
        rig.desk.forceQuit(pid: 600, app: "ChatGPT")
        await rig.desk.idle()
        #expect(rig.desk.state.notice == .windowUnresponsive(app: "ChatGPT"))
        #expect(await rig.apps.isRunning(pid: 600))
    }

    @Test("退得慢（15 秒）：先提示可以强制退出，它自己退了提示就撤掉")
    func slowQuitClearsNotice() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(chat, pid: 600)
        rig.apps.onQuit(chat, .slow(after: 15))
        var notices: [PhoneDesk.Notice?] = []
        rig.desk.onChange = { if notices.last != .some($0.notice) { notices.append($0.notice) } }
        rig.desk.quit(app)
        await rig.desk.idle()
        #expect(notices == [nil, .quitStalled(app: "ChatGPT", pid: 600), nil])
        #expect(!rig.journal.all.contains("force 600"))
    }

    @Test("正在退出的 App：再点退出不重复请；它的窗口不放、不新建窗口、不重新打开")
    func quittingAppIsLeftAlone() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(ghosttyApp, pid: 10, windows: [ghostty])
        rig.apps.onQuit(ghosttyApp, .slow(after: 5))
        rig.apps.willSpawn(ghosttyApp, .init(id: 40, frame: ghosttyNew))
        rig.desk.quit(app)
        rig.desk.quit(app)
        rig.desk.pull(ghostty)
        rig.desk.newWindow(app)
        rig.desk.open(ghosttyApp)
        await rig.desk.idle()
        #expect(rig.journal.all == ["quit 10"])
    }

    @Test("看守在别的动作进行中盯到对话框：排在后面放，「正在打开」不会被前一个动作结束时冲掉")
    func askingWaitsItsTurn() async throws {
        let rig = try Rig()
        await rig.ready()
        let notesRunning = rig.apps.run(notesApp, pid: 13, windows: [notes])
        rig.apps.onQuit(notesApp, .asks(notes.key.id, answer: .save(after: 10)))
        rig.apps.willSpawn(chat, .init(id: 30, frame: chatHome, delay: 3))
        let works = recordWork(rig.desk)
        rig.desk.quit(notesRunning)
        rig.desk.open(chat)
        await rig.desk.idle()
        let pid = try #require(await rig.apps.pid(of: chat))
        let chatWindow = WindowInfo(key: WindowKey(pid: pid, id: 30), app: chat.name, bundleID: chat.bundleID, title: "")
        #expect(works() == [nil, .opening(chat), .placing(chatWindow), nil, .placing(notes), nil])
    }

    @Test("退出时手机屏上它的窗口放不回去（App 卡住）：不退，提示无响应")
    func quitWhenStuck() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(ghosttyApp, pid: 10, windows: [ghostty])
        await rig.pull(ghostty)
        rig.windows.blockMoves()
        rig.journal.clear()
        rig.desk.quit(app)
        await rig.desk.idle()
        #expect(!rig.journal.all.contains("quit 10"))
        #expect(rig.desk.state.notice == .windowUnresponsive(app: "Ghostty"))
        #expect(rig.desk.state.quitting.isEmpty)
        #expect(rig.desk.state.placed?.window == ghostty)
    }

    @Test("撤屏时还在盯：不再盯，提示和「正在退出」都不带到下一次")
    func removeScreenStopsWatching() async throws {
        let rig = try Rig()
        await rig.ready()
        let app = rig.apps.run(chat, pid: 600)
        rig.apps.onQuit(chat, .stalls)
        rig.desk.quit(app)
        rig.desk.setWanted(false)
        await rig.desk.idle()
        rig.desk.setWanted(true)
        await rig.desk.idle()
        #expect(rig.desk.state.notice == nil)
        #expect(rig.desk.state.quitting.isEmpty)
    }

    // MARK: 关窗口

    @Test("关窗口：按它的关闭按钮，关上了就好")
    func closeWindow() async throws {
        let rig = try Rig()
        await rig.ready()
        rig.desk.close(finder)
        #expect(rig.desk.state.closing == [finder.key])
        await rig.desk.idle()
        #expect(rig.journal.all == ["close 8"])
        #expect(rig.windows.window(finder.key) == nil)
        #expect(rig.desk.state.closing.isEmpty)
    }

    @Test("关全屏的窗口：先退出全屏再按关闭（全屏时按了没反应）")
    func closeFullScreenWindow() async throws {
        let rig = try Rig()
        await rig.ready()
        rig.desk.close(safari)
        await rig.desk.idle()
        #expect(rig.journal.all == ["fullscreen 9 off ∅", "close 9"])
        #expect(rig.windows.window(safari.key) == nil)
    }

    @Test("关手机屏上的窗口：先缩回字号、挪回原位再关（App 记得关掉的窗口在哪），原来全屏的不再恢复全屏")
    func closePlacedWindow() async throws {
        let rig = try Rig()
        await rig.ready()
        await rig.pull(safari)
        rig.journal.clear()
        rig.desk.close(safari)
        await rig.desk.idle()
        #expect(rig.journal.all == ["move 9 \(safariWindowed.short) ✎", "close 9"])
        #expect(rig.desk.state.placed == nil)
        #expect(rig.record == nil)
        #expect(rig.windows.window(safari.key) == nil)
    }

    @Test("关窗口时问「要存储吗」：放上手机屏等答；存了就关上")
    func closeAsksToSave() async throws {
        let rig = try Rig()
        await rig.ready()
        rig.windows.onClose(finder.key, .asks(after: 4, save: true))
        rig.desk.close(finder)
        await rig.desk.idle()
        #expect(Array(rig.journal.all.prefix(2)) == ["close 8", "move 8 \(phoneArea.short) ✎"])
        #expect(rig.windows.window(finder.key) == nil)
        #expect(rig.desk.state.closing.isEmpty)
        #expect(rig.desk.state.notice == nil)
    }

    @Test("关窗口时对话框里点了取消：窗口留着，不提示")
    func closeCanceled() async throws {
        let rig = try Rig()
        await rig.ready()
        rig.windows.onClose(finder.key, .asks(after: 4, save: false))
        rig.desk.close(finder)
        await rig.desk.idle()
        #expect(rig.windows.window(finder.key) != nil)
        #expect(rig.desk.state.closing.isEmpty)
        #expect(rig.desk.state.notice == nil)
    }

    @Test("按了关闭却一直不关、也不问：等满 5 秒提示无响应")
    func closeStalls() async throws {
        let rig = try Rig()
        await rig.ready()
        rig.windows.onClose(finder.key, .stalls)
        let started = rig.timeline.now
        rig.desk.close(finder)
        await rig.desk.idle()
        #expect(rig.desk.state.notice == .windowUnresponsive(app: "Finder"))
        #expect(rig.timeline.now - started >= PhoneDesk.closeWait)
        #expect(rig.desk.state.closing.isEmpty)
    }

    @Test("App 没响应、按不到关闭按钮：提示无响应，不盯")
    func closeUnreachable() async throws {
        let rig = try Rig()
        await rig.ready()
        rig.windows.block()
        rig.desk.close(finder)
        await rig.desk.idle()
        #expect(rig.desk.state.notice == .windowUnresponsive(app: "Finder"))
        #expect(rig.desk.state.closing.isEmpty)
    }

    // MARK: 告诉数据源

    @Test("退完、关上的那一刻告诉数据源，而且在撤掉「正在退出 / 正在关闭」之前：列表同一帧里去掉它，不以原样闪回")
    func reportsGone() async throws {
        let rig = try Rig()
        await rig.ready()
        let notesRunning = rig.apps.run(notesApp, pid: 13, windows: [notes])
        rig.apps.onQuit(notesApp, .slow(after: 3))
        var gone: [PhoneDesk.Gone] = []
        var goneWhenSettled: [[PhoneDesk.Gone]] = []
        var busy = false
        rig.desk.onGone = { gone.append($0) }
        rig.desk.onChange = { state in
            let now = !state.quitting.isEmpty || !state.closing.isEmpty
            if busy, !now { goneWhenSettled.append(gone) }
            busy = now
        }
        rig.desk.quit(notesRunning)
        await rig.desk.idle()
        rig.desk.close(finder)
        await rig.desk.idle()
        #expect(goneWhenSettled == [[.app(13)], [.app(13), .window(finder.key)]])
    }

    @Test("没退、没关（点了取消、卡住）不告诉数据源")
    func keepsWhenNotGone() async throws {
        let rig = try Rig()
        await rig.ready()
        let notesRunning = rig.apps.run(notesApp, pid: 13, windows: [notes])
        rig.apps.onQuit(notesApp, .asks(notes.key.id, answer: .cancel(after: 2)))
        rig.windows.onClose(finder.key, .stalls)
        var gone: [PhoneDesk.Gone] = []
        rig.desk.onGone = { gone.append($0) }
        rig.desk.quit(notesRunning)
        rig.desk.close(finder)
        await rig.desk.idle()
        #expect(gone.isEmpty)
    }
}
