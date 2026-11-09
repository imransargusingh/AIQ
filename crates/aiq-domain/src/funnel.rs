//! The tip funnel (design `docs/design/tip-funnel.md`): where each new task's tip went, counted from stored rows.
//! Pure: rows and the time come in, the counts go out. Built so a stretch with no tips is visible (2026-11-04 to
//! 11-08 AIQ showed no tip at all and nothing said so).
use crate::clock::{DAY_MS, Now, local_day};
use aiq_contracts::frames::{FunnelCount, FunnelTask, TipDrought, TipFunnel};
use std::collections::HashMap;
use std::collections::HashSet;

/// One point where a live new task's tip path ended or began to wait (`tip_path` row). Each exit of the core's
/// next-step path writes exactly one of these; `LabelAsked` is followed by the label's own end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    /// A tip was decided for the task (shown, held or expired: see card_decision).
    Decided,
    /// The kind is known and AIQ holds its next-step tip on purpose (coaching-from-rungs D6), by the kind's state:
    /// fewer than the minimum analyzed tasks, most already at their right rung, the next step blocked, AIQ not
    /// sure, or flagged "This doesn't fit".
    KindTooFew,
    KindRightRung,
    KindBlocked,
    KindNotSure,
    KindFlagged,
    /// The kind is known, but none of the person's analyzed tasks is of this kind yet.
    KindNew,
    /// The kind coaches a move the pack doesn't have (a pack defect; pack tests should stop it).
    NoTipForKind,
    /// A kind label named the task, but the label self-check (8 in 10) holds label tips.
    LabelHeld,
    /// Word matching is held by its self-check and labels can't run.
    GuessHeld,
    /// No word match, and labels can't run (background analysis or labels off, or a tool without labels).
    NoMatch,
    /// Waiting on a kind label. As the latest step, the label never came back.
    LabelAsked,
    /// The label call failed (error, timeout, not signed in).
    LabelFailed,
    /// The label answered, not sure enough to coach from.
    LabelUnsure,
    /// The label was sure, but of a job labels never coach from (`label::NOT_COACHED_JOBS`: questions).
    LabelNotCoached,
    /// The first prompt's label was not sure enough, so a later, fuller prompt of the same task was labelled
    /// (once per task). Followed by that label's own end; as the latest step, the second label never came back.
    LabelRetry,
    /// The label came back after the session had moved on to another task.
    LabelLate,
    /// Next-step tips are turned off in Settings.
    TipsOff,
    /// The session was first set aside as automated; a person's second prompt brought it back too late for a tip.
    LateFirstPrompt,
}

pub const ALL: [Step; 19] = [
    Step::Decided,
    Step::KindTooFew,
    Step::KindRightRung,
    Step::KindBlocked,
    Step::KindNotSure,
    Step::KindFlagged,
    Step::KindNew,
    Step::NoTipForKind,
    Step::LabelHeld,
    Step::GuessHeld,
    Step::NoMatch,
    Step::LabelAsked,
    Step::LabelFailed,
    Step::LabelUnsure,
    Step::LabelNotCoached,
    Step::LabelRetry,
    Step::LabelLate,
    Step::TipsOff,
    Step::LateFirstPrompt,
];

impl Step {
    pub fn as_str(self) -> &'static str {
        match self {
            Step::Decided => "decided",
            Step::KindTooFew => "kind_too_few",
            Step::KindRightRung => "kind_right_rung",
            Step::KindBlocked => "kind_blocked",
            Step::KindNotSure => "kind_not_sure",
            Step::KindFlagged => "kind_flagged",
            Step::KindNew => "kind_new",
            Step::NoTipForKind => "no_tip_for_kind",
            Step::LabelHeld => "label_held",
            Step::GuessHeld => "guess_held",
            Step::NoMatch => "no_match",
            Step::LabelAsked => "label_asked",
            Step::LabelFailed => "label_failed",
            Step::LabelUnsure => "label_unsure",
            Step::LabelNotCoached => "label_not_coached",
            Step::LabelRetry => "label_retry",
            Step::LabelLate => "label_late",
            Step::TipsOff => "tips_off",
            Step::LateFirstPrompt => "late_first_prompt",
        }
    }

    pub fn parse(s: &str) -> Option<Step> {
        ALL.into_iter().find(|x| x.as_str() == s)
    }

    /// The kind of task is known at this step.
    pub fn kind_known(self) -> bool {
        !matches!(
            self,
            Step::GuessHeld
                | Step::NoMatch
                | Step::LabelAsked
                | Step::LabelFailed
                | Step::LabelUnsure
                | Step::LabelNotCoached
                | Step::LabelRetry
                | Step::LabelLate
                | Step::TipsOff
                | Step::LateFirstPrompt
        )
    }

    /// The step for a known kind whose coaching row is in `state` (None: no row for this kind yet).
    pub fn for_kind(state: Option<crate::coach::RowState>) -> Step {
        use crate::coach::RowState::*;
        match state {
            None => Step::KindNew,
            Some(Coach) => Step::NoTipForKind,
            Some(TooFew) => Step::KindTooFew,
            Some(RightRung) => Step::KindRightRung,
            Some(Blocked) => Step::KindBlocked,
            Some(NotSure) => Step::KindNotSure,
            Some(Flagged) => Step::KindFlagged,
        }
    }
}

/// Plain words for a step code or a card decision's reason code, for `aiq status` and Home. One definition.
pub fn words(code: &str) -> &'static str {
    match code {
        // Why a task got no tip.
        "kind_too_few" => "too few tasks of this kind analyzed yet",
        "kind_right_rung" => "you already work this kind at its right rung",
        "kind_blocked" => "this kind's next step is blocked outside your control",
        "kind_not_sure" => "AIQ isn't sure yet what this kind's next step is",
        "kind_flagged" => "you said this kind's tip doesn't fit",
        "kind_new" => "first task of this kind",
        "no_tip_for_kind" => "this kind's tip is missing",
        "label_held" => "kind labels on hold: their accuracy check is under 8 in 10",
        "guess_held" => "word matching on hold: its accuracy check is under 8 in 10",
        "no_match" => "kind of task not recognised",
        "label_asked" => "kind label never came back",
        "label_failed" => "kind label failed",
        "label_unsure" => "AIQ wasn't sure what kind of task it was",
        "label_not_coached" => "kind label says it's a question; labels don't give tips for questions",
        "label_retry" => "kind label asked again from a later prompt, and it never came back",
        "label_late" => "kind label came after the next task started",
        "tips_off" => "next-step tips turned off",
        "late_first_prompt" => "started as automated, counted later",
        // Why a decided tip was held (card_decision reason codes).
        "daily_cap" => "daily tip limit reached",
        "quiet_period" => "quiet time after the last tip",
        "cooldown" => "same tip shown in the last 72 hours",
        "accuracy_paused" => "this kind's accuracy check is under 8 in 10",
        "session_budget" => "one tip per session already shown",
        "one_at_a_time" => "another tip was already waiting",
        "low_confidence" => "not sure enough",
        "job_mismatch" => "doesn't fit this kind of work",
        "tip_off" => "you turned this tip off",
        "snoozed" => "you snoozed this tip",
        "habit_adopted" => "you already do this",
        "paused" => "AIQ paused",
        "expired" => "ready, but the session ended first",
        _ => "other",
    }
}

/// What helps, for the no-tip reasons a person can act on or should not worry about; empty for the rest. Shown
/// under the reason on Home. Coaching tone: what to try, never what went wrong.
pub fn help(code: &str) -> &'static str {
    match code {
        "no_match" | "label_unsure" | "label_retry" => {
            "AIQ reads how a task starts. Opening with one sentence that names the job, like \"Fix the login crash when the token expires\", helps it find a tip."
        }
        "label_held" | "guess_held" => {
            "AIQ holds these while it rechecks its own accuracy. Nothing to do on your side."
        }
        "kind_too_few" | "kind_new" => "Tips for a kind start once AIQ has analyzed a few tasks of it.",
        "tips_off" => "Next-step tips can be turned on in Settings.",
        _ => "",
    }
}

/// The first words of an opening prompt for the no-tip list: secrets removed, whitespace collapsed, at most
/// `OPENING_WORDS` words and `OPENING_CHARS` characters, "…" when cut. Enough to recognise the task and see how it
/// began; the task page has the rest.
pub fn opening(text: &str) -> String {
    let (clean, _) = crate::secrets::redact(text);
    let words: Vec<&str> = clean.split_whitespace().collect();
    let mut out = String::new();
    let mut cut = words.len() > OPENING_WORDS;
    for w in words.iter().take(OPENING_WORDS) {
        let next = if out.is_empty() { w.chars().count() } else { out.chars().count() + 1 + w.chars().count() };
        if next > OPENING_CHARS {
            if out.is_empty() {
                out = w.chars().take(OPENING_CHARS).collect();
            }
            cut = true;
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    if cut {
        out.push('…');
    }
    out
}

/// What a failed kind-label call ran into (a `kind_label.outcome` other than `ok`), in plain words naming the tool
/// that made the call. Shown beside "Kind label failed" so the cause is visible without opening label.log.
pub fn label_failure_words(outcome: &str, provider: &str) -> String {
    let tool = tool_words(provider);
    match outcome {
        "not_signed_in" => format!("{tool} isn't signed in for AIQ's label call"),
        "missing" => format!("AIQ couldn't find {tool} to run the label call"),
        "timeout" => format!("the label call to {tool} took too long"),
        "schema" => format!("{tool}'s label answer couldn't be read"),
        "error" => format!("the label call to {tool} returned an error"),
        _ => String::new(),
    }
}

pub const OPENING_WORDS: usize = 8;
pub const OPENING_CHARS: usize = 60;

/// At most this many no-tip tasks are listed (newest first); the counts still cover them all.
pub const LOST_MAX: usize = 50;

/// The tool a tip showed in, in plain words.
pub fn tool_words(surface: &str) -> &str {
    match surface {
        "claude-code" => "Claude Code",
        "copilot-cli" => "Copilot CLI",
        "vscode" => "VS Code",
        other => other,
    }
}

/// A `tip_path` row as the store returns it.
#[derive(Debug, Clone, PartialEq)]
pub struct StepRow {
    pub task_id: String,
    pub surface: String,
    pub step: String,
    pub utc_ms: i64,
    pub offset_min: i32,
}

/// One card's life as the store returns it: first decision, latest decision, delivery, open and answer.
#[derive(Debug, Clone, PartialEq)]
pub struct CardRow {
    pub card_id: String,
    pub first_utc_ms: i64,
    pub outcome: String,
    pub reason: String,
    /// Where it was handed over, if it was.
    pub shown_on: Option<String>,
    pub opened: bool,
    pub answered: bool,
    /// The task the card's signal fired on, and whether it is a next-step tip (`coach::DETECTOR`) rather than one
    /// that fired mid-task.
    pub task_id: Option<String>,
    pub next_step: bool,
}

/// A delivery's local day (for the drought check), as the store returns it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeliveryDay {
    pub utc_ms: i64,
    pub offset_min: i32,
}

/// A day of work with at least this many new tasks and no tip shown is a drought.
/// assume: 3 tasks is "a day of work" (not measured; the laptop averaged ~10 new tasks a working day 11-01..11-04,
/// from the 30-day rung counts). If wrong, a light day warns needlessly (too low) or a dry day hides (too high).
pub const DROUGHT_MIN_TASKS: u32 = 3;

fn bump(v: &mut Vec<FunnelCount>, code: &str) {
    match v.iter_mut().find(|c| c.code == code) {
        Some(c) => c.count += 1,
        None => v.push(FunnelCount {
            code: code.to_owned(),
            words: words(code).to_owned(),
            count: 1,
            help: help(code).to_owned(),
        }),
    }
}

fn sorted(mut v: Vec<FunnelCount>) -> Vec<FunnelCount> {
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.code.cmp(&b.code)));
    v
}

/// Latest step per task among `steps` (in insert order), kept only for tasks first seen at or after `since_ms`.
fn latest_steps(steps: &[StepRow], since_ms: i64) -> HashMap<&str, (&StepRow, &StepRow)> {
    let mut by_task: HashMap<&str, (&StepRow, &StepRow)> = HashMap::new();
    for s in steps {
        by_task.entry(&s.task_id).and_modify(|(_, last)| *last = s).or_insert((s, s));
    }
    by_task.retain(|_, (first, _)| first.utc_ms >= since_ms);
    by_task
}

/// The funnel of tasks first seen and tips first decided at or after `since_ms`.
pub fn build(steps: &[StepRow], cards: &[CardRow], since_ms: i64, days: u32) -> TipFunnel {
    let mut f = TipFunnel { days, ..TipFunnel::default() };
    let mut decided: HashSet<&str> = HashSet::new();
    for (task, (first, last)) in latest_steps(steps, since_ms) {
        f.tasks += 1;
        let step = Step::parse(&last.step);
        let known = step.is_some_and(Step::kind_known);
        if known {
            f.kind_known += 1;
        }
        if step == Some(Step::Decided) {
            f.tip_found += 1;
            decided.insert(task);
        } else {
            bump(&mut f.no_tip, &last.step);
            f.lost.push(FunnelTask {
                task_id: task.to_owned(),
                surface: first.surface.clone(),
                first_ms: first.utc_ms,
                code: last.step.clone(),
                words: words(&last.step).to_owned(),
                kind_known: known,
                opening: String::new(),
                detail: String::new(),
            });
        }
    }
    f.lost.sort_by(|a, b| b.first_ms.cmp(&a.first_ms).then_with(|| a.task_id.cmp(&b.task_id)));
    f.lost.truncate(LOST_MAX);
    // A task's next-step tip counts once at Shown and Opened, so those bars never pass Tip ready.
    let (mut shown_tasks, mut opened_tasks): (HashSet<&str>, HashSet<&str>) = (HashSet::new(), HashSet::new());
    for c in cards.iter().filter(|c| c.first_utc_ms >= since_ms) {
        f.tips += 1;
        if c.shown_on.is_some() {
            match c.task_id.as_deref().filter(|t| c.next_step && decided.contains(t)) {
                Some(t) => {
                    shown_tasks.insert(t);
                    if c.opened {
                        opened_tasks.insert(t);
                    }
                }
                None => f.other_shown += 1,
            }
        }
        if let Some(on) = &c.shown_on {
            match f.shown.iter_mut().find(|s| &s.code == on) {
                Some(s) => s.count += 1,
                None => f.shown.push(FunnelCount {
                    code: on.clone(),
                    words: tool_words(on).to_owned(),
                    count: 1,
                    help: String::new(),
                }),
            }
            f.opened += c.opened as u32;
            f.answered += c.answered as u32;
        } else if c.reason == "expired" {
            f.expired += 1;
        } else if c.outcome == "show_ready" {
            f.waiting += 1;
        } else {
            bump(&mut f.held, &c.reason);
        }
    }
    f.tip_shown = shown_tasks.len() as u32;
    f.tip_opened = opened_tasks.len() as u32;
    f.no_tip = sorted(f.no_tip);
    f.held = sorted(f.held);
    f.shown = sorted(f.shown);
    f
}

/// Local days before today, within the last `days`, with at least `DROUGHT_MIN_TASKS` new tasks and no tip shown,
/// newest first. Today is left out: the day isn't over. Each names the most common reason its tasks got no tip.
pub fn droughts(steps: &[StepRow], deliveries: &[DeliveryDay], now: Now, days: u32) -> Vec<TipDrought> {
    let today = now.local_day();
    let since = now.local_midnight_utc_ms() - days as i64 * DAY_MS - DAY_MS; // a day of slack for offset changes
    let shown: std::collections::HashSet<i64> = deliveries.iter().map(|d| local_day(d.utc_ms, d.offset_min)).collect();
    let mut per_day: HashMap<i64, Vec<&StepRow>> = HashMap::new();
    for (first, last) in latest_steps(steps, since).values() {
        per_day.entry(local_day(first.utc_ms, first.offset_min)).or_default().push(last);
    }
    let mut out: Vec<TipDrought> = per_day
        .into_iter()
        .filter(|(d, tasks)| {
            *d < today && *d >= today - days as i64 && !shown.contains(d) && tasks.len() as u32 >= DROUGHT_MIN_TASKS
        })
        .map(|(d, tasks)| {
            let mut why = Vec::new();
            tasks.iter().for_each(|s| bump(&mut why, &s.step));
            TipDrought { day: iso_date(d), tasks: tasks.len() as u32, main_reason: sorted(why).into_iter().next() }
        })
        .collect();
    out.sort_by(|a, b| b.day.cmp(&a.day));
    out
}

/// `YYYY-MM-DD` of a day number (days since 1970-01-01), proleptic Gregorian (H. Hinnant's civil_from_days).
pub fn iso_date(day: i64) -> String {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;
    // 2026-11-08T12:00Z, offset -480 (Pacific): local day is 2026-11-08.
    const NOW: Now = Now { utc_ms: 1_794_139_200_000, offset_min: -480 };

    fn step(task: &str, step: Step, utc_ms: i64) -> StepRow {
        StepRow {
            task_id: task.into(),
            surface: "copilot-cli".into(),
            step: step.as_str().into(),
            utc_ms,
            offset_min: -480,
        }
    }

    fn card(id: &str, outcome: &str, reason: &str, shown_on: Option<&str>) -> CardRow {
        CardRow {
            card_id: id.into(),
            first_utc_ms: NOW.utc_ms - H,
            outcome: outcome.into(),
            reason: reason.into(),
            shown_on: shown_on.map(Into::into),
            opened: false,
            answered: false,
            task_id: None,
            next_step: false,
        }
    }

    fn tip_for(task: &str, id: &str, shown_on: Option<&str>) -> CardRow {
        CardRow { task_id: Some(task.into()), next_step: true, ..card(id, "show_ready", "ok", shown_on) }
    }

    #[test]
    fn every_step_round_trips_and_has_words() {
        for s in ALL {
            assert_eq!(Step::parse(s.as_str()), Some(s));
            if s != Step::Decided {
                assert_ne!(words(s.as_str()), "other", "{} has no words", s.as_str());
            }
        }
    }

    #[test]
    fn every_hold_reason_has_words() {
        use crate::trigger::Reason::*;
        for r in [
            Paused,
            JobMismatch,
            LowConfidence,
            SessionBudget,
            OneAtATime,
            DailyCap,
            Expired,
            QuietPeriod,
            TipOff,
            Snoozed,
            Cooldown,
            HabitAdopted,
            AccuracyPaused,
        ] {
            assert_ne!(words(r.as_str()), "other", "{} has no words", r.as_str());
        }
    }

    #[test]
    fn every_kind_state_maps_to_its_own_step() {
        use crate::coach::RowState::*;
        let steps: Vec<Step> =
            [None, Some(Coach), Some(TooFew), Some(RightRung), Some(Blocked), Some(NotSure), Some(Flagged)]
                .into_iter()
                .map(Step::for_kind)
                .collect();
        let unique: std::collections::HashSet<_> = steps.iter().collect();
        assert_eq!(unique.len(), steps.len());
        assert!(steps.iter().all(|s| s.kind_known() && *s != Step::Decided));
    }

    #[test]
    fn iso_date_matches_known_days() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(NOW.local_day()), "2026-11-08");
        assert_eq!(iso_date(-1), "1969-12-31");
        assert_eq!(iso_date(11_016), "2000-02-29");
    }

    #[test]
    fn each_no_tip_step_is_counted_under_its_own_code() {
        let t = NOW.utc_ms - H;
        let steps: Vec<StepRow> = ALL.iter().enumerate().map(|(i, s)| step(&format!("t{i}"), *s, t)).collect();
        let f = build(&steps, &[], NOW.utc_ms - 24 * H, 1);
        assert_eq!(f.tasks, ALL.len() as u32);
        assert_eq!(f.kind_known, 9, "decided, the six kind states, missing tip, label held");
        assert_eq!(f.tip_found, 1);
        assert_eq!(f.no_tip.len(), ALL.len() - 1);
        assert!(f.no_tip.iter().all(|c| c.count == 1 && c.words != "other"));
        assert!(!f.no_tip.iter().any(|c| c.code == "decided"));
    }

    #[test]
    fn the_latest_step_wins_and_a_task_counts_once() {
        let t = NOW.utc_ms - 2 * H;
        let steps = [step("a", Step::LabelAsked, t), step("a", Step::LabelHeld, t + 1000)];
        let f = build(&steps, &[], NOW.utc_ms - 24 * H, 1);
        assert_eq!((f.tasks, f.kind_known), (1, 1));
        assert_eq!(f.no_tip[0].code, "label_held");
    }

    #[test]
    fn a_label_that_never_came_back_stays_label_asked() {
        let f = build(&[step("a", Step::LabelAsked, NOW.utc_ms - H)], &[], 0, 1);
        assert_eq!(f.no_tip[0].code, "label_asked");
        assert_eq!(f.kind_known, 0);
    }

    #[test]
    fn tasks_and_tips_before_the_period_are_left_out() {
        let old = NOW.utc_ms - 48 * H;
        let mut c = card("old", "show_ready", "ok", Some("vscode"));
        c.first_utc_ms = old;
        let f = build(&[step("a", Step::NoMatch, old)], &[c], NOW.utc_ms - 24 * H, 1);
        assert_eq!((f.tasks, f.tips), (0, 0));
    }

    #[test]
    fn tips_split_into_shown_per_surface_held_by_reason_waiting_and_expired() {
        let mut opened = card("c1", "show_ready", "ok", Some("copilot-cli"));
        opened.opened = true;
        opened.answered = true;
        let cards = [
            opened,
            card("c2", "show_ready", "ok", Some("copilot-cli")),
            card("c3", "show_ready", "ok", Some("vscode")),
            card("c4", "hold", "daily_cap", None),
            card("c5", "hold", "quiet_period", None),
            card("c6", "drop", "cooldown", None),
            card("c7", "drop", "accuracy_paused", None),
            card("c8", "drop", "expired", None),
            card("c9", "show_ready", "ok", None),
        ];
        let f = build(&[], &cards, 0, 7);
        assert_eq!(f.tips, 9);
        assert_eq!(
            f.shown.iter().map(|s| (s.code.as_str(), s.count)).collect::<Vec<_>>(),
            [("copilot-cli", 2), ("vscode", 1)]
        );
        assert_eq!((f.opened, f.answered), (1, 1));
        assert_eq!((f.expired, f.waiting), (1, 1));
        let held: Vec<_> = f.held.iter().map(|h| h.code.as_str()).collect();
        assert_eq!(held, ["accuracy_paused", "cooldown", "daily_cap", "quiet_period"]);
    }

    #[test]
    fn shown_and_opened_follow_the_same_tasks_so_they_never_pass_tip_ready() {
        // The 2026-11-09 laptop case: 6 new tasks, none with a tip, and one tip shown that week for a task from
        // before the funnel counted. Home said "0 had a tip ready" next to "Shown 1".
        let t = NOW.utc_ms - 3 * H;
        let mut steps: Vec<StepRow> = (0..6).map(|i| step(&format!("n{i}"), Step::LabelUnsure, t + i)).collect();
        steps.push(step("d", Step::Decided, t + 10));
        steps.push(step("e", Step::Decided, t + 11));
        let mut opened = tip_for("d", "c1", Some("copilot-cli"));
        opened.opened = true;
        let mid_task = CardRow { next_step: false, ..tip_for("n0", "c3", Some("vscode")) };
        let cards = [
            opened,
            tip_for("d", "c2", Some("copilot-cli")), // shown again: still one task
            mid_task,
            tip_for("old-task", "c4", Some("copilot-cli")),
            tip_for("e", "c5", None), // held
        ];
        let f = build(&steps, &cards, NOW.utc_ms - 24 * H, 1);
        assert_eq!((f.tasks, f.tip_found, f.tip_shown, f.tip_opened), (8, 2, 1, 1));
        assert_eq!(f.other_shown, 2, "the mid-task tip and the older task's tip; a re-show is the same task's tip");
        assert_eq!(f.shown.iter().map(|c| c.count).sum::<u32>(), 4, "every shown tip still counts by tool");
        assert!(f.tip_shown <= f.tip_found && f.tip_opened <= f.tip_shown);
    }

    #[test]
    fn lost_tasks_are_listed_newest_first_with_their_reason_and_stage() {
        let t = NOW.utc_ms - 3 * H;
        let steps = [
            step("a", Step::LabelAsked, t),
            step("a", Step::LabelUnsure, t + 5),
            step("b", Step::KindTooFew, t + 1),
            step("c", Step::Decided, t + 2),
        ];
        let f = build(&steps, &[], NOW.utc_ms - 24 * H, 1);
        let lost: Vec<_> = f.lost.iter().map(|l| (l.task_id.as_str(), l.code.as_str(), l.kind_known)).collect();
        assert_eq!(lost, [("b", "kind_too_few", true), ("a", "label_unsure", false)]);
        assert_eq!(f.lost[1].first_ms, t, "a task's time is when it began");
        assert_eq!(f.lost[1].words, "AIQ wasn't sure what kind of task it was");
        let many: Vec<_> = (0..LOST_MAX + 5).map(|i| step(&format!("x{i}"), Step::NoMatch, t + i as i64)).collect();
        let f = build(&many, &[], NOW.utc_ms - 24 * H, 1);
        assert_eq!((f.lost.len(), f.tasks as usize), (LOST_MAX, LOST_MAX + 5));
    }

    #[test]
    fn opening_keeps_the_first_words_and_never_a_secret() {
        assert_eq!(opening("fix   it\nplease"), "fix it please");
        assert_eq!(opening("one two three four five six seven eight nine"), "one two three four five six seven eight…");
        assert_eq!(opening(""), "");
        let long = "a".repeat(80);
        assert_eq!(opening(&long).chars().count(), OPENING_CHARS + 1);
        let s = opening("use token ghp_0123456789abcdefghijklmnopqrstuvwxyzAB now");
        assert!(!s.contains("ghp_0123"), "{s}");
        assert!(s.chars().count() <= OPENING_CHARS + 1);
    }

    #[test]
    fn a_failed_label_names_what_the_call_ran_into() {
        assert_eq!(
            label_failure_words("not_signed_in", "copilot-cli"),
            "Copilot CLI isn't signed in for AIQ's label call"
        );
        assert_eq!(label_failure_words("timeout", "claude-code"), "the label call to Claude Code took too long");
        for o in ["missing", "schema", "error"] {
            assert!(!label_failure_words(o, "copilot-cli").is_empty(), "{o}");
        }
        assert_eq!(label_failure_words("ok", "copilot-cli"), "");
    }

    #[test]
    fn reasons_a_person_can_act_on_carry_what_helps() {
        let f = build(&[step("a", Step::LabelUnsure, NOW.utc_ms - H)], &[], 0, 1);
        assert!(f.no_tip[0].help.starts_with("AIQ reads how a task starts."));
        assert_eq!(help("label_failed"), "");
        for code in ["no_match", "label_unsure", "label_retry", "label_held", "guess_held", "kind_too_few", "tips_off"]
        {
            let h = help(code).to_lowercase();
            assert!(!h.is_empty() && !h.contains("judg") && !h.contains("watch"), "{code}");
        }
    }

    #[test]
    fn a_working_day_with_no_tip_is_a_drought_naming_its_main_reason() {
        let yesterday = NOW.utc_ms - 24 * H;
        let steps = [
            step("a", Step::NoMatch, yesterday),
            step("b", Step::NoMatch, yesterday + 1),
            step("c", Step::LabelHeld, yesterday + 2),
        ];
        let d = droughts(&steps, &[], NOW, 7);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].day, "2026-11-07");
        assert_eq!(d[0].tasks, 3);
        assert_eq!(d[0].main_reason.as_ref().unwrap().code, "no_match");
        assert_eq!(d[0].main_reason.as_ref().unwrap().count, 2);
    }

    #[test]
    fn no_drought_when_a_tip_showed_that_day_or_the_day_was_light_or_is_today() {
        let yesterday = NOW.utc_ms - 24 * H;
        let three =
            |t: i64| [step("a", Step::NoMatch, t), step("b", Step::NoMatch, t + 1), step("c", Step::NoMatch, t + 2)];
        let shown = [DeliveryDay { utc_ms: yesterday + 5 * H, offset_min: -480 }];
        assert!(droughts(&three(yesterday), &shown, NOW, 7).is_empty(), "a tip showed");
        assert!(droughts(&three(yesterday)[..2], &[], NOW, 7).is_empty(), "two tasks is a light day");
        assert!(droughts(&three(NOW.utc_ms - H), &[], NOW, 7).is_empty(), "today isn't over");
        assert!(droughts(&three(NOW.utc_ms - 9 * 24 * H), &[], NOW, 7).is_empty(), "outside the window");
    }

    #[test]
    fn droughts_are_newest_first() {
        let day = |n: i64, id: &str| {
            let t = NOW.utc_ms - n * 24 * H;
            [
                step(&format!("{id}1"), Step::NoMatch, t),
                step(&format!("{id}2"), Step::NoMatch, t),
                step(&format!("{id}3"), Step::NoMatch, t),
            ]
        };
        let steps: Vec<_> = day(3, "x").into_iter().chain(day(1, "y")).collect();
        let d = droughts(&steps, &[], NOW, 7);
        assert_eq!(d.iter().map(|x| x.day.as_str()).collect::<Vec<_>>(), ["2026-11-07", "2026-11-05"]);
    }
}
