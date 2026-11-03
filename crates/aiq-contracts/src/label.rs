//! The kind-of-task label's answer contract (design `docs/design/kind-label.md`): the JSON Schema the small
//! model answers to, and the strict parse. The vocabulary is the judge's (`judge::JOBS`, `judge::TASK_TYPES`).
use crate::judge::{JOBS, TASK_TYPES, task_type_label};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub job: String,
    pub job_confidence: f64,
    pub task_type: String,
    pub task_type_confidence: f64,
}

pub fn schema() -> Value {
    let mut types: Vec<&str> = Vec::new();
    for (_, id, _) in TASK_TYPES {
        if !types.contains(&id) {
            types.push(id);
        }
    }
    json!({
        "type": "object", "additionalProperties": false,
        "properties": {
            "job": {"type": "string", "enum": JOBS},
            "job_confidence": {"type": "number", "minimum": 0, "maximum": 1},
            "task_type": {"type": "string", "enum": types},
            "task_type_confidence": {"type": "number", "minimum": 0, "maximum": 1}
        },
        "required": ["job", "job_confidence", "task_type", "task_type_confidence"]
    })
}

/// Strict read of one answer: a known job, and a kind that belongs to that job (or `other`).
pub fn parse(v: &Value) -> Result<Label, String> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned).ok_or_else(|| format!("no {k}"));
    let c = |k: &str| {
        v.get(k).and_then(Value::as_f64).filter(|x| (0.0..=1.0).contains(x)).ok_or_else(|| format!("bad {k}"))
    };
    let l = Label {
        job: s("job")?,
        job_confidence: c("job_confidence")?,
        task_type: s("task_type")?,
        task_type_confidence: c("task_type_confidence")?,
    };
    if !JOBS.contains(&l.job.as_str()) {
        return Err(format!("unknown job {}", l.job));
    }
    if l.task_type != "other" && task_type_label(&l.job, &l.task_type).is_none() {
        return Err(format!("{} is not a kind of {}", l.task_type, l.job));
    }
    Ok(l)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn ans(job: &str, jc: f64, ty: &str, tc: f64) -> Value {
        json!({"job": job, "job_confidence": jc, "task_type": ty, "task_type_confidence": tc})
    }

    #[test]
    fn schema_lists_every_job_and_kind_once() {
        let v = schema();
        assert_eq!(v["properties"]["job"]["enum"].as_array().unwrap().len(), JOBS.len());
        let kinds = v["properties"]["task_type"]["enum"].as_array().unwrap();
        for (_, id, _) in TASK_TYPES {
            assert_eq!(kinds.iter().filter(|k| k.as_str() == Some(id)).count(), 1, "{id}");
        }
    }

    #[test]
    fn parse_is_strict() {
        assert_eq!(parse(&ans("code_change", 0.9, "fix_bug", 0.85)).unwrap().task_type, "fix_bug");
        assert!(parse(&ans("code_change", 0.9, "other", 0.5)).is_ok());
        assert!(parse(&ans("code_change", 0.9, "release", 0.9)).is_err(), "a kind of another job");
        assert!(parse(&ans("chat", 0.9, "other", 0.9)).is_err());
        assert!(parse(&ans("write_doc", 1.2, "spec", 0.9)).is_err());
        assert!(parse(&json!({"job": "write_doc"})).is_err());
    }
}
