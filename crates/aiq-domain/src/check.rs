//! Accuracy check of the rung judge (design `docs/design/judge-accuracy-check.md`): which judged tasks to
//! check, and how the judge's stored answer agrees with the blind second reading. Pure: no I/O, no clock.
//! Every rate is a `Count` (k of n) with a Wilson interval; nothing is a stored percentage (derive, don't
//! store).
use aiq_contracts::check::CheckAnswer;
use std::collections::BTreeMap;

pub const CHECK_VERSION: &str = "c1";
/// Default sample size (K4; a first guess, the report prints intervals so it can't look precise).
pub const DEFAULT_SAMPLE: usize = 30;
/// Each job gets at least this many when it has them (K4).
pub const PER_JOB_MIN: usize = 5;
/// z for a two-sided 90% interval.
const Z90: f64 = 1.644_853_6;

/// One judged task that could be checked.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub judgment_id: String,
    pub job: String,
}

/// splitmix64: a tiny seeded generator, so a run's sample is reproducible from its stored seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

/// A seeded random sample of `n` candidates, stratified by job: each job first gets up to `PER_JOB_MIN`,
/// then the rest is filled at random. Input order does not matter (sorted by id first). Returns judgment ids.
pub fn sample(candidates: &[Candidate], n: usize, seed: u64) -> Vec<String> {
    let mut all: Vec<&Candidate> = candidates.iter().collect();
    all.sort_by(|a, b| a.judgment_id.cmp(&b.judgment_id));
    all.dedup_by(|a, b| a.judgment_id == b.judgment_id);
    let mut rng = Rng(seed);
    rng.shuffle(&mut all);
    let mut by_job: BTreeMap<&str, Vec<&Candidate>> = BTreeMap::new();
    for c in &all {
        by_job.entry(c.job.as_str()).or_default().push(c);
    }
    let mut picked: Vec<String> = Vec::new();
    let floor = PER_JOB_MIN.min(n / by_job.len().max(1)).max(1);
    for list in by_job.values() {
        for c in list.iter().take(floor) {
            if picked.len() < n {
                picked.push(c.judgment_id.clone());
            }
        }
    }
    for c in &all {
        if picked.len() >= n {
            break;
        }
        if !picked.contains(&c.judgment_id) {
            picked.push(c.judgment_id.clone());
        }
    }
    picked
}

/// k of n, with a two-sided 90% Wilson interval.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Count {
    pub k: u32,
    pub n: u32,
}

impl Count {
    pub fn add(&mut self, hit: bool) {
        self.n += 1;
        self.k += hit as u32;
    }
    /// (low, high) as fractions; None when n is 0.
    pub fn wilson(&self) -> Option<(f64, f64)> {
        if self.n == 0 {
            return None;
        }
        let (k, n, z) = (self.k as f64, self.n as f64, Z90);
        let p = k / n;
        let d = 1.0 + z * z / n;
        let c = (p + z * z / (2.0 * n)) / d;
        let h = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / d;
        Some(((c - h).max(0.0), (c + h).min(1.0)))
    }
    /// "24 of 30 (66–89%)", or "none checked".
    pub fn show(&self) -> String {
        match self.wilson() {
            None => "none checked".into(),
            Some((lo, hi)) => format!("{} of {} ({:.0}–{:.0}%)", self.k, self.n, lo * 100.0, hi * 100.0),
        }
    }
}

/// The judge's stored answer for one task, as the check compares it.
#[derive(Debug, Clone, PartialEq)]
pub struct JudgeView {
    pub job: String,
    pub task_type: Option<String>,
    pub current_rung: u8,
    pub model_rung: u8,
    pub uncertain: bool,
    /// Model-read facts by id ("F4" …); None = unknown to the judge.
    pub facts: BTreeMap<String, Option<bool>>,
}

/// One fact's tally.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FactTally {
    /// Both answered; agreement among those.
    pub agree: Count,
    /// The grader said the transcript can't settle it.
    pub cant_settle: u32,
    /// The judge had it as unknown while the grader answered.
    pub judge_unknown: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Agreement {
    pub job: Count,
    /// Only where both picked the same job (a task type is per job).
    pub task_type: Count,
    pub task_type_by_job: BTreeMap<String, Count>,
    pub rung_exact: Count,
    pub rung_within_one: Count,
    pub model_rung_exact: Count,
    /// The same three, on confident (not Uncertain) judgments only: the ones AIQ coaches.
    pub confident_job: Count,
    pub confident_type: Count,
    pub confident_rung_exact: Count,
    pub facts: BTreeMap<String, FactTally>,
    /// Kind of task (job/type the judge named) -> type agreement, for the later pause rule (K6b).
    pub by_kind: BTreeMap<(String, String), Count>,
}

/// Compare each judged task with its second reading.
pub fn agreement(pairs: &[(JudgeView, CheckAnswer)]) -> Agreement {
    let mut a = Agreement::default();
    for (j, c) in pairs {
        let job_ok = j.job == c.job;
        a.job.add(job_ok);
        let rung_ok = j.current_rung == c.current_rung;
        a.rung_exact.add(rung_ok);
        a.rung_within_one.add(j.current_rung.abs_diff(c.current_rung) <= 1);
        a.model_rung_exact.add(j.model_rung == c.current_rung);
        let type_ok = job_ok && j.task_type.as_deref() == Some(c.task_type.as_str());
        if job_ok && j.task_type.is_some() {
            a.task_type.add(type_ok);
            a.task_type_by_job.entry(j.job.clone()).or_default().add(type_ok);
        }
        if let Some(t) = &j.task_type {
            a.by_kind.entry((j.job.clone(), t.clone())).or_default().add(type_ok);
        }
        if !j.uncertain {
            a.confident_job.add(job_ok);
            if j.task_type.is_some() {
                a.confident_type.add(type_ok);
            }
            a.confident_rung_exact.add(rung_ok);
        }
        for (id, reading) in c.facts() {
            let t = a.facts.entry(id.to_owned()).or_default();
            match (reading, j.facts.get(id).copied().flatten()) {
                (None, _) => t.cant_settle += 1,
                (Some(_), None) => t.judge_unknown += 1,
                (Some(g), Some(v)) => t.agree.add(g == v),
            }
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cands(spec: &[(&str, usize)]) -> Vec<Candidate> {
        let mut v = Vec::new();
        for (job, n) in spec {
            for i in 0..*n {
                v.push(Candidate { judgment_id: format!("{job}-{i:03}"), job: (*job).into() });
            }
        }
        v
    }

    #[test]
    fn sample_is_reproducible_and_order_free() {
        let c = cands(&[("ask_research", 40), ("code_change", 8), ("write_doc", 3)]);
        let mut rev = c.clone();
        rev.reverse();
        assert_eq!(sample(&c, 30, 7), sample(&rev, 30, 7));
        assert_ne!(sample(&c, 30, 7), sample(&c, 30, 8));
        let s = sample(&c, 30, 7);
        assert_eq!(s.len(), 30);
        let mut d = s.clone();
        d.sort();
        d.dedup();
        assert_eq!(d.len(), 30, "no task twice");
    }

    #[test]
    fn sample_gives_each_job_its_floor() {
        let c = cands(&[("ask_research", 100), ("code_change", 8), ("write_doc", 3)]);
        for seed in 0..20 {
            let s = sample(&c, 30, seed);
            let n = |j: &str| s.iter().filter(|id| id.starts_with(j)).count();
            assert!(n("code_change") >= 5 && n("write_doc") == 3, "seed {seed}: {s:?}");
        }
    }

    #[test]
    fn sample_smaller_than_asked_uses_all() {
        let c = cands(&[("code_change", 4)]);
        assert_eq!(sample(&c, 30, 1).len(), 4);
        assert!(sample(&[], 30, 1).is_empty());
    }

    #[test]
    fn wilson_matches_known_values() {
        let (lo, hi) = Count { k: 24, n: 30 }.wilson().unwrap();
        assert!((lo - 0.657).abs() < 0.001 && (hi - 0.893).abs() < 0.001, "{lo} {hi}");
        let (lo, hi) = Count { k: 0, n: 10 }.wilson().unwrap();
        assert_eq!(lo, 0.0);
        assert!(hi > 0.1 && hi < 0.25);
        assert_eq!(Count::default().wilson(), None);
        assert_eq!(Count { k: 24, n: 30 }.show(), "24 of 30 (66–89%)");
    }

    fn judge(job: &str, ty: &str, rung: u8, uncertain: bool, f6: Option<bool>) -> JudgeView {
        JudgeView {
            job: job.into(),
            task_type: Some(ty.into()),
            current_rung: rung,
            model_rung: rung,
            uncertain,
            facts: [("F4".to_owned(), Some(true)), ("F6".to_owned(), f6)].into_iter().collect(),
        }
    }

    fn grader(job: &str, ty: &str, rung: u8, f4: Option<bool>, f6: Option<bool>) -> CheckAnswer {
        CheckAnswer {
            job: job.into(),
            task_type: ty.into(),
            current_rung: rung,
            f4_goal_context_constraints: f4,
            f5_reprompts_added_information: None,
            f6_output_checked: f6,
            f7_written_brief: None,
            f12_step_by_step: None,
            f13_whole_outcome_handed_over: None,
        }
    }

    #[test]
    fn agreement_counts_job_type_rung_and_facts() {
        let pairs = vec![
            (
                judge("ask_research", "explain", 1, false, None),
                grader("ask_research", "explain", 2, Some(true), Some(true)),
            ),
            (
                judge("ask_research", "compare", 1, true, Some(false)),
                grader("ask_research", "find", 1, Some(false), None),
            ),
            (
                judge("code_change", "fix_bug", 2, false, Some(true)),
                grader("ask_research", "troubleshoot", 4, None, Some(false)),
            ),
        ];
        let a = agreement(&pairs);
        assert_eq!((a.job.k, a.job.n), (2, 3));
        assert_eq!((a.task_type.k, a.task_type.n), (1, 2), "type compared only where the job matches");
        assert_eq!((a.rung_exact.k, a.rung_within_one.k, a.rung_exact.n), (1, 2, 3));
        assert_eq!((a.confident_job.k, a.confident_job.n), (1, 2), "uncertain judgment left out");
        let f4 = a.facts["F4"];
        assert_eq!((f4.agree.k, f4.agree.n, f4.cant_settle), (1, 2, 1));
        let f6 = a.facts["F6"];
        assert_eq!((f6.agree.k, f6.agree.n, f6.cant_settle, f6.judge_unknown), (0, 1, 1, 1));
        assert_eq!(a.by_kind[&("code_change".to_owned(), "fix_bug".to_owned())], Count { k: 0, n: 1 });
    }
}
