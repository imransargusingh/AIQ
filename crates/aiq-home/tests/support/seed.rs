//! A seeded store for the read-model tests and the UI fixture (shared through `#[path]`).
//! Times are relative to `now` so the window rules are exercised at their edges.
#![allow(dead_code)]
use aiq_core::paths::Paths;
use aiq_store::{NewDecision, NewEvent, NewSignal, Store};

pub const MIN: i64 = 60_000;
pub const DAY: i64 = 86_400_000;
/// 2026-11-01 12:00 UTC, a Sunday. The fixture's fixed clock.
pub const NOW: i64 = 1_793_534_400_000;

pub const AIQ: &str = r"c:\users\imran\source\repos\aiq";
pub const FORESIGHT: &str = r"c:\users\imran\source\repos\foresight";
pub const PIPELINE: &str = r"c:\users\imran\source\repos\pipeline";

/// PR #15's import tables, as that PR defines them (schema v4). Read defensively by aiq-home.
pub const IMPORT_DDL: &str = "
CREATE TABLE import_session (
  tool TEXT NOT NULL, source_session_id TEXT NOT NULL, source_mtime_ms INTEGER NOT NULL, run_id TEXT NOT NULL,
  folder TEXT NOT NULL, started_utc_ms INTEGER, human_turns INTEGER NOT NULL, non_interactive INTEGER,
  template_key INTEGER, outcome TEXT NOT NULL, why TEXT NOT NULL, PRIMARY KEY (tool, source_session_id));
CREATE TABLE import_event (
  tool TEXT NOT NULL, source_session_id TEXT NOT NULL, seq INTEGER NOT NULL, kind TEXT NOT NULL,
  ts_utc_ms INTEGER, text TEXT, actions TEXT, PRIMARY KEY (tool, source_session_id, seq));";

pub struct Seed {
    pub store: Store,
    n: u32,
}

impl Seed {
    pub fn new(paths: &Paths) -> Self {
        paths.ensure_dirs().unwrap();
        Self { store: Store::open(&paths.db()).unwrap(), n: 0 }
    }

    fn id(&mut self) -> String {
        self.n += 1;
        format!("0190{:08}-0000-7000-8000-000000000000", self.n)
    }

    pub fn prompt(&mut self, surface: &str, session: &str, at: i64, folder: Option<&str>, text: &str) -> String {
        let id = self.id();
        let meta = serde_json::json!({ "hook_event": "UserPromptSubmit", "has_text": true, "folder": folder }).to_string();
        self.store
            .insert_event(
                &NewEvent {
                    event_id: &id,
                    surface,
                    session,
                    kind: "prompt",
                    ts_utc_ms: at,
                    prompt_id: None,
                    trace_id: None,
                    collector_version: "test",
                    prompt_chars: Some(text.len() as i64),
                    payload_meta: &meta,
                    text: Some(text),
                },
                at,
            )
            .unwrap();
        id
    }

    pub fn stop(&mut self, surface: &str, session: &str, at: i64) -> String {
        let id = self.id();
        self.store
            .insert_event(
                &NewEvent {
                    event_id: &id,
                    surface,
                    session,
                    kind: "stop",
                    ts_utc_ms: at,
                    prompt_id: None,
                    trace_id: None,
                    collector_version: "test",
                    prompt_chars: None,
                    payload_meta: r#"{"hook_event":"Stop"}"#,
                    text: None,
                },
                at,
            )
            .unwrap();
        id
    }

    /// The person deleted this prompt's text (the content row goes; the event stays).
    pub fn delete_text(&mut self, event_id: &str) {
        self.store.raw().execute("DELETE FROM content WHERE event_id = ?1", [event_id]).unwrap();
    }

    pub fn tip(&mut self, surface: &str, session: &str, at: i64, evidence: Vec<String>, opened: bool) -> String {
        let signal_id = self.id();
        let card_id = self.id();
        let decision_id = self.id();
        self.store
            .insert_signal(
                &NewSignal {
                    signal_id: &signal_id,
                    task_id: &evidence[0],
                    surface,
                    session,
                    detector_id: "reprompt",
                    detector_version: 1,
                    pack_version: "bundled-4",
                    opportunity_id: "reprompt",
                    reprompt_count: 3,
                    score: 1.0,
                    evidence: &evidence,
                },
                at,
            )
            .unwrap();
        let card = serde_json::json!({
            "schema": "aiq.card.v1", "card_id": card_id, "opportunity_id": "reprompt", "tip_id": "reprompt.restate",
            "noticed": "You reworded the same request 3 times in this task.",
            "why": "Rewording often means the agent is missing context, not words.",
            "try_this": "Say what is wrong with the last answer and add the one fact it lacked.",
            "evidence_ref": format!("aiq show {card_id}"), "pack_version": "bundled-4"
        })
        .to_string();
        self.store
            .insert_decision(
                &NewDecision {
                    decision_id: &decision_id,
                    card_id: &card_id,
                    signal_id: &signal_id,
                    surface,
                    session,
                    outcome: "show_ready",
                    reason_code: "ok",
                    rule_id: "in_moment.v1",
                    pack_version: "bundled-4",
                    card_json: &card,
                },
                at,
            )
            .unwrap();
        self.store
            .insert_delivery(&card_id, surface, session, aiq_domain::clock::Now { utc_ms: at, offset_min: 0 })
            .unwrap();
        if opened {
            self.store.insert_open(&card_id, "cli", at + MIN).unwrap();
        }
        card_id
    }

    pub fn set_aside(&mut self, folder: &str, at: i64) {
        self.store.insert_folder_rule(folder, "set_aside", "burst", "live", at).unwrap();
    }

    pub fn setting(&mut self, key: &str, value: &str, at: i64) {
        self.store.set_setting(key, value, at).unwrap();
    }

    pub fn import_tables(&mut self) {
        self.store.raw().execute_batch(IMPORT_DDL).unwrap();
    }

    pub fn imported_session(&mut self, tool: &str, sid: &str, folder: &str, outcome: &str, prompts: &[(i64, &str)]) {
        self.store
            .raw()
            .execute(
                "INSERT INTO import_session VALUES (?1, ?2, 0, 'run1', ?3, ?4, ?5, NULL, NULL, ?6, '')",
                rusqlite::params![tool, sid, folder, prompts.first().map(|p| p.0), prompts.len() as i64, outcome],
            )
            .unwrap();
        for (i, (at, text)) in prompts.iter().enumerate() {
            self.store
                .raw()
                .execute(
                    "INSERT INTO import_event VALUES (?1, ?2, ?3, 'prompt', ?4, ?5, NULL)",
                    rusqlite::params![tool, sid, (i * 2) as i64, at, text],
                )
                .unwrap();
            self.store
                .raw()
                .execute(
                    "INSERT INTO import_event VALUES (?1, ?2, ?3, 'stop', ?4, NULL, '[]')",
                    rusqlite::params![tool, sid, (i * 2 + 1) as i64, at + MIN],
                )
                .unwrap();
        }
    }
}

/// The standard seeded store. Returns the card id of the delivered tip.
pub fn standard(paths: &Paths, with_import: bool) -> String {
    let mut s = Seed::new(paths);
    let now = NOW;
    // Claude Code, one task of three prompts two days ago, in the aiq repo. A tip fired on it.
    let a = s.prompt("claude-code", "cc1", now - 2 * DAY, Some(AIQ), "refactor the login handler to return typed errors");
    let b = s.prompt("claude-code", "cc1", now - 2 * DAY + 4 * MIN, Some(AIQ), "refactor login handler so it returns typed errors");
    let c = s.prompt("claude-code", "cc1", now - 2 * DAY + 9 * MIN, Some(AIQ), "please refactor the login handler, typed errors returned");
    s.stop("claude-code", "cc1", now - 2 * DAY + 12 * MIN);
    s.tip("claude-code", "cc1", now - 2 * DAY + 12 * MIN, vec![a, b, c], true);
    // Copilot CLI, two tasks in one session nine days ago (a 45 minute gap splits them).
    s.prompt("copilot-cli", "cp1", now - 9 * DAY, Some(FORESIGHT), "write a spec for the export button");
    s.stop("copilot-cli", "cp1", now - 9 * DAY + MIN);
    s.prompt("copilot-cli", "cp1", now - 9 * DAY + 46 * MIN, Some(FORESIGHT), "<img src=x onerror=alert(1)> add tests");
    // VS Code: each turn is its own session, but they are one task (span_surfaces).
    s.prompt("vscode", "v1", now - 16 * DAY, None, "explain this stack trace");
    s.prompt("vscode", "v2", now - 16 * DAY + 3 * MIN, None, "now fix it");
    // A task 20 days ago whose text the person deleted. Still a task; Your data says deleted.
    let d = s.prompt("claude-code", "cc2", now - 20 * DAY, Some(AIQ), "secret project notes");
    s.delete_text(&d);
    // Outside the 30-day window.
    s.prompt("claude-code", "cc0", now - 40 * DAY, Some(AIQ), "old work");
    // A pipeline folder AIQ set aside: never a task.
    s.prompt("claude-code", "p1", now - 3 * DAY, Some(PIPELINE), "summarise record 1");
    s.set_aside(PIPELINE, now - 3 * DAY);
    // A tip delivered today, not opened.
    let e = s.prompt("copilot-cli", "cp2", now - 30 * MIN, Some(FORESIGHT), "draft the release notes");
    s.tip("copilot-cli", "cp2", now - 20 * MIN, vec![e], false);
    s.setting("daily_cap", "3", now - 5 * DAY);
    if with_import {
        s.import_tables();
        s.imported_session("copilot-cli", "imp1", r"C:\Users\Imran\source\repos\Foresight", "imported", &[(now - 25 * DAY, "plan the migration"), (now - 25 * DAY + 5 * MIN, "go")]);
        s.imported_session("copilot-cli", "imp2", r"C:\x\Executive_Presence", "automated", &[(now - 25 * DAY, "collect data")]);
    }
    String::new()
}
