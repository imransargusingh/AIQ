//! Coaching from rung results (design `docs/design/coaching-from-rungs.md`, ADR-0008). Pure: no I/O.
//! One definition used by the core (next-rung tips) and AIQ home (split, rows, habits):
//! - a judgment's **move** is the next rung's unmet guardrail fact, confidently "no" (D2), never model text;
//! - a **task-type row** groups current judgments by (job, task type) and coaches a move only when the type
//!   recurs and the same move is coachable on most of its tasks (D3);
//! - the **split** counts finished / could finish / needs you over judged, not-Uncertain tasks (D9, Q-A);
//! - **habits** are replayed from judged facts every read, never stored (D6, D7);
//! - a live task's **type guess** is the nearest neighbours over the person's own judged first prompts (D5).
use crate::rung::{self, Facts, Gate};
use aiq_contracts::judge::{ReadRecord, task_type_label};
use aiq_contracts::pack::{CoachParams, MoveDef};
use std::collections::{BTreeMap, BTreeSet};

pub const DAY_MS: i64 = 86_400_000;
/// Card opportunity and detector of next-rung tips.
pub const OPPORTUNITY: &str = "next_rung";
pub const DETECTOR: &str = "task_type";
pub const DETECTOR_VERSION: u32 = 1;
/// Store key of the next-rung tips setting: auto (on unless AIQ's self-check turned them off), on, off.
pub const SETTING_NEXT_RUNG_TIPS: &str = "next_rung_tips";
/// Rung of "agents finished": Delegated or higher (Q-A).
pub const FINISHED_RUNG: u8 = 3;

pub fn tip_id(move_id: &str) -> String {
    format!("{OPPORTUNITY}.{move_id}")
}

pub fn validate_setting(key: &str, value: &str) -> Option<Result<String, String>> {
    (key == SETTING_NEXT_RUNG_TIPS).then(|| match value {
        "auto" | "on" | "off" => Ok(value.to_owned()),
        _ => Err("next-rung-tips must be auto, on or off".into()),
    })
}

/// The task type's label for a card: "Writing tests" → "writing tests" (first letter only, so "PRD" stays).
pub fn card_label(job: &str, task_type: &str) -> String {
    let l = task_type_label(job, task_type).unwrap_or("this kind of task");
    let mut c = l.chars();
    c.next().map(|f| f.to_lowercase().chain(c).collect()).unwrap_or_default()
}

// ------------------------------------------------------------------ input

/// One judged task as the coaching rules read it (the current judgment of one task).
#[derive(Debug, Clone, PartialEq)]
pub struct JudgedTask {
    pub task_id: String,
    pub live: bool,
    pub tool: String,
    pub first_ms: i64,
    /// code_change, write_doc or unclassified.
    pub job: String,
    /// None when judged before task types existed (j1) or the job is unclassified.
    pub task_type: Option<String>,
    /// judged, uncertain, right_rung, blocked
    pub state: String,
    pub current: u8,
    pub potential: u8,
    pub coachable: bool,
    pub facts: Facts,
    pub corrections: Option<u32>,
}

impl JudgedTask {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        task_id: &str,
        live: bool,
        tool: &str,
        first_ms: i64,
        job: &str,
        task_type: Option<&str>,
        state: &str,
        current: u8,
        potential: u8,
        coachable: bool,
        facts_json: &str,
        read: ReadRecord,
    ) -> Self {
        let reprompts = read.reprompts.unwrap_or(read.prompts.saturating_sub(1));
        JudgedTask {
            task_id: task_id.to_owned(),
            live,
            tool: tool.to_owned(),
            first_ms,
            job: job.to_owned(),
            task_type: task_type.map(str::to_owned),
            state: state.to_owned(),
            current,
            potential,
            coachable,
            facts: rung::facts_from_json(facts_json, reprompts),
            corrections: read.corrections,
        }
    }

    pub fn uncertain(&self) -> bool {
        self.state == "uncertain"
    }

    /// (job, task type) when both are known and the job has a pack.
    pub fn kind(&self) -> Option<(&str, &str)> {
        let t = self.task_type.as_deref()?;
        rung::PACK_JOBS.contains(&self.job.as_str()).then_some((self.job.as_str(), t))
    }
}

fn job_matches(m: &MoveDef, job: &str) -> bool {
    m.job == "any" || m.job == job
}

// ------------------------------------------------------------------ moves (D2)

/// The moves of one judgment, in pack order: for a coachable task whose potential is one rung up, each move
/// of that step whose fact the next rung's guardrail confidently fails. Unknown facts give no move.
pub fn moves_for<'a>(t: &JudgedTask, moves: &'a [MoveDef]) -> Vec<&'a MoveDef> {
    if !t.coachable || t.uncertain() || t.potential != t.current + 1 {
        return vec![];
    }
    let fails: BTreeSet<&str> = rung::missing(&t.facts, t.potential)
        .into_iter()
        .filter_map(|g| match g {
            Gate::Fails(id) => Some(id),
            _ => None,
        })
        .collect();
    moves
        .iter()
        .filter(|m| m.step == t.current && job_matches(m, &t.job))
        .filter(|m| m.facts.iter().any(|f| fails.contains(f.as_str())))
        .collect()
}

// ------------------------------------------------------------------ feedback and flags (D8)

#[derive(Debug, Clone, PartialEq)]
pub struct Feedback {
    pub tip_id: String,
    /// helpful, not_helpful, snooze, off, on, no_answer
    pub answer: String,
    pub utc_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Flag {
    pub job: String,
    pub task_type: String,
    pub move_id: String,
    /// flag or undo
    pub action: String,
    pub reason: Option<String>,
    pub utc_ms: i64,
}

/// Whether a tip may show now (the person's own answers, coaching-loop US-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipPref {
    Shown,
    /// "Don't show this again": never, on any surface, until the person turns it back on.
    Off,
    /// Snoozed until this time.
    Snoozed(i64),
}

/// Latest off / on / snooze answer decides; helpful and not-helpful don't change whether it shows.
pub fn tip_pref(feedback: &[Feedback], tip_id: &str, now_ms: i64, p: &CoachParams) -> TipPref {
    let last = feedback.iter().filter(|f| f.tip_id == tip_id && ["off", "on", "snooze"].contains(&f.answer.as_str())).max_by_key(|f| f.utc_ms);
    match last.map(|f| (f.answer.as_str(), f.utc_ms)) {
        Some(("off", _)) => TipPref::Off,
        Some(("snooze", at)) if now_ms < at + p.snooze_days as i64 * DAY_MS => TipPref::Snoozed(at + p.snooze_days as i64 * DAY_MS),
        _ => TipPref::Shown,
    }
}

/// Two "Not helpful" answers in the window rank the tip below every tip without one (US-4).
pub fn ranked_last(feedback: &[Feedback], flags: &[Flag], job: &str, move_id: &str, now_ms: i64, p: &CoachParams) -> bool {
    let since = now_ms - p.not_helpful_days as i64 * DAY_MS;
    let id = tip_id(move_id);
    let nh = feedback.iter().filter(|f| f.tip_id == id && f.answer == "not_helpful" && f.utc_ms >= since).count();
    let nu = flags
        .iter()
        .filter(|f| {
            f.job == job && f.move_id == move_id && f.action == "flag" && f.reason.as_deref() == Some("not_useful") && f.utc_ms >= since
        })
        .count();
    nh >= 2 || nu >= 2
}

/// Moves flagged now ("This doesn't fit"): the newest flag-or-undo per (job, task type, move) is a flag.
pub fn flagged_now(flags: &[Flag]) -> BTreeSet<(String, String, String)> {
    let mut last: BTreeMap<(String, String, String), (i64, usize, &str)> = BTreeMap::new();
    for (i, f) in flags.iter().enumerate() {
        let k = (f.job.clone(), f.task_type.clone(), f.move_id.clone());
        let e = last.entry(k).or_insert((f.utc_ms, i, f.action.as_str()));
        if (f.utc_ms, i) >= (e.0, e.1) {
            *e = (f.utc_ms, i, f.action.as_str());
        }
    }
    last.into_iter().filter(|(_, (_, _, a))| *a == "flag").map(|(k, _)| k).collect()
}

// ------------------------------------------------------------------ task-type rows (D3)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// A move is coached: today → today + 1.
    Coach,
    /// Fewer than `recurring_min` judged tasks in the window.
    TooFew,
    /// Most tasks are at their right rung.
    RightRung,
    /// Something outside the person caps the next step (Repo fixes).
    Blocked,
    /// No move on a majority of tasks: "AIQ isn't sure yet".
    NotSure,
    /// The move was flagged "This doesn't fit".
    Flagged,
}

impl RowState {
    pub fn as_str(self) -> &'static str {
        match self {
            RowState::Coach => "coach",
            RowState::TooFew => "too_few",
            RowState::RightRung => "right_rung",
            RowState::Blocked => "blocked",
            RowState::NotSure => "not_sure",
            RowState::Flagged => "flagged",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeRow {
    pub job: String,
    pub task_type: String,
    pub label: &'static str,
    /// Judged tasks of this type in the window, Uncertain included. Measured count.
    pub tasks: u32,
    pub uncertain: u32,
    /// tasks × 7 / window days.
    pub per_week: f64,
    /// Most common current rung among non-Uncertain tasks; ties go lower. None when all are Uncertain.
    pub today: Option<u8>,
    /// today + 1 when a move is coached.
    pub potential: Option<u8>,
    pub state: RowState,
    /// The coached move (state Coach), or the flagged one (state Flagged).
    pub move_id: Option<String>,
    /// Newest first.
    pub task_ids: Vec<String>,
    pub newest_ms: i64,
}

/// One row per (job, task type) with judged tasks whose first prompt is in the window, most frequent first.
pub fn type_rows(tasks: &[JudgedTask], moves: &[MoveDef], flags: &[Flag], now_ms: i64, p: &CoachParams) -> Vec<TypeRow> {
    let since = now_ms - p.window_days as i64 * DAY_MS;
    let mut groups: BTreeMap<(&str, &str), Vec<&JudgedTask>> = BTreeMap::new();
    for t in tasks.iter().filter(|t| t.first_ms >= since) {
        if let Some(k) = t.kind() {
            groups.entry(k).or_default().push(t);
        }
    }
    let flagged = flagged_now(flags);
    let mut rows: Vec<TypeRow> = groups
        .into_iter()
        .map(|((job, ty), mut ts)| {
            ts.sort_by(|a, b| b.first_ms.cmp(&a.first_ms).then_with(|| a.task_id.cmp(&b.task_id)));
            let sure: Vec<&&JudgedTask> = ts.iter().filter(|t| !t.uncertain()).collect();
            let mut by_rung = [0u32; 5];
            for t in &sure {
                by_rung[(t.current.clamp(1, 5) - 1) as usize] += 1;
            }
            let top = by_rung.iter().copied().max().unwrap_or(0);
            let today = (top > 0).then(|| by_rung.iter().position(|&n| n == top).unwrap() as u8 + 1);
            let mut row = TypeRow {
                job: job.to_owned(),
                task_type: ty.to_owned(),
                label: task_type_label(job, ty).unwrap_or("Other"),
                tasks: ts.len() as u32,
                uncertain: (ts.len() - sure.len()) as u32,
                per_week: ts.len() as f64 * 7.0 / p.window_days as f64,
                today,
                potential: None,
                state: RowState::NotSure,
                move_id: None,
                task_ids: ts.iter().map(|t| t.task_id.clone()).collect(),
                newest_ms: ts.first().map_or(0, |t| t.first_ms),
            };
            let Some(today) = today else { return row };
            if (ts.len() as u32) < p.recurring_min {
                row.state = RowState::TooFew;
                return row;
            }
            let at: Vec<&&&JudgedTask> = sure.iter().filter(|t| t.current == today).collect();
            let n = at.len() as f64;
            let share = |pred: &dyn Fn(&JudgedTask) -> bool| at.iter().filter(|t| pred(t)).count() as f64 / n;
            if share(&|t| t.state == "blocked") > p.type_majority {
                row.state = RowState::Blocked;
                return row;
            }
            if share(&|t| t.state == "right_rung") > p.type_majority {
                row.state = RowState::RightRung;
                return row;
            }
            // The move coachable on the most tasks at today's rung (pack order breaks ties); it needs a majority.
            let best = moves
                .iter()
                .filter(|m| m.step == today && job_matches(m, job))
                .map(|m| (m, at.iter().filter(|t| moves_for(t, moves).iter().any(|x| x.move_id == m.move_id)).count()))
                .fold(None::<(&MoveDef, usize)>, |acc, (m, c)| match acc {
                    Some((_, bc)) if bc >= c => acc,
                    _ => Some((m, c)),
                });
            if let Some((m, c)) = best
                && c as f64 / n > p.type_majority
            {
                if flagged.contains(&(job.to_owned(), ty.to_owned(), m.move_id.clone())) {
                    row.state = RowState::Flagged;
                } else {
                    row.state = RowState::Coach;
                    row.potential = Some(today + 1);
                }
                row.move_id = Some(m.move_id.clone());
            }
            row
        })
        .collect();
    rows.sort_by(|a, b| b.tasks.cmp(&a.tasks).then_with(|| a.label.cmp(b.label)).then_with(|| a.job.cmp(&b.job)));
    rows
}

/// Home's opportunity rows: coached rows, at most `n`, by times a week, then rungs to gain, then label; a move
/// with two "Not helpful" (or two "not useful" flags) in 30 days ranks after every other.
pub fn opportunities<'a>(rows: &'a [TypeRow], feedback: &[Feedback], flags: &[Flag], now_ms: i64, p: &CoachParams, n: usize) -> Vec<&'a TypeRow> {
    let mut v: Vec<(bool, &TypeRow)> = rows
        .iter()
        .filter(|r| r.state == RowState::Coach)
        .map(|r| (ranked_last(feedback, flags, &r.job, r.move_id.as_deref().unwrap_or(""), now_ms, p), r))
        .collect();
    v.sort_by(|(la, a), (lb, b)| {
        la.cmp(lb)
            .then_with(|| b.tasks.cmp(&a.tasks))
            .then_with(|| (b.potential.unwrap_or(0) - b.today.unwrap_or(0)).cmp(&(a.potential.unwrap_or(0) - a.today.unwrap_or(0))))
            .then_with(|| a.label.cmp(b.label))
    });
    v.into_iter().take(n).map(|(_, r)| r).collect()
}

// ------------------------------------------------------------------ split (D9, Q-A)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Split {
    /// Judged, not-Uncertain tasks in the window: the grain. finished + could + needs == tasks.
    pub tasks: u32,
    pub finished: u32,
    pub could: u32,
    pub needs: u32,
    /// Uncertain judgments left out, counted beside.
    pub uncertain: u32,
}

/// Finished under the interim rule (Imran 2026-11-02, Q-A): judged Delegated or higher and no re-prompt that
/// corrected the agent. Corrections unknown (judged before they were recorded) is not finished.
pub fn finished(t: &JudgedTask) -> bool {
    !t.uncertain() && t.current >= FINISHED_RUNG && t.corrections == Some(0)
}

/// The split over tasks whose first prompt is in `[from_ms, to_ms)`. Could finish = not finished, and its
/// type's row coaches a move toward Delegated or higher.
pub fn split(tasks: &[JudgedTask], rows: &[TypeRow], from_ms: i64, to_ms: i64) -> Split {
    let could_kinds: BTreeSet<(&str, &str)> = rows
        .iter()
        .filter(|r| r.state == RowState::Coach && r.potential.is_some_and(|p| p >= FINISHED_RUNG))
        .map(|r| (r.job.as_str(), r.task_type.as_str()))
        .collect();
    let mut s = Split::default();
    for t in tasks.iter().filter(|t| t.first_ms >= from_ms && t.first_ms < to_ms) {
        if t.uncertain() {
            s.uncertain += 1;
            continue;
        }
        s.tasks += 1;
        if finished(t) {
            s.finished += 1;
        } else if t.kind().is_some_and(|k| could_kinds.contains(&k)) {
            s.could += 1;
        } else {
            s.needs += 1;
        }
    }
    s
}

// ------------------------------------------------------------------ habits (D6, D7)

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HabitState {
    NotYet,
    Tried,
    Adopted,
}

impl HabitState {
    pub fn as_str(self) -> &'static str {
        match self {
            HabitState::NotYet => "not_yet",
            HabitState::Tried => "tried",
            HabitState::Adopted => "adopted",
        }
    }
}

/// Did this task show the behaviour a move asks for? None when the fact is unknown, or the habit does not
/// apply (F5 with no re-prompt): such a task is not eligible and counts neither way.
pub fn observe(t: &JudgedTask, m: &MoveDef) -> Option<bool> {
    if t.uncertain() || !job_matches(m, &t.job) || !rung::PACK_JOBS.contains(&t.job.as_str()) {
        return None;
    }
    let f = &t.facts;
    let b = |x: &rung::Merged| x.value;
    let seen = |id: &str| -> Option<bool> {
        match id {
            "F1" => b(&f.f1),
            "F3" => f.f3.map(|c| c == rung::Checker::Agent),
            "F4" => b(&f.f4),
            "F5" => (f.reprompts >= 1).then_some(()).and(b(&f.f5)),
            "F6" => b(&f.f6),
            "F7" => b(&f.f7),
            "F8" => b(&f.f8),
            "F12" => b(&f.f12).map(|v| !v),
            "F13" => b(&f.f13),
            _ => None,
        }
    };
    // Every fact the move names must be known; the behaviour is seen when all of them hold.
    let vals: Vec<Option<bool>> = m.facts.iter().map(|id| seen(id)).collect();
    if vals.iter().any(Option::is_none) {
        return None;
    }
    Some(vals.into_iter().all(|v| v == Some(true)))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Habit {
    pub job: String,
    pub move_id: String,
    pub name: String,
    pub state: HabitState,
    /// Eligible tasks in the last `habit_days`, and how many showed the behaviour.
    pub eligible: u32,
    pub seen: u32,
    /// Set when the newest observed day moved the state (for the banners).
    pub moved_to: Option<HabitState>,
    pub moved_ms: Option<i64>,
}

/// Replay one habit's observations (task time, seen) with coaching-loop D5: Tried after one use; Adopted when
/// seen in more than half of at least `habit_min` eligible tasks over `habit_days`; back to Tried when a
/// later window has enough eligible tasks and half or fewer show it. Changes only on observed days.
pub fn replay(obs: &[(i64, bool)], p: &CoachParams, offset_min: i32) -> (HabitState, Option<(HabitState, i64)>) {
    let day = |ms: i64| (ms + offset_min as i64 * 60_000).div_euclid(DAY_MS);
    let mut o: Vec<(i64, bool)> = obs.to_vec();
    o.sort();
    let days: BTreeSet<i64> = o.iter().map(|(t, _)| day(*t)).collect();
    let mut state = HabitState::NotYet;
    let mut last_move = None;
    for d in days {
        let upto: Vec<&(i64, bool)> = o.iter().filter(|(t, _)| day(*t) <= d).collect();
        let window: Vec<&&(i64, bool)> = upto.iter().filter(|(t, _)| day(*t) > d - p.habit_days as i64).collect();
        let eligible = window.len() as u32;
        let seen = window.iter().filter(|(_, s)| *s).count() as u32;
        let before = state;
        if eligible >= p.habit_min && seen * 2 > eligible {
            state = HabitState::Adopted;
        } else if state == HabitState::Adopted && eligible >= p.habit_min {
            state = HabitState::Tried;
        } else if state == HabitState::NotYet && upto.iter().any(|(_, s)| *s) {
            state = HabitState::Tried;
        }
        if state != before {
            let at = window.iter().map(|(t, _)| *t).max().unwrap_or(0);
            last_move = Some((state, at));
        }
    }
    (state, last_move)
}

/// Every habit with at least one eligible task in the coaching window, by job then pack order. One habit per
/// (job, move): moves for any job become one habit per pack job.
pub fn habits(tasks: &[JudgedTask], moves: &[MoveDef], now_ms: i64, offset_min: i32, p: &CoachParams) -> Vec<Habit> {
    let since = now_ms - p.window_days.max(p.habit_days) as i64 * DAY_MS;
    let recent = now_ms - p.habit_days as i64 * DAY_MS;
    let mut out = Vec::new();
    for job in rung::PACK_JOBS {
        for m in moves.iter().filter(|m| job_matches(m, job)) {
            let obs: Vec<(i64, bool)> = tasks
                .iter()
                .filter(|t| t.job == job)
                .filter_map(|t| observe(t, m).map(|s| (t.first_ms, s)))
                .collect();
            if !obs.iter().any(|(t, _)| *t >= since) {
                continue;
            }
            let (state, moved) = replay(&obs, p, offset_min);
            let win: Vec<&(i64, bool)> = obs.iter().filter(|(t, _)| *t >= recent).collect();
            out.push(Habit {
                job: job.to_owned(),
                move_id: m.move_id.clone(),
                name: m.habit.clone(),
                state,
                eligible: win.len() as u32,
                seen: win.iter().filter(|(_, s)| *s).count() as u32,
                moved_to: moved.map(|(s, _)| s),
                moved_ms: moved.map(|(_, t)| t),
            });
        }
    }
    out
}

// ------------------------------------------------------------------ type guess (D5, D10)

/// One judged task's first prompt as a neighbour: its type and word set (never stored, never shown).
#[derive(Debug, Clone, PartialEq)]
pub struct Neighbour {
    pub task_id: String,
    pub job: String,
    pub task_type: String,
    pub first_ms: i64,
    pub words: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Guess {
    pub job: String,
    pub task_type: String,
    /// Jaccard of the best neighbour.
    pub similarity: f64,
    /// The top neighbours the guess leaned on, best first.
    pub neighbours: Vec<String>,
}

/// Nearest neighbours: the best match must reach `match_threshold`, and at least `match_agree` of the top
/// `match_top` must share its (job, task type). `other` types never guess (nothing to coach).
pub fn guess_type(words: &BTreeSet<String>, neighbours: &[Neighbour], p: &CoachParams) -> Option<Guess> {
    if words.is_empty() {
        return None;
    }
    let mut scored: Vec<(f64, &Neighbour)> = neighbours.iter().map(|n| (crate::text::jaccard(words, &n.words), n)).collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| b.1.first_ms.cmp(&a.1.first_ms)));
    let top: Vec<&(f64, &Neighbour)> = scored.iter().take(p.match_top as usize).collect();
    let (best, n0) = **top.first()?;
    if best < p.match_threshold || n0.task_type == "other" {
        return None;
    }
    let agree = top.iter().filter(|(_, n)| n.job == n0.job && n.task_type == n0.task_type).count() as u32;
    (agree >= p.match_agree).then(|| Guess {
        job: n0.job.clone(),
        task_type: n0.task_type.clone(),
        similarity: best,
        neighbours: top.iter().map(|(_, n)| n.task_id.clone()).collect(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GuessCheck {
    /// Guesses whose task was later judged with a task type.
    pub checked: u32,
    pub right: u32,
}

impl GuessCheck {
    /// AIQ turns next-rung tips off by itself (D10): enough checks and too few right.
    pub fn auto_off(&self, p: &CoachParams) -> bool {
        self.checked >= p.guess_check_min && (self.right as f64) < p.guess_precision * self.checked as f64
    }
}

/// Check each guess (task id, job, type) against that task's later judgment. Not-yet-judged and Uncertain
/// judgments are not checked; a judged `other` counts as wrong.
pub fn check_guesses<'a>(guesses: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>, judged: &BTreeMap<&str, &JudgedTask>) -> GuessCheck {
    let mut c = GuessCheck::default();
    let mut seen = BTreeSet::new();
    for (task, job, ty) in guesses {
        if !seen.insert(task) {
            continue;
        }
        let Some(j) = judged.get(task) else { continue };
        if j.uncertain() || j.task_type.is_none() {
            continue;
        }
        c.checked += 1;
        if j.job == job && j.task_type.as_deref() == Some(ty) {
            c.right += 1;
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> CoachParams {
        CoachParams::default()
    }

    fn mv(id: &str, step: u8, facts: &[&str], job: &str) -> MoveDef {
        MoveDef {
            move_id: id.into(),
            step,
            facts: facts.iter().map(|s| s.to_string()).collect(),
            job: job.into(),
            habit: format!("habit {id}"),
            noticed: "this looks like {type}. x".into(),
            why: "w".into(),
            try_this: "t".into(),
            starter: "s".into(),
        }
    }

    fn moves() -> Vec<MoveDef> {
        vec![
            mv("brief", 1, &["F4"], "any"),
            mv("new_info", 1, &["F5"], "any"),
            mv("check_code", 1, &["F6"], "code_change"),
            mv("check_sources", 1, &["F6"], "write_doc"),
            mv("hand_over", 2, &["F13"], "code_change"),
            mv("let_agent_edit", 2, &["F1", "F12"], "code_change"),
            mv("written_brief", 3, &["F7"], "any"),
            mv("agent_checks", 3, &["F3"], "code_change"),
            mv("split_and_check", 3, &["F8"], "any"),
        ]
    }

    /// facts JSON: each (id, Some(v)) known at 0.9, missing ids unknown.
    fn facts(known: &[(&str, bool)], f3: Option<&str>) -> String {
        let mut parts: Vec<String> =
            known.iter().map(|(id, v)| format!(r#""{id}":{{"value":{v},"confidence":0.9,"source":"model","disputed":false}}"#)).collect();
        if let Some(c) = f3 {
            parts.push(format!(r#""F3":{{"value":"{c}","source":"log"}}"#));
        }
        format!("{{{}}}", parts.join(","))
    }

    #[allow(clippy::too_many_arguments)]
    fn task(id: &str, day: i64, ty: &str, current: u8, state: &str, coachable: bool, fj: &str, corrections: Option<u32>) -> JudgedTask {
        JudgedTask::new(
            id,
            true,
            "copilot-cli",
            NOW - day * DAY_MS,
            "code_change",
            Some(ty),
            state,
            current,
            if coachable { current + 1 } else { current },
            coachable,
            fj,
            ReadRecord { prompts: 1, reprompts: Some(0), corrections },
        )
    }

    const NOW: i64 = 1_800_000_000_000;

    /// A Delegated task where the agent did not run the checks: move agent_checks.
    fn delegated_person_checks(id: &str, day: i64) -> JudgedTask {
        task(id, day, "tests", 3, "judged", true, &facts(&[("F1", true), ("F13", true), ("F6", true), ("F12", false), ("F7", false), ("F8", false)], Some("person")), Some(0))
    }

    #[test]
    fn a_move_only_from_a_confident_no_and_pack_order_breaks_ties() {
        let m = moves();
        let t = delegated_person_checks("a", 1);
        let got: Vec<&str> = moves_for(&t, &m).iter().map(|m| m.move_id.as_str()).collect();
        assert_eq!(got, vec!["written_brief", "agent_checks", "split_and_check"], "pack order");
        // Unknown F3 and F7, F8: no move.
        let u = task("b", 1, "tests", 3, "judged", true, &facts(&[("F1", true), ("F13", true), ("F6", true), ("F12", false)], None), Some(0));
        assert!(moves_for(&u, &m).is_empty(), "unknown is never a move");
        // Not coachable or Uncertain: no move.
        let mut nc = delegated_person_checks("c", 1);
        nc.coachable = false;
        assert!(moves_for(&nc, &m).is_empty());
        let mut un = delegated_person_checks("d", 1);
        un.state = "uncertain".into();
        assert!(moves_for(&un, &m).is_empty());
        // Directed → Delegated driven step by step: let_agent_edit (F12 rule-out).
        let d = task("e", 1, "tests", 2, "judged", true, &facts(&[("F1", true), ("F12", true), ("F13", true), ("F6", true)], None), Some(0));
        assert_eq!(moves_for(&d, &m).iter().map(|m| m.move_id.as_str()).collect::<Vec<_>>(), vec!["let_agent_edit"]);
        // A doc task never gets a code move.
        let mut doc = task("f", 1, "spec", 1, "judged", true, &facts(&[("F4", true), ("F6", false)], None), None);
        doc.job = "write_doc".into();
        assert_eq!(moves_for(&doc, &m).iter().map(|m| m.move_id.as_str()).collect::<Vec<_>>(), vec!["check_sources"]);
    }

    #[test]
    fn rows_need_recurrence_and_a_majority_and_ties_go_lower() {
        let m = moves();
        let p = params();
        let two = vec![delegated_person_checks("a", 1), delegated_person_checks("b", 2)];
        let r = type_rows(&two, &m, &[], NOW, &p);
        assert_eq!((r[0].state, r[0].tasks), (RowState::TooFew, 2));
        let three = vec![delegated_person_checks("a", 1), delegated_person_checks("b", 2), delegated_person_checks("c", 3)];
        let r = type_rows(&three, &m, &[], NOW, &p);
        assert_eq!(r[0].state, RowState::Coach);
        assert_eq!((r[0].today, r[0].potential), (Some(3), Some(4)));
        assert_eq!(r[0].move_id.as_deref(), Some("written_brief"), "every move ties: pack order");
        assert!((r[0].per_week - 0.7).abs() < 1e-9);
        // Exactly half with the move is not a majority.
        let no_move = task("x", 1, "tests", 3, "judged", false, "{}", Some(0));
        let no_move2 = task("y", 2, "tests", 3, "judged", false, "{}", Some(0));
        let half = vec![delegated_person_checks("a", 1), delegated_person_checks("b", 2), no_move, no_move2];
        let r = type_rows(&half, &m, &[], NOW, &p);
        assert_eq!(r[0].state, RowState::NotSure, "2 of 4 is not more than half");
        // Today's rung ties go lower.
        let mixed = vec![
            delegated_person_checks("a", 1),
            task("b", 2, "tests", 2, "right_rung", false, "{}", Some(0)),
            delegated_person_checks("c", 3),
            task("d", 4, "tests", 2, "right_rung", false, "{}", Some(0)),
        ];
        assert_eq!(type_rows(&mixed, &m, &[], NOW, &p)[0].today, Some(2));
        // Outside the window: no row.
        let old = vec![delegated_person_checks("a", 31)];
        assert!(type_rows(&old, &m, &[], NOW, &p).is_empty());
    }

    #[test]
    fn a_flagged_move_is_never_coached_and_undo_restores_it() {
        let m = moves();
        let p = params();
        let ts: Vec<_> = (0..3).map(|i| delegated_person_checks(&format!("t{i}"), i + 1)).collect();
        let flag = |a: &str, at: i64| Flag {
            job: "code_change".into(),
            task_type: "tests".into(),
            move_id: "written_brief".into(),
            action: a.into(),
            reason: Some("evidence_wrong".into()),
            utc_ms: at,
        };
        let r = type_rows(&ts, &m, &[flag("flag", 1)], NOW, &p);
        assert_eq!((r[0].state, r[0].potential), (RowState::Flagged, None));
        assert!(opportunities(&r, &[], &[], NOW, &p, 3).is_empty());
        let r = type_rows(&ts, &m, &[flag("flag", 1), flag("undo", 2)], NOW, &p);
        assert_eq!(r[0].state, RowState::Coach);
    }

    #[test]
    fn opportunities_rank_by_frequency_and_not_helpful_goes_last() {
        let m = moves();
        let p = params();
        let mut ts: Vec<JudgedTask> = (0..3).map(|i| delegated_person_checks(&format!("t{i}"), i + 1)).collect();
        for i in 0..4 {
            let mut t = task(&format!("f{i}"), i + 1, "fix_bug", 1, "judged", true, &facts(&[("F4", false), ("F6", true)], None), Some(0));
            t.facts.reprompts = 0;
            ts.push(t);
        }
        let rows = type_rows(&ts, &m, &[], NOW, &p);
        let o = opportunities(&rows, &[], &[], NOW, &p, 3);
        assert_eq!(o.iter().map(|r| r.task_type.as_str()).collect::<Vec<_>>(), vec!["fix_bug", "tests"]);
        let nh = |at: i64| Feedback { tip_id: tip_id("brief"), answer: "not_helpful".into(), utc_ms: at };
        let o = opportunities(&rows, &[nh(NOW - DAY_MS), nh(NOW - 2 * DAY_MS)], &[], NOW, &p, 3);
        assert_eq!(o[0].task_type, "tests", "two not-helpful in 30 days rank last");
        let o = opportunities(&rows, &[nh(NOW - DAY_MS), nh(NOW - 40 * DAY_MS)], &[], NOW, &p, 3);
        assert_eq!(o[0].task_type, "fix_bug", "one in the window is not enough");
    }

    #[test]
    fn split_excludes_uncertain_and_parts_sum_to_the_grain() {
        let m = moves();
        let p = params();
        let mut ts: Vec<JudgedTask> = (0..3).map(|i| delegated_person_checks(&format!("t{i}"), i + 1)).collect();
        ts.push(task("c", 1, "tests", 3, "judged", true, "{}", Some(1))); // corrected: not finished
        ts.push(task("u", 1, "tests", 3, "uncertain", false, "{}", Some(0)));
        ts.push(task("o", 1, "fix_bug", 1, "judged", false, "{}", Some(0)));
        ts.push(task("j1", 1, "tests", 4, "judged", false, "{}", None)); // corrections unknown (j1)
        let mut untyped = task("n", 1, "tests", 1, "judged", false, "{}", Some(0));
        untyped.task_type = None;
        ts.push(untyped);
        let rows = type_rows(&ts, &m, &[], NOW, &p);
        let s = split(&ts, &rows, NOW - 30 * DAY_MS, NOW + 1);
        assert_eq!(s.uncertain, 1);
        assert_eq!(s.tasks, 7);
        assert_eq!(s.finished, 3);
        assert_eq!(s.could, 2, "the corrected and the j1 tests task: tests coaches toward Orchestrated");
        assert_eq!(s.needs, 2, "a NULL type is Needs you");
        assert_eq!(s.finished + s.could + s.needs, s.tasks);
    }

    #[test]
    fn habit_replay_follows_the_d5_boundaries() {
        let p = params();
        let d = |n: i64| NOW + n * DAY_MS;
        assert_eq!(replay(&[], &p, 0).0, HabitState::NotYet);
        assert_eq!(replay(&[(d(0), false)], &p, 0).0, HabitState::NotYet, "eligible but not seen");
        assert_eq!(replay(&[(d(0), true)], &p, 0).0, HabitState::Tried, "one observed use");
        assert_eq!(replay(&[(d(0), true), (d(1), false), (d(2), true)], &p, 0).0, HabitState::Adopted, "2 of 3");
        assert_eq!(replay(&[(d(0), true), (d(1), true)], &p, 0).0, HabitState::Tried, "2 eligible is too few");
        // Adopted, then a later window with 3 eligible and 1 seen: back to Tried.
        let lapse = [(d(0), true), (d(1), true), (d(2), true), (d(20), false), (d(21), true), (d(22), false)];
        let (s, moved) = replay(&lapse, &p, 0);
        assert_eq!(s, HabitState::Tried);
        assert_eq!(moved.map(|m| m.0), Some(HabitState::Tried));
        // Adopted, then only 2 eligible later: stays Adopted.
        let quiet = [(d(0), true), (d(1), true), (d(2), true), (d(20), false), (d(21), false)];
        assert_eq!(replay(&quiet, &p, 0).0, HabitState::Adopted);
        // Order-independent within the input.
        let mut rev = lapse.to_vec();
        rev.reverse();
        assert_eq!(replay(&rev, &p, 0), replay(&lapse, &p, 0));
    }

    #[test]
    fn habit_observations_ignore_unknown_facts_and_f5_needs_a_reprompt() {
        let m = moves();
        let new_info = m.iter().find(|m| m.move_id == "new_info").unwrap();
        let t = task("a", 1, "tests", 1, "judged", false, &facts(&[("F5", true)], None), Some(0));
        assert_eq!(observe(&t, new_info), None, "no re-prompt: F5 habit not eligible");
        let mut r = t.clone();
        r.facts.reprompts = 1;
        assert_eq!(observe(&r, new_info), Some(true));
        let unknown = task("b", 1, "tests", 1, "judged", false, "{}", Some(0));
        assert_eq!(observe(&unknown, &m[0]), None);
        let lae = m.iter().find(|m| m.move_id == "let_agent_edit").unwrap();
        let driven = task("c", 1, "tests", 2, "judged", false, &facts(&[("F1", true), ("F12", true)], None), Some(0));
        assert_eq!(observe(&driven, lae), Some(false));
        let mut un = driven.clone();
        un.state = "uncertain".into();
        assert_eq!(observe(&un, lae), None);
        let hs = habits(&[driven], &m, NOW, 0, &params());
        assert!(hs.iter().any(|h| h.move_id == "let_agent_edit" && h.state == HabitState::NotYet && h.eligible == 1));
    }

    fn nb(id: &str, ty: &str, words: &[&str]) -> Neighbour {
        Neighbour {
            task_id: id.into(),
            job: "code_change".into(),
            task_type: ty.into(),
            first_ms: 0,
            words: words.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn type_guess_needs_threshold_and_two_of_three() {
        let p = params();
        let w = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<BTreeSet<_>>();
        let ns = vec![
            nb("a", "tests", &["unit", "tests", "parser", "cases"]),
            nb("b", "tests", &["unit", "tests", "lexer", "cases"]),
            nb("c", "fix_bug", &["crash", "parser", "null"]),
        ];
        let g = guess_type(&w(&["unit", "tests", "parser"]), &ns, &p).unwrap();
        assert_eq!((g.job.as_str(), g.task_type.as_str()), ("code_change", "tests"));
        assert!(g.similarity >= 0.45);
        assert_eq!(g.neighbours.len(), 3);
        assert!(guess_type(&w(&["deploy", "kubernetes"]), &ns, &p).is_none(), "below threshold");
        let split = vec![nb("a", "tests", &["unit", "tests"]), nb("c", "fix_bug", &["unit", "crash"]), nb("d", "review", &["unit", "pr"])];
        assert!(guess_type(&w(&["unit", "tests"]), &split, &p).is_none(), "only 1 of 3 agree");
        let other = vec![nb("a", "other", &["x", "y"]), nb("b", "other", &["x", "y"])];
        assert!(guess_type(&w(&["x", "y"]), &other, &p).is_none(), "other never guesses");
        assert!(guess_type(&BTreeSet::new(), &ns, &p).is_none());
    }

    #[test]
    fn self_check_turns_tips_off_at_seven_of_ten() {
        let p = params();
        let judged: Vec<JudgedTask> = (0..10).map(|i| delegated_person_checks(&format!("t{i}"), 1)).collect();
        let map: BTreeMap<&str, &JudgedTask> = judged.iter().map(|t| (t.task_id.as_str(), t)).collect();
        let ids: Vec<String> = (0..10).map(|i| format!("t{i}")).collect();
        let g = |right: usize| -> GuessCheck {
            check_guesses(ids.iter().enumerate().map(|(i, id)| (id.as_str(), "code_change", if i < right { "tests" } else { "fix_bug" })), &map)
        };
        assert_eq!(g(8), GuessCheck { checked: 10, right: 8 });
        assert!(!g(8).auto_off(&p));
        assert!(g(7).auto_off(&p));
        let nine: BTreeMap<&str, &JudgedTask> = map.iter().take(9).map(|(k, v)| (*k, *v)).collect();
        assert!(!check_guesses(ids.iter().map(|id| (id.as_str(), "code_change", "fix_bug")), &nine).auto_off(&p), "9 checks is too few");
    }

    #[test]
    fn tip_preferences_follow_the_latest_answer() {
        let p = params();
        let f = |a: &str, at: i64| Feedback { tip_id: "x".into(), answer: a.into(), utc_ms: at };
        assert_eq!(tip_pref(&[], "x", NOW, &p), TipPref::Shown);
        assert_eq!(tip_pref(&[f("off", 1)], "x", NOW, &p), TipPref::Off);
        assert_eq!(tip_pref(&[f("off", 1), f("helpful", 2)], "x", NOW, &p), TipPref::Off, "helpful never lifts off");
        assert_eq!(tip_pref(&[f("off", 1), f("on", 2)], "x", NOW, &p), TipPref::Shown);
        assert!(matches!(tip_pref(&[f("snooze", NOW - DAY_MS)], "x", NOW, &p), TipPref::Snoozed(_)));
        assert_eq!(tip_pref(&[f("snooze", NOW - 8 * DAY_MS)], "x", NOW, &p), TipPref::Shown);
    }

    #[test]
    fn card_labels_lower_case_the_first_letter_only() {
        assert_eq!(card_label("write_doc", "spec"), "writing a spec or PRD");
        assert_eq!(card_label("code_change", "tests"), "writing tests");
    }
}
