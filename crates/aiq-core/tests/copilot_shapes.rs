//! Copilot's session log read through AIQ's own readers, using records shaped like real ones.
//!
//! Every other Copilot log test builds its records by hand from what we believe Copilot writes. That is how
//! rule RC passed its tests while dropping every desktop-app Claude Code prompt (11-03 to 11-08): no test
//! input had the shape of a real record. These fixtures are six real Copilot CLI / VS Code agent sessions from
//! the maintainer's laptop (2026-11-09, Copilot CLI 1.0.x), reduced to their shape: key names, JSON types,
//! numbers, timestamps and a short allowlist of values (event types, `delivery`, built-in tool names) survive;
//! every other string is `<text>` and every id a stable fake. No prompt, command, path, tool output or tool
//! name outside the allowlist is in them. `copilot-6` has a sub-agent; `copilot-2` and `copilot-6` have
//! messages sent while the agent worked (`steering`); `copilot-5` has a `user.message` with no `delivery`.
//!
//! Measured on the same laptop (354 recent sessions): sub-agent records still carry a top-level `agentId`
//! (36,805 of 36,805 sub-agent tool starts; their data also has `parentToolCallId`), which is what
//! `copilot_record` and `typed_message` read. Refresh the fixtures with the scripts in the PR that added them
//! when Copilot's log changes; the expected counts below are read from the fixtures, never from the code.
use aiq_core::agent_actions::{Actions, copilot_record};
use aiq_core::copilot_log::{steering_since, typed_message};
use serde_json::Value;
use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/copilot-shapes")
}

fn rows(name: &str) -> Vec<Value> {
    let text = std::fs::read_to_string(dir().join(name)).unwrap();
    text.lines().map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{name}: {e}: {l}"))).collect()
}

/// (file, main-line typed messages, of them steering, main-line tool calls), counted from the fixtures.
const EXPECTED: [(&str, usize, usize, u32); 6] = [
    ("copilot-1.jsonl", 4, 0, 10),
    ("copilot-2.jsonl", 5, 1, 10),
    ("copilot-3.jsonl", 1, 0, 0),
    ("copilot-4.jsonl", 1, 0, 3),
    ("copilot-5.jsonl", 2, 0, 12),
    ("copilot-6.jsonl", 4, 2, 5),
];

#[test]
fn real_shaped_logs_give_the_typed_messages_and_tool_calls_they_hold() {
    for (name, typed, steering, tools) in EXPECTED {
        let rows = rows(name);
        let got: Vec<_> = rows.iter().filter_map(typed_message).collect();
        assert_eq!(got.len(), typed, "{name}: typed messages");
        assert_eq!(got.iter().filter(|t| t.delivery == "steering").count(), steering, "{name}: steering");
        assert!(got.iter().all(|t| t.ts_ms.is_some()), "{name}: every typed message keeps its time");
        let mut acts = Actions::default();
        for r in &rows {
            copilot_record(r, &mut acts);
        }
        assert_eq!(acts.tool_calls(), tools, "{name}: main-line tool calls (a sub-agent's are its own work)");
    }
}

#[test]
fn a_sub_agents_messages_and_tool_calls_are_never_the_persons() {
    let rows = rows("copilot-6.jsonl");
    let sub: Vec<_> = rows.iter().filter(|r| r.get("agentId").is_some_and(|a| !a.is_null())).collect();
    assert!(sub.iter().any(|r| r["type"] == "user.message"), "the fixture has a sub-agent's message");
    assert!(sub.iter().any(|r| r["type"] == "tool.execution_start"), "and a sub-agent's tool call");
    assert!(sub.iter().all(|r| typed_message(r).is_none()));
    let mut acts = Actions::default();
    for r in &sub {
        copilot_record(r, &mut acts);
    }
    assert_eq!(acts.tool_calls(), 0);
}

#[test]
fn a_message_without_delivery_is_not_taken_from_the_log() {
    // Measured: 1 of 14 main-line messages in these sessions had no `delivery` (it carries `source` instead).
    // The log reader takes only steering messages; the rest come from the hook, so this one is left alone.
    let rows = rows("copilot-5.jsonl");
    let none: Vec<_> = rows.iter().filter(|r| r["type"] == "user.message" && r["data"]["delivery"].is_null()).collect();
    assert_eq!(none.len(), 1);
    assert!(typed_message(none[0]).is_none());
}

#[test]
fn the_live_reader_takes_only_steering_messages_from_a_real_shaped_log() {
    for (name, _, steering, _) in EXPECTED {
        let (got, end) = steering_since(&dir().join(name), 0).unwrap();
        assert_eq!(got.len(), steering, "{name}");
        assert!(got.iter().all(|t| t.delivery == "steering"), "{name}");
        let len = std::fs::metadata(dir().join(name)).unwrap().len();
        assert_eq!(end, len, "{name}: the next read starts at the end");
        let (again, _) = steering_since(&dir().join(name), end).unwrap();
        assert!(again.is_empty(), "{name}: nothing is taken twice");
    }
}
