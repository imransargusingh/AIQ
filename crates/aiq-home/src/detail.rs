//! Task detail (design aiq-home-deferred-pages E8, E9): one task as the Tasks page lists it, plus the text
//! of its prompts in order. The task comes from the same read as the snapshot, so the two always agree.
use crate::{Snapshot, TaskRow, snapshot};
use aiq_contracts::pack::PackV1;
use aiq_core::paths::Paths;
use aiq_domain::clock::Now;
use aiq_store::Store;
use rusqlite::OptionalExtension;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskDetail {
    pub task: TaskRow,
    pub prompts: Vec<TaskPromptText>,
    /// Measured: how long this read took.
    pub read_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskPromptText {
    pub event_id: String,
    pub ts_ms: Option<i64>,
    /// The person's own prompt text. None when deleted (or never captured).
    pub text: Option<String>,
    /// A live prompt whose text was captured and later deleted.
    pub deleted: bool,
}

/// None when the task is not in the current window (or the store can't be read).
pub fn task_detail(paths: &Paths, pack: &PackV1, now: Now, task_id: &str) -> Option<TaskDetail> {
    let started = std::time::Instant::now();
    let s: Snapshot = snapshot(paths, pack, now);
    let task = s.tasks.into_iter().find(|t| t.task_id == task_id)?;
    let store = Store::open_read_only(&paths.db()).ok()?;
    let c = store.raw();
    let mut prompts = Vec::with_capacity(task.members.len());
    for (id, _) in &task.members {
        let p = match parse_import_id(id) {
            Some((tool, sid, seq)) => c
                .query_row(
                    "SELECT ts_utc_ms, text FROM import_event WHERE tool = ?1 AND source_session_id = ?2 AND seq = ?3",
                    rusqlite::params![tool, sid, seq],
                    |r| Ok(TaskPromptText { event_id: id.clone(), ts_ms: r.get(0)?, text: r.get(1)?, deleted: false }),
                )
                .optional()
                .ok()
                .flatten(),
            None => c
                .query_row(
                    "SELECT e.ts_utc_ms, t.text,
                            t.event_id IS NULL AND coalesce(json_extract(e.payload_meta, '$.has_text'), 0) = 1
                     FROM event e LEFT JOIN content t ON t.event_id = e.event_id WHERE e.event_id = ?1",
                    [id],
                    |r| {
                        Ok(TaskPromptText {
                            event_id: id.clone(),
                            ts_ms: r.get(0)?,
                            text: r.get(1)?,
                            deleted: r.get(2)?,
                        })
                    },
                )
                .optional()
                .ok()
                .flatten(),
        };
        // A row deleted between the two reads (delete everything) is left out, never invented.
        if let Some(p) = p {
            prompts.push(p);
        }
    }
    Some(TaskDetail { task, prompts, read_ms: started.elapsed().as_millis() as u64 })
}

/// `import:{tool}:{source session id}:{seq}`, as the snapshot names imported prompts. Session ids may hold `:`.
fn parse_import_id(id: &str) -> Option<(&str, &str, i64)> {
    let rest = id.strip_prefix("import:")?;
    let (tool, rest) = rest.split_once(':')?;
    let (sid, seq) = rest.rsplit_once(':')?;
    Some((tool, sid, seq.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::parse_import_id;

    #[test]
    fn import_ids_parse_with_colons_in_the_session() {
        assert_eq!(parse_import_id("import:copilot-cli:a:b:7"), Some(("copilot-cli", "a:b", 7)));
        assert_eq!(parse_import_id("0190-live-id"), None);
        assert_eq!(parse_import_id("import:claude-code:s:x"), None);
    }
}
