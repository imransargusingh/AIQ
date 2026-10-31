//! `aiq hook claude-code <event>`: the hot path. Guarantees (design: Exposed contract 1):
//! always exits 0; stdout is empty, or exactly one `{"systemMessage": …}` on Stop; never writes
//! `additionalContext`, `decision` or `continue`; never opens the store or loads the pack.
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
/// Bytes past the cap that are read and thrown away (bounded, so a runaway writer cannot hold the hook).
pub const MAX_DRAIN: u64 = 16 * 1024 * 1024;
/// Connect + write budget (ADR-0005).
pub const SEND_BUDGET: Duration = Duration::from_millis(20);
/// Stop: what the core may spend finding a card, and the client's reply wait on top of the send.
pub const STOP_WAIT_MS: u64 = 30;
pub const STOP_REPLY_BUDGET: Duration = Duration::from_millis(35);
const CORE_START_BACKOFF_S: u64 = 10;
const SMALL_LOG_MAX: u64 = 32 * 1024;

pub fn run(host: &str, event: &str) -> ExitCode {
    let start = Instant::now();
    std::panic::set_hook(Box::new(|_| {})); // a panic must never print into Claude Code
    let paths = Paths::from_env();
    let outcome = std::panic::catch_unwind(|| inner(&paths, host, event)).unwrap_or("panic");
    let _ = bounded_append(
        &paths.hook_timings(),
        &format!("{event} {} {outcome}", start.elapsed().as_micros()),
        SMALL_LOG_MAX,
    );
    ExitCode::SUCCESS
}

fn count(paths: &Paths, code: &str) {
    let _ = bounded_append(&paths.hook_errors(), code, SMALL_LOG_MAX);
}

/// Returns a short outcome label for the timing log.
fn inner(paths: &Paths, host: &str, event: &str) -> &'static str {
    let mut buf = Vec::new();
    let mut stdin = std::io::stdin().lock();
    let _ = (&mut stdin).take(MAX_STDIN).read_to_end(&mut buf);
    // Discard the rest instead of closing early, so the host never sees a broken pipe while writing.
    let _ = std::io::copy(&mut stdin.take(MAX_DRAIN), &mut std::io::sink());
    if host != "claude-code" {
        return "unknown-host";
    }
    let v: Value = match serde_json::from_slice(&buf) {
        Ok(v @ Value::Object(_)) => v,
        _ => {
            count(paths, codes::HOOK_PARSE);
            return "parse";
        }
    };
    let Some(ev) = to_event(&v, event) else {
        count(paths, codes::HOOK_FOREIGN_HOST);
        return "foreign";
    };
    if paths.pause_flag().exists() {
        return "paused"; // nothing captured while paused
    }
    let trace_id = uuid::Uuid::new_v4().simple().to_string();
    let token = ipc::read_token(paths).unwrap_or_default();
    match &ev.body {
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
                        let _ = writeln!(out, "{}", claude_code_system_message(&card));
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
            let raw = v.get("prompt").and_then(Value::as_str);
            let (text, truncated) = match raw {
                Some(t) => {
                    let (c, cut) = cap_text(t, MAX_PROMPT_BYTES);
                    (Some(c), cut)
                }
                None => (None, false),
            };
            EventBody::Prompt(PromptPayload {
                text,
                chars: raw.map_or(0, |t| t.chars().count() as u64),
                prompt_id: s("prompt_id"),
                truncated,
            })
        }
        "Stop" => EventBody::Stop(StopPayload {
            stop_hook_active: v.get("stop_hook_active").and_then(Value::as_bool).unwrap_or(false),
            prompt_id: s("prompt_id"),
        }),
        _ => return None,
    };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    Some(EventV1 {
        schema: SCHEMA.into(),
        event_id: uuid::Uuid::now_v7().to_string(),
        surface: "claude-code".into(),
        surface_session_id: session.to_owned(),
        seq: None,
        ts: aiq_core::sysclock::format_rfc3339_ms(now),
        collector_version: env!("CARGO_PKG_VERSION").into(),
        body,
    })
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
    fn to_event_stop_drops_last_assistant_message() {
        let v = json!({"session_id":"s","hook_event_name":"Stop","stop_hook_active":true,"prompt_id":"p","last_assistant_message":"secret reply"});
        let e = to_event(&v, "Stop").unwrap();
        assert!(!serde_json::to_string(&e).unwrap().contains("secret reply"));
        assert_eq!(e.body, EventBody::Stop(StopPayload { stop_hook_active: true, prompt_id: Some("p".into()) }));
    }
}
