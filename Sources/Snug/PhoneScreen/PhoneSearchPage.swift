// App 资源库的搜索：点首页的搜索框进入，空着时按字母分组列出全部 App（同 iOS 的 App 资源库），
// 输入后搜 App 名（中文、英文、拼音、首字母）和窗口标题（见 AppList.search）。取消、或从结果里打开了东西就退出
import AppKit
import SnugCore
import SwiftUI

struct SearchPage: View {
    let catalog: AppCatalog
    @Bindable var search: PhonePages
    let phone: PhoneDesk.State
    let intents: any PhoneIntents
    @FocusState private var focused: Bool

    private var query: String { search.trimmedQuery }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                SearchBar {
                    TextField(Lang.tr("搜索 App", "Search apps"), text: $search.query)
                        .textFieldStyle(.plain)
                        .focused($focused)
                    ClearButton(text: $search.query)
                }
                BarButton(title: Lang.tr("取消", "Cancel")) { search.close() }
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)
            .padding(.bottom, 16)
            ScrollView {
                results.padding(.horizontal, 20).padding(.bottom, 24)
            }
            .scrollIndicators(.never)
        }
        // 面板在进入搜索时才成为键盘焦点，等它一拍再聚焦输入框
        .task {
            try? await Task.sleep(for: .milliseconds(50))
            focused = true
        }
    }

    @ViewBuilder private var results: some View {
        if query.isEmpty {
            LazyVStack(alignment: .leading, spacing: 8) {
                ForEach(AppList.grouped(catalog.installed)) { group in
                    Text(group.letter)
                        .font(PhoneStyle.label.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .padding(.leading, 4)
                        .padding(.top, 8)
                    ListCard(group.apps.map(\.app)) { appRow($0) }
                }
            }
        } else {
            let result = AppList.search(query, installed: catalog.installed, running: catalog.running)
            if result.isEmpty {
                EmptyState(symbol: "magnifyingglass", title: Lang.tr("无结果", "No results"), text: Lang.tr("没有与「\(query)」相符的 App 或窗口", "No apps or windows match “\(query)”"))
            } else {
                VStack(alignment: .leading, spacing: 24) {
                    if !result.apps.isEmpty {
                        TitledSection("App") { ListCard(result.apps) { appRow($0) } }
                    }
                    if !result.windows.isEmpty {
                        TitledSection(Lang.tr("窗口", "Windows")) { ListCard(result.windows) { windowRow($0) } }
                    }
                }
            }
        }
    }

    private func appRow(_ app: AppInfo) -> some View {
        let running = catalog.running(app)
        let quitting = running.map { phone.quitting.contains($0.pid) } ?? false
        let subtitle = quitting ? Lang.tr("正在退出", "Quitting…") : running?.windowSummary ?? Lang.tr("未打开", "Not open")
        return ResultRow(icon: catalog.icon(for: app), title: app.name, subtitle: subtitle) {
            search.close()
            intents.open(app)
        }
        .disabled(quitting)
    }

    private func windowRow(_ window: WindowInfo) -> some View {
        ResultRow(icon: catalog.icon(for: window), title: window.displayTitle, subtitle: window.app) {
            search.close()
            intents.pull(window)
        }
        .disabled(phone.quitting.contains(window.key.pid) || phone.closing.contains(window.key))
    }
}

private struct ResultRow: View {
    let icon: NSImage
    let title: String
    let subtitle: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            AppRowLabel(icon: icon, title: title, subtitle: subtitle).padding(16)
        }
        .buttonStyle(PressableStyle())
    }
}
