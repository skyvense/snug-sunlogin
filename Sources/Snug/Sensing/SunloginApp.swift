// 向日葵 自身的几样直接观测：进程号、覆盖窗口在不在屏
import AppKit
import CoreGraphics
import SnugCore

@MainActor
enum SunloginApp {
    /// 向日葵 主程序的进程号；没在运行返回 nil
    static func pid() -> pid_t? {
        NSRunningApplication.runningApplications(withBundleIdentifier: Sunlogin.bundleID).first?.processIdentifier
    }

    /// AweSun（被远程时推流的后台进程）的进程号；没在运行返回 nil。它没有 bundle ID，按可执行文件名认
    static func serverPID() -> pid_t? {
        NSWorkspace.shared.runningApplications
            .first { $0.executableURL?.lastPathComponent == Sunlogin.serverProcess }?.processIdentifier
    }

    /// AweSun 的覆盖窗口在屏：被远程时 向日葵 会在本机屏幕上放一个提示窗口
    static func overlayVisible() -> Bool {
        guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else {
            return false
        }
        return list.contains { ($0[kCGWindowOwnerName as String] as? String) == Sunlogin.serverProcess }
    }
}
