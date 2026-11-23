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
use crate::projects::Turn;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// A message the person typed into a session's main line (not a sub-agent's, not one autopilot sent).
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
    // Autopilot mode sends follow-up turns itself and marks them (measured 2026-11-15: a program's `copilot -p`
    // sessions passed as multi-prompt work because AIQ counted these). Only the person's own turns count.
    if d["isAutopilotContinuation"].as_bool() == Some(true) {
        return None;
    }
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

/// Who sent the live prompt whose hook text is `text` and whose hook ran at `prompt_ms`, from its own record in the
/// session log (DC-096): `Own` for a message the person typed, `Other` for one autopilot sent
/// (`isAutopilotContinuation`), `Unknown` while no record matches yet. The record is the main-line `user.message`
/// with the same text (secrets removed as the hook does, whitespace trimmed; a prompt the hook cut is matched on its
/// start), stamped within
/// `projects::TURN_MATCH_MS` of the hook, nearest first, and not one an earlier prompt took (`taken`: record times).
/// Returns the turn and the time of the record matched. Reads the last `max_bytes` only.
/// A session with no log file at all counts, as before this rule: nothing can be read, so nothing is held.
/// assume: Copilot writes the record with the text the hook sends. Confirm: `live_autopilot.py` on the laptop finds a
/// record for (nearly) every live prompt. If false: such prompts wait `TURN_WAIT_MS`, then count as before.
pub fn turn_of(
    path: &Path,
    text: &str,
    truncated: bool,
    prompt_ms: i64,
    taken: &[i64],
    max_bytes: u64,
) -> (Turn, Option<i64>) {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Turn::Own, None),
        Err(_) => return (Turn::Unknown, None),
    };
    let Ok(len) = f.metadata().map(|m| m.len()) else { return (Turn::Unknown, None) };
    let start = len.saturating_sub(max_bytes);
    let mut buf = Vec::new();
    if f.seek(SeekFrom::Start(start)).is_err() || f.take(max_bytes).read_to_end(&mut buf).is_err() {
        return (Turn::Unknown, None);
    }
    let want = text.trim();
    let same = |c: &str| c.trim() == want || (truncated && c.trim_start().starts_with(want.trim_end()));
    let mut best: Option<(i64, bool)> = None;
    let lines = String::from_utf8_lossy(&buf);
    for e in lines.lines().skip(usize::from(start > 0)).filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
        if e.get("type").and_then(Value::as_str) != Some("user.message") || !e.get("agentId").is_none_or(Value::is_null)
        {
            continue;
        }
        let d = &e["data"];
        let Some(ts) = ts_ms(&e["timestamp"]) else { continue };
        // The hook removes secrets before it sends the text, so the record's text is compared the same way.
        if taken.contains(&ts) || !same(&aiq_domain::secrets::redact(d["content"].as_str().unwrap_or("")).0) {
            continue;
        }
        if (ts - prompt_ms).abs() <= crate::projects::TURN_MATCH_MS
            && best.is_none_or(|(b, _)| (ts - prompt_ms).abs() < (b - prompt_ms).abs())
        {
            best = Some((ts, d["isAutopilotContinuation"].as_bool() == Some(true)));
        }
    }
    match best {
        Some((ts, true)) => (Turn::Other, Some(ts)),
        Some((ts, false)) => (Turn::Own, Some(ts)),
        None => (Turn::Unknown, None),
    }
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

    /// DC-096: the record says who sent the prompt; the hook does not.
    #[test]
    fn turn_of_reads_who_sent_the_prompt_from_its_record() {
        let d = tempfile::TempDir::new().unwrap();
        let p = d.path().join("events.jsonl");
        let at = |s: u32| format!("2026-11-03T00:00:{s:02}Z");
        let ms = |s: i64| 1_793_664_000_000 + s * 1000;
        let auto = |text: &str, ts: &str| {
            json!({"type":"user.message","timestamp":ts,
                "data":{"content":text,"delivery":"idle","isAutopilotContinuation":true}})
            .to_string()
        };
        let sub = json!({"type":"user.message","agentId":"a1","timestamp":at(20),
            "data":{"content":"keep going","isAutopilotContinuation":false}});
        let body = [
            msg("idle", "fix the login bug", &at(0)),
            auto("keep going", &at(10)),
            sub.to_string(),
            msg("idle", "keep going", &at(30)),
            auto("keep going", &at(50)),
        ]
        .join("\n");
        std::fs::write(&p, body + "\n").unwrap();
        let m = 256 * 1024;
        assert_eq!(turn_of(&p, " fix the login bug\n", false, ms(1), &[], m), (Turn::Own, Some(ms(0))));
        assert_eq!(turn_of(&p, "keep going", false, ms(11), &[], m), (Turn::Other, Some(ms(10))), "autopilot sent it");
        // The person typed the same words: the nearest record decides, and a sub-agent's line never does.
        assert_eq!(turn_of(&p, "keep going", false, ms(21), &[], m), (Turn::Own, Some(ms(30))));
        assert_eq!(turn_of(&p, "keep going", false, ms(31), &[ms(30)], m), (Turn::Other, Some(ms(50))), "taken");
        assert_eq!(turn_of(&p, "fix the login", true, ms(1), &[], m), (Turn::Own, Some(ms(0))), "cut by the hook");
        // Not written yet, or too far from the hook: unknown, so the core waits for it.
        assert_eq!(turn_of(&p, "now the tests", false, ms(60), &[], m), (Turn::Unknown, None));
        assert_eq!(turn_of(&p, "fix the login bug", false, ms(200), &[], m), (Turn::Unknown, None));
        // The hook sends the text with secrets removed; the record still holds them.
        let raw = "deploy with token ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        std::fs::write(&p, msg("idle", raw, &at(5)) + "\n").unwrap();
        let hooked = aiq_domain::secrets::redact(raw).0;
        assert_ne!(hooked, raw);
        assert_eq!(turn_of(&p, &hooked, false, ms(5), &[], m), (Turn::Own, Some(ms(5))));
        // No log at all: nothing to wait for, it counts as before.
        assert_eq!(turn_of(&d.path().join("none.jsonl"), "x", false, ms(0), &[], m), (Turn::Own, None));
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
