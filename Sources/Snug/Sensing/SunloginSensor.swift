// 每拍合成一份对 向日葵 的观测：覆盖窗口（同步查）、控制端表格（后台读）、推流（日志子进程）
import Foundation
import SnugCore

/// 一拍的观测
struct SunloginSample: Equatable {
    var signals: SunloginSignals
    /// 向日葵 此刻在推的屏（推流日志里解析出来的）；不知道就是 nil
    var streamingDisplay: DisplayID?
}

@MainActor
final class SunloginSensor {
    private let controllers = ControllerReader()
    private let captureLog = CaptureLogWatcher()

    func start() { captureLog.start() }
    func stop() { captureLog.stop() }

    /// now 是墙钟：推流日志的时间戳是墙钟
    func sample(now: Date) -> SunloginSample {
        let capture = captureLog.sample(at: now)
        return SunloginSample(signals: SunloginSignals(overlayVisible: SunloginApp.overlayVisible(),
                                           controllers: controllers.poll(),
                                           captureActive: capture.active),
                        streamingDisplay: capture.display)
    }
}
