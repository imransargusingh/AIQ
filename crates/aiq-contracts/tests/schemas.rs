//! D6: the serde types and the published JSON Schemas agree.
use aiq_contracts::card::{self, CardV1};
use aiq_contracts::event::{self, EventBody, EventV1, PromptPayload, StopPayload};
use serde_json::Value;

fn schema(name: &str) -> jsonschema::Validator {
    let p = format!("{}/../../contracts/{name}", env!("CARGO_MANIFEST_DIR"));
    let v: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    jsonschema::validator_for(&v).unwrap()
}

fn ev(body: EventBody) -> EventV1 {
    EventV1 {
        schema: event::SCHEMA.into(),
        event_id: "01a1935c-3bba-74ae-a607-3e84a3d4c8e7".into(),
        surface: "claude-code".into(),
        surface_session_id: "s".into(),
        seq: None,
        ts: "2026-10-31T12:00:00Z".into(),
        collector_version: "0.1.0".into(),
        cwd: None,
        projects: false,
        body,
    }
}

#[test]
fn event_schema_accepts_serialized_prompt_and_stop() {
    let s = schema("aiq.event.v1.json");
    let p =
        ev(EventBody::Prompt(PromptPayload { text: Some("x".into()), chars: 1, prompt_id: None, truncated: false }));
    let st = ev(EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: Some("p".into()) }));
    let missing = ev(EventBody::Prompt(PromptPayload { text: None, chars: 0, prompt_id: None, truncated: false }));
    for e in [p, st, missing] {
        let v = serde_json::to_value(&e).unwrap();
        assert!(s.is_valid(&v), "{v}");
    }
}

#[test]
fn event_schema_surfaces_phase2_accepted_unknown_rejected() {
    let s = schema("aiq.event.v1.json");
    for (surface, ok) in [("copilot-cli", true), ("vscode", true), ("cursor", false)] {
        let mut v = serde_json::to_value(ev(EventBody::Stop(StopPayload::default()))).unwrap();
        v["surface"] = surface.into();
        assert_eq!(s.is_valid(&v), ok, "{surface}");
    }
}

#[test]
fn event_schema_rejects_bad_kind_and_long_session() {
    let s = schema("aiq.event.v1.json");
    let mut v = serde_json::to_value(ev(EventBody::Stop(StopPayload::default()))).unwrap();
    v["kind"] = "tool".into();
    assert!(!s.is_valid(&v));
    let mut v = serde_json::to_value(ev(EventBody::Stop(StopPayload::default()))).unwrap();
    v["surface_session_id"] = "x".repeat(129).into();
    assert!(!s.is_valid(&v));
}

#[test]
fn card_schema_accepts_card_and_rejects_multiline_noticed() {
    let s = schema("aiq.card.v1.json");
    let mut c = CardV1 {
        schema: card::SCHEMA.into(),
        card_id: "c1".into(),
        opportunity_id: "reprompt".into(),
        tip_id: "t".into(),
        noticed: "n".into(),
        why: "w".into(),
        try_this: "t".into(),
        evidence_ref: "aiq show c1".into(),
        pack_version: "1".into(),
    };
    assert!(s.is_valid(&serde_json::to_value(&c).unwrap()));
    c.noticed = "a\nb".into();
    assert!(!s.is_valid(&serde_json::to_value(&c).unwrap()));
}
