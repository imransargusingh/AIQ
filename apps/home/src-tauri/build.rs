fn main() {
    // Only these twenty-four app commands exist; the capability file allows exactly these (design: STRIDE, E).
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
        "snapshot",
        "pause",
        "team_tips",
        "set_setting",
        "forget_all",
        "task",
        "forget_item",
        "finish_setup",
        "tour_seen",
        "guidance_flag",
        "tip_on",
        "tip_off",
        "connect_tools",
        "run_update",
        "disconnect_tool",
        "include_folder",
        "judge_preview",
        "open_privacy",
        "open_sign_in",
        "open_tool_help",
        "analyze_task",
        "diagnostics",
        "save_diagnostics",
        "show_diagnostics",
    ])))
    .expect("tauri build");
}
