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

    /// 上一拍记下的悬浮窗候选清单；只在集合变化时记一行，避免每拍刷屏
    private var lastOverlayDump: [String] = []

    /// now 是墙钟：推流日志的时间戳是墙钟
    func sample(now: Date) -> SunloginSample {
        let capture = captureLog.sample(at: now)
        let overlay = SunloginApp.overlayVisible()
        // 诊断：overlay 为真时把命中的窗口清单记下来（变化才记），认出没被控时常驻的那个窗口
        if overlay {
            let dump = SunloginApp.overlayWindowDescriptions()
            if dump != lastOverlayDump {
                lastOverlayDump = dump
                Log.info("悬浮窗候选（owner=AweSun 全部在屏窗口）：\(dump.joined(separator: "；"))")
            }
        } else if !lastOverlayDump.isEmpty {
            lastOverlayDump = []
            Log.info("悬浮窗候选清空")
        }
        return SunloginSample(signals: SunloginSignals(overlayVisible: overlay,
                                           controllers: controllers.poll(),
                                           captureActive: capture.active),
                        streamingDisplay: capture.display)
    }
}
