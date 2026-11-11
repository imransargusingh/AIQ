//! Task detail (design aiq-home-deferred-pages E8, E9): one task as the Tasks page lists it, plus the text
//! of its prompts in order. The task comes from the same read as the snapshot, so the two always agree.
use crate::{Analysis, Blocked, Retry, Snapshot, TaskRow, analysis_provider, retries, snapshot};
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
    /// Where this task's analysis stands, so the page says "waiting", "trying again at" or "stopped trying"
    /// instead of one "Not analyzed yet" for all of them (DC-055).
    pub analysis: TaskAnalysis,
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

/// Where one task's analysis stands. Every field is read from the store or judge.log; nothing is forecast.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskAnalysis {
    /// analyzed · off · paused · no_tool · blocked · open · gave_up · retrying · today_done · queued
    pub state: &'static str,
    /// open: when the task counts as finished (quiet for the idle gap).
    pub closes_ms: Option<i64>,
    /// retrying: when AIQ tries again.
    pub next_try_ms: Option<i64>,
    /// Failed tries so far, and when the newest one started.
    pub tries: u32,
    pub last_try_ms: Option<i64>,
    /// The tool's own words on the newest failed try, from judge.log. None when not recorded.
    pub reply: Option<String>,
    /// queued: tasks waiting now that AIQ analyzes first (newer ones).
    pub ahead: u32,
    /// blocked: the tool and why (missing or not_signed_in), as Home's analysis card says it.
    pub blocked: Option<Blocked>,
}

/// Tries before AIQ stops trying a task (`GIVE_UP` failed calls, two calls a try).
pub const TRIES: u32 = (crate::GIVE_UP / 2) as u32;

/// The task's state from the window's tasks, the last week's calls (as the judge's queue reads them) and Home's
/// analysis card. `reply` is the tool's words on the newest failed try.
pub fn task_analysis(
    t: &TaskRow,
    tasks: &[TaskRow],
    calls: &[aiq_store::CallOutcome],
    home: &Analysis,
    now_ms: i64,
    idle_gap_ms: i64,
    reply: Option<String>,
) -> TaskAnalysis {
    let fails = retries(calls, |_| None);
    let r: Retry = fails.get(t.task_id.as_str()).copied().unwrap_or_default();
    let mut a = TaskAnalysis {
        state: "queued",
        closes_ms: None,
        next_try_ms: None,
        tries: r.tries as u32,
        last_try_ms: (r.calls > 0).then_some(r.last_ms),
        reply: if r.calls > 0 { reply } else { None },
        ahead: 0,
        blocked: None,
    };
    let provider = analysis_provider(&t.tool);
    let blocked = home.blocked.as_ref().filter(|b| Some(b.tool.as_str()) == provider);
    a.state = if t.judgment.is_some() {
        "analyzed"
    } else if home.state == "off" {
        "off"
    } else if home.state == "paused" {
        "paused"
    } else if provider.is_none() {
        "no_tool"
    } else if let Some(b) = blocked {
        a.blocked = Some(b.clone());
        "blocked"
    } else if t.last_ms + idle_gap_ms >= now_ms {
        a.closes_ms = Some(t.last_ms + idle_gap_ms);
        "open"
    } else if r.given_up() {
        "gave_up"
    } else if let Some(at) = r.next_try_ms().filter(|at| *at > now_ms) {
        a.next_try_ms = Some(at);
        "retrying"
    } else if home.state == "today_done" {
        "today_done"
    } else {
        "queued"
    };
    if a.state == "queued" {
        // The judge takes waiting tasks newest first, so the ones ahead are the newer waiting ones.
        let due =
            |o: &TaskRow| fails.get(o.task_id.as_str()).is_none_or(|r| r.next_try_ms().is_some_and(|at| at <= now_ms));
        a.ahead = tasks
            .iter()
            .filter(|o| o.task_id != t.task_id && o.first_ms > t.first_ms && o.judgment.is_none())
            .filter(|o| analysis_provider(&o.tool).is_some() && o.last_ms + idle_gap_ms < now_ms && due(o))
            .count() as u32;
    }
    a
}

/// The tool's words on a task's newest failed try: the last element of judge.log's
/// `failed <task> <tool> ["<outcome> <words>", ...]` line, without the outcome word. None when not logged.
pub fn judge_log_reply(log: &str, task_id: &str) -> Option<String> {
    let prefix = format!("failed {task_id} ");
    let line = log.lines().rev().find(|l| l.starts_with(&prefix))?;
    let list = &line[line.find('[')?..];
    let items: Vec<String> = serde_json::from_str(list).ok()?;
    let last = items.last()?.trim();
    let words = last.split_once(' ').map_or("", |(_, w)| w).trim();
    (!words.is_empty()).then(|| words.to_owned())
}

/// None when the task is not in the current window (or the store can't be read).
pub fn task_detail(paths: &Paths, pack: &PackV1, now: Now, task_id: &str) -> Option<TaskDetail> {
    let started = std::time::Instant::now();
    let s: Snapshot = snapshot(paths, pack, now);
    let store = Store::open_read_only(&paths.db()).ok()?;
    let calls = store.judge_calls_since(now.utc_ms - 7 * aiq_domain::clock::DAY_MS).unwrap_or_default();
    let log = std::fs::read_to_string(paths.data.join("judge.log")).unwrap_or_default();
    let analysis = {
        let t = s.tasks.iter().find(|t| t.task_id == task_id)?;
        let idle = pack.idle_gap_min as i64 * 60_000;
        task_analysis(t, &s.tasks, &calls, &s.home.analysis, now.utc_ms, idle, judge_log_reply(&log, task_id))
    };
    let task = s.tasks.into_iter().find(|t| t.task_id == task_id)?;
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
    Some(TaskDetail { task, prompts, analysis, read_ms: started.elapsed().as_millis() as u64 })
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
