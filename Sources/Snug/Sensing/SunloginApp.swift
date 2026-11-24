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
        return list.contains { w in
            guard (w[kCGWindowOwnerName as String] as? String) == Sunlogin.overlayWindowOwner
                && ((w[kCGWindowLayer as String] as? Int) ?? 0) >= Sunlogin.overlayWindowMinimumLayer
            else { return false }
            // 0x0 的窗口什么都不显示：2026-11-24 问题机实锤，向日葵没被控时也有一个 layer=1001 的
            // 0x0 占位窗口常驻在屏，只认图层会把「没人连」误判成在场（退出也永远判不下来）；要求有真实面积
            guard let b = w[kCGWindowBounds as String] as? [String: Any],
                  let width = b["Width"] as? Double, let height = b["Height"] as? Double else { return false }
            return width > 0 && height > 0
        }
    }

    /// 诊断用：把 向日葵 名下所有在屏窗口逐个描述出来（图层、 bounds、编号、窗口名），
    /// 用来认出没被控时也常驻在屏、把 overlay 信号顶成真的到底是哪个窗口。只记日志，不参与判定
    static func overlayWindowDescriptions() -> [String] {
        guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else {
            return []
        }
        return list
            .filter { ($0[kCGWindowOwnerName as String] as? String) == Sunlogin.overlayWindowOwner }
            .map { w in
                let layer = (w[kCGWindowLayer as String] as? Int) ?? -1
                let number = (w[kCGWindowNumber as String] as? Int) ?? -1
                let name = (w[kCGWindowName as String] as? String) ?? ""
                var bounds = "?"
                if let b = w[kCGWindowBounds as String] as? [String: Any],
                   let x = b["X"] as? Double, let y = b["Y"] as? Double,
                   let width = b["Width"] as? Double, let height = b["Height"] as? Double {
                    bounds = "\(Int(width))x\(Int(height))@(\(Int(x)),\(Int(y)))"
                }
                return "layer=\(layer) #\(number) \(bounds) 名=\(name.isEmpty ? "（空）" : name)"
            }
            .sorted()
    }
}
