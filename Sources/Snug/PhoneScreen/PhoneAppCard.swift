// 「运行中」页的一张 App 卡片：头上是 App（点它同点 Dock 图标；右边新建窗口、退出），下面是它的各个窗口
// （点一下放上手机屏，右边关掉）。放在手机屏上的那个窗口排第一、写明在手机屏上，紧跟一行字号和还原。
// 退出、关窗口都要再点一下确认，同一时刻只有一处在等确认（见 PhoneHomeView.Confirm）
import AppKit
import SnugCore
import SwiftUI

struct AppCard: View {
    /// 窗口按要显示的顺序（放在手机屏上的排第一，见 AppCatalog.app(of:)）
    let app: RunningApp
    let phone: PhoneDesk.State
    @Binding var confirming: PhoneHomeView.Confirm?
    let icon: NSImage
    let intents: any PhoneIntents

    private var quitting: Bool { phone.quitting.contains(app.pid) }
    private var askingQuit: Bool { confirming == .quit(app.pid) }
    /// 这个 App 放在手机屏上的窗口
    private var placed: PhoneDesk.Placed? { phone.placed.flatMap { $0.window.key.pid == app.pid ? $0 : nil } }

    var body: some View {
        VStack(spacing: 0) {
            header
            ForEach(app.windows) { window in
                let onPhone = window.key == placed?.window.key
                Divider().padding(.leading, 80)
                WindowRow(window: window, onPhone: onPhone, closing: phone.closing.contains(window.key),
                          confirming: $confirming, intents: intents)
                if onPhone, let placed {
                    PlacedControls(placed: placed, work: phone.work, intents: intents)
                }
            }
            .disabled(quitting)
        }
        .card()
    }

    private var header: some View {
        HStack(spacing: 8) {
            Button { intents.open(app.app) } label: {
                AppRowLabel(icon: icon, title: app.app.name, subtitle: subtitle, subtitleColor: askingQuit ? .red : .secondary)
            }
            .buttonStyle(PressableStyle())
            .disabled(quitting)
            actions
        }
        .padding(.leading, 16)
        .padding(.trailing, 12)
        .padding(.vertical, 12)
    }

    private var subtitle: String {
        if askingQuit { return Lang.tr("退出 \(app.app.name)？", "Quit \(app.app.name)?") }
        if quitting { return Lang.tr("正在退出", "Quitting…") }
        return app.windowSummary
    }

    @ViewBuilder private var actions: some View {
        if quitting {
            ProgressView().controlSize(.small).frame(width: PhoneStyle.touchTarget)
        } else if askingQuit {
            ConfirmButtons(title: Lang.tr("退出", "Quit"), confirming: $confirming) { intents.quit(app) }
        } else {
            HStack(spacing: 8) {
                RoundButton(symbol: "macwindow.badge.plus", label: Lang.tr("新建窗口", "New Window")) { intents.newWindow(app) }
                if AppList.canQuit(app.app) {
                    RoundButton(symbol: "xmark.app", label: Lang.tr("退出", "Quit")) { confirming = .quit(app.pid) }
                }
            }
        }
    }
}

/// App 卡片里的一个窗口：点一下放上手机屏，已经在手机屏上的就回到它；右边关掉它
private struct WindowRow: View {
    let window: WindowInfo
    let onPhone: Bool
    let closing: Bool
    @Binding var confirming: PhoneHomeView.Confirm?
    let intents: any PhoneIntents

    private var askingClose: Bool { confirming == .close(window.key) }

    var body: some View {
        HStack(spacing: 8) {
            Button { onPhone ? intents.returnToPlaced() : intents.pull(window) } label: {
                HStack(spacing: 16) {
                    Image(systemName: onPhone ? "iphone" : "macwindow")
                        .font(PhoneStyle.headline.weight(.regular))
                        .foregroundStyle(onPhone ? PhoneStyle.tint : .secondary)
                        .frame(width: PhoneStyle.iconSize)
                    Text(askingClose ? Lang.tr("关闭此窗口？", "Close this window?") : window.displayTitle)
                        .font(PhoneStyle.body)
                        .foregroundStyle(askingClose ? Color.red : .primary)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    if !askingClose {
                        if onPhone { Tag(text: Lang.tr("在手机屏上", "On Phone Screen"), color: PhoneStyle.tint) }
                        if window.minimized { Tag(text: Lang.tr("已最小化", "Minimized"), color: .secondary) }
                    }
                }
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
            }
            .buttonStyle(PressableStyle())
            trailing
        }
        .padding(.leading, 16)
        .padding(.trailing, 12)
        .frame(height: PhoneStyle.touchTarget)
        .disabled(closing)
    }

    @ViewBuilder private var trailing: some View {
        if closing {
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text(Lang.tr("正在关闭", "Closing…")).font(PhoneStyle.label).foregroundStyle(.secondary)
            }
        } else if askingClose {
            ConfirmButtons(title: Lang.tr("关闭", "Close"), confirming: $confirming) { intents.close(window) }
        } else {
            Button { confirming = .close(window.key) } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(PhoneStyle.headline.weight(.regular))
                    .foregroundStyle(.tertiary)
                    .frame(width: PhoneStyle.touchTarget, height: PhoneStyle.touchTarget)
            }
            .buttonStyle(PressableStyle())
            .accessibilityLabel(Lang.tr("关闭窗口", "Close Window"))
        }
    }
}

/// 窗口行右边的小标签：在手机屏上、已最小化
private struct Tag: View {
    let text: String
    let color: Color

    var body: some View {
        Text(text)
            .font(PhoneStyle.label)
            .foregroundStyle(color)
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .background(color.opacity(0.12), in: Capsule())
    }
}

/// 手机屏上那个窗口的字号和还原，排在它下面一行、和标题对齐。这个 App 调不了字号时只有还原
private struct PlacedControls: View {
    let placed: PhoneDesk.Placed
    let work: PhoneDesk.Work?
    let intents: any PhoneIntents

    var body: some View {
        HStack(spacing: 12) {
            if placed.zoomable {
                RoundButton(symbol: "textformat.size.smaller", label: Lang.tr("缩小字号", "Zoom Out Text")) { intents.zoom(by: -1) }
                    .disabled(placed.zoom <= ZoomBook.range.lowerBound)
                Text(zoomText)
                    .font(PhoneStyle.body.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(minWidth: 72)
                    .contentTransition(.numericText())
                RoundButton(symbol: "textformat.size.larger", label: Lang.tr("放大字号", "Zoom In Text")) { intents.zoom(by: 1) }
                    .disabled(placed.zoom >= ZoomBook.range.upperBound)
            }
            Spacer(minLength: 8)
            Button { intents.putBack() } label: {
                HStack(spacing: 8) {
                    if work == .puttingBack {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: "arrow.uturn.backward")
                    }
                    Text(Lang.tr("还原", "Restore"))
                }
                .font(PhoneStyle.body.weight(.medium))
                .padding(.horizontal, 20)
                .frame(height: PhoneStyle.touchTarget)
                .background(PhoneStyle.background, in: Capsule())
            }
            .buttonStyle(PressableStyle())
        }
        .padding(.leading, 80)
        .padding(.trailing, 12)
        .padding(.bottom, 12)
    }

    private var zoomText: String {
        placed.zoom == 0 ? Lang.tr("默认字号", "Default size") : Lang.tr("字号", "Text") + " \(placed.zoom > 0 ? "+" : "")\(placed.zoom)"
    }
}
