// 手机屏 App 资源库共用的小部件：App 图标、「图标 + 标题 + 副标题」一行、圆形和胶囊按钮、再点一下确认的一对键、空状态、小标题分节、
// 一组行的卡片、搜索框和它的清除键、导航栏右边的文字按钮、输入框底、卡片底。
// 尺寸、颜色取自 PhoneStyle
import AppKit
import SwiftUI

struct AppIcon: View {
    let image: NSImage
    let size: CGFloat

    var body: some View {
        Image(nsImage: image).resizable().frame(width: size, height: size)
    }
}

/// 图标 + 标题 + 副标题的一行（App 卡片的头、搜索结果）
struct AppRowLabel: View {
    let icon: NSImage
    let title: String
    let subtitle: String
    var subtitleColor = Color.secondary

    var body: some View {
        HStack(spacing: 16) {
            AppIcon(image: icon, size: PhoneStyle.iconSize)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(PhoneStyle.headline).lineLimit(1)
                Text(subtitle).font(PhoneStyle.body).foregroundStyle(subtitleColor).lineLimit(1)
            }
            Spacer(minLength: 8)
        }
        .contentShape(Rectangle())
    }
}

/// 圆形的图标按钮：新建窗口、退出、字号
struct RoundButton: View {
    let symbol: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(PhoneStyle.headline)
                .frame(width: PhoneStyle.touchTarget, height: PhoneStyle.touchTarget)
                .background(PhoneStyle.background, in: Circle())
        }
        .buttonStyle(PressableStyle())
        .accessibilityLabel(label)
    }
}

/// 胶囊形的文字按钮，确认时用，危险的那个红底白字。
/// 看着 44 点高，好放进窗口那一行；上下各多算几点，点中的高度仍是 touchTarget
struct PillButton: View {
    private static let height: CGFloat = 44

    let title: String
    var destructive = false
    let action: () -> Void

    var body: some View {
        Button(role: destructive ? .destructive : nil, action: action) {
            Text(title)
                .font(PhoneStyle.body.weight(.medium))
                .foregroundStyle(destructive ? Color.white : .primary)
                .padding(.horizontal, 20)
                .frame(height: Self.height)
                .background(destructive ? Color.red : PhoneStyle.background, in: Capsule())
                .padding(.vertical, (PhoneStyle.touchTarget - Self.height) / 2)
        }
        .buttonStyle(PressableStyle())
    }
}

/// 再点一下确认：「取消」和红色的确认键（退出 App、关窗口、退出手机屏）。两个键都先作废等确认的状态，确认键再做事
struct ConfirmButtons<Value>: View {
    let title: String
    @Binding var confirming: Value?
    var spacing: CGFloat = 8
    let action: () -> Void

    var body: some View {
        HStack(spacing: spacing) {
            PillButton(title: Lang.tr("取消", "Cancel")) { confirming = nil }
            PillButton(title: title, destructive: true) {
                confirming = nil
                action()
            }
        }
    }
}

/// 空的、正在读取、没找到：symbol 为 nil 时显示加载动画
struct EmptyState: View {
    let symbol: String?
    var title: String?
    let text: String

    var body: some View {
        VStack(spacing: 12) {
            if let symbol {
                Image(systemName: symbol).font(PhoneStyle.largeTitle).foregroundStyle(.tertiary)
            } else {
                ProgressView()
            }
            if let title { Text(title).font(PhoneStyle.headline) }
            Text(text).font(PhoneStyle.body).foregroundStyle(.secondary).multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 48)
    }
}

/// 列表里的一节：小标题 + 内容
struct TitledSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(PhoneStyle.label)
                .foregroundStyle(.secondary)
                .padding(.leading, 4)
            content
        }
    }
}

/// 一组行放在一张卡片里，中间用分隔线隔开（分隔线从 dividerInset 起，和标题对齐）
struct ListCard<Item: Identifiable, Row: View>: View {
    let items: [Item]
    let dividerInset: CGFloat
    let row: (Item) -> Row

    init(_ items: [Item], dividerInset: CGFloat = 80, @ViewBuilder row: @escaping (Item) -> Row) {
        self.items = items
        self.dividerInset = dividerInset
        self.row = row
    }

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                if index > 0 { Divider().padding(.leading, dividerInset) }
                row(item)
            }
        }
        .card()
    }
}

/// 搜索框的样子（首页上是个按钮，搜索页里是真的输入框）
struct SearchBar<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
            content
            Spacer(minLength: 0)
        }
        .font(PhoneStyle.body)
        .padding(.horizontal, 12)
        .fieldBackground()
    }
}

/// 输入框里的清除键：有字时才出现
struct ClearButton: View {
    @Binding var text: String

    var body: some View {
        if !text.isEmpty {
            Button { text = "" } label: {
                Image(systemName: "xmark.circle.fill").foregroundStyle(.tertiary)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Lang.tr("清除", "Clear"))
        }
    }
}

/// 导航栏右边的文字按钮（取消、完成）：靠右对齐，点中的范围不小于 touchTarget
struct BarButton: View {
    let title: String
    var bold = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(bold ? PhoneStyle.body.weight(.semibold) : PhoneStyle.body)
                .foregroundStyle(PhoneStyle.tint)
                .frame(minWidth: PhoneStyle.touchTarget, minHeight: PhoneStyle.touchTarget, alignment: .trailing)
        }
        .buttonStyle(PressableStyle())
    }
}

extension View {
    /// 分组列表的卡片底（iOS 的 secondarySystemGroupedBackground）
    func card() -> some View {
        background(PhoneStyle.card, in: RoundedRectangle(cornerRadius: PhoneStyle.cornerRadius, style: .continuous))
    }

    /// 输入框的底（搜索框、数字框）：48 点高，iOS 的 tertiarySystemFill
    func fieldBackground() -> some View {
        frame(height: 48)
            .background(PhoneStyle.fill, in: RoundedRectangle(cornerRadius: PhoneStyle.cornerRadius, style: .continuous))
    }
}
