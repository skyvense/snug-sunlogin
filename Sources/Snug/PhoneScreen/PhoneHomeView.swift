// 手机屏上的「App 资源库」：盖住整块手机屏（连这块屏上的菜单栏一起），看起来像手机上的原生 App。
// 顶部固定一行：左边返回键（放着窗口时才有，那一行一直留着，有没有返回键内容都不跳），右边「退出手机屏」「手机型号」；
// 标题和搜索框跟着列表滚走，分段控件「运行中 / 全部」滚到顶就停住、一直看得见，滚回顶部时页眉完整恢复。
// - 运行中：在跑的 App，窗口挂在各自的 App 下（见 PhoneAppCard）。放着窗口时它的 App 单列在最上面「当前 App」，其余在「其他 App」
// - 全部：本机所有 App 的图标网格，按字典序，正在运行的下方有圆点
// - 搜索：见 PhoneSearchPage；手机型号：见 PhoneModelPage
// - 退出手机屏：页眉里再问一次，讲明窗口会还原、下次远程恢复
// - 正在打开、正在放、正在调整比例、正在退出：整屏讲明是哪个 App、哪个窗口、哪台手机；出了问题：顶部写明出了什么事、怎么办
// 每次回到列表、每次出了问题都从最上面看起。文字只用短词，图标全用 SF Symbols
import AppKit
import SnugCore
import SwiftUI

/// 盖在首页上的页面：搜索、手机型号。打开着时列表面板临时接收键盘（见 PhoneScreenWindows），
/// 两个页都有输入框，query 是打开着的那一页的搜索词
@MainActor
@Observable
final class PhonePages {
    enum Page {
        case search, phoneModel
    }

    private(set) var page: Page?
    var query = ""

    var active: Bool { page != nil }
    /// 去掉首尾空格的搜索词：只打了空格算没搜
    var trimmedQuery: String { query.trimmingCharacters(in: .whitespaces) }

    func open(_ page: Page) {
        self.page = page
        query = ""
    }

    func close() {
        page = nil
        query = ""
    }
}

struct PhoneHomeView: View {
    let state: AppState
    let catalog: AppCatalog
    let pages: PhonePages
    let intents: any PhoneIntents

    @State private var tab: Tab
    @State private var confirming: Confirm?

    /// tab、confirming：渲染样例用
    init(state: AppState, catalog: AppCatalog, pages: PhonePages, intents: any PhoneIntents,
         tab: Tab = .running, confirming: Confirm? = nil) {
        self.state = state
        self.catalog = catalog
        self.pages = pages
        self.intents = intents
        _tab = State(initialValue: tab)
        _confirming = State(initialValue: confirming)
    }

    enum Tab: CaseIterable, Identifiable {
        case running, all
        var id: Self { self }
        var title: String { self == .running ? Lang.tr("运行中", "Running") : Lang.tr("全部", "All") }
    }

    /// 在等再点一下确认的：退出哪个 App、关哪个窗口、退出手机屏。同一时刻只有一个；
    /// 有了别的动作、换了页、收起了列表就作废，防误触要的是连着按两下
    enum Confirm: Equatable {
        case quit(Int32)
        case close(WindowKey)
        case dismissScreen
    }

    /// 退出手机屏的图标：右上角的按钮、确认卡和「正在退出」共用
    static let dismissSymbol = "rectangle.portrait.and.arrow.forward"
    private static let top = "top"
    /// 顶部那一行的左右边距，和下面的内容一样
    private static let rowMargin: CGFloat = 20
    private var phone: PhoneDesk.State { state.phone }

    var body: some View {
        ZStack {
            switch pages.page {
            case .search: SearchPage(catalog: catalog, search: pages, phone: phone, intents: intents)
            case .phoneModel: PhoneModelPage(state: state, pages: pages, intents: intents)
            case nil: home
            }
            if let busy = BusyView.Content(phone.work, catalog: catalog, phone: state.config.phone) {
                BusyView(content: busy).transition(.opacity)
            }
        }
        .animation(.easeOut(duration: 0.2), value: phone.work)
        .background(PhoneStyle.background)
        .onChange(of: phone.listVisible) { confirming = nil }
        .onChange(of: phone.work) { confirming = nil }
        .onChange(of: tab) { confirming = nil }
        .onChange(of: pages.active) { confirming = nil }
    }

    private var home: some View {
        ScrollViewReader { proxy in
            VStack(spacing: 0) {
                backRow(proxy)
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0, pinnedViews: [.sectionHeaders]) {
                        header.id(Self.top)
                        SwiftUI.Section {
                            page.padding(.horizontal, 20).padding(.top, 4).padding(.bottom, 24)
                        } header: {
                            TabPicker(selection: $tab)
                                .padding(.horizontal, 20)
                                .padding(.vertical, 12)
                                .background(PhoneStyle.background)
                        }
                    }
                }
                .scrollIndicators(.never)
                .onChange(of: phone.listVisible) { _, visible in
                    if visible { proxy.scrollTo(Self.top, anchor: .top) }
                }
                // 提示在页眉里：翻到下面时出了问题，滚回去让人看见
                .onChange(of: phone.notice) { _, notice in
                    if notice != nil { withAnimation { proxy.scrollTo(Self.top, anchor: .top) } }
                }
            }
        }
        .disabled(phone.work != nil)
    }

    /// 返回键那一行一直留着：有没有返回键，下面的内容都不跳。右边距由最右的图标按钮自己留（见 navButton）
    private func backRow(_ proxy: ScrollViewProxy) -> some View {
        HStack {
            if let placed = phone.placed {
                BackButton(app: placed.window.app) { intents.returnToPlaced() }
            }
            Spacer(minLength: 12)
            HStack(spacing: 0) {
                // 确认卡在页眉里：每次点都滚回去，翻到下面时再点也看得见
                navButton(Self.dismissSymbol, label: state.tr("退出手机屏", "Exit Phone Screen")) {
                    confirming = .dismissScreen
                    withAnimation { proxy.scrollTo(Self.top, anchor: .top) }
                }
                navButton("iphone.gen3", label: state.tr("手机型号", "Phone Model"), atEdge: true) { pages.open(.phoneModel) }
            }
        }
        .frame(height: PhoneStyle.touchTarget)
        .padding(.leading, Self.rowMargin)
        .padding(.top, 12)
    }

    /// 顶部那一行右边的图标按钮：点按范围 touchTarget 见方，图标居中。
    /// 最右那个（atEdge）把这一行的右边距算进点按范围：图标贴着边距，和型号页的「完成」、搜索页的「取消」对齐，
    /// 点按范围伸到屏幕边上，图标左边不用留一大块空，两个图标才不会隔得太开
    private func navButton(_ symbol: String, label: String, atEdge: Bool = false, action: @escaping () -> Void) -> some View {
        let margin = atEdge ? Self.rowMargin : 0
        return Button(action: action) {
            Image(systemName: symbol)
                .font(PhoneStyle.headline.weight(.regular))
                .foregroundStyle(PhoneStyle.tint)
                .frame(width: PhoneStyle.touchTarget - margin, height: PhoneStyle.touchTarget, alignment: atEdge ? .trailing : .center)
                .padding(.trailing, margin)
        }
        .buttonStyle(PressableStyle())
        .accessibilityLabel(label)
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("App").font(PhoneStyle.largeTitle)
            Button { pages.open(.search) } label: {
                SearchBar { Text(state.tr("搜索 App", "Search apps")).foregroundStyle(.secondary) }
            }
            .buttonStyle(PressableStyle())
            if confirming == .dismissScreen {
                Banner(symbol: Self.dismissSymbol, tint: .red, headline: state.tr("退出手机屏？", "Exit the phone screen?"), detail: state.tr("窗口还原，下次远程时恢复", "Windows will be restored; it comes back next session")) {
                    // 间距和提示卡里的「稍后 / 强制退出」一样
                    ConfirmButtons(title: state.tr("退出", "Exit"), confirming: $confirming, spacing: 12) { intents.dismissScreen() }
                }
            } else if let notice = phone.notice {
                NoticeBanner(notice: notice, intents: intents)
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 4)
    }

    @ViewBuilder private var page: some View {
        switch tab {
        case .running: runningPage
        case .all: AllApps(catalog: catalog, quitting: phone.quitting, intents: intents)
        }
    }

    private var runningPage: some View {
        VStack(alignment: .leading, spacing: 24) {
            if let placed = phone.placed {
                TitledSection(state.tr("当前 App", "Current App")) { appCard(catalog.app(of: placed.window)) }
            }
            let others = catalog.running.filter { $0.pid != phone.placed?.window.key.pid }
            if !catalog.loaded {
                EmptyState(symbol: nil, text: state.tr("正在读取", "Loading…"))
            } else if others.isEmpty {
                if phone.placed == nil { EmptyState(symbol: "square.grid.2x2", text: state.tr("没有正在运行的 App", "No apps running")) }
            } else if phone.placed != nil {
                TitledSection(state.tr("其他 App", "Other Apps")) { appCards(others) }
            } else {
                appCards(others)
            }
        }
    }

    private func appCards(_ apps: [RunningApp]) -> some View {
        LazyVStack(spacing: 12) {
            ForEach(apps) { appCard($0) }
        }
    }

    private func appCard(_ app: RunningApp) -> some View {
        AppCard(app: app, phone: phone, confirming: $confirming, icon: catalog.icon(for: app.app), intents: intents)
    }
}

// MARK: 页眉

/// 分段控件：和 iOS 的一样，选中的那段浮起来
private struct TabPicker: View {
    @Binding var selection: PhoneHomeView.Tab

    var body: some View {
        HStack(spacing: 0) {
            ForEach(PhoneHomeView.Tab.allCases) { tab in
                Button { selection = tab } label: {
                    Text(tab.title)
                        .font(PhoneStyle.label.weight(selection == tab ? .semibold : .medium))
                        .frame(maxWidth: .infinity)
                        .frame(height: 38)
                        .background {
                            if selection == tab {
                                RoundedRectangle(cornerRadius: 9, style: .continuous)
                                    .fill(PhoneStyle.segment)
                                    .shadow(color: .black.opacity(0.12), radius: 4, y: 2)
                            }
                        }
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(3)
        .background(PhoneStyle.fill, in: RoundedRectangle(cornerRadius: PhoneStyle.cornerRadius, style: .continuous))
        .animation(.easeOut(duration: 0.15), value: selection)
    }
}

/// 回到手机屏上的窗口：和 iPhone 左上角「‹ App 名」回到上一个 App 一样
private struct BackButton: View {
    let app: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: "chevron.left").font(PhoneStyle.headline)
                Text(app).font(PhoneStyle.body.weight(.medium)).lineLimit(1)
            }
            .foregroundStyle(PhoneStyle.tint)
            .padding(.leading, 16)
            .padding(.trailing, 20)
            .frame(height: PhoneStyle.touchTarget)
            .background(PhoneStyle.tint.opacity(0.12), in: Capsule())
        }
        .buttonStyle(PressableStyle())
        .accessibilityLabel(Lang.tr("返回 \(app)", "Back to \(app)"))
    }
}

/// 出了什么事、怎么办；没退出的 App 给「稍后 / 强制退出」
private struct NoticeBanner: View {
    let notice: PhoneDesk.Notice
    let intents: any PhoneIntents

    var body: some View {
        let text = notice.text
        Banner(symbol: "exclamationmark.circle.fill", tint: .orange, headline: text.headline, detail: text.detail) {
            if case .quitStalled(let app, let pid) = notice {
                HStack(spacing: 12) {
                    PillButton(title: Lang.tr("稍后", "Later")) { intents.dismissNotice() }
                    PillButton(title: Lang.tr("强制退出", "Force Quit"), destructive: true) { intents.forceQuit(pid: pid, app: app) }
                }
            }
        }
    }
}

/// 页眉里的一张卡：图标、一句话、补充说明，下面可以带按钮（提示、退出手机屏的确认）
private struct Banner<Actions: View>: View {
    let symbol: String
    let tint: Color
    let headline: String
    let detail: String?
    @ViewBuilder let actions: Actions

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Image(systemName: symbol).foregroundStyle(tint)
            VStack(alignment: .leading, spacing: 4) {
                Text(headline).font(PhoneStyle.body.weight(.semibold))
                if let detail {
                    Text(detail).font(PhoneStyle.body).foregroundStyle(.secondary)
                }
                actions
            }
        }
        .font(PhoneStyle.body)
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
    }
}

// MARK: 全部

/// 本机所有 App：图标网格，4 列，按字典序；正在运行的下方有圆点，正在退出的点不了
private struct AllApps: View {
    let catalog: AppCatalog
    let quitting: Set<Int32>
    let intents: any PhoneIntents

    private static let columns = Array(repeating: GridItem(.flexible(), spacing: 0), count: 4)

    var body: some View {
        if catalog.installed.isEmpty {
            EmptyState(symbol: nil, text: Lang.tr("正在读取", "Loading…"))
        } else {
            let running = Dictionary(catalog.running.map { ($0.app.bundleID, $0.pid) }, uniquingKeysWith: { first, _ in first })
            LazyVGrid(columns: Self.columns, spacing: 20) {
                ForEach(catalog.installed) { item in
                    let pid = running[item.app.bundleID]
                    Button { intents.open(item.app) } label: {
                        VStack(spacing: 8) {
                            AppIcon(image: catalog.icon(for: item.app), size: 72)
                            Text(item.app.name).font(PhoneStyle.label).lineLimit(1).frame(maxWidth: 116)
                            Circle()
                                .fill(.secondary)
                                .frame(width: 6, height: 6)
                                .opacity(pid == nil ? 0 : 1)
                                .padding(.top, -4)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(PressableStyle())
                    .disabled(pid.map(quitting.contains) ?? false)
                }
            }
        }
    }
}

// MARK: 正在打开、正在放、正在调整比例、正在退出

/// 整屏讲明正在打开哪个 App、放哪个窗口、按哪台手机调整比例、在退出手机屏。窗口挪好列表就收起，失败了回到列表、顶部写明原因
private struct BusyView: View {
    struct Content: Equatable {
        enum Art: Equatable {
            case icon(NSImage)
            case symbol(String)
        }

        let art: Art
        let title: String
        let subtitle: String?
        let status: String

        @MainActor
        init?(_ work: PhoneDesk.Work?, catalog: AppCatalog, phone: PhoneChoice) {
            switch work {
            case .opening(let app):
                (art, title, subtitle, status) = (.icon(catalog.icon(for: app)), app.name, nil, Lang.tr("正在打开", "Opening…"))
            case .placing(let window):
                (art, title, subtitle, status) = (.icon(catalog.icon(for: window)), window.app,
                                                  window.title.isEmpty ? nil : window.title, Lang.tr("正在打开", "Opening…"))
            case .reshaping:
                (art, title, subtitle, status) = (.symbol("iphone.gen3"), phone.title, phone.screen.sizeText, Lang.tr("正在调整屏幕比例", "Adjusting the screen ratio"))
            case .dismissing:
                (art, title, subtitle, status) = (.symbol(PhoneHomeView.dismissSymbol), PhoneScreenSpec.name, nil, Lang.tr("正在退出", "Exiting…"))
            default:
                return nil
            }
        }
    }

    let content: Content

    var body: some View {
        VStack(spacing: 16) {
            switch content.art {
            case .icon(let image):
                AppIcon(image: image, size: PhoneStyle.iconSize * 2)
            case .symbol(let name):
                Image(systemName: name)
                    .font(.system(size: PhoneStyle.iconSize * 1.5, weight: .light))
                    .foregroundStyle(.secondary)
                    .frame(width: PhoneStyle.iconSize * 2, height: PhoneStyle.iconSize * 2)
                    .accessibilityHidden(true)
            }
            VStack(spacing: 4) {
                Text(content.title).font(PhoneStyle.headline).lineLimit(1)
                if let subtitle = content.subtitle {
                    Text(subtitle).font(PhoneStyle.body).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            VStack(spacing: 8) {
                ProgressView()
                Text(content.status).font(PhoneStyle.label).foregroundStyle(.secondary)
            }
            .padding(.top, 16)
        }
        .padding(.horizontal, 32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(PhoneStyle.background)
    }
}
