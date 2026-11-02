//! Trigger engine, rule `in_moment.v2` (Phase 1 table plus the Phase 2 quiet period): show-ready / hold / drop for one Signal.
use crate::job::{Job, opportunity_allowed};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    ShowReady,
    Hold,
    Drop,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::ShowReady => "show_ready",
            Outcome::Hold => "hold",
            Outcome::Drop => "drop",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Paused,
    JobMismatch,
    LowConfidence,
    SessionBudget,
    OneAtATime,
    DailyCap,
    Ok,
    Expired,
    /// A card was delivered on any surface within the quiet period (Phase 2: budget per person).
    QuietPeriod,
    /// The person said "Don't show this again" for this tip (coaching-loop US-4).
    TipOff,
    /// The person snoozed this tip.
    Snoozed,
    /// The same tip showed within its cooldown (coaching-from-rungs D12).
    Cooldown,
    /// The tip's habit is Adopted: no point coaching it (D12).
    HabitAdopted,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Paused => "paused",
            Reason::JobMismatch => "job_mismatch",
            Reason::LowConfidence => "low_confidence",
            Reason::SessionBudget => "session_budget",
            Reason::OneAtATime => "one_at_a_time",
            Reason::DailyCap => "daily_cap",
            Reason::Ok => "ok",
            Reason::Expired => "expired",
            Reason::QuietPeriod => "quiet_period",
            Reason::TipOff => "tip_off",
            Reason::Snoozed => "snoozed",
            Reason::Cooldown => "cooldown",
            Reason::HabitAdopted => "habit_adopted",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecideInput {
    pub paused: bool,
    pub job: Job,
    pub core_opportunity: bool,
    pub score: f64,
    pub threshold: f64,
    pub delivered_in_session: bool,
    /// A card was delivered on any surface less than the pack's quiet period ago.
    pub in_quiet_period: bool,
    pub ready_in_session: bool,
    pub deliveries_counted_today: u32,
    pub daily_cap: u32,
    /// The person's own answers or the tip's cooldown rule this tip out (a drop, checked right after pause).
    pub suppressed: Option<Reason>,
}

/// Is `now` inside the person-wide quiet period that follows the last delivery on any surface?
pub fn in_quiet_period(last_delivery_utc_ms: Option<i64>, now_utc_ms: i64, quiet_min: u32) -> bool {
    last_delivery_utc_ms.is_some_and(|t| now_utc_ms - t < quiet_min as i64 * 60_000)
}

/// Checks run in the design's order; the first that matches decides.
pub fn decide(i: &DecideInput) -> (Outcome, Reason) {
    if i.paused {
        (Outcome::Drop, Reason::Paused)
    } else if let Some(r) = i.suppressed {
        (Outcome::Drop, r)
    } else if !opportunity_allowed(&i.job, i.core_opportunity) {
        (Outcome::Drop, Reason::JobMismatch)
    } else if i.score < i.threshold {
        (Outcome::Hold, Reason::LowConfidence)
    } else if i.delivered_in_session {
        (Outcome::Hold, Reason::SessionBudget)
    } else if i.in_quiet_period {
        (Outcome::Hold, Reason::QuietPeriod)
    } else if i.ready_in_session {
        (Outcome::Hold, Reason::OneAtATime)
    } else if i.deliveries_counted_today >= i.daily_cap {
        (Outcome::Hold, Reason::DailyCap)
    } else {
        (Outcome::ShowReady, Reason::Ok)
    }
}

/// Re-check at Stop, just before delivery: the budget may have been used since the decision.
pub fn may_deliver(
    paused: bool,
    delivered_in_session: bool,
    in_quiet_period: bool,
    deliveries_counted_today: u32,
    daily_cap: u32,
) -> bool {
    !paused && !delivered_in_session && !in_quiet_period && deliveries_counted_today < daily_cap
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> DecideInput {
        DecideInput {
            paused: false,
            job: Job::Unclassified,
            core_opportunity: true,
            score: 1.0,
            threshold: 0.75,
            delivered_in_session: false,
            in_quiet_period: false,
            ready_in_session: false,
            deliveries_counted_today: 0,
            daily_cap: 5,
            suppressed: None,
        }
    }

    #[test]
    fn suppressed_tips_drop_after_pause_and_before_everything_else() {
        let mut i = ok();
        i.suppressed = Some(Reason::TipOff);
        assert_eq!(decide(&i), (Outcome::Drop, Reason::TipOff));
        i.paused = true;
        assert_eq!(decide(&i), (Outcome::Drop, Reason::Paused));
    }

    #[test]
    fn decide_all_clear_show_ready() {
        assert_eq!(decide(&ok()), (Outcome::ShowReady, Reason::Ok));
    }

    #[test]
    fn decide_paused_drops_before_everything() {
        let i = DecideInput { paused: true, delivered_in_session: true, score: 0.0, ..ok() };
        assert_eq!(decide(&i), (Outcome::Drop, Reason::Paused));
    }

    #[test]
    fn decide_unclassified_non_core_drops() {
        let i = DecideInput { core_opportunity: false, ..ok() };
        assert_eq!(decide(&i), (Outcome::Drop, Reason::JobMismatch));
    }

    #[test]
    fn decide_score_boundary_075_shows_0749_holds() {
        assert_eq!(decide(&DecideInput { score: 0.75, ..ok() }).1, Reason::Ok);
        assert_eq!(decide(&DecideInput { score: 0.749, ..ok() }), (Outcome::Hold, Reason::LowConfidence));
    }

    #[test]
    fn decide_session_budget_before_one_at_a_time() {
        let i = DecideInput { delivered_in_session: true, ready_in_session: true, ..ok() };
        assert_eq!(decide(&i), (Outcome::Hold, Reason::SessionBudget));
    }

    #[test]
    fn decide_ready_card_holds_second() {
        assert_eq!(decide(&DecideInput { ready_in_session: true, ..ok() }), (Outcome::Hold, Reason::OneAtATime));
    }

    #[test]
    fn decide_daily_cap_reached_holds_one_below_shows() {
        assert_eq!(decide(&DecideInput { deliveries_counted_today: 5, ..ok() }), (Outcome::Hold, Reason::DailyCap));
        assert_eq!(decide(&DecideInput { deliveries_counted_today: 4, ..ok() }).1, Reason::Ok);
    }

    #[test]
    fn decide_daily_cap_zero_always_holds() {
        assert_eq!(decide(&DecideInput { daily_cap: 0, ..ok() }), (Outcome::Hold, Reason::DailyCap));
    }

    #[test]
    fn codes_are_the_stored_strings() {
        // These strings are persisted in card_decision; changing one rewrites history's meaning.
        let o: Vec<_> = [Outcome::ShowReady, Outcome::Hold, Outcome::Drop].map(Outcome::as_str).into();
        assert_eq!(o, ["show_ready", "hold", "drop"]);
        let r: Vec<_> = [
            Reason::Paused,
            Reason::JobMismatch,
            Reason::LowConfidence,
            Reason::SessionBudget,
            Reason::OneAtATime,
            Reason::DailyCap,
            Reason::Ok,
            Reason::Expired,
            Reason::QuietPeriod,
        ]
        .map(Reason::as_str)
        .into();
        assert_eq!(
            r,
            [
                "paused",
                "job_mismatch",
                "low_confidence",
                "session_budget",
                "one_at_a_time",
                "daily_cap",
                "ok",
                "expired",
                "quiet_period"
            ]
        );
    }

    #[test]
    fn may_deliver_each_guard() {
        assert!(may_deliver(false, false, false, 4, 5));
        assert!(!may_deliver(true, false, false, 0, 5));
        assert!(!may_deliver(false, true, false, 0, 5));
        assert!(!may_deliver(false, false, true, 0, 5));
        assert!(!may_deliver(false, false, false, 5, 5));
    }

    #[test]
    fn decide_quiet_period_holds_after_session_budget_before_one_at_a_time() {
        assert_eq!(decide(&DecideInput { in_quiet_period: true, ..ok() }), (Outcome::Hold, Reason::QuietPeriod));
        let both = DecideInput { in_quiet_period: true, delivered_in_session: true, ..ok() };
        assert_eq!(decide(&both).1, Reason::SessionBudget);
        let ready = DecideInput { in_quiet_period: true, ready_in_session: true, ..ok() };
        assert_eq!(decide(&ready).1, Reason::QuietPeriod);
    }

    #[test]
    fn in_quiet_period_boundary_2959_inside_3000_outside() {
        let t = 1_000_000_000;
        assert!(!in_quiet_period(None, t, 30));
        assert!(in_quiet_period(Some(t), t + 29 * 60_000 + 59_999, 30));
        assert!(!in_quiet_period(Some(t), t + 30 * 60_000, 30));
        assert!(!in_quiet_period(Some(t), t, 0));
    }
}
