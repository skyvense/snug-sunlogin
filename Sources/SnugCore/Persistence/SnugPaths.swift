// 数据目录与各状态文件路径的唯一来源。测试里换成临时目录
import Foundation

public struct SnugPaths: Equatable, Sendable {
    public let dataDirectory: URL
    public let logFile: URL

    public init(dataDirectory: URL, logFile: URL) {
        self.dataDirectory = dataDirectory
        self.logFile = logFile
    }

    /// ~/Library/Application Support/Snug/ 与 ~/Library/Logs/SnugSunlogin.log
    public static var standard: SnugPaths {
        let library = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask)[0]
        return SnugPaths(
            dataDirectory: library.appendingPathComponent("Application Support/SnugSunlogin", isDirectory: true),
            logFile: library.appendingPathComponent("Logs/SnugSunlogin.log"))
    }

    /// 用户设置
    public var config: URL { file("config.json") }
    /// 手机屏上那个窗口的还原记录：放上去之前写，放回原位才删；崩溃后下次启动按它还原
    public var phoneRecord: URL { file("phone-screen.json") }
    /// 每个 App 在手机屏上记住的字号级数
    public var zoomBook: URL { file("zoom.json") }
    /// 留下的账：在手机屏上被 Snug 之外关掉的窗口，等它回来时按账放回原位、缩回字号
    public var leftBehind: URL { file("left-behind.json") }
    /// 「仅输出手机屏」关掉的物理屏清单：关之前写、恢复后删；崩溃后下次启动按它先恢复
    public var onlyVirtualRecord: URL { file("only-virtual.json") }

    private func file(_ name: String) -> URL { dataDirectory.appendingPathComponent(name) }
}
