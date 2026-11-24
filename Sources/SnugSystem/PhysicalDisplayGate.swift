// 「仅输出手机屏」：连入时把手机屏之外的显示屏整块停用，断开/退出时恢复。
// 停用走桥接层的 SkyLight 私有接口（SnugSetDisplayEnabled）；公开 API 没有这个入口。
// 安全顺序（同 Windows 版 physical_off 的做法）：
// - 关之前先把要关的屏写进记录文件，崩溃/断电后下次启动先按记录恢复（healOnLaunch）
// - 只在手机屏确实在线时才关别的屏，任何时候都不让用户落到一块屏都没有
// - 每次下发后都核对在线列表：没生效的如实记日志并当作没关，不拿假状态骗后面的恢复
import AppKit
import CGVirtualDisplayBridge
import SnugCore

@MainActor
public final class PhysicalDisplayGate {
    private let recordURL: URL
    /// 本进程关掉的屏（已核对确实下线）
    public private(set) var disabled: [DisplayID] = []
    /// 机制不可用只报一次：每拍对账都会走到，不能刷屏
    private var reportedUnavailable = false

    public init(paths: SnugPaths) { recordURL = paths.onlyVirtualRecord }

    public var isHolding: Bool { !disabled.isEmpty }

    /// 启动第一件事：上次若留有记录（崩溃、被强退、断电），先把那些屏恢复回来
    public func healOnLaunch() {
        let recorded = readRecord()
        guard !recorded.isEmpty else { return }
        Log.warn("上次留有 \(recorded.count) 块被关掉的屏（记录 \(recordURL.lastPathComponent)），启动先恢复")
        for id in recorded { setEnabled(id, true) }
        clearRecord()
        // 恢复没生效的下次启动再试：核对还在不在线，留下的重新记回
        let stillOff = recorded.filter { !Self.onlineIDs().contains($0) }
        if !stillOff.isEmpty {
            Log.error("恢复后仍有 \(stillOff.count) 块屏不在线（编号 \(stillOff)），记录保留，下次启动再恢复")
            writeRecord(stillOff)
        }
    }

    /// 连入且手机屏在线时调用：把手机屏之外的屏都关掉（已关的跳过）
    public func applyOnlyVirtual(phone: DisplayID) {
        guard !reportedUnavailable else { return } // 机制已知不可用（见 setEnabled），别每拍重试
        let online = Self.onlineIDs()
        guard online.contains(phone) else { return } // 手机屏不在，绝不关别的屏
        let targets = online.filter { $0 != phone && !disabled.contains($0) }
        guard !targets.isEmpty else { return }
        // 先落盘再动手：关到一半崩溃，记录里也是全的
        writeRecord(disabled + targets)
        for id in targets {
            let name = Screens.name(of: id) // 先记名字：屏一下线就查不到了
            guard setEnabled(id, false) else { continue }
            if waitOnline(id, want: false) {
                disabled.append(id)
                Log.info("「仅输出手机屏」：已关掉显示屏 #\(id)（\(name)）")
            } else {
                Log.warn("显示屏 #\(id) 下发了停用但仍在在线列表里，当作没关掉")
            }
        }
        writeRecord(disabled)
        if disabled.isEmpty { clearRecord() }
    }

    /// 恢复所有本进程关掉的屏。恢复不回来的留在记录里，下次启动 healOnLaunch 再试
    public func restore() {
        guard !disabled.isEmpty else { return }
        let restoring = disabled
        disabled = []
        var failed: [DisplayID] = []
        for id in restoring {
            guard setEnabled(id, true), waitOnline(id, want: true) else {
                Log.error("显示屏 #\(id) 恢复失败，留在记录里下次启动再试")
                failed.append(id)
                continue
            }
            Log.info("「仅输出手机屏」：已恢复显示屏 #\(id)")
        }
        if failed.isEmpty { clearRecord() } else { writeRecord(failed) }
    }

    // MARK: 内部

    /// 下发启用/停用；机制不可用与调用失败分开记。返回是否下发成功（生效与否由调用方核对在线列表）
    private func setEnabled(_ id: DisplayID, _ on: Bool) -> Bool {
        // 机制停用（2026-11-24 真机崩溃，报告 Snug-2026-11-24-172458.ips）：macOS 26 的 SkyLight 里
        // CGSConfigureDisplayEnabled 已是 SLSConfigureDisplayEnabled，崩溃帧在 checkCapacity(CGSConfigData*)——
        // 它第一个参数要的是显示配置对象，不是连接号，按连接号调直接段错误。真实签名核实前不许再调；
        // 真被关掉的屏靠重启恢复（显示配置随 WindowServer 会话重置）
        if !reportedUnavailable {
            reportedUnavailable = true
            Log.error("「仅输出手机屏」的停用机制与这台 macOS 的 SkyLight 不合（真机崩溃），已停用；若有屏还关着，重启 Mac 恢复")
        }
        return false
        // swiftlint:disable:next unreachable_code —— 机制核实后恢复下面的调用
        switch SnugSetDisplayEnabled(id, on) {
        case 0: return true
        case 1:
            if !reportedUnavailable {
                reportedUnavailable = true
                Log.error("停用显示屏的机制不可用（SkyLight 里找不到 CGSConfigureDisplayEnabled），「仅输出手机屏」不生效")
            }
            return false
        default:
            Log.error("显示屏 #\(id) \(on ? "启用" : "停用")下发失败（SkyLight 返回错误）")
            return false
        }
    }

    /// 等一块屏在在线列表里变成想要的状态（停用/恢复都不是瞬时的），最多等约 0.8 秒
    private func waitOnline(_ id: DisplayID, want: Bool) -> Bool {
        for _ in 0..<16 {
            if Self.onlineIDs().contains(id) == want { return true }
            Thread.sleep(forTimeInterval: 0.05)
        }
        return Self.onlineIDs().contains(id) == want
    }

    static func onlineIDs() -> [DisplayID] {
        var count: UInt32 = 0
        guard CGGetOnlineDisplayList(0, nil, &count) == .success, count > 0 else { return [] }
        var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
        guard CGGetOnlineDisplayList(count, &ids, &count) == .success else { return [] }
        return Array(ids.prefix(Int(count)))
    }

    private func readRecord() -> [DisplayID] {
        guard let data = try? Data(contentsOf: recordURL),
              let ids = try? JSONDecoder().decode([DisplayID].self, from: data) else { return [] }
        return ids
    }

    private func writeRecord(_ ids: [DisplayID]) {
        guard let data = try? JSONEncoder().encode(ids) else { return }
        try? data.write(to: recordURL, options: .atomic)
    }

    private func clearRecord() {
        try? FileManager.default.removeItem(at: recordURL)
    }
}
