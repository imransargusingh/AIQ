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
        let meta =
            serde_json::json!({ "hook_event": "UserPromptSubmit", "has_text": true, "folder": folder }).to_string();
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

    /// A current judgment (schema v5) for `task_id`, and its two judge calls at `at`.
    #[allow(clippy::too_many_arguments)]
    pub fn judgment(
        &mut self,
        task_id: &str,
        origin: &str,
        provider: &str,
        at: i64,
        (job, current, potential, state, coachable): (&str, u8, u8, &str, bool),
        evidence: &[&str],
        gap: &str,
    ) {
        let id = self.id();
        for run in 1..=2u8 {
            self.store
                .insert_judge_call(&aiq_store::NewJudgeCall {
                    call_id: &format!("{id}-{run}"),
                    task_id,
                    run,
                    provider,
                    model: Some("claude-sonnet-5-5"),
                    started_utc_ms: at,
                    latency_ms: 9_000,
                    outcome: "ok",
                    code: None,
                    input_chars: 6_000,
                    tool_session_id: None,
                    cost_usd: (provider == "claude-code").then_some(0.02),
                    tokens_in: None,
                    tokens_out: None,
                })
                .unwrap();
        }
        self.store
            .insert_judgment(&aiq_store::JudgmentRow {
                judgment_id: id,
                task_id: task_id.into(),
                origin: origin.into(),
                provider: provider.into(),
                model: Some("claude-sonnet-5-5".into()),
                judged_utc_ms: at,
                job: job.into(),
                job_confidence: 0.9,
                model_rung: current,
                current_rung: current,
                current_confidence: 0.85,
                potential_rung: potential,
                potential_confidence: 0.85,
                ceiling_rung: 5,
                state: state.into(),
                coachable,
                facts: "{}".into(),
                evidence: serde_json::json!(evidence).to_string(),
                gap: gap.into(),
                blockers: "[]".into(),
                placement: "[]".into(),
                read: serde_json::json!({"prompts": 3, "chars": 160, "actions": true}).to_string(),
                call_ids: "[]".into(),
                fact_set_version: "fs1".into(),
                rubric_version: "r1".into(),
                judge_version: "j1".into(),
                task_type: None,
                task_type_confidence: None,
            })
            .unwrap();
    }

    pub fn set_aside(&mut self, folder: &str, at: i64) {
        self.store.insert_folder_rule(folder, "set_aside", "burst", "live", at).unwrap();
    }

    pub fn setting(&mut self, key: &str, value: &str, at: i64) {
        self.store.set_setting(key, value, at).unwrap();
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
/// Imported sessions use the schema v4 tables (`aiq import`, PR #15).
pub fn standard(paths: &Paths, with_import: bool) -> String {
    let mut s = Seed::new(paths);
    let now = NOW;
    // Claude Code, one task of three prompts two days ago, in the aiq repo. A tip fired on it.
    let a =
        s.prompt("claude-code", "cc1", now - 2 * DAY, Some(AIQ), "refactor the login handler to return typed errors");
    let b = s.prompt(
        "claude-code",
        "cc1",
        now - 2 * DAY + 4 * MIN,
        Some(AIQ),
        "refactor login handler so it returns typed errors",
    );
    let c = s.prompt(
        "claude-code",
        "cc1",
        now - 2 * DAY + 9 * MIN,
        Some(AIQ),
        "please refactor the login handler, typed errors returned",
    );
    s.stop("claude-code", "cc1", now - 2 * DAY + 12 * MIN);
    s.tip("claude-code", "cc1", now - 2 * DAY + 12 * MIN, vec![a.clone(), b, c], true);
    // Judged Directed, next step Delegated (coachable). Model text with markup must render as text (TR-5).
    s.judgment(
        &a,
        "live",
        "claude-code",
        now - 3 * 60 * MIN,
        ("code_change", 2, 3, "judged", true),
        &[
            "You gave the agent the goal and the error type to use.",
            "You checked the diff before keeping it <b>bold</b>.",
        ],
        "Hand the agent the whole change with the tests it must pass, and let it run them.",
    );
    // Copilot CLI, two tasks in one session nine days ago (a 45 minute gap splits them). The first is judged
    // Uncertain (job unclear).
    let sp = s.prompt("copilot-cli", "cp1", now - 9 * DAY, Some(FORESIGHT), "write a spec for the export button");
    s.judgment(
        &sp,
        "live",
        "copilot-cli",
        now - 2 * 60 * MIN,
        ("unclassified", 1, 2, "uncertain", false),
        &[
            "You asked for a spec in one line.",
            "Not Directed: AIQ couldn't tell whether the output was checked before use.",
        ],
        "Give the agent the audience, goal and constraints.",
    );
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
        s.imported_session(
            "copilot-cli",
            "imp1",
            r"C:\Users\Imran\source\repos\Foresight",
            "imported",
            &[(now - 25 * DAY, "plan the migration"), (now - 25 * DAY + 5 * MIN, "go")],
        );
        s.judgment(
            "import:copilot-cli:imp1:0",
            "imported",
            "copilot-cli",
            now - 60 * MIN,
            ("write_doc", 3, 3, "right_rung", false),
            &["You handed the agent the whole plan with what done looks like."],
            "",
        );
        s.imported_session(
            "copilot-cli",
            "imp2",
            r"C:\x\Executive_Presence",
            "automated",
            &[(now - 25 * DAY, "collect data")],
        );
    }
    String::new()
}
