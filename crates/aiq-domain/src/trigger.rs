//! Trigger engine, rule `in_moment.v1`: show-ready / hold / drop for one Signal (design decision table).
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
    pub ready_in_session: bool,
    pub deliveries_counted_today: u32,
    pub daily_cap: u32,
}

/// Checks run in the design's order; the first that matches decides.
pub fn decide(i: &DecideInput) -> (Outcome, Reason) {
    if i.paused {
        (Outcome::Drop, Reason::Paused)
    } else if !opportunity_allowed(&i.job, i.core_opportunity) {
        (Outcome::Drop, Reason::JobMismatch)
    } else if i.score < i.threshold {
        (Outcome::Hold, Reason::LowConfidence)
    } else if i.delivered_in_session {
        (Outcome::Hold, Reason::SessionBudget)
    } else if i.ready_in_session {
        (Outcome::Hold, Reason::OneAtATime)
    } else if i.deliveries_counted_today >= i.daily_cap {
        (Outcome::Hold, Reason::DailyCap)
    } else {
        (Outcome::ShowReady, Reason::Ok)
    }
}

/// Re-check at Stop, just before delivery: the budget may have been used since the decision.
pub fn may_deliver(paused: bool, delivered_in_session: bool, deliveries_counted_today: u32, daily_cap: u32) -> bool {
    !paused && !delivered_in_session && deliveries_counted_today < daily_cap
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
            ready_in_session: false,
            deliveries_counted_today: 0,
            daily_cap: 5,
        }
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
                "expired"
            ]
        );
    }

    #[test]
    fn may_deliver_each_guard() {
        assert!(may_deliver(false, false, 4, 5));
        assert!(!may_deliver(true, false, 0, 5));
        assert!(!may_deliver(false, true, 0, 5));
        assert!(!may_deliver(false, false, 5, 5));
    }
}
