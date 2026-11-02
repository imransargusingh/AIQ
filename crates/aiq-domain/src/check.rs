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
    /// "24 of 30 (66-89%)", or "none checked".
    pub fn show(&self) -> String {
        match self.wilson() {
            None => "none checked".into(),
            Some((lo, hi)) => format!("{} of {} ({:.0}-{:.0}%)", self.k, self.n, lo * 100.0, hi * 100.0),
        }
    }
}

/// The judge's stored answer for one task, as the check compares it.
#[derive(Debug, Clone, PartialEq)]
pub struct JudgeView {
    /// The judge's job; `other` when it gave none (stored as `unclassified`), so it compares with the grader's.
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
    /// The same three, on confident (not Uncertain) judgments only: the ones AIQ coaches. `confident_type`
    /// is compared only where the job matched, as `task_type` is (one definition of "kind agreement").
    pub confident_job: Count,
    pub confident_type: Count,
    pub confident_rung_exact: Count,
    pub facts: BTreeMap<String, FactTally>,
    /// Kind of task (job/type the judge named) -> type agreement, for the later pause rule (K6b).
    pub by_kind: BTreeMap<(String, String), Count>,
    /// Where the judge gave no pack job (`other`): the job the second reading picked, and how often.
    pub no_job_read_as: BTreeMap<String, u32>,
}

/// Compare each judged task with its second reading.
pub fn agreement(pairs: &[(JudgeView, CheckAnswer)]) -> Agreement {
    let mut a = Agreement::default();
    for (j, c) in pairs {
        let job_ok = j.job == c.job;
        a.job.add(job_ok);
        if j.job == "other" {
            *a.no_job_read_as.entry(c.job.clone()).or_default() += 1;
        }
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
            if job_ok && j.task_type.is_some() {
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

// ------------------------------------------------------------------ pause rule (K6b)

/// The pause bar (K6b): below 8 in 10 agreement a pack's job, or one kind of task, stops being coached.
/// Guess, not measured: it is the bar every rule in AIQ uses (rung-model US-7, the live type-guess
/// self-check, the no-job and kind-of-task rules). Raise or lower it only from a measured repeat-run spread.
pub const PAUSE_BAR: f64 = 0.8;
/// A job needs at least this many checked readings before it can pause (guess: the run_ship gate's 10).
pub const PAUSE_MIN_JOB: u32 = 10;
/// A kind of task needs at least this many (guess: design K6's 5; a kind is rarely sampled 10 times).
pub const PAUSE_MIN_KIND: u32 = 5;

/// What a pause covers: a pack's job (every next-step tip for tasks of that job), or one kind of task.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    Job(String),
    Kind(String, String),
}

impl Scope {
    /// `run_ship` or `code_change/fix_bug`: the form settings and the CLI use.
    pub fn key(&self) -> String {
        match self {
            Scope::Job(j) => j.clone(),
            Scope::Kind(j, t) => format!("{j}/{t}"),
        }
    }
    pub fn parse(s: &str) -> Option<Scope> {
        let ok = |p: &str| !p.is_empty() && p.len() <= 40 && p.chars().all(|c| c.is_ascii_lowercase() || c == '_');
        match s.split_once('/') {
            Some((j, t)) if ok(j) && ok(t) => Some(Scope::Kind(j.into(), t.into())),
            None if ok(s) => Some(Scope::Job(s.into())),
            _ => None,
        }
    }
    pub fn job(&self) -> &str {
        match self {
            Scope::Job(j) | Scope::Kind(j, _) => j,
        }
    }
    fn min_checked(&self) -> u32 {
        match self {
            Scope::Job(_) => PAUSE_MIN_JOB,
            Scope::Kind(..) => PAUSE_MIN_KIND,
        }
    }
}

/// One second reading of a judgment that is still the task's current one, as the pause rule reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub run_id: String,
    pub run_started_ms: i64,
    /// The judge's job (`unclassified` when it gave none) and kind.
    pub judge_job: String,
    pub judge_type: Option<String>,
    pub reader_job: String,
    pub reader_type: String,
}

/// The verdict for one scope, from the newest checks.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub scope: Scope,
    /// Agreement over the window: the newest whole check runs until at least the scope's minimum.
    pub agree: Count,
    /// The newest run in the window: an override names it, so new evidence lets AIQ decide again.
    pub newest_run: String,
    pub runs: u32,
    /// Enough readings and under the bar.
    pub paused: bool,
}

/// The pause rule (K6b). Per scope, readings are taken newest run first, whole runs at a time, until the
/// scope's minimum is reached; older runs then stop counting, so a later check that clears the bar resumes
/// tips by itself. A job is scored on tasks the judge put in that job (did the reader agree?); a kind on
/// tasks the judge gave that kind (same job and same kind?). `other` kinds never coach, so are never scored.
pub fn verdicts(readings: &[Reading]) -> Vec<Verdict> {
    let runs = newest_first(readings.iter().map(|r| (r.run_id.as_str(), r.run_started_ms)));
    let mut scored: BTreeMap<Scope, Vec<(&str, bool)>> = BTreeMap::new();
    for run in &runs {
        for r in readings.iter().filter(|r| r.run_id == *run) {
            if r.judge_job == "unclassified" || r.judge_job == "other" {
                continue;
            }
            let job_ok = r.reader_job == r.judge_job;
            scored.entry(Scope::Job(r.judge_job.clone())).or_default().push((run, job_ok));
            if let Some(t) = r.judge_type.as_deref().filter(|t| *t != "other") {
                let hit = job_ok && r.reader_type == t;
                scored.entry(Scope::Kind(r.judge_job.clone(), t.to_owned())).or_default().push((run, hit));
            }
        }
    }
    scored
        .into_iter()
        .map(|(scope, list)| {
            let (agree, seen) = window(list, scope.min_checked());
            let paused = agree.n >= scope.min_checked() && (agree.k as f64) < PAUSE_BAR * agree.n as f64;
            Verdict { newest_run: seen[0].to_owned(), runs: seen.len() as u32, agree, paused, scope }
        })
        .collect()
}

/// Check run ids, newest first (ties by id), each once.
fn newest_first<'a>(runs: impl Iterator<Item = (&'a str, i64)>) -> Vec<&'a str> {
    let mut runs: Vec<(&str, i64)> = runs.collect();
    runs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(a.0)));
    runs.dedup();
    runs.into_iter().map(|(r, _)| r).collect()
}

/// The newest whole runs of `list` (already newest run first) until at least `min` readings: older runs
/// then stop counting, so later evidence decides by itself. Returns the agreement and the runs used.
fn window(list: Vec<(&str, bool)>, min: u32) -> (Count, Vec<&str>) {
    let mut agree = Count::default();
    let mut seen: Vec<&str> = Vec::new();
    for (run, hit) in list {
        if !seen.contains(&run) {
            if agree.n >= min {
                break;
            }
            seen.push(run);
        }
        agree.add(hit);
    }
    (agree, seen)
}

// ------------------------------------------------------------------ reclaim gate (design reclaim-low-confidence)

/// Rule R2 goes live only after at least this many second readings of the tasks it would reclaim (no-job N3).
pub const RECLAIM_MIN: u32 = 10;

/// One second reading of a task rule R2 gives a job (or would): did the reader name the same job?
#[derive(Debug, Clone, PartialEq)]
pub struct RuleReading {
    pub run_id: String,
    pub run_started_ms: i64,
    /// The job R2 gives the task.
    pub rule_job: String,
    pub reader_job: String,
}

/// Whether R2 is live, and the evidence it rests on.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuleGate {
    pub agree: Count,
    pub runs: u32,
    pub live: bool,
}

/// The reclaim gate (no-job N3, the same bar and window as the pause rule): R2 is live when the newest whole
/// check runs hold at least `RECLAIM_MIN` readings of R2 tasks and the reader agreed on at least 8 in 10.
/// Off is the default: no readings, too few, or under the bar. A later check under the bar turns it off.
pub fn reclaim_gate(readings: &[RuleReading]) -> RuleGate {
    let runs = newest_first(readings.iter().map(|r| (r.run_id.as_str(), r.run_started_ms)));
    let list: Vec<(&str, bool)> = runs
        .iter()
        .flat_map(|run| readings.iter().filter(move |r| r.run_id == *run).map(|r| (*run, r.reader_job == r.rule_job)))
        .collect();
    let (agree, seen) = window(list, RECLAIM_MIN);
    let live = agree.n >= RECLAIM_MIN && (agree.k as f64) >= PAUSE_BAR * agree.n as f64;
    RuleGate { agree, runs: seen.len() as u32, live }
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
        assert_eq!(Count { k: 24, n: 30 }.show(), "24 of 30 (66-89%)");
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
        assert!(a.no_job_read_as.is_empty());
    }

    #[test]
    fn confident_kind_uses_the_same_denominator_as_kind() {
        // The 15:37Z report printed kind 14 of 17 but confident kind 14 of 18: the confident count also
        // scored tasks whose job differed. Confident kind must be a subset of kind, never larger.
        let pairs = vec![
            (judge("code_change", "fix_bug", 2, false, None), grader("ask_research", "troubleshoot", 2, None, None)),
            (judge("code_change", "feature", 2, false, None), grader("code_change", "feature", 2, None, None)),
            (judge("code_change", "tests", 2, true, None), grader("code_change", "fix_bug", 2, None, None)),
        ];
        let a = agreement(&pairs);
        assert_eq!((a.task_type.k, a.task_type.n), (1, 2));
        assert_eq!((a.confident_type.k, a.confident_type.n), (1, 1), "job miss and uncertain both left out");
        assert!(a.confident_type.n <= a.task_type.n);
    }

    #[test]
    fn no_job_agrees_with_other_and_records_what_the_grader_picked() {
        let mut none = judge("other", "x", 1, true, None);
        none.task_type = None;
        let pairs = vec![
            (none.clone(), grader("other", "other", 1, None, None)),
            (none, grader("ask_research", "explain", 1, None, None)),
        ];
        let a = agreement(&pairs);
        assert_eq!((a.job.k, a.job.n), (1, 2));
        assert_eq!(a.task_type.n, 0, "no type to compare when the judge gave no job");
        assert_eq!(a.no_job_read_as.get("other"), Some(&1));
        assert_eq!(a.no_job_read_as.get("ask_research"), Some(&1));
    }

    fn reading(run: &str, at: i64, judge: (&str, &str), reader: (&str, &str)) -> Reading {
        Reading {
            run_id: run.into(),
            run_started_ms: at,
            judge_job: judge.0.into(),
            judge_type: Some(judge.1.into()),
            reader_job: reader.0.into(),
            reader_type: reader.1.into(),
        }
    }

    fn many(run: &str, at: i64, n: usize, hits: usize, kind: (&str, &str)) -> Vec<Reading> {
        (0..n).map(|i| reading(run, at, kind, if i < hits { kind } else { ("ask_research", "explain") })).collect()
    }

    fn find<'a>(v: &'a [Verdict], key: &str) -> &'a Verdict {
        v.iter().find(|x| x.scope.key() == key).unwrap()
    }

    #[test]
    fn a_job_pauses_at_seven_of_ten_not_at_eight_and_needs_ten() {
        let v = verdicts(&many("r1", 1, 10, 7, ("run_ship", "deploy")));
        let j = find(&v, "run_ship");
        assert!(j.paused && j.agree == Count { k: 7, n: 10 });
        assert!(!find(&verdicts(&many("r1", 1, 10, 8, ("run_ship", "deploy"))), "run_ship").paused);
        let few = verdicts(&many("r1", 1, 9, 0, ("run_ship", "deploy")));
        assert!(!find(&few, "run_ship").paused, "9 checked is too few for a job");
        assert!(find(&few, "run_ship/deploy").paused, "but 9 is enough for a kind (5)");
    }

    #[test]
    fn a_kind_pauses_at_three_of_five_and_needs_the_job_to_match() {
        let mut r = many("r1", 1, 4, 4, ("code_change", "fix_bug"));
        r.push(reading("r1", 1, ("code_change", "fix_bug"), ("ask_research", "fix_bug")));
        let k = find(&verdicts(&r), "code_change/fix_bug").clone();
        assert_eq!((k.agree, k.paused), (Count { k: 4, n: 5 }, false));
        r.push(reading("r1", 1, ("code_change", "fix_bug"), ("code_change", "feature")));
        let k = find(&verdicts(&r), "code_change/fix_bug").clone();
        assert_eq!((k.agree, k.paused), (Count { k: 4, n: 6 }, true));
        let few = verdicts(&many("r1", 1, 4, 0, ("code_change", "fix_bug")));
        assert!(!find(&few, "code_change/fix_bug").paused, "4 checked is too few");
    }

    #[test]
    fn a_newer_check_that_clears_the_bar_resumes_and_older_runs_stop_counting() {
        let mut r = many("old", 1, 10, 5, ("run_ship", "deploy"));
        assert!(find(&verdicts(&r), "run_ship").paused);
        r.extend(many("new", 2, 10, 9, ("run_ship", "deploy")));
        let j = find(&verdicts(&r), "run_ship").clone();
        assert_eq!((j.agree, j.paused, j.newest_run.as_str(), j.runs), (Count { k: 9, n: 10 }, false, "new", 1));
        // A small newer run is pooled with the one before it, whole runs at a time.
        let mut r = many("old", 1, 10, 5, ("run_ship", "deploy"));
        r.extend(many("new", 2, 5, 5, ("run_ship", "deploy")));
        let j = find(&verdicts(&r), "run_ship").clone();
        assert_eq!((j.agree, j.paused, j.runs), (Count { k: 10, n: 15 }, true, 2));
    }

    #[test]
    fn no_job_and_other_kinds_are_never_scored() {
        let r = vec![
            reading("r", 1, ("unclassified", "other"), ("run_ship", "deploy")),
            reading("r", 1, ("code_change", "other"), ("code_change", "fix_bug")),
        ];
        let v = verdicts(&r);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].scope, Scope::Job("code_change".into()));
    }

    #[test]
    fn scope_keys_round_trip_and_bad_ones_are_refused() {
        for k in ["run_ship", "code_change/fix_bug"] {
            assert_eq!(Scope::parse(k).unwrap().key(), k);
        }
        for k in ["", "a/b/c", "Run", "x/", "../etc", "a b"] {
            assert!(Scope::parse(k).is_none(), "{k}");
        }
    }

    fn rule(run: &str, at: i64, n: usize, hits: usize) -> Vec<RuleReading> {
        (0..n)
            .map(|i| RuleReading {
                run_id: run.into(),
                run_started_ms: at,
                rule_job: "ask_research".into(),
                reader_job: if i < hits { "ask_research" } else { "other" }.into(),
            })
            .collect()
    }

    #[test]
    fn reclaim_is_off_until_ten_checked_and_eight_in_ten_agree() {
        assert!(!reclaim_gate(&[]).live, "off by default");
        assert!(!reclaim_gate(&rule("r1", 1, 9, 9)).live, "9 of 9 is too few");
        let on = reclaim_gate(&rule("r1", 1, 10, 8));
        assert!(on.live && on.agree == Count { k: 8, n: 10 } && on.runs == 1);
        assert!(!reclaim_gate(&rule("r1", 1, 10, 7)).live, "7 of 10 is under the bar");
        // The measured pre-run_ship reading (4 of 7) never turns it on.
        assert!(!reclaim_gate(&rule("r1", 1, 7, 4)).live);
    }

    #[test]
    fn reclaim_reads_the_newest_whole_runs_so_a_later_check_turns_it_off_or_on() {
        let mut r = rule("old", 1, 10, 10);
        r.extend(rule("new", 2, 10, 6));
        let g = reclaim_gate(&r);
        assert!(!g.live && g.agree == Count { k: 6, n: 10 }, "the newer check decides");
        let mut r = rule("old", 1, 10, 3);
        r.extend(rule("new", 2, 12, 11));
        assert!(reclaim_gate(&r).live);
        // Two small runs together reach the minimum.
        let mut r = rule("a", 1, 6, 5);
        r.extend(rule("b", 2, 6, 5));
        let g = reclaim_gate(&r);
        assert!(g.live && g.agree == Count { k: 10, n: 12 } && g.runs == 2);
    }
}
