//! Auto-update (design `docs/design/auto-update.md`). One mechanism: the installed `upgrade.ps1` does the
//! network call, the checksum and the swap; this module decides *when*, starts it, and reads back its outcome
//! (`update.json`) for `aiq update` and AIQ home. The core's kicker asks once an hour; a check runs at most once
//! a day; a waiting update is applied only under "Update automatically", and only when AIQ is idle (the script
//! decides idle, because only it sees the processes at the moment of the swap).
use crate::paths::Paths;
use crate::sysclock::parse_rfc3339_ms as parse_utc_ms;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Store key of the person's choice: `auto` (default) installs by itself; `ask` downloads and tells first.
pub const SETTING_UPDATE: &str = "update";
pub const MODES: [&str; 2] = ["auto", "ask"];
pub const DEFAULT_MODE: &str = "auto";
/// A check is due once this long has passed since the last one, successful or not.
pub const CHECK_EVERY_MS: i64 = 24 * 3600 * 1000;
/// How long Home says "AIQ updated itself" after it did.
pub const APPLIED_NOTE_MS: i64 = 7 * 24 * 3600 * 1000;
const KICK_FIRST: Duration = Duration::from_secs(5 * 60);
const KICK_EVERY: Duration = Duration::from_secs(3600);

pub fn validate_mode(value: &str) -> Result<String, String> {
    if MODES.contains(&value) { Ok(value.to_owned()) } else { Err("update must be auto or ask".into()) }
}

/// `update.json`, as `upgrade.ps1` writes it. Every field is optional: a missing file is "never checked".
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct State {
    /// current, waiting, applied, failed or rolled_back.
    pub state: Option<String>,
    pub checked_at: Option<String>,
    pub check_error: Option<String>,
    pub installed: Option<String>,
    pub latest: Option<String>,
    /// The release tag downloaded, checked and waiting to install.
    pub available: Option<String>,
    pub staged: Option<String>,
    /// Why the last apply waited (AIQ home open, a hook or judge running).
    pub blocked: Option<String>,
    /// The version before the last update or rollback, and when it happened.
    pub from: Option<String>,
    pub at: Option<String>,
    pub message: Option<String>,
    /// A tag the person rolled back from; never installed again by itself.
    pub skip: Option<String>,
}

pub fn state_path(paths: &Paths) -> PathBuf {
    paths.data.join("update.json")
}

pub fn script_path(paths: &Paths) -> PathBuf {
    paths.data.join("bin").join("upgrade.ps1")
}

/// Unreadable or missing reads as never checked, which only makes a check due (degrades, never invents).
pub fn read_state(paths: &Paths) -> State {
    std::fs::read(state_path(paths)).ok().and_then(|b| serde_json::from_slice(strip_bom(&b)).ok()).unwrap_or_default()
}

/// Windows PowerShell 5.1's `Set-Content -Encoding UTF8` writes a byte-order mark.
fn strip_bom(b: &[u8]) -> &[u8] {
    b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b)
}

/// The line the shipped `upgrade.ps1` carries. A hand-written `upgrade.ps1` (the dogfood laptop's, which
/// swaps staged dev builds and runs a paid accuracy check) lacks it, so the core never starts that one.
pub const SCRIPT_MARKER: &str = "AIQ-AUTO-UPDATE: 1";

/// Auto-update runs only for an installed copy: this exe sits in `<data>\bin` next to the shipped
/// `upgrade.ps1` (it carries [`SCRIPT_MARKER`]), on Windows. A developer build run from `target\` never
/// updates itself.
pub fn supported(paths: &Paths) -> bool {
    let bin = paths.data.join("bin");
    cfg!(windows)
        && script_is_shipped(paths)
        && std::env::current_exe().ok().and_then(|e| e.parent().map(|p| same_dir(p, &bin))).unwrap_or(false)
}

pub fn script_is_shipped(paths: &Paths) -> bool {
    std::fs::read(script_path(paths))
        .map(|b| String::from_utf8_lossy(&b).lines().take(40).any(|l| l.contains(SCRIPT_MARKER)))
        .unwrap_or(false)
}

fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Ask for a newer release; with `apply`, install it straight away when idle.
    Check {
        apply: bool,
    },
    /// Try again to install the update already downloaded.
    Apply,
    Rollback,
}

/// The kicker's rule. `mode` is the stored setting (None: the default).
pub fn decide(now_ms: i64, st: &State, mode: Option<&str>) -> Option<Action> {
    let auto = mode.unwrap_or(DEFAULT_MODE) == "auto";
    let due = st.checked_at.as_deref().and_then(parse_utc_ms).is_none_or(|t| now_ms - t >= CHECK_EVERY_MS);
    if due {
        Some(Action::Check { apply: auto })
    } else if auto && st.state.as_deref() == Some("waiting") {
        Some(Action::Apply)
    } else {
        None
    }
}

/// The script's arguments for one action. `wait_pid`: the caller about to exit (AIQ home, the CLI).
pub fn script_args(script: &std::path::Path, action: Action, wait_pid: Option<u32>, relaunch: bool) -> Vec<String> {
    let mut a: Vec<String> = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.push(script.display().to_string());
    match action {
        Action::Check { apply } => {
            a.push("-Check".into());
            if apply {
                a.push("-Apply".into());
            }
        }
        Action::Apply => a.push("-Apply".into()),
        Action::Rollback => a.push("-Rollback".into()),
    }
    if let Some(pid) = wait_pid {
        a.push("-WaitPid".into());
        a.push(pid.to_string());
    }
    if relaunch {
        a.push("-Relaunch".into());
    }
    a
}

/// Start `upgrade.ps1` for `action` and return at once; it outlives this process (it may stop it).
pub fn start(paths: &Paths, action: Action, wait_pid: Option<u32>, relaunch: bool) -> std::io::Result<()> {
    if !supported(paths) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "updates run only for AIQ installed with install.ps1 on Windows",
        ));
    }
    spawn_hidden(&script_args(&script_path(paths), action, wait_pid, relaunch))
}

#[cfg(windows)]
fn spawn_hidden(args: &[String]) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    // The system's own PowerShell, not whatever `powershell` resolves to on PATH.
    let ps = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    // A hidden console (not DETACHED_PROCESS): the console programs the script starts share it, so no
    // window flashes. Breakaway is refused when the caller's job forbids it; then start without it.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let mk = || {
        let mut c = Command::new(&ps);
        c.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        c
    };
    let base = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
    mk().creation_flags(base | CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
        .or_else(|_| mk().creation_flags(base).spawn())
        .map(|_| ())
}

#[cfg(not(windows))]
fn spawn_hidden(_args: &[String]) -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "updates are Windows only"))
}

/// The core's kicker: 5 minutes after start, then hourly, ask `decide` and start the script when it says so.
/// It never waits on the network itself, and the script's own lock keeps runs from overlapping.
pub fn start_background(paths: Paths) {
    if !supported(&paths) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(KICK_FIRST);
        loop {
            let mode = aiq_store::Store::open_read_only(&paths.db())
                .ok()
                .and_then(|s| s.setting(SETTING_UPDATE).ok().flatten());
            let now = chrono::Utc::now().timestamp_millis();
            if let Some(action) = decide(now, &read_state(&paths), mode.as_deref()) {
                match start(&paths, action, None, false) {
                    Ok(()) => tracing::info!(?action, "update run started"),
                    Err(e) => tracing::warn!(error = %e, "update run did not start"),
                }
            }
            std::thread::sleep(KICK_EVERY);
        }
    });
}

/// What AIQ home and `aiq update` show. Times in UTC ms; strings as the script wrote them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    /// False for a copy not installed by install.ps1 (a dev build): nothing below can happen.
    pub supported: bool,
    pub mode: String,
    pub running: String,
    /// One of: none, waiting, applied, failed, rolled_back.
    pub note: &'static str,
    pub available: Option<String>,
    pub blocked: Option<String>,
    pub from: Option<String>,
    pub at_ms: Option<i64>,
    pub message: Option<String>,
    pub checked_ms: Option<i64>,
    pub check_error: Option<String>,
    /// A previous version is kept (`.old` copies), so "Go back" can work.
    pub can_rollback: bool,
}

pub fn view(paths: &Paths, mode: Option<&str>, now_ms: i64) -> View {
    let st = read_state(paths);
    let at_ms = st.at.as_deref().and_then(parse_utc_ms);
    let recent = at_ms.is_some_and(|t| now_ms - t < APPLIED_NOTE_MS);
    let note = match st.state.as_deref() {
        Some("waiting") => "waiting",
        Some("applied") if recent => "applied",
        Some("failed") if recent => "failed",
        Some("rolled_back") if recent => "rolled_back",
        _ => "none",
    };
    View {
        supported: supported(paths),
        mode: mode.filter(|m| MODES.contains(m)).unwrap_or(DEFAULT_MODE).to_owned(),
        running: env!("CARGO_PKG_VERSION").to_owned(),
        note,
        available: st.available.clone(),
        blocked: st.blocked.clone(),
        from: st.from.clone(),
        at_ms,
        message: st.message.clone(),
        checked_ms: st.checked_at.as_deref().and_then(parse_utc_ms),
        check_error: st.check_error.clone(),
        can_rollback: paths.data.join("bin").join("aiq.exe.old").is_file(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: &str = "2026-11-02T21:00:00Z";
    fn t0() -> i64 {
        parse_utc_ms(T0).unwrap()
    }
    fn st(state: Option<&str>, checked: Option<&str>) -> State {
        State { state: state.map(Into::into), checked_at: checked.map(Into::into), ..State::default() }
    }

    #[test]
    fn never_checked_checks_and_applies_under_auto() {
        assert_eq!(decide(t0(), &State::default(), None), Some(Action::Check { apply: true }));
        assert_eq!(decide(t0(), &State::default(), Some("ask")), Some(Action::Check { apply: false }));
    }

    #[test]
    fn checks_once_a_day_whatever_the_last_outcome() {
        let s = st(Some("current"), Some(T0));
        assert_eq!(decide(t0() + CHECK_EVERY_MS - 1, &s, None), None);
        assert_eq!(decide(t0() + CHECK_EVERY_MS, &s, None), Some(Action::Check { apply: true }));
        let unreadable = st(None, Some("yesterday"));
        assert_eq!(decide(t0(), &unreadable, None), Some(Action::Check { apply: true }));
    }

    #[test]
    fn a_waiting_update_is_retried_hourly_only_under_auto() {
        let s = st(Some("waiting"), Some(T0));
        assert_eq!(decide(t0() + 3_600_000, &s, None), Some(Action::Apply));
        assert_eq!(decide(t0() + 3_600_000, &s, Some("auto")), Some(Action::Apply));
        assert_eq!(decide(t0() + 3_600_000, &s, Some("ask")), None);
    }

    #[test]
    fn script_args_name_the_action_and_the_wait() {
        let p = std::path::Path::new("u.ps1");
        let a = script_args(p, Action::Check { apply: true }, None, false);
        assert_eq!(&a[4..], ["-File", "u.ps1", "-Check", "-Apply"]);
        let a = script_args(p, Action::Apply, Some(42), true);
        assert_eq!(&a[5..], ["u.ps1", "-Apply", "-WaitPid", "42", "-Relaunch"]);
        let a = script_args(p, Action::Rollback, None, false);
        assert_eq!(a.last().unwrap(), "-Rollback");
    }

    #[test]
    fn reads_what_powershell_writes_with_a_bom() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.extend_from_slice(
            br#"{
    "state":  "waiting",
    "available":  "v0.2.0",
    "checked_at":  "2026-11-02T21:00:00Z",
    "blocked":  "AIQ home is open",
    "unknown_key": 3
}"#,
        );
        std::fs::write(state_path(&p), b).unwrap();
        let s = read_state(&p);
        assert_eq!(s.state.as_deref(), Some("waiting"));
        assert_eq!(s.available.as_deref(), Some("v0.2.0"));
        let v = view(&p, None, t0());
        assert_eq!((v.note, v.mode.as_str(), v.checked_ms), ("waiting", "auto", Some(t0())));
        assert_eq!(v.blocked.as_deref(), Some("AIQ home is open"));
        assert!(!v.supported, "a test binary is not an installed copy");
    }

    #[test]
    fn notes_fade_after_a_week_and_a_missing_file_is_none() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        assert_eq!(view(&p, Some("ask"), t0()).note, "none");
        assert_eq!(view(&p, Some("bogus"), t0()).mode, "auto");
        std::fs::write(state_path(&p), format!(r#"{{"state":"applied","from":"0.1.0","at":"{T0}"}}"#)).unwrap();
        assert_eq!(view(&p, None, t0() + 1000).note, "applied");
        assert_eq!(view(&p, None, t0() + APPLIED_NOTE_MS).note, "none");
    }

    #[test]
    fn only_the_shipped_script_is_ever_started() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        assert!(!script_is_shipped(&p), "no script");
        std::fs::create_dir_all(d.path().join("bin")).unwrap();
        std::fs::write(script_path(&p), "# hand-written: swap .new exes, run aiq judge --check\n").unwrap();
        assert!(!script_is_shipped(&p), "a hand-written upgrade.ps1 is never started by the core");
        let shipped = include_str!("../../../packaging/windows/upgrade.ps1");
        std::fs::write(script_path(&p), shipped).unwrap();
        assert!(script_is_shipped(&p), "the repo's upgrade.ps1 carries the marker near its top");
    }

    #[test]
    fn mode_validation() {
        assert_eq!(validate_mode("ask"), Ok("ask".into()));
        assert!(validate_mode("never").is_err());
    }
}
