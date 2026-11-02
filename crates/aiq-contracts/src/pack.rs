//! Pack schema v1. Phase 1 ships one bundled, unsigned pack behind the `PackSource` seam; the Phase 4
//! signed-pack loader must produce the same type.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackV1 {
    pub schema: String,
    pub pack_id: String,
    pub version: String,
    /// A Task closes when consecutive prompts are further apart than this.
    pub idle_gap_min: u32,
    pub reprompt: RepromptParams,
    pub trigger: TriggerParams,
    #[serde(default)]
    pub automated: AutomatedParams,
    pub tips: Vec<Tip>,
    /// Coaching from rung results (design coaching-from-rungs). Absent in older packs: defaults.
    #[serde(default)]
    pub coach: CoachParams,
    /// One move per rung step and fact, with pack-written text (never model text).
    #[serde(default)]
    pub moves: Vec<MoveDef>,
}

/// Coaching parameters (design coaching-from-rungs, "New pack parameters"). Every value is a first guess
/// until measured on real use, except where the comment says otherwise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CoachParams {
    /// Window for task-type rows, the split and type-guess neighbours (Home's window, spec Q3).
    pub window_days: u32,
    /// A task type is recurring with at least this many judged tasks in the window (D3).
    pub recurring_min: u32,
    /// The same move must be coachable on more than this share of the type's tasks at its rung (D3).
    pub type_majority: f64,
    /// Nearest-neighbour type guess: best Jaccard at least this (D5; guess, to set from `aiq coach check-guess`).
    pub match_threshold: f64,
    /// ... and at least this many of the top `match_top` agree on the type.
    pub match_agree: u32,
    pub match_top: u32,
    /// Self-check (D10): with at least `guess_check_min` checked guesses, below `guess_precision` right turns
    /// next-rung tips off by AIQ itself.
    pub guess_check_min: u32,
    pub guess_precision: f64,
    /// The same tip at most once per this many hours (D12).
    pub move_cooldown_h: u32,
    /// Home's split shows with at least this many tasks (aiq-home US-H2, spec value).
    pub split_min: u32,
    /// A week with fewer tasks shows "too few tasks".
    pub week_min: u32,
    /// `delegated_no_correction` (Imran 2026-11-02, Q-A) until F2 exists; then `f2_share`.
    pub kept_rule: String,
    /// "Snooze" lasts this many days (coaching-loop US-4, spec value).
    pub snooze_days: u32,
    /// Two "Not helpful" (or "not useful" flags) within this many days rank a tip last (spec value).
    pub not_helpful_days: u32,
    /// Habit window and minimum eligible tasks for Adopted (coaching-loop D5, spec values).
    pub habit_days: u32,
    pub habit_min: u32,
}

impl Default for CoachParams {
    fn default() -> Self {
        Self {
            window_days: 30,
            recurring_min: 3,
            type_majority: 0.5,
            match_threshold: 0.45,
            match_agree: 2,
            match_top: 3,
            guess_check_min: 10,
            guess_precision: 0.8,
            move_cooldown_h: 72,
            split_min: 10,
            week_min: 3,
            kept_rule: "delegated_no_correction".into(),
            snooze_days: 7,
            not_helpful_days: 30,
            habit_days: 14,
            habit_min: 3,
        }
    }
}

/// One coached move: the step it climbs, the guardrail facts that make it the move, and its text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveDef {
    pub move_id: String,
    /// The rung the move starts from (1..=4); it is coached toward `step + 1`.
    pub step: u8,
    /// Guardrail fact ids of rung `step + 1` this move answers (a confident "no" on any of them).
    pub facts: Vec<String>,
    /// `any`, `code_change` or `write_doc`.
    pub job: String,
    /// The Habits page name.
    pub habit: String,
    /// The card's "What we noticed"; `{type}` is the task type's label, lower case.
    pub noticed: String,
    pub why: String,
    pub try_this: String,
    /// A starter prompt with `<placeholders>`, copied by the person, never sent.
    pub starter: String,
    /// A model-read fact must be at least this confident before its "no" is coached. For facts the real-data
    /// fact check found weak (F4 read right 10 of 15, PR #27). Log facts are always 1.0.
    #[serde(default)]
    pub min_confidence: Option<f64>,
}

/// Facts a move may name (the rung guardrails' facts, rung-model fact set fs1).
pub const MOVE_FACTS: [&str; 10] = ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F12", "F13"];

/// The import spec's automated-session rule, live side (docs/specs/history-import.md, US-12). Defaults are
/// the spec's tested values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomatedParams {
    pub burst_sessions: u32,
    pub burst_min: u32,
    pub template_sessions: u32,
    pub template_window_h: u32,
    pub template_chars: usize,
}

impl Default for AutomatedParams {
    fn default() -> Self {
        Self { burst_sessions: 50, burst_min: 30, template_sessions: 11, template_window_h: 24, template_chars: 200 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepromptParams {
    pub detector_id: String,
    pub detector_version: u32,
    pub opportunity_id: String,
    /// Opportunity applies to every Job, including Unclassified.
    pub core_opportunity: bool,
    pub similarity: f64,
    pub max_new_words: usize,
    pub threshold: u32,
    /// How many earlier prompts of the current request are compared.
    pub window: usize,
    pub min_word_len: usize,
    pub stop_words: Vec<String>,
    pub cues: Vec<String>,
    /// Surfaces whose sessions are one stream for the Task rule. VS Code agent mode can start a new
    /// Copilot session every turn (laptop probe run 2), so a reword loop there spans sessions.
    #[serde(default)]
    pub span_surfaces: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerParams {
    pub rule_id: String,
    pub in_moment_threshold: f64,
    pub ready_expiry_min: u32,
    pub default_daily_cap: u32,
    /// After a card is delivered on any surface, cards on every surface are held this many minutes
    /// (Phase 2: the budget is per person across surfaces).
    pub quiet_min: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tip {
    pub tip_id: String,
    pub opportunity_id: String,
    /// May contain `{count}`.
    pub noticed: String,
    pub why: String,
    pub try_this: String,
}

impl PackV1 {
    /// Structural checks the Phase 4 loader must also run.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != "aiq.pack.v1" {
            return Err(format!("unknown pack schema {}", self.schema));
        }
        let r = &self.reprompt;
        if !(0.0..=1.0).contains(&r.similarity) || !(0.0..=1.0).contains(&self.trigger.in_moment_threshold) {
            return Err("similarity and threshold must be in 0..=1".into());
        }
        if r.threshold == 0 || r.window == 0 || self.idle_gap_min == 0 {
            return Err("threshold, window and idle gap must be positive".into());
        }
        if !self.tips.iter().any(|t| t.opportunity_id == r.opportunity_id) {
            return Err(format!("no tip for opportunity {}", r.opportunity_id));
        }
        let c = &self.coach;
        for (n, x) in [("type_majority", c.type_majority), ("match_threshold", c.match_threshold), ("guess_precision", c.guess_precision)] {
            if !(0.0..=1.0).contains(&x) {
                return Err(format!("coach.{n} must be in 0..=1"));
            }
        }
        if c.match_agree == 0 || c.match_agree > c.match_top || c.window_days == 0 || c.habit_days == 0 {
            return Err("coach: match_agree must be 1..=match_top and windows positive".into());
        }
        if !["delegated_no_correction", "f2_share"].contains(&c.kept_rule.as_str()) {
            return Err(format!("coach.kept_rule {}", c.kept_rule));
        }
        let mut seen = std::collections::BTreeSet::new();
        for m in &self.moves {
            if !seen.insert(m.move_id.as_str()) {
                return Err(format!("move {} appears twice", m.move_id));
            }
            if m.min_confidence.is_some_and(|c| !(0.0..=1.0).contains(&c)) {
                return Err(format!("move {}: min_confidence in 0..=1", m.move_id));
            }
            if !(1..=4).contains(&m.step) || m.facts.is_empty() {
                return Err(format!("move {}: step 1..=4 and at least one fact", m.move_id));
            }
            if let Some(f) = m.facts.iter().find(|f| !MOVE_FACTS.contains(&f.as_str())) {
                return Err(format!("move {}: unknown fact {f}", m.move_id));
            }
            if !["any", "code_change", "write_doc"].contains(&m.job.as_str()) {
                return Err(format!("move {}: job {}", m.move_id, m.job));
            }
            let text = [&m.noticed, &m.why, &m.try_this, &m.starter, &m.habit];
            if text.iter().any(|t| t.is_empty() || t.contains('\n')) || !m.noticed.contains("{type}") {
                return Err(format!("move {}: one-line text, noticed names {{type}}", m.move_id));
            }
        }
        Ok(())
    }
}
