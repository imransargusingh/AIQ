//! AIQ's Copilot CLI status line (ADR-0005 rule 5; Phase 2 design, Exposed contract 2), as a program of its own.
//!
//! Why a separate program (design auto-update U5n): Copilot CLI runs its status-line command through cmd.exe, and
//! Microsoft Defender's attack surface reduction rule "Use advanced protection against ransomware" (set by IT)
//! refused a freshly installed `aiq.exe` started by cmd.exe and showed the person a "Risky action blocked" pop-up
//! (Imran's laptop 2026-11-08 04:26Z and 04:28Z, 17-19 min after an update; another alpha user 2026-11-07). Every
//! AIQ block in that log was a start by cmd.exe or powershell.exe within about an hour of an update. This program
//! changes only when this crate does, and a release reuses the previous release's file byte for byte while the
//! crate's version is unchanged, so after the first release it is never a fresh file.
//!
//! To stay unchanged it depends on no other AIQ crate: the data folder, pipe name, token and request shape are
//! restated here, and `crates/aiq/tests/statusline_contract.rs` fails if they ever differ from the core's.
//! The core renders the line (`TipLineReply::line` / `line_plain`), so how a tip looks can change without
//! changing this program.
//!
//! Reads Copilot's session JSON on stdin, asks the core for this session's line, prints at most one line, and
//! always exits 0. Never starts the core and never opens the store.
use serde_json::Value;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SURFACE: &str = "copilot-cli";
pub const MAX_STDIN: u64 = 64 * 1024;
/// As the core's `MAX_SESSION_ID`.
pub const MAX_SESSION_ID: usize = 128;
/// As the core's `MAX_FRAME_BYTES`.
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
/// Connect + write, then the core's own 50 ms render wait.
pub const BUDGET: Duration = Duration::from_millis(80);
/// As the hook's timing log cap.
const TIMINGS_LOG_MAX: u64 = 128 * 1024;

/// Where AIQ keeps its files, by the core's rules (`aiq_core::paths::Paths::from_env`).
#[derive(Debug, Clone, PartialEq)]
pub struct Dirs {
    pub data: PathBuf,
    pub runtime: PathBuf,
}

impl Dirs {
    pub fn from_env() -> Self {
        if let Some(h) = std::env::var_os("AIQ_HOME").filter(|v| !v.is_empty()) {
            let p = PathBuf::from(h);
            return Self { data: p.clone(), runtime: p };
        }
        #[cfg(windows)]
        {
            let p = std::env::var_os("USERPROFILE")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(".aiq");
            Self { data: p.clone(), runtime: p }
        }
        #[cfg(not(windows))]
        {
            let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
            let data = std::env::var_os("XDG_DATA_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share"))
                .join("aiq");
            let runtime = std::env::var_os("XDG_RUNTIME_DIR")
                .filter(|v| !v.is_empty())
                .map(|r| PathBuf::from(r).join("aiq"))
                .unwrap_or_else(|| data.clone());
            Self { data, runtime }
        }
    }
    pub fn pause_flag(&self) -> PathBuf {
        self.data.join("paused")
    }
    pub fn token(&self) -> PathBuf {
        self.runtime.join("token")
    }
    pub fn hook_timings(&self) -> PathBuf {
        self.data.join("hook-timings.log")
    }
    pub fn hook_timings_prev(&self) -> PathBuf {
        self.data.join("hook-timings.1.log")
    }
    #[cfg(not(windows))]
    pub fn socket(&self) -> PathBuf {
        self.runtime.join("core.sock")
    }
    /// As the core's `Paths::pipe_name`.
    pub fn pipe_name(&self) -> String {
        let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
        let clean: String = user.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        format!("aiq-core-{clean}-{:016x}", fnv1a(self.runtime.to_string_lossy().to_lowercase().as_bytes()))
    }
}

fn fnv1a(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, x| (h ^ *x as u64).wrapping_mul(0x100000001b3))
}

/// The status line's session id. The key is not documented (probe P2); try the likely shapes.
pub fn session_id(v: &Value) -> Option<String> {
    let direct = ["sessionId", "session_id"].iter().find_map(|k| v.get(*k).and_then(Value::as_str));
    direct
        .or_else(|| v.get("session").and_then(|s| s.get("id")).and_then(Value::as_str))
        .filter(|s| !s.is_empty() && s.len() <= MAX_SESSION_ID)
        .map(str::to_owned)
}

/// The core's `Request::TipLine` frame.
pub fn tip_line_frame(token: &str, session: Option<&str>) -> String {
    serde_json::json!({ "op": "tip_line", "token": token, "surface": SURFACE, "session": session }).to_string()
}

/// `NO_COLOR` set and not empty (no-color.org): plain text.
pub fn color_allowed() -> bool {
    std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

/// The line to print and the outcome for the timing log, from the core's `TipLineReply`. A core that sends no
/// rendered line (older than this program) shows nothing.
pub fn line_from_reply(reply: &Value, color: bool, has_session: bool) -> (Option<String>, &'static str) {
    let pick = if color { "line" } else { "line_plain" };
    let line = reply.get(pick).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
    let is_tip =
        !reply.get("card").is_none_or(Value::is_null) || !reply.get("waiting_card_id").is_none_or(Value::is_null);
    let outcome = match (&line, is_tip, has_session) {
        (None, _, true) => "none",
        (None, _, false) => "none-no-session",
        (Some(_), false, _) => "trend",
        (Some(_), true, true) => "tip",
        (Some(_), true, false) => "tip-no-session",
    };
    (line, outcome)
}

pub fn run() -> ExitCode {
    let start = Instant::now();
    std::panic::set_hook(Box::new(|_| {}));
    let dirs = Dirs::from_env();
    let outcome = std::panic::catch_unwind(|| inner(&dirs)).unwrap_or("panic");
    let line = format!("statusline {} {outcome} sl=v{VERSION}", start.elapsed().as_micros());
    let _ = rotating_append(&dirs.hook_timings(), &dirs.hook_timings_prev(), &line, TIMINGS_LOG_MAX);
    ExitCode::SUCCESS
}

fn inner(dirs: &Dirs) -> &'static str {
    let mut buf = Vec::new();
    let _ = io::stdin().lock().take(MAX_STDIN).read_to_end(&mut buf);
    if dirs.pause_flag().exists() {
        return "paused";
    }
    let session = serde_json::from_slice::<Value>(&buf).ok().as_ref().and_then(session_id);
    let token = std::fs::read_to_string(dirs.token()).map(|s| s.trim().to_owned()).unwrap_or_default();
    let Ok(Some(reply)) = request(dirs, &tip_line_frame(&token, session.as_deref()), BUDGET) else {
        return "no-core";
    };
    let Ok(reply) = serde_json::from_str::<Value>(&reply) else { return "bad-reply" };
    let (line, outcome) = line_from_reply(&reply, color_allowed(), session.is_some());
    if let Some(l) = line {
        let mut out = io::stdout().lock();
        let _ = writeln!(out, "{l}");
        let _ = out.flush();
    }
    outcome
}

/// Send one frame and read one reply line within `timeout`, on a helper thread so a stuck core never holds the
/// status line past its budget.
fn request(dirs: &Dirs, frame: &str, timeout: Duration) -> io::Result<Option<String>> {
    let (tx, rx) = mpsc::channel();
    let dirs = dirs.clone();
    let line = format!("{frame}\n");
    std::thread::spawn(move || {
        let r = (|| -> io::Result<Option<String>> {
            let mut s = connect(&dirs)?;
            s.write_all(line.as_bytes())?;
            s.flush()?;
            let mut br = BufReader::new((&mut s).take(MAX_FRAME_BYTES as u64 + 1));
            let mut buf = Vec::new();
            br.read_until(b'\n', &mut buf)?;
            if buf.is_empty() || buf.len() > MAX_FRAME_BYTES {
                return Ok(None);
            }
            if buf.last() == Some(&b'\n') {
                buf.pop();
            }
            Ok(String::from_utf8(buf).ok())
        })();
        let _ = tx.send(r);
    });
    rx.recv_timeout(timeout).unwrap_or_else(|_| Err(io::Error::new(io::ErrorKind::TimedOut, "core did not answer")))
}

fn connect(dirs: &Dirs) -> io::Result<interprocess::local_socket::Stream> {
    use interprocess::local_socket::{Stream, prelude::*};
    #[cfg(not(windows))]
    {
        use interprocess::local_socket::GenericFilePath;
        let p = dirs.socket();
        Stream::connect(p.as_path().to_fs_name::<GenericFilePath>()?)
    }
    #[cfg(windows)]
    {
        use interprocess::local_socket::GenericNamespaced;
        let s = Stream::connect(dirs.pipe_name().to_ns_name::<GenericNamespaced>()?)?;
        // Security review S7, as the core's `ipc::connect`: send nothing (not the token) unless the pipe's server
        // runs as this user.
        let pid = s.peer_creds()?.pid().ok_or_else(|| io::Error::other("pipe server process unknown"))?;
        win::require_same_user(pid)?;
        Ok(s)
    }
}

/// As the core's `paths::rotating_append`: past `max` bytes the log moves to `prev` and starts again.
fn rotating_append(path: &Path, prev: &Path, line: &str, max: u64) -> io::Result<()> {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > max) {
        let _ = std::fs::rename(path, prev);
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")
}

#[cfg(windows)]
mod win {
    use std::io;
    use std::ptr::null_mut;
    use widestring::U16CStr;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// As the core's `ipc::win::require_same_user`.
    pub fn require_same_user(pid: u32) -> io::Result<()> {
        // SAFETY: the handle is checked and closed on every path.
        let theirs = unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "pipe server not queryable"));
            }
            let sid = process_user_sid(h);
            CloseHandle(h);
            sid?
        };
        // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing.
        if theirs == process_user_sid(unsafe { GetCurrentProcess() })? {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "pipe server runs as another user"))
        }
    }

    fn process_user_sid(process: HANDLE) -> io::Result<String> {
        // SAFETY: standard token query; every handle and buffer is checked and freed on each path.
        unsafe {
            let mut tok: HANDLE = null_mut();
            if OpenProcessToken(process, TOKEN_QUERY, &mut tok) == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut len = 0u32;
            GetTokenInformation(tok, TokenUser, null_mut(), 0, &mut len);
            let mut buf = vec![0u8; len as usize];
            let ok = GetTokenInformation(tok, TokenUser, buf.as_mut_ptr().cast(), len, &mut len);
            CloseHandle(tok);
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            let tu = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s: *mut u16 = null_mut();
            if ConvertSidToStringSidW(tu.User.Sid, &mut s) == 0 {
                return Err(io::Error::last_os_error());
            }
            let out = U16CStr::from_ptr_str(s).to_string_lossy();
            LocalFree(s.cast());
            Ok(out)
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn own_process_passes_and_a_missing_one_fails_closed() {
            super::require_same_user(std::process::id()).unwrap();
            assert!(super::require_same_user(0).is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_id_known_shapes_and_limits() {
        assert_eq!(session_id(&json!({"sessionId":"a"})).as_deref(), Some("a"));
        assert_eq!(session_id(&json!({"session_id":"b"})).as_deref(), Some("b"));
        assert_eq!(session_id(&json!({"session":{"id":"c"}})).as_deref(), Some("c"));
        assert_eq!(session_id(&json!({"session":{"id":""}})), None);
        assert_eq!(session_id(&json!({"sessionId":"x".repeat(200)})), None);
        assert_eq!(session_id(&json!({"model":{"id":"m"}})), None);
    }

    #[test]
    fn picks_the_line_for_the_color_setting_and_names_the_outcome() {
        let tip = json!({"card": {"x": 1}, "waiting_card_id": null, "line": "C", "line_plain": "P"});
        assert_eq!(line_from_reply(&tip, true, true), (Some("C".into()), "tip"));
        assert_eq!(line_from_reply(&tip, false, false), (Some("P".into()), "tip-no-session"));
        let waiting = json!({"card": null, "waiting_card_id": "k1", "line": "W", "line_plain": "w"});
        assert_eq!(line_from_reply(&waiting, true, true), (Some("W".into()), "tip"));
        let trend = json!({"card": null, "waiting_card_id": null, "trend": "t", "line": "t", "line_plain": "t"});
        assert_eq!(line_from_reply(&trend, true, true), (Some("t".into()), "trend"));
        assert_eq!(line_from_reply(&json!({"card": null, "waiting_card_id": null}), true, true), (None, "none"));
        // A core older than this program sends no rendered line: nothing is shown.
        assert_eq!(line_from_reply(&json!({"card": {"x": 1}}), true, false), (None, "none-no-session"));
    }

    #[test]
    fn the_log_rotates_past_its_cap() {
        let d = tempfile::tempdir().unwrap();
        let (p, prev) = (d.path().join("t.log"), d.path().join("t.1.log"));
        for i in 0..30 {
            rotating_append(&p, &prev, &format!("line {i:02}"), 64).unwrap();
        }
        assert!(std::fs::metadata(&p).unwrap().len() <= 64 + 8);
        assert!(prev.exists());
    }
}
