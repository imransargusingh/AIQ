//! `aiq-statusline` depends on no other AIQ crate so it can stay byte-for-byte unchanged across releases (design
//! auto-update U5n). It restates the core's data folder, pipe name, token path, limits and request shape; this
//! test fails the moment any of them differs from the core's own definition.
use aiq_contracts::frames::{MAX_FRAME_BYTES, Request};
use aiq_core::paths::Paths;
use aiq_statusline::Dirs;

fn same(p: &Paths, d: &Dirs) {
    assert_eq!(p.data, d.data);
    assert_eq!(p.runtime, d.runtime);
    assert_eq!(p.token(), d.token());
    assert_eq!(p.pause_flag(), d.pause_flag());
    assert_eq!(p.hook_timings(), d.hook_timings());
    assert_eq!(p.hook_timings_prev(), d.hook_timings_prev());
    #[cfg(windows)]
    assert_eq!(p.pipe_name(), d.pipe_name());
    #[cfg(not(windows))]
    assert_eq!(p.socket(), d.socket());
}

#[test]
fn the_status_line_program_reaches_the_same_core_with_the_same_frame() {
    same(&Paths::from_env(), &Dirs::from_env());
    let d = tempfile::tempdir().unwrap();
    // SAFETY: the only test in this binary, so no other thread reads the environment meanwhile.
    unsafe { std::env::set_var("AIQ_HOME", d.path()) };
    same(&Paths::from_env(), &Dirs::from_env());

    for session in [Some("c1"), None] {
        let theirs: serde_json::Value = serde_json::from_str(&aiq_statusline::tip_line_frame("tok", session)).unwrap();
        let ours = Request::TipLine {
            token: "tok".into(),
            surface: aiq_core::service::COPILOT_CLI.into(),
            session: session.map(str::to_owned),
        };
        assert_eq!(theirs, serde_json::to_value(&ours).unwrap());
        assert_eq!(serde_json::from_value::<Request>(theirs).unwrap(), ours);
    }
    assert_eq!(aiq_statusline::MAX_FRAME_BYTES, MAX_FRAME_BYTES);
    assert_eq!(aiq_statusline::MAX_SESSION_ID, aiq_contracts::event::MAX_SESSION_ID);
}
