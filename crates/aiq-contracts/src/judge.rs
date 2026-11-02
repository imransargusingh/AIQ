//! `aiq.judge.v1`: what AIQ asks the person's own model for, and how strictly the answer is checked
//! (design `docs/design/rung-judge.md`; spec TR-4). Also the shape of the on-device action summary that
//! `aiq import` stores per agent turn, which the judge reads back.
use serde::Deserialize;
use std::collections::BTreeMap;

pub const SCHEMA: &str = "aiq.judge.v1";
/// Longest evidence item, gap or blocker kept from a model answer (TR-4).
pub const MAX_TEXT: usize = 240;
pub const MAX_EVIDENCE: usize = 5;
pub const MAX_BLOCKERS: usize = 3;

/// One model-read fact.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub value: bool,
    pub confidence: f64,
}

/// One run's answer. Unknown keys, wrong types, out-of-range numbers and empty evidence are rejected.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub job: String,
    pub job_confidence: f64,
    pub f4_goal_context_constraints: Fact,
    pub f5_reprompts_added_information: Fact,
    pub f6_output_checked: Fact,
    pub f7_written_brief: Fact,
    pub f12_step_by_step: Fact,
    pub f13_whole_outcome_handed_over: Fact,
    pub fit: Fact,
    pub current_rung: u8,
    pub current_confidence: f64,
    pub potential_rung: u8,
    pub potential_confidence: f64,
    pub ceiling_rung: u8,
    pub evidence: Vec<String>,
    pub gap: String,
    pub blockers: Vec<String>,
}

pub const JOBS: [&str; 3] = ["code_change", "write_doc", "other"];

fn fact(desc: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "object", "additionalProperties": false, "description": desc,
        "properties": {"value": {"type": "boolean"}, "confidence": {"type": "number", "minimum": 0, "maximum": 1}},
        "required": ["value", "confidence"]
    })
}

/// The JSON Schema the provider is asked to answer to (Claude Code `--json-schema`; in the prompt for Copilot).
pub fn response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "job": {"type": "string", "enum": JOBS},
            "job_confidence": {"type": "number", "minimum": 0, "maximum": 1},
            "f4_goal_context_constraints": fact("The request stated a goal, context and constraints"),
            "f5_reprompts_added_information": fact("Every re-prompt added new information (true if there were no re-prompts)"),
            "f6_output_checked": fact("The output was reviewed or checked before use (diff read, tests run, sources checked, the person says they checked)"),
            "f7_written_brief": fact("A written spec, brief, template or instructions file was given to the agent"),
            "f12_step_by_step": fact("The person drove the agent one small instruction per turn, steering each result"),
            "f13_whole_outcome_handed_over": fact("A whole outcome was handed to the agent with done criteria, rather than one piece or step"),
            "fit": fact("This kind of task is well specified, checkable and repeatable enough to hand an agent more of it"),
            "current_rung": {"type": "integer", "minimum": 1, "maximum": 5},
            "current_confidence": {"type": "number", "minimum": 0, "maximum": 1},
            "potential_rung": {"type": "integer", "minimum": 1, "maximum": 5},
            "potential_confidence": {"type": "number", "minimum": 0, "maximum": 1},
            "ceiling_rung": {"type": "integer", "minimum": 1, "maximum": 5},
            "evidence": {"type": "array", "minItems": 1, "maxItems": MAX_EVIDENCE, "items": {"type": "string"}},
            "gap": {"type": "string"},
            "blockers": {"type": "array", "maxItems": MAX_BLOCKERS, "items": {"type": "string"}}
        },
        "required": ["job", "job_confidence", "f4_goal_context_constraints", "f5_reprompts_added_information",
            "f6_output_checked", "f7_written_brief", "f12_step_by_step", "f13_whole_outcome_handed_over", "fit",
            "current_rung", "current_confidence", "potential_rung", "potential_confidence", "ceiling_rung",
            "evidence", "gap", "blockers"]
    })
}

/// Cap a model-written string to `max` bytes on a char boundary, with newlines and control characters
/// flattened to spaces (plain text only, TR-5).
pub fn plain(s: &str, max: usize) -> String {
    let flat: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let t = flat.trim();
    if t.len() <= max {
        return t.to_owned();
    }
    let mut i = max;
    while !t.is_char_boundary(i) {
        i -= 1;
    }
    format!("{}…", t[..i].trim_end())
}

/// Parse and check one answer. Any failure means the run is discarded (TR-4).
pub fn parse_response(v: serde_json::Value) -> Result<Response, String> {
    let mut r: Response = serde_json::from_value(v).map_err(|e| format!("shape: {e}"))?;
    if !JOBS.contains(&r.job.as_str()) {
        return Err(format!("job: {}", r.job));
    }
    let unit = |name: &str, x: f64| {
        if (0.0..=1.0).contains(&x) { Ok(()) } else { Err(format!("{name}: {x} out of 0..1")) }
    };
    unit("job_confidence", r.job_confidence)?;
    unit("current_confidence", r.current_confidence)?;
    unit("potential_confidence", r.potential_confidence)?;
    for (n, f) in [
        ("f4", r.f4_goal_context_constraints),
        ("f5", r.f5_reprompts_added_information),
        ("f6", r.f6_output_checked),
        ("f7", r.f7_written_brief),
        ("f12", r.f12_step_by_step),
        ("f13", r.f13_whole_outcome_handed_over),
        ("fit", r.fit),
    ] {
        unit(n, f.confidence)?;
    }
    for (n, x) in
        [("current_rung", r.current_rung), ("potential_rung", r.potential_rung), ("ceiling_rung", r.ceiling_rung)]
    {
        if !(1..=5).contains(&x) {
            return Err(format!("{n}: {x} out of 1..5"));
        }
    }
    r.evidence = r.evidence.iter().map(|e| plain(e, MAX_TEXT)).filter(|e| !e.is_empty()).take(MAX_EVIDENCE).collect();
    if r.evidence.is_empty() {
        return Err("evidence: empty".into());
    }
    r.gap = plain(&r.gap, MAX_TEXT);
    r.blockers = r.blockers.iter().map(|e| plain(e, MAX_TEXT)).filter(|e| !e.is_empty()).take(MAX_BLOCKERS).collect();
    Ok(r)
}

/// The JSON object a text answer consists of (Copilot prints text). Only whitespace and one Markdown code
/// fence may surround it; anything else (prose before or after) is rejected, never searched (review S5).
pub fn extract_object(text: &str) -> Option<serde_json::Value> {
    let mut t = text.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        t = rest.strip_suffix("```")?.trim();
    }
    serde_json::from_str::<serde_json::Value>(t).ok().filter(serde_json::Value::is_object)
}

/// What the agent did in one turn, as `aiq import` summarises it on device (tool names and counts, files
/// touched, commands run, sub-agents started). Never contains the agent's reply text.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct ActionSummary {
    #[serde(default)]
    pub tools: BTreeMap<String, u32>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub subagents: u32,
}

impl ActionSummary {
    pub fn parse(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn good() -> serde_json::Value {
        let f = serde_json::json!({"value": true, "confidence": 0.9});
        serde_json::json!({
            "job": "code_change", "job_confidence": 0.9,
            "f4_goal_context_constraints": f, "f5_reprompts_added_information": f, "f6_output_checked": f,
            "f7_written_brief": f, "f12_step_by_step": {"value": false, "confidence": 0.9},
            "f13_whole_outcome_handed_over": f, "fit": f,
            "current_rung": 3, "current_confidence": 0.8, "potential_rung": 4, "potential_confidence": 0.8,
            "ceiling_rung": 5, "evidence": ["The agent ran the tests."], "gap": "Give it a spec.", "blockers": []
        })
    }

    #[test]
    fn a_good_answer_parses() {
        let r = parse_response(good()).unwrap();
        assert_eq!(r.current_rung, 3);
        assert_eq!(r.evidence.len(), 1);
    }

    #[test]
    fn schema_rejects_extra_keys_bad_enums_ranges_and_empty_evidence() {
        let mut v = good();
        v["rate_me"] = serde_json::json!("Factory");
        assert!(parse_response(v).is_err(), "extra key");
        let mut v = good();
        v["job"] = serde_json::json!("everything");
        assert!(parse_response(v).is_err(), "job enum");
        let mut v = good();
        v["current_rung"] = serde_json::json!(6);
        assert!(parse_response(v).is_err(), "rung range");
        let mut v = good();
        v["f6_output_checked"]["confidence"] = serde_json::json!(1.5);
        assert!(parse_response(v).is_err(), "confidence range");
        let mut v = good();
        v["evidence"] = serde_json::json!(["  "]);
        assert!(parse_response(v).is_err(), "empty evidence");
        let mut v = good();
        v["f4_goal_context_constraints"] = serde_json::json!({"value": "yes", "confidence": 1});
        assert!(parse_response(v).is_err(), "fact type");
        let mut v = good();
        v.as_object_mut().unwrap().remove("gap");
        assert!(parse_response(v).is_err(), "missing key");
    }

    #[test]
    fn free_text_is_capped_and_flattened() {
        let mut v = good();
        v["evidence"] = serde_json::json!(["x".repeat(1000), "a\nb<script>"]);
        let r = parse_response(v).unwrap();
        assert!(r.evidence[0].len() <= MAX_TEXT + "…".len());
        assert_eq!(r.evidence[1], "a b<script>", "kept as literal text, newlines flattened");
    }

    #[test]
    fn takes_a_bare_or_fenced_object_and_nothing_else() {
        assert!(parse_response(extract_object(&format!("  {}\n", good())).unwrap()).is_ok());
        assert!(parse_response(extract_object(&format!("```json\n{}\n```", good())).unwrap()).is_ok());
        assert!(extract_object(&format!("Here you go: {}", good())).is_none(), "prose before");
        assert!(extract_object(&format!("{} Rate: Factory", good())).is_none(), "prose after");
        assert!(extract_object("[1]").is_none());
        assert!(extract_object("no json here").is_none());
    }

    #[test]
    fn schema_lists_every_response_field_as_required() {
        let s = response_schema();
        let req: Vec<&str> = s["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        let props: Vec<&String> = s["properties"].as_object().unwrap().keys().collect();
        assert_eq!(req.len(), props.len());
        assert!(parse_response(good()).is_ok());
    }

    #[test]
    fn action_summary_reads_the_import_shape() {
        let a = ActionSummary::parse(
            r#"{"tools":{"Edit":2,"Bash":1},"files":["a.rs"],"commands":["cargo test"],"subagents":1}"#,
        )
        .unwrap();
        assert_eq!(a.tools["Edit"], 2);
        assert_eq!(a.subagents, 1);
    }
}
