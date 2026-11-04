//! Keep the core running without the person doing anything (design aiq-home-app, "Core down").
//!
//! Three starters, one guard: a prompt's hook (`aiq` hook path), AIQ home when it finds no core, and Windows
//! at sign-in (`aiq-home.exe --start-core`, from the per-user Run entry the core writes for itself). All of
//! them share the start-time file the hook already uses, so they don't start a core each, and the core's own
//! lock (`server::run_with`) turns away any second core that still slips through.
use crate::paths::Paths;
use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// A start younger than this counts as still starting, and nobody starts another. Same as the hook's backoff.
/// assume: a core reaches its lock well inside 10 s of being started (the lock is its first step, before the
/// store opens). If false, a slow start shows "AIQ couldn't start" until it answers; nothing else breaks.
pub const START_GRACE_S: u64 = 10;

/// What AIQ home shows about the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CoreState {
    /// It answered.
    Running,
    /// It holds its lock but did not answer in time: opening the store, or busy writing. Not an error.
    Busy,
    /// Not running, and a start is under way.
    Starting,
    /// Not running, and the last start did not hold: it failed, or exited at once. AIQ keeps retrying.
    Failed,
}

/// What a starter knows when it decides.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    /// The core answered a status request.
    pub answered: bool,
    /// Another process holds the core's lock.
    pub lock_held: bool,
    /// The core wrote `core-error` the last time it failed to start (it removes the file when it starts).
    pub error_file: bool,
    /// When anyone last started a core (the shared start-time file), in Unix seconds.
    pub last_start_s: Option<u64>,
    pub now_s: u64,
    /// Starts this starter already made without the core answering since.
    pub tries: u32,
}

/// The core's state, and whether to start one now.
pub fn decide(p: &Probe) -> (CoreState, bool) {
    if p.answered {
        return (CoreState::Running, false);
    }
    if p.lock_held {
        return (CoreState::Busy, false);
    }
    let recent = p.last_start_s.is_some_and(|t| p.now_s < t.saturating_add(START_GRACE_S));
    let start = !recent;
    // A start that is now over and left no core behind failed. The error file only counts once this starter has
    // tried: on the first look it may be left over from long ago, and this start may well succeed.
    let failed = p.tries + u32::from(start) > 1 || (p.tries > 0 && p.error_file);
    (if failed { CoreState::Failed } else { CoreState::Starting }, start)
}

pub fn now_s() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// When anyone last started a core.
pub fn last_start(paths: &Paths) -> Option<u64> {
    std::fs::read_to_string(paths.start_lock()).ok().and_then(|s| s.trim().parse().ok())
}

pub fn mark_start(paths: &Paths, now_s: u64) {
    let _ = paths.ensure_dirs();
    let _ = crate::paths::write_user_only(&paths.start_lock(), now_s.to_string().as_bytes());
}

/// The code of the core's last start failure (`core-error` holds "CODE: detail"), if it left one.
pub fn last_error_code(paths: &Paths) -> Option<String> {
    let text = std::fs::read_to_string(paths.core_error()).ok()?;
    let code = text.split(':').next().unwrap_or("").trim();
    (!code.is_empty()).then(|| code.to_owned())
}

/// True when a core holds its lock. Takes the lock for an instant when it is free.
/// assume: a core starting in that same instant loses its lock and exits with AIQ-CORE-ALREADY-RUNNING. The
/// window is one lock call wide; the caller's next look finds no core and starts one, so the cost is one retry.
pub fn lock_held(paths: &Paths) -> bool {
    let Ok(f) = std::fs::OpenOptions::new().write(true).open(paths.core_lock()) else { return false };
    matches!(f.try_lock(), Err(std::fs::TryLockError::WouldBlock))
}

/// Start `<exe> core` with no window, outside the caller's process group, so it outlives the caller.
pub fn spawn_core(exe: &Path) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mk = || {
        let mut c = Command::new(exe);
        c.arg("core").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        c
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{
            CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS,
        };
        let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        // Same flags as the hook's start (spike S3b): breakaway is refused when the caller's job forbids it.
        mk().creation_flags(base | CREATE_BREAKAWAY_FROM_JOB).spawn().or_else(|_| mk().creation_flags(base).spawn())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::process::CommandExt;
        let mut child = mk().process_group(0).spawn()?;
        // Reap it when it ends, so a dev build never leaves a zombie behind.
        std::thread::spawn(move || child.wait());
        Ok(())
    }
}

/// The Run entry's name under HKCU\Software\Microsoft\Windows\CurrentVersion\Run.
pub const RUN_VALUE: &str = "AIQ";
#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// What Windows runs at sign-in: AIQ home's start-only mode (a windowless exe, so nothing flashes).
pub fn login_command(bin: &Path) -> String {
    format!("\"{}\" --start-core", bin.join("aiq-home.exe").display())
}

/// Only an install made by install.ps1 and not undone (its record is there) with AIQ home beside this exe.
fn login_start_wanted(paths: &Paths) -> bool {
    crate::update::supported(paths)
        && paths.data.join("windows-install.json").exists()
        && paths.data.join("bin").join("aiq-home.exe").exists()
}

/// Make sure Windows starts AIQ at sign-in. Run by the core at each start, so an update reaches it too.
/// Ok(true) when it wrote the entry. Turning AIQ off in Task Manager's Startup apps is kept: Windows records that
/// apart from the entry (StartupApproved), and this only rewrites the entry when its text differs.
pub fn ensure_login_start(paths: &Paths) -> Result<bool, String> {
    if !login_start_wanted(paths) {
        return Ok(false);
    }
    let want = login_command(&paths.data.join("bin"));
    if reg::get(RUN_VALUE).as_deref() == Some(want.as_str()) {
        return Ok(false);
    }
    reg::set(RUN_VALUE, &want)?;
    Ok(true)
}

/// Remove the sign-in entry (`aiq uninstall --all`). Ok(true) when there was one.
pub fn remove_login_start() -> Result<bool, String> {
    reg::delete(RUN_VALUE)
}

#[cfg(windows)]
mod reg {
    use super::RUN_KEY;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn get(name: &str) -> Option<String> {
        let (key, name) = (wide(RUN_KEY), wide(name));
        let mut buf = vec![0u16; 1024];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: both strings are NUL-terminated and outlive the call; buf holds `bytes` bytes.
        let r = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if r != ERROR_SUCCESS {
            return None;
        }
        let n = (bytes as usize / 2).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..n]);
        Some(s.trim_end_matches('\0').to_owned())
    }

    pub fn set(name: &str, value: &str) -> Result<(), String> {
        let (key, name, data) = (wide(RUN_KEY), wide(name), wide(value));
        // SAFETY: NUL-terminated strings that outlive the call; the data length is in bytes, NUL included.
        let r = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if r == ERROR_SUCCESS { Ok(()) } else { Err(format!("the sign-in entry could not be written (error {r})")) }
    }

    pub fn delete(name: &str) -> Result<bool, String> {
        let (key, name) = (wide(RUN_KEY), wide(name));
        // SAFETY: NUL-terminated strings that outlive the call.
        let r = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
        match r {
            ERROR_SUCCESS => Ok(true),
            ERROR_FILE_NOT_FOUND => Ok(false),
            _ => Err(format!("the sign-in entry could not be removed (error {r})")),
        }
    }
}

/// Windows only: elsewhere there is no sign-in entry to keep.
#[cfg(not(windows))]
mod reg {
    pub fn get(_: &str) -> Option<String> {
        None
    }
    pub fn set(_: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    pub fn delete(_: &str) -> Result<bool, String> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe() -> Probe {
        Probe { answered: false, lock_held: false, error_file: false, last_start_s: None, now_s: 1000, tries: 0 }
    }

    #[test]
    fn a_core_that_answers_or_holds_its_lock_is_never_started_again() {
        assert_eq!(decide(&Probe { answered: true, ..probe() }), (CoreState::Running, false));
        assert_eq!(decide(&Probe { lock_held: true, tries: 3, ..probe() }), (CoreState::Busy, false));
    }

    #[test]
    fn no_core_starts_one_and_says_starting() {
        assert_eq!(decide(&probe()), (CoreState::Starting, true));
        // An old error file does not make the first start look failed.
        assert_eq!(decide(&Probe { error_file: true, ..probe() }), (CoreState::Starting, true));
    }

    #[test]
    fn a_recent_start_by_anyone_is_not_doubled() {
        let p = Probe { last_start_s: Some(995), ..probe() };
        assert_eq!(decide(&p), (CoreState::Starting, false));
        assert_eq!(decide(&Probe { last_start_s: Some(1000 - START_GRACE_S), ..probe() }), (CoreState::Starting, true));
    }

    #[test]
    fn a_start_that_left_no_core_is_failed_and_retried() {
        // The core wrote its error after this starter's try: failed at once, no second start inside the grace.
        assert_eq!(
            decide(&Probe { tries: 1, error_file: true, last_start_s: Some(998), ..probe() }),
            (CoreState::Failed, false)
        );
        // The grace ran out with no core and no lock: failed, and try again.
        assert_eq!(decide(&Probe { tries: 1, last_start_s: Some(900), ..probe() }), (CoreState::Failed, true));
        // Still inside the grace, no error: still starting.
        assert_eq!(decide(&Probe { tries: 1, last_start_s: Some(998), ..probe() }), (CoreState::Starting, false));
    }

    #[test]
    fn the_lock_probe_sees_a_held_lock_and_frees_a_free_one() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path());
        paths.ensure_dirs().unwrap();
        assert!(!lock_held(&paths), "no lock file: no core");
        let held =
            std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(paths.core_lock()).unwrap();
        held.try_lock().unwrap();
        assert!(lock_held(&paths));
        drop(held);
        assert!(!lock_held(&paths));
        assert!(!lock_held(&paths), "the probe itself never leaves the lock held");
    }

    #[test]
    fn start_time_and_error_code_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path());
        assert_eq!(last_start(&paths), None);
        mark_start(&paths, 1234);
        assert_eq!(last_start(&paths), Some(1234));
        assert_eq!(last_error_code(&paths), None);
        std::fs::write(paths.core_error(), "AIQ-STORE-OPEN: database is locked\n").unwrap();
        assert_eq!(last_error_code(&paths).as_deref(), Some("AIQ-STORE-OPEN"));
    }

    #[test]
    fn the_sign_in_command_runs_home_in_start_only_mode() {
        let c = login_command(Path::new(r"C:\Users\x\.aiq\bin"));
        assert!(c.starts_with('"') && c.ends_with("aiq-home.exe\" --start-core"), "{c}");
    }

    #[test]
    fn no_sign_in_entry_outside_an_installed_copy() {
        let dir = tempfile::tempdir().unwrap();
        // The test binary is not in <data>\bin, so it is never an installed copy.
        assert_eq!(ensure_login_start(&Paths::at(dir.path())), Ok(false));
    }
}
