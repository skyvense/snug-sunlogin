// 手机屏上的「手机型号」页：点首页右上角的手机图标进入。最上面是现在用的那台，下面自己填分辨率，
// 再下面按品牌列出机型库（Assets/phone-models.json），顶部能搜型号。选好就回到首页，手机屏当场换成它的比例
// （换的时候整屏写明「正在调整屏幕比例」）。和设置窗口里的「手机型号」是同一份配置
import SnugCore
import SwiftUI

struct PhoneModelPage: View {
    let state: AppState
    @Bindable var pages: PhonePages
    let intents: any PhoneIntents

    private var query: String { pages.trimmedQuery }
    private var current: PhoneChoice { state.config.phone }

    var body: some View {
        VStack(spacing: 0) {
            // 和 iOS 的导航栏一样：标题居中，右边「完成」
            ZStack {
                Text(state.tr("手机型号", "Phone Model")).font(PhoneStyle.headline)
                HStack {
                    Spacer()
                    BarButton(title: state.tr("完成", "Done"), bold: true) { pages.close() }
                }
            }
            .frame(height: PhoneStyle.touchTarget)
            .padding(.horizontal, 20)
            .padding(.top, 12)
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    SearchBar {
                        TextField(state.tr("搜索型号", "Search models"), text: $pages.query).textFieldStyle(.plain)
                        ClearButton(text: $pages.query)
                    }
                    content
                }
                .padding(.horizontal, 20)
                .padding(.top, 8)
                .padding(.bottom, 24)
            }
            .scrollIndicators(.never)
        }
    }

    @ViewBuilder private var content: some View {
        let brands = state.phones.search(query)
        LazyVStack(alignment: .leading, spacing: 24) {
            if query.isEmpty {
                TitledSection(state.tr("当前 · 手机屏按它调整比例", "Current · The phone screen follows its ratio")) { CurrentPhone(choice: current) }
            }
            if brands.isEmpty {
                if query.isEmpty {
                    EmptyState(symbol: "iphone.gen3", title: state.tr("未读取到机型库", "Model library unavailable"), text: state.tr("可在下方自定义分辨率", "You can enter a custom resolution below"))
                } else {
                    EmptyState(symbol: "magnifyingglass", title: state.tr("无结果", "No results"), text: state.tr("没有与「\(query)」相符的型号", "No models match “\(query)”"))
                }
            }
            // 搜不到时也给自定义：列表里没有的手机就靠它
            if query.isEmpty || brands.isEmpty {
                TitledSection(state.tr("自定义", "Custom")) { CustomScreen(current: current) { choose(PhoneChoice(model: nil, screen: $0)) } }
            }
            ForEach(brands) { brand in
                TitledSection(brand.name) {
                    ListCard(brand.models, dividerInset: 20) { model in
                        ModelRow(model: model, selected: model.id == current.model) { choose(PhoneChoice(model)) }
                    }
                }
            }
        }
    }

    private func choose(_ phone: PhoneChoice) {
        pages.close()
        intents.setPhone(phone)
    }
}

/// 现在用的那台：名字、分辨率
private struct CurrentPhone: View {
    let choice: PhoneChoice

    var body: some View {
        HStack(spacing: 16) {
            Image(systemName: "iphone.gen3")
                .font(PhoneStyle.headline.weight(.regular))
                .foregroundStyle(PhoneStyle.tint)
                .frame(width: PhoneStyle.iconSize)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                Text(choice.title).font(PhoneStyle.headline).lineLimit(1)
                Text(choice.screen.sizeText).font(PhoneStyle.body).foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .padding(16)
        .card()
    }
}

/// 自己填分辨率：宽 × 高，横着填也行，比例也行（见 PhoneScreen.custom）。现在用的是自己填的，就先填上它
private struct CustomScreen: View {
    let current: PhoneChoice
    let use: (PhoneScreen) -> Void
    @State private var width = ""
    @State private var height = ""

    private var screen: PhoneScreen? { PhoneScreen.custom(Int(width), Int(height)) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                NumberField(placeholder: Lang.tr("宽", "Width"), text: $width)
                Text("×").font(PhoneStyle.body).foregroundStyle(.secondary)
                NumberField(placeholder: Lang.tr("高", "Height"), text: $height)
                PillButton(title: Lang.tr("使用", "Use")) { screen.map(use) }
                    .disabled(screen == nil)
            }
            Text(PhoneScreen.customHint)
                .font(PhoneStyle.label)
                .foregroundStyle(.secondary)
        }
        .padding(16)
        .card()
        .onAppear {
            guard let screen = current.customScreen else { return }
            width = String(screen.width)
            height = String(screen.height)
        }
    }
}

private struct NumberField: View {
    let placeholder: String
    @Binding var text: String

    var body: some View {
        TextField(placeholder, text: $text)
            .textFieldStyle(.plain)
            .font(PhoneStyle.body.monospacedDigit())
            .multilineTextAlignment(.center)
            .padding(.horizontal, 12)
            .fieldBackground()
            .onChange(of: text) { _, new in
                let digits = new.filter(\.isASCII).filter(\.isNumber)
                if digits != new { text = digits }
            }
    }
}

/// 机型库里的一台：型号、分辨率，选中的打勾
private struct ModelRow: View {
    let model: PhoneModel
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.name).font(PhoneStyle.body).lineLimit(1)
                    Text(model.screen.sizeText).font(PhoneStyle.label).foregroundStyle(.secondary)
                }
                Spacer(minLength: 8)
                if selected {
                    Image(systemName: "checkmark").font(PhoneStyle.body.weight(.semibold)).foregroundStyle(PhoneStyle.tint)
                }
            }
            .padding(.horizontal, 20)
            .frame(minHeight: PhoneStyle.touchTarget + 12)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressableStyle())
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
