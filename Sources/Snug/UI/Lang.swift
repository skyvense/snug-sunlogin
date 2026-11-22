// 界面语言：和 Windows 版同一套做法——文案在代码里成对写（中文原文 + 英文），
// 取词时按当前语言挑一条。当前语言由用户的选择（config.language）决定：
// 选了中文/English 就用它；选「跟随系统」时看系统首选语言，是中文用中文，否则用 English。
// AppController 在启动和每次改语言时调 apply 把结果同步到这里和 AppState.english：
// SwiftUI 界面读 AppState.english（走 state.tr）保证改语言立刻重画；
// 菜单、状态文案等非界面代码读 Lang.tr（同一份结果的静态镜像）
import SnugCore

@MainActor
enum Lang {
    /// 当前实际显示英文与否（apply 维护）
    static private(set) var english = false

    /// 系统首选语言是不是中文（跟随系统时用它定）
    static var systemIsChinese: Bool {
        (Locale.preferredLanguages.first ?? "").lowercased().hasPrefix("zh")
    }

    /// 选择 + 系统语言 → 实际显示英文与否
    static func isEnglish(_ pref: LanguagePref) -> Bool {
        switch pref {
        case .en: return true
        case .zh: return false
        case .system: return !systemIsChinese
        }
    }

    static func apply(_ pref: LanguagePref) {
        english = isEnglish(pref)
    }

    /// 非界面代码取词（菜单、对话框、状态文案的计算）
    static func tr(_ zh: String, _ en: String) -> String { english ? en : zh }
}

extension AppState {
    /// 界面里取词：读的是可观察的 english，改语言时所有读过它的界面一起重画
    func tr(_ zh: String, _ en: String) -> String { english ? en : zh }
}
