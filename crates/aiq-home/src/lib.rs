//! AIQ home read model (design `docs/design/aiq-home-app.md`). One `Snapshot` of the person's local store,
//! read in one read-only transaction, so every number on a page agrees. Nothing here writes: changes go
//! through `aiq_core::control`, which talks to the core (the only writer).
//!
//! No rung judge exists yet (Phases 4–5), so `judged` is 0 and every task's `rung` is None. Nothing is
//! guessed: tasks are counted, never judged here.
use aiq_contracts::card::CardV1;
use aiq_contracts::codes;
use aiq_contracts::pack::PackV1;
use aiq_core::paths::Paths;
use aiq_domain::automated::folder_key;
use aiq_domain::clock::{DAY_MS, Now, deliveries_counted_today};
use aiq_domain::reprompt::{TaskPrompt, TaskSpan, split_tasks};
use aiq_domain::tipline::TipStays;
use aiq_store::Store;
use serde::Serialize;
use std::collections::BTreeSet;
use std::time::Instant;

pub const SCHEMA: &str = "aiq.home.v1";
/// Home's window (spec Q3).
pub const WINDOW_DAYS: i64 = 30;
/// Weekly columns on Home.
pub const WEEKS: i64 = 4;
/// Tip history window (spec US-H5).
pub const TIP_DAYS: i64 = 90;
/// Your data lists at most this many newest events; the counts are always exact.
pub const MAX_ITEMS: usize = 500;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub schema: &'static str,
    pub now_ms: i64,
    pub offset_min: i32,
    pub status: Status,
    pub home: Home,
    pub tasks: Vec<TaskRow>,
    pub tips: Vec<TipRow>,
    pub data: Data,
    pub settings: Settings,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    /// The core answered just now. Filled by the caller (`with_core`); false until then.
    pub core: bool,
    pub paused: bool,
    /// A store file exists (false on a fresh install).
    pub store_found: bool,
    /// Newest stored event time, for "this shows what was stored at {time}".
    pub last_event_ms: Option<i64>,
    /// Measured: how long this snapshot took to read.
    pub read_ms: u64,
    /// Set when the store could not be read; the views show their error state with this code.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Home {
    pub window_days: i64,
    /// Tasks whose first prompt is in the window. Measured count.
    pub tasks_in_window: u32,
    /// Judged tasks in the window. Always 0 until the rung judge exists.
    pub judged: u32,
    pub imported_in_window: u32,
    /// Oldest first; the last one ends now.
    pub weeks: Vec<Week>,
    /// (tool, tasks in window), most first.
    pub by_tool: Vec<(String, u32)>,
    pub set_aside: Vec<SetAside>,
    pub tips_today: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Week {
    pub start_ms: i64,
    pub tasks: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetAside {
    pub folder: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskRow {
    pub task_id: String,
    /// claude-code, copilot-cli, vscode
    pub tool: String,
    pub folder: Option<String>,
    pub first_ms: i64,
    pub last_ms: i64,
    pub prompts: u32,
    /// live or imported
    pub origin: &'static str,
    /// None until the job classifier exists (Phase 4).
    pub job: Option<String>,
    /// None until the rung judge exists (Phase 5). 1..=5 when judged.
    pub rung: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TipRow {
    pub card_id: String,
    pub tool: String,
    pub delivered_ms: i64,
    pub noticed: String,
    pub why: String,
    pub try_this: String,
    pub opened: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Data {
    pub events: u64,
    pub prompts: u64,
    pub turn_ends: u64,
    pub imported_sessions: u64,
    pub imported_events: u64,
    pub store_bytes: u64,
    /// Newest first, at most `MAX_ITEMS`.
    pub items: Vec<Item>,
    pub items_capped: bool,
    /// Where the data lives, for "stored only on this computer".
    pub location: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub event_id: String,
    pub ts_ms: i64,
    pub tool: String,
    /// prompt or stop
    pub kind: String,
    /// The person's own prompt text. None for a turn end, or when deleted.
    pub text: Option<String>,
    /// A prompt whose text was captured and later deleted.
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Settings {
    pub daily_cap: u32,
    pub copilot_tip_stays: &'static str,
}

/// Read the whole snapshot. Never fails: an unreadable store gives the empty snapshot with `status.error`.
pub fn snapshot(paths: &Paths, pack: &PackV1, now: Now) -> Snapshot {
    let started = Instant::now();
    let mut s = empty(paths, pack, now);
    s.status.paused = aiq_core::control::is_paused(paths);
    let db = paths.db();
    if db.exists() {
        s.status.store_found = true;
        if let Err(e) = read(&db, pack, now, &mut s) {
            let e2 = empty(paths, pack, now);
            s.home = e2.home;
            s.tasks = e2.tasks;
            s.tips = e2.tips;
            s.data = e2.data;
            s.status.error = Some(format!("{}: {e}", codes::HOME_STORE_READ));
        }
    }
    s.status.read_ms = started.elapsed().as_millis() as u64;
    s
}

/// Fill `status.core` by asking the core, with a short timeout (the window must not wait on it).
pub fn with_core(mut s: Snapshot, paths: &Paths) -> Snapshot {
    use aiq_contracts::frames::Request;
    s.status.core =
        aiq_core::control::ask(paths, std::time::Duration::from_millis(400), |token| Request::Status { token })
            .is_some();
    s
}

fn empty(paths: &Paths, pack: &PackV1, now: Now) -> Snapshot {
    let week0 = now.utc_ms - WEEKS * 7 * DAY_MS;
    Snapshot {
        schema: SCHEMA,
        now_ms: now.utc_ms,
        offset_min: now.offset_min,
        status: Status {
            core: false,
            paused: false,
            store_found: false,
            last_event_ms: None,
            read_ms: 0,
            error: None,
        },
        home: Home {
            window_days: WINDOW_DAYS,
            tasks_in_window: 0,
            judged: 0,
            imported_in_window: 0,
            weeks: (0..WEEKS).map(|i| Week { start_ms: week0 + i * 7 * DAY_MS, tasks: 0 }).collect(),
            by_tool: vec![],
            set_aside: vec![],
            tips_today: 0,
        },
        tasks: vec![],
        tips: vec![],
        data: Data {
            events: 0,
            prompts: 0,
            turn_ends: 0,
            imported_sessions: 0,
            imported_events: 0,
            store_bytes: 0,
            items: vec![],
            items_capped: false,
            location: paths.data.display().to_string(),
        },
        settings: Settings {
            daily_cap: pack.trigger.default_daily_cap,
            copilot_tip_stays: TipStays::NextPrompt.as_str(),
        },
    }
}

type R<T> = Result<T, String>;

fn q<T>(r: rusqlite::Result<T>) -> R<T> {
    r.map_err(|e| e.to_string())
}

fn read(db: &std::path::Path, pack: &PackV1, now: Now, s: &mut Snapshot) -> R<()> {
    let store = Store::open_read_only(db).map_err(|e| e.to_string())?;
    let c = store.raw();
    // One read transaction: every number in the snapshot comes from the same committed state.
    q(c.execute_batch("BEGIN DEFERRED"))?;
    let out = read_in_tx(&store, pack, now, s);
    let _ = c.execute_batch("COMMIT");
    out
}

fn read_in_tx(store: &Store, pack: &PackV1, now: Now, s: &mut Snapshot) -> R<()> {
    let c = store.raw();
    let window_start = now.utc_ms - WINDOW_DAYS * DAY_MS;
    let gap = pack.idle_gap_min as i64 * 60_000;
    let span = &pack.reprompt.span_surfaces;

    // Settings: latest value per key, parsed as the core parses them.
    if store.has_table("setting_change").map_err(|e| e.to_string())? {
        if let Some(cap) = store.setting("daily_cap").ok().flatten().and_then(|v| v.parse().ok()) {
            s.settings.daily_cap = cap;
        }
        if let Some(t) = store.setting("copilot_tip_stays").ok().flatten().and_then(|v| TipStays::parse(&v)) {
            s.settings.copilot_tip_stays = t.as_str();
        }
    }

    // Set-aside folders: never shown as tasks (import spec H4).
    let set_aside: Vec<(String, String, i64)> =
        if store.has_table("folder_rule").map_err(|e| e.to_string())? { store.set_aside_folders().map_err(|e| e.to_string())? } else { vec![] };
    s.home.set_aside = set_aside.iter().map(|(f, r, _)| SetAside { folder: f.clone(), reason: r.clone() }).collect();
    let aside: BTreeSet<&str> = set_aside.iter().map(|(f, _, _)| f.as_str()).collect();

    // Live tasks. Read from one idle gap before the window so a Task that started just before it is whole.
    let mut rows: Vec<(String, String, String, i64, Option<String>)> = Vec::new();
    {
        let mut st = q(c.prepare(
            "SELECT e.event_id, e.surface, e.surface_session_id, e.ts_utc_ms, json_extract(e.payload_meta, '$.folder')
             FROM event e LEFT JOIN content t ON t.event_id = e.event_id
             WHERE e.kind = 'prompt' AND e.ts_utc_ms >= ?1 AND (t.text IS NULL OR trim(t.text) != '')
             ORDER BY e.ts_utc_ms, e.event_id",
        ))?;
        let it = q(st.query_map([window_start - gap], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))))?;
        for row in it {
            rows.push(q(row)?);
        }
    }
    let folder_of: std::collections::BTreeMap<&str, Option<&str>> =
        rows.iter().map(|(id, _, _, _, f)| (id.as_str(), f.as_deref())).collect();
    let live = split_tasks(
        rows.iter().map(|(id, surface, session, ts, _)| TaskPrompt { event_id: id, surface, session, utc_ms: *ts }),
        span,
        gap,
    );
    let mut tasks: Vec<TaskRow> = live
        .into_iter()
        .map(|t| {
            let folder = folder_of.get(t.task_id.as_str()).copied().flatten().map(str::to_owned);
            row(t, folder, "live")
        })
        .collect();

    // Imported tasks (history import, PR #15), when this store has its tables.
    if store.has_table("import_session").map_err(|e| e.to_string())? && store.has_table("import_event").map_err(|e| e.to_string())? {
        let mut st = q(c.prepare(
            "SELECT s.tool, s.source_session_id, s.folder, e.seq, e.ts_utc_ms
             FROM import_event e JOIN import_session s ON s.tool = e.tool AND s.source_session_id = e.source_session_id
             WHERE s.outcome = 'imported' AND e.kind = 'prompt' AND e.ts_utc_ms IS NOT NULL AND e.ts_utc_ms >= ?1
               AND (e.text IS NULL OR trim(e.text) != '')
             ORDER BY e.ts_utc_ms, s.source_session_id, e.seq",
        ))?;
        let it = q(st.query_map([window_start - gap], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
        }))?;
        let mut imp: Vec<(String, String, String, String, i64)> = Vec::new();
        for r in it {
            let (tool, sid, folder, seq, ts) = q(r)?;
            imp.push((format!("import:{tool}:{sid}:{seq}"), tool, sid, folder_key(&folder), ts));
        }
        let folder_of: std::collections::BTreeMap<&str, &str> =
            imp.iter().map(|(id, _, _, f, _)| (id.as_str(), f.as_str())).collect();
        // Imported sessions never span: each source session is its own stream.
        let spans = split_tasks(
            imp.iter().map(|(id, tool, sid, _, ts)| TaskPrompt { event_id: id, surface: tool, session: sid, utc_ms: *ts }),
            &[],
            gap,
        );
        for t in spans {
            let f = folder_of.get(t.task_id.as_str()).map(|f| f.to_string()).filter(|f| !f.is_empty());
            tasks.push(row(t, f, "imported"));
        }
    }

    tasks.retain(|t| t.first_ms >= window_start && !t.folder.as_deref().is_some_and(|f| aside.contains(f)));
    tasks.sort_by(|a, b| b.first_ms.cmp(&a.first_ms).then_with(|| a.task_id.cmp(&b.task_id)));

    s.home.tasks_in_window = tasks.len() as u32;
    s.home.imported_in_window = tasks.iter().filter(|t| t.origin == "imported").count() as u32;
    for w in s.home.weeks.iter_mut() {
        let end = w.start_ms + 7 * DAY_MS;
        w.tasks = tasks.iter().filter(|t| t.first_ms >= w.start_ms && t.first_ms < end).count() as u32;
    }
    let mut by: std::collections::BTreeMap<&str, u32> = Default::default();
    for t in &tasks {
        *by.entry(t.tool.as_str()).or_default() += 1;
    }
    let mut by: Vec<(String, u32)> = by.into_iter().map(|(k, n)| (k.to_owned(), n)).collect();
    by.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    s.home.by_tool = by;
    s.tasks = tasks;

    // Tips: delivered cards in the last 90 days, newest first; tips today counted as the trigger engine counts.
    s.home.tips_today = deliveries_counted_today(store.delivery_days().map_err(|e| e.to_string())?, now.local_day());
    {
        let has_open = store.has_table("card_open").map_err(|e| e.to_string())?;
        let opened = if has_open { "EXISTS (SELECT 1 FROM card_open o WHERE o.card_id = d.card_id)" } else { "0" };
        let sql = format!(
            "SELECT d.card_id, d.surface, d.delivered_utc_ms,
                    (SELECT x.card_json FROM card_decision x WHERE x.card_id = d.card_id ORDER BY x.rowid DESC LIMIT 1),
                    {opened}
             FROM card_delivery d WHERE d.delivered_utc_ms >= ?1 ORDER BY d.delivered_utc_ms DESC, d.card_id"
        );
        let mut st = q(c.prepare(&sql))?;
        let it = q(st.query_map([now.utc_ms - TIP_DAYS * DAY_MS], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, bool>(4)?))
        }))?;
        for r in it {
            let (card_id, tool, at, json, opened) = q(r)?;
            let card = json.and_then(|j| serde_json::from_str::<CardV1>(&j).ok());
            let (noticed, why, try_this) = match card {
                Some(c) => (c.noticed, c.why, c.try_this),
                None => (String::new(), String::new(), String::new()),
            };
            s.tips.push(TipRow { card_id, tool, delivered_ms: at, noticed, why, try_this, opened });
        }
    }

    // Your data: exact counts, newest items.
    for (kind, n) in store.events_by_kind().map_err(|e| e.to_string())? {
        s.data.events += n;
        match kind.as_str() {
            "prompt" => s.data.prompts += n,
            "stop" => s.data.turn_ends += n,
            _ => {}
        }
    }
    s.data.store_bytes = store.size_bytes();
    s.status.last_event_ms = q(c.query_row("SELECT max(ts_utc_ms) FROM event", [], |r| r.get(0)))?;
    {
        let mut st = q(c.prepare(
            "SELECT e.event_id, e.ts_utc_ms, e.surface, e.kind, t.text,
                    e.kind = 'prompt' AND t.event_id IS NULL AND coalesce(json_extract(e.payload_meta, '$.has_text'), 0) = 1
             FROM event e LEFT JOIN content t ON t.event_id = e.event_id
             ORDER BY e.ts_utc_ms DESC, e.event_id DESC LIMIT ?1",
        ))?;
        let it = q(st.query_map([MAX_ITEMS as i64 + 1], |r| {
            Ok(Item {
                event_id: r.get(0)?,
                ts_ms: r.get(1)?,
                tool: r.get(2)?,
                kind: r.get(3)?,
                text: r.get(4)?,
                deleted: r.get(5)?,
            })
        }))?;
        for i in it {
            s.data.items.push(q(i)?);
        }
        if s.data.items.len() > MAX_ITEMS {
            s.data.items.truncate(MAX_ITEMS);
            s.data.items_capped = true;
        }
    }
    if store.has_table("import_session").map_err(|e| e.to_string())? {
        s.data.imported_sessions =
            q(c.query_row("SELECT count(*) FROM import_session WHERE outcome = 'imported'", [], |r| r.get::<_, i64>(0)))? as u64;
    }
    if store.has_table("import_event").map_err(|e| e.to_string())? {
        s.data.imported_events = q(c.query_row("SELECT count(*) FROM import_event", [], |r| r.get::<_, i64>(0)))? as u64;
    }
    Ok(())
}

fn row(t: TaskSpan, folder: Option<String>, origin: &'static str) -> TaskRow {
    TaskRow {
        task_id: t.task_id,
        tool: t.surface,
        folder,
        first_ms: t.first_ms,
        last_ms: t.last_ms,
        prompts: t.prompts,
        origin,
        job: None,
        rung: None,
    }
}
