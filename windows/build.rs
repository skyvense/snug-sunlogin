// 把 app.manifest 嵌进 exe：清单里声明了 requireAdministrator
// （改 VDD 配置、重启驱动设备都需要管理员）和 PerMonitorV2 DPI 感知。
fn main() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("app.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
