//! Sessions driven through claude.ai Projects / Remote Control (import spec US-12, rule RC).
//!
//! The Claude desktop app runs Claude Code for a claude.ai Project thread (a Remote Control session) with
//! `CLAUDE_CODE_ENTRYPOINT=claude-desktop`, the same value as a session typed in the app, so the entrypoint
//! cannot tell them apart. The transcript can: every user record Claude Code writes for such a session
//! carries one of the markers below. Laptop measurement 2026-11-02 (Claude Code 2.1.299–2.1.316, 675
//! transcripts): 72 of 72 `turnOrigin=human` desktop records carried `projectsUserTurn: true`; relayed
//! briefs carry `origin.subkind = "projects-relay"`; other Claude sessions' messages carry
//! `turnOrigin = "peer"`; and the session's first record, written before the first prompt's hook fires
//! (64 ms–3.7 s earlier, within the first 4 KB, 11 of 11 live sessions), is a desktop `turnOrigin = "system"`
//! turn (14 of 14 Projects sessions; 0 in 9 terminal and 639 headless transcripts). These are Claude's own sessions working for the person, not the person's
//! hand-typed work, so AIQ sets them aside like other automated sessions; `aiq include <folder>` undoes it.
//! Live, the rule decides per turn (DC-043, 2026-11-08): every desktop session carries the head marker, so a turn the
//! person typed in such a session (on the computer or the phone) still counts; see `turn_at`.
//! assume: Claude Code keeps these field names. Confirm: a Remote Control session after an update shows up
//! as set aside in `aiq status`. If false: those sessions are recorded as tasks again (no data lost).
use serde_json::Value;
use std::io::Read;
use std::path::Path;

/// Bytes of a transcript the core reads once per live Claude Code session (it used to be the hook, but on
/// Imran's laptop just opening the transcript cost ~0.4 s of every prompt, measured 2026-11-02): the marker the hook relies on is the session's first record, measured
/// within the first 4 KB (11 of 11 live sessions); 32 KB leaves room for a long first prompt. Import keeps
/// reading whole transcripts.
/// assume: the first record stays within 32 KB. Confirm: a new Remote Control session shows as set aside in
/// `aiq status`. If false: that live session is recorded as a task, not set aside (no data lost).
pub const HEAD_BYTES: u64 = 32 * 1024;

/// True when this transcript record shows a Projects / Remote Control session.
pub fn is_marker(r: &Value) -> bool {
    r.get("projectsUserTurn").and_then(Value::as_bool) == Some(true)
        || r["origin"]["subkind"].as_str() == Some("projects-relay")
        || r.get("turnOrigin").and_then(Value::as_str) == Some("peer")
        || r["origin"]["kind"].as_str() == Some("peer")
        || (r.get("turnOrigin").and_then(Value::as_str) == Some("system")
            && r.get("entrypoint").and_then(Value::as_str) == Some("claude-desktop"))
}

/// What a transcript's head says about rule RC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
    Projects,
    /// A user record, or a full head, with no marker: the answer will not change.
    NotProjects,
    /// Missing, unreadable, or no user record yet: Claude Code had not written the session's first records.
    /// Laptop 2026-11-02 21:28Z: the first prompt's check found nothing in 0 ms, yet the transcript's
    /// marker record is stamped 51 ms before it; the session was then recorded as the person's work.
    Unknown,
}

/// Read the transcript's head (up to `max_bytes`) for a Projects marker.
pub fn scan(path: &Path, max_bytes: u64) -> Head {
    let Ok(f) = std::fs::File::open(path) else { return Head::Unknown };
    let mut buf = Vec::new();
    if f.take(max_bytes).read_to_end(&mut buf).is_err() {
        return Head::Unknown;
    }
    let text = String::from_utf8_lossy(&buf);
    let mut user = false;
    for r in text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
        if is_marker(&r) {
            return Head::Projects;
        }
        user |= r.get("type").and_then(Value::as_str) == Some("user");
    }
    if user || buf.len() as u64 >= max_bytes { Head::NotProjects } else { Head::Unknown }
}

/// Who typed one turn of a Projects / Remote Control session. Measured on Imran's laptop 2026-11-08 (field names and
/// counts only): a desktop session he drove by hand from the laptop and from the phone app carried the session-head
/// marker, so rule RC set all of it aside, yet 33 of its 40 typed records were his own (`turnOrigin = "human"`,
/// `origin.kind = "human"`, `projectsUserTurn = true`) and 7 were relayed briefs (`origin.subkind = "projects-relay"`).
/// The person's own turns count; Claude's relayed ones do not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// The person typed it (on the computer or through Remote Control).
    Own,
    /// A relayed brief, another Claude's message, or a record with no sign of a person.
    Other,
    /// No typed record near the prompt's time yet (not written, or the file is unreadable).
    Unknown,
}

/// Bytes from the end of a transcript the core reads per prompt of a Projects session to find that prompt's record.
/// simplify: a fixed tail. Ceiling: a turn whose record sits more than 256 KB before the file's end when the core
/// reads it (the record is written before the prompt's hook runs, so normally it is the last lines). Upgrade
/// trigger: `rc.turn` log lines with `turn = "unknown"` for prompts whose session had a Stop.
pub const TAIL_BYTES: u64 = 256 * 1024;

/// How far a typed record's time may sit before the prompt's hook time and still be that prompt's record.
pub const TURN_MATCH_MS: i64 = 60_000;

/// A record that is a typed turn (by the person or relayed), not a tool result, a meta line or a sub-agent's.
fn typed(r: &Value) -> bool {
    r.get("type").and_then(Value::as_str) == Some("user")
        && r.get("isMeta").and_then(Value::as_bool) != Some(true)
        && r.get("isSidechain").and_then(Value::as_bool) != Some(true)
        && (r["message"]["content"].is_string() || r.get("turnOrigin").is_some() || r.get("origin").is_some())
}

/// True when the person typed this record.
pub fn is_own(r: &Value) -> bool {
    r.get("turnOrigin").and_then(Value::as_str) == Some("human")
        && (r.get("projectsUserTurn").and_then(Value::as_bool) == Some(true)
            || r["origin"]["kind"].as_str() == Some("human"))
}

/// The turn of the prompt whose hook ran at `prompt_ms`: the newest typed record stamped at most 2 s after it, if that
/// record is at most `TURN_MATCH_MS` older. Reads the last `max_bytes` only.
pub fn turn_at(path: &Path, prompt_ms: i64, max_bytes: u64) -> Turn {
    use std::io::{Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else { return Turn::Unknown };
    let Ok(len) = f.metadata().map(|m| m.len()) else { return Turn::Unknown };
    let start = len.saturating_sub(max_bytes);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return Turn::Unknown;
    }
    let mut buf = Vec::new();
    if f.take(max_bytes).read_to_end(&mut buf).is_err() {
        return Turn::Unknown;
    }
    let text = String::from_utf8_lossy(&buf);
    let lines = text.lines().skip(usize::from(start > 0)); // a cut first line is not a record
    let mut best: Option<(i64, bool)> = None;
    for r in lines.filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(typed) {
        let Some(ts) = r["timestamp"].as_str().and_then(crate::sysclock::parse_rfc3339_ms) else { continue };
        if ts <= prompt_ms + 2_000 && best.is_none_or(|(b, _)| ts >= b) {
            best = Some((ts, is_own(&r)));
        }
    }
    match best {
        Some((ts, own)) if ts >= prompt_ms - TURN_MATCH_MS => {
            if own {
                Turn::Own
            } else {
                Turn::Other
            }
        }
        _ => Turn::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn markers_are_the_measured_projects_fields_only() {
        assert!(is_marker(&json!({"type":"user","turnOrigin":"human","projectsUserTurn":true})));
        assert!(is_marker(&json!({"type":"user","origin":{"kind":"task-notification","subkind":"projects-relay"}})));
        assert!(is_marker(&json!({"type":"user","turnOrigin":"peer"})));
        assert!(is_marker(&json!({"type":"user","origin":{"kind":"peer"}})));
        assert!(is_marker(&json!({"type":"user","isMeta":true,"turnOrigin":"system","entrypoint":"claude-desktop"})));
        assert!(!is_marker(&json!({"type":"user","isMeta":true,"turnOrigin":"system","entrypoint":"cli"})));
        // A person typing in a terminal, a headless run, a background task's notice: not Projects.
        assert!(!is_marker(&json!({"type":"user","turnOrigin":"human","origin":{"kind":"human"}})));
        assert!(!is_marker(&json!({"type":"user","turnOrigin":"sdk"})));
        assert!(!is_marker(
            &json!({"type":"user","turnOrigin":"task_notification","origin":{"kind":"task-notification"}})
        ));
        assert!(!is_marker(&json!({"type":"user","projectsUserTurn":false})));
    }

    #[test]
    fn scan_reads_only_the_head_and_never_fails_loudly() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("t.jsonl");
        assert_eq!(scan(&p, HEAD_BYTES), Head::Unknown, "missing file");
        let pad = json!({"type":"assistant","message":{"content":"x".repeat(1000)}}).to_string();
        let mark = json!({"type":"user","projectsUserTurn":true}).to_string();
        std::fs::write(&p, format!("{pad}\n{mark}\n")).unwrap();
        assert_eq!(scan(&p, HEAD_BYTES), Head::Projects);
        assert_eq!(scan(&p, 500), Head::NotProjects, "marker past a full head is not read");
        std::fs::write(&p, format!("{pad}\nnot json \"peer\"\n")).unwrap();
        assert_eq!(scan(&p, HEAD_BYTES), Head::Unknown, "no user record yet");
    }

    #[test]
    fn only_a_user_record_or_a_full_head_settles_not_projects() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("t.jsonl");
        // Claude Code 2.1.3xx writes queue records before the session's first user record (laptop, 2026-11-02).
        let queue = json!({"type":"queue-operation","operation":"enqueue"}).to_string();
        std::fs::write(&p, format!("{queue}\n")).unwrap();
        assert_eq!(scan(&p, HEAD_BYTES), Head::Unknown);
        std::fs::write(&p, "").unwrap();
        assert_eq!(scan(&p, HEAD_BYTES), Head::Unknown, "empty file");
        let marker = json!({"type":"user","isMeta":true,"turnOrigin":"system","entrypoint":"claude-desktop"});
        std::fs::write(&p, format!("{queue}\n{marker}\n")).unwrap();
        assert_eq!(scan(&p, HEAD_BYTES), Head::Projects);
        let human = json!({"type":"user","turnOrigin":"human","entrypoint":"cli"});
        std::fs::write(&p, format!("{queue}\n{human}\n")).unwrap();
        assert_eq!(scan(&p, HEAD_BYTES), Head::NotProjects);
    }

    #[test]
    fn a_persons_typed_turn_is_told_from_a_relayed_one_by_its_own_record() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("t.jsonl");
        let at = |s: u32| format!("2026-11-08T22:54:{s:02}.000Z");
        let ms = |s: u32| crate::sysclock::parse_rfc3339_ms(&at(s)).unwrap();
        let head =
            json!({"type":"user","isMeta":true,"turnOrigin":"system","entrypoint":"claude-desktop","timestamp":at(0)});
        // Shaped like the laptop's records (2026-11-08): the person's turn, a relayed brief, a tool result.
        let own = json!({"type":"user","turnOrigin":"human","origin":{"kind":"human"},"projectsUserTurn":true,
            "message":{"content":"fix the login test"},"timestamp":at(10)});
        let relay = json!({"type":"user","turnOrigin":"peer","origin":{"kind":"task-notification","subkind":"projects-relay"},
            "message":{"content":"brief"},"timestamp":at(30)});
        let tool = json!({"type":"user","message":{"content":[{"type":"tool_result"}]},"timestamp":at(40)});
        let rows = [&head, &own, &relay, &tool].map(|r| r.to_string()).join("\n");
        std::fs::write(&p, format!("{rows}\n")).unwrap();
        assert_eq!(turn_at(&p, ms(10), TAIL_BYTES), Turn::Own);
        assert_eq!(turn_at(&p, ms(11), TAIL_BYTES), Turn::Own, "hook a moment after the record");
        assert_eq!(turn_at(&p, ms(31), TAIL_BYTES), Turn::Other, "a relayed brief; the tool result is not a turn");
        assert_eq!(turn_at(&p, ms(10) + TURN_MATCH_MS + 31_000, TAIL_BYTES), Turn::Unknown, "record not written yet");
        assert_eq!(turn_at(&d.path().join("gone.jsonl"), ms(10), TAIL_BYTES), Turn::Unknown);
        // A record with only turnOrigin human and no other sign is not counted as the person's.
        assert!(!is_own(&json!({"type":"user","turnOrigin":"human"})));
        // The tail cut mid-record skips the cut line and still finds the turn.
        let pad = json!({"type":"assistant","message":{"content":"x".repeat(5000)}}).to_string();
        std::fs::write(&p, format!("{pad}\n{own}\n")).unwrap();
        assert_eq!(turn_at(&p, ms(10), own.to_string().len() as u64 + 100), Turn::Own);
    }
}
