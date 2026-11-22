// 权限在系统设置里的叫法：菜单、设置页、欢迎页、手机屏提示都用它，用户照着字面就能在系统设置里找到
import Foundation
import PermissionFlow

enum PermissionName {
    /// macOS 27 起叫「设备控制和数据访问」，更早叫「辅助功能」
    static var accessibility: String {
        PermissionFlowResources.localizedString(
            for: PermissionFlowResources.accessibilityName(), localeIdentifier: Lang.english ? "en" : "zh-Hans")
    }
    /// macOS 15 起叫「录屏与系统录音」，更早叫「屏幕录制」（PermissionFlow 只有旧叫法）
    static var screenRecording: String {
        let recent = ProcessInfo.processInfo.operatingSystemVersion.majorVersion >= 15
        return Lang.english ? (recent ? "Screen & System Audio Recording" : "Screen Recording")
                            : (recent ? "录屏与系统录音" : "屏幕录制")
    }
}
