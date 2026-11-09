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

    /// The turn ended by `stop_id` had these actions recorded live (schema v9 `turn_action`); `tools` None writes
    /// a not-recorded row.
    pub fn turn_action(&mut self, stop_id: &str, surface: &str, session: &str, at: i64, tools: Option<&[(&str, i64)]>) {
        let row = aiq_store::TurnActionRow {
            stop_event_id: stop_id.into(),
            surface: surface.into(),
            surface_session_id: session.into(),
            ts_utc_ms: at,
            outcome: if tools.is_some() { "recorded" } else { "not_recorded" }.into(),
            why: tools.is_none().then(|| "read_error".into()),
            tool_calls: tools.map(|t| t.iter().map(|(_, n)| n).sum()),
            commands: tools.map(|_| 0),
            files: tools.map(|_| 0),
            subagents: tools.map(|_| 0),
            tools: tools.map(|t| {
                serde_json::Value::Object(t.iter().map(|(k, n)| (k.to_string(), (*n).into())).collect()).to_string()
            }),
            bytes_read: 100,
            read_ms: 1,
            utc_ms: at,
        };
        self.store.insert_turn_action(&row).unwrap();
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
            "noticed": "This request went round 3 times. The agent may need one new detail.",
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

    /// A next-step tip for pack move `move_id` (tip id `next_rung.<move>`), delivered at `at`.
    #[allow(clippy::too_many_arguments)]
    pub fn next_tip(
        &mut self,
        surface: &str,
        session: &str,
        at: i64,
        evidence: Vec<String>,
        move_id: &str,
        try_this: &str,
        opened: bool,
    ) -> String {
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
                    detector_id: "next_rung",
                    detector_version: 1,
                    pack_version: "bundled-4",
                    opportunity_id: "next_rung",
                    reprompt_count: 0,
                    score: 1.0,
                    evidence: &evidence,
                },
                at,
            )
            .unwrap();
        let card = serde_json::json!({
            "schema": "aiq.card.v1", "card_id": card_id, "opportunity_id": "next_rung", "tip_id": format!("next_rung.{move_id}"),
            "noticed": "This looks like a kind of task you repeat.",
            "why": "One step would let the agent take more of it.",
            "try_this": try_this,
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
    for i in 0..12 {
        s.store
            .insert_set_aside_session("claude-code", &format!("pa{i}"), PIPELINE, "burst", now - 3 * DAY + i * 90 * MIN)
            .unwrap();
    }
    // AIQ's own probe folders (two `run0`s in different probes): Home folds them into one line.
    for (i, f) in [
        r"c:\users\imran\appdata\local\temp\aiq-copilot-probe-3glkf9fj\run0",
        r"c:\users\imran\appdata\local\temp\aiq-copilot-probe-8qz2mk1d\run0",
    ]
    .iter()
    .enumerate()
    {
        s.store.insert_folder_rule(f, "set_aside", "aiq-check", "live", now - 4 * DAY + i as i64 * MIN).unwrap();
        s.store
            .insert_set_aside_session(
                "copilot-cli",
                &format!("probe{i}"),
                f,
                "aiq-check",
                now - 4 * DAY + i as i64 * MIN,
            )
            .unwrap();
    }
    // A tip delivered today, not opened.
    let e = s.prompt("copilot-cli", "cp2", now - 30 * MIN, Some(FORESIGHT), "draft the release notes");
    s.tip("copilot-cli", "cp2", now - 20 * MIN, vec![e], false);
    s.setting("daily_cap", "3", now - 5 * DAY);
    tip_paths(&mut s, now);
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

/// The tip funnel's week (design tip-funnel): yesterday six tasks and no tip (mostly an unrecognised kind), two days
/// ago the tip on cc1, today the tip on cp2 and one task still waiting for its kind label.
pub fn tip_paths(s: &mut Seed, now: i64) {
    use aiq_domain::clock::Now;
    let at = |t: i64| Now { utc_ms: t, offset_min: 0 };
    let step = |task: &str, step: &str, t: i64| s.store.insert_tip_step(task, "copilot-cli", step, at(t)).unwrap();
    for (i, st) in ["no_match", "no_match", "no_match", "label_held", "label_held", "kind_too_few"].iter().enumerate() {
        step(&format!("y{i}"), st, now - DAY + i as i64 * 40 * MIN);
    }
    step("d2a", "decided", now - 2 * DAY);
    step("d2b", "no_match", now - 2 * DAY + 30 * MIN);
    step("d2c", "kind_right_rung", now - 2 * DAY + 60 * MIN);
    step("t0", "decided", now - 30 * MIN);
    step("t1", "label_asked", now - 10 * MIN);
}

/// A v6 judgment with a kind of task, for the coaching read-outs. Code tasks hand over the whole change
/// (F13 yes) and lack a written brief (F7 no); doc tasks do not hand over yet (F13 no).
pub fn coach_row(
    task_id: &str,
    job: &str,
    task_type: &str,
    current: u8,
    state: &str,
    corrections: u32,
) -> aiq_store::JudgmentRow {
    let f13 = job == "code_change";
    let facts = format!(
        r#"{{"F1":{{"value":true,"confidence":1.0,"source":"log","disputed":false}},
      "F4":{{"value":true,"confidence":0.9,"source":"model","disputed":false}},
      "F6":{{"value":true,"confidence":0.9,"source":"model","disputed":false}},
      "F7":{{"value":false,"confidence":0.9,"source":"model","disputed":false}},
      "F8":{{"value":false,"confidence":1.0,"source":"log","disputed":false}},
      "F12":{{"value":false,"confidence":0.9,"source":"model","disputed":false}},
      "F13":{{"value":{f13},"confidence":0.9,"source":"model","disputed":false}},
      "F3":{{"value":"person","source":"log"}}}}"#
    );
    aiq_store::JudgmentRow {
        judgment_id: format!("j-{task_id}"),
        task_id: task_id.into(),
        origin: "live".into(),
        provider: "copilot-cli".into(),
        model: None,
        judged_utc_ms: NOW - MIN,
        job: job.into(),
        job_confidence: 0.9,
        model_rung: current,
        current_rung: current,
        current_confidence: 0.85,
        potential_rung: if state == "judged" { current + 1 } else { current },
        potential_confidence: 0.85,
        ceiling_rung: 5,
        state: state.into(),
        coachable: state == "judged",
        facts,
        evidence: r#"["The agent wrote the tests."]"#.into(),
        gap: "Give the agent <b>a brief</b>.".into(),
        blockers: "[]".into(),
        placement: "[]".into(),
        read: format!(r#"{{"prompts":2,"reprompts":1,"corrections":{corrections}}}"#),
        call_ids: "[]".into(),
        fact_set_version: "fs1".into(),
        rubric_version: "r1".into(),
        judge_version: "j2".into(),
        task_type: Some(task_type.into()),
        task_type_confidence: Some(0.9),
    }
}

/// The coaching fixture: sixteen typed tasks over three weeks, twelve in the aiq folder and the four specs in
/// foresight. Writing tests and Writing a spec each have
/// a next step, fixing bugs is at the right rung, refactoring has too few tasks, one task is Uncertain.
/// One tip is turned off, so Settings lists it.
pub fn coaching(paths: &Paths) {
    coaching_from(paths, 0);
}

/// `coaching` with its ids starting after `first`, so it can share a store with another seed.
fn coaching_from(paths: &Paths, first: u32) {
    let mut s = Seed::new(paths);
    s.n = first;
    let kinds: [(&str, &str, u8, &str, u32, &str); 16] = [
        ("code_change", "tests", 3, "judged", 0, "write unit tests for the parser"),
        ("code_change", "tests", 3, "judged", 0, "add tests for the date helpers"),
        ("code_change", "tests", 3, "judged", 0, "cover the import path with tests"),
        ("code_change", "tests", 3, "judged", 0, "write tests for the settings page"),
        ("code_change", "tests", 3, "judged", 1, "tests for the hook timeout"),
        ("code_change", "tests", 3, "judged", 0, "add unit tests for the store"),
        ("code_change", "tests", 3, "uncertain", 0, "test this"),
        ("write_doc", "spec", 2, "judged", 0, "draft the spec for the export page"),
        ("write_doc", "spec", 2, "judged", 1, "write the PRD for sharing"),
        ("write_doc", "spec", 2, "judged", 0, "spec the onboarding flow"),
        ("write_doc", "spec", 2, "judged", 0, "write a spec for tip history"),
        ("code_change", "fix_bug", 1, "right_rung", 0, "why does login fail on Safari"),
        ("code_change", "fix_bug", 1, "right_rung", 0, "the parser test fails, fix it"),
        ("code_change", "fix_bug", 1, "right_rung", 0, "fix the crash on empty input"),
        ("code_change", "refactor", 2, "judged", 0, "clean up the CLI module"),
        ("code_change", "refactor", 2, "judged", 0, "split the service file"),
    ];
    for (i, (job, ty, cur, state, corr, text)) in kinds.iter().enumerate() {
        let at = NOW - (i as i64 + 1) * 31 * 60 * MIN; // about 1.3 days apart, oldest about three weeks ago
        // The specs ran in another folder, so the projects differ (design project-filter).
        let folder = if *job == "write_doc" { FORESIGHT } else { AIQ };
        let id = s.prompt("copilot-cli", &format!("k{i}"), at, Some(folder), text);
        s.stop("copilot-cli", &format!("k{i}"), at + 3 * MIN);
        s.store.insert_judgment(&coach_row(&id, job, ty, *cur, state, *corr)).unwrap();
    }
    s.store
        .insert_tip_feedback(&aiq_store::TipFeedbackRow {
            card_id: None,
            tip_id: "next_rung.agent_checks".into(),
            answer: "off".into(),
            surface: "cli".into(),
            utc_ms: NOW - DAY,
        })
        .unwrap();
    // Next-step tips (UI review item 6): one before the newest code tasks, whose checks the person then ran (3
    // of 3), and one before the specs, whose whole-draft habit didn't follow (0 of 3).
    let ev = vec!["k0".to_string()];
    s.next_tip(
        "copilot-cli",
        "k0",
        NOW - 7 * 31 * 60 * MIN,
        ev.clone(),
        "check_code",
        "Check the diff or run the tests before using the result.",
        true,
    );
    s.next_tip(
        "copilot-cli",
        "k7",
        NOW - 12 * 31 * 60 * MIN,
        ev,
        "hand_over_doc",
        "Ask for the whole draft with what done means.",
        false,
    );
}

/// The tour's example data (design home-tour T2): every Home view filled at once. The progress seed (rung mix,
/// ladder map, kinds), the coaching seed (habits, tip history) and a task open right now, in one store.
pub fn tour(paths: &Paths) {
    progress(paths);
    coaching_from(paths, 5_000);
    // A morning of finished work before the open task, so the Day view has a full day: three kinds, three rungs,
    // and a quiet gap of over 30 minutes that shrinks.
    let mut s = Seed::new(paths);
    s.n = 20_000;
    let morning: [(&str, &str, u8, &str, i64, &str); 4] = [
        ("code_change", "fix_bug", 1, "right_rung", 200, "the checkout total is off by one cent, fix it"),
        ("code_change", "tests", 3, "judged", 160, "write tests for the cart discounts"),
        ("code_change", "tests", 3, "judged", 120, "cover the refund path with tests"),
        ("write_doc", "spec", 2, "judged", 50, "draft the spec for saved carts"),
    ];
    for (i, (job, ty, cur, state, ago, text)) in morning.iter().enumerate() {
        let at = NOW - ago * MIN;
        let se = format!("t{i}");
        let id = s.prompt("copilot-cli", &se, at, Some(AIQ), text);
        s.prompt("copilot-cli", &se, at + 6 * MIN, Some(AIQ), "keep going");
        s.stop("copilot-cli", &se, at + 14 * MIN);
        let mut j = coach_row(&id, job, ty, *cur, state, 0);
        // This morning's tests came with a written brief, so the habit the progress seed grows (0 of 5, then 5 of
        // 5) still shows as growing once the coaching tasks join the last 30 days: the tour's habit line and chip.
        if *ty == "tests" {
            j.facts = j.facts.replace(r#""F7":{"value":false"#, r#""F7":{"value":true"#);
        }
        s.store.insert_judgment(&j).unwrap();
    }
    // Two more briefed test tasks earlier this week, so the growth is beyond chance (9 of 15 against 0 of 5).
    for (i, d) in [2, 4].iter().enumerate() {
        let at = NOW - d * DAY;
        let se = format!("wb{i}");
        let id =
            s.prompt("copilot-cli", &se, at, Some(AIQ), "add tests for the coupon rules; the brief is in AGENTS.md");
        s.stop("copilot-cli", &se, at + 9 * MIN);
        let mut j = coach_row(&id, "code_change", "tests", 3, "judged", 0);
        j.facts = j.facts.replace(r#""F7":{"value":false"#, r#""F7":{"value":true"#);
        s.store.insert_judgment(&j).unwrap();
    }
    // A folder a script drives (design DC-043): the tour's "Automated, not counted" step has a row to point at.
    s.prompt("claude-code", "nr1", NOW - 2 * DAY, Some(TOUR_SCRIPT), "summarise yesterday's orders");
    s.store.insert_folder_rule(TOUR_SCRIPT, "set_aside", "template", "live", NOW - 2 * DAY).unwrap();
    for i in 0..9 {
        s.store
            .insert_set_aside_session(
                "claude-code",
                &format!("nr{i}"),
                TOUR_SCRIPT,
                "template",
                NOW - 2 * DAY + i * 4 * 60 * MIN,
            )
            .unwrap();
    }
    open_task(paths);
}

/// The tour's script-driven folder, already neutral (the fixture example renames only the seed's own folders).
pub const TOUR_SCRIPT: &str = r"c:\users\you\source\repos\nightly-report";

/// Rung progress (the success measure): kinds of task judged in the last 30 days and the 30 before.
/// Writing tests moves up (Directed to Delegated) while its written-brief habit grows (0 of 5, then 5 of 5);
/// a spec moves down; fixing a bug stays; adding a feature's typical rung flips by chance only (holding);
/// refactoring is too few.
pub fn progress(paths: &Paths) {
    let mut s = Seed::new(paths);
    // (job, type, rung, state, days ago, written brief)
    let mut rows: Vec<(&str, &str, u8, &str, i64, bool)> = vec![];
    for d in [36, 38, 40, 42, 44] {
        rows.push(("code_change", "tests", 2, "judged", d, false));
        rows.push(("write_doc", "spec", 3, "judged", d + 1, false));
    }
    for d in [36, 38, 40] {
        rows.push(("code_change", "fix_bug", 1, "right_rung", d + 2, false));
    }
    for (d, r) in [(37, 1), (39, 1), (41, 3)] {
        rows.push(("code_change", "feature", r, "judged", d, false));
    }
    for d in [1, 3, 5, 7, 9] {
        rows.push(("code_change", "tests", 3, "judged", d, true));
        rows.push(("write_doc", "spec", 2, "judged", d + 1, false));
    }
    for d in [2, 4, 6] {
        rows.push(("code_change", "fix_bug", 1, "right_rung", d + 1, false));
    }
    for (d, r) in [(8, 3), (10, 3), (12, 1)] {
        rows.push(("code_change", "feature", r, "judged", d, false));
    }
    rows.push(("code_change", "tests", 4, "uncertain", 2, false));
    rows.push(("code_change", "refactor", 2, "judged", 3, false));
    rows.push(("code_change", "refactor", 2, "judged", 37, false));
    for (i, (job, ty, cur, state, days, brief)) in rows.iter().enumerate() {
        let at = NOW - days * DAY - (i as i64) * MIN;
        let id = s.prompt("copilot-cli", &format!("p{i}"), at, Some(AIQ), &format!("{ty} task number {i}"));
        s.stop("copilot-cli", &format!("p{i}"), at + 3 * MIN);
        let mut j = coach_row(&id, job, ty, *cur, state, 0);
        if *brief {
            j.facts = j.facts.replace(r#""F7":{"value":false"#, r#""F7":{"value":true"#);
        }
        s.store.insert_judgment(&j).unwrap();
    }
}

/// A task open right now (design live-rung-trend): Copilot CLI, five prompts in the last 22 minutes, labelled
/// Write a spec, the agent editing from turn 2, turn 4 not recorded, a tip delivered after turn 3, and the agent
/// still on prompt 5.
pub fn open_task(paths: &Paths) -> Seed {
    let mut s = Seed::new(paths);
    s.n = 10_000; // after the standard seed's ids
    let (now, sv, se) = (NOW, "copilot-cli", "live1");
    let first = "Write the spec for CSV export from the report page. Context: ops want one file per day; the \
                 handler is in web/report/export.ts. Keep the column order unchanged.";
    let at = |m: i64| now - 22 * MIN + m * MIN;
    let p1 = s.prompt(sv, se, at(0), Some(AIQ), first);
    s.store
        .insert_kind_label(&aiq_store::KindLabelRow {
            label_id: "lbl-live1".into(),
            task_id: p1,
            surface: sv.into(),
            surface_session_id: se.into(),
            provider: "copilot-cli".into(),
            model: None,
            outcome: "ok".into(),
            job: Some("write_doc".into()),
            job_confidence: Some(0.9),
            task_type: Some("spec".into()),
            task_type_confidence: Some(0.85),
            requested_utc_ms: at(0),
            started_utc_ms: at(0),
            latency_ms: 900,
            tokens_in: None,
            tokens_out: None,
            cost_usd: None,
            utc_ms: at(0) + 900,
        })
        .unwrap();
    let x = s.stop(sv, se, at(2));
    s.turn_action(&x, sv, se, at(2), Some(&[("view", 3), ("grep", 1)]));
    s.prompt(sv, se, at(4), Some(AIQ), "ok, draft it in docs/specs/export.md");
    let x = s.stop(sv, se, at(8));
    s.turn_action(&x, sv, se, at(8), Some(&[("view", 1), ("apply_patch", 2)]));
    s.prompt(sv, se, at(10), Some(AIQ), "add the file name rule");
    let x = s.stop(sv, se, at(12));
    s.turn_action(&x, sv, se, at(12), Some(&[("apply_patch", 1)]));
    s.tip(sv, se, at(12), vec!["3 prompts".into()], false);
    s.prompt(sv, se, at(15), Some(AIQ), "and the header row");
    let x = s.stop(sv, se, at(17));
    s.turn_action(&x, sv, se, at(17), None);
    s.prompt(sv, se, at(21), Some(AIQ), "tighten the wording");
    s
}

/// Two more tasks open beside `open_task` (several open tasks, option A): a VS Code bug fix whose agent made the
/// edits at its third turn and is working on the fourth, and a Claude Code task waiting for the person.
pub fn more_open_tasks(paths: &Paths) {
    let mut s = Seed::new(paths);
    s.n = 20_000; // after open_task's ids
    let (sv, se) = ("vscode", "vs2");
    let at = |m: i64| NOW - 40 * MIN + m * MIN;
    let p = s.prompt(sv, se, at(0), Some(AIQ), "why does the date parser drop the time zone?");
    live_label(&mut s, &p, sv, se, at(0), "code_change", "fix_bug");
    let x = s.stop(sv, se, at(3));
    s.turn_action(&x, sv, se, at(3), Some(&[("read_file", 4)]));
    s.prompt(sv, se, at(9), Some(AIQ), "show me where it reads the offset");
    let x = s.stop(sv, se, at(12));
    s.turn_action(&x, sv, se, at(12), Some(&[("read_file", 2), ("grep_search", 1)]));
    s.prompt(sv, se, at(20), Some(AIQ), "fix it in src/parse.rs and keep the API");
    let x = s.stop(sv, se, at(27));
    s.turn_action(&x, sv, se, at(27), Some(&[("read_file", 1), ("apply_patch", 2)]));
    s.prompt(sv, se, NOW - MIN / 2, Some(AIQ), "now the tests for it");
    let (sv, se) = ("claude-code", "cc3");
    let at = |m: i64| NOW - 12 * MIN + m * MIN;
    let p = s.prompt(
        sv,
        se,
        at(0),
        Some(FORESIGHT),
        "Write unit tests for the CSV export in web/report/export.ts. Cover empty days and the header row; done when \
         the suite passes.",
    );
    live_label(&mut s, &p, sv, se, at(0), "code_change", "tests");
    let x = s.stop(sv, se, at(6));
    s.turn_action(&x, sv, se, at(6), Some(&[("Read", 3), ("Write", 1), ("Bash", 1)]));
}

/// A confident first-prompt label for a live task, as the core stores it.
fn live_label(s: &mut Seed, task_id: &str, sv: &str, se: &str, at: i64, job: &str, ty: &str) {
    s.store
        .insert_kind_label(&aiq_store::KindLabelRow {
            label_id: format!("lbl-{se}"),
            task_id: task_id.into(),
            surface: sv.into(),
            surface_session_id: se.into(),
            provider: sv.into(),
            model: None,
            outcome: "ok".into(),
            job: Some(job.into()),
            job_confidence: Some(0.9),
            task_type: Some(ty.into()),
            task_type_confidence: Some(0.85),
            requested_utc_ms: at,
            started_utc_ms: at,
            latency_ms: 900,
            tokens_in: None,
            tokens_out: None,
            cost_usd: None,
            utc_ms: at + 900,
        })
        .unwrap();
}
