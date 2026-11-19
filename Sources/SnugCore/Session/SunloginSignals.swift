// 一拍里对 向日葵 的观测
import Foundation

public struct SunloginSignals: Equatable, Sendable, CustomStringConvertible {
    /// AweSun 的覆盖窗口在屏
    public var overlayVisible: Bool
    /// 向日葵 主窗口的控制端表格
    public var controllers: ControllerTable
    /// AweSun 是否在推流。nil = 未知（日志监听没在跑）
    public var captureActive: Bool?

    public init(overlayVisible: Bool, controllers: ControllerTable, captureActive: Bool?) {
        self.overlayVisible = overlayVisible
        self.controllers = controllers
        self.captureActive = captureActive
    }

    /// 任一路信号给出了在场的正面证据：覆盖窗口在屏、认出了控制端、推流在进行。
    /// 表格读不到或没认出控制端都不算缺席的证据（见 ControllerTable）
    public var isPresent: Bool { overlayVisible || !controllers.names.isEmpty || captureActive == true }

    public var description: String {
        let capture = captureActive.map { $0 ? "是" : "否" } ?? "未知"
        return "overlay=\(overlayVisible ? "是" : "否") 控制端=\(controllers) capture=\(capture)"
    }
}

public enum ConnectionEvent: Equatable, Sendable {
    /// 连入时认出的控制端（可能还没读到，为空）
    case connected(controllers: [String])
    /// 同时连着的控制端变了：有人加入、有人离开
    case controllersChanged([String])
    case disconnected
}
