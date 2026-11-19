// 更新源：从哪取 update.json 和包。本 fork 只用 GitHub Release：发版时 update.json 和更新包
// 一起挂在每个版本的 Release 上（见 .github/workflows/release.yml）。上游还有一个腾讯云国内源，
// 本 fork 没有配置（原先指向占位地址，检查更新必然失败），已经去掉
import Foundation

public enum UpdateSource: String, Codable, CaseIterable, Sendable {
    /// GitHub Release
    case github

    public var title: String {
        switch self {
        case .github: "GitHub"
        }
    }

    private static let githubBase = URL(string: "https://github.com/skyvense/snug-sunlogin/releases")!

    /// 最新发布的 update.json
    public var feedURL: URL {
        switch self {
        case .github: Self.githubBase.appendingPathComponent("latest/download/update.json")
        }
    }

    /// 某个版本下的文件
    public func url(of file: String, in version: Version) -> URL {
        switch self {
        case .github: Self.githubBase.appendingPathComponent("download/v\(version)").appendingPathComponent(file)
        }
    }

    /// 自己换不了时，请用户手动下载的地方
    public var downloadPage: URL {
        switch self {
        case .github: Self.githubBase.appendingPathComponent("latest")
        }
    }
}
