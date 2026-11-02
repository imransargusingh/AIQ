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
}
