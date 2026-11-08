//! Next-rung tips end to end in the core (design coaching-from-rungs, test plan "Integration (core)"):
//! a live first prompt that looks like a coachable kind of task gets one pack-written card at that turn's end,
//! under the existing budget, the person's answers, the cooldown and AIQ's own self-check.
use aiq_contracts::event::{EventBody, EventV1, PromptPayload, SCHEMA, StopPayload};
use aiq_core::counters::Counters;
use aiq_core::pack::BundledPack;
use aiq_core::service::{Service, TipLineOut};
use aiq_core::sysclock::format_rfc3339_ms;
use aiq_domain::PackSource;
use aiq_domain::clock::{Clock, Now};
use aiq_store::{CheckRow, CheckRunRow, JudgmentRow, Store, TypeGuessRow};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
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

/// A Delegated task where the person ran the checks: the type coaches the step-3 moves.
const FACTS: &str = r#"{"F1":{"value":true,"confidence":1.0,"source":"log","disputed":false},
 "F4":{"value":true,"confidence":0.9,"source":"model","disputed":false},
 "F5":{"value":true,"confidence":0.9,"source":"model","disputed":false},
 "F6":{"value":true,"confidence":0.9,"source":"model","disputed":false},
 "F7":{"value":false,"confidence":0.9,"source":"model","disputed":false},
 "F8":{"value":false,"confidence":1.0,"source":"log","disputed":false},
 "F12":{"value":false,"confidence":0.9,"source":"model","disputed":false},
 "F13":{"value":true,"confidence":0.9,"source":"model","disputed":false},
 "fit":{"value":true,"confidence":0.9,"source":"model","disputed":false},
 "F3":{"value":"person","source":"log"}}"#;

const PAST: [&str; 3] = [
    "write unit tests for the parser module covering empty input and bad tokens",
    "write unit tests for the config loader covering empty input and missing keys",
    "write unit tests for the parser errors covering empty input",
];
const NEW: &str = "write unit tests for the lexer module covering empty input";

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
        Self { _d: d, path, clock, svc, n: 0 }
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
    /// A prompt; returns its event id (a live task's id when it is the task's first prompt).
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
    /// Copilot CLI: agentStop is an ingest; the status line then delivers.
    fn cp_turn_end(&mut self, session: &str) -> Option<TipLineOut> {
        let body = EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None });
        let e = self.ev("copilot-cli", session, body);
        self.svc.ingest(&e, None);
        self.svc.tip_line("copilot-cli", Some(session))
    }
    fn judge(&self, task_id: &str, task_type: &str) {
        let s = Store::open(&self.path).unwrap();
        s.insert_judgment(&judgment(task_id, task_type)).unwrap();
    }
    /// Three judged past "Writing tests" tasks, each its own task (2 h apart), then 2 h of quiet.
    fn seed(&mut self) -> Vec<String> {
        let mut ids = vec![];
        for (i, t) in PAST.iter().enumerate() {
            let id = self.prompt("copilot-cli", &format!("old{i}"), t);
            self.judge(&id, "tests");
            ids.push(id);
            self.advance(2 * HOUR);
        }
        ids
    }
    fn decisions(&mut self) -> Vec<(String, String, u64)> {
        self.svc.status().decisions
    }
}

fn judgment(task_id: &str, task_type: &str) -> JudgmentRow {
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
        potential_rung: 4,
        potential_confidence: 0.85,
        ceiling_rung: 5,
        state: "judged".into(),
        coachable: true,
        facts: FACTS.into(),
        evidence: r#"["The agent wrote the tests."]"#.into(),
        gap: "Give it a brief.".into(),
        blockers: "[]".into(),
        placement: "[]".into(),
        read: r#"{"prompts":1,"reprompts":0,"corrections":0}"#.into(),
        call_ids: "[]".into(),
        fact_set_version: "fs1".into(),
        rubric_version: "r1".into(),
        judge_version: "j2".into(),
        task_type: Some(task_type.into()),
        task_type_confidence: Some(0.9),
    }
}

fn full(t: Option<TipLineOut>) -> Option<aiq_contracts::card::CardV1> {
    match t {
        Some(TipLineOut::Full(c)) => Some(c),
        _ => None,
    }
}

#[test]
fn copilot_first_prompt_of_a_coachable_kind_gets_one_pack_tip_at_turn_end() {
    let mut h = H::new();
    let past = h.seed();
    h.prompt("copilot-cli", "new", NEW);
    // Mid-turn: nothing shows while the agent works.
    assert!(h.svc.tip_line("copilot-cli", Some("new")).is_none());
    let card = full(h.cp_turn_end("new")).expect("next-rung card at the first turn end");
    assert_eq!(card.opportunity_id, "next_rung");
    assert_eq!(card.tip_id, "next_rung.written_brief", "pack order among the type's moves");
    assert!(card.noticed.starts_with("this looks like writing tests."), "{}", card.noticed);
    assert!(card.starter.is_some());
    // card_text_never_contains_model_text: no judge gap, no evidence, no prompt words beyond the label.
    let all = format!("{} {} {}", card.noticed, card.why, card.try_this);
    assert!(!all.contains("Give it a brief") && !all.contains("parser") && !all.contains("lexer"));
    assert!(!aiq_domain::rung::RUNG_NAMES.iter().any(|r| all.contains(r)));
    // show_evidence_has_no_prompt_text: the evidence is the judged tasks' time, tool and kind.
    let show = h.svc.show(&card.card_id, "cli");
    assert!(show.evidence.is_empty());
    assert_eq!(show.leaned_on.len(), 3);
    assert!(show.leaned_on.iter().all(|l| l.task_type == "Writing tests" && l.tool == "copilot-cli"));
    let json = serde_json::to_string(&show).unwrap();
    for p in PAST {
        assert!(!json.contains(p), "show leaks a neighbour's prompt");
    }
    // One guess row per new task that had enough judged neighbours: the third seeded task (two then) and
    // the new one.
    let guesses = Store::open(&h.path).unwrap().type_guesses().unwrap();
    assert_eq!(guesses.len(), 2);
    assert_eq!(guesses[0].task_id, past[2]);
    assert!(guesses.iter().all(|g| (g.job.as_str(), g.task_type.as_str()) == ("code_change", "tests")));
    // A second prompt in the same task never raises another next-rung card.
    h.prompt("copilot-cli", "new", "also cover unicode input");
    assert!(h.decisions().iter().filter(|(_, r, _)| r == "ok").map(|(_, _, n)| n).sum::<u64>() <= 1);
}

#[test]
fn claude_code_gets_the_card_at_stop_and_an_unlike_prompt_gets_nothing() {
    let mut h = H::new();
    h.seed();
    h.prompt("claude-code", "cc", "deploy the staging cluster with the new helm chart");
    let stop = EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None });
    let e = h.ev("claude-code", "cc", stop);
    assert!(h.svc.take_card(&e, None, true).is_none(), "no match: no guess, no card");
    h.advance(2 * HOUR);
    h.prompt("claude-code", "cc2", NEW);
    let stop = EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None });
    let e = h.ev("claude-code", "cc2", stop);
    let c = h.svc.take_card(&e, None, true).expect("card at Stop");
    assert_eq!(c.opportunity_id, "next_rung");
}

#[test]
fn off_snooze_cooldown_and_quiet_period_hold_the_tip() {
    let mut h = H::new();
    h.seed();
    h.prompt("copilot-cli", "a", NEW);
    let c = full(h.cp_turn_end("a")).unwrap();
    // Within the quiet period a new task's tip is held.
    h.advance(40 * MIN);
    h.prompt("copilot-cli", "b", NEW);
    assert!(h.decisions().iter().any(|(_, r, _)| r == "quiet_period" || r == "cooldown"));
    // After the quiet period, still inside 72 h: cooldown drops it.
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "c", NEW);
    assert!(h.decisions().iter().any(|(o, r, _)| o == "drop" && r == "cooldown"), "{:?}", h.decisions());
    // "Don't show this again" wins over the cooldown ending.
    h.svc.tip_feedback(Some(&c.card_id), None, "off", "cli").unwrap();
    h.advance(73 * HOUR);
    h.prompt("copilot-cli", "d", NEW);
    assert!(full(h.cp_turn_end("d")).is_none());
    assert!(h.decisions().iter().any(|(_, r, _)| r == "tip_off"));
    // Turned back on: shows again (cooldown passed).
    h.svc.tip_feedback(None, Some("next_rung.written_brief"), "on", "home").unwrap();
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "e", NEW);
    assert!(full(h.cp_turn_end("e")).is_some(), "{:?}", h.decisions());
    // Snooze: 7 days on every surface.
    h.svc.tip_feedback(None, Some("next_rung.written_brief"), "snooze", "vscode").unwrap();
    h.advance(73 * HOUR);
    h.prompt("copilot-cli", "f", NEW);
    assert!(full(h.cp_turn_end("f")).is_none());
    assert!(h.decisions().iter().any(|(_, r, _)| r == "snoozed"));
}

#[test]
fn flagged_type_and_setting_off_raise_no_card() {
    let mut h = H::new();
    h.seed();
    h.svc.guidance_flag("code_change", "tests", "written_brief", "flag", Some("not_useful"), None).unwrap();
    h.prompt("copilot-cli", "a", NEW);
    assert!(full(h.cp_turn_end("a")).is_none(), "flagged move is never coached");
    assert!(h.svc.guidance_flag("code_change", "tests", "nope", "flag", Some("other"), None).is_err());
    assert!(
        h.svc.guidance_flag("code_change", "spec", "brief", "flag", Some("other"), None).is_err(),
        "not a code type"
    );
    h.svc.guidance_flag("code_change", "tests", "written_brief", "undo", None, None).unwrap();
    h.svc.set("next_rung_tips", "off").unwrap();
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "b", NEW);
    assert!(full(h.cp_turn_end("b")).is_none());
    assert_eq!(Store::open(&h.path).unwrap().type_guesses().unwrap().len(), 2, "off: no guess made");
    assert!(h.svc.set("next_rung_tips", "sometimes").is_err());
    h.svc.set("next_rung_tips", "auto").unwrap();
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "c", NEW);
    assert!(full(h.cp_turn_end("c")).is_some(), "undo restores the move");
}

#[test]
fn self_check_holds_tips_at_seven_of_ten_keeps_guessing_and_on_brings_them_back() {
    let mut h = H::new();
    h.seed();
    {
        let s = Store::open(&h.path).unwrap();
        // With the third seeded task's right guess: ten checked guesses, 7 right, 3 judged "fix_bug".
        for i in 0..9 {
            let id = format!("g{i}");
            s.insert_judgment(&judgment(&id, if i < 6 { "tests" } else { "fix_bug" })).unwrap();
            s.insert_type_guess(&TypeGuessRow {
                guess_id: id.clone(),
                task_id: id,
                surface: "copilot-cli".into(),
                job: "code_change".into(),
                task_type: "tests".into(),
                similarity: 0.6,
                neighbours: vec![],
                utc_ms: 1,
                method: "words".into(),
            })
            .unwrap();
        }
    }
    let task = h.prompt("copilot-cli", "a", NEW);
    assert!(full(h.cp_turn_end("a")).is_none());
    // Held, but still guessed and stored, so the check keeps scoring and can turn tips back on (DC-036).
    let g = Store::open(&h.path).unwrap().type_guesses().unwrap();
    assert!(g.iter().any(|g| g.task_id == task && g.method == "words"), "the word guess is still made");
    let st = h.svc.status().coach.unwrap();
    assert_eq!((st.checked, st.right, st.tips_on), (10, 7, false));
    h.svc.set("next_rung_tips", "on").unwrap();
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "b", NEW);
    assert!(full(h.cp_turn_end("b")).is_some());
}

/// One accuracy-check run over ten code_change "tests" judgments; the reader agrees on `agree` of them.
fn check_run(path: &std::path::Path, run: &str, started: i64, agree: usize) {
    let s = Store::open(path).unwrap();
    let ids: Vec<String> = (0..10).map(|i| format!("{run}-c{i}")).collect();
    s.insert_check_run(&CheckRunRow {
        run_id: run.into(),
        seed: 1,
        requested: 10,
        sample: ids.iter().map(|id| format!("j-{id}")).collect(),
        started_utc_ms: started,
        check_version: "c1".into(),
    })
    .unwrap();
    for (i, id) in ids.iter().enumerate() {
        s.insert_judgment(&judgment(id, "tests")).unwrap();
        let (job, ty) = if i < agree { ("code_change", "tests") } else { ("ask_research", "explain") };
        s.insert_check(&CheckRow {
            check_id: format!("k-{id}"),
            run_id: run.into(),
            judgment_id: format!("j-{id}"),
            task_id: id.clone(),
            grader_provider: "copilot-cli".into(),
            grader_model: Some("reader".into()),
            outcome: "ok".into(),
            answer: Some(format!(
                r#"{{"job":"{job}","task_type":"{ty}","current_rung":3,"f4_goal_context_constraints":null,
                "f5_reprompts_added_information":null,"f6_output_checked":null,"f7_written_brief":null,
                "f12_step_by_step":null,"f13_whole_outcome_handed_over":null}}"#
            )),
            latency_ms: 1,
            cost_usd: None,
            checked_utc_ms: started,
        })
        .unwrap();
    }
}

/// Design K6b: the accuracy check under 8 of 10 for a job holds its tips (recorded as accuracy_paused), the
/// person can keep them showing, and a later check that clears the bar brings them back by itself.
#[test]
fn accuracy_check_pauses_a_job_keep_on_overrides_and_a_later_check_resumes() {
    let mut h = H::new();
    h.seed();
    check_run(&h.path, "r1", T0, 7);
    h.prompt("copilot-cli", "a", NEW);
    assert!(full(h.cp_turn_end("a")).is_none(), "7 of 10: paused");
    assert!(h.decisions().iter().any(|(o, r, _)| o == "drop" && r == "accuracy_paused"), "{:?}", h.decisions());
    let st = h.svc.status().coach.unwrap();
    let job = st.accuracy_paused.iter().find(|p| p.scope == "code_change").unwrap();
    assert_eq!((job.agreed, job.checked, job.kept_on, job.label.as_str()), (7, 10, false, "Code a change"));
    assert!(st.accuracy_paused.iter().any(|p| p.scope == "code_change/tests"), "the kind is under the bar too");
    assert!(st.tips_on, "the self-check is a separate rule and stays on");

    // Keep showing: both the job and the kind, for this evidence.
    assert!(h.svc.set("keep_on.run_ship", "on").is_err(), "not paused, nothing to keep on");
    assert!(h.svc.set("keep_on.code_change", "maybe").is_err());
    assert!(h.svc.set("keep_on.Bad/Scope/x", "on").is_err());
    h.svc.set("keep_on.code_change", "on").unwrap();
    h.svc.set("keep_on.code_change/tests", "on").unwrap();
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "b", NEW);
    assert!(full(h.cp_turn_end("b")).is_some(), "{:?}", h.decisions());
    assert!(h.svc.status().coach.unwrap().accuracy_paused.iter().all(|p| p.kept_on));

    // A newer check, still under the bar: new evidence, so AIQ decides again and pauses.
    check_run(&h.path, "r2", T0 + HOUR, 6);
    h.advance(80 * HOUR);
    h.prompt("copilot-cli", "c", NEW);
    assert!(full(h.cp_turn_end("c")).is_none(), "{:?}", h.decisions());
    assert!(h.svc.status().coach.unwrap().accuracy_paused.iter().any(|p| !p.kept_on));

    // A later check that clears the bar: tips come back with no action from the person.
    check_run(&h.path, "r3", T0 + 2 * HOUR, 9);
    h.advance(80 * HOUR);
    h.prompt("copilot-cli", "d", NEW);
    assert!(full(h.cp_turn_end("d")).is_some(), "{:?}", h.decisions());
    assert!(h.svc.status().coach.unwrap().accuracy_paused.is_empty());
}

/// "Always on" ignores the accuracy pause, as it ignores the self-check.
#[test]
fn always_on_ignores_the_accuracy_pause() {
    let mut h = H::new();
    h.seed();
    check_run(&h.path, "r1", T0, 5);
    h.svc.set("next_rung_tips", "on").unwrap();
    h.prompt("copilot-cli", "a", NEW);
    assert!(full(h.cp_turn_end("a")).is_some(), "{:?}", h.decisions());
}

/// Coaching-loop US-1: the first-prompt decision within 2 s p95 with 5,000 judgments (measured here, printed).
#[test]
fn guess_and_decision_fit_the_budget_with_5000_judgments() {
    let mut h = H::new();
    h.seed();
    {
        let mut s = Store::open(&h.path).unwrap();
        for i in 0..5_000 {
            let id = format!("bulk{i:05}");
            let text = format!("{} variant {i}", PAST[i % 3]);
            s.insert_event(
                &aiq_store::NewEvent {
                    event_id: &id,
                    surface: "copilot-cli",
                    session: &format!("bulk{i}"),
                    kind: "prompt",
                    ts_utc_ms: T0 + i as i64,
                    prompt_id: None,
                    trace_id: None,
                    collector_version: "t",
                    prompt_chars: Some(text.len() as i64),
                    payload_meta: "{}",
                    text: Some(&text),
                },
                T0,
            )
            .unwrap();
            s.insert_judgment(&judgment(&id, "tests")).unwrap();
        }
    }
    let mut ms = vec![];
    for i in 0..20 {
        h.advance(40 * MIN); // a new task each time; the cache stays warm (it rebuilds hourly or on change)
        let t = std::time::Instant::now();
        h.prompt("copilot-cli", &format!("p{i}"), NEW);
        ms.push(t.elapsed().as_millis() as u64);
    }
    ms.sort();
    let p95 = ms[ms.len() * 95 / 100 - 1];
    let first = ms.iter().max().copied().unwrap();
    let warm = ms[0];
    let cache_ms = h.svc.status().coach.map_or(0, |c| c.cache_ms);
    println!(
        "MEASURED first-prompt ingest + next-rung decision with 5,003 judgments: warm {warm} ms, p95 {p95} ms, max {first} ms (hourly cache rebuild {cache_ms} ms)"
    );
    assert!(p95 <= 2_000, "p95 {p95} ms");
}

// ------------------------------------------------------------------ kind labels (design kind-label)

/// A first prompt the word guess cannot name (no overlap with the seeded "Writing tests" prompts).
const UNLIKE: &str = "the session cookie expires too soon after login, can you look into it";

type Asked = Arc<Mutex<Vec<aiq_core::service::LabelRequest>>>;

/// Wire a recording labeller and turn background judging on.
fn with_labeller(h: &mut H) -> Asked {
    let asked: Asked = Arc::default();
    let a = asked.clone();
    h.svc.set_label_requester(Box::new(move |r| a.lock().unwrap().push(r)));
    h.svc.set("judge", "auto").unwrap();
    asked
}

/// What `aiq label` stores: one answered label call.
fn store_label(h: &H, task: &str, session: &str, task_type: &str, conf: f64) {
    Store::open(&h.path)
        .unwrap()
        .insert_kind_label(&aiq_store::KindLabelRow {
            label_id: format!("l-{task}-{conf}"),
            task_id: task.into(),
            surface: "copilot-cli".into(),
            surface_session_id: session.into(),
            provider: "copilot-cli".into(),
            model: Some("small".into()),
            outcome: "ok".into(),
            job: Some("code_change".into()),
            job_confidence: Some(0.95),
            task_type: Some(task_type.into()),
            task_type_confidence: Some(conf),
            requested_utc_ms: h.now(),
            started_utc_ms: h.now(),
            latency_ms: 6_000,
            tokens_in: Some(900),
            tokens_out: Some(30),
            cost_usd: None,
            utc_ms: h.now(),
        })
        .unwrap();
}

#[test]
fn no_word_match_asks_for_a_label_and_the_label_decides_the_tip_before_the_turn_ends() {
    let mut h = H::new();
    h.seed();
    let asked = with_labeller(&mut h);
    let task = h.prompt("copilot-cli", "s", UNLIKE);
    {
        let a = asked.lock().unwrap();
        assert_eq!(a.len(), 1, "one request for the new task");
        assert_eq!(
            (a[0].task_id.as_str(), a[0].surface.as_str(), a[0].session.as_str()),
            (task.as_str(), "copilot-cli", "s")
        );
    }
    assert!(Store::open(&h.path).unwrap().type_guesses().unwrap().iter().all(|g| g.task_id != task));
    // The label comes back mid-turn (a judge-sized call is ~11 s; turns run 18 s and up).
    h.advance(10_000);
    store_label(&h, &task, "s", "tests", 0.9);
    h.svc.on_kind_label(&task).unwrap();
    let card = full(h.cp_turn_end("s")).expect("a next-step tip from the label");
    assert_eq!(card.tip_id, "next_rung.written_brief");
    assert!(card.noticed.starts_with("this looks like writing tests."));
    let g: Vec<_> =
        Store::open(&h.path).unwrap().type_guesses().unwrap().into_iter().filter(|g| g.task_id == task).collect();
    assert_eq!(g.len(), 1);
    assert_eq!((g[0].method.as_str(), g[0].similarity, g[0].neighbours.len()), ("label", 0.9, 0));
    // A repeated notice decides nothing again.
    h.svc.on_kind_label(&task).unwrap();
    assert_eq!(Store::open(&h.path).unwrap().type_guesses().unwrap().iter().filter(|g| g.task_id == task).count(), 1);
    // A later prompt of the same task asks for nothing.
    h.prompt("copilot-cli", "s", "also check the remember-me cookie");
    assert_eq!(asked.lock().unwrap().len(), 1);
}

#[test]
fn a_word_match_needs_no_label_and_labels_stay_off_without_judging_or_when_turned_off() {
    let mut h = H::new();
    h.seed();
    let asked = with_labeller(&mut h);
    h.prompt("copilot-cli", "w", NEW);
    assert!(asked.lock().unwrap().is_empty(), "the word guess named it: no model call");
    h.svc.set("judge", "off").unwrap();
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "x", UNLIKE);
    h.svc.set("judge", "auto").unwrap();
    h.svc.set("kind_label", "off").unwrap();
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "y", UNLIKE);
    assert!(asked.lock().unwrap().is_empty(), "judging off, or labels off: nothing is sent");
    assert!(!h.svc.status().coach.unwrap().labels.unwrap().enabled);
    h.svc.set("kind_label", "auto").unwrap();
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "z", UNLIKE);
    assert_eq!(asked.lock().unwrap().len(), 1);
    assert!(h.svc.set("kind_label_daily", "501").is_err());
}

#[test]
fn an_unsure_label_or_one_for_a_task_the_session_left_raises_nothing() {
    let mut h = H::new();
    h.seed();
    with_labeller(&mut h);
    let t1 = h.prompt("copilot-cli", "s", UNLIKE);
    store_label(&h, &t1, "s", "tests", 0.7);
    h.svc.on_kind_label(&t1).unwrap();
    assert!(full(h.cp_turn_end("s")).is_none(), "below 0.8: no guess, no tip");
    assert!(Store::open(&h.path).unwrap().type_guesses().unwrap().iter().all(|g| g.task_id != t1));
    // A new task in another session, whose session then moves on before the label is back.
    h.advance(2 * HOUR);
    let t2 = h.prompt("copilot-cli", "m", UNLIKE);
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "m", "something else entirely now");
    store_label(&h, &t2, "m", "tests", 0.95);
    h.svc.on_kind_label(&t2).unwrap();
    assert!(Store::open(&h.path).unwrap().type_guesses().unwrap().iter().all(|g| g.task_id != t2));
}

#[test]
fn a_label_tip_is_held_by_the_accuracy_pause_like_a_word_tip() {
    let mut h = H::new();
    h.seed();
    with_labeller(&mut h);
    check_run(&h.path, "r1", T0, 7);
    let t = h.prompt("copilot-cli", "s", UNLIKE);
    store_label(&h, &t, "s", "tests", 0.9);
    h.svc.on_kind_label(&t).unwrap();
    assert!(full(h.cp_turn_end("s")).is_none());
    assert!(h.decisions().iter().any(|(o, r, _)| o == "drop" && r == "accuracy_paused"), "{:?}", h.decisions());
}

#[test]
fn labels_have_their_own_self_check_hold_tips_below_eight_in_ten_and_keep_being_checked() {
    let mut h = H::new();
    h.seed();
    let asked = with_labeller(&mut h);
    {
        let s = Store::open(&h.path).unwrap();
        for i in 0..10 {
            let id = format!("lg{i}");
            s.insert_judgment(&judgment(&id, if i < 7 { "tests" } else { "fix_bug" })).unwrap();
            s.insert_type_guess(&TypeGuessRow {
                guess_id: id.clone(),
                task_id: id,
                surface: "copilot-cli".into(),
                job: "code_change".into(),
                task_type: "tests".into(),
                similarity: 0.9,
                neighbours: vec![],
                utc_ms: 1,
                method: "label".into(),
            })
            .unwrap();
        }
    }
    let task = h.prompt("copilot-cli", "a", UNLIKE);
    // Labels 7 of 10: AIQ still asks, so the check keeps measuring and can recover (it never could before).
    assert_eq!(asked.lock().unwrap().len(), 1, "still asks for a label");
    h.advance(10_000);
    store_label(&h, &task, "a", "tests", 0.9);
    h.svc.on_kind_label(&task).unwrap();
    assert!(h.cp_turn_end("a").is_none(), "no tip from a label while the check holds");
    let g = Store::open(&h.path).unwrap().type_guesses().unwrap();
    assert!(g.iter().any(|g| g.task_id == task && g.method == "label"), "the label is stored, to be checked");
    let st = h.svc.status().coach.unwrap();
    let l = st.labels.unwrap();
    assert_eq!((l.checked, l.right), (10, 7));
    assert_eq!((st.checked, st.right), (1, 1), "word guesses keep their own count");
    assert!(st.tips_on, "word guesses still coach");
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "b", NEW);
    assert!(full(h.cp_turn_end("b")).is_some());
    // Ten newer right labels: the newest twenty are 17 of 20, so label tips come back on their own.
    {
        let s = Store::open(&h.path).unwrap();
        for i in 0..10 {
            let id = format!("lr{i}");
            s.insert_judgment(&judgment(&id, "tests")).unwrap();
            s.insert_type_guess(&TypeGuessRow {
                guess_id: id.clone(),
                task_id: id,
                surface: "copilot-cli".into(),
                job: "code_change".into(),
                task_type: "tests".into(),
                similarity: 0.9,
                neighbours: vec![],
                utc_ms: h.now(),
                method: "label".into(),
            })
            .unwrap();
        }
    }
    h.advance(73 * HOUR); // past the same-tip cooldown of "b"'s tip
    let task = h.prompt("copilot-cli", "c", UNLIKE);
    h.advance(10_000);
    store_label(&h, &task, "c", "tests", 0.9);
    h.svc.on_kind_label(&task).unwrap();
    assert!(full(h.cp_turn_end("c")).is_some(), "label tips are back once the newest checks pass");
}

// ------------------------------------------------------------------ tip funnel (design tip-funnel)

/// The latest tip-path step of a task.
fn last_step(h: &H, task: &str) -> String {
    let steps = Store::open(&h.path).unwrap().tip_steps_since(0).unwrap();
    steps.into_iter().rev().find(|s| s.task_id == task).map(|s| s.step).unwrap_or_else(|| "none".into())
}

/// One label call that failed, as `aiq label` stores it.
fn store_failed_label(h: &H, task: &str, session: &str) {
    Store::open(&h.path)
        .unwrap()
        .insert_kind_label(&aiq_store::KindLabelRow {
            label_id: format!("l-{task}-fail"),
            task_id: task.into(),
            surface: "copilot-cli".into(),
            surface_session_id: session.into(),
            provider: "copilot-cli".into(),
            model: None,
            outcome: "timeout".into(),
            job: None,
            job_confidence: None,
            task_type: None,
            task_type_confidence: None,
            requested_utc_ms: h.now(),
            started_utc_ms: h.now(),
            latency_ms: 30_000,
            tokens_in: None,
            tokens_out: None,
            cost_usd: None,
            utc_ms: h.now(),
        })
        .unwrap();
}

#[test]
fn every_new_task_ends_its_tip_path_at_a_counted_step() {
    let mut h = H::new();
    let past = h.seed();
    assert_eq!(last_step(&h, &past[0]), "no_match", "nothing judged yet, no labeller");
    let t = h.prompt("copilot-cli", "a", NEW);
    assert_eq!(last_step(&h, &t), "decided");
    h.advance(2 * HOUR);
    let t = h.prompt("copilot-cli", "b", UNLIKE);
    assert_eq!(last_step(&h, &t), "no_match");
    // Flagged kind: a known kind held on purpose, by its state.
    h.svc.guidance_flag("code_change", "tests", "written_brief", "flag", Some("not_useful"), None).unwrap();
    h.advance(2 * HOUR);
    let t = h.prompt("copilot-cli", "c", NEW);
    assert_eq!(last_step(&h, &t), "kind_flagged");
    h.svc.guidance_flag("code_change", "tests", "written_brief", "undo", None, None).unwrap();
    // Tips turned off.
    h.svc.set("next_rung_tips", "off").unwrap();
    h.advance(2 * HOUR);
    let t = h.prompt("copilot-cli", "d", NEW);
    assert_eq!(last_step(&h, &t), "tips_off");
    h.svc.set("next_rung_tips", "auto").unwrap();
    // Labels: asked, then failed, unsure, late, or decided.
    with_labeller(&mut h);
    h.advance(2 * HOUR);
    let t = h.prompt("copilot-cli", "e", UNLIKE);
    assert_eq!(last_step(&h, &t), "label_asked", "a label that never comes back stays here");
    store_failed_label(&h, &t, "e");
    h.svc.on_kind_label(&t).unwrap();
    assert_eq!(last_step(&h, &t), "label_failed");
    h.advance(2 * HOUR);
    let t = h.prompt("copilot-cli", "f", UNLIKE);
    store_label(&h, &t, "f", "tests", 0.7);
    h.svc.on_kind_label(&t).unwrap();
    assert_eq!(last_step(&h, &t), "label_unsure");
    h.advance(2 * HOUR);
    let t = h.prompt("copilot-cli", "g", UNLIKE);
    h.advance(2 * HOUR);
    h.prompt("copilot-cli", "g", "something else entirely now");
    store_label(&h, &t, "g", "tests", 0.95);
    h.svc.on_kind_label(&t).unwrap();
    assert_eq!(last_step(&h, &t), "label_late");
    h.advance(2 * HOUR);
    let t = h.prompt("copilot-cli", "h", UNLIKE);
    store_label(&h, &t, "h", "tests", 0.9);
    h.svc.on_kind_label(&t).unwrap();
    assert_eq!(last_step(&h, &t), "decided");
    // A kind none of the person's tasks has been analyzed as yet.
    h.advance(2 * HOUR);
    let t = h.prompt("copilot-cli", "i", UNLIKE);
    store_label(&h, &t, "i", "fix_bug", 0.9);
    h.svc.on_kind_label(&t).unwrap();
    assert!(last_step(&h, &t).starts_with("kind_"), "{}", last_step(&h, &t));
    // Every row's step is one the funnel knows.
    let steps = Store::open(&h.path).unwrap().tip_steps_since(0).unwrap();
    assert!(steps.iter().all(|s| aiq_domain::funnel::Step::parse(&s.step).is_some()));
}

#[test]
fn held_self_checks_are_counted_as_their_own_steps() {
    let mut h = H::new();
    h.seed();
    {
        let s = Store::open(&h.path).unwrap();
        for i in 0..10 {
            for method in ["words", "label"] {
                let id = format!("{method}{i}");
                s.insert_judgment(&judgment(&id, if i < 7 { "tests" } else { "fix_bug" })).unwrap();
                s.insert_type_guess(&TypeGuessRow {
                    guess_id: id.clone(),
                    task_id: id,
                    surface: "copilot-cli".into(),
                    job: "code_change".into(),
                    task_type: "tests".into(),
                    similarity: 0.9,
                    neighbours: vec![],
                    utc_ms: 1,
                    method: method.into(),
                })
                .unwrap();
            }
        }
    }
    // Words held and no labeller: the task ends at guess_held.
    let t = h.prompt("copilot-cli", "a", NEW);
    assert_eq!(last_step(&h, &t), "guess_held");
    // Labels held too: the label is asked for, stored, and the tip held.
    with_labeller(&mut h);
    h.advance(2 * HOUR);
    let t = h.prompt("copilot-cli", "b", UNLIKE);
    store_label(&h, &t, "b", "tests", 0.9);
    h.svc.on_kind_label(&t).unwrap();
    assert_eq!(last_step(&h, &t), "label_held");
}

#[test]
fn status_carries_the_funnel_and_a_dry_working_day_warns() {
    let mut h = H::new();
    h.seed();
    // Three tasks nothing can name, on one day; then the next day.
    for s in ["x", "y", "z"] {
        h.advance(HOUR);
        h.prompt("copilot-cli", s, UNLIKE);
    }
    let f = h.svc.status().tip_funnel.expect("funnel");
    assert!(f.today.tasks >= 3 || f.week.tasks >= 6, "{f:?}");
    assert!(f.week.no_tip.iter().any(|c| c.code == "no_match"));
    h.advance(24 * HOUR);
    let f = h.svc.status().tip_funnel.unwrap();
    assert_eq!(f.droughts.len(), 1, "{f:?}");
    assert_eq!(f.droughts[0].main_reason.as_ref().unwrap().code, "no_match");
    // A shown tip on a day clears that day.
    let mut h = H::new();
    h.seed();
    h.prompt("copilot-cli", "a", NEW);
    assert!(full(h.cp_turn_end("a")).is_some());
    h.advance(24 * HOUR);
    let f = h.svc.status().tip_funnel.unwrap();
    assert!(f.droughts.iter().all(|d| d.day != aiq_domain::funnel::iso_date(T0 / 86_400_000)), "{f:?}");
    assert_eq!(f.week.shown.iter().map(|c| c.count).sum::<u32>(), 1);
}
