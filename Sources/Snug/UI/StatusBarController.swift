// 菜单栏：只用来看状态、打开设置和关于、退出。Snug 的操作都在手机上，Mac 这边不用管。
// 每次展开都按 AppState 现做，和设置窗口永远一致
import AppKit
import Observation
import SnugCore
import SwiftUI

@MainActor
final class StatusBarController: NSObject, NSMenuDelegate {
    private let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let state: AppState
    private let intents: any AppIntents
    private let navigation: any Navigation

    init(state: AppState, intents: any AppIntents, navigation: any Navigation) {
        self.state = state
        self.intents = intents
        self.navigation = navigation
        super.init()
        let menu = NSMenu()
        menu.delegate = self
        menu.autoenablesItems = false
        item.menu = menu
        item.button?.image = Self.icon
        refreshButton()
    }

    /// 菜单栏图标：SF Symbols 的 iphone.gen3（系统图标本身就是模板图，深色菜单栏自动反白）
    private static let icon: NSImage? = NSImage(systemSymbolName: "iphone.gen3", accessibilityDescription: "Snug")?
        .withSymbolConfiguration(.init(pointSize: 15, weight: .medium))

    /// 关着手机屏时图标变灰；提示文字随状态变
    private func refreshButton() {
        withObservationTracking {
            item.button?.appearsDisabled = !state.config.phoneScreen || !state.accessibilityTrusted
            item.button?.toolTip = Lang.tr("Snug：", "Snug: ") + StatusText(state).headline
        } onChange: { [weak self] in
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.refreshButton() } }
        }
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        menu.addItem(status())
        if state.legacyAppRunning { menu.addItem(action(Lang.tr("退出旧版 UU远程伴侣", "Quit Legacy UU Remote Companion"), #selector(quitLegacy), symbol: "xmark.circle")) }
        menu.addItem(.separator())
        menu.addItem(action(Lang.tr("设置…", "Settings…"), #selector(openSettings), key: ",", symbol: "gearshape"))
        let about = action(Lang.tr("关于 Snug", "About Snug"), #selector(openAbout), symbol: "info.circle")
        // 有新版本：系统的菜单角标，更新在「关于」页里
        if state.update.hasNewVersion { about.badge = NSMenuItemBadge(string: Lang.tr("新版本", "New Version")) }
        menu.addItem(about)
        menu.addItem(.separator())
        menu.addItem(action(Lang.tr("退出 Snug", "Quit Snug"), #selector(quit), key: "q", symbol: "power"))
    }

    /// 最上面一行：状态圆点 + 此刻的状态和一行说明。只看不点，所以用自定义视图（不会高亮）
    private func status() -> NSMenuItem {
        let view = NSHostingView(rootView: MenuStatusView(text: StatusText(state), color: state.statusColor))
        view.frame.size = view.fittingSize
        view.autoresizingMask = [.width]
        let it = NSMenuItem()
        it.view = view
        return it
    }

    private func action(_ title: String, _ sel: Selector, key: String = "", symbol: String) -> NSMenuItem {
        let it = NSMenuItem(title: title, action: sel, keyEquivalent: key)
        it.target = self
        it.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)
        return it
    }

    @objc private func quitLegacy() { intents.quitLegacyApp() }
    @objc private func openSettings() { navigation.openSettings() }
    @objc private func openAbout() { navigation.openAbout() }
    @objc private func quit() { intents.quit() }
}

private struct MenuStatusView: View {
    let text: StatusText
    let color: Color

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Circle().fill(color).frame(width: 8, height: 8)
            VStack(alignment: .leading, spacing: 2) {
                Text(text.headline).font(.system(size: 13, weight: .medium))
                if let detail = text.detail { Text(detail).font(.system(size: 11)).foregroundStyle(.secondary) }
            }
        }
        .fixedSize()
        .padding(.horizontal, 14)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}
