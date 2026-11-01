//! Task rule and the re-prompt detector (`reprompt.v1`, Opportunity 1).
//!
//! Task (Phase 1 rule): the prompts of one surface session, split when consecutive prompts are more
//! than the pack's idle gap apart. Task id = event id of its first prompt.
// simplify: one session = one Task within the idle gap. Ceiling: a topic switch inside 30 minutes stays
// one Task. Upgrade trigger: real-data week shows grouping F1 < 0.9, or Phase 2 needs cross-surface Tasks.
use crate::text::{Features, features, jaccard};
use aiq_contracts::pack::{PackV1, RepromptParams};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// One prompt as the detector sees it.
#[derive(Debug, Clone)]
pub struct PromptObs<'a> {
    pub event_id: &'a str,
    pub utc_ms: i64,
    pub text: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignalDraft {
    pub task_id: String,
    pub reprompt_count: u32,
    pub similar: u32,
    pub cue_only: u32,
    /// (similar re-prompts + 0.5 × cue-only re-prompts) / count
    pub score: f64,
    /// Event ids: the request's first prompt, then each re-prompt.
    pub evidence: Vec<String>,
}

/// Detector working set for one open Task. In memory only; rebuilt from stored events at core start.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskState {
    pub task_id: String,
    pub last_ms: i64,
    seen_words: BTreeSet<String>,
    seen_mentions: BTreeSet<String>,
    /// Word sets of the last `window` content prompts (not cue-only ones) of the current request.
    request: VecDeque<BTreeSet<String>>,
    count: u32,
    similar: u32,
    cue_only: u32,
    evidence: Vec<String>,
    fired: bool,
}

impl TaskState {
    fn new(task_id: &str, utc_ms: i64) -> Self {
        Self {
            task_id: task_id.to_owned(),
            last_ms: utc_ms,
            seen_words: BTreeSet::new(),
            seen_mentions: BTreeSet::new(),
            request: VecDeque::new(),
            count: 0,
            similar: 0,
            cue_only: 0,
            evidence: Vec::new(),
            fired: false,
        }
    }

    fn start_request(&mut self, f: &Features, event_id: &str, window: usize) {
        self.request.clear();
        self.count = 0;
        self.similar = 0;
        self.cue_only = 0;
        self.fired = false;
        self.evidence = vec![event_id.to_owned()];
        self.push_request(f, window);
    }

    fn push_request(&mut self, f: &Features, window: usize) {
        self.request.push_back(f.words.clone());
        while self.request.len() > window {
            self.request.pop_front();
        }
    }

    fn observe(&mut self, f: &Features, event_id: &str, p: &RepromptParams) -> Option<SignalDraft> {
        if self.request.is_empty() {
            self.start_request(f, event_id, p.window);
            return None;
        }
        let similar = self.request.iter().any(|w| jaccard(w, &f.words) >= p.similarity);
        let cue = p.cues.iter().any(|c| f.normalized.starts_with(c.as_str()));
        let new_mention = f.mentions.iter().any(|m| !self.seen_mentions.contains(m));
        let new_words = f.words.iter().filter(|w| !self.seen_words.contains(*w)).count();
        let new_info = new_mention || f.pasted || new_words > p.max_new_words;

        if (similar || cue) && !new_info {
            self.count += 1;
            if similar {
                self.similar += 1;
            } else {
                self.cue_only += 1;
            }
            if !self.fired {
                self.evidence.push(event_id.to_owned());
            }
            // Only content prompts join the request window, so a repeated "try again" stays cue-only.
            if similar {
                self.push_request(f, p.window);
            }
            if self.count >= p.threshold && !self.fired {
                self.fired = true;
                return Some(SignalDraft {
                    task_id: self.task_id.clone(),
                    reprompt_count: self.count,
                    similar: self.similar,
                    cue_only: self.cue_only,
                    score: (self.similar as f64 + 0.5 * self.cue_only as f64) / self.count as f64,
                    evidence: self.evidence.clone(),
                });
            }
            None
        } else {
            self.start_request(f, event_id, p.window);
            None
        }
    }
}

/// All open Tasks, keyed by (surface, surface_session_id).
#[derive(Debug, Clone, PartialEq)]
pub struct Engine {
    params: RepromptParams,
    idle_gap_ms: i64,
    tasks: BTreeMap<(String, String), TaskState>,
}

impl Engine {
    pub fn new(pack: &PackV1) -> Self {
        Self { params: pack.reprompt.clone(), idle_gap_ms: pack.idle_gap_min as i64 * 60_000, tasks: BTreeMap::new() }
    }

    pub fn idle_gap_ms(&self) -> i64 {
        self.idle_gap_ms
    }

    /// Feed one prompt, in event order. Returns a Signal when a request reaches the threshold.
    /// Empty or whitespace-only prompts are ignored: they neither count nor reset.
    pub fn observe(&mut self, surface: &str, session: &str, obs: PromptObs<'_>) -> Option<SignalDraft> {
        if obs.text.trim().is_empty() {
            return None;
        }
        let key = (surface.to_owned(), session.to_owned());
        let gap = self.idle_gap_ms;
        let task = self
            .tasks
            .entry(key)
            .and_modify(|t| {
                if obs.utc_ms - t.last_ms > gap {
                    *t = TaskState::new(obs.event_id, obs.utc_ms);
                }
            })
            .or_insert_with(|| TaskState::new(obs.event_id, obs.utc_ms));
        task.last_ms = task.last_ms.max(obs.utc_ms);
        let f = features(obs.text, &self.params);
        let out = task.observe(&f, obs.event_id, &self.params);
        task.seen_words.extend(f.words);
        task.seen_mentions.extend(f.mentions);
        out
    }

    /// Current Task id for a session, if one is open.
    pub fn task_id(&self, surface: &str, session: &str) -> Option<&str> {
        self.tasks.get(&(surface.to_owned(), session.to_owned())).map(|t| t.task_id.as_str())
    }

    /// Drop Tasks idle for longer than the idle gap; they can never receive another prompt.
    pub fn prune(&mut self, now_ms: i64) {
        let gap = self.idle_gap_ms;
        self.tasks.retain(|_, t| now_ms - t.last_ms <= gap);
    }

    pub fn open_tasks(&self) -> usize {
        self.tasks.len()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use aiq_contracts::pack::{Tip, TriggerParams};

    pub(crate) fn params() -> RepromptParams {
        RepromptParams {
            detector_id: "reprompt".into(),
            detector_version: 1,
            opportunity_id: "reprompt".into(),
            core_opportunity: true,
            similarity: 0.6,
            max_new_words: 4,
            threshold: 3,
            window: 8,
            min_word_len: 3,
            stop_words: "a an the and or to of in on for with from by is are be it this that these those i we you my our me us do does did can could should would will please just also then now so if at as not no yes ok okay use using make add run show write fix get let what how why when which there here all any one into out up its them they he she".split(' ').map(String::from).collect(),
            cues: vec!["try again".into(), "still not working".into(), "still doesn't work".into(), "that's wrong".into(), "no, i meant".into()],
        }
    }

    pub(crate) fn pack() -> PackV1 {
        PackV1 {
            schema: "aiq.pack.v1".into(),
            pack_id: "test".into(),
            version: "t1".into(),
            idle_gap_min: 30,
            reprompt: params(),
            trigger: TriggerParams {
                rule_id: "in_moment.v1".into(),
                in_moment_threshold: 0.75,
                ready_expiry_min: 30,
                default_daily_cap: 5,
                quiet_min: 30,
            },
            automated: Default::default(),
            tips: vec![Tip {
                tip_id: "t".into(),
                opportunity_id: "reprompt".into(),
                noticed: "n {count}".into(),
                why: "w".into(),
                try_this: "t".into(),
            }],
        }
    }

    const MIN: i64 = 60_000;

    /// Feed prompts one minute apart; return every signal.
    fn run(e: &mut Engine, prompts: &[&str]) -> Vec<SignalDraft> {
        prompts
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let id = format!("e{i:05}");
                e.observe("claude-code", "s", PromptObs { event_id: &id, utc_ms: i as i64 * MIN, text: t })
            })
            .collect()
    }

    const BASE: &str = "refactor the login handler to return typed errors";
    const R1: &str = "refactor login handler so it returns typed errors";
    const R2: &str = "login handler refactor: return typed errors please";
    const R3: &str = "please refactor the login handler, typed errors returned";

    #[test]
    fn observe_two_reprompts_no_signal() {
        assert!(run(&mut Engine::new(&pack()), &[BASE, R1, R2]).is_empty());
    }

    #[test]
    fn observe_three_reprompts_one_signal_with_evidence() {
        let s = run(&mut Engine::new(&pack()), &[BASE, R1, R2, R3]);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].reprompt_count, 3);
        assert_eq!(s[0].similar, 3);
        assert_eq!(s[0].score, 1.0);
        assert_eq!(s[0].evidence, vec!["e00000", "e00001", "e00002", "e00003"]);
        assert_eq!(s[0].task_id, "e00000");
    }

    #[test]
    fn observe_more_reprompts_after_signal_no_second_signal_same_request() {
        assert_eq!(run(&mut Engine::new(&pack()), &[BASE, R1, R2, R3, R1, R2]).len(), 1);
    }

    #[test]
    fn observe_new_request_after_signal_can_signal_again() {
        let other = "write unit tests covering the payment retry backoff schedule";
        let o1 = "write unit tests for payment retry backoff schedule";
        let s = run(&mut Engine::new(&pack()), &[BASE, R1, R2, R3, other, o1, o1, o1]);
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn observe_jaccard_boundary_060_counts_059_does_not() {
        // 3 of 5 = 0.6 → similar; 0.6 of words reused, ≤ 4 new words.
        let mut p = pack();
        p.reprompt.max_new_words = 100;
        let base = "alpha bravo charlie delta";
        let at_060 = "alpha bravo charlie echo"; // |∩|=3, |∪|=5 → 0.6
        let s = run(&mut Engine::new(&p), &[base, at_060, at_060, at_060]);
        assert_eq!(s.len(), 1);
        p.reprompt.similarity = 0.6000001;
        let s = run(&mut Engine::new(&p), &[base, at_060, at_060, at_060]);
        // the 2nd/3rd copies are identical to the 1st re-prompt (1.0), but the 1st re-prompt itself
        // fails at 0.6000001 and starts a new request, so only 2 re-prompts follow it.
        assert!(s.is_empty());
    }

    #[test]
    fn observe_four_new_words_still_reprompt_five_is_new_info() {
        let mut p = pack();
        p.reprompt.similarity = 0.0; // isolate the new-words rule: every prompt "similar"
        let base = "alpha bravo charlie";
        let four = "alpha bravo charlie delta echo foxtrot golf";
        let s = run(&mut Engine::new(&p), &[base, four, four, four]);
        assert_eq!(s.len(), 1, "4 new words is not new information");
        let five = "alpha bravo charlie delta echo foxtrot golf hotel";
        let s = run(&mut Engine::new(&p), &[base, five, five, five]);
        // first `five` adds 5 new words → new request; the next two copies add 0 → only 2 re-prompts.
        assert!(s.is_empty(), "5 new words is new information");
    }

    #[test]
    fn observe_window_bounds_which_earlier_prompts_count() {
        // C is similar to A (0.6) but not to B; it counts only while A is still inside the window.
        let (a, b, c) = ("alpha bravo charlie delta", "alpha bravo charlie echo", "alpha bravo delta foxtrot");
        let mut p = pack();
        p.reprompt.threshold = 2;
        p.reprompt.window = 2;
        assert_eq!(run(&mut Engine::new(&p), &[a, b, c]).len(), 1);
        p.reprompt.window = 1;
        assert!(run(&mut Engine::new(&p), &[a, b, c]).is_empty());
    }

    #[test]
    fn observe_new_file_mention_resets() {
        let with_file = "refactor the login handler to return typed errors in src/auth.rs";
        assert!(run(&mut Engine::new(&pack()), &[BASE, R1, with_file, R3]).is_empty());
    }

    #[test]
    fn observe_known_file_mention_is_not_new() {
        let b = "fix src/auth.rs login handler typed errors";
        let s = run(&mut Engine::new(&pack()), &[b, b, b, b]);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn observe_pasted_block_resets() {
        let pasted = "refactor login handler typed errors ```fn x() {}```";
        assert!(run(&mut Engine::new(&pack()), &[BASE, R1, pasted, R3]).is_empty());
    }

    #[test]
    fn observe_cue_only_run_scores_half() {
        let s = run(&mut Engine::new(&pack()), &[BASE, "try again", "Still not working!", "try again"]);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].cue_only, 3);
        assert_eq!(s[0].score, 0.5);
    }

    #[test]
    fn observe_mixed_similar_and_cue_score() {
        let s = run(&mut Engine::new(&pack()), &[BASE, R1, "try again", R2]);
        assert_eq!(s.len(), 1);
        assert!((s[0].score - (2.0 + 0.5) / 3.0).abs() < 1e-9);
    }

    #[test]
    fn observe_empty_prompt_ignored() {
        let s = run(&mut Engine::new(&pack()), &[BASE, R1, "   ", "", R2, R3]);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn observe_unrelated_followup_resets() {
        let follow = "now deploy the staging environment with terraform";
        assert!(run(&mut Engine::new(&pack()), &[BASE, R1, R2, follow, R3]).is_empty());
    }

    const T0: i64 = 1_793_448_000_000; // 2026-10-31T12:00:00Z: real epoch times, not small offsets

    #[test]
    fn observe_idle_gap_over_30min_starts_new_task() {
        let mut e = Engine::new(&pack());
        assert_eq!(e.idle_gap_ms(), 30 * MIN);
        let mut sigs = 0;
        for (i, (t, text)) in [(0, BASE), (1, R1), (2, R2), (33, R3)].into_iter().enumerate() {
            let id = format!("e{i}");
            sigs +=
                e.observe("claude-code", "s", PromptObs { event_id: &id, utc_ms: T0 + t * MIN, text }).is_some() as u32;
        }
        assert_eq!(sigs, 0);
        assert_eq!(e.task_id("claude-code", "s"), Some("e3"));
    }

    #[test]
    fn observe_exactly_30min_gap_same_task() {
        let mut e = Engine::new(&pack());
        e.observe("claude-code", "s", PromptObs { event_id: "a", utc_ms: T0, text: BASE });
        e.observe("claude-code", "s", PromptObs { event_id: "b", utc_ms: T0 + 30 * MIN, text: R1 });
        assert_eq!(e.task_id("claude-code", "s"), Some("a"));
        e.observe("claude-code", "s", PromptObs { event_id: "c", utc_ms: T0 + 61 * MIN, text: R2 });
        assert_eq!(e.task_id("claude-code", "s"), Some("c"));
    }

    #[test]
    fn observe_sessions_are_independent() {
        let mut e = Engine::new(&pack());
        for (i, t) in [BASE, R1, R2].iter().enumerate() {
            e.observe("claude-code", "a", PromptObs { event_id: &format!("a{i}"), utc_ms: i as i64, text: t });
        }
        assert!(e.observe("claude-code", "b", PromptObs { event_id: "b0", utc_ms: 5, text: R3 }).is_none());
        assert!(e.observe("claude-code", "a", PromptObs { event_id: "a3", utc_ms: 6, text: R3 }).is_some());
    }

    #[test]
    fn observe_ten_thousand_prompts_one_task_bounded() {
        let mut e = Engine::new(&pack());
        let prompts: Vec<&str> = std::iter::once(BASE).chain(std::iter::repeat_n(R1, 9_999)).collect();
        let start = std::time::Instant::now();
        let mut sigs = 0;
        for (i, t) in prompts.iter().enumerate() {
            sigs += e
                .observe("claude-code", "s", PromptObs { event_id: &i.to_string(), utc_ms: i as i64, text: t })
                .is_some() as u32;
        }
        assert_eq!(sigs, 1);
        let st = e.tasks.values().next().unwrap();
        assert!(st.request.len() <= 8 && st.evidence.len() == 4);
        assert!(start.elapsed().as_secs() < 5);
    }

    #[test]
    fn prune_drops_idle_tasks_only() {
        let mut e = Engine::new(&pack());
        e.observe("claude-code", "old", PromptObs { event_id: "o", utc_ms: 0, text: BASE });
        e.observe("claude-code", "new", PromptObs { event_id: "n", utc_ms: 40 * MIN, text: BASE });
        assert_eq!(e.open_tasks(), 2);
        e.prune(41 * MIN);
        assert_eq!(e.open_tasks(), 1);
        assert_eq!(e.task_id("claude-code", "new"), Some("n"));
    }
}
