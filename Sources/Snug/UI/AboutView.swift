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
        Credit(name: "UUyc-companion", url: "https://github.com/aoyamashou/UUyc-companion",
               author: "AoyamaShou", usage: "项目前身"),
        Credit(name: "PermissionFlow", url: "https://github.com/jaywcjlove/PermissionFlow",
               author: "小弟调调（jaywcjlove）", usage: "授权引导"),
        Credit(name: "Better Remote", url: "https://github.com/pincc-debug/better-remote",
               author: "pincc-debug", usage: "单指滚动思路"),
        Credit(name: "FreeDisplay", url: "https://github.com/huberdf/FreeDisplay",
               author: "huberdf", usage: "虚拟屏接口声明"),
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
                            Text(Updater.current?.description ?? "开发版").foregroundStyle(.secondary).monospacedDigit()
                        }
                        Text("手机远程 Mac 的显示与触控增强")
                        Link("github.com/zephyr4123/snug", destination: URL(string: "https://github.com/skyvense/snug-sunlogin")!)
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
                Text("社区 PR 作者：bubble、gencheng liu、Rongzheng Liu").foregroundStyle(.secondary)
            } header: {
                Text("致谢（MIT 许可）")
            } footer: {
                Text("非 向日葵 官方软件").font(.footnote).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(width: SettingsWindowController.width)
        .fixedSize(horizontal: false, vertical: true)
    }
}
