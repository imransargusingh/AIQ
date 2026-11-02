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
    fn forget_without_core_deletes_files() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        p.ensure_dirs().unwrap();
        std::fs::write(p.spool(), "x\n").unwrap();
        forget_all(&p, Duration::from_millis(50)).unwrap();
        assert!(!p.spool().exists());
    }
}
