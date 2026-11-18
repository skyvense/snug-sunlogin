// 向日葵（AweSun）的身份。各处认向日葵（感知、手势只接管它发来的操作、App 资源库里排除它）都认这里。
//
// 这是从 Snug 的 UU远程 版移植到向日葵时唯一集中改动的身份文件。移植说明见仓库根目录 PORTING.md。
import Foundation

public enum Sunlogin {
    /// 向日葵 Mac 主程序（AweSun.app）。bundle id 已由公开资料核对：
    /// 安装收据 com.oray.sunlogin.macclient、Homebrew cask `awesun` 均为此值
    public static let bundleID = "com.oray.sunlogin.macclient"
    /// 被远程时在本机推流、注入鼠标键盘、放覆盖窗口的进程（按可执行文件名认）。
    ///
    /// ⚠️ 待实机核对：UU远程 里这三件事由同一个 UURemoteServer 进程做；向日葵把工作拆在
    /// 主程序 AweSun 与若干 com.oray.awesun.* 后台项之间，推流进程与注入进程可能不是同一个。
    /// 先以主程序 AweSun 为候选值。在被控 Mac 上运行 scripts/discover-sunlogin.sh，
    /// 按它抓到的真实进程名改这一行即可（日志判定、事件来源、覆盖窗口三处都只认这里）。
    public static let serverProcess = "AweSun"
}
