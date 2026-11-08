//! The tip funnel (design `docs/design/tip-funnel.md`): where each new task's tip went, counted from stored rows.
//! Pure: rows and the time come in, the counts go out. Built so a stretch with no tips is visible (2026-11-04 to
//! 11-08 AIQ showed no tip at all and nothing said so).
use crate::clock::{DAY_MS, Now, local_day};
use aiq_contracts::frames::{FunnelCount, TipDrought, TipFunnel};
use std::collections::HashMap;

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
        "label_unsure" => "kind label not sure enough",
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
        None => v.push(FunnelCount { code: code.to_owned(), words: words(code).to_owned(), count: 1 }),
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
    for (_, last) in latest_steps(steps, since_ms).values() {
        f.tasks += 1;
        let step = Step::parse(&last.step);
        if step.is_some_and(Step::kind_known) {
            f.kind_known += 1;
        }
        if step == Some(Step::Decided) {
            f.tip_found += 1;
        } else {
            bump(&mut f.no_tip, &last.step);
        }
    }
    for c in cards.iter().filter(|c| c.first_utc_ms >= since_ms) {
        f.tips += 1;
        if let Some(on) = &c.shown_on {
            match f.shown.iter_mut().find(|s| &s.code == on) {
                Some(s) => s.count += 1,
                None => f.shown.push(FunnelCount { code: on.clone(), words: tool_words(on).to_owned(), count: 1 }),
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
        }
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
