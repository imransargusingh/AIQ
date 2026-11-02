fn main() {
    // Only these ten app commands exist; the capability file allows exactly these (design: STRIDE, E).
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
        "snapshot",
        "pause",
        "set_setting",
        "forget_all",
        "task",
        "forget_item",
        "finish_setup",
        "guidance_flag",
        "tip_on",
        "connect_tools",
    ])))
    .expect("tauri build");
}
