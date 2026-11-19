// 把 app.manifest 嵌进 exe：清单里声明了 requireAdministrator
// （改 VDD 配置、重启驱动设备都需要管理员）和 PerMonitorV2 DPI 感知。
fn main() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("app.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    // 关掉链接器自带的 UAC 片段：它默认写 asInvoker，与本清单的 requireAdministrator
    // 冲突，mt.exe 会报 c1010001（首次 CI 实测）。关掉后以 app.manifest 为准。
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:NO");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
