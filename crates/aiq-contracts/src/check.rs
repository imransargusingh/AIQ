//! `aiq.check.v1`: the second, blind reading of one judged task (design `docs/design/judge-accuracy-check.md`
//! K2). The grader never sees the judge's answer. Each model-read fact may be `null`: the transcript can't
//! settle it. Parsed as strictly as the judge's answer; anything else is "not checked", never guessed.
use crate::judge::{JOBS, task_type_ids};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "aiq.check.v1";

/// The model-read facts the grader answers, in the judge's own names.
pub const FACTS: [&str; 6] = ["F4", "F5", "F6", "F7", "F12", "F13"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckAnswer {
    pub job: String,
    pub task_type: String,
    pub current_rung: u8,
    pub f4_goal_context_constraints: Option<bool>,
    pub f5_reprompts_added_information: Option<bool>,
    pub f6_output_checked: Option<bool>,
    pub f7_written_brief: Option<bool>,
    pub f12_step_by_step: Option<bool>,
    pub f13_whole_outcome_handed_over: Option<bool>,
}

impl CheckAnswer {
    /// (fact id, reading) in `FACTS` order; None = can't settle.
    pub fn facts(&self) -> [(&'static str, Option<bool>); 6] {
        [
            ("F4", self.f4_goal_context_constraints),
            ("F5", self.f5_reprompts_added_information),
            ("F6", self.f6_output_checked),
            ("F7", self.f7_written_brief),
            ("F12", self.f12_step_by_step),
            ("F13", self.f13_whole_outcome_handed_over),
        ]
    }
}

fn fact(desc: &str) -> serde_json::Value {
    serde_json::json!({"type": ["boolean", "null"], "description": desc})
}

pub fn check_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "job": {"type": "string", "enum": JOBS},
            "task_type": {"type": "string", "enum": task_type_ids()},
            "current_rung": {"type": "integer", "minimum": 1, "maximum": 5},
            "f4_goal_context_constraints": fact("The first request stated a goal, context and constraints; null if the transcript can't settle it"),
            "f5_reprompts_added_information": fact("Every re-prompt added new information (true if there were no re-prompts); null if it can't be settled"),
            "f6_output_checked": fact("The output was reviewed or checked before use; null if the transcript can't show it"),
            "f7_written_brief": fact("A written spec, brief, template or instructions file was given to the agent; null if it can't be settled"),
            "f12_step_by_step": fact("The person drove the agent one small instruction per turn; null if it can't be settled"),
            "f13_whole_outcome_handed_over": fact("A whole outcome was handed over with done criteria; null if it can't be settled")
        },
        "required": ["job", "task_type", "current_rung", "f4_goal_context_constraints", "f5_reprompts_added_information",
            "f6_output_checked", "f7_written_brief", "f12_step_by_step", "f13_whole_outcome_handed_over"]
    })
}

/// Parse and check one grader answer. Any failure means the task counts as not checked.
pub fn parse_check(v: serde_json::Value) -> Result<CheckAnswer, String> {
    // A missing fact must not read as "can't settle": every key is required, `null` only when written.
    let Some(o) = v.as_object() else { return Err("shape: not an object".into()) };
    for k in check_schema()["required"].as_array().into_iter().flatten().filter_map(|k| k.as_str()) {
        if !o.contains_key(k) {
            return Err(format!("shape: missing {k}"));
        }
    }
    let a: CheckAnswer = serde_json::from_value(v).map_err(|e| format!("shape: {e}"))?;
    if !JOBS.contains(&a.job.as_str()) {
        return Err(format!("job: {}", a.job));
    }
    if !task_type_ids().contains(&a.task_type.as_str()) {
        return Err(format!("task_type: {}", a.task_type));
    }
    if !(1..=5).contains(&a.current_rung) {
        return Err(format!("current_rung: {} out of 1..5", a.current_rung));
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn good() -> serde_json::Value {
        json!({"job": "ask_research", "task_type": "explain", "current_rung": 1,
            "f4_goal_context_constraints": false, "f5_reprompts_added_information": true, "f6_output_checked": null,
            "f7_written_brief": false, "f12_step_by_step": false, "f13_whole_outcome_handed_over": null})
    }

    #[test]
    fn null_fact_is_cant_settle() {
        let a = parse_check(good()).unwrap();
        assert_eq!(a.facts()[2], ("F6", None));
        assert_eq!(a.facts()[0], ("F4", Some(false)));
    }

    #[test]
    fn rejects_unknown_keys_bad_job_type_and_rung() {
        let mut v = good();
        v["confidence"] = json!(0.9);
        assert!(parse_check(v).is_err(), "unknown key");
        for (k, bad) in [("job", json!("chat")), ("task_type", json!("poetry")), ("current_rung", json!(6))] {
            let mut v = good();
            v[k] = bad;
            assert!(parse_check(v).is_err(), "{k}");
        }
        let mut v = good();
        v.as_object_mut().unwrap().remove("f6_output_checked");
        assert!(parse_check(v).is_err(), "missing fact");
    }

    #[test]
    fn schema_lists_every_field_as_required() {
        let s = check_schema();
        let req: Vec<&str> = s["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        let props: Vec<&String> = s["properties"].as_object().unwrap().keys().collect();
        assert_eq!(req.len(), props.len());
    }
}
