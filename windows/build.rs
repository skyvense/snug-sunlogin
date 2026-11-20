// 把 app.ico 与 app.manifest 嵌进 exe，统一走 winresource 生成的资源脚本：
// 图标是 Mac 版同款金橙标记（资源 ID 1，运行时按这个 ID 取）；
// 清单里声明了 requireAdministrator（改 VDD 配置、重启驱动设备都需要管理员）
// 和 PerMonitorV2 DPI 感知。
// 此前手动走 /MANIFESTINPUT 的做法嵌不了图标，故改走资源脚本这条路。
fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = dir.join("app.manifest");
    let icon = dir.join("assets").join("app.ico");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", icon.display());
    // 版本号以仓库根 VERSION 为单一来源（和 Mac 版共用一个版本号）：
    // 编进程序供标题栏与更新检查读取，也写进 exe 的文件版本信息
    let version_path = dir.join("..").join("VERSION");
    let version = std::fs::read_to_string(&version_path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "0.0.0".to_string());
    println!("cargo:rustc-env=SNUG_VERSION={version}");
    println!("cargo:rerun-if-changed={}", version_path.display());
    // 只在 Windows 目标下嵌：winresource 要调 SDK 的 rc.exe，非 Windows 宿主上没有
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon.to_str().expect("图标路径应为 UTF-8"));
        res.set_manifest_file(manifest.to_str().expect("清单路径应为 UTF-8"));
        res.set("FileVersion", &version);
        res.set("ProductVersion", &version);
        res.compile().expect("嵌入图标与清单失败");
    }
}
