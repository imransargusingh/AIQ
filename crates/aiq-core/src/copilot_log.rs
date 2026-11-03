//! Copilot's own session log, `<copilot home>/session-state/<session>/events.jsonl`, read-only.
//!
//! Copilot CLI and VS Code agent mode both write it. `aiq import` reads whole files; the core reads only the
//! new part of a live session's file at each `agentStop`, to pick up the prompts the hook never sees.
//! Measured on Imran's laptop 2026-11-03 (Copilot CLI 1.0.106-2, 52 typed messages in 9 sessions): a message
//! sent while the agent is still working (`delivery = "steering"`) fires no `userPromptSubmitted` hook,
//! 7 of 7; `idle` and `queued` messages fire it (36 of 38). So the core takes `steering` messages from the
//! log and leaves the rest to the hook.
//! assume: Copilot keeps these field names (`type`, `agentId`, `data.delivery`, `data.content`). Confirm: a
//! message typed while the agent works shows in AIQ home's Your data. If false: those messages are missed, as
//! before (nothing else changes).
use serde_json::Value;
use std::path::{Path, PathBuf};

/// A message the person typed into a session's main line (not a sub-agent's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typed {
    /// `idle` (agent waiting), `queued` (sent while busy, run after) or `steering` (sent into the running turn).
    pub delivery: String,
    pub ts_ms: Option<i64>,
    pub text: String,
}

/// One log record as a typed message, or None. Shared with `aiq import`.
pub fn typed_message(e: &Value) -> Option<Typed> {
    if e.get("type").and_then(Value::as_str) != Some("user.message") || !e.get("agentId").is_none_or(Value::is_null) {
        return None;
    }
    let d = &e["data"];
    Some(Typed {
        delivery: d["delivery"].as_str()?.to_owned(),
        ts_ms: ts_ms(&e["timestamp"]),
        text: d["content"].as_str().unwrap_or("").to_owned(),
    })
}

pub fn ts_ms(v: &Value) -> Option<i64> {
    v.as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp_millis())
}

/// The session's log file. None for a session id that is not a plain folder name.
pub fn session_log(copilot_home: &Path, session: &str) -> Option<PathBuf> {
    let ok = !session.is_empty()
        && session.len() <= 128
        && session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then(|| copilot_home.join("session-state").join(session).join("events.jsonl"))
}

/// Steering messages in the file from byte `from` on, and the offset to read from next time (whole lines only,
/// at most `agent_actions::MAX_READ_BYTES`). The core reads them with the turn's actions (`agent_actions::read_since`).
pub fn steering_since(path: &Path, from: u64) -> std::io::Result<(Vec<Typed>, u64)> {
    let r = crate::agent_actions::read_since(path, from, crate::agent_actions::LogKind::Copilot)?;
    Ok((r.steering, r.next))
}

/// A stable event id for a message read from the log, so reading the same line again stores nothing new.
pub fn event_id(surface: &str, session: &str, t: &Typed) -> String {
    let fnv = |seed: u64| {
        [surface, "\u{1f}", session, "\u{1f}", &t.ts_ms.unwrap_or(0).to_string(), "\u{1f}", &t.text]
            .iter()
            .flat_map(|s| s.bytes())
            .fold(seed, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
    };
    uuid::Uuid::from_u64_pair(fnv(0xcbf2_9ce4_8422_2325), fnv(0x6c62_272e_07bb_0142)).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msg(delivery: &str, text: &str, ts: &str) -> String {
        json!({"type":"user.message","timestamp":ts,"data":{"content":text,"delivery":delivery}}).to_string()
    }

    #[test]
    fn takes_main_line_steering_only_and_whole_lines_only() {
        let d = tempfile::TempDir::new().unwrap();
        let p = d.path().join("events.jsonl");
        let sub = json!({"type":"user.message","agentId":"a1","data":{"content":"x","delivery":"steering"}});
        let body = [
            msg("idle", "first", "2026-11-03T00:00:00Z"),
            msg("steering", "also check the tests", "2026-11-03T00:00:05Z"),
            sub.to_string(),
            msg("queued", "then this", "2026-11-03T00:00:06Z"),
            json!({"type":"tool.execution_complete","data":{"result":"no steering here"}}).to_string(),
        ]
        .join("\n");
        std::fs::write(&p, format!("{body}\n{}", &msg("steering", "half", "2026-11-03T00:00:09Z")[..20])).unwrap();
        let (got, next) = steering_since(&p, 0).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text, "also check the tests");
        assert_eq!(got[0].ts_ms, Some(1_793_664_005_000));
        assert_eq!(next, body.len() as u64 + 1, "the half-written last line waits");
        // The line is finished later: the next read takes it and nothing earlier again.
        std::fs::write(&p, format!("{body}\n{}\n", msg("steering", "half", "2026-11-03T00:00:09Z"))).unwrap();
        let (got, _) = steering_since(&p, next).unwrap();
        assert_eq!(got.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["half"]);
        // A replaced, shorter file is read from the start.
        std::fs::write(&p, format!("{}\n", msg("steering", "new", "2026-11-03T00:01:00Z"))).unwrap();
        assert_eq!(steering_since(&p, 10_000).unwrap().0.len(), 1);
    }

    #[test]
    fn session_log_refuses_ids_that_are_not_plain_names() {
        let h = Path::new("/h");
        assert!(session_log(h, "ad4c0f5d-1b2c-4d5e-8f90-123456789abc").is_some());
        for bad in ["", "..", "../x", "a/b", "a\\b", "c:x"] {
            assert!(session_log(h, bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn event_id_is_stable_and_differs_by_message() {
        let t = |s: &str| Typed { delivery: "steering".into(), ts_ms: Some(1), text: s.into() };
        assert_eq!(event_id("vscode", "s", &t("a")), event_id("vscode", "s", &t("a")));
        assert_ne!(event_id("vscode", "s", &t("a")), event_id("vscode", "s", &t("b")));
        assert_ne!(event_id("vscode", "s", &t("a")), event_id("copilot-cli", "s", &t("a")));
    }
}
