// AppController 的意图（设置窗口、菜单栏、手机屏上的列表都走这里）和退出收尾
import AppKit
import ServiceManagement
import SnugCore

extension AppController: AppIntents {
    func setPhoneScreen(_ on: Bool) { changeConfig("手机屏 → \(on ? "开" : "关")") { $0.phoneScreen = on } }

    func setOnlyVirtual(_ on: Bool) { changeConfig("仅输出手机屏 → \(on ? "开" : "关")") { $0.onlyVirtual = on } }

    func setLanguage(_ pref: LanguagePref) { changeConfig("界面语言 → \(pref.rawValue)") { $0.language = pref } }

    func setGestures(_ on: Bool) {
        guard changeConfig("手机手势 → \(on ? "开" : "关")", { $0.gestures = on }) else { return }
        syncGestures()
    }

    /// 关掉时把浮着的截图一并收掉
    func setScreenshot(_ on: Bool) {
        guard changeConfig("截图键 → \(on ? "开" : "关")", { $0.screenshot = on }) else { return }
        if on { refreshScreenRecording() } else { screenshots.dismiss() }
    }

    func setZoomSteps(_ steps: Int) {
        let steps = steps.clamped(to: AppConfig.zoomStepsRange)
        guard changeConfig("放大字号 → \(steps) 级", { $0.zoomSteps = steps }) else { return }
        desk.setDefaultZoom(steps)
    }

    /// 打开没成时写明原因：在「登录项」里被关过（系统要用户去那里允许）、不在「应用程序」里，还是别的
    func setLaunchAtLogin(_ on: Bool) {
        var failed = false
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
            Log.info("设置：开机启动 → \(on ? "开" : "关")")
        } catch {
            failed = true
            Log.error("开机启动设置失败（\(Bundle.main.bundlePath)）：\(error)")
        }
        let status = SMAppService.mainApp.status
        let issue: LoginItemIssue? = if on, status == .requiresApproval {
            .needsApproval
        } else if failed {
            Bundle.main.bundlePath.hasPrefix("/Applications/") ? .failed : .notInApplications
        } else {
            nil
        }
        if issue == .needsApproval { Log.warn("开机启动待用户在「登录项」里允许") }
        state.update(\.loginItemIssue, issue)
        refreshLaunchAtLogin()
    }

    func openLoginItemsSettings() { SMAppService.openSystemSettingsLoginItems() }

    func refreshLaunchAtLogin() {
        let enabled = SMAppService.mainApp.status == .enabled
        state.update(\.launchAtLogin, enabled)
        if enabled { state.update(\.loginItemIssue, nil) }
    }

    func markWelcomed() { changeConfig("看过欢迎页") { $0.welcomed = true } }

    /// 记下这次运行的版本，交回是不是刚换了版本（应用内更新或手动替换）。第一次记的不算
    func recordVersion() -> Bool {
        guard let current = Updater.current, state.config.lastVersion != current else { return false }
        let previous = state.config.lastVersion
        changeConfig("运行版本 → \(current)") { $0.lastVersion = current }
        return previous != nil
    }

    func quitLegacyApp() { LegacyApp.quit() }

    func quit() { onQuitRequested?() }

    func checkForUpdates() { updater.check(userInitiated: true) }

    func installUpdate() { updater.install() }

    /// 换了更新源，上一个源连不上之类的提示就不作数了
    func setUpdateSource(_ source: UpdateSource) {
        guard changeConfig("更新源 → \(source.title)", { $0.updateSource = source }) else { return }
        state.update.problem = nil
    }

    func openDownloadPage() { NSWorkspace.shared.open(state.config.updateSource.downloadPage) }

    func setPhone(_ phone: PhoneChoice) {
        let fit = ScreenFit(phone: phone.screen)
        guard changeConfig("手机型号 → \(phone.title) \(phone.screen.sizeText)（手机屏 \(fit)）", { $0.phone = phone }) else { return }
        desk.setFit(fit)
    }

    // MARK: 退出收尾

    /// 停止监控，放回窗口、撤掉手机屏。timeout 内没收完也照样回调：手机屏会随进程退出消失，
    /// 窗口位置和字号留给下次启动按还原记录处理。超时用加进 .common 的 Timer，退出流程的 run loop 模式下也会触发
    func finishForQuit(timeout: TimeInterval, completion: @escaping @MainActor () -> Void) {
        state.quitting = true
        stopMonitoring()
        Log.info("退出收尾开始（最多等 \(Int(timeout))s）")
        var done = false
        let finish: @MainActor (Bool) -> Void = { clean in
            guard !done else { return }
            done = true
            if !clean { Log.warn("收尾 \(Int(timeout))s 内没做完，先退出；窗口留给下次启动放回") }
            completion()
        }
        Task { @MainActor in
            await desk.shutdown()
            finish(true)
        }
        let timer = Timer(timeInterval: timeout, repeats: false) { _ in
            MainActor.assumeIsolated { finish(false) }
        }
        RunLoop.main.add(timer, forMode: .common)
    }

    // MARK: 配置

    /// 改配置、写盘、通知界面，交回是不是真的改了（值没变就什么都不做，也不打日志）。写盘失败时配置保持原样，界面上提示
    @discardableResult
    func changeConfig(_ what: String, _ change: (inout AppConfig) -> Void) -> Bool {
        do {
            guard try store.update(change) else { return false }
        } catch {
            state.update(\.configSaveFailed, true)
            Log.error("设置（\(what)）写盘失败：\(error)")
            return false
        }
        Log.info("设置：\(what)")
        state.update(\.configSaveFailed, false)
        state.update(\.config, store.config)
        // 语言可能跟着变了：静态镜像和界面观察的是同一份结果，一起同步
        Lang.apply(store.config.language)
        state.update(\.english, Lang.english)
        return true
    }
}

extension AppController: PhoneIntents {
    func pull(_ window: WindowInfo) { desk.pull(window) }

    func open(_ app: AppInfo) {
        switch AppList.action(for: app, running: catalog.running) {
        case .pull(let window): desk.pull(window)
        case .open(let app): desk.open(app)
        }
    }

    func newWindow(_ app: RunningApp) { desk.newWindow(app) }
    func quit(_ app: RunningApp) { desk.quit(app) }
    func forceQuit(pid: Int32, app: String) { desk.forceQuit(pid: pid, app: app) }
    func close(_ window: WindowInfo) { desk.close(window) }
    func dismissNotice() { desk.dismissNotice() }
    func showList() { desk.showList() }
    func returnToPlaced() { desk.returnToPlaced() }
    func putBack() { desk.putBack() }
    func zoom(by delta: Int) { desk.zoom(by: delta) }
    func dismissScreen() { desk.dismissScreen() }

    func takeScreenshot() {
        guard let window = state.phone.placed?.window else { return }
        Task { await screenshots.take(window) }
    }

    func openCrop() { screenshots.openCrop() }
    func cancelCrop() { screenshots.cancelCrop() }
    func finishCrop(_ selection: CGRect) { screenshots.finishCrop(selection) }
}
