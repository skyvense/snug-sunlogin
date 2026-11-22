// 应用内更新的界面：「关于 Snug」里的「软件更新」一组，和查到新版本时弹出的更新窗口。
// 都只读 AppState.update（Updater 写），意图走 AppIntents
import AppKit
import SwiftUI
import SnugCore

extension UpdateState {
    /// 一句话的状态：「关于 Snug」里软件更新那一行的说明
    var summary: String {
        switch phase {
        case .checking: return Lang.tr("检查中", "Checking…")
        case .downloading(let fraction): return Lang.tr("下载中", "Downloading") + " \(Int(fraction * 100))%"
        case .installing: return Lang.tr("安装中", "Installing…")
        case .idle: break
        }
        if let problem { return problem.text }
        return switch check {
        case .unknown: Updater.current == nil ? Lang.tr("开发版不检查更新", "Development builds don't check for updates") : Lang.tr("尚未检查", "Not checked yet")
        case .upToDate: Lang.tr("已是最新", "Up to date")
        case .available(let feed): Lang.tr("新版本", "New version") + " \(feed.version)"
        }
    }
}

/// 「关于 Snug」里的软件更新：状态和操作一行，更新源一行
struct UpdateSection: View {
    let state: AppState
    let intents: any AppIntents
    let navigation: any Navigation

    var body: some View {
        Section {
            LabeledContent {
                action
            } label: {
                SettingLabel(state.tr("软件更新", "Software Update"), detail: state.update.summary, warning: state.update.problem != nil,
                             symbol: "arrow.down", tint: .blue)
            }
            LabeledContent {
                Picker(state.tr("更新源", "Update Source"), selection: Binding(get: { state.config.updateSource }, set: { intents.setUpdateSource($0) })) {
                    ForEach(UpdateSource.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                .pickerStyle(.menu)
                .labelsHidden()
                .fixedSize()
                .disabled(state.update.phase != .idle)
            } label: {
                SettingLabel(state.tr("更新源", "Update Source"), symbol: "server.rack", tint: .gray)
            }
        }
    }

    @ViewBuilder
    private var action: some View {
        let update = state.update
        switch update.phase {
        case .checking, .installing:
            ProgressView().controlSize(.small)
        case .downloading(let fraction):
            ProgressView(value: fraction).frame(width: 96)
        case .idle:
            HStack(spacing: 12) {
                if update.available != nil {
                    Button(state.tr("更新内容", "What's New")) { navigation.openUpdate() }.buttonStyle(.link)
                }
                if update.problem?.needsDownload == true {
                    Button(state.tr("前往下载", "Go to Download")) { intents.openDownloadPage() }
                } else if update.available != nil {
                    Button(update.problem == nil ? state.tr("立即更新", "Update Now") : state.tr("重试", "Retry")) { intents.installUpdate() }
                        .buttonStyle(.borderedProminent)
                } else {
                    Button(state.tr("检查更新", "Check for Updates")) { intents.checkForUpdates() }.disabled(Updater.current == nil)
                }
            }
        }
    }
}

/// 更新窗口：讲清落下了哪几版、各改了什么，由用户选「稍后」或「立即更新」
struct UpdateView: View {
    let state: AppState
    let current: Version
    let intents: any AppIntents
    let close: () -> Void

    var body: some View {
        if let feed = state.update.available {
            let notes = feed.releases(after: current)
            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 12) {
                    Image(nsImage: NSApp.applicationIconImage)
                        .resizable()
                        .frame(width: 48, height: 48)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(state.tr("新版本", "New version") + " \(feed.version.description)").font(.title3.bold())
                        Text(notes.count > 1 ? state.tr("当前", "Current") + " \(current.description) · " + state.tr("共 \(notes.count) 个新版本", "\(notes.count) new versions") : state.tr("当前", "Current") + " \(current.description)")
                            .foregroundStyle(.secondary)
                    }
                }
                // 内容少时按内容高度，多了封顶 240 再滚动
                ViewThatFits(in: .vertical) {
                    noteList(notes)
                    ScrollView { noteList(notes) }
                }
                .frame(maxHeight: 240)
                .background(.fill.quaternary, in: .rect(cornerRadius: 10))
                footer
            }
            .padding(20)
            .frame(width: 480)
        }
    }

    private func noteList(_ notes: [ReleaseNote]) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            ForEach(Array(notes.enumerated()), id: \.element.version) { index, note in
                if index > 0 { Divider() }
                ReleaseNoteView(note: note)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// 底部：没开始时是按钮，进行中是进度，没成时写明原因并给出下一步
    private var footer: some View {
        let update = state.update
        return HStack(spacing: 12) {
            switch update.phase {
            case .downloading(let fraction):
                Text(state.tr("下载中", "Downloading"))
                ProgressView(value: fraction)
                Text("\(Int(fraction * 100))%").monospacedDigit().foregroundStyle(.secondary)
            case .installing:
                ProgressView().controlSize(.small)
                Text(state.tr("安装中，完成后自动重新打开", "Installing — Snug will reopen when done")).foregroundStyle(.secondary)
                Spacer()
            case .idle, .checking:
                if let problem = update.problem {
                    Label(problem.text, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                }
                Spacer()
                Button(state.tr("稍后", "Later"), action: close).keyboardShortcut(.cancelAction)
                if update.problem?.needsDownload == true {
                    Button(state.tr("前往下载", "Go to Download")) { intents.openDownloadPage() }.keyboardShortcut(.defaultAction)
                } else {
                    Button(update.problem == nil ? state.tr("立即更新", "Update Now") : state.tr("重试", "Retry")) { intents.installUpdate() }
                        .keyboardShortcut(.defaultAction)
                        .disabled(update.phase == .checking)
                }
            }
        }
        .frame(minHeight: 24)
    }
}

/// 一个版本的更新日志：版本号和日期、标题，正文分成小标题、条目、段落
private struct ReleaseNoteView: View {
    let note: ReleaseNote

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline) {
                    Text(note.version.description).font(.subheadline.weight(.semibold)).monospacedDigit()
                    Spacer()
                    Text(note.date).font(.subheadline).foregroundStyle(.secondary).monospacedDigit()
                }
                Text(note.title).font(.headline)
            }
            ForEach(Array(note.blocks.enumerated()), id: \.offset) { _, block in
                switch block {
                case .heading(let text):
                    Text(text).font(.subheadline.weight(.semibold)).foregroundStyle(.secondary).padding(.top, 4)
                case .bullet(let text):
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("•").foregroundStyle(.secondary)
                        Text(inline(text))
                    }
                case .paragraph(let text):
                    Text(inline(text)).foregroundStyle(.secondary)
                }
            }
        }
        .fixedSize(horizontal: false, vertical: true)
    }

    /// 行内的 Markdown（`代码`、**加粗**）照样显示
    private func inline(_ text: String) -> AttributedString {
        (try? AttributedString(markdown: text, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)))
            ?? AttributedString(text)
    }
}

@MainActor
final class UpdateWindowController {
    private let state: AppState
    private let intents: any AppIntents
    private var window: NSWindow?

    init(state: AppState, intents: any AppIntents) {
        self.state = state
        self.intents = intents
    }

    func show(activating: Bool) {
        guard let current = Updater.current else { return }
        if window == nil {
            let view = UpdateView(state: state, current: current, intents: intents) { [weak self] in self?.window?.close() }
            window = NSWindow(hosting: view, title: Lang.tr("软件更新", "Software Update"))
        }
        window?.present(activating: activating)
    }
}
