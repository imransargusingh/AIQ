//! Job classification seam. Phase 1 = always Unclassified; Phase 4 replaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Unclassified,
    Named(String),
}

pub trait JobClassifier {
    fn classify(&self, task_id: &str) -> Job;
}

#[derive(Debug, Default)]
pub struct AlwaysUnclassified;

impl JobClassifier for AlwaysUnclassified {
    fn classify(&self, _task_id: &str) -> Job {
        Job::Unclassified
    }
}

/// Invariant (Job classification aggregate): an Unclassified Task gets only core Opportunities.
pub fn opportunity_allowed(job: &Job, core_opportunity: bool) -> bool {
    core_opportunity || !matches!(job, Job::Unclassified)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unclassified_gets_only_core_opportunities() {
        let j = AlwaysUnclassified.classify("t");
        assert_eq!(j, Job::Unclassified);
        assert!(opportunity_allowed(&j, true));
        assert!(!opportunity_allowed(&j, false));
        assert!(opportunity_allowed(&Job::Named("code".into()), false));
    }
}
