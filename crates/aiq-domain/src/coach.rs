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
/// The analysis's job confidence at or above which it is the answer key for the self-checks: the coaching bar.
/// Measured 2026-11-10 on the laptop: the analysis named no job on 13 of 36 labelled tasks, and was sure of job
/// and kind on 11 of 23 of the rest, so "right" against an unsure analysis was a coin the check could not see.
pub const ANSWER_KEY_MIN: f64 = 0.8;
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
/// When the kind is not known (a job-level tip, `ANY_KIND`), the job's own words.
pub fn card_label(job: &str, task_type: &str) -> String {
    let l = task_type_label(job, task_type).or_else(|| job_words(job)).unwrap_or("this kind of task");
    let mut c = l.chars();
    c.next().map(|f| f.to_lowercase().chain(c).collect()).unwrap_or_default()
}

/// A pack job in the words a card uses when the kind of task is not known.
pub fn job_words(job: &str) -> Option<&'static str> {
    match job {
        "code_change" => Some("changing code"),
        "write_doc" => Some("writing a doc"),
        "ask_research" => Some("a question or research"),
        "run_ship" => Some("running or shipping something"),
        _ => None,
    }
}

// ------------------------------------------------------------------ input

/// One judged task as the coaching rules read it (the current judgment of one task).
#[derive(Debug, Clone, PartialEq)]
pub struct JudgedTask {
    pub task_id: String,
    pub live: bool,
    pub tool: String,
    pub first_ms: i64,
    /// code_change, write_doc, ask_research, run_ship or unclassified.
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
    /// The analysis's own confidence in its job (1.0 when not given). The self-checks use only sure analyses as
    /// the answer key (`ANSWER_KEY_MIN`): an unsure analysis cannot say whether a guess was right.
    pub job_confidence: f64,
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
            facts: rung::facts_from_json(facts_json, reprompts, job),
            corrections: read.corrections,
            job_confidence: 1.0,
        }
    }

    /// The analysis's confidence in its job (`job_confidence`).
    pub fn with_job_confidence(mut self, c: f64) -> Self {
        self.job_confidence = c;
        self
    }

    /// The analysis is sure enough of the job to be the answer key for a guess (`ANSWER_KEY_MIN`).
    pub fn answer_key(&self) -> bool {
        !self.uncertain() && self.job_confidence >= ANSWER_KEY_MIN
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
    let conf = |id: &str| t.facts.all().into_iter().find(|m| m.id == id).map_or(1.0, |m| m.confidence);
    moves
        .iter()
        .filter(|m| m.step == t.current && job_matches(m, &t.job))
        .filter(|m| m.facts.iter().any(|f| fails.contains(f.as_str()) && m.min_confidence.is_none_or(|c| conf(f) >= c)))
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
    let last = feedback
        .iter()
        .filter(|f| f.tip_id == tip_id && ["off", "on", "snooze"].contains(&f.answer.as_str()))
        .max_by_key(|f| f.utc_ms);
    match last.map(|f| (f.answer.as_str(), f.utc_ms)) {
        Some(("off", _)) => TipPref::Off,
        Some(("snooze", at)) if now_ms < at + p.snooze_days as i64 * DAY_MS => {
            TipPref::Snoozed(at + p.snooze_days as i64 * DAY_MS)
        }
        _ => TipPref::Shown,
    }
}

/// Two "Not helpful" answers in the window rank the tip below every tip without one (US-4).
pub fn ranked_last(
    feedback: &[Feedback],
    flags: &[Flag],
    job: &str,
    move_id: &str,
    now_ms: i64,
    p: &CoachParams,
) -> bool {
    let since = now_ms - p.not_helpful_days as i64 * DAY_MS;
    let id = tip_id(move_id);
    let nh = feedback.iter().filter(|f| f.tip_id == id && f.answer == "not_helpful" && f.utc_ms >= since).count();
    let nu = flags
        .iter()
        .filter(|f| {
            f.job == job
                && f.move_id == move_id
                && f.action == "flag"
                && f.reason.as_deref() == Some("not_useful")
                && f.utc_ms >= since
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

/// The (job, move) pairs whose habit is Adopted now: one definition of "already does this" for the rows and
/// the live tip. Before, a row coached the move missing on most tasks over 30 days while the habit, over 14 days,
/// was Adopted, so the live tip was dropped (laptop 2026-11-10: 2 of 2 decided tips dropped as habit_adopted).
pub fn adopted(habits: &[Habit]) -> BTreeSet<(String, String)> {
    habits.iter().filter(|h| h.state == HabitState::Adopted).map(|h| (h.job.clone(), h.move_id.clone())).collect()
}

/// The task type of a job-level row (`job_rows`): all of the job's kinds together.
pub const ANY_KIND: &str = "any";

/// One row per (job, task type) with judged tasks whose first prompt is in the window, most frequent first.
/// A move whose habit is Adopted (`adopted`) is not coached; the row coaches its next move instead.
pub fn type_rows(
    tasks: &[JudgedTask],
    moves: &[MoveDef],
    flags: &[Flag],
    adopted: &BTreeSet<(String, String)>,
    now_ms: i64,
    p: &CoachParams,
) -> Vec<TypeRow> {
    let since = now_ms - p.window_days as i64 * DAY_MS;
    let mut groups: BTreeMap<(&str, &str), Vec<&JudgedTask>> = BTreeMap::new();
    for t in tasks.iter().filter(|t| t.first_ms >= since) {
        if let Some(k) = t.kind() {
            groups.entry(k).or_default().push(t);
        }
    }
    let flagged = flagged_now(flags);
    let is_flagged = |job: &str, ty: &str, m: &str| flagged.contains(&(job.to_owned(), ty.to_owned(), m.to_owned()));
    let mut rows: Vec<TypeRow> = groups
        .into_iter()
        .map(|((job, ty), ts)| {
            let label = task_type_label(job, ty).unwrap_or("Other");
            rate(job, ty, label, ts, moves, &|m| is_flagged(job, ty, m), adopted, p)
        })
        .collect();
    rows.sort_by(|a, b| b.tasks.cmp(&a.tasks).then_with(|| a.label.cmp(b.label)).then_with(|| a.job.cmp(&b.job)));
    rows
}

/// One row per pack job over all of its kinds (task type `ANY_KIND`), for a live task whose job is known but whose
/// kind is not (design kind-label, "Coach by job"). Every habit is defined per job, never per kind, so the job is
/// enough to pick one. Measured 2026-11-10 on the laptop: a sure label's job matched the analysis 17 of 21 times,
/// its job and kind 9 of 13. A move flagged "This doesn't fit" on any kind of the job is not coached here.
pub fn job_rows(
    tasks: &[JudgedTask],
    moves: &[MoveDef],
    flags: &[Flag],
    adopted: &BTreeSet<(String, String)>,
    now_ms: i64,
    p: &CoachParams,
) -> Vec<TypeRow> {
    let since = now_ms - p.window_days as i64 * DAY_MS;
    let flagged = flagged_now(flags);
    rung::PACK_JOBS
        .iter()
        .filter_map(|job| {
            let ts: Vec<&JudgedTask> = tasks.iter().filter(|t| t.first_ms >= since && t.job == *job).collect();
            (!ts.is_empty()).then(|| {
                let any_kind = |m: &str| flagged.iter().any(|(j, _, f)| j == job && f == m);
                rate(job, ANY_KIND, "This kind of task", ts, moves, &any_kind, adopted, p)
            })
        })
        .collect()
}

/// Rate one group of tasks (D3): its typical rung and the move coachable on most of them at that rung.
#[allow(clippy::too_many_arguments)]
fn rate(
    job: &str,
    ty: &str,
    label: &'static str,
    mut ts: Vec<&JudgedTask>,
    moves: &[MoveDef],
    flagged: &dyn Fn(&str) -> bool,
    adopted: &BTreeSet<(String, String)>,
    p: &CoachParams,
) -> TypeRow {
    ts.sort_by(|a, b| b.first_ms.cmp(&a.first_ms).then_with(|| a.task_id.cmp(&b.task_id)));
    let sure: Vec<&&JudgedTask> = ts.iter().filter(|t| !t.uncertain()).collect();
    let today = typical_rung(sure.iter().map(|t| t.current));
    let mut row = TypeRow {
        job: job.to_owned(),
        task_type: ty.to_owned(),
        label,
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
    // The move coachable on the most tasks at today's rung (pack order breaks ties); it needs a majority. A move
    // the person has already adopted is skipped, so the next one is coached instead.
    let best = moves
        .iter()
        .filter(|m| m.step == today && job_matches(m, job))
        .filter(|m| !adopted.contains(&(job.to_owned(), m.move_id.clone())))
        .map(|m| (m, at.iter().filter(|t| moves_for(t, moves).iter().any(|x| x.move_id == m.move_id)).count()))
        .fold(None::<(&MoveDef, usize)>, |acc, (m, c)| match acc {
            Some((_, bc)) if bc >= c => acc,
            _ => Some((m, c)),
        });
    if let Some((m, c)) = best
        && c as f64 / n > p.type_majority
    {
        if flagged(&m.move_id) {
            row.state = RowState::Flagged;
        } else {
            row.state = RowState::Coach;
            row.potential = Some(today + 1);
        }
        row.move_id = Some(m.move_id.clone());
    }
    row
}

/// Most common rung among these current rungs; ties go lower. None when there are none.
pub fn typical_rung(rungs: impl IntoIterator<Item = u8>) -> Option<u8> {
    let mut by_rung = [0u32; 5];
    for r in rungs {
        by_rung[(r.clamp(1, 5) - 1) as usize] += 1;
    }
    let top = by_rung.iter().copied().max().unwrap_or(0);
    (top > 0).then(|| by_rung.iter().position(|&n| n == top).unwrap() as u8 + 1)
}

// ------------------------------------------------------------------ rung progress (success measure)

/// Each half of the progress comparison: the last 30 days against the 30 days before. 14-day halves held too
/// few tasks per kind to tell a move from chance on the laptop data (measured 2026-11-08: one false "down").
// simplify: fixed at 30; becomes a CoachParams field if Imran wants it editable in Settings.
pub const PROGRESS_DAYS: u32 = 30;

/// A rung move or a habit change shows only when chance alone would give a split this uneven less often than
/// this (two-sided Fisher exact test). Measured 2026-11-08: the typical rung flips on one or two tasks when
/// nearly every task is Assisted, so a move needs this check on top of the typical rungs differing.
// simplify: 0.05 with about 60 kind x habit pairs per person lets ~3 false changes through; raise the bar or
// cap the rows if people report habit changes they don't recognise.
pub const CHANCE_P: f64 = 0.05;

/// Tasks where a habit fits, in each half, before AIQ says the habit changed for a kind of task.
pub const HABIT_CHANGE_MIN: u32 = 5;

/// Two-sided Fisher exact test: `a` of `n1` against `c` of `n2`. The chance of a split at least this uneven
/// when both halves share one rate. 1.0 when either half is empty.
pub fn fisher_p(a: u32, n1: u32, c: u32, n2: u32) -> f64 {
    if n1 == 0 || n2 == 0 {
        return 1.0;
    }
    let n = n1 + n2;
    let k = a + c;
    let lf = |m: u32| (1..=m).map(|i| (i as f64).ln()).sum::<f64>();
    let lc = |m: u32, r: u32| lf(m) - lf(r) - lf(m - r);
    let pr = |x: u32| (lc(n1, x) + lc(n2, k - x) - lc(n, k)).exp();
    let p0 = pr(a);
    let lo = k.saturating_sub(n2);
    let hi = k.min(n1);
    (lo..=hi).map(pr).filter(|q| *q <= p0 * (1.0 + 1e-9)).sum::<f64>().min(1.0)
}

/// Share of `xs` at rung `at` or above is beyond chance against `ys` (and higher).
fn above_by_more_than_chance(xs: &[u8], ys: &[u8], at: u8) -> bool {
    let hi = |v: &[u8]| v.iter().filter(|r| **r >= at).count() as u32;
    let (a, c) = (hi(xs), hi(ys));
    a * ys.len() as u32 > c * xs.len() as u32 && fisher_p(a, xs.len() as u32, c, ys.len() as u32) < CHANCE_P
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    Up,
    Down,
    /// The typical rungs match, or differ by no more than chance.
    Same,
    /// Either half has fewer than `recurring_min` judged, not-Uncertain tasks.
    TooFew,
}

impl Trend {
    pub fn as_str(self) -> &'static str {
        match self {
            Trend::Up => "up",
            Trend::Down => "down",
            Trend::Same => "same",
            Trend::TooFew => "too_few",
        }
    }
}

/// One kind of task's typical rung in the last `PROGRESS_DAYS` against the `PROGRESS_DAYS` before: Imran's
/// success measure, "a recurring task type moves up a rung". Counts are measured; rungs are the judge's.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub job: String,
    pub task_type: String,
    pub label: &'static str,
    /// Judged, not-Uncertain tasks in each half: the tasks behind each typical rung.
    pub recent: u32,
    pub earlier: u32,
    /// Uncertain judgments in both halves, left out of the rungs and counted beside.
    pub uncertain: u32,
    /// Typical rung (most common, ties lower) of each half; None when that half has no tasks.
    pub recent_rung: Option<u8>,
    pub earlier_rung: Option<u8>,
    pub trend: Trend,
}

/// One row per kind of task with judged tasks in the last 2 × `PROGRESS_DAYS`. A kind needs at least
/// `recurring_min` (the tips' repeated-kind minimum) judged, not-Uncertain tasks in BOTH halves before AIQ
/// says anything; otherwise it is "too few". It moved up (down) only when its typical rung rose (fell) AND the
/// share of tasks it now sits at changed by more than chance (`CHANCE_P`): for up, more tasks at the new rung
/// or above; for down, fewer tasks above the new rung. Otherwise "same".
/// The judge's fallback kind "other" is not one kind of work and is left out. Order: moved up, moved down, same, too few; then most recent tasks, then label.
pub fn progress(tasks: &[JudgedTask], now_ms: i64, p: &CoachParams) -> Vec<Progress> {
    let half = PROGRESS_DAYS as i64 * DAY_MS;
    let (mid, start) = (now_ms - half, now_ms - 2 * half);
    #[derive(Default)]
    struct Halves {
        recent: Vec<u8>,
        earlier: Vec<u8>,
        uncertain: u32,
    }
    let mut groups: BTreeMap<(&str, &str), Halves> = BTreeMap::new();
    for t in tasks.iter().filter(|t| t.first_ms >= start && t.first_ms <= now_ms) {
        let Some(k) = t.kind().filter(|(_, ty)| *ty != "other") else { continue };
        let g = groups.entry(k).or_default();
        match (t.uncertain(), t.first_ms >= mid) {
            (true, _) => g.uncertain += 1,
            (false, true) => g.recent.push(t.current),
            (false, false) => g.earlier.push(t.current),
        }
    }
    let mut out: Vec<Progress> = groups
        .into_iter()
        .map(|((job, ty), Halves { recent, earlier, uncertain })| {
            let (rr, er) = (typical_rung(recent.iter().copied()), typical_rung(earlier.iter().copied()));
            let enough = recent.len() as u32 >= p.recurring_min && earlier.len() as u32 >= p.recurring_min;
            let trend = match (enough, rr, er) {
                (true, Some(r), Some(e)) if r > e && above_by_more_than_chance(&recent, &earlier, r) => Trend::Up,
                (true, Some(r), Some(e)) if r < e && above_by_more_than_chance(&earlier, &recent, r + 1) => Trend::Down,
                (true, Some(_), Some(_)) => Trend::Same,
                _ => Trend::TooFew,
            };
            Progress {
                job: job.to_owned(),
                task_type: ty.to_owned(),
                label: task_type_label(job, ty).unwrap_or("Other"),
                recent: recent.len() as u32,
                earlier: earlier.len() as u32,
                uncertain,
                recent_rung: rr,
                earlier_rung: er,
                trend,
            }
        })
        .collect();
    let rank = |t: Trend| match t {
        Trend::Up => 0,
        Trend::Down => 1,
        Trend::Same => 2,
        Trend::TooFew => 3,
    };
    out.sort_by(|a, b| {
        rank(a.trend)
            .cmp(&rank(b.trend))
            .then_with(|| b.recent.cmp(&a.recent))
            .then_with(|| a.label.cmp(b.label))
            .then_with(|| a.job.cmp(&b.job))
    });
    out
}

// ------------------------------------------------------------------ milestones (the record of steps up)

/// Tasks of one kind at a rung (or higher) within `MILESTONE_DAYS` before that rung counts as reached. One task
/// at a higher rung is too thin to celebrate: measured on the laptop 2026-11-10, 26 "first task at a higher rung"
/// events against 12 kinds that held one for 3 tasks in 30 days.
pub const MILESTONE_TASKS: u32 = 3;
pub const MILESTONE_DAYS: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MilestoneKind {
    /// `MILESTONE_TASKS` tasks of the kind at `rung` or higher within `MILESTONE_DAYS`.
    Reached,
    /// The kind's typical rung moved up by more than chance (the `progress` rule), on the day it first did.
    MovedUp,
}

impl MilestoneKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MilestoneKind::Reached => "reached",
            MilestoneKind::MovedUp => "moved_up",
        }
    }
}

/// One step up a kind of task took. Milestones stay once earned: a quieter month never takes one away.
#[derive(Debug, Clone, PartialEq)]
pub struct Milestone {
    pub job: String,
    pub task_type: String,
    pub label: &'static str,
    pub kind: MilestoneKind,
    /// The rung reached (Reached) or the new typical rung (MovedUp), 2..=5.
    pub rung: u8,
    /// MovedUp: the typical rung of the 30 days before. Reached: None.
    pub from_rung: Option<u8>,
    /// Reached: the task that made `MILESTONE_TASKS`. MovedUp: the day the move first held.
    pub earned_ms: i64,
    /// The kind's first task at `rung` or higher.
    pub first_ms: i64,
}

/// Every milestone in this task history, newest first (ties: higher rung, then label). Uncertain judgments and
/// the judge's fallback kind "other" never count. MovedUp replays `progress` once a day from the first day both
/// halves can hold tasks through today, and keeps the first day each (from, to) move held for a kind.
// simplify: the replay is days x tasks (about 140 x 900 on the laptop, 2026-11-10); keep a per-day index if a
// history of several years makes the snapshot slow.
pub fn milestones(tasks: &[JudgedTask], now_ms: i64, p: &CoachParams) -> Vec<Milestone> {
    let window = MILESTONE_DAYS as i64 * DAY_MS;
    let mut kinds: BTreeMap<(&str, &str), Vec<(i64, u8)>> = BTreeMap::new();
    for t in tasks.iter().filter(|t| !t.uncertain() && t.first_ms <= now_ms) {
        if let Some(k) = t.kind().filter(|(_, ty)| *ty != "other") {
            kinds.entry(k).or_default().push((t.first_ms, t.current));
        }
    }
    let make = |job: &str, ty: &str, kind, rung, from_rung, earned_ms, first_ms| Milestone {
        job: job.to_owned(),
        task_type: ty.to_owned(),
        label: task_type_label(job, ty).unwrap_or("Other"),
        kind,
        rung,
        from_rung,
        earned_ms,
        first_ms,
    };
    let mut out = Vec::new();
    for (&(job, ty), v) in kinds.iter_mut() {
        v.sort();
        for rung in 2..=5u8 {
            let at: Vec<i64> = v.iter().filter(|(_, r)| *r >= rung).map(|(t, _)| *t).collect();
            let n = MILESTONE_TASKS as usize;
            if let Some(w) = at.windows(n).find(|w| w[n - 1] - w[0] < window) {
                out.push(make(job, ty, MilestoneKind::Reached, rung, None, w[n - 1], at[0]));
            }
        }
    }
    if let Some(oldest) = kinds.values().flat_map(|v| v.first()).map(|(t, _)| *t).min() {
        let mut seen: BTreeSet<(String, String, u8, u8)> = BTreeSet::new();
        // The earlier half first holds a task the day after `oldest` + one half; today is always checked last.
        let first_day = oldest + PROGRESS_DAYS as i64 * DAY_MS;
        let days = (first_day..now_ms).step_by(DAY_MS as usize).chain([now_ms]);
        for day in days.filter(|d| *d >= first_day) {
            for r in progress(tasks, day, p).into_iter().filter(|r| r.trend == Trend::Up) {
                let (Some(to), Some(from)) = (r.recent_rung, r.earlier_rung) else { continue };
                if seen.insert((r.job.clone(), r.task_type.clone(), from, to)) {
                    let first = kinds
                        .get(&(r.job.as_str(), r.task_type.as_str()))
                        .and_then(|v| v.iter().find(|(_, x)| *x >= to))
                        .map_or(day, |(t, _)| *t);
                    out.push(make(&r.job, &r.task_type, MilestoneKind::MovedUp, to, Some(from), day, first));
                }
            }
        }
    }
    out.sort_by(|a, b| {
        b.earned_ms.cmp(&a.earned_ms).then_with(|| b.rung.cmp(&a.rung)).then_with(|| a.label.cmp(b.label))
    });
    out
}

/// A milestone earned this recently still gets its one note where the person works (status line, Claude Code,
/// VS Code); an older one is only on the Milestones page.
pub const MILESTONE_NOTE_WITHIN_MS: i64 = 7 * DAY_MS;

/// The milestone to say once: the newest earned after `seen_ms` and within `MILESTONE_NOTE_WITHIN_MS`. A move
/// beats a rung reached on the same day, then the higher rung. None when nothing new.
pub fn milestone_to_note(ms: &[Milestone], seen_ms: i64, now_ms: i64) -> Option<&Milestone> {
    ms.iter().filter(|m| m.earned_ms > seen_ms && now_ms - m.earned_ms < MILESTONE_NOTE_WITHIN_MS).max_by(|a, b| {
        a.earned_ms
            .cmp(&b.earned_ms)
            .then_with(|| (a.kind == MilestoneKind::MovedUp).cmp(&(b.kind == MilestoneKind::MovedUp)))
            .then_with(|| a.rung.cmp(&b.rung))
    })
}

/// "Fixing a bug or failing test reached Delegated", "Adding a feature moved up to Delegated".
pub fn milestone_words(m: &Milestone) -> String {
    let rung = crate::rung::rung_name(m.rung);
    match m.kind {
        MilestoneKind::Reached => format!("{} reached {rung}", m.label),
        MilestoneKind::MovedUp => format!("{} moved up to {rung}", m.label),
    }
}

/// A kind of task partway to its next milestone: `have` of `MILESTONE_TASKS` tasks at `rung` or higher in the
/// last `MILESTONE_DAYS`.
#[derive(Debug, Clone, PartialEq)]
pub struct NearMilestone {
    pub job: String,
    pub task_type: String,
    pub label: &'static str,
    pub rung: u8,
    pub have: u32,
}

/// The kinds closest to their next milestone: per kind, the rung above the highest it reached (Directed when it
/// reached none), with at least one but fewer than `MILESTONE_TASKS` tasks there lately. Most tasks first, then
/// higher rung, then label; at most `limit`.
pub fn near_milestones(tasks: &[JudgedTask], reached: &[Milestone], now_ms: i64, limit: usize) -> Vec<NearMilestone> {
    let since = now_ms - MILESTONE_DAYS as i64 * DAY_MS;
    let mut top: BTreeMap<(&str, &str), u8> = BTreeMap::new();
    for m in reached.iter().filter(|m| m.kind == MilestoneKind::Reached) {
        let e = top.entry((m.job.as_str(), m.task_type.as_str())).or_insert(1);
        *e = (*e).max(m.rung);
    }
    let mut recent: BTreeMap<(&str, &str), Vec<u8>> = BTreeMap::new();
    for t in tasks.iter().filter(|t| !t.uncertain() && t.first_ms > since && t.first_ms <= now_ms) {
        if let Some(k) = t.kind().filter(|(_, ty)| *ty != "other") {
            recent.entry(k).or_default().push(t.current);
        }
    }
    let mut out: Vec<NearMilestone> = recent
        .into_iter()
        .filter_map(|((job, ty), rs)| {
            let rung = top.get(&(job, ty)).copied().unwrap_or(1) + 1;
            let have = rs.iter().filter(|r| **r >= rung).count() as u32;
            (rung <= 5 && (1..MILESTONE_TASKS).contains(&have)).then(|| NearMilestone {
                job: job.to_owned(),
                task_type: ty.to_owned(),
                label: task_type_label(job, ty).unwrap_or("Other"),
                rung,
                have,
            })
        })
        .collect();
    out.sort_by(|a, b| b.have.cmp(&a.have).then_with(|| b.rung.cmp(&a.rung)).then_with(|| a.label.cmp(b.label)));
    out.truncate(limit);
    out
}

/// How far one move reaches on the person's judged tasks in the window (measured from stored facts, no model).
/// A next-step tip fires in the moment only for a kind of task whose coached move this is (D3/D4), so
/// `kinds` is what can fire; `tasks` counts every judged task where the move applies.
#[derive(Debug, Clone, PartialEq)]
pub struct Reach {
    pub move_id: String,
    pub job: String,
    pub habit: String,
    /// Judged, not-Uncertain tasks in the window where this move applies (moves_for).
    pub tasks: u32,
    /// Kinds of task whose coached next step is this move: (label, judged tasks of that kind in the window).
    pub kinds: Vec<(&'static str, u32)>,
}

/// One `Reach` per pack move, in pack order.
pub fn reach(tasks: &[JudgedTask], rows: &[TypeRow], moves: &[MoveDef], now_ms: i64, p: &CoachParams) -> Vec<Reach> {
    let since = now_ms - p.window_days as i64 * DAY_MS;
    let recent: Vec<&JudgedTask> = tasks.iter().filter(|t| t.first_ms >= since && !t.uncertain()).collect();
    moves
        .iter()
        .map(|m| Reach {
            move_id: m.move_id.clone(),
            job: m.job.clone(),
            habit: m.habit.clone(),
            tasks: recent.iter().filter(|t| moves_for(t, moves).iter().any(|x| x.move_id == m.move_id)).count() as u32,
            kinds: rows
                .iter()
                .filter(|r| r.state == RowState::Coach && r.move_id.as_deref() == Some(m.move_id.as_str()))
                .map(|r| (r.label, r.tasks))
                .collect(),
        })
        .collect()
}

/// Judged, not-Uncertain tasks in the window per job, and how many the judge was sure (≥ `min_conf`) had no
/// goal, context and limits (F4 "no"), at any rung. The F4 tip only coaches Assisted tasks; this is the
/// ceiling if it coached every rung.
pub fn f4_missing(tasks: &[JudgedTask], now_ms: i64, min_conf: f64, p: &CoachParams) -> BTreeMap<String, (u32, u32)> {
    let since = now_ms - p.window_days as i64 * DAY_MS;
    let mut out: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for t in tasks.iter().filter(|t| t.first_ms >= since && !t.uncertain()) {
        let e = out.entry(t.job.clone()).or_default();
        e.0 += 1;
        if t.facts.f4.value == Some(false) && t.facts.f4.confidence >= min_conf {
            e.1 += 1;
        }
    }
    out
}

/// Home's opportunity rows: coached rows, at most `n`, by times a week, then rungs to gain, then label; a move
/// with two "Not helpful" (or two "not useful" flags) in 30 days ranks after every other.
pub fn opportunities<'a>(
    rows: &'a [TypeRow],
    feedback: &[Feedback],
    flags: &[Flag],
    now_ms: i64,
    p: &CoachParams,
    n: usize,
) -> Vec<&'a TypeRow> {
    let mut v: Vec<(bool, &TypeRow)> = rows
        .iter()
        .filter(|r| r.state == RowState::Coach)
        .map(|r| (ranked_last(feedback, flags, &r.job, r.move_id.as_deref().unwrap_or(""), now_ms, p), r))
        .collect();
    v.sort_by(|(la, a), (lb, b)| {
        la.cmp(lb)
            .then_with(|| b.tasks.cmp(&a.tasks))
            .then_with(|| {
                (b.potential.unwrap_or(0) - b.today.unwrap_or(0))
                    .cmp(&(a.potential.unwrap_or(0) - a.today.unwrap_or(0)))
            })
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
    /// Not finished, and the judge sees a next step at least one rung up (potential > current): a subset of
    /// could + needs, so the move people can make now shows even when no kind reaches Delegated.
    pub up: u32,
    /// The rung those tasks' next steps reach most often (ties: the lower rung). None when `up` is 0.
    pub up_to: Option<u8>,
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
    let mut to = [0u32; 6];
    for t in tasks.iter().filter(|t| t.first_ms >= from_ms && t.first_ms < to_ms) {
        if t.uncertain() {
            s.uncertain += 1;
            continue;
        }
        s.tasks += 1;
        if !finished(t) && t.potential > t.current {
            s.up += 1;
            to[usize::from(t.potential.min(5))] += 1;
        }
        if finished(t) {
            s.finished += 1;
        } else if t.kind().is_some_and(|k| could_kinds.contains(&k)) {
            s.could += 1;
        } else {
            s.needs += 1;
        }
    }
    let most = to.iter().copied().max().unwrap_or(0);
    s.up_to = (most > 0).then(|| to.iter().position(|&n| n == most).unwrap_or(0) as u8);
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
            "F14" => b(&f.f14),
            "F15" => b(&f.f15),
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
    /// That move was back from Adopted to Tried (a lapse), not a first use.
    pub moved_back: bool,
}

/// Replay one habit's observations (task time, seen) with coaching-loop D5: Tried after one use; Adopted when
/// seen in more than half of at least `habit_min` eligible tasks over `habit_days`; back to Tried when a
/// later window has enough eligible tasks and half or fewer show it. Changes only on observed days.
/// The last move is (new state, time, moved back from Adopted).
pub fn replay(obs: &[(i64, bool)], p: &CoachParams, offset_min: i32) -> (HabitState, Option<(HabitState, i64, bool)>) {
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
        let lapsed = state == HabitState::Adopted && eligible >= p.habit_min;
        let first_use = state == HabitState::NotYet && upto.iter().any(|(_, s)| *s);
        if eligible >= p.habit_min && seen * 2 > eligible {
            state = HabitState::Adopted;
        } else if lapsed || first_use {
            state = HabitState::Tried;
        }
        if state != before {
            let at = window.iter().map(|(t, _)| *t).max().unwrap_or(0);
            last_move = Some((state, at, before == HabitState::Adopted));
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
            let obs: Vec<(i64, bool)> =
                tasks.iter().filter(|t| t.job == job).filter_map(|t| observe(t, m).map(|s| (t.first_ms, s))).collect();
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
                moved_to: moved.map(|(s, _, _)| s),
                moved_ms: moved.map(|(_, t, _)| t),
                moved_back: moved.is_some_and(|(_, _, back)| back),
            });
        }
    }
    out
}

/// One habit's use in one kind of task, last `PROGRESS_DAYS` against the `PROGRESS_DAYS` before, when it
/// changed by more than chance. Habits move before rungs do (measured 2026-11-08: no kind moved a rung, three
/// habits changed beyond chance), so these are the steps toward a move.
#[derive(Debug, Clone, PartialEq)]
pub struct HabitChange {
    pub job: String,
    pub task_type: String,
    pub label: &'static str,
    pub move_id: String,
    pub habit: String,
    /// Tasks of this kind where the habit fits (its facts known), and how many showed it, in each half.
    pub earlier_seen: u32,
    pub earlier_eligible: u32,
    pub recent_seen: u32,
    pub recent_eligible: u32,
    /// true: seen more often now ("growing"); false: less often lately.
    pub up: bool,
    pub p: f64,
}

/// Every (kind of task, habit) whose share of tasks showing it changed by more than chance between the two
/// halves, with at least `HABIT_CHANGE_MIN` tasks where it fits in each half. Uncertain judgments and the
/// fallback kind "other" are left out. Growing first, then strongest evidence, then label and habit.
pub fn habit_changes(tasks: &[JudgedTask], moves: &[MoveDef], now_ms: i64) -> Vec<HabitChange> {
    let half = PROGRESS_DAYS as i64 * DAY_MS;
    let (mid, start) = (now_ms - half, now_ms - 2 * half);
    let mut kinds: BTreeMap<(&str, &str), Vec<&JudgedTask>> = BTreeMap::new();
    for t in tasks.iter().filter(|t| t.first_ms >= start && t.first_ms <= now_ms && !t.uncertain()) {
        if let Some(k) = t.kind().filter(|(_, ty)| *ty != "other") {
            kinds.entry(k).or_default().push(t);
        }
    }
    let mut out = Vec::new();
    for ((job, ty), ts) in kinds {
        for m in moves.iter().filter(|m| job_matches(m, job)) {
            let (mut es, mut ee, mut rs, mut re) = (0u32, 0u32, 0u32, 0u32);
            for t in &ts {
                let Some(seen) = observe(t, m) else { continue };
                if t.first_ms >= mid {
                    re += 1;
                    rs += seen as u32;
                } else {
                    ee += 1;
                    es += seen as u32;
                }
            }
            if ee < HABIT_CHANGE_MIN || re < HABIT_CHANGE_MIN || rs * ee == es * re {
                continue;
            }
            let p = fisher_p(es, ee, rs, re);
            if p >= CHANCE_P {
                continue;
            }
            out.push(HabitChange {
                job: job.to_owned(),
                task_type: ty.to_owned(),
                label: task_type_label(job, ty).unwrap_or("Other"),
                move_id: m.move_id.clone(),
                habit: m.habit.clone(),
                earlier_seen: es,
                earlier_eligible: ee,
                recent_seen: rs,
                recent_eligible: re,
                up: rs * ee > es * re,
                p,
            });
        }
    }
    out.sort_by(|a, b| {
        b.up.cmp(&a.up)
            .then_with(|| a.p.total_cmp(&b.p))
            .then_with(|| a.label.cmp(b.label))
            .then_with(|| a.move_id.cmp(&b.move_id))
    });
    out
}

/// Tasks after a tip that a habit's outcome is read from (UI review 2026-11-04, item 6).
pub const AFTER_TIP_TASKS: usize = 3;

/// What happened after a next-step tip for move `m` delivered at `at_ms`: of the first `n` judged tasks that
/// started after it and where the habit fits (its facts are known, Uncertain left out), how many showed it.
/// Returns (seen, checked); checked is 0 until such a task exists. Measured from judgments, never self-reported.
pub fn after_tip(tasks: &[JudgedTask], m: &MoveDef, at_ms: i64, n: usize) -> (u32, u32) {
    let mut obs: Vec<(i64, bool)> =
        tasks.iter().filter(|t| t.first_ms > at_ms).filter_map(|t| observe(t, m).map(|s| (t.first_ms, s))).collect();
    obs.sort();
    obs.truncate(n);
    (obs.iter().filter(|(_, s)| *s).count() as u32, obs.len() as u32)
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
    let mut scored: Vec<(f64, &Neighbour)> =
        neighbours.iter().map(|n| (crate::text::jaccard(words, &n.words), n)).collect();
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

/// Check each guess (task id, job, type; oldest first) against that task's later judgment. Not-yet-judged and
/// Uncertain judgments are not checked; a judged `other` counts as wrong. Only the newest
/// `2 × guess_check_min` checked guesses count (DC-036): an all-time count let old misses, and labels from an
/// older prompt or model, hold tips forever (laptop 2026-11-08: from 6 of 10 it needed 10 right in a row, and
/// the misses never left).
/// `jobs_only`: a guess is right when it names the judged job (labels, which coach by job); else job and kind.
pub fn check_guesses<'a>(
    guesses: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
    judged: &BTreeMap<&str, &JudgedTask>,
    jobs_only: bool,
    p: &CoachParams,
) -> GuessCheck {
    let mut seen = BTreeSet::new();
    let mut results = Vec::new();
    for (task, job, ty) in guesses {
        if !seen.insert(task) {
            continue;
        }
        let Some(j) = judged.get(task) else { continue };
        if !j.answer_key() || j.task_type.is_none() {
            continue;
        }
        results.push(j.job == job && (jobs_only || j.task_type.as_deref() == Some(ty)));
    }
    let newest = &results[results.len().saturating_sub(2 * p.guess_check_min as usize)..];
    GuessCheck { checked: newest.len() as u32, right: newest.iter().filter(|r| **r).count() as u32 }
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
            counts: "c".into(),
            min_confidence: None,
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
        let mut parts: Vec<String> = known
            .iter()
            .map(|(id, v)| format!(r#""{id}":{{"value":{v},"confidence":0.9,"source":"model","disputed":false}}"#))
            .collect();
        if let Some(c) = f3 {
            parts.push(format!(r#""F3":{{"value":"{c}","source":"log"}}"#));
        }
        format!("{{{}}}", parts.join(","))
    }

    #[allow(clippy::too_many_arguments)]
    fn task(
        id: &str,
        day: i64,
        ty: &str,
        current: u8,
        state: &str,
        coachable: bool,
        fj: &str,
        corrections: Option<u32>,
    ) -> JudgedTask {
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
        task(
            id,
            day,
            "tests",
            3,
            "judged",
            true,
            &facts(
                &[("F1", true), ("F13", true), ("F6", true), ("F12", false), ("F7", false), ("F8", false)],
                Some("person"),
            ),
            Some(0),
        )
    }

    #[test]
    fn a_move_only_from_a_confident_no_and_pack_order_breaks_ties() {
        let m = moves();
        let t = delegated_person_checks("a", 1);
        let got: Vec<&str> = moves_for(&t, &m).iter().map(|m| m.move_id.as_str()).collect();
        assert_eq!(got, vec!["written_brief", "agent_checks", "split_and_check"], "pack order");
        // Unknown F3 and F7, F8: no move.
        let u = task(
            "b",
            1,
            "tests",
            3,
            "judged",
            true,
            &facts(&[("F1", true), ("F13", true), ("F6", true), ("F12", false)], None),
            Some(0),
        );
        assert!(moves_for(&u, &m).is_empty(), "unknown is never a move");
        // Not coachable or Uncertain: no move.
        let mut nc = delegated_person_checks("c", 1);
        nc.coachable = false;
        assert!(moves_for(&nc, &m).is_empty());
        let mut un = delegated_person_checks("d", 1);
        un.state = "uncertain".into();
        assert!(moves_for(&un, &m).is_empty());
        // Directed → Delegated driven step by step: let_agent_edit (F12 rule-out).
        let d = task(
            "e",
            1,
            "tests",
            2,
            "judged",
            true,
            &facts(&[("F1", true), ("F12", true), ("F13", true), ("F6", true)], None),
            Some(0),
        );
        assert_eq!(moves_for(&d, &m).iter().map(|m| m.move_id.as_str()).collect::<Vec<_>>(), vec!["let_agent_edit"]);
        // A weak fact's "no" needs its move's higher bar (F4 at 0.9 here passes 0.8, at 0.7 it does not).
        let mut strict = m.clone();
        strict[0].min_confidence = Some(0.8);
        let weak = |c: f64| {
            let fj = format!(
                r#"{{"F4":{{"value":false,"confidence":{c},"source":"model","disputed":false}},"F6":{{"value":true,"confidence":0.9,"source":"model","disputed":false}}}}"#
            );
            task("w", 1, "tests", 1, "judged", true, &fj, Some(0))
        };
        assert_eq!(moves_for(&weak(0.9), &strict).len(), 1);
        assert!(moves_for(&weak(0.7), &strict).is_empty());
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
        let r = type_rows(&two, &m, &[], &BTreeSet::new(), NOW, &p);
        assert_eq!((r[0].state, r[0].tasks), (RowState::TooFew, 2));
        let three =
            vec![delegated_person_checks("a", 1), delegated_person_checks("b", 2), delegated_person_checks("c", 3)];
        let r = type_rows(&three, &m, &[], &BTreeSet::new(), NOW, &p);
        assert_eq!(r[0].state, RowState::Coach);
        assert_eq!((r[0].today, r[0].potential), (Some(3), Some(4)));
        assert_eq!(r[0].move_id.as_deref(), Some("written_brief"), "every move ties: pack order");
        assert!((r[0].per_week - 0.7).abs() < 1e-9);
        // Exactly half with the move is not a majority.
        let no_move = task("x", 1, "tests", 3, "judged", false, "{}", Some(0));
        let no_move2 = task("y", 2, "tests", 3, "judged", false, "{}", Some(0));
        let half = vec![delegated_person_checks("a", 1), delegated_person_checks("b", 2), no_move, no_move2];
        let r = type_rows(&half, &m, &[], &BTreeSet::new(), NOW, &p);
        assert_eq!(r[0].state, RowState::NotSure, "2 of 4 is not more than half");
        // Today's rung ties go lower.
        let mixed = vec![
            delegated_person_checks("a", 1),
            task("b", 2, "tests", 2, "right_rung", false, "{}", Some(0)),
            delegated_person_checks("c", 3),
            task("d", 4, "tests", 2, "right_rung", false, "{}", Some(0)),
        ];
        assert_eq!(type_rows(&mixed, &m, &[], &BTreeSet::new(), NOW, &p)[0].today, Some(2));
        // Outside the window: no row.
        let old = vec![delegated_person_checks("a", 31)];
        assert!(type_rows(&old, &m, &[], &BTreeSet::new(), NOW, &p).is_empty());
    }

    #[test]
    fn an_adopted_habit_is_not_coached_and_the_next_move_is() {
        let (m, p) = (moves(), params());
        let assisted = |id: &str, day: i64| {
            task(id, day, "tests", 1, "judged", true, &facts(&[("F4", false), ("F6", false)], None), Some(0))
        };
        let ts: Vec<JudgedTask> = (0..3).map(|i| assisted(&format!("a{i}"), i + 1)).collect();
        assert_eq!(type_rows(&ts, &m, &[], &BTreeSet::new(), NOW, &p)[0].move_id.as_deref(), Some("brief"));
        let adopted = BTreeSet::from([("code_change".to_owned(), "brief".to_owned())]);
        let r = type_rows(&ts, &m, &[], &adopted, NOW, &p);
        assert_eq!(
            (r[0].state, r[0].move_id.as_deref()),
            (RowState::Coach, Some("check_code")),
            "next move, not a drop"
        );
        let all = BTreeSet::from([
            ("code_change".to_owned(), "brief".to_owned()),
            ("code_change".to_owned(), "check_code".to_owned()),
        ]);
        assert_eq!(type_rows(&ts, &m, &[], &all, NOW, &p)[0].state, RowState::NotSure);
        let h = |state| Habit {
            job: "code_change".into(),
            move_id: "brief".into(),
            name: "n".into(),
            state,
            eligible: 3,
            seen: 3,
            moved_to: None,
            moved_ms: None,
            moved_back: false,
        };
        assert_eq!(super::adopted(&[h(HabitState::Adopted), h(HabitState::Tried)]).len(), 1);
    }

    #[test]
    fn a_job_row_coaches_across_kinds_too_few_to_coach_alone() {
        let (m, p) = (moves(), params());
        let gap = facts(&[("F4", false), ("F6", true)], None);
        let ts = vec![
            task("a", 1, "tests", 1, "judged", true, &gap, Some(0)),
            task("b", 2, "fix_bug", 1, "judged", true, &gap, Some(0)),
            task("c", 3, "refactor", 1, "judged", true, &gap, Some(0)),
        ];
        let kinds = type_rows(&ts, &m, &[], &BTreeSet::new(), NOW, &p);
        assert!(kinds.iter().all(|r| r.state == RowState::TooFew), "one task of each kind");
        let jobs = job_rows(&ts, &m, &[], &BTreeSet::new(), NOW, &p);
        assert_eq!(jobs.len(), 1, "only jobs with tasks");
        let j = &jobs[0];
        assert_eq!(
            (j.job.as_str(), j.task_type.as_str(), j.state, j.move_id.as_deref()),
            ("code_change", ANY_KIND, RowState::Coach, Some("brief"))
        );
        assert_eq!(card_label("code_change", ANY_KIND), "changing code");
        assert_eq!(card_label("code_change", "fix_bug"), "fixing a bug or failing test");
        // A flag on any kind of the job stops the job row's move.
        let f = Flag {
            job: "code_change".into(),
            task_type: "tests".into(),
            move_id: "brief".into(),
            action: "flag".into(),
            reason: Some("not_useful".into()),
            utc_ms: 1,
        };
        assert_eq!(job_rows(&ts, &m, &[f], &BTreeSet::new(), NOW, &p)[0].state, RowState::Flagged);
    }

    #[test]
    fn reach_counts_tasks_per_move_and_only_coached_kinds_can_fire() {
        let m = moves();
        let p = params();
        // Three Assisted code tasks with no goal or limits (F4 no) and checked output (F6 yes): `brief`.
        let assisted = |id: &str, day: i64| {
            task(id, day, "tests", 1, "judged", true, &facts(&[("F4", false), ("F6", true)], None), Some(0))
        };
        let mut ts: Vec<JudgedTask> = (0..3).map(|i| assisted(&format!("a{i}"), i + 1)).collect();
        ts.push(delegated_person_checks("d", 1));
        let mut old = assisted("old", 31);
        old.task_id = "old".into();
        ts.push(old);
        let rows = type_rows(&ts, &m, &[], &BTreeSet::new(), NOW, &p);
        let r = reach(&ts, &rows, &m, NOW, &p);
        assert_eq!(r.len(), m.len(), "one per move, pack order");
        let brief = r.iter().find(|x| x.move_id == "brief").unwrap();
        assert_eq!(brief.tasks, 3, "the 31-day-old task is outside the window");
        assert_eq!(brief.kinds, vec![("Writing tests", 4)], "the kind's coached move is brief (3 of 4)");
        let wb = r.iter().find(|x| x.move_id == "written_brief").unwrap();
        assert_eq!((wb.tasks, wb.kinds.len()), (1, 0), "a move on one task is not the kind's move");
        // F4 ceiling at any rung: the Delegated task has F4 unknown; three of four sure-no in the window.
        let f4 = f4_missing(&ts, NOW, 0.8, &p);
        assert_eq!(f4.get("code_change"), Some(&(4, 3)));
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
        let r = type_rows(&ts, &m, &[flag("flag", 1)], &BTreeSet::new(), NOW, &p);
        assert_eq!((r[0].state, r[0].potential), (RowState::Flagged, None));
        assert!(opportunities(&r, &[], &[], NOW, &p, 3).is_empty());
        let r = type_rows(&ts, &m, &[flag("flag", 1), flag("undo", 2)], &BTreeSet::new(), NOW, &p);
        assert_eq!(r[0].state, RowState::Coach);
    }

    #[test]
    fn opportunities_rank_by_frequency_and_not_helpful_goes_last() {
        let m = moves();
        let p = params();
        let mut ts: Vec<JudgedTask> = (0..3).map(|i| delegated_person_checks(&format!("t{i}"), i + 1)).collect();
        for i in 0..4 {
            let mut t = task(
                &format!("f{i}"),
                i + 1,
                "fix_bug",
                1,
                "judged",
                true,
                &facts(&[("F4", false), ("F6", true)], None),
                Some(0),
            );
            t.facts.reprompts = 0;
            ts.push(t);
        }
        let rows = type_rows(&ts, &m, &[], &BTreeSet::new(), NOW, &p);
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
        let rows = type_rows(&ts, &m, &[], &BTreeSet::new(), NOW, &p);
        let s = split(&ts, &rows, NOW - 30 * DAY_MS, NOW + 1);
        assert_eq!(s.uncertain, 1);
        assert_eq!(s.tasks, 7);
        assert_eq!(s.finished, 3);
        assert_eq!(s.could, 2, "the corrected and the j1 tests task: tests coaches toward Orchestrated");
        assert_eq!(s.needs, 2, "a NULL type is Needs you");
        assert_eq!(s.finished + s.could + s.needs, s.tasks);
        assert!(s.up <= s.could + s.needs, "up is a subset of the not-finished tasks");
    }

    #[test]
    fn split_counts_tasks_one_step_from_a_higher_rung() {
        let m = moves();
        let p = params();
        let mut ts = vec![
            task("a", 1, "tests", 1, "judged", true, "{}", Some(0)),
            task("b", 1, "tests", 1, "judged", true, "{}", Some(0)),
            task("c", 1, "tests", 2, "judged", true, "{}", Some(0)),
            task("r", 1, "tests", 1, "right_rung", false, "{}", Some(0)),
            task("u", 1, "tests", 1, "uncertain", true, "{}", Some(0)),
        ];
        ts[0].potential = 2;
        ts[1].potential = 2;
        ts[2].potential = 3;
        ts[3].potential = 1;
        ts[4].potential = 2;
        let rows = type_rows(&ts, &m, &[], &BTreeSet::new(), NOW, &p);
        let s = split(&ts, &rows, NOW - 30 * DAY_MS, NOW + 1);
        assert_eq!(s.up, 3, "uncertain and right-rung tasks don't count");
        assert_eq!(s.up_to, Some(2), "two of three reach Directed");
        let none = split(&ts[3..4], &rows, NOW - 30 * DAY_MS, NOW + 1);
        assert_eq!((none.up, none.up_to), (0, None));
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
        assert_eq!(moved.map(|m| (m.0, m.2)), Some((HabitState::Tried, true)), "a lapse, not a first use");
        assert_eq!(replay(&[(d(0), true)], &p, 0).1.map(|m| m.2), Some(false), "first use is not a lapse");
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
        let split = vec![
            nb("a", "tests", &["unit", "tests"]),
            nb("c", "fix_bug", &["unit", "crash"]),
            nb("d", "review", &["unit", "pr"]),
        ];
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
            check_guesses(
                ids.iter()
                    .enumerate()
                    .map(|(i, id)| (id.as_str(), "code_change", if i < right { "tests" } else { "fix_bug" })),
                &map,
                false,
                &p,
            )
        };
        assert_eq!(g(8), GuessCheck { checked: 10, right: 8 });
        assert!(!g(8).auto_off(&p));
        assert!(g(7).auto_off(&p));
        let jobs = check_guesses(ids.iter().map(|id| (id.as_str(), "code_change", "any")), &map, true, &p);
        assert_eq!(jobs, GuessCheck { checked: 10, right: 10 }, "a job-level guess is checked on its job only");
        let nine: BTreeMap<&str, &JudgedTask> = map.iter().take(9).map(|(k, v)| (*k, *v)).collect();
        assert!(
            !check_guesses(ids.iter().map(|id| (id.as_str(), "code_change", "fix_bug")), &nine, false, &p).auto_off(&p),
            "9 checks is too few"
        );
    }

    #[test]
    fn self_check_counts_only_the_newest_twenty_so_old_misses_leave() {
        let p = params();
        let judged: Vec<JudgedTask> = (0..30).map(|i| delegated_person_checks(&format!("t{i}"), 1)).collect();
        let map: BTreeMap<&str, &JudgedTask> = judged.iter().map(|t| (t.task_id.as_str(), t)).collect();
        let ids: Vec<String> = (0..30).map(|i| format!("t{i}")).collect();
        // Oldest 10 wrong (as the laptop's 6 of 10, but worse), then 20 right: the wrong ones have left.
        let c = check_guesses(
            ids.iter()
                .enumerate()
                .map(|(i, id)| (id.as_str(), "code_change", if i < 10 { "fix_bug" } else { "tests" })),
            &map,
            false,
            &p,
        );
        assert_eq!(c, GuessCheck { checked: 20, right: 20 });
        assert!(!c.auto_off(&p));
        // Newest misses still count: 10 right then 10 wrong is held.
        let c = check_guesses(
            ids.iter()
                .enumerate()
                .skip(10)
                .map(|(i, id)| (id.as_str(), "code_change", if i < 20 { "tests" } else { "fix_bug" })),
            &map,
            false,
            &p,
        );
        assert_eq!(c, GuessCheck { checked: 20, right: 10 });
        assert!(c.auto_off(&p));
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
    // ---------------------------------------------------------------- rung progress

    fn at(id: &str, day: i64, ty: &str, current: u8, state: &str) -> JudgedTask {
        task(id, day, ty, current, state, false, "{}", Some(0))
    }

    #[test]
    fn typical_rung_is_the_most_common_and_ties_go_lower() {
        assert_eq!(typical_rung([2, 3, 3]), Some(3));
        assert_eq!(typical_rung([2, 3]), Some(2));
        assert_eq!(typical_rung([]), None);
    }

    /// Days ago for a task `d` days into the earlier half.
    const E: i64 = PROGRESS_DAYS as i64;

    #[test]
    fn progress_compares_the_last_30_days_with_the_30_before() {
        let mut ts = vec![];
        // tests: Directed before, Delegated now, every task -> up (p = 1/35).
        for i in 0..3 {
            ts.push(at(&format!("te{i}"), E + 6 + i, "tests", 2, "judged"));
        }
        for i in 0..4 {
            ts.push(at(&format!("tr{i}"), 1 + i, "tests", 3, "judged"));
        }
        ts.push(at("tu", 2, "tests", 5, "uncertain"));
        // fix_bug: Delegated before, Directed now, five each -> down (p < 0.01).
        for i in 0..5 {
            ts.push(at(&format!("fe{i}"), E + 1 + i, "fix_bug", 3, "judged"));
            ts.push(at(&format!("fr{i}"), 3 + i, "fix_bug", 2, "right_rung"));
        }
        // refactor: same rung both halves.
        for i in 0..3 {
            ts.push(at(&format!("re{i}"), E + 2 + i, "refactor", 2, "judged"));
            ts.push(at(&format!("rr{i}"), 5 + i, "refactor", 2, "judged"));
        }
        // deps: two earlier tasks only -> too few, even though the rung differs.
        ts.push(at("de0", E + 6, "deps", 1, "judged"));
        ts.push(at("de1", E + 7, "deps", 1, "judged"));
        for i in 0..3 {
            ts.push(at(&format!("dr{i}"), 2 + i, "deps", 4, "judged"));
        }
        // other: the judge's fallback, never a row. Older than both halves: out.
        for i in 0..6 {
            ts.push(at(&format!("o{i}"), 1 + i * 3, "other", 1 + (i as u8 % 3), "judged"));
        }
        ts.push(at("old", 2 * E + 1, "tests", 1, "judged"));
        let p = progress(&ts, NOW, &params());
        let got: Vec<_> = p
            .iter()
            .map(|r| (r.task_type.as_str(), r.trend.as_str(), r.recent, r.earlier, r.recent_rung, r.earlier_rung))
            .collect();
        assert_eq!(
            got,
            vec![
                ("tests", "up", 4, 3, Some(3), Some(2)),
                ("fix_bug", "down", 5, 5, Some(2), Some(3)),
                ("refactor", "same", 3, 3, Some(2), Some(2)),
                ("deps", "too_few", 3, 2, Some(4), Some(1)),
            ]
        );
        assert_eq!(p[0].uncertain, 1, "uncertain counted beside, not on a rung");
    }

    /// The laptop's 2026-11-08 shapes: a typical rung that flips on one or two tasks is not a move.
    #[test]
    fn progress_says_same_when_the_typical_rung_differs_only_by_chance() {
        let mut ts = vec![];
        let mut add = |pre: &str, base: i64, rungs: &[(u8, usize)], ty: &str| {
            let mut i = 0;
            for &(r, n) in rungs {
                for _ in 0..n {
                    ts.push(at(&format!("{pre}{i}"), base + (i as i64 % 20), ty, r, "judged"));
                    i += 1;
                }
            }
        };
        // fix_bug, 14-day halves on 11-08: 3 Assisted + 4 Delegated, then 12 + 4 Directed + 2 Delegated.
        add("fe", E + 1, &[(1, 3), (3, 4)], "fix_bug");
        add("fr", 1, &[(1, 12), (2, 4), (3, 2)], "fix_bug");
        // feature, 30-day halves on 11-08: mode 1 then mode 3, p = 0.71.
        add("ae", E + 1, &[(1, 5), (2, 2), (3, 4), (4, 1)], "feature");
        add("ar", 1, &[(1, 5), (2, 2), (3, 8)], "feature");
        let p = progress(&ts, NOW, &params());
        let got: Vec<_> = p.iter().map(|r| (r.task_type.as_str(), r.trend, r.earlier_rung, r.recent_rung)).collect();
        assert_eq!(
            got,
            vec![("fix_bug", Trend::Same, Some(3), Some(1)), ("feature", Trend::Same, Some(1), Some(3))],
            "rungs kept for the record, but neither moved"
        );
    }

    #[test]
    fn fisher_p_matches_known_values() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-3;
        assert!(close(fisher_p(0, 3, 4, 4), 1.0 / 35.0), "{}", fisher_p(0, 3, 4, 4));
        assert!(close(fisher_p(4, 7, 6, 18), 0.378), "{}", fisher_p(4, 7, 6, 18));
        assert!(close(fisher_p(2, 12, 9, 14), 0.021), "{}", fisher_p(2, 12, 9, 14));
        assert!(close(fisher_p(2, 7, 11, 11), 0.002), "{}", fisher_p(2, 7, 11, 11));
        assert!(close(fisher_p(3, 6, 3, 6), 1.0));
        assert_eq!(fisher_p(0, 0, 3, 5), 1.0, "an empty half says nothing");
    }

    #[test]
    fn habit_changes_need_five_tasks_a_half_and_more_than_chance() {
        let m = mv("written_brief", 3, &["F7"], "code_change");
        let yes = facts(&[("F7", true)], None);
        let no = facts(&[("F7", false)], None);
        let mut ts = vec![];
        // feature: 2 of 12 before, 9 of 14 now (p = 0.021) -> growing.
        for i in 0..12 {
            ts.push(task(
                &format!("fe{i}"),
                E + 1 + i,
                "feature",
                1,
                "judged",
                true,
                if i < 2 { &yes } else { &no },
                None,
            ));
        }
        for i in 0..14 {
            ts.push(task(&format!("fr{i}"), 1 + i, "feature", 1, "judged", true, if i < 9 { &yes } else { &no }, None));
        }
        // fix_bug: 3 of 4 before, 0 of 6 now -> too few before.
        for i in 0..4 {
            ts.push(task(
                &format!("be{i}"),
                E + 1 + i,
                "fix_bug",
                1,
                "judged",
                true,
                if i < 3 { &yes } else { &no },
                None,
            ));
        }
        for i in 0..6 {
            ts.push(task(&format!("br{i}"), 1 + i, "fix_bug", 1, "judged", true, &no, None));
        }
        // tests: 3 of 6 then 4 of 6 -> chance.
        for i in 0..6 {
            ts.push(task(
                &format!("te{i}"),
                E + 1 + i,
                "tests",
                1,
                "judged",
                true,
                if i < 3 { &yes } else { &no },
                None,
            ));
            ts.push(task(&format!("tr{i}"), 1 + i, "tests", 1, "judged", true, if i < 4 { &yes } else { &no }, None));
        }
        // Uncertain and unknown facts are not tasks where it fits.
        ts.push(task("u", 2, "feature", 1, "uncertain", false, &yes, None));
        ts.push(task("k", 2, "feature", 1, "judged", true, &facts(&[], None), None));
        let got = habit_changes(&ts, std::slice::from_ref(&m), NOW);
        assert_eq!(got.len(), 1, "{got:?}");
        let h = &got[0];
        assert_eq!(
            (h.task_type.as_str(), h.label, h.up, h.earlier_seen, h.earlier_eligible, h.recent_seen, h.recent_eligible),
            ("feature", "Adding a feature", true, 2, 12, 9, 14)
        );
        // Reversed in time it is "less often lately".
        let flip: Vec<JudgedTask> = ts
            .iter()
            .filter(|t| t.task_type.as_deref() == Some("feature"))
            .map(|t| JudgedTask { first_ms: NOW - (2 * E * DAY_MS - (NOW - t.first_ms)) + 1, ..t.clone() })
            .collect();
        let down = habit_changes(&flip, &[m], NOW);
        assert_eq!(down.len(), 1);
        assert!(!down[0].up && down[0].earlier_seen == 9 && down[0].recent_seen == 2, "{down:?}");
    }

    #[test]
    fn progress_needs_the_repeated_kind_minimum_in_both_halves() {
        let mut ts = vec![at("e0", E + 6, "tests", 1, "judged"), at("e1", E + 7, "tests", 1, "judged")];
        ts.push(at("e2", E + 8, "tests", 1, "uncertain"));
        for i in 0..4 {
            ts.push(at(&format!("r{i}"), 1 + i, "tests", 3, "judged"));
        }
        assert_eq!(
            progress(&ts, NOW, &params())[0].trend,
            Trend::TooFew,
            "an uncertain task does not make up the count"
        );
        ts.push(at("e3", E + 9, "tests", 1, "judged"));
        assert_eq!(progress(&ts, NOW, &params())[0].trend, Trend::Up);
        let strict = CoachParams { recurring_min: 4, ..params() };
        assert_eq!(progress(&ts, NOW, &strict)[0].trend, Trend::TooFew, "the minimum is the tips' recurring_min");
        assert!(progress(&[], NOW, &params()).is_empty());
    }

    #[test]
    fn a_rung_is_reached_with_three_tasks_at_it_within_30_days() {
        let mut ts = vec![
            at("a0", 80, "fix_bug", 1, "judged"),
            // One Delegated task, then two more within 30 days of the second: reached on day 10.
            at("d0", 70, "fix_bug", 3, "judged"),
            at("d1", 35, "fix_bug", 3, "judged"),
            at("d2", 20, "fix_bug", 3, "judged"),
            at("d3", 10, "fix_bug", 3, "judged"),
            // Directed is reached on the same tasks that count for Delegated (rung or higher).
            at("u0", 12, "fix_bug", 5, "uncertain"),
            // Two at Directed, one uncertain: not reached.
            at("t0", 9, "tests", 2, "judged"),
            at("t1", 8, "tests", 2, "judged"),
            at("t2", 7, "tests", 2, "uncertain"),
            // Three Delegated tasks spread wider than 30 days: not reached.
            at("r0", 90, "refactor", 3, "judged"),
            at("r1", 55, "refactor", 3, "judged"),
            at("r2", 20, "refactor", 3, "judged"),
            // The fallback kind never counts.
            at("o0", 3, "other", 4, "judged"),
            at("o1", 2, "other", 4, "judged"),
            at("o2", 1, "other", 4, "judged"),
        ];
        let got: Vec<_> = milestones(&ts, NOW, &params())
            .into_iter()
            .map(|m| (m.task_type, m.kind.as_str(), m.rung, (NOW - m.earned_ms) / DAY_MS, (NOW - m.first_ms) / DAY_MS))
            .collect();
        assert_eq!(
            got,
            vec![("fix_bug".to_owned(), "reached", 3, 10, 70), ("fix_bug".to_owned(), "reached", 2, 10, 70),]
        );
        // A later quiet month never takes it away.
        for i in 0..10 {
            ts.push(at(&format!("q{i}"), 1 + i % 5, "fix_bug", 1, "judged"));
        }
        assert_eq!(milestones(&ts, NOW, &params()).iter().filter(|m| m.task_type == "fix_bug").count(), 2);
        assert!(milestones(&[], NOW, &params()).is_empty());
    }

    #[test]
    fn a_move_that_holds_today_on_a_short_history_is_a_milestone() {
        // 45 days of history: the move shows on Home today, so the record has it too (earned today).
        let mut ts = vec![];
        for d in [36, 38, 40, 42, 44] {
            ts.push(at(&format!("e{d}"), d, "tests", 2, "judged"));
        }
        for d in [1, 3, 5, 7, 9] {
            ts.push(at(&format!("r{d}"), d, "tests", 3, "judged"));
        }
        assert_eq!(progress(&ts, NOW, &params())[0].trend, Trend::Up);
        let m: Vec<_> =
            milestones(&ts, NOW, &params()).into_iter().filter(|m| m.kind == MilestoneKind::MovedUp).collect();
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!((m[0].from_rung, m[0].rung), (Some(2), 3));
        assert!(m[0].earned_ms <= NOW && m[0].earned_ms > NOW - 9 * DAY_MS, "the first day it held: {:?}", m[0]);
    }

    #[test]
    fn the_note_names_the_newest_unseen_milestone_from_the_last_week() {
        let m = |kind, rung, ago: i64| Milestone {
            job: "code_change".into(),
            task_type: "fix_bug".into(),
            label: "Fixing a bug or failing test",
            kind,
            rung,
            from_rung: None,
            earned_ms: NOW - ago * DAY_MS,
            first_ms: NOW - 40 * DAY_MS,
        };
        let ms = vec![
            m(MilestoneKind::Reached, 2, 2),
            m(MilestoneKind::Reached, 3, 2),
            m(MilestoneKind::MovedUp, 2, 2),
            m(MilestoneKind::Reached, 3, 9),
        ];
        let n = milestone_to_note(&ms, 0, NOW).unwrap();
        assert_eq!((n.kind, n.rung), (MilestoneKind::MovedUp, 2), "a move beats a rung reached the same day");
        assert_eq!(milestone_words(n), "Fixing a bug or failing test moved up to Directed");
        assert_eq!(milestone_words(&ms[1]), "Fixing a bug or failing test reached Delegated");
        assert!(milestone_to_note(&ms, NOW - 2 * DAY_MS, NOW).is_none(), "said once: nothing after the seen mark");
        assert!(milestone_to_note(&ms[3..], 0, NOW).is_none(), "older than a week: only on the page");
    }

    #[test]
    fn near_milestones_name_the_next_rung_each_kind_is_partway_to() {
        let ts = vec![
            // fix_bug reached Delegated long ago; 2 Orchestrated tasks lately -> 2 of 3 at Orchestrated.
            at("f0", 90, "fix_bug", 3, "judged"),
            at("f1", 85, "fix_bug", 3, "judged"),
            at("f2", 80, "fix_bug", 3, "judged"),
            at("f3", 5, "fix_bug", 4, "judged"),
            at("f4", 4, "fix_bug", 4, "judged"),
            // tests reached nothing; one Directed lately -> 1 of 3 at Directed.
            at("t0", 3, "tests", 2, "judged"),
            at("t1", 2, "tests", 1, "judged"),
            // feature: Directed only before the last 30 days -> not near.
            at("e0", 40, "feature", 2, "judged"),
            at("e1", 3, "feature", 1, "judged"),
            // refactor: uncertain doesn't count.
            at("r0", 3, "refactor", 2, "uncertain"),
        ];
        let ms = milestones(&ts, NOW, &params());
        let got: Vec<_> =
            near_milestones(&ts, &ms, NOW, 3).into_iter().map(|n| (n.task_type, n.rung, n.have)).collect();
        assert_eq!(got, vec![("fix_bug".to_owned(), 4, 2), ("tests".to_owned(), 2, 1)]);
        assert_eq!(near_milestones(&ts, &ms, NOW, 1).len(), 1);
    }

    #[test]
    fn a_typical_rung_move_is_kept_on_the_first_day_it_held() {
        let mut ts = vec![];
        // Assisted for 30 days, then Delegated for 30 days: the progress rule says up once both halves hold
        // enough, and the milestone keeps that first day, not today.
        for i in 0..6 {
            ts.push(at(&format!("e{i}"), 2 * E + 20 - i * 4, "feature", 1, "judged"));
            ts.push(at(&format!("r{i}"), E + 20 - i * 4, "feature", 3, "judged"));
        }
        let ms = milestones(&ts, NOW, &params());
        let moved: Vec<_> = ms.iter().filter(|m| m.kind == MilestoneKind::MovedUp).collect();
        assert_eq!(moved.len(), 1, "{ms:?}");
        let m = moved[0];
        assert_eq!((m.rung, m.from_rung, m.label), (3, Some(1), "Adding a feature"));
        let days_ago = (NOW - m.earned_ms) / DAY_MS;
        assert!(days_ago > 0 && days_ago < E + 20, "after the first Delegated feature, before today: {days_ago}");
        let day_before = NOW - (days_ago + 1) * DAY_MS;
        assert!(progress(&ts, day_before, &params()).iter().all(|r| r.trend != Trend::Up), "not held the day before");
        assert_eq!(m.first_ms, NOW - (E + 20) * DAY_MS, "first Delegated feature");
        assert!(progress(&ts, NOW, &params()).iter().all(|r| r.trend != Trend::Up), "no longer comparable today");
    }

    #[test]
    fn after_a_tip_counts_the_next_tasks_where_the_habit_fits() {
        let m = mv("check_code", 1, &["F6"], "code_change");
        let yes = facts(&[("F6", true)], None);
        let no = facts(&[("F6", false)], None);
        let ts = vec![
            task("before", 9, "tests", 1, "judged", true, &yes, None),
            task("a", 5, "tests", 1, "judged", true, &yes, None),
            task("unknown", 4, "tests", 1, "judged", true, &facts(&[], None), None),
            task("unsure", 4, "tests", 1, "uncertain", false, &yes, None),
            task("b", 3, "tests", 1, "judged", true, &no, None),
            task("c", 2, "tests", 1, "judged", true, &yes, None),
            task("d", 1, "tests", 1, "judged", true, &no, None),
        ];
        let at = NOW - 6 * DAY_MS;
        // a, b, c: the first three after the tip with F6 known; d is past the three.
        assert_eq!(after_tip(&ts, &m, at, AFTER_TIP_TASKS), (2, 3));
        assert_eq!(after_tip(&ts, &m, NOW, AFTER_TIP_TASKS), (0, 0));
    }
}
