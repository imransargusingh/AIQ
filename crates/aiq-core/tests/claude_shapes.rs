//! Claude Code transcripts read through AIQ's own readers, using records shaped like real ones.
//!
//! Rule RC dropped 268 of 269 Claude Code prompts from 11-03 to 11-08 (DC-043, fixed in #205) while its tests
//! passed: they were built from records we wrote by hand, and none had the shape of a desktop-app session.
//! These fixtures are four real transcripts from the maintainer's laptop (2026-11-09, 829 transcripts read),
//! the smallest of each kind AIQ must tell apart, reduced to their shape by `fixtures/claude-shapes/
//! claude_shape.py`: key names, JSON types, numbers, booleans and an allowlist of values (record types,
//! turnOrigin, entrypoint, origin kind/subkind, roles, block types, built-in tool names) survive; every other
//! string is `<text>`, every id a stable fake, and timestamps start at 2026-01-01 with their gaps kept.
//! - `desktop-project-thread`: a claude.ai project thread run by the desktop app. Its head carries the
//!   desktop `turnOrigin = "system"` marker; one turn is the person's (`turnOrigin = "human"`,
//!   `projectsUserTurn`), one is a relayed brief (`turnOrigin = "peer"`, `subkind = "projects-relay"`).
//! - `desktop-sdk`: a session the desktop app ran through its SDK (`entrypoint = "claude-desktop"`,
//!   `turnOrigin = "sdk"`), with no marker, so rule RC counts it.
//! - `terminal`: a terminal session (`entrypoint = "cli"`, `turnOrigin = "human"`).
//! - `headless`: a `claude -p` run (`entrypoint = "sdk-cli"`).
//!
//! Measured on the same laptop, typed records by marker (829 transcripts): 790 `sdk`/`sdk-cli`; 72 `human` +
//! `projectsUserTurn` in marker sessions; 29 `human` in terminal sessions; 28 `peer`/`projects-relay`; 14
//! `sdk`/`claude-desktop` with no marker; 13 `task_notification`; 1 `scheduled` (`subkind =
//! "scheduled-trigger"`, `claude-desktop`, no marker). Expected counts are read from the fixtures.
use aiq_core::agent_actions::{Actions, claude_record, claude_typed};
use aiq_core::projects::{HEAD_BYTES, Head, Turn, scan, turn_at};
use serde_json::Value;
use std::path::PathBuf;

fn path(kind: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/claude-shapes/claude-{kind}.jsonl"))
}

fn rows(kind: &str) -> Vec<Value> {
    let text = std::fs::read_to_string(path(kind)).unwrap();
    text.lines().map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{kind}: {e}: {l}"))).collect()
}

fn ms(r: &Value) -> i64 {
    aiq_core::sysclock::parse_rfc3339_ms(r["timestamp"].as_str().unwrap()).unwrap()
}

#[test]
fn rule_rc_reads_the_head_of_each_real_shaped_transcript() {
    assert_eq!(scan(&path("desktop-project-thread"), HEAD_BYTES), Head::Projects);
    for kind in ["desktop-sdk", "terminal", "headless"] {
        assert_eq!(scan(&path(kind), HEAD_BYTES), Head::NotProjects, "{kind}");
    }
}

#[test]
fn in_a_project_thread_the_persons_turn_is_their_own_and_the_relayed_brief_is_not() {
    let typed: Vec<_> = rows("desktop-project-thread").into_iter().filter(claude_typed).collect();
    assert_eq!(typed.len(), 2, "one typed by the person, one relayed");
    let verdicts: Vec<_> = typed
        .iter()
        .map(|r| {
            (
                r["turnOrigin"].as_str().unwrap().to_owned(),
                turn_at(&path("desktop-project-thread"), ms(r), &[], 256 * 1024).0,
            )
        })
        .collect();
    assert_eq!(verdicts, [("human".to_owned(), Turn::Own), ("peer".to_owned(), Turn::Other)]);
}

#[test]
fn real_shaped_transcripts_give_the_typed_prompts_and_tool_calls_they_hold() {
    // (kind, typed prompts, main-line tool calls), counted from the fixtures.
    for (kind, typed, tools) in
        [("desktop-project-thread", 2, 3), ("desktop-sdk", 1, 3), ("terminal", 2, 2), ("headless", 1, 3)]
    {
        let rows = rows(kind);
        assert_eq!(rows.iter().filter(|r| claude_typed(r)).count(), typed, "{kind}: typed prompts");
        let mut acts = Actions::default();
        for r in &rows {
            claude_record(r, &mut acts);
        }
        assert_eq!(acts.tool_calls(), tools, "{kind}: tool calls");
    }
}
