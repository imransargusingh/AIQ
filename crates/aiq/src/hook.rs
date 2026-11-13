//! `aiq hook claude-code <event>` and `aiq hook copilot <event>`: the hot path. Guarantees (Phase 1
//! design: Exposed contract 1; Phase 2 design: Exposed contract 1): always exits 0; never opens the store
//! or loads the pack. Claude Code: stdout is empty, or exactly one `{"systemMessage": …}` (a tip or a milestone
//! note) on Stop; never
//! `additionalContext`, `decision` or `continue`. Copilot: stderr is always empty; stdout is empty, or on
//! a Copilot CLI `agentStop` exactly one display-only `{"type":"progress",…}` line with the tip (setting
//! `copilot_tip_in_chat`, default on); never `decision`, `additionalContext` or exit code 2.
use aiq_contracts::card::{claude_code_note_message, claude_code_system_message, copilot_progress_message_with_tool};
use aiq_contracts::codes;
use aiq_contracts::event::{EventBody, EventV1, MAX_PROMPT_BYTES, PromptPayload, SCHEMA, StopPayload, cap_text};
use aiq_contracts::frames::{Request, SpoolLine, TakeCardReply};
use aiq_core::ipc;
use aiq_core::paths::{Paths, bounded_append, rotating_append};
use aiq_core::spool;
use serde_json::Value;
use std::io::{Read, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_STDIN: u64 = 256 * 1024;
/// Set by AIQ on processes it starts itself (the Phase 5 judge's `copilot -p` / `claude -p`), whose hooks
/// would otherwise feed AIQ its own calls (surface spike: `copilot -p` fires the person's hooks).
pub const INTERNAL_ENV: &str = "AIQ_INTERNAL";
/// Claude Code's start mode, inherited by its hooks: `cli`, `claude-desktop`, or `sdk-*` for a headless
/// `claude -p` / Agent SDK run. Import spec rule NI: `sdk-cli` sessions are automated (586 of 586 on
/// Imran's laptop), so the live hook records nothing for them (US-12).
/// assume: hooks inherit the variable like every child of Claude Code (seen in this session's tool
/// processes, whose transcript `entrypoint` matches it). Confirm: a `claude -p` run with AIQ installed adds
/// no events. If false: headless runs are still caught by the core's burst/template rule.
pub const CLAUDE_ENTRYPOINT_ENV: &str = "CLAUDE_CODE_ENTRYPOINT";
/// Longest working-folder path sent to the core.
const MAX_CWD_BYTES: usize = 1024;

/// Rule NI for Claude Code: an `sdk-*` entrypoint is a program, not a person.
pub fn claude_headless(entrypoint: Option<&str>) -> bool {
    entrypoint.is_some_and(|e| e.starts_with("sdk"))
}
/// Bytes past the cap that are read and thrown away (bounded, so a runaway writer cannot hold the hook).
pub const MAX_DRAIN: u64 = 16 * 1024 * 1024;
/// Connect + write budget (ADR-0005).
pub const SEND_BUDGET: Duration = Duration::from_millis(20);
/// Stop: what the core may spend finding a card, and the client's reply wait on top of the send.
pub const STOP_WAIT_MS: u64 = 30;
pub const STOP_REPLY_BUDGET: Duration = Duration::from_millis(35);
const CORE_START_BACKOFF_S: u64 = 10;
const SMALL_LOG_MAX: u64 = 32 * 1024;
/// One fill of the timing log, about 1,000 runs at ~120 bytes a line. When full it moves to
/// `hook-timings.1.log`, so `aiq status` reads one to two fills. The status-line command writes here too, with
/// this same cap: it used its own 32 KiB cap and emptied the log every time it passed 32 KiB (measured on
/// Imran's laptop 2026-11-04: 15.5 KB, 323 lines).
const TIMINGS_LOG_MAX: u64 = 128 * 1024;

pub fn run(host: &str, event: &str) -> ExitCode {
    let start = Instant::now();
    std::panic::set_hook(Box::new(|_| {})); // a panic must never print into Claude Code
    let paths = Paths::from_env();
    let mut marks = Marks::default();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        inner_outcome(&paths, host, event, start, &mut marks)
    }))
    .unwrap_or("panic");
    if host == "copilot" {
        // After the event is sent, so capture never waits on it. Copilot CLI only (VS Code's agent host sets no
        // loader pid): which Copilot runs AIQ's hooks, so analysis calls the same one (DC-056).
        let _ = std::panic::catch_unwind(|| record_copilot_seen(&paths, |k| std::env::var(k).ok()));
    }
    let total = start.elapsed().as_micros();
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    append_timing(&paths, &timing_line(event, total, outcome, &marks, at));
    ExitCode::SUCCESS
}

/// Keep what this hook sees of the Copilot CLI that runs it (`aiq_core::copilot_seen`). Nothing when the loader
/// pid is absent (VS Code agent mode, or another host).
fn record_copilot_seen(paths: &Paths, env: impl Fn(&str) -> Option<String>) {
    let Some(pid) = env("COPILOT_LOADER_PID").and_then(|p| p.trim().parse::<u32>().ok()) else { return };
    let at_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    let seen = aiq_core::copilot_seen::Seen {
        exe: process_exe(pid).map(|p| p.display().to_string()),
        hook_dir: env("COPILOT_HOOK_DIR").filter(|v| !v.is_empty()),
        home_env: env("COPILOT_HOME").filter(|v| !v.is_empty()),
        at_ms,
        known: Vec::new(), // `record` carries the earlier ones over
    };
    let _ = aiq_core::copilot_seen::record(paths, &seen);
}

/// The full path of a running process's program. None when it can't be read.
#[cfg(windows)]
fn process_exe(pid: u32) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    // SAFETY: a handle opened here is closed before return; the buffer outlives the call and its length is passed.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        (ok != 0).then(|| std::ffi::OsString::from_wide(&buf[..len as usize]).into())
    }
}

#[cfg(not(windows))]
fn process_exe(pid: u32) -> Option<std::path::PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

/// The only writer of the timing log (the hooks and the status-line command), so no caller can pass its own cap.
pub fn append_timing(paths: &Paths, line: &str) {
    let _ = rotating_append(&paths.hook_timings(), &paths.hook_timings_prev(), line, TIMINGS_LOG_MAX);
}

/// Where one hook run spent its time, so a slow run can be tied to a phase, a build and a time of day.
#[derive(Default)]
pub struct Marks {
    /// Copilot's own start-up (its event timestamp to this process running): shown in `aiq status`, not
    /// counted against AIQ's budget (Imran, 2026-11-01).
    pub host_ms: Option<i64>,
    /// Reading the host's payload from stdin.
    pub read_us: Option<u128>,
    /// Payload read to frame ready: parse, secret removal, the transcript check.
    pub prep_us: Option<u128>,
    /// The working folder's tag (`automated::folder_tag`), so `aiq status` can leave out runs from folders
    /// AIQ sets aside as automated or as its own checks. Never the path itself.
    pub dir: Option<String>,
    /// The session's tag (`automated::session_tag`), so a person's session in a set-aside folder counts like
    /// it does for tasks (two or more prompts). Never the id itself.
    pub session: Option<String>,
}

/// `<event> <total us> <outcome>` stays first, so older readers keep working; then `key=value` fields.
/// The send phase is derived (total - read - prep), never stored twice.
pub fn timing_line(event: &str, total_us: u128, outcome: &str, m: &Marks, at_unix_s: u64) -> String {
    let mut l = format!("{event} {total_us} {outcome}");
    if let Some(ms) = m.host_ms {
        l += &format!(" host_ms={ms}");
    }
    if let Some(us) = m.read_us {
        l += &format!(" read_us={us}");
    }
    if let Some(us) = m.prep_us {
        l += &format!(" prep_us={us}");
    }
    if let Some(d) = &m.dir {
        l += &format!(" dir={d}");
    }
    if let Some(t) = &m.session {
        l += &format!(" s={t}");
    }
    l += &format!(" at={at_unix_s} v={}", env!("CARGO_PKG_VERSION"));
    l
}

fn count(paths: &Paths, code: &str) {
    let _ = bounded_append(&paths.hook_errors(), code, SMALL_LOG_MAX);
}

/// Milliseconds from the host's event `timestamp` (Copilot: epoch ms) to now; None if absent or implausible.
pub fn host_start_ms(v: &Value, now_ms: i64) -> Option<i64> {
    let ts = v.get("timestamp")?.as_i64()?;
    Some(now_ms - ts).filter(|d| (0..60_000).contains(d))
}

/// Returns a short outcome label for the timing log; fills `marks` as phases finish.
fn inner_outcome(paths: &Paths, host: &str, event: &str, start: Instant, marks: &mut Marks) -> &'static str {
    let mut buf = Vec::new();
    let mut stdin = std::io::stdin().lock();
    let _ = (&mut stdin).take(MAX_STDIN).read_to_end(&mut buf);
    // Discard the rest instead of closing early, so the host never sees a broken pipe while writing.
    let _ = std::io::copy(&mut stdin.take(MAX_DRAIN), &mut std::io::sink());
    let read_done = start.elapsed().as_micros();
    marks.read_us = Some(read_done);
    if std::env::var_os(INTERNAL_ENV).is_some_and(|v| !v.is_empty()) {
        return "internal";
    }
    if host != "claude-code" && host != "copilot" {
        return "unknown-host";
    }
    if host == "claude-code" && claude_headless(std::env::var(CLAUDE_ENTRYPOINT_ENV).ok().as_deref()) {
        return "automated";
    }
    let v: Value = match serde_json::from_slice(&buf) {
        Ok(v @ Value::Object(_)) => v,
        _ => {
            count(paths, codes::HOOK_PARSE);
            return "parse";
        }
    };
    if host == "copilot" {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
        marks.host_ms = host_start_ms(&v, now);
    }
    let parsed = if host == "copilot" {
        to_copilot_event(&v, event, copilot_surface(|k| std::env::var_os(k).is_some()))
    } else {
        to_event(&v, event)
    };
    let Some(ev) = parsed else {
        count(paths, codes::HOOK_FOREIGN_HOST);
        return "foreign";
    };
    marks.dir = ev.cwd.as_deref().and_then(aiq_domain::automated::folder_tag);
    marks.session = aiq_domain::automated::session_tag(&ev.surface_session_id);
    if paths.pause_flag().exists() {
        return "paused"; // nothing captured while paused
    }
    let trace_id = uuid::Uuid::new_v4().simple().to_string();
    marks.prep_us = Some(start.elapsed().as_micros() - read_done);
    let token = ipc::read_token(paths).unwrap_or_default();
    match &ev.body {
        // Copilot: prompts are ingests, and so is VS Code's agentStop (the extension renders there). Copilot
        // CLI's agentStop takes the card like Claude Code's Stop and prints it into the conversation.
        EventBody::Prompt(_) | EventBody::Stop(_) if host == "copilot" && !copilot_cli_stop(&ev) => {
            let frame = Request::Ingest { token, trace_id: Some(trace_id.clone()), event: ev.clone() };
            match ipc::request(paths, &serde_json::to_string(&frame).unwrap_or_default(), false, SEND_BUDGET) {
                Ok(_) => "sent",
                Err(_) => {
                    spool_frame(paths, &trace_id, &ev);
                    if matches!(ev.body, EventBody::Prompt(_)) {
                        maybe_start_core(paths);
                    }
                    "spooled"
                }
            }
        }
        EventBody::Prompt(_) => {
            let frame = Request::Ingest { token, trace_id: Some(trace_id.clone()), event: ev.clone() };
            match ipc::request(paths, &serde_json::to_string(&frame).unwrap_or_default(), false, SEND_BUDGET) {
                Ok(_) => "sent",
                Err(_) => {
                    spool_frame(paths, &trace_id, &ev);
                    maybe_start_core(paths);
                    "spooled"
                }
            }
        }
        EventBody::Stop(_) => {
            let frame = Request::TakeCard {
                token: token.clone(),
                trace_id: Some(trace_id.clone()),
                event: ev.clone(),
                wait_ms: STOP_WAIT_MS,
            };
            let (reply, written) = ipc::request_tracked(
                paths,
                &serde_json::to_string(&frame).unwrap_or_default(),
                true,
                SEND_BUDGET + STOP_REPLY_BUDGET,
            );
            match reply {
                Ok(Some(line)) => match serde_json::from_str::<TakeCardReply>(&line) {
                    Ok(TakeCardReply { card: Some(card), plain, .. }) => {
                        let color = color_allowed() && !plain;
                        let text = if host == "copilot" {
                            // The team's "Tool to try" line: Copilot surfaces only, never stored (team tips file).
                            let tool = aiq_core::team_tips::line_on(paths, &ev.surface, &card.tip_id);
                            copilot_progress_message_with_tool(&card, color, tool.as_deref())
                        } else {
                            claude_code_system_message(&card, color)
                        };
                        let mut out = std::io::stdout().lock();
                        if writeln!(out, "{text}").and_then(|_| out.flush()).is_err() {
                            return "card-unprinted"; // no confirmation: the tip stays ready for the next turn end
                        }
                        drop(out);
                        // The tip counts as shown only once the core hears it was printed (DC-022).
                        let shown = Request::Shown {
                            token,
                            card_id: card.card_id.clone(),
                            surface: ev.surface.clone(),
                            session: ev.surface_session_id.clone(),
                        };
                        let frame = serde_json::to_string(&shown).unwrap_or_default();
                        match ipc::request(paths, &frame, false, SEND_BUDGET) {
                            Ok(_) => "card",
                            Err(_) => {
                                count(paths, codes::HOOK_SHOWN_UNSENT);
                                "card-unsent"
                            }
                        }
                    }
                    // No tip: a milestone just earned, said once in the conversation (Claude Code only).
                    // simplify: printed without a "shown" confirmation; a note lost here is still on the Milestones
                    // page. Upgrade trigger: a note reported missing from a turn end that printed nothing.
                    Ok(TakeCardReply { card: None, note: Some(note), plain, .. }) if host != "copilot" => {
                        let text = claude_code_note_message(&note, color_allowed() && !plain);
                        let mut out = std::io::stdout().lock();
                        if writeln!(out, "{text}").and_then(|_| out.flush()).is_err() {
                            "note-unprinted"
                        } else {
                            "note"
                        }
                    }
                    _ => "no-card",
                },
                Ok(None) => "no-card",
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => stop_timed_out(paths, &trace_id, &ev, written),
                // The sync Stop hook never starts the core: spool-plus-spawn measured over budget (S3a).
                Err(_) => {
                    spool_frame(paths, &trace_id, &ev);
                    "spooled"
                }
            }
        }
    }
}

/// The Stop hook's wait ran out. Written: the core holds the stop and only its reply was late. Not written:
/// this process ends now and takes the half-done send with it, so the turn end goes to the spool (DC-034;
/// the core ingests by event id, so a frame that got through after all is still stored once).
fn stop_timed_out(paths: &Paths, trace_id: &str, ev: &EventV1, written: bool) -> &'static str {
    count(paths, codes::HOOK_STOP_TIMEOUT);
    if written {
        return "stop-timeout";
    }
    spool_frame(paths, trace_id, ev);
    "stop-timeout-spooled"
}

/// A Copilot CLI turn end, which shows its tip in the conversation (VS Code's turn ends never print).
fn copilot_cli_stop(ev: &EventV1) -> bool {
    matches!(ev.body, EventBody::Stop(_)) && ev.surface == COPILOT_CLI_SURFACE
}

/// Build `aiq.event.v1` from a Claude Code payload. None when the payload is not Claude Code's shape for
/// this event: snake_case `session_id` and a `hook_event_name` equal to the argv event (Copilot CLI
/// payloads use camelCase `sessionId` and no event name).
pub fn to_event(v: &Value, event: &str) -> Option<EventV1> {
    let session = v.get("session_id")?.as_str().filter(|s| !s.is_empty())?;
    if v.get("hook_event_name")?.as_str()? != event {
        return None;
    }
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
    let body = match event {
        "UserPromptSubmit" => {
            EventBody::Prompt(prompt_payload(v.get("prompt").and_then(Value::as_str), s("prompt_id")))
        }
        "Stop" => EventBody::Stop(StopPayload {
            stop_hook_active: v.get("stop_hook_active").and_then(Value::as_bool).unwrap_or(false),
            prompt_id: s("prompt_id"),
        }),
        _ => return None,
    };
    let mut ev = new_event("claude-code", session, cwd_of(v), body);
    // Rule RC: the hook never opens the transcript. On Imran's laptop that open alone was ~0.4 s of every
    // Claude Code hook (measured 2026-11-02: Stop fell from p95 493 ms to 0.2 ms when it stopped reading it;
    // Copilot hooks, which never read it, prepare in 0.3 ms). Every event carries the path; the core reads
    // the head off the hook's clock until it settles, so a head not yet written at the first prompt is
    // read again at that turn's Stop.
    ev.transcript = v
        .get("transcript_path")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty() && t.len() <= 4096)
        .map(str::to_owned);
    Some(ev)
}

pub const COPILOT_CLI_SURFACE: &str = aiq_core::service::COPILOT_CLI;
pub const VSCODE_SURFACE: &str = "vscode";

/// Surface of a Copilot hook. Copilot CLI and VS Code agent mode share `~/.copilot/hooks` and send the same
/// payload, so the environment decides. Laptop probe run 2 (2026-11-01): VS Code's agent host sets
/// `COPILOT_CLI_RUN_AS_NODE` and not `COPILOT_LOADER_PID`; the Copilot CLI sets `COPILOT_LOADER_PID` (and
/// `COPILOT_HOOK_DIR`) and not `COPILOT_CLI_RUN_AS_NODE`.
/// assume: these env names hold across Copilot / VS Code updates (one VS Code turn and 2 CLI sessions seen,
/// Copilot CLI 1.0.105). Confirm: the Phase 2 laptop check. If false: VS Code turns are tagged
/// `copilot-cli`, so their tips wait for the status line instead of a VS Code notification (no data lost).
pub fn copilot_surface(is_set: impl Fn(&str) -> bool) -> &'static str {
    if is_set("COPILOT_CLI_RUN_AS_NODE") && !is_set("COPILOT_LOADER_PID") {
        VSCODE_SURFACE
    } else {
        COPILOT_CLI_SURFACE
    }
}

/// Build `aiq.event.v1` from a Copilot payload: camelCase `sessionId`, no event name (it comes from argv),
/// and none of Claude Code's snake_case fields (a Claude-shaped payload here is a foreign host).
pub fn to_copilot_event(v: &Value, event: &str, surface: &str) -> Option<EventV1> {
    if v.get("session_id").is_some() || v.get("hook_event_name").is_some() {
        return None;
    }
    let session = v.get("sessionId")?.as_str().filter(|s| !s.is_empty())?;
    let body = match event {
        "userPromptSubmitted" => EventBody::Prompt(prompt_payload(v.get("prompt").and_then(Value::as_str), None)),
        "agentStop" => EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None }),
        _ => return None,
    };
    Some(new_event(surface, session, cwd_of(v), body))
}

/// Secrets are removed here, before the frame is sent or spooled (security review S2); the core removes
/// them again on ingest for anything that reaches it another way.
fn prompt_payload(raw: Option<&str>, prompt_id: Option<String>) -> PromptPayload {
    let (text, truncated) = match raw {
        Some(t) => {
            let (c, cut) = cap_text(&aiq_domain::secrets::redact(t).0, MAX_PROMPT_BYTES);
            (Some(c), cut)
        }
        None => (None, false),
    };
    PromptPayload { text, chars: raw.map_or(0, |t| t.chars().count() as u64), prompt_id, truncated }
}

/// Both hosts send the session's working folder as `cwd`.
fn cwd_of(v: &Value) -> Option<String> {
    v.get("cwd").and_then(Value::as_str).filter(|c| !c.is_empty() && c.len() <= MAX_CWD_BYTES).map(str::to_owned)
}

fn new_event(surface: &str, session: &str, cwd: Option<String>, body: EventBody) -> EventV1 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    EventV1 {
        schema: SCHEMA.into(),
        event_id: uuid::Uuid::now_v7().to_string(),
        surface: surface.into(),
        surface_session_id: session.to_owned(),
        seq: None,
        ts: aiq_core::sysclock::format_rfc3339_ms(now),
        collector_version: env!("CARGO_PKG_VERSION").into(),
        cwd,
        projects: false,
        transcript: None,
        body,
    }
}

fn spool_frame(paths: &Paths, trace_id: &str, ev: &EventV1) {
    count(paths, codes::HOOK_SPOOLED);
    let line = serde_json::to_string(&SpoolLine { trace_id: Some(trace_id.to_owned()), event: ev.clone() })
        .unwrap_or_default();
    let _ = paths.ensure_dirs();
    match spool::append_frame(&paths.spool_dir(), &ev.event_id, &line, spool::MAX_SPOOL_BYTES) {
        Ok(0) => {}
        Ok(n) => (0..n).for_each(|_| count(paths, codes::SPOOL_DROPPED)),
        Err(_) => count(paths, codes::STORE_WRITE),
    }
}

/// no-color.org: any non-empty `NO_COLOR` turns colour off. Claude Code passes its environment to hooks.
pub fn color_allowed() -> bool {
    std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

/// At most one start attempt per 10 s, guarded by a timestamp in a lock file.
fn maybe_start_core(paths: &Paths) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let last = std::fs::read_to_string(paths.start_lock()).ok().and_then(|s| s.trim().parse::<u64>().ok());
    if last.is_some_and(|t| now < t + CORE_START_BACKOFF_S) {
        count(paths, codes::CORE_START_BACKOFF);
        return;
    }
    // An update is swapping the exes: a core started now would hold aiq.exe (DC-071). upgrade.ps1 starts it after.
    if aiq_core::update::swapping(paths, now) {
        count(paths, codes::CORE_START_UPDATING);
        return;
    }
    let _ = aiq_core::paths::write_user_only(&paths.start_lock(), now.to_string().as_bytes());
    let _ = crate::spawn::core_detached();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_timed_out_stop_that_never_left_goes_to_the_spool() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::at(d.path());
        let stop = to_event(&json!({"session_id":"s","hook_event_name":"Stop"}), "Stop").unwrap();

        assert_eq!(stop_timed_out(&paths, "t1", &stop, true), "stop-timeout");
        assert_eq!(aiq_core::spool::pending(&paths.spool_dir()), 0, "the core already holds a written stop");

        assert_eq!(stop_timed_out(&paths, "t2", &stop, false), "stop-timeout-spooled");
        let lines = aiq_core::spool::take_frames(&paths.spool_dir()).unwrap();
        let (frames, bad) = aiq_core::spool::parse(&lines);
        assert_eq!(bad, 0);
        assert_eq!(frames.len(), 1, "the turn end is kept for the core");
        assert_eq!(frames[0].event.event_id, stop.event_id);
        let errors = std::fs::read_to_string(paths.hook_errors()).unwrap();
        assert_eq!(errors.matches(codes::HOOK_STOP_TIMEOUT).count(), 2, "both timeouts still count");
    }

    #[test]
    fn timing_line_keeps_the_old_head_and_adds_phases_time_and_build() {
        let m = Marks {
            host_ms: Some(40),
            read_us: Some(1200),
            prep_us: Some(300),
            dir: Some("0a1b2c3d".into()),
            session: Some("5e55a000".into()),
        };
        let l = timing_line("UserPromptSubmit", 2000, "sent", &m, 1_762_000_000);
        assert!(l.starts_with(
            "UserPromptSubmit 2000 sent host_ms=40 read_us=1200 prep_us=300 dir=0a1b2c3d s=5e55a000 at=1762000000 v="
        ));
        let bare = timing_line("Stop", 9, "paused", &Marks::default(), 5);
        assert_eq!(bare, format!("Stop 9 paused at=5 v={}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn the_timing_log_has_one_writer() {
        // Defect class HOOK-LOG-CAP: the status-line command wrote the same log with a 32 KiB cap and emptied it.
        // Any other file that names the log must not append to anything; only hook::append_timing writes it.
        for (name, src) in [("statusline.rs", include_str!("statusline.rs")), ("cli.rs", include_str!("cli.rs"))] {
            assert!(
                !(src.contains("hook_timings") && src.contains("_append(")),
                "{name} names the timing log and appends; use hook::append_timing"
            );
        }
    }

    #[test]
    fn to_event_claude_prompt_payload() {
        let v = json!({"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":"fix it","prompt_id":"p1","cwd":"/x","transcript_path":"/t","permission_mode":"default"});
        let e = to_event(&v, "UserPromptSubmit").unwrap();
        assert_eq!(e.surface_session_id, "s");
        assert_eq!(
            e.body,
            EventBody::Prompt(PromptPayload {
                text: Some("fix it".into()),
                chars: 6,
                prompt_id: Some("p1".into()),
                truncated: false
            })
        );
    }

    #[test]
    fn to_event_passes_the_transcript_path_on_every_event_and_never_reads_it() {
        let d = tempfile::tempdir().unwrap();
        let t = d.path().join("s.jsonl");
        std::fs::write(
            &t,
            format!("{}\n", json!({"type":"user","isMeta":true,"turnOrigin":"system","entrypoint":"claude-desktop"})),
        )
        .unwrap();
        let p = json!({"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":"x","cwd":"/x","transcript_path":t.to_string_lossy()});
        let e = to_event(&p, "UserPromptSubmit").unwrap();
        assert!(!e.projects, "the core decides, not the hook");
        assert_eq!(e.transcript.as_deref(), Some(t.to_string_lossy().as_ref()));
        let stop = json!({"session_id":"s","hook_event_name":"Stop","cwd":"/x","transcript_path":t.to_string_lossy()});
        let s = to_event(&stop, "Stop").unwrap();
        assert!(!s.projects);
        assert_eq!(s.transcript.as_deref(), Some(t.to_string_lossy().as_ref()), "the Stop can settle rule RC");
    }

    #[test]
    fn host_start_ms_plausible_only() {
        assert_eq!(host_start_ms(&json!({"timestamp": 1_000}), 1_150), Some(150));
        assert_eq!(host_start_ms(&json!({"timestamp": 2_000}), 1_000), None);
        assert_eq!(host_start_ms(&json!({"timestamp": "x"}), 1_000), None);
        assert_eq!(host_start_ms(&json!({}), 1_000), None);
    }

    #[test]
    fn to_copilot_event_prompt_and_stop() {
        let v = json!({"sessionId":"c1","timestamp":1793500000000_i64,"cwd":"/x","prompt":"fix it"});
        let e = to_copilot_event(&v, "userPromptSubmitted", COPILOT_CLI_SURFACE).unwrap();
        assert_eq!((e.surface.as_str(), e.surface_session_id.as_str()), ("copilot-cli", "c1"));
        assert!(
            matches!(&e.body, EventBody::Prompt(p) if p.text.as_deref() == Some("fix it") && p.prompt_id.is_none())
        );
        let v = json!({"sessionId":"c1","timestamp":1,"stopReason":"end_turn","transcriptPath":"/t"});
        let e = to_copilot_event(&v, "agentStop", COPILOT_CLI_SURFACE).unwrap();
        assert_eq!(e.body, EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None }));
    }

    #[test]
    fn a_copilot_cli_hook_records_the_program_and_home_that_run_it() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::at(d.path());
        std::fs::create_dir_all(&paths.data).unwrap();
        let me = std::process::id().to_string();
        let env = |k: &str| match k {
            "COPILOT_LOADER_PID" => Some(me.clone()),
            "COPILOT_HOOK_DIR" => Some("/h/.copilot/hooks".to_string()),
            _ => None,
        };
        record_copilot_seen(&paths, env);
        let s = aiq_core::copilot_seen::read(&paths).unwrap();
        assert_eq!(s.exe.clone().map(std::path::PathBuf::from), std::env::current_exe().ok(), "this process's program");
        assert_eq!(s.home(), Some(std::path::PathBuf::from("/h/.copilot")));
        // VS Code agent mode sets no loader pid: nothing recorded.
        let d2 = tempfile::tempdir().unwrap();
        let p2 = Paths::at(d2.path());
        std::fs::create_dir_all(&p2.data).unwrap();
        record_copilot_seen(&p2, |_| None);
        assert!(aiq_core::copilot_seen::read(&p2).is_none());
    }

    #[test]
    fn copilot_surface_from_probe_env() {
        let has = |names: &'static [&'static str]| move |k: &str| names.contains(&k);
        // Laptop probe run 2: VS Code agent mode (session target Copilot) and Copilot CLI 1.0.105.
        let vscode: &[&str] = &["COPILOT_CLI", "COPILOT_CLI_RUN_AS_NODE", "COPILOT_MCP_APPS", "COPILOT_PROJECT_DIR"];
        let cli: &[&str] = &["COPILOT_CLI", "COPILOT_CLI_BINARY_VERSION", "COPILOT_HOOK_DIR", "COPILOT_LOADER_PID"];
        assert_eq!(copilot_surface(has(vscode)), "vscode");
        assert_eq!(copilot_surface(has(cli)), "copilot-cli");
        assert_eq!(copilot_surface(has(&[])), "copilot-cli");
        assert_eq!(copilot_surface(has(&["COPILOT_CLI_RUN_AS_NODE", "COPILOT_LOADER_PID"])), "copilot-cli");
        let v = json!({"sessionId":"v1","prompt":"x"});
        assert_eq!(to_copilot_event(&v, "userPromptSubmitted", VSCODE_SURFACE).unwrap().surface, "vscode");
    }

    #[test]
    fn claude_headless_is_sdk_entrypoints_only() {
        assert!(claude_headless(Some("sdk-cli")) && claude_headless(Some("sdk-ts")));
        assert!(!claude_headless(Some("cli")) && !claude_headless(Some("claude-desktop")) && !claude_headless(None));
    }

    #[test]
    fn events_carry_the_hosts_cwd() {
        let v = json!({"sessionId":"c1","cwd":"C:\\EP","prompt":"x"});
        assert_eq!(
            to_copilot_event(&v, "userPromptSubmitted", COPILOT_CLI_SURFACE).unwrap().cwd.as_deref(),
            Some("C:\\EP")
        );
        let v = json!({"session_id":"s","hook_event_name":"Stop","cwd":"x".repeat(MAX_CWD_BYTES + 1)});
        assert_eq!(to_event(&v, "Stop").unwrap().cwd, None, "an oversized cwd is dropped, not cut");
    }

    #[test]
    fn to_copilot_event_claude_shaped_or_unknown_dropped() {
        let claude = json!({"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":"x"});
        assert!(to_copilot_event(&claude, "userPromptSubmitted", COPILOT_CLI_SURFACE).is_none());
        assert!(
            to_copilot_event(&json!({"sessionId":"s","session_id":"s"}), "agentStop", COPILOT_CLI_SURFACE).is_none()
        );
        assert!(to_copilot_event(&json!({"sessionId":""}), "agentStop", COPILOT_CLI_SURFACE).is_none());
        assert!(to_copilot_event(&json!({"sessionId":"s"}), "preToolUse", COPILOT_CLI_SURFACE).is_none());
    }

    #[test]
    fn to_copilot_event_oversized_prompt_truncates() {
        let v = json!({"sessionId":"s","prompt":"x".repeat(MAX_PROMPT_BYTES + 10)});
        let EventBody::Prompt(p) = to_copilot_event(&v, "userPromptSubmitted", COPILOT_CLI_SURFACE).unwrap().body
        else {
            panic!()
        };
        assert!(p.truncated && p.text.unwrap().len() == MAX_PROMPT_BYTES);
    }

    #[test]
    fn to_event_copilot_shaped_payload_dropped() {
        let v = json!({"sessionId":"s","prompt":"fix it","timestamp":1});
        assert!(to_event(&v, "UserPromptSubmit").is_none());
    }

    #[test]
    fn to_event_event_name_mismatch_dropped() {
        let v = json!({"session_id":"s","hook_event_name":"Stop"});
        assert!(to_event(&v, "UserPromptSubmit").is_none());
        assert!(to_event(&json!({"session_id":"","hook_event_name":"Stop"}), "Stop").is_none());
    }

    #[test]
    fn to_event_missing_prompt_keeps_event_without_text() {
        let v = json!({"session_id":"s","hook_event_name":"UserPromptSubmit"});
        let e = to_event(&v, "UserPromptSubmit").unwrap();
        assert!(matches!(e.body, EventBody::Prompt(PromptPayload { text: None, .. })));
    }

    #[test]
    fn to_event_oversized_prompt_truncates_to_64kb() {
        let v = json!({"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":"é".repeat(40_000)});
        let EventBody::Prompt(p) = to_event(&v, "UserPromptSubmit").unwrap().body else { panic!() };
        assert!(p.truncated);
        assert!(p.text.unwrap().len() <= MAX_PROMPT_BYTES);
        assert_eq!(p.chars, 40_000);
    }

    #[test]
    fn prompts_lose_their_secrets_before_send_or_spool() {
        let key = "ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
        let v = json!({"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":format!("push with {key}")});
        let e = to_event(&v, "UserPromptSubmit").unwrap();
        let wire = serde_json::to_string(&e).unwrap();
        assert!(!wire.contains(key) && wire.contains(aiq_domain::secrets::MARKER), "{wire}");
        let v = json!({"sessionId":"c","prompt":format!("use {key}")});
        let wire =
            serde_json::to_string(&to_copilot_event(&v, "userPromptSubmitted", COPILOT_CLI_SURFACE).unwrap()).unwrap();
        assert!(!wire.contains(key), "{wire}");
    }

    #[test]
    fn to_event_stop_drops_last_assistant_message() {
        let v = json!({"session_id":"s","hook_event_name":"Stop","stop_hook_active":true,"prompt_id":"p","last_assistant_message":"secret reply"});
        let e = to_event(&v, "Stop").unwrap();
        assert!(!serde_json::to_string(&e).unwrap().contains("secret reply"));
        assert_eq!(e.body, EventBody::Stop(StopPayload { stop_hook_active: true, prompt_id: Some("p".into()) }));
    }
}
