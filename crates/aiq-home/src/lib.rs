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

mod detail;
mod twins;
pub use detail::{TaskDetail, TaskPromptText, task_detail};

pub const SCHEMA: &str = "aiq.home.v1";
/// Home's window (spec Q3).
pub const WINDOW_DAYS: i64 = 30;
/// Weekly columns on Home.
pub const WEEKS: i64 = 4;
/// Tip history window (spec US-H5).
pub const TIP_DAYS: i64 = 90;
/// Your data lists at most this many newest events; the counts are always exact.
pub const MAX_ITEMS: usize = 500;
/// Shown under "Tasks agents finished" (Imran 2026-11-02, Q-A: until AIQ can measure your own edits).
pub const FINISHED_RULE: &str =
    "A task counts as finished when it was judged Delegated or higher and you sent no re-prompt correcting the agent.";
/// Home shows at most this many kinds of task with a next step (aiq-home US-H3).
pub const OPPORTUNITY_ROWS: usize = 3;
/// A project's name the person chose: setting `project_name.<folder key>` (design project-filter P2).
pub const PROJECT_NAME: &str = aiq_core::service::SETTING_PROJECT_NAME;
/// The project of tasks with no folder recorded (design project-filter P1).
pub const NO_FOLDER: &str = "Folder not recorded";

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
    pub setup: Setup,
    /// Coaching from rung results (design coaching-from-rungs): every value derived from judgments here.
    pub coaching: Coaching,
    /// Auto-update (design auto-update U5): the switch and the one line Home shows.
    pub update: aiq_core::update::View,
    /// One row per project with a task in the window, most room to grow first (design project-filter).
    pub projects: Vec<Project>,
}

/// One project: the tasks whose session ran in one folder (design project-filter P1–P5b). Its numbers run the
/// same rules as Home's over this project's tasks only (P5).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Project {
    /// The folder key the tasks carry (`TaskRow.folder`); "" for tasks with no folder recorded.
    pub key: String,
    pub name: String,
    /// The person chose the name (setting `project_name.<key>`); false when AIQ named it from the folder.
    pub renamed: bool,
    pub tasks_in_window: u32,
    pub judged: u32,
    pub uncertain: u32,
    pub rungs: [u32; 5],
    pub most_common_rung: Option<u8>,
    pub imported_in_window: u32,
    pub weeks: Vec<Week>,
    pub by_tool: Vec<(String, u32)>,
    pub coaching: Coaching,
}

/// Home's split, the kinds of task, the habits (design coaching-from-rungs D3, D6, D9; aiq-home H2–H4).
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Coaching {
    pub split: SplitOut,
    /// The split shows with at least this many tasks (US-H2).
    pub split_min: u32,
    /// The rule under the headline (Imran 2026-11-02, Q-A).
    pub rule: &'static str,
    /// Oldest first, same weeks as `home.weeks`.
    pub weeks: Vec<WeekSplit>,
    pub week_min: u32,
    /// Home's rows: at most 3 kinds of task with a next step, ranked (US-H3).
    pub opportunities: Vec<KindRow>,
    /// Every kind of task with judged tasks in the window, most tasks first.
    pub kinds: Vec<KindRow>,
    pub habits: Vec<HabitRow>,
    /// Some habit observations come from imported history (LINDDUN "unawareness" footnote).
    pub habits_imported: bool,
    /// Judged tasks in the window whose kind is not known yet (judged before kinds existed).
    pub untyped: u32,
    /// Each kind of task's typical rung, last `progress_days` against the `progress_days` before (the success
    /// measure: a recurring kind moves up a rung). Moved up first.
    pub progress: Vec<ProgressRow>,
    pub progress_days: u32,
    /// Judged, not-Uncertain tasks each half needs before AIQ says a kind moved (the tips' repeated-kind rule).
    pub progress_min: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProgressRow {
    pub job: String,
    pub task_type: String,
    pub label: &'static str,
    pub recent: u32,
    pub earlier: u32,
    pub uncertain: u32,
    pub recent_rung: Option<u8>,
    pub earlier_rung: Option<u8>,
    /// up, down, same, too_few
    pub trend: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
pub struct SplitOut {
    pub tasks: u32,
    pub finished: u32,
    pub could: u32,
    pub needs: u32,
    pub uncertain: u32,
    /// Not finished, with a next step at least one rung up (aiq-domain coach::Split::up).
    pub up: u32,
    /// The rung those next steps reach most often.
    pub up_to: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WeekSplit {
    pub start_ms: i64,
    pub tasks: u32,
    pub finished: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MoveOut {
    pub move_id: String,
    pub habit: String,
    pub why: String,
    pub try_this: String,
    pub starter: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindRow {
    pub job: String,
    pub task_type: String,
    pub label: &'static str,
    pub tasks: u32,
    pub uncertain: u32,
    pub per_week: f64,
    pub today: Option<u8>,
    pub potential: Option<u8>,
    /// coach, too_few, right_rung, blocked, not_sure, flagged
    pub state: &'static str,
    /// The move coached (state coach) or flagged (state flagged).
    pub next: Option<MoveOut>,
    /// Two "Not helpful" answers in 30 days: ranked last.
    pub ranked_last: bool,
    /// The newest task of this kind, and the judge's own words about it (plain text, TR-5).
    pub last_task_id: Option<String>,
    pub last_gap: Option<String>,
    pub task_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HabitRow {
    pub job: String,
    pub move_id: String,
    pub name: String,
    /// not_yet, tried, adopted
    pub state: &'static str,
    pub eligible: u32,
    pub seen: u32,
    pub moved_to: Option<&'static str>,
    pub moved_ms: Option<i64>,
    /// The move was back from Adopted to Tried.
    pub moved_back: bool,
}

/// First-run setup (design aiq-home-deferred-pages E5–E7).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Setup {
    /// The person finished setup once; AIQ home then opens on Home.
    pub done: bool,
    pub tools: Vec<aiq_core::tools::ToolFound>,
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
    /// Measured: the part of `read_ms` before the queries run: the pause check, opening the store and its read lock. Logged by the
    /// shell (home.log), not sent to the page; it tells a slow open or lock wait apart from slow queries.
    #[serde(skip)]
    pub open_ms: u64,
    /// Set when the store could not be read; the views show their error state with this code.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Home {
    pub window_days: i64,
    /// Tasks whose first prompt is in the window. Measured count.
    pub tasks_in_window: u32,
    /// Tasks in the window with a judgment. Measured count.
    pub judged: u32,
    /// Of those, judged Uncertain (shown, never coached, not in `rungs`).
    pub uncertain: u32,
    /// Judged, not-Uncertain tasks in the window per current rung (index 0 = Assisted).
    pub rungs: [u32; 5],
    /// The rung with the most judged tasks in the window (ties go to the lower rung). None when none.
    pub most_common_rung: Option<u8>,
    /// Judging today: tasks sent, calls, cost where the tool reported it.
    pub judge_today: JudgeToday,
    pub imported_in_window: u32,
    /// Oldest first; the last one ends now.
    pub weeks: Vec<Week>,
    /// (tool, tasks in window), most first.
    pub by_tool: Vec<(String, u32)>,
    pub set_aside: Vec<SetAside>,
    pub tips_today: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JudgeToday {
    pub tasks: u32,
    pub calls: u32,
    pub failed_calls: u32,
    /// Sum of the cost the tool reported. None when no call reported one ("not recorded", never a guess).
    pub cost_usd: Option<f64>,
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
    /// The judged job (code_change, write_doc, ask_research, run_ship, unclassified); None until judged.
    pub job: Option<String>,
    /// Current rung 1..=5; None until judged.
    pub rung: Option<u8>,
    /// The current judgment; None until judged.
    pub judgment: Option<TaskJudgment>,
    /// Session of the first prompt (the judge finds the tool's transcript by it). The Tasks page's Day view
    /// draws one lane per session (design day-view D2).
    pub session: String,
    /// Keys and times of the task's prompts, in order. The page gets the times only, as `prompt_ms` (Day view
    /// ticks); prompt ids never leave the read model.
    #[serde(rename = "prompt_ms", serialize_with = "times_only")]
    pub members: Vec<(String, i64)>,
}

/// One task's current judgment as the views show it. Model-written text is plain text (TR-5).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskJudgment {
    pub judged_ms: i64,
    /// judged, uncertain, right_rung, blocked
    pub state: String,
    pub current: u8,
    pub potential: u8,
    pub ceiling: u8,
    pub current_confidence: f64,
    pub potential_confidence: f64,
    pub job: String,
    /// The kind of task within the job; None when judged before kinds existed.
    pub task_type: Option<String>,
    pub task_type_label: Option<&'static str>,
    pub coachable: bool,
    pub evidence: Vec<String>,
    pub gap: String,
    pub blockers: Vec<String>,
    /// claude-code or copilot-cli: the person's own account that read the task.
    pub provider: String,
    pub model: Option<String>,
    /// "What was read?": how many prompts and characters were sent.
    pub read_prompts: u32,
    pub read_chars: u32,
    pub read_actions: bool,
    pub versions: String,
}

fn times_only<S: serde::Serializer>(m: &[(String, i64)], s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq(m.iter().map(|(_, t)| t))
}

fn json_strings(s: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
}

impl TaskRow {
    fn attach(&mut self, j: &aiq_store::JudgmentRow) {
        let read: serde_json::Value = serde_json::from_str(&j.read).unwrap_or_default();
        let n = |k: &str| read.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
        self.job = Some(j.job.clone());
        self.rung = Some(j.current_rung);
        self.judgment = Some(TaskJudgment {
            judged_ms: j.judged_utc_ms,
            state: j.state.clone(),
            current: j.current_rung,
            potential: j.potential_rung,
            ceiling: j.ceiling_rung,
            current_confidence: j.current_confidence,
            potential_confidence: j.potential_confidence,
            job: j.job.clone(),
            task_type: j.task_type.clone(),
            task_type_label: j.task_type.as_deref().and_then(|t| aiq_contracts::judge::task_type_label(&j.job, t)),
            coachable: j.coachable,
            evidence: json_strings(&j.evidence),
            gap: j.gap.clone(),
            blockers: json_strings(&j.blockers),
            provider: j.provider.clone(),
            model: j.model.clone(),
            read_prompts: n("prompts"),
            read_chars: n("chars"),
            read_actions: read.get("actions").and_then(serde_json::Value::as_bool).unwrap_or(false),
            versions: format!("{} · {} · {}", j.fact_set_version, j.rubric_version, j.judge_version),
        });
    }
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
    /// Where tips show; each true unless the person set it "off" (core `SWITCH_SETTINGS`).
    pub copilot_tip_in_chat: bool,
    pub tip_popup: bool,
    pub tip_highlight: bool,
    /// auto (judge new tasks in the background) or off. "off" until `aiq install` connects a tool or the person first runs `aiq judge`.
    pub judge: String,
    pub judge_daily_tasks: u32,
    /// on or off: read Copilot's own history file for messages sent while the agent works (no hook fires).
    pub copilot_mid_turn: String,
    /// on or off: read the tool's own log at each turn's end and store what the agent did, as counts.
    pub agent_actions: String,
    /// auto, on or off.
    pub next_rung_tips: String,
    /// Next-rung tips may show now (off by the setting, or by AIQ's self-check under auto).
    pub next_rung_on: bool,
    /// AIQ's self-check of its kind-of-task guesses (D10). Measured counts.
    pub guess_checked: u32,
    pub guess_right: u32,
    pub guess_auto_off: bool,
    /// Tip ids the person turned off ("Don't show this again"), for "Turn back on".
    pub tips_off: Vec<String>,
    /// Every next-step tip the pack can show, in pack order, for Settings' on/off list.
    pub next_tips: Vec<NextTip>,
    /// Jobs and kinds of task whose next-step tips the accuracy check paused (design K6b), still-paused
    /// first, then the ones the person kept on. Same rule and words as the core's status.
    pub accuracy_paused: Vec<aiq_contracts::frames::PausedScope>,
}

/// One next-step tip as Settings lists it: its tip id, the habit it builds, and its job (or `any`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NextTip {
    pub tip_id: String,
    pub habit: String,
    pub job: String,
}

/// Read the whole snapshot. Never fails: an unreadable store gives the empty snapshot with `status.error`.
pub fn snapshot(paths: &Paths, pack: &PackV1, now: Now) -> Snapshot {
    let started = Instant::now();
    let mut s = empty(paths, pack, now);
    s.status.paused = aiq_core::control::is_paused(paths);
    s.setup = Setup { done: aiq_core::control::setup_done(paths), tools: aiq_core::tools::found() };
    let db = paths.db();
    if db.exists() {
        s.status.store_found = true;
        if let Err(e) = read(&db, pack, now, started, &mut s) {
            let e2 = empty(paths, pack, now);
            s.home = e2.home;
            s.tasks = e2.tasks;
            s.tips = e2.tips;
            s.data = e2.data;
            s.coaching = e2.coaching;
            s.projects = e2.projects;
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
            open_ms: 0,
            error: None,
        },
        home: Home {
            window_days: WINDOW_DAYS,
            tasks_in_window: 0,
            judged: 0,
            uncertain: 0,
            rungs: [0; 5],
            most_common_rung: None,
            judge_today: JudgeToday { tasks: 0, calls: 0, failed_calls: 0, cost_usd: None },
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
            copilot_tip_in_chat: true,
            tip_popup: true,
            tip_highlight: true,
            judge: "off".into(),
            judge_daily_tasks: aiq_domain::rung::DEFAULT_DAILY_TASKS,
            copilot_mid_turn: "on".into(),
            agent_actions: "on".into(),
            next_rung_tips: "auto".into(),
            next_rung_on: true,
            guess_checked: 0,
            guess_right: 0,
            guess_auto_off: false,
            tips_off: vec![],
            next_tips: pack
                .moves
                .iter()
                .map(|m| NextTip {
                    tip_id: aiq_domain::coach::tip_id(&m.move_id),
                    habit: m.habit.clone(),
                    job: m.job.clone(),
                })
                .collect(),
            accuracy_paused: vec![],
        },
        setup: Setup { done: false, tools: vec![] },
        coaching: Coaching {
            split_min: pack.coach.split_min,
            week_min: pack.coach.week_min,
            progress_days: aiq_domain::coach::PROGRESS_DAYS,
            progress_min: pack.coach.recurring_min,
            rule: FINISHED_RULE,
            weeks: (0..WEEKS).map(|i| WeekSplit { start_ms: week0 + i * 7 * DAY_MS, tasks: 0, finished: 0 }).collect(),
            ..Default::default()
        },
        update: aiq_core::update::view(paths, None, now.utc_ms),
        projects: vec![],
    }
}

type R<T> = Result<T, String>;

fn q<T>(r: rusqlite::Result<T>) -> R<T> {
    r.map_err(|e| e.to_string())
}

fn read(db: &std::path::Path, pack: &PackV1, now: Now, started: Instant, s: &mut Snapshot) -> R<()> {
    let store = Store::open_read_only(db).map_err(|e| e.to_string())?;
    let c = store.raw();
    // One read transaction: every number in the snapshot comes from the same committed state.
    q(c.execute_batch("BEGIN DEFERRED"))?;
    // The first read takes the read lock; time to here is open + lock wait (IO1: measured, not guessed).
    q(c.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)))?;
    s.status.open_ms = started.elapsed().as_millis() as u64;
    let out = read_in_tx(&store, pack, now, s);
    let _ = c.execute_batch("COMMIT");
    out
}

fn read_in_tx(store: &Store, pack: &PackV1, now: Now, s: &mut Snapshot) -> R<()> {
    let c = store.raw();
    let window_start = now.utc_ms - WINDOW_DAYS * DAY_MS;

    // Settings: latest value per key, parsed as the core parses them.
    if store.has_table("setting_change").map_err(|e| e.to_string())? {
        if let Some(cap) = store.setting("daily_cap").ok().flatten().and_then(|v| v.parse().ok()) {
            s.settings.daily_cap = cap;
        }
        if let Some(t) = store.setting("copilot_tip_stays").ok().flatten().and_then(|v| TipStays::parse(&v)) {
            s.settings.copilot_tip_stays = t.as_str();
        }
        let off = |k: &str| store.setting(k).ok().flatten().as_deref() == Some("off");
        s.settings.copilot_tip_in_chat = !off(aiq_core::service::SETTING_COPILOT_TIP_IN_CHAT);
        s.settings.tip_popup = !off(aiq_core::service::SETTING_TIP_POPUP);
        s.settings.tip_highlight = !off(aiq_core::service::SETTING_TIP_HIGHLIGHT);
        if let Some(m) = store.setting(aiq_core::update::SETTING_UPDATE).ok().flatten()
            && aiq_core::update::validate_mode(&m).is_ok()
        {
            s.update.mode = m;
        }
        if let Some(v) = store.setting("judge").ok().flatten() {
            s.settings.judge = v;
        }
        if let Some(n) = store.setting("judge_daily_tasks").ok().flatten().and_then(|v| v.parse().ok()) {
            s.settings.judge_daily_tasks = n;
        }
        if let Some(v) = store.setting(aiq_core::service::SETTING_COPILOT_MID_TURN).ok().flatten() {
            s.settings.copilot_mid_turn = v;
        }
        if let Some(v) = store.setting(aiq_core::service::SETTING_AGENT_ACTIONS).ok().flatten() {
            s.settings.agent_actions = v;
        }
    }

    // Set-aside folders: never shown as tasks (import spec H4).
    let set_aside: Vec<(String, String, i64)> = if store.has_table("folder_rule").map_err(|e| e.to_string())? {
        store.set_aside_folders().map_err(|e| e.to_string())?
    } else {
        vec![]
    };
    s.home.set_aside = set_aside.iter().map(|(f, r, _)| SetAside { folder: f.clone(), reason: r.clone() }).collect();
    let aside: BTreeSet<&str> = set_aside.iter().map(|(f, _, _)| f.as_str()).collect();

    let mut tasks = task_list(store, pack, window_start, &aside)?;
    let judgments = store.current_judgments().map_err(|e| e.to_string())?;
    for t in tasks.iter_mut() {
        if let Some(j) = judgments.get(&t.task_id) {
            t.attach(j);
        }
    }
    let all = tally(tasks.iter(), &s.home.weeks);
    let h = &mut s.home;
    (h.tasks_in_window, h.judged, h.uncertain, h.rungs, h.most_common_rung) =
        (all.tasks, all.judged, all.uncertain, all.rungs, all.most_common_rung);
    (h.imported_in_window, h.weeks, h.by_tool) = (all.imported, all.weeks, all.by_tool);
    {
        let calls = store.judge_calls_since(now.local_midnight_utc_ms()).map_err(|e| e.to_string())?;
        let t = &mut s.home.judge_today;
        t.calls = calls.len() as u32;
        t.failed_calls = calls.iter().filter(|c| c.outcome != "ok").count() as u32;
        t.tasks = calls.iter().map(|c| c.task_id.as_str()).collect::<BTreeSet<_>>().len() as u32;
        let costs: Vec<f64> = calls.iter().filter_map(|c| c.cost_usd).collect();
        t.cost_usd = (!costs.is_empty()).then(|| costs.iter().sum());
    }
    coaching(store, pack, now, &tasks, &judgments, s)?;
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
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, bool>(4)?,
            ))
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
            q(c.query_row("SELECT count(*) FROM import_session WHERE outcome = 'imported'", [], |r| {
                r.get::<_, i64>(0)
            }))? as u64;
    }
    if store.has_table("import_event").map_err(|e| e.to_string())? {
        s.data.imported_events =
            q(c.query_row("SELECT count(*) FROM import_event", [], |r| r.get::<_, i64>(0)))? as u64;
    }
    Ok(())
}

/// Every task whose first prompt is at or after `since_ms`, live and imported, newest first. A session's own
/// verdict decides, not its folder's: an imported session counts when the import kept it, and a live session in a
/// set-aside folder counts when a person sent it two or more prompts (the rule's H ≥ 2). The one-prompt live
/// sessions there are left out (the app runs recorded before the folder rule fired). The one Task list both AIQ home and the rung judge read, so their task ids always agree.
pub fn tasks(store: &Store, pack: &PackV1, since_ms: i64) -> Result<Vec<TaskRow>, String> {
    let aside: BTreeSet<String> = if store.has_table("folder_rule").map_err(|e| e.to_string())? {
        store.set_aside_folders().map_err(|e| e.to_string())?.into_iter().map(|(f, _, _)| f).collect()
    } else {
        BTreeSet::new()
    };
    let aside: BTreeSet<&str> = aside.iter().map(String::as_str).collect();
    task_list(store, pack, since_ms, &aside)
}

/// A live prompt: event id, surface, session, time, folder, text.
type LiveRow = (String, String, String, i64, Option<String>, Option<String>);
/// An imported prompt: key, tool, session, folder key, time, text.
type ImportRow = (String, String, String, String, i64, Option<String>);

fn task_list(store: &Store, pack: &PackV1, window_start: i64, aside: &BTreeSet<&str>) -> R<Vec<TaskRow>> {
    let c = store.raw();
    let gap = pack.idle_gap_min as i64 * 60_000;
    let span = &pack.reprompt.span_surfaces;
    // Live prompts. Read from one idle gap before the window so a Task that started just before it is whole.
    let mut rows: Vec<LiveRow> = Vec::new();
    {
        let mut st = q(c.prepare(
            "SELECT e.event_id, e.surface, e.surface_session_id, e.ts_utc_ms, json_extract(e.payload_meta, '$.folder'),
                    t.text
             FROM event e LEFT JOIN content t ON t.event_id = e.event_id
             WHERE e.kind = 'prompt' AND e.ts_utc_ms >= ?1 AND (t.text IS NULL OR trim(t.text) != '')
             ORDER BY e.ts_utc_ms, e.event_id",
        ))?;
        let it = q(st.query_map([window_start - gap], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        }))?;
        for row in it {
            rows.push(q(row)?);
        }
    }
    // Imported prompts (`aiq import`, schema v4). A store from before v4 has no import tables.
    let mut imp: Vec<ImportRow> = Vec::new();
    if store.has_table("import_session").map_err(|e| e.to_string())?
        && store.has_table("import_event").map_err(|e| e.to_string())?
    {
        let mut st = q(c.prepare(
            "SELECT s.tool, s.source_session_id, s.folder, e.seq, e.ts_utc_ms, e.text
             FROM import_event e JOIN import_session s ON s.tool = e.tool AND s.source_session_id = e.source_session_id
             WHERE s.outcome = 'imported' AND e.kind = 'prompt' AND e.ts_utc_ms IS NOT NULL AND e.ts_utc_ms >= ?1
               AND (e.text IS NULL OR trim(e.text) != '')
             ORDER BY e.ts_utc_ms, s.source_session_id, e.seq",
        ))?;
        let it = q(st.query_map([window_start - gap], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        }))?;
        for r in it {
            let (tool, sid, folder, seq, ts, text) = q(r)?;
            imp.push((format!("import:{tool}:{sid}:{seq}"), tool, sid, folder_key(&folder), ts, text));
        }
    }
    // A session both captured live and imported is one session: its imported prompts that have a live twin are
    // left out, and the rest join the live stream (twins.rs).
    let fates = twins::fates(
        &rows
            .iter()
            .map(|(_, s, sid, ts, _, t)| twins::Copy { tool: s, session: sid, utc_ms: *ts, text: t.as_deref() })
            .collect::<Vec<_>>(),
        &imp.iter()
            .map(|(_, tool, sid, _, ts, t)| twins::Copy { tool, session: sid, utc_ms: *ts, text: t.as_deref() })
            .collect::<Vec<_>>(),
    );
    // Live stream: (event id, surface, session, time, folder).
    let mut stream: Vec<(&str, &str, &str, i64, Option<&str>)> =
        rows.iter().map(|(id, s, sid, ts, f, _)| (id.as_str(), s.as_str(), sid.as_str(), *ts, f.as_deref())).collect();
    let mut alone: Vec<&ImportRow> = Vec::new();
    for (p, fate) in imp.iter().zip(&fates) {
        match *fate {
            twins::Fate::Alone => alone.push(p),
            twins::Fate::Twin => {}
            twins::Fate::Extra(j) => {
                let f = rows[j].4.as_deref().or(Some(p.3.as_str()).filter(|f| !f.is_empty()));
                stream.push((p.0.as_str(), rows[j].1.as_str(), rows[j].2.as_str(), p.4, f));
            }
        }
    }
    stream.sort_by(|a, b| a.3.cmp(&b.3).then_with(|| a.0.cmp(b.0)));

    let folder_of: std::collections::BTreeMap<&str, Option<&str>> =
        stream.iter().map(|(id, _, _, _, f)| (*id, *f)).collect();
    let mut prompts_in: std::collections::BTreeMap<(&str, &str), u32> = std::collections::BTreeMap::new();
    for (_, surface, session, _, _) in &stream {
        *prompts_in.entry((surface, session)).or_default() += 1;
    }
    // Live tasks of one-prompt sessions in set-aside folders: (surface, session) of their first prompt.
    let session_of: std::collections::BTreeMap<&str, (&str, &str)> =
        stream.iter().map(|(id, surface, session, _, _)| (*id, (*surface, *session))).collect();
    let live = split_tasks(
        stream.iter().map(|(id, surface, session, ts, _)| TaskPrompt { event_id: id, surface, session, utc_ms: *ts }),
        span,
        gap,
    );
    let mut tasks: Vec<TaskRow> = live
        .into_iter()
        .filter(|t| {
            let in_aside = folder_of.get(t.task_id.as_str()).copied().flatten().is_some_and(|f| aside.contains(f));
            !in_aside
                || session_of.get(t.task_id.as_str()).is_some_and(|k| prompts_in.get(k).copied().unwrap_or(0) >= 2)
        })
        .map(|t| {
            let folder = folder_of.get(t.task_id.as_str()).copied().flatten().map(str::to_owned);
            row(t, folder, "live")
        })
        .collect();

    // Imported-only sessions never span: each source session is its own stream.
    let folder_of: std::collections::BTreeMap<&str, &str> =
        alone.iter().map(|(id, _, _, f, _, _)| (id.as_str(), f.as_str())).collect();
    let spans = split_tasks(
        alone.iter().map(|(id, tool, sid, _, ts, _)| TaskPrompt {
            event_id: id,
            surface: tool,
            session: sid,
            utc_ms: *ts,
        }),
        &[],
        gap,
    );
    for t in spans {
        let f = folder_of.get(t.task_id.as_str()).map(|f| f.to_string()).filter(|f| !f.is_empty());
        tasks.push(row(t, f, "imported"));
    }

    tasks.retain(|t| t.first_ms >= window_start);
    tasks.sort_by(|a, b| b.first_ms.cmp(&a.first_ms).then_with(|| a.task_id.cmp(&b.task_id)));

    Ok(tasks)
}

fn row(t: TaskSpan, folder: Option<String>, origin: &'static str) -> TaskRow {
    TaskRow {
        task_id: t.task_id,
        tool: t.surface,
        // An imported Copilot CLI session with no `cwd` carries "(none)": no folder (design project-filter P1).
        folder: folder.filter(|f| !f.is_empty() && f != "(none)"),
        first_ms: t.first_ms,
        last_ms: t.last_ms,
        prompts: t.prompts,
        origin,
        job: None,
        rung: None,
        judgment: None,
        session: t.session,
        members: t.members,
    }
}

/// The coaching numbers over the same task list and judgments as the rest of the snapshot.
fn coaching(
    store: &Store,
    pack: &PackV1,
    now: Now,
    tasks: &[TaskRow],
    judgments: &std::collections::HashMap<String, aiq_store::JudgmentRow>,
    s: &mut Snapshot,
) -> R<()> {
    use aiq_domain::coach;
    let e = |e: aiq_store::StoreError| e.to_string();
    let feedback = aiq_core::coaching::feedback(store).map_err(e)?;
    let flags = aiq_core::coaching::flags(store).map_err(e)?;
    let all: Vec<&TaskRow> = tasks.iter().collect();
    let judged = judged_tasks(&all, judgments);
    coach_view(pack, now, &judged, judgments, &feedback, &flags, &mut s.coaching);

    // Projects (design project-filter): the same tally and coaching over each folder's tasks.
    let names = project_names(
        tasks.iter().map(|t| t.folder.as_deref().unwrap_or("")),
        &store.settings_with_prefix(PROJECT_NAME).map_err(e)?,
    );
    let mut by_project: std::collections::BTreeMap<&str, Vec<&TaskRow>> = Default::default();
    for t in tasks {
        by_project.entry(t.folder.as_deref().unwrap_or("")).or_default().push(t);
    }
    let blank = s.coaching.clone();
    let mut projects: Vec<Project> = by_project
        .into_iter()
        .map(|(key, ts)| {
            let t = tally(ts.iter().copied(), &s.home.weeks);
            let pj = judged_tasks(&ts, judgments);
            let mut c = Coaching {
                split_min: blank.split_min,
                week_min: blank.week_min,
                rule: blank.rule,
                ..Default::default()
            };
            c.progress_days = blank.progress_days;
            c.progress_min = blank.progress_min;
            c.weeks = blank.weeks.iter().map(|w| WeekSplit { tasks: 0, finished: 0, ..*w }).collect();
            coach_view(pack, now, &pj, judgments, &feedback, &flags, &mut c);
            let (name, renamed) = names.get(key).cloned().unwrap_or_else(|| (NO_FOLDER.to_owned(), false));
            Project {
                key: key.to_owned(),
                name,
                renamed,
                tasks_in_window: t.tasks,
                judged: t.judged,
                uncertain: t.uncertain,
                rungs: t.rungs,
                most_common_rung: t.most_common_rung,
                imported_in_window: t.imported,
                weeks: t.weeks,
                by_tool: t.by_tool,
                coaching: c,
            }
        })
        .collect();
    // Most room to grow first (P5b): projects with enough judged tasks to say, then tasks that could move up a
    // rung (the same `split.up` Home shows), then the lower rung, then more tasks, then the name.
    projects.sort_by(|a, b| {
        let few = |p: &Project| p.coaching.split.tasks < p.coaching.split_min;
        few(a)
            .cmp(&few(b))
            .then_with(|| b.coaching.split.up.cmp(&a.coaching.split.up))
            .then_with(|| a.most_common_rung.unwrap_or(9).cmp(&b.most_common_rung.unwrap_or(9)))
            .then_with(|| b.tasks_in_window.cmp(&a.tasks_in_window))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    s.projects = projects;

    // Settings: next-rung tips and the self-check, from the same facts the core reads.
    let p = &pack.coach;
    let mode = aiq_core::coaching::mode(store);
    let by_id: std::collections::BTreeMap<&str, &coach::JudgedTask> =
        judged.iter().map(|t| (t.task_id.as_str(), t)).collect();
    let guesses = store.type_guesses().map_err(e)?;
    let check = coach::check_guesses(
        guesses.iter().map(|g| (g.task_id.as_str(), g.job.as_str(), g.task_type.as_str())),
        &by_id,
    );
    let st = &mut s.settings;
    st.guess_checked = check.checked;
    st.guess_right = check.right;
    st.guess_auto_off = check.auto_off(p);
    st.next_rung_on = match mode.as_str() {
        "off" => false,
        "on" => true,
        _ => !st.guess_auto_off,
    };
    st.next_rung_tips = mode;
    let mut off: Vec<String> = feedback.iter().map(|f| f.tip_id.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    off.retain(|t| coach::tip_pref(&feedback, t, now.utc_ms, p) == coach::TipPref::Off);
    st.tips_off = off;
    let readings = aiq_core::coaching::readings(store).map_err(e)?;
    let paused: Vec<_> = aiq_domain::check::verdicts(&readings).into_iter().filter(|v| v.paused).collect();
    st.accuracy_paused = aiq_core::coaching::paused_scopes(store, &paused);
    Ok(())
}

/// Home's counts over some tasks (all of them, or one project's): one definition for both (DM: derive, don't
/// store twice).
struct Tally {
    tasks: u32,
    judged: u32,
    uncertain: u32,
    rungs: [u32; 5],
    most_common_rung: Option<u8>,
    imported: u32,
    weeks: Vec<Week>,
    by_tool: Vec<(String, u32)>,
}

fn tally<'a>(tasks: impl Iterator<Item = &'a TaskRow>, weeks: &[Week]) -> Tally {
    let mut t = Tally {
        tasks: 0,
        judged: 0,
        uncertain: 0,
        rungs: [0; 5],
        most_common_rung: None,
        imported: 0,
        weeks: weeks.iter().map(|w| Week { start_ms: w.start_ms, tasks: 0 }).collect(),
        by_tool: vec![],
    };
    let mut by: std::collections::BTreeMap<&str, u32> = Default::default();
    for r in tasks {
        t.tasks += 1;
        if let Some(j) = &r.judgment {
            t.judged += 1;
            if j.state == "uncertain" {
                t.uncertain += 1;
            } else {
                t.rungs[(j.current.clamp(1, 5) - 1) as usize] += 1;
            }
        }
        t.imported += u32::from(r.origin == "imported");
        if let Some(w) = t.weeks.iter_mut().find(|w| r.first_ms >= w.start_ms && r.first_ms < w.start_ms + 7 * DAY_MS) {
            w.tasks += 1;
        }
        *by.entry(r.tool.as_str()).or_default() += 1;
    }
    let top = t.rungs.iter().copied().max().unwrap_or(0);
    t.most_common_rung = (top > 0).then(|| t.rungs.iter().position(|&n| n == top).unwrap() as u8 + 1);
    t.by_tool = by.into_iter().map(|(k, n)| (k.to_owned(), n)).collect();
    t.by_tool.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    t
}

fn judged_tasks<'a>(
    tasks: &[&'a TaskRow],
    judgments: &'a std::collections::HashMap<String, aiq_store::JudgmentRow>,
) -> Vec<aiq_domain::coach::JudgedTask> {
    tasks
        .iter()
        .filter_map(|t| {
            let j = judgments.get(&t.task_id)?;
            Some(aiq_domain::coach::JudgedTask::new(
                &t.task_id,
                t.origin == "live",
                &t.tool,
                t.first_ms,
                &j.job,
                j.task_type.as_deref(),
                &j.state,
                j.current_rung,
                j.potential_rung,
                j.coachable,
                &j.facts,
                aiq_contracts::judge::parse_read(&j.read),
            ))
        })
        .collect()
}

/// The split, kinds, next steps, habits and progress over these judged tasks, into `c`.
fn coach_view(
    pack: &PackV1,
    now: Now,
    judged: &[aiq_domain::coach::JudgedTask],
    judgments: &std::collections::HashMap<String, aiq_store::JudgmentRow>,
    feedback: &[aiq_domain::coach::Feedback],
    flags: &[aiq_domain::coach::Flag],
    c: &mut Coaching,
) {
    use aiq_domain::coach;
    let p = &pack.coach;
    let rows = coach::type_rows(judged, &pack.moves, flags, now.utc_ms, p);
    let sp = coach::split(judged, &rows, now.utc_ms - WINDOW_DAYS * DAY_MS, now.utc_ms + 1);
    c.split = SplitOut {
        tasks: sp.tasks,
        finished: sp.finished,
        could: sp.could,
        needs: sp.needs,
        uncertain: sp.uncertain,
        up: sp.up,
        up_to: sp.up_to,
    };
    for w in c.weeks.iter_mut() {
        let ws = coach::split(judged, &rows, w.start_ms, w.start_ms + 7 * DAY_MS);
        (w.tasks, w.finished) = (ws.tasks, ws.finished);
    }
    c.untyped = judged.iter().filter(|t| !t.uncertain() && t.task_type.is_none()).count() as u32;
    let move_out = |id: &str| {
        pack.moves.iter().find(|m| m.move_id == id).map(|m| MoveOut {
            move_id: m.move_id.clone(),
            habit: m.habit.clone(),
            why: m.why.clone(),
            try_this: m.try_this.clone(),
            starter: m.starter.clone(),
        })
    };
    let kind = |r: &coach::TypeRow| {
        let last = r.task_ids.iter().filter_map(|id| judgments.get(id)).find(|j| j.state != "uncertain");
        KindRow {
            job: r.job.clone(),
            task_type: r.task_type.clone(),
            label: r.label,
            tasks: r.tasks,
            uncertain: r.uncertain,
            per_week: (r.per_week * 10.0).round() / 10.0,
            today: r.today,
            potential: r.potential,
            state: r.state.as_str(),
            next: r.move_id.as_deref().and_then(move_out),
            ranked_last: r
                .move_id
                .as_deref()
                .is_some_and(|m| coach::ranked_last(feedback, flags, &r.job, m, now.utc_ms, p)),
            last_task_id: r.task_ids.first().cloned(),
            last_gap: last.map(|j| j.gap.clone()).filter(|g| !g.is_empty()),
            task_ids: r.task_ids.clone(),
        }
    };
    c.opportunities =
        coach::opportunities(&rows, feedback, flags, now.utc_ms, p, OPPORTUNITY_ROWS).into_iter().map(kind).collect();
    c.kinds = rows.iter().map(kind).collect();
    c.habits = coach::habits(judged, &pack.moves, now.utc_ms, now.offset_min, p)
        .into_iter()
        .map(|h| HabitRow {
            job: h.job,
            move_id: h.move_id,
            name: h.name,
            state: h.state.as_str(),
            eligible: h.eligible,
            seen: h.seen,
            moved_to: h.moved_to.map(|m| m.as_str()),
            moved_ms: h.moved_ms,
            moved_back: h.moved_back,
        })
        .collect();
    c.habits_imported = judged.iter().any(|t| !t.live);
    c.progress = coach::progress(judged, now.utc_ms, p)
        .into_iter()
        .map(|r| ProgressRow {
            job: r.job,
            task_type: r.task_type,
            label: r.label,
            recent: r.recent,
            earlier: r.earlier,
            uncertain: r.uncertain,
            recent_rung: r.recent_rung,
            earlier_rung: r.earlier_rung,
            trend: r.trend.as_str(),
        })
        .collect();
}

/// Each folder key's project name and whether the person chose it (design project-filter P2): the person's
/// name (`project_name.<key>` setting, not empty) wins; else the folder's last part; folders that share a last
/// part show their parent too. "" (no folder recorded) is named `NO_FOLDER`.
pub fn project_names<'a>(
    keys: impl Iterator<Item = &'a str>,
    renames: &[(String, String)],
) -> std::collections::BTreeMap<String, (String, bool)> {
    let keys: BTreeSet<&str> = keys.collect();
    let parts = |k: &str| k.split(['\\', '/']).filter(|p| !p.is_empty()).map(str::to_owned).collect::<Vec<_>>();
    let last = |k: &str| parts(k).last().cloned().unwrap_or_else(|| k.to_owned());
    let mut seen: std::collections::BTreeMap<String, u32> = Default::default();
    for k in keys.iter().filter(|k| !k.is_empty()) {
        *seen.entry(last(k).to_lowercase()).or_default() += 1;
    }
    let chosen: std::collections::BTreeMap<&str, &str> = renames
        .iter()
        .filter_map(|(k, v)| Some((k.strip_prefix(PROJECT_NAME)?, v.trim())))
        .filter(|(_, v)| !v.is_empty())
        .collect();
    keys.into_iter()
        .map(|k| {
            let named = if let Some(v) = chosen.get(k) {
                ((*v).to_owned(), true)
            } else if k.is_empty() {
                (NO_FOLDER.to_owned(), false)
            } else if seen.get(&last(k).to_lowercase()).copied().unwrap_or(0) > 1 {
                let p = parts(k);
                let sep = if k.contains('\\') { "\\" } else { "/" };
                (p[p.len().saturating_sub(2)..].join(sep), false)
            } else {
                (last(k), false)
            };
            (k.to_owned(), named)
        })
        .collect()
}

#[cfg(test)]
mod project_tests {
    use super::*;

    #[test]
    fn names_come_from_the_folder_and_shared_last_parts_show_their_parent() {
        let keys = [r"c:\work\api", r"c:\oss\api", r"c:\repos\foresight", "/home/me/aiq", ""];
        let n = project_names(keys.iter().copied(), &[]);
        let got = |k: &str| n[k].0.clone();
        assert_eq!(got(r"c:\work\api"), r"work\api");
        assert_eq!(got(r"c:\oss\api"), r"oss\api");
        assert_eq!(got(r"c:\repos\foresight"), "foresight");
        assert_eq!(got("/home/me/aiq"), "aiq");
        assert_eq!(got(""), NO_FOLDER);
        assert!(n.values().all(|(_, renamed)| !renamed));
    }

    #[test]
    fn the_persons_name_wins_and_blank_means_automatic() {
        let renames = vec![
            (format!("{PROJECT_NAME}c:\\repos\\foresight"), "Strategy".to_owned()),
            (format!("{PROJECT_NAME}c:\\repos\\aiq"), "  ".to_owned()),
        ];
        let n = project_names([r"c:\repos\foresight", r"c:\repos\aiq"].into_iter(), &renames);
        assert_eq!(n[r"c:\repos\foresight"], ("Strategy".to_owned(), true));
        assert_eq!(n[r"c:\repos\aiq"], ("aiq".to_owned(), false));
    }
}
