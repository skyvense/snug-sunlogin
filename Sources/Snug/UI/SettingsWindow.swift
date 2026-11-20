// 设置窗口：macOS App 设置窗口的标准做法，顶部工具栏标签页「通用 / 关于」（关于页见 AboutView）。
// 通用页：顶部是此刻的状态，需要处理的问题每个一行、各带解决它的按钮；下面是总开关、手机型号和几项微调，再下面是开机启动和权限。
// 每行配 SF Symbols 图标，说明只用短词。只读 AppState，意图走 AppIntents
import AppKit
import PermissionFlow
import SwiftUI
import SnugCore

struct SettingsView: View {
    let state: AppState
    let intents: any AppIntents
    let navigation: any Navigation

    var body: some View {
        Form {
            Section { StatusHeader(state: state, navigation: navigation) }
            if !issues.isEmpty {
                Section {
                    ForEach(issues) { IssueRow(issue: $0) }
                }
            }
            Section {
                Toggle(isOn: binding(\.config.phoneScreen, intents.setPhoneScreen)) {
                    SettingLabel("手机屏", detail: "向日葵 中增加竖屏", symbol: "iphone", tint: .blue)
                }
                LabeledContent {
                    PhoneMenu(state: state, choose: intents.setPhone)
                } label: {
                    SettingLabel("手机型号", detail: "手机屏按它调整比例", symbol: "iphone.gen3", tint: .indigo)
                }
                .disabled(!state.config.phoneScreen)
                Toggle(isOn: binding(\.config.gestures, intents.setGestures)) {
                    SettingLabel("触控手势", detail: "向日葵下暂不可用", symbol: "hand.draw.fill", tint: .orange)
                }
                .disabled(true) // 手势在向日葵下有异常，修好前不让开启（配置层也强制关闭）
                Toggle(isOn: binding(\.config.screenshot, intents.setScreenshot)) {
                    SettingLabel("截图", detail: "指示条左侧的相机键 · 截当前窗口到剪贴板", symbol: "camera.viewfinder", tint: .pink)
                }
                .disabled(!state.config.phoneScreen)
                LabeledContent {
                    ZoomSlider(steps: binding(\.config.zoomSteps, intents.setZoomSteps))
                } label: {
                    SettingLabel("字号放大", detail: "终端、编辑器", symbol: "textformat.size", tint: .green)
                }
                .disabled(!state.config.phoneScreen)
            }
            Section {
                Toggle(isOn: binding(\.launchAtLogin, intents.setLaunchAtLogin)) {
                    SettingLabel("开机启动", symbol: "power", tint: .gray)
                }
                // 没授权时「授权…」在顶部状态（辅助功能）或问题行（录屏）里，这里只写状态
                LabeledContent {
                    PermissionStatus(granted: state.accessibilityTrusted)
                } label: {
                    SettingLabel.accessibility
                }
                if state.config.phoneScreen, state.config.screenshot {
                    LabeledContent {
                        PermissionStatus(granted: state.screenRecordingTrusted)
                    } label: {
                        SettingLabel.screenRecording
                    }
                }
            } footer: {
                HStack {
                    Spacer()
                    Button("退出 Snug") { intents.quit() }
                        .help("还原窗口后退出")
                }
                .padding(.top, 8)
            }
        }
        .formStyle(.grouped)
        .frame(width: SettingsWindowController.width)
        .fixedSize(horizontal: false, vertical: true)
    }

    /// 需要用户处理的问题，和解决它的按钮（有的话）
    private var issues: [Issue] {
        var list: [Issue] = []
        if state.legacyAppRunning {
            list.append(Issue(id: "legacy", text: StatusText(headline: "旧版「UU远程伴侣」运行中", detail: "与 Snug 同时运行将相互干扰"),
                              action: .run("退出旧版", intents.quitLegacyApp)))
        }
        if let issue = state.loginItemIssue {
            list.append(Issue(id: "login", text: issue.text,
                              action: issue == .needsApproval ? .run("前往允许", intents.openLoginItemsSettings) : nil))
        }
        if state.config.phoneScreen, state.config.screenshot, !state.screenRecordingTrusted {
            list.append(Issue(id: "screen-recording",
                              text: StatusText(headline: "截图需要\(PermissionName.screenRecording)权限", detail: "系统设置 › 隐私与安全性"),
                              action: .authorize(.screenRecording)))
        }
        if state.configSaveFailed {
            list.append(Issue(id: "config", text: StatusText(headline: "设置未保存", detail: "修改未生效，请重试"), action: nil))
        }
        return list
    }

    /// 控件的绑定：取值时现读 AppState，不在刷新时存一份副本。副本在点下去之后、界面刷新之前还是旧值，
    /// 控件会先弹回旧值再变过去，一次操作成了两次变化；这次操作若让行增减（如「截图」），
    /// 窗口按两次变化各改一回尺寸，叠出多余的高度或塌成 0，整页空白
    private func binding<T>(_ keyPath: KeyPath<AppState, T>, _ set: @escaping @MainActor (T) -> Void) -> Binding<T> {
        Binding(get: { state[keyPath: keyPath] }, set: { set($0) })
    }
}

/// 顶部：此刻的状态；缺权限时主按钮「授权…」就在这里
private struct StatusHeader: View {
    let state: AppState
    let navigation: any Navigation

    var body: some View {
        let text = StatusText(state)
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 20, weight: .semibold))
                .foregroundStyle(.white)
                .frame(width: 40, height: 40)
                .background(state.statusColor.gradient, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            VStack(alignment: .leading, spacing: 4) {
                Text(text.headline).font(.headline)
                if let detail = text.detail { Text(detail).foregroundStyle(.secondary) }
            }
            Spacer(minLength: 12)
            if !state.accessibilityTrusted {
                Button("授权…") { navigation.openWelcome() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(.vertical, 4)
    }

    private var symbol: String {
        if !state.accessibilityTrusted { return "hand.raised.fill" }
        switch state.connection {
        case .idle: return "iphone.gen3"
        case .reconnecting: return "arrow.triangle.2.circlepath"
        case .connected: return "iphone.gen3.radiowaves.left.and.right"
        }
    }
}

private struct Issue: Identifiable {
    enum Action {
        case run(String, @MainActor () -> Void)
        /// 缺权限：「授权…」打开系统设置对应的隐私页（见 AuthorizeButton）
        case authorize(PermissionFlowPane)
    }

    let id: String
    let text: StatusText
    let action: Action?
}

/// 一个问题一行：警示图标、出了什么事、去哪里处理，右边是解决它的按钮
private struct IssueRow: View {
    let issue: Issue

    var body: some View {
        LabeledContent {
            switch issue.action {
            case .run(let title, let run): Button(title, action: run)
            case .authorize(let pane): AuthorizeButton(pane: pane)
            case nil: EmptyView()
            }
        } label: {
            HStack(spacing: 12) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: 2) {
                    Text(issue.text.headline)
                    if let detail = issue.text.detail { Text(detail).font(.callout).foregroundStyle(.secondary) }
                }
            }
        }
    }
}

/// 手机型号：按品牌分组的菜单，选中的打勾；最后一项自己填分辨率（弹出小窗）。和手机屏上的「手机型号」页是同一份配置
private struct PhoneMenu: View {
    let state: AppState
    let choose: @MainActor (PhoneChoice) -> Void
    @State private var customizing = false

    /// 菜单项的勾也现读 AppState（理由同 SettingsView.binding）
    private var current: PhoneChoice { state.config.phone }

    var body: some View {
        Menu(current.customScreen.map { "\(current.title) \($0.sizeText)" } ?? current.title) {
            ForEach(state.phones.brands) { brand in
                Menu(brand.name) {
                    ForEach(brand.models) { model in
                        Toggle(model.name, isOn: Binding(get: { model.id == current.model },
                                                         set: { _ in choose(PhoneChoice(model)) }))
                    }
                }
            }
            if !state.phones.brands.isEmpty { Divider() }
            Toggle("自定义分辨率…", isOn: Binding(get: { current.model == nil }, set: { _ in customizing = true }))
        }
        .fixedSize()
        .popover(isPresented: $customizing, arrowEdge: .bottom) {
            CustomScreenForm(current: current) { screen in
                customizing = false
                if let screen { choose(PhoneChoice(model: nil, screen: screen)) }
            }
        }
    }
}

/// 自己填分辨率：宽 × 高，横着填也行，比例也行（见 PhoneScreen.custom）
private struct CustomScreenForm: View {
    let current: PhoneChoice
    /// 填好的分辨率；取消为 nil
    let done: (PhoneScreen?) -> Void
    @State private var width: Int?
    @State private var height: Int?

    private var screen: PhoneScreen? { PhoneScreen.custom(width, height) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("自定义分辨率").font(.headline)
            HStack(spacing: 8) {
                TextField("宽", value: $width, format: .number.grouping(.never))
                Text("×").foregroundStyle(.secondary)
                TextField("高", value: $height, format: .number.grouping(.never))
            }
            .textFieldStyle(.roundedBorder)
            .multilineTextAlignment(.center)
            Text(PhoneScreen.customHint).font(.callout).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("取消") { done(nil) }
                    .keyboardShortcut(.cancelAction)
                Button("使用") { done(screen) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(screen == nil)
            }
        }
        .padding(16)
        .frame(width: 280)
        .onAppear {
            guard let screen = current.customScreen else { return }
            width = screen.width
            height = screen.height
        }
    }
}

/// 字号放大：系统「文字大小」那样的带刻度滑块，两端小 A、大 A，每档一格
private struct ZoomSlider: View {
    @Binding var steps: Int

    var body: some View {
        let range = AppConfig.zoomStepsRange
        Slider(value: Binding(get: { Double(steps) }, set: { steps = Int($0.rounded()) }),
               in: Double(range.lowerBound)...Double(range.upperBound), step: 1) {
            Text("字号放大")
        } minimumValueLabel: {
            Image(systemName: "textformat.size.smaller")
        } maximumValueLabel: {
            Image(systemName: "textformat.size.larger")
        }
        .labelsHidden()
        .frame(width: 160)
        .accessibilityValue(steps == 0 ? "不放大" : "+\(steps) 级")
    }
}

/// 设置窗口：两个标签页各是一个 SwiftUI 视图。NSTabViewController 自己会让窗口标题跟着当前页走，
/// 高度跟着当前页的内容（切页、出现提示）变，左上角不动（实测）
@MainActor
final class SettingsWindowController {
    enum Tab: Int {
        case general, about
    }

    static let width: CGFloat = 520

    private let state: AppState
    private let intents: any AppIntents
    private weak var navigation: (any Navigation)?
    private var window: NSWindow?
    private var tabs: NSTabViewController?

    init(state: AppState, intents: any AppIntents, navigation: any Navigation) {
        self.state = state
        self.intents = intents
        self.navigation = navigation
    }

    func show(_ tab: Tab = .general, activating: Bool = true) {
        if window == nil, let navigation {
            let tabs = NSTabViewController()
            tabs.tabStyle = .toolbar
            tabs.addTabViewItem(Self.item("通用", symbol: "gearshape",
                                          SettingsView(state: state, intents: intents, navigation: navigation)))
            tabs.addTabViewItem(Self.item("关于", symbol: "info.circle",
                                          AboutView(state: state, intents: intents, navigation: navigation)))
            // 先选好页再建窗口：窗口按这一页的大小建，居中才是正的
            tabs.selectedTabViewItemIndex = tab.rawValue
            let w = NSWindow(contentViewController: tabs)
            w.styleMask = [.titled, .closable, .miniaturizable]
            w.toolbarStyle = .preference
            w.isReleasedWhenClosed = false
            // 只在建好时居中：之后关了再开，还在用户拖到的地方
            w.center()
            self.tabs = tabs
            window = w
        } else {
            tabs?.selectedTabViewItemIndex = tab.rawValue
        }
        window?.present(activating: activating)
    }

    private static func item(_ label: String, symbol: String, _ view: some View) -> NSTabViewItem {
        let hosting = NSHostingController(rootView: view)
        hosting.sizingOptions = [.preferredContentSize]
        hosting.title = label   // 窗口标题跟着当前页走
        let item = NSTabViewItem(viewController: hosting)
        item.label = label
        item.image = NSImage(systemSymbolName: symbol, accessibilityDescription: label)
        return item
    }
}
