//! `aiq.event.v1`: one collector event. JSON Schema: `contracts/aiq.event.v1.json`.
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "aiq.event.v1";
/// Prompt text is cut at this many bytes (on a char boundary) and marked `truncated`.
pub const MAX_PROMPT_BYTES: usize = 64 * 1024;
pub const MAX_SESSION_ID: usize = 128;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventV1 {
    pub schema: String,
    /// UUIDv7, set by the hook client; the idempotency key.
    pub event_id: String,
    pub surface: String,
    pub surface_session_id: String,
    /// Deviation from ADR-0005: Claude Code gives no sequence number, so this is null.
    pub seq: Option<i64>,
    /// RFC 3339 UTC, client clock.
    pub ts: String,
    pub collector_version: String,
    #[serde(flatten)]
    pub body: EventBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum EventBody {
    Prompt(PromptPayload),
    Stop(StopPayload),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PromptPayload {
    /// None when the host sent no prompt field (payload drift; stored with no content).
    pub text: Option<String>,
    pub chars: u64,
    pub prompt_id: Option<String>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StopPayload {
    pub stop_hook_active: bool,
    pub prompt_id: Option<String>,
}

impl EventV1 {
    pub fn kind(&self) -> &'static str {
        match self.body {
            EventBody::Prompt(_) => "prompt",
            EventBody::Stop(_) => "stop",
        }
    }
}

/// Cut `s` to at most `max` bytes on a char boundary. Returns (text, truncated).
pub fn cap_text(s: &str, max: usize) -> (String, bool) {
    if s.len() <= max {
        return (s.to_owned(), false);
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample_prompt() -> EventV1 {
        EventV1 {
            schema: SCHEMA.into(),
            event_id: "01a1935c-3bba-74ae-a607-3e84a3d4c8e7".into(),
            surface: "claude-code".into(),
            surface_session_id: "s1".into(),
            seq: None,
            ts: "2026-10-31T12:00:00Z".into(),
            collector_version: "0.1.0".into(),
            body: EventBody::Prompt(PromptPayload {
                text: Some("fix it".into()),
                chars: 6,
                prompt_id: Some("p1".into()),
                truncated: false,
            }),
        }
    }

    #[test]
    fn event_roundtrip_kind_and_payload_adjacently_tagged() {
        let e = sample_prompt();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "prompt");
        assert_eq!(v["payload"]["text"], "fix it");
        assert_eq!(serde_json::from_value::<EventV1>(v).unwrap(), e);
    }

    #[test]
    fn event_unknown_fields_ignored() {
        let mut v = serde_json::to_value(sample_prompt()).unwrap();
        v["future_field"] = serde_json::json!(1);
        v["payload"]["future"] = serde_json::json!("x");
        assert!(serde_json::from_value::<EventV1>(v).is_ok());
    }

    #[test]
    fn cap_text_multibyte_boundary_never_splits_char() {
        let s = "é".repeat(10); // 20 bytes
        let (t, cut) = cap_text(&s, 5);
        assert!(cut);
        assert_eq!(t, "éé");
        assert_eq!(cap_text("abc", 3), ("abc".into(), false));
    }
}
