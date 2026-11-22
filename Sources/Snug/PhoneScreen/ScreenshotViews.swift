// 截图后浮在小横条上方的两种浮层：缩略图卡片（点它进裁剪）和一句结果提示。
// 面板按内容大小摆，所以尺寸直接乘这一档的倍率，不用 scaleEffect（它不改布局尺寸，面板会量错）
import SnugCore
import SwiftUI

/// 刚截好：缩略图 +「已复制截图 · 点按裁剪」
struct ScreenshotThumbnail: View {
    let image: NSImage
    /// 这一档比基准宽度宽几倍：跟着手机屏一起放大，在手机上看着大小不变
    let scale: CGFloat
    let onOpen: () -> Void

    var body: some View {
        Button(action: onOpen) {
            HStack(spacing: 12 * scale) {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .frame(width: 48 * scale, height: 48 * scale)
                    .clipShape(RoundedRectangle(cornerRadius: 8 * scale, style: .continuous))
                OverlayText(headline: Lang.tr("已复制截图", "Screenshot copied"), detail: Lang.tr("点按裁剪", "Tap to crop"), scale: scale)
            }
            .padding(12 * scale)
            .overlayCard(scale: scale)
        }
        .buttonStyle(PressableStyle())
        .fixedSize()
        .accessibilityLabel(Lang.tr("截图已复制，点按裁剪", "Screenshot copied, tap to crop"))
    }
}

/// 一句结果：裁好了，或者没截成、为什么
struct ScreenshotNoticeView: View {
    let notice: ScreenshotNotice
    let scale: CGFloat

    var body: some View {
        let text = notice.text
        HStack(spacing: 10 * scale) {
            Image(systemName: notice.symbol)
                .font(.system(size: 20 * scale, weight: .semibold))
                .foregroundStyle(.white)
            OverlayText(headline: text.headline, detail: text.detail, scale: scale)
        }
        .padding(.horizontal, 16 * scale)
        .padding(.vertical, 12 * scale)
        .overlayCard(scale: scale)
        .fixedSize()
    }
}

private extension View {
    /// 两种浮层共用的深色圆角底
    func overlayCard(scale: CGFloat) -> some View {
        background(.black.opacity(0.82), in: RoundedRectangle(cornerRadius: 16 * scale, style: .continuous))
    }
}

/// 白字标题 + 一行淡一些的说明，字号同 PhoneStyle.body / label
private struct OverlayText: View {
    let headline: String
    let detail: String?
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 2 * scale) {
            Text(headline).font(.system(size: 18 * scale, weight: .semibold))
            if let detail {
                Text(detail).font(.system(size: 15 * scale, weight: .medium)).opacity(0.7)
            }
        }
        .foregroundStyle(.white)
    }
}
