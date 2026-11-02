//! The person's controls, shared by the `aiq` CLI and AIQ home: ask the running core, pause, change a
//! setting, delete everything. One definition, so both front ends behave the same.
use crate::ipc;
use crate::paths::Paths;
use aiq_contracts::frames::{OkReply, Request};
use std::time::Duration;

/// Send one request to the running core and return its reply line. None when the core is not reachable.
pub fn ask(paths: &Paths, timeout: Duration, req: impl FnOnce(String) -> Request) -> Option<String> {
    let token = ipc::read_token(paths)?;
    let frame = serde_json::to_string(&req(token)).ok()?;
    ipc::request(paths, &frame, true, timeout).ok().flatten()
}

/// Paused is the flag file: hooks and the status line read it without the core.
pub fn is_paused(paths: &Paths) -> bool {
    paths.pause_flag().exists()
}

/// Pause or resume. The flag file is the switch; the core records the change now, or reconciles with
/// the flag when it next starts.
pub fn set_paused(paths: &Paths, on: bool, timeout: Duration) -> std::io::Result<()> {
    let _ = paths.ensure_dirs();
    if on {
        crate::paths::write_user_only(&paths.pause_flag(), b"paused\n")?;
    } else {
        match std::fs::remove_file(paths.pause_flag()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    let value = if on { "true" } else { "false" };
    let _ = ask(paths, timeout, |token| Request::Setting { token, key: "paused".into(), value: value.into() });
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlError {
    /// The core is not running, so a change that only the core may write cannot be made.
    NotRunning,
    /// The core refused, with its reason.
    Refused(String),
    /// A local file operation failed.
    Io(String),
}

fn ok_reply(line: Option<String>) -> Result<(), ControlError> {
    match line.and_then(|l| serde_json::from_str::<OkReply>(&l).ok()) {
        Some(OkReply { ok: true, .. }) => Ok(()),
        Some(OkReply { message, .. }) => Err(ControlError::Refused(message.unwrap_or_default())),
        None => Err(ControlError::NotRunning),
    }
}

/// Change a setting through the core (the only writer). The core validates the value.
pub fn set_setting(paths: &Paths, key: &str, value: &str, timeout: Duration) -> Result<(), ControlError> {
    ok_reply(ask(paths, timeout, |token| Request::Setting { token, key: key.into(), value: value.into() }))
}

/// Delete everything AIQ stored: through the core when it runs, else by deleting the files.
pub fn forget_all(paths: &Paths, timeout: Duration) -> Result<(), ControlError> {
    match ok_reply(ask(paths, timeout, |token| Request::Forget { token })) {
        Err(ControlError::NotRunning) => {
            crate::server::forget_files(paths).map_err(|e| ControlError::Io(e.to_string()))
        }
        other => other,
    }
}

/// Delete one prompt's text (design aiq-home-deferred-pages E3): through the core when it runs, because it
/// also holds the text in memory; else directly in the store, as nobody else writes while the core is down.
pub fn forget_item(paths: &Paths, event_id: &str, timeout: Duration) -> Result<(), ControlError> {
    match ok_reply(ask(paths, timeout, |token| Request::ForgetItem { token, event_id: event_id.into() })) {
        Err(ControlError::NotRunning) => {
            let db = paths.db();
            if !db.exists() {
                return Err(ControlError::Refused("that item has no stored text to delete".into()));
            }
            let store = aiq_store::Store::open(&db).map_err(|e| ControlError::Io(e.to_string()))?;
            match store.forget_content(event_id) {
                Ok(true) => Ok(()),
                Ok(false) => Err(ControlError::Refused("that item has no stored text to delete".into())),
                Err(e) => Err(ControlError::Io(e.to_string())),
            }
        }
        other => other,
    }
}

/// AIQ home's first-run setup finished.
pub fn setup_done(paths: &Paths) -> bool {
    paths.setup_flag().exists()
}

pub fn finish_setup(paths: &Paths) -> std::io::Result<()> {
    paths.ensure_dirs()?;
    crate::paths::write_user_only(&paths.setup_flag(), b"done\n")
}

/// Record the person's answer to a tip (coaching-from-rungs D8): through the core when it runs, else
/// directly in the store, as nobody else writes while the core is down.
pub fn tip_feedback(
    paths: &Paths,
    card_id: Option<&str>,
    tip_id: Option<&str>,
    answer: &str,
    surface: &str,
    timeout: Duration,
) -> Result<(), ControlError> {
    let req = |token| Request::TipFeedback {
        token,
        card_id: card_id.map(str::to_owned),
        tip_id: tip_id.map(str::to_owned),
        answer: answer.into(),
        surface: surface.into(),
    };
    match ok_reply(ask(paths, timeout, req)) {
        Err(ControlError::NotRunning) => {
            let store = direct_store(paths)?;
            crate::coaching::record_feedback(&store, card_id, tip_id, answer, surface, now_ms())
                .map(|_| ())
                .map_err(ControlError::Refused)
        }
        other => other,
    }
}

/// "This doesn't fit" (`flag`) or its undo, through the core when it runs, else directly in the store.
#[allow(clippy::too_many_arguments)]
pub fn guidance_flag(
    paths: &Paths,
    job: &str,
    task_type: &str,
    move_id: &str,
    action: &str,
    reason: Option<&str>,
    note: Option<&str>,
    timeout: Duration,
) -> Result<(), ControlError> {
    let req = |token| Request::GuidanceFlag {
        token,
        job: job.into(),
        task_type: task_type.into(),
        move_id: move_id.into(),
        action: action.into(),
        reason: reason.map(str::to_owned),
        note: note.map(str::to_owned),
    };
    match ok_reply(ask(paths, timeout, req)) {
        Err(ControlError::NotRunning) => {
            let store = direct_store(paths)?;
            let pack = crate::pack::parse(crate::pack::BUNDLED_TOML).map_err(ControlError::Io)?;
            crate::coaching::record_flag(&store, &pack, job, task_type, move_id, action, reason, note, now_ms())
                .map_err(ControlError::Refused)
        }
        other => other,
    }
}

fn direct_store(paths: &Paths) -> Result<aiq_store::Store, ControlError> {
    if !paths.db().exists() {
        return Err(ControlError::Refused("AIQ has no data yet".into()));
    }
    aiq_store::Store::open(&paths.db()).map_err(|e| ControlError::Io(e.to_string()))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_flag_round_trip_without_core() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        assert!(!is_paused(&p));
        set_paused(&p, true, Duration::from_millis(50)).unwrap();
        assert!(is_paused(&p));
        set_paused(&p, false, Duration::from_millis(50)).unwrap();
        set_paused(&p, false, Duration::from_millis(50)).unwrap();
        assert!(!is_paused(&p));
    }

    #[test]
    fn setting_without_core_is_not_running() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        assert_eq!(set_setting(&p, "daily_cap", "3", Duration::from_millis(50)), Err(ControlError::NotRunning));
    }

    #[test]
    fn setup_flag_round_trip() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(&d.path().join("fresh"));
        assert!(!setup_done(&p));
        finish_setup(&p).unwrap();
        assert!(setup_done(&p));
    }

    #[test]
    fn forget_item_without_core_deletes_in_the_store() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        assert!(matches!(forget_item(&p, "e1", Duration::from_millis(50)), Err(ControlError::Refused(_))));
        let s = aiq_store::Store::open(&p.db()).unwrap();
        s.raw()
            .execute_batch(
                "INSERT INTO surface_session VALUES ('claude-code', 's', 0);
                 INSERT INTO event VALUES ('e1', 'claude-code', 's', 'prompt', 1, 1, 'p1', NULL, 't', 5, '{\"has_text\":1}');
                 INSERT INTO content VALUES ('e1', 'hello');",
            )
            .unwrap();
        drop(s);
        forget_item(&p, "e1", Duration::from_millis(50)).unwrap();
        let s = aiq_store::Store::open(&p.db()).unwrap();
        let n: i64 = s.raw().query_row("SELECT count(*) FROM content", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        assert!(matches!(forget_item(&p, "e1", Duration::from_millis(50)), Err(ControlError::Refused(_))));
    }

    #[test]
    fn forget_without_core_deletes_files() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        p.ensure_dirs().unwrap();
        std::fs::write(p.spool(), "x\n").unwrap();
        forget_all(&p, Duration::from_millis(50)).unwrap();
        assert!(!p.spool().exists());
    }
}
