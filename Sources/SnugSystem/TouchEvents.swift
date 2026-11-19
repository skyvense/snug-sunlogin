// 手势翻译和 CGEvent 之间的读写：接管哪几种事件、事件类型对应 TouchInput 的哪一种、拼要补发的事件、打上记号发出去；
// SunloginEventSource 认哪些事件是 AweSun 注入的。监听线程、状态机和按 TouchCommand 分派在 TouchTap。
// 补发的事件打上记号：拷贝出来的事件保留 向日葵 的进程号，不打记号会被自己再接管一次，来回打转
import CoreGraphics
import Darwin
import Foundation
import SnugCore

enum TouchEvents {
    /// eventSourceUserData 里的记号（'SNUG'）
    private static let marker: Int64 = 0x534E_5547

    /// 接管的事件：只有鼠标左键（向日葵 把手指的按下、拖动、抬起都当成左键发过来）
    static let mask = [CGEventType.leftMouseDown, .leftMouseDragged, .leftMouseUp].reduce(CGEventMask(0)) { $0 | (1 << $1.rawValue) }

    static func kind(of type: CGEventType) -> TouchInput.Kind? {
        switch type {
        case .leftMouseDown: .press
        case .leftMouseDragged: .drag
        case .leftMouseUp: .release
        default: nil
        }
    }

    /// Snug 自己补发的事件
    static func isReplay(_ event: CGEvent) -> Bool {
        event.getIntegerValueField(.eventSourceUserData) == marker
    }

    /// 打上记号再发：监听再看到它时原样放行
    static func post(_ event: CGEvent?) {
        guard let event else { return }
        event.setIntegerValueField(.eventSourceUserData, value: marker)
        event.post(tap: .cgSessionEventTap)
    }

    /// 右键按下、抬起
    static func rightClick(at point: CGPoint) -> [CGEvent] {
        [CGEventType.rightMouseDown, .rightMouseUp].compactMap { type in
            let click = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: point, mouseButton: .right)
            click?.setIntegerValueField(.mouseEventClickState, value: 1)
            return click
        }
    }

    /// 带相位的像素滚动：App 看到的和触控板两指滑动一样
    static func scroll(_ s: ScrollStep) -> CGEvent? {
        guard let e = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2,
                              wheel1: s.pixelsY, wheel2: s.pixelsX, wheel3: 0) else { return nil }
        e.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
        e.setIntegerValueField(.scrollWheelEventScrollPhase, value: s.phase.rawValue)
        e.setIntegerValueField(.scrollWheelEventMomentumPhase, value: s.momentum.rawValue)
        e.setDoubleValueField(.scrollWheelEventFixedPtDeltaAxis1, value: s.deltaY)
        e.setDoubleValueField(.scrollWheelEventFixedPtDeltaAxis2, value: s.deltaX)
        e.location = s.location
        return e
    }
}

/// 认 AweSun 注入的事件：按来源进程号查可执行文件名，按进程号缓存。本机的触控板、鼠标不是它发的，一律不认
struct SunloginEventSource {
    private var known: [pid_t: String] = [:]

    /// 事件来源进程的可执行文件名；进程号查不到（本机硬件输入的来源进程号多半是 0）时返回空串
    mutating func sourceName(_ event: CGEvent) -> String {
        let pid = pid_t(event.getIntegerValueField(.eventSourceUnixProcessID))
        guard pid > 0 else { return "" }
        if let name = known[pid] { return name }
        var buffer = [CChar](repeating: 0, count: Int(MAXPATHLEN))
        let length = Int(proc_pidpath(pid, &buffer, UInt32(buffer.count)))
        let path = length > 0 ? String(decoding: buffer.prefix(length).map { UInt8(bitPattern: $0) }, as: UTF8.self) : ""
        let name = (path as NSString).lastPathComponent
        known[pid] = name
        return name
    }

    mutating func sent(_ event: CGEvent) -> Bool {
        sourceName(event) == Sunlogin.serverProcess
    }
}
