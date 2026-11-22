// 设置页、「关于 Snug」、欢迎页共用的行标题：系统设置那样的彩色方块图标（SF Symbols）+ 标题 + 一行短说明；
// 以及权限那几行的标题、状态和「授权…」按钮
import PermissionFlow
import SwiftUI

struct SettingLabel: View {
    let title: String
    var detail: String?
    /// 说明是要用户注意的问题：橙色、带警示图标，和正常状态区分开
    var warning = false
    let symbol: String
    let tint: Color

    init(_ title: String, detail: String? = nil, warning: Bool = false, symbol: String, tint: Color) {
        self.title = title
        self.detail = detail
        self.warning = warning
        self.symbol = symbol
        self.tint = tint
    }

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.white)
                .frame(width: 24, height: 24)
                .background(tint.gradient, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                if let detail, warning {
                    Label(detail, systemImage: "exclamationmark.triangle.fill").font(.callout).foregroundStyle(.orange)
                } else if let detail {
                    Text(detail).font(.callout).foregroundStyle(.secondary)
                }
            }
        }
    }
}

extension SettingLabel {
    /// 辅助功能权限那一行（欢迎页、设置页）
    static var accessibility: SettingLabel {
        SettingLabel(PermissionName.accessibility, detail: Lang.tr("用于移动窗口与转换手势", "Used to move windows and translate gestures"), symbol: "hand.raised.fill", tint: .indigo)
    }

    /// 录屏权限那一行（设置页，开了截图键才有）
    static var screenRecording: SettingLabel {
        SettingLabel(PermissionName.screenRecording, detail: Lang.tr("用于截图", "Used for screenshots"), symbol: "rectangle.dashed.badge.record", tint: .red)
    }
}

/// 权限的状态：已授权带一个小对勾，未授权只写字。「授权…」放在各页的主按钮位置，不在这一行重复
struct PermissionStatus: View {
    let granted: Bool

    var body: some View {
        if granted {
            HStack(spacing: 4) {
                Image(systemName: "checkmark.circle.fill").foregroundStyle(.green).imageScale(.small)
                Text(Lang.tr("已授权", "Granted")).foregroundStyle(.secondary)
            }
        } else {
            Text(Lang.tr("未授权", "Not granted")).foregroundStyle(.secondary)
        }
    }
}

/// 「授权…」：PermissionFlow 打开系统设置对应的隐私页，旁边浮出一个小窗口，把 Snug 拖进列表即可
struct AuthorizeButton: View {
    let pane: PermissionFlowPane

    var body: some View {
        let app = Bundle.main.bundleURL
        PermissionFlowButton(
            pane: pane,
            suggestedAppURLs: [app],
            configuration: .init(requiredAppURLs: [app], promptForAccessibilityTrust: false, localeIdentifier: Lang.english ? "en" : "zh-Hans")
        ) { _ in
            Text(Lang.tr("授权…", "Authorize…"))
        }
    }
}
