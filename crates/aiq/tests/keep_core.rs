//! AIQ home keeps the core running (design aiq-home-app, "Core down"), through the real `aiq` binary: with no
//! core it starts one, which then answers; while that core holds its lock no second start is made, even when
//! the core is slow to answer; a missing exe is a failed start with a code, never a silent one.
use aiq_core::autostart::{self, CoreState};
use aiq_core::paths::Paths;
use aiq_home::{StartOutcome, core_answers, ensure_core};
use std::path::Path;
use std::time::{Duration, Instant};

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn home_starts_one_core_and_never_a_second() {
    let dir = tempfile::tempdir().unwrap();
    // The spawned core finds its folder the way every `aiq` does. The only test in this binary, so no other
    // thread reads the environment while it changes.
    // SAFETY: single-threaded at this point; nothing else in this process reads AIQ_HOME concurrently.
    unsafe { std::env::set_var("AIQ_HOME", dir.path()) };
    let paths = Paths::from_env();
    let exe = Path::new(env!("CARGO_BIN_EXE_aiq"));

    let mut tries = 0;
    let (state, error, outcome) = ensure_core(&paths, false, exe, &mut tries);
    assert_eq!((state, error, outcome, tries), (CoreState::Starting, None, StartOutcome::Started, 1));
    wait_for("the started core to answer", || core_answers(&paths, Duration::from_millis(400)));
    assert!(autostart::lock_held(&paths));

    // A slow answer (Home's 400 ms ask missed) with the lock held is busy: no second start, tries reset.
    assert_eq!(ensure_core(&paths, false, exe, &mut tries), (CoreState::Busy, None, StartOutcome::None));
    assert_eq!(tries, 0);
    // Even with the shared start time long past, a held lock blocks a start.
    autostart::mark_start(&paths, 1);
    assert_eq!(ensure_core(&paths, false, exe, &mut tries), (CoreState::Busy, None, StartOutcome::None));
    assert_eq!(ensure_core(&paths, true, exe, &mut tries), (CoreState::Running, None, StartOutcome::None));

    // A second core started anyway is turned away by the lock, and the first keeps answering.
    autostart::spawn_core(exe).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert!(core_answers(&paths, Duration::from_millis(400)));

    // Stop it (forget everything is the core's own way out), then the lock is free again.
    aiq_core::control::forget_all(&paths, Duration::from_secs(5)).unwrap();
    wait_for("the core to exit", || !autostart::lock_held(&paths));

    // An exe that cannot run is a failed start with a code at once.
    let mut tries = 0;
    let _ = std::fs::remove_file(paths.start_lock());
    let (state, error, outcome) = ensure_core(&paths, false, &dir.path().join("missing-aiq"), &mut tries);
    assert_eq!(state, CoreState::Failed);
    assert_eq!(error.as_deref(), Some("AIQ-HOME-CORE-START"));
    assert!(matches!(outcome, StartOutcome::Refused(_)));
    // Inside the grace no retry; the state stays failed rather than flipping back to starting.
    let again = ensure_core(&paths, false, &dir.path().join("missing-aiq"), &mut tries);
    assert_eq!(again, (CoreState::Failed, Some("AIQ-HOME-CORE-START".into()), StartOutcome::None));
}
