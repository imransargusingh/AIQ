//! `aiq statusline copilot`: Copilot CLI's status-line command (ADR-0005 rule 5; Phase 2 design, Exposed
//! contract 2). Reads Copilot's session JSON on stdin, asks the core for this session's tip line, prints at
//! most one line, always exits 0. Never starts the core (it runs on every refresh) and never opens the store.
use aiq_contracts::card::{render_status_line, render_waiting_marker};
use aiq_contracts::frames::{Request, TipLineReply};
use aiq_core::ipc;
use aiq_core::paths::{Paths, bounded_append};
use serde_json::Value;
use std::io::{Read, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

pub const MAX_STDIN: u64 = 64 * 1024;
/// Connect + write, then the core's own 50 ms render wait.
pub const BUDGET: Duration = Duration::from_millis(80);

pub fn run() -> ExitCode {
    let start = Instant::now();
    std::panic::set_hook(Box::new(|_| {}));
    let paths = Paths::from_env();
    let outcome = std::panic::catch_unwind(|| inner(&paths)).unwrap_or("panic");
    let _ = bounded_append(
        &paths.hook_timings(),
        &format!("statusline {} {outcome}", start.elapsed().as_micros()),
        32 * 1024,
    );
    ExitCode::SUCCESS
}

/// The status line's session id. The key is not documented (probe P2); try the likely shapes.
pub fn session_id(v: &Value) -> Option<String> {
    let direct = ["sessionId", "session_id"].iter().find_map(|k| v.get(*k).and_then(Value::as_str));
    direct
        .or_else(|| v.get("session").and_then(|s| s.get("id")).and_then(Value::as_str))
        .filter(|s| !s.is_empty() && s.len() <= aiq_contracts::event::MAX_SESSION_ID)
        .map(str::to_owned)
}

fn inner(paths: &Paths) -> &'static str {
    let mut buf = Vec::new();
    let _ = std::io::stdin().lock().take(MAX_STDIN).read_to_end(&mut buf);
    if paths.pause_flag().exists() {
        return "paused";
    }
    let session = serde_json::from_slice::<Value>(&buf).ok().as_ref().and_then(session_id);
    let has_session = session.is_some();
    let token = ipc::read_token(paths).unwrap_or_default();
    let frame = Request::TipLine { token, surface: crate::hook::COPILOT_SURFACE.into(), session };
    let Ok(Some(line)) = ipc::request(paths, &serde_json::to_string(&frame).unwrap_or_default(), true, BUDGET) else {
        return "no-core";
    };
    let text = match serde_json::from_str::<TipLineReply>(&line) {
        Ok(TipLineReply { card: Some(c), .. }) => render_status_line(&c, crate::hook::color_allowed()),
        Ok(TipLineReply { waiting_card_id: Some(id), .. }) => render_waiting_marker(&id),
        _ => return if has_session { "none" } else { "none-no-session" },
    };
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{text}");
    let _ = out.flush();
    if has_session { "tip" } else { "tip-no-session" }
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
}
