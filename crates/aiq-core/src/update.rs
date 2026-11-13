//! Auto-update (design `docs/design/auto-update.md`). One mechanism: the installed `upgrade.ps1` does the
//! network call, the checksum and the swap; this module decides *when*, starts it, and reads back its outcome
//! (`update.json`) for `aiq update` and AIQ home. The core's kicker asks once an hour; a check runs once a day,
//! hourly after one failed, or every 15 minutes while AIQ home is open (`home_check_due`), also while an update waits
//! (a newer release replaces the waiting one); a waiting update is applied only under "Update automatically", and
//! only when AIQ is idle, after an hourly check for a newer one (the script
//! decides idle, because only it sees the processes at the moment of the swap).
use crate::paths::Paths;
use crate::sysclock::parse_rfc3339_ms as parse_utc_ms;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Store key of the person's choice: `auto` (default) installs by itself; `ask` downloads and tells first;
/// `off` never checks by itself (Imran, 2026-11-02), so AIQ makes no network call of its own; `aiq update --now`
/// still checks when the person asks.
pub const SETTING_UPDATE: &str = "update";
pub const MODES: [&str; 3] = ["auto", "ask", "off"];
pub const DEFAULT_MODE: &str = "auto";
/// A check is due once this long has passed since the last successful one.
pub const CHECK_EVERY_MS: i64 = 24 * 3600 * 1000;
/// The hourly cadence: after a check that failed (no release yet, no network, a proxy), and while AIQ home is
/// open when the last check had to use api.github.com. Five minutes under an hour, so an hourly timer that fires
/// a little early still finds it due.
pub const HOURLY_MS: i64 = 55 * 60 * 1000;
/// While AIQ home is open it checks this often (Imran, 2026-11-05: 0.1.8 went live just after an 18:02Z check
/// and Home showed it at 18:58Z). Home asks every minute; this bar keeps it to one check per 15 minutes. A check
/// is one HEAD of github.com/<repo>/releases/latest, which is not on GitHub's 60-an-hour API limit (measured
/// 2026-11-05: 30 in a row all answered 302 with no rate-limit headers). Only when the last check had to use the
/// API (`source` = "api") does Home fall back to HOURLY_MS, so a shared office address spends at most one API
/// call an hour per person.
pub const HOME_EVERY_MS: i64 = 15 * 60 * 1000;
/// When AIQ home opens it checks if the last check is this old (Imran, 2026-11-04: 0.1.3 was out at 14:33Z and a
/// Home opened after a 14:17Z check would not have seen it until ~15:12Z). The hourly timer and focus keep HOURLY_MS.
pub const HOME_OPEN_MS: i64 = 10 * 60 * 1000;
/// How long Home says "AIQ updated itself" (or that an update failed or was undone) after it happened. A day:
/// Home also hides the note once it has been seen and offers a dismiss (Imran, 2026-11-04: a week-long banner at
/// the top of Home took prime space).
pub const APPLIED_NOTE_MS: i64 = 24 * 3600 * 1000;
const KICK_FIRST: Duration = Duration::from_secs(5 * 60);
const KICK_EVERY: Duration = Duration::from_secs(3600);

/// The person's stored choice; None (the default) when unset or the store can't be read.
pub fn stored_mode(paths: &Paths) -> Option<String> {
    aiq_store::Store::open_read_only(&paths.db()).ok().and_then(|s| s.setting(SETTING_UPDATE).ok().flatten())
}

pub fn validate_mode(value: &str) -> Result<String, String> {
    if MODES.contains(&value) { Ok(value.to_owned()) } else { Err("update must be auto, ask or off".into()) }
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
    /// Why the last install failed, in the error's own words with folders cut (`AIQ-UPDATE-REASON`). Missing from
    /// a script before 0.1.56, which kept the error in upgrade-log.txt only.
    pub reason: Option<String>,
    /// A tag the person rolled back from; never installed again by itself.
    pub skip: Option<String>,
    /// After a check GitHub's API refused for its rate limit (403/429): when the limit resets. The next check
    /// waits for it rather than the hour (2026-11-04: a shared office address had used all 60 calls).
    pub retry_at: Option<String>,
    /// Which source answered the last good check: "redirect" (github.com, no API limit) or "api". Missing (a
    /// script before 0.1.9, or the test feed) reads as the redirect, the source every release so far used.
    pub source: Option<String>,
}

/// Written by upgrade.ps1 (UTC seconds) just before it stops AIQ to swap the exes, removed once the swap is done and
/// before it starts the core again. While it is there, a hook that finds the core down does not start one (devbox,
/// 2026-11-13, DC-071: a busy Copilot's hooks restart a stopped core within seconds, and that core held aiq.exe).
pub fn swapping_path(paths: &Paths) -> PathBuf {
    paths.data.join("update-swapping")
}
/// A marker older than this is a script that ended mid-swap: it never keeps the core down longer.
pub const SWAPPING_MAX_S: u64 = 120;

/// Is an update swapping the exes right now? False for no marker, an unreadable one, or one older than
/// [`SWAPPING_MAX_S`] (degrades to "start the core", the behaviour before the marker existed).
pub fn swapping(paths: &Paths, now_s: u64) -> bool {
    std::fs::read_to_string(swapping_path(paths))
        .ok()
        .and_then(|s| s.trim().trim_start_matches('\u{feff}').parse::<u64>().ok())
        .is_some_and(|t| (now_s >= t && now_s - t < SWAPPING_MAX_S) || (t > now_s && t - now_s < 5))
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
    if mode == Some("off") {
        return None;
    }
    let auto = mode.unwrap_or(DEFAULT_MODE) == "auto";
    let waiting = st.state.as_deref() == Some("waiting");
    if check_due(now_ms, st, CHECK_EVERY_MS) {
        Some(Action::Check { apply: auto })
    } else if auto && waiting && check_due(now_ms, st, HOURLY_MS) {
        // Before installing what waits, ask whether something newer came out since it was downloaded, so AIQ home
        // closing after hours never installs a stale release (2026-11-05, laptop: 0.1.8 waited from 18:58Z and
        // 0.1.9 came out at 20:32Z). The script installs what waits even if this check fails.
        Some(Action::Check { apply: true })
    } else if auto && waiting {
        Some(Action::Apply)
    } else {
        None
    }
}

/// A check is due `every` after the last one, or about an hour after one that failed (a 404 before the first release
/// is published made the laptop wait a full day on 2026-11-03). After a rate-limited one it is due when the limit
/// resets (`retry_at`, at most a day on). Never checked, or an unreadable time: due.
fn check_due(now_ms: i64, st: &State, every: i64) -> bool {
    let Some(t) = st.checked_at.as_deref().and_then(parse_utc_ms) else { return true };
    if st.check_error.is_none() {
        return now_ms - t >= every;
    }
    match retry_at_ms(st) {
        Some(r) => now_ms >= r.min(t + CHECK_EVERY_MS),
        None => now_ms - t >= HOURLY_MS,
    }
}

fn retry_at_ms(st: &State) -> Option<i64> {
    st.check_error.as_ref().and(st.retry_at.as_deref()).and_then(parse_utc_ms)
}

/// How long the kicker sleeps: an hour, or less when a rate limit resets sooner (a minute after the reset, so
/// the check lands after it).
fn kick_sleep(now_ms: i64, st: &State) -> Duration {
    match retry_at_ms(st).map(|r| r + 60_000 - now_ms) {
        Some(ms) if ms > 0 && ms < KICK_EVERY.as_millis() as i64 => Duration::from_millis(ms.max(60_000) as u64),
        _ => KICK_EVERY,
    }
}

/// AIQ home asks this every minute while open, then starts a check (download, no install) when it says so: a
/// person who keeps Home open sees a new release within about 15 minutes and can Install now, instead of waiting
/// for the daily check. Not with updates off. It keeps checking while an update waits (2026-11-05, laptop: Home
/// stopped checking once 0.1.8 waited at 18:58Z, so 0.1.9 at 20:32Z never showed): the script downloads only a
/// newer release than the one waiting and replaces it, and keeps the one waiting when nothing newer is out.
pub fn home_check_due(now_ms: i64, st: &State, mode: Option<&str>) -> bool {
    home_due(now_ms, st, mode, HOME_EVERY_MS)
}

/// As `home_check_due`, for the moment AIQ home opens: due 10 minutes after the last check, not 15.
pub fn home_open_check_due(now_ms: i64, st: &State, mode: Option<&str>) -> bool {
    home_due(now_ms, st, mode, HOME_OPEN_MS)
}

/// After a check that needed api.github.com, Home waits the hour so it never spends the shared API limit faster.
fn home_due(now_ms: i64, st: &State, mode: Option<&str>, every: i64) -> bool {
    let every = if st.source.as_deref() == Some("api") { HOURLY_MS } else { every };
    mode != Some("off") && check_due(now_ms, st, every)
}

/// AIQ home installs a waiting update by itself once its window has been in the background this long (design
/// auto-update U5n). Laptop, 2026-11-08: Home stayed open from 04:09Z, so 0.1.19, 0.1.20 and 0.1.21 were each
/// downloaded and replaced in turn and none installed (`blocked` = "AIQ home is open"); the kicker's install waits
/// for Home to close, which a person who keeps Home open never does. Home measures the time away (no focus, or
/// minimised); this module decides whether the install may start.
pub const AWAY_INSTALL_MS: i64 = 10 * 60 * 1000;
/// After one away install was started, the next waits this long, so a run that found AIQ busy (a hook, a judge)
/// and reopened AIQ home never turns into Home closing and reopening every minute.
pub const AWAY_RETRY_MS: i64 = 30 * 60 * 1000;
/// AIQ home reopens minimised after an away install when it finds this marker younger than this.
pub const REOPEN_MINIMIZED_MS: i64 = 5 * 60 * 1000;

/// When AIQ home last started an away install (UTC ms in a small file). App state, not the person's data.
pub fn away_try_path(paths: &Paths) -> PathBuf {
    paths.data.join("update-away-try")
}

/// Written just before an away install closes AIQ home: the AIQ home that upgrade.ps1 reopens starts minimised,
/// so it never pops up over what the person is doing.
pub fn reopen_minimized_path(paths: &Paths) -> PathBuf {
    paths.data.join("home-reopen-minimized")
}

fn read_ms(path: &std::path::Path) -> Option<i64> {
    std::fs::read_to_string(path).ok().and_then(|s| s.trim().parse().ok())
}

/// May AIQ home, away for `AWAY_INSTALL_MS` (Home measures that), start installing the waiting update now? Only
/// under "Update automatically": "Ask first" keeps the person's own press. `last_try_ms`: the last away install
/// Home started (`away_try_path`).
pub fn away_install_due(now_ms: i64, st: &State, mode: Option<&str>, last_try_ms: Option<i64>) -> bool {
    mode.unwrap_or(DEFAULT_MODE) == "auto"
        && st.state.as_deref() == Some("waiting")
        && st.staged.is_some()
        && last_try_ms.is_none_or(|t| now_ms - t >= AWAY_RETRY_MS || now_ms < t)
}

/// `away_install_due` with the last try read from disk; when due, records this try and the reopen marker. Never
/// errors: a file that cannot be written only means the next try may come sooner, or Home reopens normally.
pub fn begin_away_install(paths: &Paths, now_ms: i64, st: &State, mode: Option<&str>) -> bool {
    if !away_install_due(now_ms, st, mode, read_ms(&away_try_path(paths))) {
        return false;
    }
    let _ = std::fs::write(away_try_path(paths), now_ms.to_string());
    let _ = std::fs::write(reopen_minimized_path(paths), now_ms.to_string());
    true
}

/// What AIQ home's once-a-minute ask starts: never both an install and a check. Laptop, 2026-11-09 02:52:28Z:
/// Home's check and its away install started 18 ms apart, the check took the script's lock, the install logged
/// "Another AIQ update is running. Nothing changed." and the next try waited 30 minutes; 0.1.29 stayed waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeStep {
    /// Install the waiting update (Home has been away `AWAY_INSTALL_MS`); the try is recorded.
    AwayInstall,
    /// Look for a newer release (download only).
    Check,
    Nothing,
}

/// The step for one minute. `away`: Home has been in the background `AWAY_INSTALL_MS` (Home measures that). An
/// away install that is not due (an earlier try in the last `AWAY_RETRY_MS`, or "Ask first") falls back to the
/// check, so Home keeps finding newer releases while it sits in the background.
pub fn home_step(paths: &Paths, now_ms: i64, st: &State, mode: Option<&str>, away: bool) -> HomeStep {
    if away && begin_away_install(paths, now_ms, st, mode) {
        HomeStep::AwayInstall
    } else if home_check_due(now_ms, st, mode) {
        HomeStep::Check
    } else {
        HomeStep::Nothing
    }
}

/// The away install could not start: AIQ home stays open, so the marker must not minimise a later open.
pub fn cancel_reopen_minimized(paths: &Paths) {
    let _ = std::fs::remove_file(reopen_minimized_path(paths));
}

/// At AIQ home's start: true (and the marker removed) when an away install just closed it, so it opens minimised.
/// An old marker (an install that never reopened Home) is removed and ignored: the person opening AIQ home from
/// the Start menu later sees the window.
pub fn take_reopen_minimized(paths: &Paths, now_ms: i64) -> bool {
    let path = reopen_minimized_path(paths);
    let Some(t) = read_ms(&path) else {
        let _ = std::fs::remove_file(&path);
        return false;
    };
    let _ = std::fs::remove_file(&path);
    (0..REOPEN_MINIMIZED_MS).contains(&(now_ms - t))
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
            let mode = stored_mode(&paths);
            let now = chrono::Utc::now().timestamp_millis();
            let st = read_state(&paths);
            if let Some(action) = decide(now, &st, mode.as_deref()) {
                match start(&paths, action, None, false) {
                    Ok(()) => tracing::info!(?action, "update run started"),
                    Err(e) => tracing::warn!(error = %e, "update run did not start"),
                }
            }
            // The state read before a check just started: a rate limit it hits is seen on the next wake.
            let sleep = kick_sleep(now, &st);
            tracing::debug!(sleep_s = sleep.as_secs(), "update kicker sleeping");
            std::thread::sleep(sleep);
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
    /// Why the last install failed (`State::reason`).
    pub reason: Option<String>,
    pub checked_ms: Option<i64>,
    pub check_error: Option<String>,
    /// The newest release the last good check saw (it may be one the person went back from, never installed).
    pub latest: Option<String>,
    /// A previous version is kept (`.old` copies), so "Go back" can work.
    pub can_rollback: bool,
}

/// Same release, whether or not either side carries the tag's `v`.
fn same_version(a: &str, b: &str) -> bool {
    a.trim().trim_start_matches('v') == b.trim().trim_start_matches('v')
}

/// `1.2.3` (a leading `v` allowed) as numbers; None for anything else (a pre-release is never "not newer").
fn numbers(v: &str) -> Option<Vec<u64>> {
    v.trim().trim_start_matches('v').split('.').map(|p| p.parse().ok()).collect()
}

/// A waiting release that is this version or older: nothing to install (2026-11-04, laptop: Home offered
/// "AIQ 0.1.2 is ready, Install now" while it ran 0.1.2, and Install now swapped 0.1.2 over itself).
fn not_newer(available: &str, running: &str) -> bool {
    matches!((numbers(available), numbers(running)), (Some(a), Some(r)) if a <= r)
}

/// The start of the line about a failed update: the script's message without the ", so AIQ kept <version>" a script
/// before 0.1.56 added (the line says which version was kept itself), or "an update didn't install" when there is
/// none. AIQ home's line (`apps/home/src/ui.tsx` `failedLead`) does the same.
pub fn failed_lead(message: Option<&str>) -> String {
    let m = message.map(str::trim).filter(|m| !m.is_empty()).unwrap_or("an update didn't install");
    m.split_once(", so AIQ kept ").map_or(m, |(lead, _)| lead).trim_end_matches('.').to_owned()
}

pub fn view(paths: &Paths, mode: Option<&str>, now_ms: i64) -> View {
    let st = read_state(paths);
    let running = env!("CARGO_PKG_VERSION");
    let at_ms = st.at.as_deref().and_then(parse_utc_ms);
    let recent = at_ms.is_some_and(|t| now_ms - t < APPLIED_NOTE_MS);
    // An "update" from this version to itself changed nothing (2026-11-04, laptop: a stale waiting 0.1.2 was
    // applied over the hand-installed 0.1.2). Say nothing, and offer no "Go back": the `.old` copies are this
    // same version, so the real previous version is gone.
    let applied_to_itself =
        st.state.as_deref() == Some("applied") && st.from.as_deref().is_some_and(|f| same_version(f, running));
    let note = match st.state.as_deref() {
        Some("waiting") if !st.available.as_deref().is_some_and(|a| not_newer(a, running)) => "waiting",
        Some("applied") if recent && st.from.is_some() && !applied_to_itself => "applied",
        Some("failed") if recent => "failed",
        Some("rolled_back") if recent => "rolled_back",
        _ => "none",
    };
    View {
        supported: supported(paths),
        mode: mode.filter(|m| MODES.contains(m)).unwrap_or(DEFAULT_MODE).to_owned(),
        running: running.to_owned(),
        note,
        available: st.available.clone(),
        blocked: st.blocked.clone(),
        from: st.from.clone(),
        at_ms,
        message: st.message.clone(),
        reason: st.reason.clone(),
        checked_ms: st.checked_at.as_deref().and_then(parse_utc_ms),
        check_error: st.check_error.clone(),
        latest: st.latest.clone(),
        can_rollback: !applied_to_itself && paths.data.join("bin").join("aiq.exe.old").is_file(),
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
    fn a_good_check_is_repeated_once_a_day() {
        let s = st(Some("current"), Some(T0));
        assert_eq!(decide(t0() + CHECK_EVERY_MS - 1, &s, None), None);
        assert_eq!(decide(t0() + CHECK_EVERY_MS, &s, None), Some(Action::Check { apply: true }));
        let unreadable = st(None, Some("yesterday"));
        assert_eq!(decide(t0(), &unreadable, None), Some(Action::Check { apply: true }));
    }

    #[test]
    fn a_failed_check_is_retried_within_the_hour_not_the_next_day() {
        let failed = State { check_error: Some("404".into()), ..st(None, Some(T0)) };
        assert_eq!(decide(t0() + HOURLY_MS - 1, &failed, None), None);
        assert_eq!(decide(t0() + HOURLY_MS, &failed, None), Some(Action::Check { apply: true }));
        assert_eq!(decide(t0() + HOURLY_MS, &failed, Some("ask")), Some(Action::Check { apply: false }));
        assert_eq!(decide(t0() + HOURLY_MS, &failed, Some("off")), None);
    }

    #[test]
    fn a_rate_limited_check_waits_for_the_reset_not_the_hour() {
        // 2026-11-04 on the laptop: 403 with X-RateLimit-Remaining 0, reset 58 minutes later.
        let reset = t0() + 58 * 60_000;
        let limited = State {
            check_error: Some("(403) Forbidden".into()),
            retry_at: Some("2026-11-02T21:58:00Z".into()),
            ..st(None, Some(T0))
        };
        assert_eq!(decide(t0() + HOURLY_MS, &limited, None), None, "the hour is not enough");
        assert_eq!(decide(reset, &limited, None), Some(Action::Check { apply: true }));
        assert!(!home_check_due(reset - 1, &limited, None));
        assert!(home_check_due(reset, &limited, None));
        // A sooner reset is also honoured.
        let soon = State { retry_at: Some("2026-11-02T21:10:00Z".into()), ..limited.clone() };
        assert_eq!(decide(t0() + 10 * 60_000, &soon, None), Some(Action::Check { apply: true }));
        // A reset far off (a bad header) never stops checks past a day.
        let far = State { retry_at: Some("2026-12-02T21:00:00Z".into()), ..limited.clone() };
        assert_eq!(decide(t0() + CHECK_EVERY_MS, &far, None), Some(Action::Check { apply: true }));
        // retry_at left over without an error, or unreadable, changes nothing.
        let stale = State { check_error: None, ..limited.clone() };
        assert_eq!(decide(t0() + HOURLY_MS, &stale, None), None);
        let junk = State { retry_at: Some("soon".into()), ..limited };
        assert_eq!(decide(t0() + HOURLY_MS, &junk, None), Some(Action::Check { apply: true }));
    }

    #[test]
    fn the_kicker_wakes_a_minute_after_a_reset_that_comes_before_the_hour() {
        let limited = |at: &str| State {
            check_error: Some("(403) Forbidden".into()),
            retry_at: Some(at.into()),
            ..st(None, Some(T0))
        };
        assert_eq!(kick_sleep(t0(), &limited("2026-11-02T21:20:00Z")), Duration::from_secs(21 * 60));
        assert_eq!(
            kick_sleep(t0(), &limited("2026-11-02T20:59:30Z")),
            Duration::from_secs(60),
            "just passed: at least a minute"
        );
        assert_eq!(kick_sleep(t0(), &limited("2026-11-02T20:58:00Z")), KICK_EVERY, "long passed");
        assert_eq!(kick_sleep(t0() - 30_000, &limited("2026-11-02T21:00:00Z")), Duration::from_secs(90));
        assert_eq!(kick_sleep(t0(), &limited("2026-11-02T23:00:00Z")), KICK_EVERY);
        assert_eq!(kick_sleep(t0(), &st(None, Some(T0))), KICK_EVERY);
    }

    #[test]
    fn home_checks_every_15_minutes_while_open_but_never_with_updates_off() {
        assert!(home_check_due(t0(), &State::default(), None), "never checked");
        let ok = st(Some("current"), Some(T0));
        assert!(!home_check_due(t0() + HOME_EVERY_MS - 1, &ok, None));
        assert!(home_check_due(t0() + HOME_EVERY_MS, &ok, None));
        assert!(home_check_due(t0() + HOME_EVERY_MS, &ok, Some("ask")));
        assert!(!home_check_due(t0() + HOURLY_MS, &ok, Some("off")));
        let redirect = State { source: Some("redirect".into()), ..ok.clone() };
        assert!(home_check_due(t0() + HOME_EVERY_MS, &redirect, None));
        // On open: 10 minutes, not 55; still never with updates off.
        assert!(!home_open_check_due(t0() + HOME_OPEN_MS - 1, &ok, None));
        assert!(home_open_check_due(t0() + HOME_OPEN_MS, &ok, None));
        assert!(!home_open_check_due(t0() + HOURLY_MS, &ok, Some("off")));
    }

    #[test]
    fn a_waiting_update_does_not_stop_home_checking_so_a_newer_release_replaces_it() {
        // Laptop 2026-11-05: 0.1.8 waiting since 18:58Z with Home open; 0.1.9 out at 20:32Z was never seen.
        let waiting = State { available: Some("v0.1.8".into()), ..st(Some("waiting"), Some(T0)) };
        assert!(!home_check_due(t0() + HOME_EVERY_MS - 1, &waiting, None));
        assert!(home_check_due(t0() + HOME_EVERY_MS, &waiting, None));
        assert!(home_check_due(t0() + HOME_EVERY_MS, &waiting, Some("ask")));
        assert!(home_open_check_due(t0() + HOME_OPEN_MS, &waiting, None));
        assert!(!home_check_due(t0() + CHECK_EVERY_MS, &waiting, Some("off")));
    }

    #[test]
    fn home_keeps_the_hour_after_an_api_check_or_a_failed_one_so_a_shared_address_keeps_its_api_limit() {
        let api = State { source: Some("api".into()), ..st(Some("current"), Some(T0)) };
        assert!(!home_check_due(t0() + HOME_EVERY_MS, &api, None));
        assert!(!home_open_check_due(t0() + HOME_OPEN_MS, &api, None));
        assert!(home_check_due(t0() + HOURLY_MS, &api, None));
        let failed = State { check_error: Some("404".into()), ..st(Some("current"), Some(T0)) };
        assert!(!home_check_due(t0() + HOME_EVERY_MS, &failed, None));
        assert!(!home_open_check_due(t0() + HOME_OPEN_MS, &failed, None));
        assert!(home_check_due(t0() + HOURLY_MS, &failed, None));
        // Home's one-minute ask: at most 4 checks an hour after redirect checks, 1 after API checks.
        let checks = |mut s: State| {
            let mut n = 0;
            for m in 1..=60 {
                let now = t0() + m * 60_000;
                if home_check_due(now, &s, None) {
                    n += 1;
                    s.checked_at = Some(crate::sysclock::format_rfc3339_ms(now));
                }
            }
            n
        };
        assert_eq!(checks(st(Some("current"), Some(T0))), 4);
        assert_eq!(checks(api), 1);
    }

    #[test]
    fn a_waiting_update_is_retried_hourly_only_under_auto() {
        let s = st(Some("waiting"), Some(T0));
        assert_eq!(decide(t0() + HOURLY_MS - 1, &s, None), Some(Action::Apply));
        assert_eq!(decide(t0() + HOURLY_MS - 1, &s, Some("auto")), Some(Action::Apply));
        assert_eq!(decide(t0() + 3_600_000, &s, Some("ask")), None);
    }

    #[test]
    fn the_kicker_checks_for_a_newer_release_before_installing_a_waiting_one_from_over_an_hour_ago() {
        // Home closed hours after 0.1.8 was downloaded: ask first, so 0.1.9 is what installs. One run does both
        // (-Check -Apply); a check that fails still installs what waits.
        let s = st(Some("waiting"), Some(T0));
        assert_eq!(decide(t0() + HOURLY_MS, &s, None), Some(Action::Check { apply: true }));
        assert_eq!(decide(t0() + 3 * 3_600_000, &s, Some("auto")), Some(Action::Check { apply: true }));
        assert_eq!(decide(t0() + 3 * 3_600_000, &s, Some("ask")), None);
        // A check Home ran a few minutes ago is fresh enough: install straight away.
        assert_eq!(decide(t0() + HOME_EVERY_MS, &s, None), Some(Action::Apply));
    }

    #[test]
    fn home_installs_a_waiting_update_when_away_only_under_auto_and_not_every_minute() {
        let mut w = st(Some("waiting"), Some(T0));
        w.staged = Some("aiq-0.1.21-windows-x64.zip".into());
        let now = t0() + HOURLY_MS;
        assert!(away_install_due(now, &w, None, None), "default is auto");
        assert!(away_install_due(now, &w, Some("auto"), None));
        assert!(!away_install_due(now, &w, Some("ask"), None), "ask first keeps the person's own press");
        assert!(!away_install_due(now, &w, Some("off"), None));
        assert!(!away_install_due(now, &st(Some("current"), Some(T0)), None, None), "nothing waits");
        assert!(!away_install_due(now, &st(Some("waiting"), Some(T0)), None, None), "no staged zip");
        // One try, then the next only after AWAY_RETRY_MS (a busy AIQ reopens Home; it must not close again at once).
        assert!(!away_install_due(now, &w, None, Some(now - AWAY_RETRY_MS + 1)));
        assert!(away_install_due(now, &w, None, Some(now - AWAY_RETRY_MS)));
        // A last try in the future (the clock moved back) does not block for ever.
        assert!(away_install_due(now, &w, None, Some(now + 1)));
    }

    #[test]
    fn an_away_install_records_its_try_and_the_next_open_is_minimised_once() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path());
        let mut w = st(Some("waiting"), Some(T0));
        w.staged = Some("aiq.zip".into());
        let now = t0();
        assert!(begin_away_install(&paths, now, &w, None));
        assert!(!begin_away_install(&paths, now + 60_000, &w, None), "the recorded try holds the next one back");
        assert!(take_reopen_minimized(&paths, now + 20_000), "the reopened Home starts minimised");
        assert!(!take_reopen_minimized(&paths, now + 21_000), "only once");
        // An install that never reopened Home leaves an old marker: a later open shows the window.
        assert!(begin_away_install(&paths, now + AWAY_RETRY_MS, &w, None));
        assert!(!take_reopen_minimized(&paths, now + AWAY_RETRY_MS + REOPEN_MINIMIZED_MS));
        assert!(!reopen_minimized_path(&paths).exists());
        // A start that failed takes the marker back.
        assert!(begin_away_install(&paths, now + 2 * AWAY_RETRY_MS, &w, None));
        cancel_reopen_minimized(&paths);
        assert!(!take_reopen_minimized(&paths, now + 2 * AWAY_RETRY_MS + 1));
    }

    #[test]
    fn home_minute_starts_an_away_install_or_a_check_never_both() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path());
        // A waiting update and a check due on the same minute (laptop 2026-11-09 02:52:28Z): only the install.
        let mut w = st(Some("waiting"), Some(T0));
        w.staged = Some("aiq-0.1.29-windows-x64.zip".into());
        let now = t0() + HOME_EVERY_MS;
        assert!(home_check_due(now, &w, None), "the check is due too");
        assert_eq!(home_step(&paths, now, &w, None, true), HomeStep::AwayInstall);
        // The next minutes: the try holds the install back, so Home checks instead, and stays quiet once checked.
        assert_eq!(home_step(&paths, now + 60_000, &w, None, true), HomeStep::Check);
        let mut checked = w.clone();
        checked.checked_at = Some(crate::sysclock::format_rfc3339_ms(now + 60_000));
        assert_eq!(home_step(&paths, now + 120_000, &checked, None, true), HomeStep::Nothing);
        // In the window (not away): only the check, and no try is recorded.
        let dir2 = tempfile::tempdir().unwrap();
        let paths2 = Paths::at(dir2.path());
        assert_eq!(home_step(&paths2, now, &w, None, false), HomeStep::Check);
        assert!(!away_try_path(&paths2).exists());
        // "Ask first": away never installs; the check still runs.
        assert_eq!(home_step(&paths2, now, &w, Some("ask"), true), HomeStep::Check);
        assert!(!away_try_path(&paths2).exists());
        // Updates off: nothing.
        assert_eq!(home_step(&paths2, now, &w, Some("off"), true), HomeStep::Nothing);
    }

    #[test]
    fn off_never_checks_or_applies_by_itself() {
        assert_eq!(decide(t0(), &State::default(), Some("off")), None);
        assert_eq!(decide(t0() + 3_600_000, &st(Some("waiting"), Some(T0)), Some("off")), None);
        assert_eq!(view(&Paths::at(&std::env::temp_dir().join("aiq-off-view")), Some("off"), t0()).mode, "off");
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
        assert_eq!(APPLIED_NOTE_MS, 24 * 3600 * 1000, "the note lasts a day, not a week");
    }

    #[test]
    fn an_update_to_the_same_version_shows_nothing_and_offers_no_go_back() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        std::fs::create_dir_all(d.path().join("bin")).unwrap();
        std::fs::write(d.path().join("bin").join("aiq.exe.old"), b"x").unwrap();
        let me = env!("CARGO_PKG_VERSION");
        for from in [me.to_owned(), format!("v{me}")] {
            std::fs::write(state_path(&p), format!(r#"{{"state":"applied","from":"{from}","at":"{T0}"}}"#)).unwrap();
            let v = view(&p, None, t0() + 1000);
            assert_eq!(v.note, "none", "from {from}");
            assert!(!v.can_rollback, "the .old copy is this same version");
        }
        std::fs::write(state_path(&p), format!(r#"{{"state":"applied","from":"0.0.1","at":"{T0}"}}"#)).unwrap();
        let v = view(&p, None, t0() + 1000);
        assert_eq!((v.note, v.from.as_deref(), v.can_rollback), ("applied", Some("0.0.1"), true));
        std::fs::write(state_path(&p), format!(r#"{{"state":"applied","at":"{T0}"}}"#)).unwrap();
        assert_eq!(view(&p, None, t0() + 1000).note, "none", "no previous version known: nothing to name");
    }

    #[test]
    fn a_swap_marker_holds_core_starts_for_two_minutes_at_most() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        let now = 1_794_528_000;
        assert!(!swapping(&p, now), "no marker");
        // Windows PowerShell's Set-Content -Encoding UTF8 writes a byte-order mark and a line end.
        std::fs::write(swapping_path(&p), format!("\u{feff}{}\r\n", now - 3)).unwrap();
        assert!(swapping(&p, now));
        assert!(swapping(&p, now - 4), "a clock a moment behind still sees it");
        assert!(!swapping(&p, now - 3 + SWAPPING_MAX_S), "a script that ended mid-swap holds starts 2 minutes at most");
        std::fs::write(swapping_path(&p), "garbage").unwrap();
        assert!(!swapping(&p, now), "unreadable: start the core as before");
    }

    #[test]
    fn a_failed_install_carries_its_reason_and_an_old_message_loses_the_kept_version() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        std::fs::write(
            state_path(&p),
            format!(
                r#"{{"state":"failed","at":"{T0}","message":"installing v0.1.55 failed","reason":"aiq-home.exe is missing from the zip"}}"#
            ),
        )
        .unwrap();
        let v = view(&p, None, t0() + 1000);
        assert_eq!(v.note, "failed");
        assert_eq!(v.reason.as_deref(), Some("aiq-home.exe is missing from the zip"));
        assert_eq!(failed_lead(v.message.as_deref()), "installing v0.1.55 failed");
        // The devbox's update.json on 2026-11-13, written by 0.1.54's script: no reason, the kept version in the message.
        assert_eq!(failed_lead(Some("installing v0.1.55 failed, so AIQ kept 0.1.54")), "installing v0.1.55 failed");
        assert_eq!(
            failed_lead(Some("the download of v0.1.55 did not match its checksum")),
            "the download of v0.1.55 did not match its checksum"
        );
        assert_eq!(failed_lead(None), "an update didn't install");
        assert_eq!(failed_lead(Some("  ")), "an update didn't install");
    }

    #[test]
    fn a_waiting_release_that_is_not_newer_is_not_offered() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::at(d.path());
        let me = env!("CARGO_PKG_VERSION");
        for (tag, want) in [
            (format!("v{me}"), "none"),
            ("v0.0.1".into(), "none"),
            ("v99.0.0".into(), "waiting"),
            ("v99.0.0-rc1".into(), "waiting"),
        ] {
            std::fs::write(state_path(&p), format!(r#"{{"state":"waiting","available":"{tag}"}}"#)).unwrap();
            assert_eq!(view(&p, None, t0()).note, want, "{tag}");
        }
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
        assert_eq!(validate_mode("off"), Ok("off".into()));
        assert!(validate_mode("never").is_err());
    }
}
