//! `aiq hook claude-code <event>` and `aiq hook copilot <event>`: the hot path. Guarantees (Phase 1
//! design: Exposed contract 1; Phase 2 design: Exposed contract 1): always exits 0; never opens the store
//! or loads the pack. Claude Code: stdout is empty, or exactly one `{"systemMessage": …}` on Stop; never
//! `additionalContext`, `decision` or `continue`. Copilot: stdout and stderr are always empty (Copilot
//! shows the person nothing, and its hooks block the agent, so both events are plain ingests).
use aiq_contracts::card::claude_code_system_message;
use aiq_contracts::codes;
use aiq_contracts::event::{EventBody, EventV1, MAX_PROMPT_BYTES, PromptPayload, SCHEMA, StopPayload, cap_text};
use aiq_contracts::frames::{Request, SpoolLine, TakeCardReply};
use aiq_core::ipc;
use aiq_core::paths::{Paths, bounded_append};
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
/// The timing log keeps about 1,000 runs at ~110 bytes a line; it restarts when full.
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
    let total = start.elapsed().as_micros();
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let _ = bounded_append(&paths.hook_timings(), &timing_line(event, total, outcome, &marks, at), TIMINGS_LOG_MAX);
    ExitCode::SUCCESS
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
    if paths.pause_flag().exists() {
        return "paused"; // nothing captured while paused
    }
    let trace_id = uuid::Uuid::new_v4().simple().to_string();
    marks.prep_us = Some(start.elapsed().as_micros() - read_done);
    let token = ipc::read_token(paths).unwrap_or_default();
    match &ev.body {
        // Copilot: both events are ingests (nothing to show at agentStop; the status line renders).
        EventBody::Prompt(_) | EventBody::Stop(_) if host == "copilot" => {
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
            let frame =
                Request::TakeCard { token, trace_id: Some(trace_id.clone()), event: ev.clone(), wait_ms: STOP_WAIT_MS };
            match ipc::request(
                paths,
                &serde_json::to_string(&frame).unwrap_or_default(),
                true,
                SEND_BUDGET + STOP_REPLY_BUDGET,
            ) {
                Ok(Some(line)) => match serde_json::from_str::<TakeCardReply>(&line) {
                    Ok(TakeCardReply { card: Some(card) }) => {
                        let mut out = std::io::stdout().lock();
                        let _ = writeln!(out, "{}", claude_code_system_message(&card, color_allowed()));
                        let _ = out.flush();
                        "card"
                    }
                    _ => "no-card",
                },
                Ok(None) => "no-card",
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    count(paths, codes::HOOK_STOP_TIMEOUT);
                    "stop-timeout"
                }
                // The sync Stop hook never starts the core: spool-plus-spawn measured over budget (S3a).
                Err(_) => {
                    spool_frame(paths, &trace_id, &ev);
                    "spooled"
                }
            }
        }
    }
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
    // Rule RC: Claude Code writes the session's first record (a Projects session's system turn) 64 ms–3.7 s
    // before the first prompt's hook fires (laptop measurement, 11 of 11 sessions), within the first 31 KB.
    ev.projects = v
        .get("transcript_path")
        .and_then(Value::as_str)
        .is_some_and(|t| crate::projects::scan(std::path::Path::new(t), crate::projects::HEAD_BYTES));
    Some(ev)
}

pub const COPILOT_CLI_SURFACE: &str = "copilot-cli";
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
        body,
    }
}

fn spool_frame(paths: &Paths, trace_id: &str, ev: &EventV1) {
    count(paths, codes::HOOK_SPOOLED);
    let line = serde_json::to_string(&SpoolLine { trace_id: Some(trace_id.to_owned()), event: ev.clone() })
        .unwrap_or_default();
    let _ = paths.ensure_dirs();
    match spool::append(&paths.spool(), &line, spool::MAX_SPOOL_BYTES) {
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
    let _ = aiq_core::paths::write_user_only(&paths.start_lock(), now.to_string().as_bytes());
    let _ = crate::spawn::core_detached();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn timing_line_keeps_the_old_head_and_adds_phases_time_and_build() {
        let m = Marks { host_ms: Some(40), read_us: Some(1200), prep_us: Some(300) };
        let l = timing_line("UserPromptSubmit", 2000, "sent", &m, 1_762_000_000);
        assert!(l.starts_with("UserPromptSubmit 2000 sent host_ms=40 read_us=1200 prep_us=300 at=1762000000 v="));
        let bare = timing_line("Stop", 9, "paused", &Marks::default(), 5);
        assert_eq!(bare, format!("Stop 9 paused at=5 v={}", env!("CARGO_PKG_VERSION")));
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
    fn to_event_marks_projects_sessions_from_the_transcript() {
        let d = tempfile::tempdir().unwrap();
        let t = d.path().join("s.jsonl");
        let v = |t: &std::path::Path| json!({"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":"x","cwd":"/x","transcript_path":t.to_string_lossy()});
        assert!(!to_event(&v(&t), "UserPromptSubmit").unwrap().projects, "no transcript yet");
        std::fs::write(
            &t,
            format!("{}\n", json!({"type":"user","isMeta":true,"turnOrigin":"system","entrypoint":"claude-desktop"})),
        )
        .unwrap();
        assert!(to_event(&v(&t), "UserPromptSubmit").unwrap().projects);
        std::fs::write(&t, format!("{}\n", json!({"type":"user","turnOrigin":"human","entrypoint":"cli"}))).unwrap();
        assert!(!to_event(&v(&t), "UserPromptSubmit").unwrap().projects);
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
