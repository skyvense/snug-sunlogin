// 设置窗口的「关于」页：名称、版本、软件更新、致谢与免责声明。和通用页同一套分组样式，只读 AppState，意图走 AppIntents
import AppKit
import SwiftUI
import SnugCore

struct AboutView: View {
    let state: AppState
    let intents: any AppIntents
    let navigation: any Navigation

    private struct Credit: Identifiable {
        let name, url, author, usage: String
        var id: String { name }
    }

    private let credits = [
        Credit(name: "Snug", url: "https://github.com/zephyr4123/snug",
               author: "zephyr4123", usage: Lang.tr("原项目，本分支从其代码修改而来", "The original project; this fork is modified from its code")),
        Credit(name: "UUyc-companion", url: "https://github.com/aoyamashou/UUyc-companion",
               author: "AoyamaShou", usage: Lang.tr("项目前身", "Project predecessor")),
        Credit(name: "PermissionFlow", url: "https://github.com/jaywcjlove/PermissionFlow",
               author: "小弟调调（jaywcjlove）", usage: Lang.tr("授权引导", "Permission guidance")),
        Credit(name: "Better Remote", url: "https://github.com/pincc-debug/better-remote",
               author: "pincc-debug", usage: Lang.tr("单指滚动思路", "One-finger scrolling approach")),
        Credit(name: "FreeDisplay", url: "https://github.com/huberdf/FreeDisplay",
               author: "huberdf", usage: Lang.tr("虚拟屏接口声明", "Virtual display API declarations")),
    ]

    var body: some View {
        Form {
            Section {
                HStack(spacing: 16) {
                    Image(nsImage: NSApp.applicationIconImage)
                        .resizable()
                        .frame(width: 64, height: 64)
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(alignment: .firstTextBaseline, spacing: 8) {
                            Text("Snug").font(.title2.bold())
                            Text(Updater.current?.description ?? state.tr("开发版", "Development build")).foregroundStyle(.secondary).monospacedDigit()
                        }
                        Text(state.tr("手机远程 Mac 的显示与触控增强", "Display & touch enhancements for phone-to-Mac remote control"))
                        Text(state.tr("向日葵移植版 · 从原作者 Snug（zephyr4123）的代码修改而来", "Sunlogin port · Modified from the original Snug by zephyr4123")).foregroundStyle(.secondary)
                        Link("github.com/skyvense/snug-sunlogin", destination: URL(string: "https://github.com/skyvense/snug-sunlogin")!)
                    }
                }
                .padding(.vertical, 4)
            }
            UpdateSection(state: state, intents: intents, navigation: navigation)
            Section {
                ForEach(credits) { c in
                    LabeledContent {
                        Text(c.usage).foregroundStyle(.secondary)
                    } label: {
                        HStack(spacing: 4) {
                            Link(c.name, destination: URL(string: c.url)!)
                            Text("· \(c.author)").foregroundStyle(.secondary)
                        }
                    }
                }
                Text(state.tr("社区 PR 作者：bubble、gencheng liu、Rongzheng Liu", "Community PR authors: bubble, gencheng liu, Rongzheng Liu")).foregroundStyle(.secondary)
            } header: {
                Text(state.tr("致谢（MIT 许可）", "Credits (MIT License)"))
            } footer: {
                Text(state.tr("非 向日葵 官方软件", "Not an official Sunlogin product")).font(.footnote).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(width: SettingsWindowController.width)
        .fixedSize(horizontal: false, vertical: true)
    }
}
