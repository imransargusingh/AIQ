fn main() {
    // Only these four app commands exist; the capability file allows exactly these (design: STRIDE, E).
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["snapshot", "pause", "set_setting", "forget_all"])),
    )
    .expect("tauri build");
}
