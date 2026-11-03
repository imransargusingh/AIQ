//! Live rung trend (design `docs/design/live-rung-trend.md`): where an open task is heading, after each turn,
//! from the facts AIQ reads on device (the agent's actions and the prompts). No model call, no I/O.
//!
//! It is an estimate, never a judgment: the judge also reads the model facts (goal stated, output checked,
//! whole outcome handed over), which no log can show. The judge's rung replaces it when the task ends.
//! The rules are the log-fact half of the rung guardrails (`rung.rs`), so the estimate can only say what the
//! log can see:
//! - Assisted: the agent made no edit (or, for an answer or run job, looked nothing up), or the first prompt
//!   was too short to carry a goal and context (the F4 tell, from the prompt's length).
//! - Directed: it did the work, but the person drove it a turn at a time (3 or more prompts).
//! - Delegated: it did the work from at most two prompts.
//! - Orchestrated: also a spec or brief named, sub-agents ran, and the agent checked the work itself.
//!
//! Factory is never estimated (F9, a saved workflow, is not readable today).
use crate::rung::{self, Checker, LogFacts, Turn};

/// Prompts at which the person reads as driving the agent a turn at a time (the F12 tell, from the log).
/// assume: 3 prompts = step by step. Checked against the golden set (`trend_matches_golden`); if live tasks
/// show many 3-prompt Delegated judgments, raise it.
pub const STEP_BY_STEP_PROMPTS: usize = 3;
/// A first prompt under this many words reads as a bare ask: no room for a goal, context and limits (F4).
/// A slash command is exempt (a saved workflow carries its own brief).
/// assume: length stands in for F4. Checked against the golden set; the judge's F4 replaces it at task end.
pub const BRIEF_WORDS: usize = 20;

/// One turn's estimate. `None` when the turns so far cannot settle it (a turn not recorded and no yes yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub turn: u32,
    pub at_ms: i64,
    /// Estimated rung after this turn, 1 to 4.
    pub rung: Option<u8>,
    /// This turn's actions were recorded.
    pub recorded: bool,
}

/// The reasons behind the latest estimate, in words a person reads (no rung names, no model text).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Why {
    /// "Agent made the edits itself" / "Agent looked things up itself", with the turns it did.
    pub did: Option<(bool, Vec<u32>)>,
    /// Prompts so far.
    pub prompts: u32,
    /// The first prompt was long enough to carry a goal and context (or was a slash command); None when its
    /// text is gone.
    pub brief: Option<bool>,
    pub unrecorded: Vec<u32>,
    pub spec_named: bool,
    pub subagents: u32,
    pub agent_checked: bool,
}

/// The estimate after each turn, and why the last one reads as it does. `job` is the task's job if known
/// (the first-prompt label, or a judgment): an answer or run job counts looking things up as doing the work.
pub fn trend(turns: &[Turn], job: Option<&str>) -> (Vec<Step>, Why) {
    trend_with(turns, job, turns.first().map(|t| briefed(&t.prompt)))
}

/// `trend` with the first prompt's brief given: `None` when its text is gone (deleted), so a turn where the
/// agent did the work has no estimate rather than a guessed one.
pub fn trend_with(turns: &[Turn], job: Option<&str>, brief: Option<bool>) -> (Vec<Step>, Why) {
    let ran_itself = job.is_some_and(|j| rung::RAN_ITSELF_JOBS.contains(&j));
    let answer = job == Some(rung::ANSWER_JOB);
    let steps = (1..=turns.len())
        .map(|n| Step {
            turn: n as u32,
            at_ms: turns[n - 1].at_ms,
            rung: estimate(&rung::log_facts(&turns[..n]), n, brief, ran_itself, answer),
            recorded: turns[n - 1].actions.is_some(),
        })
        .collect();
    let lf = rung::log_facts(turns);
    let did_turns: Vec<u32> = turns
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            let one = rung::log_facts(std::slice::from_ref(t));
            t.actions.is_some() && did(&one, ran_itself) == Some(true)
        })
        .map(|(i, _)| i as u32 + 1)
        .collect();
    let why = Why {
        did: did(&lf, ran_itself).map(|_| (ran_itself, did_turns)),
        prompts: turns.len() as u32,
        brief,
        unrecorded: turns.iter().enumerate().filter(|(_, t)| t.actions.is_none()).map(|(i, _)| i as u32 + 1).collect(),
        spec_named: lf.spec_named == Some(true),
        subagents: lf.subagents.unwrap_or(0),
        agent_checked: lf.checks_by == Some(Checker::Agent),
    };
    (steps, why)
}

fn did(lf: &LogFacts, ran_itself: bool) -> Option<bool> {
    if ran_itself { lf.looked_up } else { lf.agent_edited }
}

/// The first prompt carries room for a goal and context, or runs a saved command.
pub fn briefed(prompt: &str) -> bool {
    let p = prompt.trim_start();
    p.starts_with('/') || p.split_whitespace().count() >= BRIEF_WORDS
}

fn estimate(lf: &LogFacts, prompts: usize, brief: Option<bool>, ran_itself: bool, answer: bool) -> Option<u8> {
    match did(lf, ran_itself)? {
        false => Some(1),
        true if !brief? => Some(1),
        true if prompts >= STEP_BY_STEP_PROMPTS => Some(2),
        true => {
            let split = if answer {
                lf.subagents.is_some_and(|n| n > 0)
            } else {
                lf.coordinated == Some(true) && lf.checks_by == Some(Checker::Agent)
            };
            Some(if lf.spec_named == Some(true) && split { 4 } else { 3 })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aiq_contracts::judge::ActionSummary;

    fn turn(at: i64, prompt: &str, tools: &[(&str, u32)], cmds: &[&str], files: &[&str], subagents: u32) -> Turn {
        Turn {
            at_ms: at,
            prompt: prompt.into(),
            actions: Some(ActionSummary {
                tools: tools.iter().map(|(k, n)| (k.to_string(), *n)).collect(),
                files: files.iter().map(|s| s.to_string()).collect(),
                commands: cmds.iter().map(|s| s.to_string()).collect(),
                subagents,
            }),
        }
    }

    /// A first prompt with room for a goal, context and limits (BRIEF_WORDS or more).
    const BRIEF: &str = "Explain what min_gap does in src/headway.rs, then fix the off-by-one when two trains leave in the same minute; keep the public API unchanged";

    #[test]
    fn a_bare_ask_reads_assisted_even_when_the_agent_edits() {
        let t = vec![turn(1, "fix the typo in src/parse.rs", &[("Edit", 1)], &[], &["src/parse.rs"], 0)];
        assert_eq!(trend(&t, Some("code_change")).0[0].rung, Some(1));
        assert_eq!(trend(&t, None).1.brief, Some(false));
        assert_eq!(trend_with(&t, None, None).0[0].rung, None);
        assert!(briefed("/fix-flaky test_reroute"));
    }

    #[test]
    fn rises_when_the_agent_edits_and_drops_when_driven_a_turn_at_a_time() {
        let t = vec![
            turn(1, BRIEF, &[("Read", 1)], &[], &[], 0),
            turn(2, "fix the off-by-one", &[("Edit", 1)], &[], &["src/a.rs"], 0),
            turn(3, "now the other one", &[("Edit", 1)], &[], &["src/a.rs"], 0),
        ];
        let (s, why) = trend(&t, Some("code_change"));
        assert_eq!(s.iter().map(|s| s.rung).collect::<Vec<_>>(), [Some(1), Some(3), Some(2)]);
        assert_eq!(why.did, Some((false, vec![2, 3])));
        assert_eq!(why.prompts, 3);
    }

    #[test]
    fn an_unrecorded_turn_is_no_estimate_until_a_yes_is_seen() {
        let t = vec![
            Turn { at_ms: 1, prompt: BRIEF.into(), actions: None },
            turn(2, "and this", &[("Edit", 1)], &[], &["x.md"], 0),
        ];
        let (s, why) = trend(&t, None);
        assert_eq!(s[0].rung, None);
        assert!(!s[0].recorded);
        assert_eq!(s[1].rung, Some(3));
        assert_eq!(why.unrecorded, vec![1]);
    }

    #[test]
    fn spec_sub_agents_and_own_checks_read_orchestrated() {
        let t = vec![turn(
            1,
            "Build docs/design/x.md end to end: split the work across agents, run cargo test after each part, and stop if the schema needs to change at all",
            &[("Task", 2), ("Edit", 4)],
            &["cargo test"],
            &["src/x.rs"],
            2,
        )];
        assert_eq!(trend(&t, Some("code_change")).0[0].rung, Some(4));
        let ask = vec![turn(
            1,
            "Compare A and B per the brief in docs/brief.md: cost, latency and how each fails; split the reading across sub-agents and cite every claim",
            &[("Task", 2)],
            &[],
            &[],
            2,
        )];
        assert_eq!(trend(&ask, Some("ask_research")).0[0].rung, Some(4));
    }

    #[test]
    fn an_answer_job_counts_looking_up_as_doing_the_work() {
        let t = vec![turn(
            1,
            "Why does the build fail on Windows since yesterday? Context: only the linker step fails, Linux is green; look at the CI log and the last three merges",
            &[("Grep", 2), ("Read", 3)],
            &[],
            &[],
            0,
        )];
        assert_eq!(trend(&t, Some("ask_research")).0[0].rung, Some(3));
        assert_eq!(trend(&t, Some("code_change")).0[0].rung, Some(1));
    }
}
