//! The one-time milestone note (Milestones design, Imran 2026-11-10 "Build as mocked"): when a kind of task
//! reaches a rung (3 tasks within 30 days), the next turn end with no tip says it once, in the place the person
//! is working: Claude Code's conversation, Copilot CLI's status line, or a VS Code notification.
use aiq_contracts::event::{EventBody, EventV1, PromptPayload, SCHEMA, StopPayload};
use aiq_core::counters::Counters;
use aiq_core::pack::BundledPack;
use aiq_core::service::{Service, TipLineOut, milestone_note_text};
use aiq_core::sysclock::format_rfc3339_ms;
use aiq_domain::PackSource;
use aiq_domain::clock::{Clock, Now};
use aiq_store::{JudgmentRow, Store};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;
const T0: i64 = 1_793_448_000_000;

#[derive(Clone)]
struct Clk(Arc<Mutex<i64>>);
impl Clock for Clk {
    fn now(&self) -> Now {
        Now { utc_ms: *self.0.lock().unwrap(), offset_min: 0 }
    }
}

struct H {
    _d: TempDir,
    path: std::path::PathBuf,
    clock: Clk,
    svc: Service<Clk>,
    n: u64,
}

impl H {
    fn new() -> Self {
        let d = TempDir::new().unwrap();
        let path = d.path().join("aiq.db");
        let clock = Clk(Arc::new(Mutex::new(T0)));
        let svc = Service::new(
            Store::open(&path).unwrap(),
            BundledPack.load().unwrap(),
            clock.clone(),
            Arc::new(Counters::default()),
        )
        .unwrap();
        let mut h = Self { _d: d, path, clock, svc, n: 0 };
        h.svc.set("next_rung_tips", "off").unwrap(); // only the note can show
        h
    }
    fn advance(&self, ms: i64) {
        *self.clock.0.lock().unwrap() += ms;
    }
    fn now(&self) -> i64 {
        *self.clock.0.lock().unwrap()
    }
    fn ev(&mut self, surface: &str, session: &str, body: EventBody) -> EventV1 {
        self.n += 1;
        EventV1 {
            schema: SCHEMA.into(),
            event_id: format!("{:013}-{:06}", self.now(), self.n),
            surface: surface.into(),
            surface_session_id: session.into(),
            seq: None,
            ts: format_rfc3339_ms(self.now()),
            collector_version: "test".into(),
            cwd: None,
            projects: false,
            transcript: None,
            body,
        }
    }
    fn prompt(&mut self, surface: &str, session: &str, text: &str) -> String {
        self.advance(MIN);
        let body = EventBody::Prompt(PromptPayload {
            text: Some(text.into()),
            chars: text.len() as u64,
            prompt_id: Some(format!("p{}", self.n + 1)),
            truncated: false,
        });
        let e = self.ev(surface, session, body);
        self.svc.ingest(&e, None);
        e.event_id
    }
    fn stop_event(&mut self, surface: &str, session: &str) -> EventV1 {
        self.advance(MIN);
        self.ev(surface, session, EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None }))
    }
    /// Copilot CLI: agentStop is an ingest; the status line then shows what it has.
    fn cp_turn_end(&mut self, session: &str) -> Option<TipLineOut> {
        let e = self.stop_event("copilot-cli", session);
        self.svc.ingest(&e, None);
        self.svc.tip_line("copilot-cli", Some(session))
    }
    /// Claude Code: the turn-end hook asks for a card and, with none, the note.
    fn cc_turn_end(&mut self, session: &str) -> Option<String> {
        self.prompt("claude-code", session, "keep going");
        let e = self.stop_event("claude-code", session);
        assert!(self.svc.take_card(&e, None, true).is_none(), "no tip in these tests");
        self.svc.stop_note(&e).map(|n| n.text)
    }
    /// Three Delegated "Writing tests" tasks a few hours apart: the kind reaches Delegated with the third.
    fn reach_delegated(&mut self) {
        let s = Store::open(&self.path).unwrap();
        for i in 0..3 {
            let id = self.prompt("copilot-cli", &format!("old{i}"), "write unit tests for the parser module");
            s.insert_judgment(&judgment(&id)).unwrap();
            self.advance(2 * HOUR);
        }
    }
}

fn judgment(task_id: &str) -> JudgmentRow {
    JudgmentRow {
        judgment_id: format!("j-{task_id}"),
        task_id: task_id.into(),
        origin: "live".into(),
        provider: "copilot-cli".into(),
        model: None,
        judged_utc_ms: 1,
        job: "code_change".into(),
        job_confidence: 0.9,
        model_rung: 3,
        current_rung: 3,
        current_confidence: 0.85,
        potential_rung: 3,
        potential_confidence: 0.85,
        ceiling_rung: 5,
        state: "right_rung".into(),
        coachable: false,
        facts: "{}".into(),
        evidence: r#"["The agent wrote the tests."]"#.into(),
        gap: String::new(),
        blockers: "[]".into(),
        placement: "[]".into(),
        read: r#"{"prompts":1,"reprompts":0,"corrections":0}"#.into(),
        call_ids: "[]".into(),
        fact_set_version: "fs1".into(),
        rubric_version: "r1".into(),
        judge_version: "j2".into(),
        task_type: Some("tests".into()),
        task_type_confidence: Some(0.9),
    }
}

const SAID: &str = "AIQ milestone: Writing tests reached Delegated · see it in AIQ Home";

#[test]
fn claude_code_says_a_new_milestone_once_at_a_turn_end_with_no_tip() {
    assert_eq!(milestone_note_text("Writing tests reached Delegated"), SAID);
    let mut h = H::new();
    assert_eq!(h.cc_turn_end("a"), None, "nothing earned yet");
    h.reach_delegated();
    assert_eq!(h.cc_turn_end("a").as_deref(), Some(SAID), "Delegated beats Directed, earned on the same task");
    assert_eq!(h.cc_turn_end("b"), None, "said once: another session never repeats it");
    assert_eq!(h.cc_turn_end("a"), None, "nor the same session's next turn");
    let e = h.stop_event("copilot-cli", "cp");
    assert!(h.svc.stop_note(&e).is_none(), "Copilot CLI's hook never takes it: its status line does");
}

#[test]
fn copilot_status_line_holds_the_note_until_the_next_prompt_then_the_trend_returns() {
    let mut h = H::new();
    h.reach_delegated();
    h.prompt("copilot-cli", "cp", "write unit tests for the lexer");
    assert_eq!(h.cp_turn_end("cp"), Some(TipLineOut::Milestone(SAID.into())));
    assert_eq!(h.svc.tip_line("copilot-cli", Some("cp")), Some(TipLineOut::Milestone(SAID.into())), "held");
    h.prompt("copilot-cli", "cp", "now the error paths");
    assert!(!matches!(h.svc.tip_line("copilot-cli", Some("cp")), Some(TipLineOut::Milestone(_))), "mid-turn");
    assert!(!matches!(h.cp_turn_end("cp"), Some(TipLineOut::Milestone(_))), "gone after the next prompt");
    assert_eq!(h.cc_turn_end("cc"), None, "said once, on one surface");
}

#[test]
fn a_note_the_hook_never_printed_is_said_at_the_next_turn_end() {
    let mut h = H::new();
    h.reach_delegated();
    h.prompt("claude-code", "a", "go");
    let e = h.stop_event("claude-code", "a");
    let n = h.svc.stop_note(&e).expect("claimed");
    h.svc.unclaim_note(&n); // the hook's wait ran out before the reply
    assert_eq!(h.cc_turn_end("b").as_deref(), Some(SAID));
}

#[test]
fn a_milestone_older_than_a_week_stays_on_the_page_only() {
    let mut h = H::new();
    h.reach_delegated();
    h.advance(8 * DAY);
    assert_eq!(h.cc_turn_end("a"), None);
    h.prompt("vscode", "v", "write tests");
    let e = h.stop_event("vscode", "v");
    h.svc.ingest(&e, None);
    assert!(h.svc.vscode_note().is_none());
}

#[test]
fn vscode_says_it_once_its_turn_ended_and_paused_says_nothing() {
    let mut h = H::new();
    h.reach_delegated();
    h.prompt("vscode", "v", "write tests for the lexer");
    assert!(h.svc.vscode_note().is_none(), "the agent is still working");
    let e = h.stop_event("vscode", "v");
    h.svc.ingest(&e, None);
    h.svc.set("paused", "true").unwrap();
    assert!(h.svc.vscode_note().is_none(), "paused");
    h.svc.set("paused", "false").unwrap();
    assert_eq!(h.svc.vscode_note().map(|n| n.text).as_deref(), Some(SAID));
    assert_eq!(h.cc_turn_end("cc"), None, "said once, on one surface");
}

#[test]
fn every_milestone_note_fits_the_copilot_status_line() {
    for (_, _, label) in aiq_contracts::judge::TASK_TYPES {
        let n = milestone_note_text(&format!("{label} moved up to Orchestrated"));
        assert!(n.chars().count() <= aiq_contracts::card::STATUS_LINE_TOOL_CAP, "{n}");
    }
}
