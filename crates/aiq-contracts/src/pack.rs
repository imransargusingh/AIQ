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
    pub tips: Vec<Tip>,
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
        Ok(())
    }
}
