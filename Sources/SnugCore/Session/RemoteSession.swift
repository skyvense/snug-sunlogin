// 会话层的组合：连接判定 + 会话生命周期，把每拍观测变成功能模块要处理的会话变化。
// 功能模块（手机屏）只消费会话状态，会话层不知道有哪些功能
import Foundation

public enum SessionTransition: Equatable, Sendable {
    /// 新会话开始
    case began(controllers: [String])
    /// 重连窗口内再连上，仍是同一会话
    case resumed(controllers: [String])
    case controllersChanged([String])
    /// 断开，进入重连窗口
    case interrupted
    /// 重连窗口过了，会话结束
    case ended
}

public struct RemoteSession: Sendable {
    public private(set) var tracker = ConnectionTracker()
    public private(set) var episode = SessionEpisode()
    /// 这次远程最后读到的那组控制端。断开那一拍 tracker 会清空，这里在重连窗口里照样记着
    private var group: [String] = []
    /// 几台同时连着时先离开的控制端和离开的时刻。别的控制端还连着，整次远程就不算断开、进不了重连窗口，
    /// 这里给离开的那台补上同样的窗口：表格漏读一拍（issue #3 的日志里有过）、手机短暂断网，手机屏和上面的窗口都还在
    private var departed: [String: Date] = [:]
    /// 这次远程从什么时候开始；不在远程里是 nil
    public private(set) var episodeBeganAt: Date?

    public init() {}

    public var isConnected: Bool { tracker.isConnected }
    /// 此刻连着的控制端（断开时为空）
    public var controllers: [String] { tracker.controllers }
    public var inEpisode: Bool { episode.isActive }

    /// 这次远程算在内的控制端：最后读到的那组，加上离开还不到重连窗口的
    public func episodeControllers(at now: Date) -> [String] {
        let lingering = departed.filter { now.timeIntervalSince($0.value) <= SessionEpisode.reconnectWindow }
        return group + lingering.keys.sorted()
    }

    public mutating func update(_ signals: SunloginSignals, now: Date) -> [SessionTransition] {
        var out: [SessionTransition] = []
        if episode.advance(to: now) == .ended {
            out.append(.ended)
            group = []
            departed = [:]
            episodeBeganAt = nil
        }
        switch tracker.update(signals, now: now) {
        case .connected(let names)?:
            if episode.connected(at: now) == .began {
                episodeBeganAt = now
                group = names
                departed = [:]
                out.append(.began(controllers: names))
            } else {
                if !names.isEmpty { regroup(names, at: now) }
                out.append(.resumed(controllers: names))
            }
        case .controllersChanged(let names)?:
            regroup(names, at: now)
            out.append(.controllersChanged(names))
        case .disconnected?:
            if episode.disconnected(at: now) == .interrupted { out.append(.interrupted) }
        case nil:
            break
        }
        return out
    }

    /// 换成新的一组：不在新组里的记下离开时刻，回来的不再算离开
    private mutating func regroup(_ names: [String], at now: Date) {
        for name in group where !names.contains(name) { departed[name] = now }
        for name in names { departed[name] = nil }
        group = names
    }

    public mutating func noteSelfInducedChange(at date: Date) {
        tracker.noteSelfInducedChange(at: date)
    }
}
