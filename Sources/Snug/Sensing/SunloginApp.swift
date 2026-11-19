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

    /// AweSun_Desktop（被远程时推流的桌面代理）的进程号；没在运行返回 nil。它没有 bundle ID，按可执行文件名认
    static func serverPID() -> pid_t? {
        NSWorkspace.shared.runningApplications
            .first { $0.executableURL?.lastPathComponent == Sunlogin.serverProcess }?.processIdentifier
    }

    /// 向日葵的覆盖窗口在屏：被控时主程序 AweSun 会放一个高图层的悬浮窗（实测图层 1001）。
    /// 与推流进程 AweSun_Desktop 不是同一个进程，所以认 Sunlogin.overlayWindowOwner，
    /// 且要求图层够高，免得把主程序的普通窗口（图层 0）误认成覆盖窗口
    static func overlayVisible() -> Bool {
        guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else {
            return false
        }
        return list.contains {
            ($0[kCGWindowOwnerName as String] as? String) == Sunlogin.overlayWindowOwner
                && (($0[kCGWindowLayer as String] as? Int) ?? 0) >= Sunlogin.overlayWindowMinimumLayer
        }
    }
}
