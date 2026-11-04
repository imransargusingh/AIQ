//! Core service behaviour with a real store and a fake clock (D1, D2, D4, D7).
use aiq_contracts::codes;
use aiq_contracts::event::{EventBody, EventV1, PromptPayload, SCHEMA, StopPayload};
use aiq_contracts::frames::{Request, SpoolLine};
use aiq_core::counters::Counters;
use aiq_core::pack::BundledPack;
use aiq_core::server::{Cmd, Dispatch, authorize, dispatch};
use aiq_core::service::{Service, TipLineOut};
use aiq_core::sysclock::format_rfc3339_ms;
use aiq_domain::PackSource;
use aiq_domain::clock::{Clock, FakeClock, Now};
use aiq_store::Store;
use proptest::prelude::*;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Instant;
use tempfile::TempDir;

const MIN: i64 = 60_000;
const T0: i64 = 1_793_448_000_000; // 2026-10-31T12:00:00Z

const BASE: &str = "refactor the login handler to return typed errors";
const R1: &str = "refactor login handler so it returns typed errors";
const R2: &str = "login handler refactor: return typed errors please";
const R3: &str = "please refactor the login handler, typed errors returned";

/// Clock shared with the test so it can move time while the service owns it.
#[derive(Clone)]
struct SharedClock(Arc<std::sync::Mutex<Now>>);
impl SharedClock {
    fn new(ms: i64) -> Self {
        Self(Arc::new(std::sync::Mutex::new(Now { utc_ms: ms, offset_min: 0 })))
    }
    fn advance(&self, ms: i64) {
        self.0.lock().unwrap().utc_ms += ms;
    }
    fn set_offset(&self, min: i32) {
        self.0.lock().unwrap().offset_min = min;
    }
}
impl Clock for SharedClock {
    fn now(&self) -> Now {
        *self.0.lock().unwrap()
    }
}

struct H {
    _d: TempDir,
    path: std::path::PathBuf,
    clock: SharedClock,
    svc: Option<Service<SharedClock>>,
    n: u64,
}

impl H {
    fn new() -> Self {
        let d = TempDir::new().unwrap();
        let path = d.path().join("aiq.db");
        let clock = SharedClock::new(T0);
        let svc = Service::new(
            Store::open(&path).unwrap(),
            BundledPack.load().unwrap(),
            clock.clone(),
            Arc::new(Counters::default()),
        )
        .unwrap();
        Self { _d: d, path, clock, svc: Some(svc), n: 0 }
    }
    fn svc(&mut self) -> &mut Service<SharedClock> {
        self.svc.as_mut().unwrap()
    }
    fn restart(&mut self) {
        drop(self.svc.take());
        let svc = Service::new(
            Store::open(&self.path).unwrap(),
            BundledPack.load().unwrap(),
            self.clock.clone(),
            Arc::new(Counters::default()),
        )
        .unwrap();
        self.svc = Some(svc);
    }
    fn id(&mut self) -> String {
        self.n += 1;
        // UUIDv7-shaped and time-ordered: ms timestamp, then a counter.
        let ms = self.clock.now().utc_ms as u64;
        format!("{:08x}-{:04x}-7000-8000-{:012x}", ms >> 16, ms & 0xffff, self.n)
    }
    fn prompt(&mut self, session: &str, text: &str) -> EventV1 {
        self.clock.advance(MIN);
        let id = self.id();
        ev(
            &id,
            session,
            self.clock.now().utc_ms,
            EventBody::Prompt(PromptPayload {
                text: Some(text.into()),
                chars: text.len() as u64,
                prompt_id: Some(format!("p-{id}")),
                truncated: false,
            }),
        )
    }
    fn send(&mut self, session: &str, text: &str) -> String {
        let e = self.prompt(session, text);
        self.svc().ingest(&e, None);
        format!("p-{}", e.event_id)
    }
    fn stop(&mut self, session: &str, prompt_id: Option<&str>) -> Option<aiq_contracts::card::CardV1> {
        let id = self.id();
        let e = ev(
            &id,
            session,
            self.clock.now().utc_ms,
            EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: prompt_id.map(str::to_owned) }),
        );
        let c = self.svc().take_card(&e, None, true);
        if let Some(c) = &c {
            self.svc().card_shown(&c.card_id, "claude-code", session); // the hook printed it
        }
        c
    }
    fn reprompt_run(&mut self, session: &str) -> String {
        for t in [BASE, R1, R2] {
            self.send(session, t);
        }
        self.send(session, R3)
    }
    /// Copilot CLI: prompts carry no prompt id, and agentStop is a plain ingest.
    fn cp_send(&mut self, session: &str, text: &str) {
        self.clock.advance(MIN);
        let id = self.id();
        let body = EventBody::Prompt(PromptPayload {
            text: Some(text.into()),
            chars: text.len() as u64,
            prompt_id: None,
            truncated: false,
        });
        let e = ev_on("copilot-cli", &id, session, self.clock.now().utc_ms, body);
        self.svc().ingest(&e, None);
    }
    fn cp_stop(&mut self, session: &str) {
        let id = self.id();
        let body = EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None });
        let e = ev_on("copilot-cli", &id, session, self.clock.now().utc_ms, body);
        self.svc().ingest(&e, None);
    }
    /// Copilot CLI agentStop as the hook now sends it: take the card for the conversation line.
    fn cp_take(&mut self, session: &str) -> Option<aiq_contracts::card::CardV1> {
        let id = self.id();
        let body = EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None });
        let e = ev_on("copilot-cli", &id, session, self.clock.now().utc_ms, body);
        let c = self.svc().take_card(&e, None, true);
        if let Some(c) = &c {
            self.svc().card_shown(&c.card_id, "copilot-cli", session); // the hook printed it
        }
        c
    }
    fn cp_run(&mut self, session: &str) {
        for t in [BASE, R1, R2, R3] {
            self.cp_send(session, t);
        }
    }
    /// A Copilot CLI prompt from a working folder; `gap_ms` before it.
    fn cp_in(&mut self, folder: &str, session: &str, text: &str, gap_ms: i64) {
        self.clock.advance(gap_ms);
        let id = self.id();
        let body = EventBody::Prompt(PromptPayload {
            text: Some(text.into()),
            chars: text.len() as u64,
            prompt_id: None,
            truncated: false,
        });
        let mut e = ev_on("copilot-cli", &id, session, self.clock.now().utc_ms, body);
        e.cwd = Some(folder.into());
        self.svc().ingest(&e, None);
    }
    fn prompts(&mut self) -> u64 {
        self.svc().status().events_by_kind.iter().find(|(k, _)| k == "prompt").map_or(0, |(_, n)| *n)
    }
    fn tip(&mut self, session: &str) -> Option<TipLineOut> {
        self.svc().tip_line("copilot-cli", Some(session))
    }
    fn decisions(&mut self) -> Vec<(String, String, u64)> {
        self.svc().status().decisions
    }
}

fn ev(id: &str, session: &str, ms: i64, body: EventBody) -> EventV1 {
    ev_on("claude-code", id, session, ms, body)
}

fn ev_on(surface: &str, id: &str, session: &str, ms: i64, body: EventBody) -> EventV1 {
    EventV1 {
        schema: SCHEMA.into(),
        event_id: id.into(),
        surface: surface.into(),
        surface_session_id: session.into(),
        seq: None,
        ts: format_rfc3339_ms(ms),
        collector_version: "test".into(),
        cwd: None,
        projects: false,
        transcript: None,
        body,
    }
}

fn d(outcome: &str, reason: &str, n: u64) -> (String, String, u64) {
    (outcome.into(), reason.into(), n)
}

#[test]
fn reprompt_three_times_card_at_next_stop() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    let card = h.stop("s", Some(&pid)).expect("card");
    assert!(card.noticed.contains("reworded this request 3 times"));
    assert_eq!(card.evidence_ref, format!("aiq show {}", card.card_id));
    assert!(h.stop("s", Some(&pid)).is_none(), "a card is shown at most once");
}

#[test]
fn second_qualifying_run_same_session_held() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    assert!(h.stop("s", Some(&pid)).is_some());
    h.send("s", "write unit tests covering the payment retry backoff schedule");
    for _ in 0..3 {
        h.send("s", "write unit tests for payment retry backoff schedule");
    }
    assert!(h.stop("s", None).is_none());
    assert_eq!(h.decisions(), vec![d("hold", "session_budget", 1), d("shown", "ok", 1)]);
}

#[test]
fn second_ready_card_before_stop_held_one_at_a_time() {
    let mut h = H::new();
    h.reprompt_run("s");
    h.send("s", "write unit tests covering the payment retry backoff schedule");
    for _ in 0..3 {
        h.send("s", "write unit tests for payment retry backoff schedule");
    }
    assert_eq!(h.decisions(), vec![d("hold", "one_at_a_time", 1), d("show_ready", "ok", 1)]);
}

#[test]
fn restart_mid_session_budget_survives() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    assert!(h.stop("s", Some(&pid)).is_some());
    h.restart();
    h.send("s", "write unit tests covering the payment retry backoff schedule");
    for _ in 0..3 {
        h.send("s", "write unit tests for payment retry backoff schedule");
    }
    assert!(h.stop("s", None).is_none());
    assert!(h.decisions().contains(&d("hold", "session_budget", 1)));
}

#[test]
fn rebuild_equals_live_working_set() {
    let mut h = H::new();
    for t in [BASE, R1, R2] {
        h.send("a", t);
    }
    h.send("b", "explain the borrow checker error in parser module");
    h.send("b", "explain borrow checker error parser module");
    let live = h.svc().engine().clone();
    h.restart();
    assert_eq!(h.svc().engine(), &live);
    // and the rebuilt engine continues where the live one stopped: one more re-prompt fires.
    h.send("a", R3);
    assert_eq!(h.svc().status().signals, 1);
}

#[test]
fn rebuild_after_signal_does_not_fire_again() {
    let mut h = H::new();
    h.reprompt_run("s");
    h.restart();
    h.send("s", R1);
    assert_eq!(h.svc().status().signals, 1);
}

#[test]
fn signal_after_stop_delivered_at_next_stop() {
    let mut h = H::new();
    for t in [BASE, R1, R2] {
        h.send("s", t);
    }
    // The 4th prompt's async hook is late: Stop for that turn arrives first.
    let late = h.prompt("s", R3);
    let late_pid = format!("p-{}", late.event_id);
    assert!(h.stop("s", Some(&late_pid)).is_none());
    h.svc().ingest(&late, None);
    // Next turn: its prompt arrives on time, and its Stop gets the waiting card.
    let next = h.send("s", "thanks, now summarise what changed");
    assert!(h.stop("s", Some(&next)).is_some());
}

#[test]
fn ready_card_waits_for_unseen_prompt_then_delivers() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    assert!(h.stop("s", Some("p-not-yet-arrived")).is_none());
    assert!(h.stop("s", Some(&pid)).is_some());
}

// The expiry window runs from the turn's end (laptop 2026-11-03: a 37-minute Copilot turn lost its tip
// 5 minutes before it ended).
#[test]
fn a_long_turn_keeps_its_card_until_the_turn_ends() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    h.clock.advance(37 * MIN);
    assert!(h.stop("s", Some(&pid)).is_some());
}

#[test]
fn ready_card_expires_30_minutes_after_its_turn_ends() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    assert!(h.stop("s", Some("p-not-yet-arrived")).is_none()); // the turn ended; the card waits
    h.clock.advance(31 * MIN);
    assert!(h.stop("s", Some(&pid)).is_none());
    assert_eq!(h.decisions(), vec![d("drop", "expired", 1)]);
}

#[test]
fn ready_card_with_no_turn_end_expires_after_4_hours() {
    let mut h = H::new();
    h.reprompt_run("s");
    h.clock.advance(4 * 60 * MIN + MIN);
    h.svc().expire_stale().unwrap();
    assert_eq!(h.decisions(), vec![d("drop", "expired", 1)]);
}

#[test]
fn stop_hook_active_never_delivers() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    let id = h.id();
    let e = ev(
        &id,
        "s",
        h.clock.now().utc_ms,
        EventBody::Stop(StopPayload { stop_hook_active: true, prompt_id: Some(pid.clone()) }),
    );
    assert!(h.svc().take_card(&e, None, true).is_none());
    assert!(h.stop("s", Some(&pid)).is_some());
}

// ------------------------------------------------------------------ hand-over confirmed by the hook (DC-022)

fn cc_stop_event(h: &mut H, session: &str, pid: &str) -> EventV1 {
    let id = h.id();
    ev(
        &id,
        session,
        h.clock.now().utc_ms,
        EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: Some(pid.to_owned()) }),
    )
}

#[test]
fn a_taken_tip_counts_as_shown_only_once_the_hook_says_it_printed_it() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    let e = cc_stop_event(&mut h, "s", &pid);
    let c = h.svc().take_card(&e, None, true).expect("handed over");
    assert!(!h.svc().store().delivered_in_session("claude-code", "s").unwrap(), "not shown yet");
    let e = cc_stop_event(&mut h, "s", &pid);
    assert!(h.svc().take_card(&e, None, true).is_none(), "never handed to two hooks at once");
    h.svc().card_shown(&c.card_id, "claude-code", "s");
    assert!(h.svc().store().delivered_in_session("claude-code", "s").unwrap(), "shown once printed");
    h.svc().card_shown(&c.card_id, "claude-code", "s");
    assert!(h.svc().status().core_counters.iter().any(|(k, n)| k == codes::CARD_SHOWN_UNKNOWN && *n == 1));
    assert_eq!(h.svc().status().cards_today, 1, "counted once");
}

#[test]
fn a_tip_the_hook_never_confirms_is_ready_again_and_never_counted() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    let e = cc_stop_event(&mut h, "s", &pid);
    let first = h.svc().take_card(&e, None, true).expect("handed over");
    h.clock.advance(aiq_core::service::HANDOVER_CONFIRM_MS - 1);
    let e = cc_stop_event(&mut h, "s", &pid);
    assert!(h.svc().take_card(&e, None, true).is_none(), "still waiting for the first hook");
    h.clock.advance(1);
    let again = h.stop("s", Some(&pid)).expect("ready again for the next turn end");
    assert_eq!(again.card_id, first.card_id);
    let st = h.svc().status();
    assert!(st.core_counters.iter().any(|(k, n)| k == codes::CARD_UNCONFIRMED && *n == 1), "{:?}", st.core_counters);
    assert_eq!(st.cards_today, 1, "the unconfirmed hand-over never counted");
}

#[test]
fn a_tip_handed_back_is_ready_at_once_and_a_late_confirmation_still_counts() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    let e = cc_stop_event(&mut h, "s", &pid);
    let c = h.svc().take_card(&e, None, true).expect("handed over");
    h.svc().hand_back(&c.card_id);
    let e = cc_stop_event(&mut h, "s", &pid);
    let c = h.svc().take_card(&e, None, true).expect("ready at once");
    h.clock.advance(aiq_core::service::HANDOVER_CONFIRM_MS * 2);
    h.svc().card_shown(&c.card_id, "claude-code", "s");
    assert!(h.svc().store().delivered_in_session("claude-code", "s").unwrap(), "printed late: still the truth");
}

#[test]
fn a_tip_being_printed_holds_other_sessions_as_the_quiet_period_does() {
    let mut h = H::new();
    let pid = h.reprompt_run("a");
    let pid_b = h.reprompt_run("b");
    let e = cc_stop_event(&mut h, "a", &pid);
    let c = h.svc().take_card(&e, None, true).expect("handed over");
    assert!(h.svc().store().ready_card("claude-code", "b").unwrap().is_some(), "b has a tip of its own");
    let e = cc_stop_event(&mut h, "b", &pid_b);
    assert!(h.svc().take_card(&e, None, true).is_none(), "one tip at a time");
    h.svc().card_shown(&c.card_id, "claude-code", "a");
}

#[test]
fn stale_take_card_not_delivered_card_kept() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    let id = h.id();
    let e = ev(
        &id,
        "s",
        h.clock.now().utc_ms,
        EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: Some(pid.clone()) }),
    );
    assert!(h.svc().take_card(&e, None, false).is_none());
    assert!(h.stop("s", Some(&pid)).is_some());
}

#[test]
fn paused_nothing_captured() {
    let mut h = H::new();
    h.svc().set("paused", "true").unwrap();
    h.reprompt_run("s");
    let st = h.svc().status();
    assert!(st.paused);
    assert!(st.events_by_kind.is_empty());
    h.svc().set("paused", "false").unwrap();
    h.send("s", BASE);
    assert_eq!(h.svc().status().events_by_kind, vec![("prompt".to_string(), 1)]);
}

#[test]
fn daily_cap_reached_holds_then_resets_next_local_day() {
    let mut h = H::new();
    h.svc().set("daily_cap", "2").unwrap();
    for s in ["a", "b"] {
        let pid = h.reprompt_run(s);
        assert!(h.stop(s, Some(&pid)).is_some());
        h.clock.advance(30 * MIN); // past the person-wide quiet period
    }
    h.reprompt_run("c");
    assert!(h.decisions().contains(&d("hold", "daily_cap", 1)));
    h.clock.advance(24 * 60 * MIN);
    let pid = h.reprompt_run("d");
    assert!(h.stop("d", Some(&pid)).is_some());
}

#[test]
fn daily_cap_zero_never_shows() {
    let mut h = H::new();
    h.svc().set("daily_cap", "0").unwrap();
    let pid = h.reprompt_run("s");
    assert!(h.stop("s", Some(&pid)).is_none());
    assert_eq!(h.decisions(), vec![d("hold", "daily_cap", 1)]);
}

#[test]
fn timezone_move_back_does_not_reopen_used_day() {
    let mut h = H::new();
    h.svc().set("daily_cap", "1").unwrap();
    h.clock.set_offset(14 * 60); // far east: local date is "tomorrow"
    let pid = h.reprompt_run("a");
    assert!(h.stop("a", Some(&pid)).is_some());
    h.clock.set_offset(-12 * 60); // far west: local date moves back a day
    h.clock.advance(30 * MIN); // past the quiet period, so only the cap can hold it
    h.reprompt_run("b");
    assert!(h.decisions().contains(&d("hold", "daily_cap", 1)));
}

#[test]
fn set_rejects_bad_values() {
    let mut h = H::new();
    assert!(h.svc().set("daily_cap", "-1").is_err());
    assert!(h.svc().set("daily_cap", "51").is_err());
    assert!(h.svc().set("paused", "maybe").is_err());
    assert!(h.svc().set("colour", "red").is_err());
    assert_eq!(h.svc().daily_cap(), 5);
}

#[test]
fn show_returns_card_reason_and_evidence() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    let card = h.stop("s", Some(&pid)).unwrap();
    let r = h.svc().show(&card.card_id, "cli");
    assert!(r.delivered);
    assert_eq!(r.reason_code.as_deref(), Some("ok"));
    let texts: Vec<_> = r.evidence.iter().map(|e| e.text.clone().unwrap()).collect();
    assert_eq!(texts, vec![BASE, R1, R2, R3]);
    assert!(h.svc().show("nope", "cli").card.is_none());
}

#[test]
fn ingest_missing_prompt_text_counted_no_detection() {
    let mut h = H::new();
    let counters = Arc::new(Counters::default());
    let mut svc =
        Service::new(Store::open(&h.path).unwrap(), BundledPack.load().unwrap(), h.clock.clone(), counters.clone())
            .unwrap();
    let id = h.id();
    let e =
        ev(&id, "s", T0, EventBody::Prompt(PromptPayload { text: None, chars: 0, prompt_id: None, truncated: false }));
    svc.ingest(&e, None);
    assert_eq!(counters.get(codes::INGEST_SHAPE), 1);
    assert_eq!(svc.status().events_by_kind, vec![("prompt".to_string(), 1)]);
}

#[test]
fn ingest_oversized_session_id_rejected() {
    let mut h = H::new();
    let id = h.id();
    let e = ev(&id, &"x".repeat(129), T0, EventBody::Stop(StopPayload::default()));
    h.svc().ingest(&e, None);
    assert!(h.svc().status().events_by_kind.is_empty());
}

#[test]
fn fake_clock_type_from_domain_is_usable() {
    let c = FakeClock::new(T0, 0);
    c.advance_ms(MIN);
    assert_eq!(c.now().utc_ms, T0 + MIN);
}

// ---- local API dispatch (no socket) ----

#[test]
fn authorize_wrong_or_missing_token_counted_no_request() {
    let c = Counters::default();
    assert!(authorize(r#"{"op":"status","token":"bad"}"#, "good", &c).is_none());
    assert!(authorize(r#"{"op":"status"}"#, "good", &c).is_none());
    assert!(authorize("not json", "good", &c).is_none());
    assert_eq!(c.get(codes::API_AUTH), 1);
    assert_eq!(c.get(codes::INGEST_SHAPE), 2);
    assert!(authorize(r#"{"op":"status","token":"good"}"#, "good", &c).is_some());
}

#[test]
fn channel_full_drops_ingest_answers_take_card_null() {
    let (tx, _rx) = mpsc::sync_channel::<Cmd>(1);
    let c = Counters::default();
    let e = ev("01a1935c-3bba-74ae-a607-3e84a3d4c8e7", "s", T0, EventBody::Stop(StopPayload::default()));
    assert_eq!(
        dispatch(Request::Ingest { token: "t".into(), trace_id: None, event: e.clone() }, &tx, &c, Instant::now()),
        Dispatch::NoReply
    );
    assert_eq!(
        dispatch(Request::Ingest { token: "t".into(), trace_id: None, event: e.clone() }, &tx, &c, Instant::now()),
        Dispatch::NoReply
    );
    assert_eq!(c.get(codes::INGEST_BACKPRESSURE), 1);
    let t0 = Instant::now();
    let r = dispatch(
        Request::TakeCard { token: "t".into(), trace_id: None, event: e, wait_ms: 30 },
        &tx,
        &c,
        Instant::now(),
    );
    assert_eq!(r, Dispatch::Reply("{\"card\":null}\n".into()));
    assert!(t0.elapsed().as_millis() < 30, "answered immediately, not after waiting");
}

// ---- spool replay property (architecture gate condition, D2) ----

type Outcome = (Vec<(String, u64)>, u64, Vec<(String, String, u64)>);

fn replay_outcome(lines: &[String]) -> Outcome {
    let d = TempDir::new().unwrap();
    let paths = aiq_core::paths::Paths::at(d.path());
    std::fs::write(paths.spool(), lines.join("\n") + "\n").unwrap();
    let clock = SharedClock::new(T0 + 60 * MIN);
    let counters = Counters::default();
    let mut svc = Service::new(
        Store::open(&paths.db()).unwrap(),
        BundledPack.load().unwrap(),
        clock,
        Arc::new(Counters::default()),
    )
    .unwrap();
    aiq_core::server::drain_spool(&paths, &mut svc, &counters);
    let st = svc.status();
    (st.events_by_kind, st.signals, st.decisions)
}

fn clean_lines(texts: &[usize]) -> Vec<String> {
    let pool = [BASE, R1, R2, R3, "deploy staging with terraform now", "try again"];
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let ms = T0 + i as i64 * MIN;
            let id = format!("{:08x}-{:04x}-7000-8000-{:012x}", (ms as u64) >> 16, (ms as u64) & 0xffff, i);
            let text = pool[*t % pool.len()];
            let e = ev(
                &id,
                "s",
                ms,
                EventBody::Prompt(PromptPayload {
                    text: Some(text.into()),
                    chars: text.len() as u64,
                    prompt_id: None,
                    truncated: false,
                }),
            );
            serde_json::to_string(&SpoolLine { trace_id: None, event: e }).unwrap()
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, .. ProptestConfig::default() })]
    #[test]
    fn spool_replay_any_interleaving_duplication_truncation_same_result(
        texts in proptest::collection::vec(0usize..6, 1..14),
        dups in proptest::collection::vec(any::<prop::sample::Index>(), 0..10),
        cut in any::<prop::sample::Index>(),
        seed in any::<u64>(),
    ) {
        let clean = clean_lines(&texts);
        let expected = replay_outcome(&clean);
        let mut messy = clean.clone();
        for ix in &dups {
            messy.push(clean[ix.index(clean.len())].clone());
        }
        // a partial copy of a line (crash mid-append) as an extra line
        let victim = &clean[cut.index(clean.len())];
        messy.push(victim[..victim.len() / 2].to_string());
        // deterministic shuffle
        let mut s = seed | 1;
        for i in (1..messy.len()).rev() {
            s ^= s << 13; s ^= s >> 7; s ^= s << 17;
            messy.swap(i, (s % (i as u64 + 1)) as usize);
        }
        prop_assert_eq!(replay_outcome(&messy), expected);
    }
}

#[test]
fn spool_replayed_twice_one_row_one_signal() {
    let lines = clean_lines(&[0, 1, 2, 3]);
    let d = TempDir::new().unwrap();
    let paths = aiq_core::paths::Paths::at(d.path());
    let counters = Counters::default();
    let mut svc = Service::new(
        Store::open(&paths.db()).unwrap(),
        BundledPack.load().unwrap(),
        SharedClock::new(T0 + 10 * MIN),
        Arc::new(Counters::default()),
    )
    .unwrap();
    for _ in 0..2 {
        std::fs::write(paths.spool(), lines.join("\n") + "\nnot json\n").unwrap();
        aiq_core::server::drain_spool(&paths, &mut svc, &counters);
    }
    let st = svc.status();
    assert_eq!(st.events_by_kind, vec![("prompt".to_string(), 4)]);
    assert_eq!(st.signals, 1);
    assert_eq!(counters.get(codes::SPOOL_BAD_LINE), 2);
    assert!(!paths.spool().exists());
}

// ---- Phase 2: Copilot CLI status line, lifetime setting, budget across surfaces ----

fn full(t: Option<TipLineOut>) -> aiq_contracts::card::CardV1 {
    match t {
        Some(TipLineOut::Full(c)) => c,
        other => panic!("expected a full tip, got {other:?}"),
    }
}

#[test]
fn copilot_tip_shows_after_turn_ends_not_mid_turn() {
    let mut h = H::new();
    h.cp_run("c");
    assert_eq!(h.tip("c"), None, "mid-turn: the agent is still working");
    h.cp_stop("c");
    let card = full(h.tip("c"));
    assert!(card.noticed.contains("reworded this request 3 times"));
    assert_eq!(h.decisions(), vec![d("shown", "ok", 1)]);
}

#[test]
fn copilot_tip_default_clears_on_next_prompt_and_stays_in_history() {
    let mut h = H::new();
    h.cp_run("c");
    h.cp_stop("c");
    let card = full(h.tip("c"));
    assert_eq!(full(h.tip("c")).card_id, card.card_id, "refreshes keep showing it");
    h.cp_send("c", "ok, add a test for the empty case");
    assert_eq!(h.tip("c"), None);
    assert!(h.svc().show(&card.card_id, "cli").delivered, "still in history");
}

#[test]
fn copilot_tip_two_prompts_or_5_min_then_waiting_until_opened() {
    let mut h = H::new();
    h.svc().set("copilot_tip_stays", "two-prompts-or-5-min").unwrap();
    h.cp_run("c");
    h.cp_stop("c");
    let card = full(h.tip("c"));
    h.cp_send("c", "next thing: write the changelog entry");
    full(h.tip("c"));
    h.clock.advance(5 * MIN);
    assert_eq!(h.tip("c"), Some(TipLineOut::Waiting(card.card_id.clone())));
    h.svc().show(&card.card_id, "cli");
    assert_eq!(h.tip("c"), None);
}

#[test]
fn copilot_tip_until_opened_survives_prompts() {
    let mut h = H::new();
    h.svc().set("copilot_tip_stays", "until-opened").unwrap();
    h.cp_run("c");
    h.cp_stop("c");
    let card = full(h.tip("c"));
    for t in ["one", "two", "three"] {
        h.cp_send("c", t);
    }
    full(h.tip("c"));
    h.svc().show(&card.card_id, "vscode");
    assert_eq!(h.tip("c"), None);
}

#[test]
fn copilot_tip_without_session_id_uses_newest_session() {
    let mut h = H::new();
    h.cp_run("c");
    h.cp_stop("c");
    full(h.svc().tip_line("copilot-cli", None));
    assert_eq!(h.svc().tip_line("copilot-cli", Some("other")), None);
}

#[test]
fn quiet_period_claude_card_then_copilot_signal_held() {
    let mut h = H::new();
    let pid = h.reprompt_run("cc");
    assert!(h.stop("cc", Some(&pid)).is_some());
    h.cp_run("c");
    h.cp_stop("c");
    assert_eq!(h.tip("c"), None);
    assert!(h.decisions().contains(&d("hold", "quiet_period", 1)));
}

#[test]
fn quiet_period_both_ready_only_first_delivered_until_30_min() {
    let mut h = H::new();
    h.reprompt_run("cc");
    h.cp_run("c");
    h.cp_stop("c");
    // Both decided show-ready before either was delivered; the re-check at delivery holds the second.
    full(h.tip("c"));
    assert!(h.stop("cc", None).is_none());
    h.clock.advance(29 * MIN);
    assert!(h.stop("cc", None).is_none());
}

#[test]
fn quiet_period_ends_after_30_minutes() {
    let mut h = H::new();
    let pid = h.reprompt_run("cc");
    assert!(h.stop("cc", Some(&pid)).is_some());
    h.clock.advance(30 * MIN);
    h.cp_run("c");
    h.cp_stop("c");
    full(h.tip("c"));
}

#[test]
fn daily_cap_counts_every_surface() {
    let mut h = H::new();
    h.svc().set("daily_cap", "1").unwrap();
    let pid = h.reprompt_run("cc");
    assert!(h.stop("cc", Some(&pid)).is_some());
    h.clock.advance(31 * MIN);
    h.cp_run("c");
    h.cp_stop("c");
    assert_eq!(h.tip("c"), None);
    assert!(h.decisions().contains(&d("hold", "daily_cap", 1)));
}

#[test]
fn copilot_tip_paused_shows_nothing() {
    let mut h = H::new();
    h.cp_run("c");
    h.cp_stop("c");
    h.svc().set("paused", "true").unwrap();
    assert_eq!(h.tip("c"), None);
}

#[test]
fn take_card_any_delivers_only_after_turn_end_once() {
    let mut h = H::new();
    h.cp_run("c");
    assert!(h.svc().take_card_any("copilot-cli").is_none());
    h.cp_stop("c");
    assert!(h.svc().take_card_any("copilot-cli").is_some());
    assert!(h.svc().take_card_any("copilot-cli").is_none());
    assert!(h.svc().take_card_any("vscode").is_none());
}

#[test]
fn set_copilot_tip_stays_validates() {
    let mut h = H::new();
    assert!(h.svc().set("copilot_tip_stays", "forever").is_err());
    assert_eq!(h.svc().copilot_tip_stays(), aiq_domain::tipline::TipStays::NextPrompt);
    h.svc().set("copilot_tip_stays", "until-opened").unwrap();
    assert_eq!(h.svc().copilot_tip_stays(), aiq_domain::tipline::TipStays::UntilOpened);
}

// Import spec US-12, live side: an app's pipeline that calls Copilot with the same first prompt.
#[test]
fn automated_template_sessions_set_folder_aside_and_record_nothing_after() {
    let mut h = H::new();
    for i in 0..10 {
        h.cp_in(
            "C:\\Repos\\Executive_Presence",
            &format!("run{i}"),
            &format!("Collect profile for user {i} from https://x/{i}"),
            3 * MIN,
        );
        h.cp_stop(&format!("run{i}"));
    }
    assert_eq!(h.prompts(), 10, "before the rule fires, the first ten are recorded (simplify: in design)");
    h.cp_in("c:/repos/executive_presence/", "run10", "Collect profile for user 77 from https://x/77", 3 * MIN);
    h.cp_stop("run10");
    assert_eq!(h.prompts(), 10, "the 11th template session records nothing");
    let st = h.svc().status();
    assert_eq!(st.automated_folders, vec![("c:\\repos\\executive_presence".to_string(), "template".to_string())]);
    assert!(st.core_counters.iter().any(|(k, n)| k == codes::INGEST_AUTOMATED && *n == 2), "prompt and stop");
    // The folder stays set aside after a restart (persisted), whatever the prompt.
    h.restart();
    h.cp_in("C:\\Repos\\Executive_Presence", "run11", "something new entirely", 60 * MIN);
    assert_eq!(h.prompts(), 10);
    // Another folder is untouched.
    h.cp_in("C:\\Repos\\AIQ", "mine", "Collect profile for user 1 from https://x/1", MIN);
    assert_eq!(h.prompts(), 11);
}

#[test]
fn automated_folder_person_session_recorded_whole_from_second_prompt() {
    let mut h = H::new();
    h.svc().store().insert_folder_rule("/ep", "set_aside", "template", "live", T0).unwrap();
    for t in [BASE, R1, R2, R3, "and one more reword of the same thing"] {
        h.cp_in("/ep", "person", t, MIN);
    }
    assert_eq!(h.prompts(), 5, "first prompt held (H = 1 so far), then a person: recorded, first prompt included");
    assert!(h.svc().status().core_counters.iter().any(|(k, n)| k == codes::INGEST_AUTOMATED && *n == 1));
    let first: String = h
        .svc()
        .store()
        .raw()
        .query_row("SELECT t.text FROM event e JOIN content t ON t.event_id = e.event_id ORDER BY e.ts_utc_ms, e.rowid LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(first, BASE, "the held first prompt is the session's first recorded prompt");
}

#[test]
fn automated_folder_one_prompt_session_records_nothing() {
    let mut h = H::new();
    h.svc().store().insert_folder_rule("/ep", "set_aside", "template", "live", T0).unwrap();
    h.cp_in("/ep", "app", "collect profile 4", MIN);
    h.cp_stop("app");
    assert_eq!(
        h.prompts(),
        0,
        "a one-prompt session in a set-aside folder stays aside; its held prompt is never stored"
    );
}

/// The Set aside list counts the sessions it left out per folder (one row per session, no text). A person's session
/// there that sends a second prompt is recorded, so it no longer counts as set aside.
#[test]
fn set_aside_sessions_are_counted_once_each_and_a_rescued_one_drops_out() {
    let mut h = H::new();
    h.svc().store().insert_folder_rule("/ep", "set_aside", "template", "live", T0).unwrap();
    h.cp_in("/ep", "app1", "collect profile 4", MIN);
    h.cp_stop("app1");
    h.cp_in("/ep", "app2", "collect profile 5", MIN);
    h.cp_in("/ep", "person", BASE, MIN);
    let counts = h.svc().store().set_aside_session_counts().unwrap();
    assert_eq!(counts.get("/ep").map(|c| c.0), Some(3), "three first prompts set aside, the stop adds nothing");
    h.cp_in("/ep", "person", R1, MIN);
    let (n, last) = h.svc().store().set_aside_session_counts().unwrap()["/ep"];
    assert_eq!(n, 2, "the person's session is recorded now, so it is not counted as set aside");
    assert!(last > T0);
}

#[test]
fn template_in_a_folder_a_person_works_in_sets_aside_only_its_sessions() {
    let mut h = H::new();
    for i in 0..5 {
        h.cp_in("/tacit", &format!("me{i}"), &format!("my own question {}", "q".repeat(i + 1)), MIN);
    }
    for i in 0..11 {
        h.cp_in("/tacit", &format!("app{i}"), &format!("distil session {i}"), MIN);
    }
    assert_eq!(h.prompts(), 15, "5 of mine and 10 app runs before the template shows; the 11th is set aside");
    assert!(h.svc().status().automated_folders.is_empty(), "11 of 16 (69%) is below the folder bar: no folder rule");
    h.cp_in("/tacit", "me9", "one more of mine", MIN);
    assert_eq!(h.prompts(), 16, "a person's one-prompt session there is still recorded");
}

#[test]
fn include_folder_undoes_set_aside() {
    let mut h = H::new();
    h.svc().store().insert_folder_rule("/ep", "set_aside", "burst", "live", T0).unwrap();
    assert!(!h.svc().include_folder("/other").unwrap(), "not set aside: nothing written");
    assert!(h.svc().include_folder("/ep/").unwrap());
    assert!(h.svc().status().automated_folders.is_empty());
    h.cp_in("/ep", "s1", "hello there", MIN);
    assert_eq!(h.prompts(), 1);
}

/// Delete one item (design aiq-home-deferred-pages E1, E2): the text leaves the store, every store file on
/// disk, and the detector's memory; the event stays, and card evidence says it was deleted.
#[test]
fn forget_item_deletes_text_everywhere_and_keeps_the_event() {
    let mut h = H::new();
    const SECRET: &str = "zebra quartz marmalade refactor the login handler";
    h.send("a", BASE);
    let gone = h.prompt("a", SECRET);
    h.svc().ingest(&gone, None);
    h.send("a", R1);
    let before = h.svc().engine().clone();

    h.svc().forget_item(&gone.event_id).unwrap();

    // Memory: the working set equals one rebuilt from a store that never had the text.
    assert_ne!(h.svc().engine(), &before);
    let now = h.svc().engine().clone();
    h.restart();
    assert_eq!(h.svc().engine(), &now);
    // Store: the event stays (a prompt happened), its text is gone.
    let st = h.svc().store();
    let text: Option<String> =
        st.raw().query_row("SELECT text FROM content WHERE event_id = ?1", [&gone.event_id], |r| r.get(0)).ok();
    assert_eq!(text, None);
    assert_eq!(st.events_by_kind().unwrap(), vec![("prompt".to_string(), 3)]);
    // Disk: no store file holds the bytes (secure_delete + WAL truncate). Fails if either is removed.
    for suffix in ["", "-wal", "-shm"] {
        let p = format!("{}{suffix}", h.path.display());
        if let Ok(bytes) = std::fs::read(&p) {
            assert!(!bytes.windows(SECRET.len()).any(|w| w == SECRET.as_bytes()), "text still in {p}");
        }
    }
    // A second delete, or an id without text, is refused and changes nothing.
    assert!(h.svc().forget_item(&gone.event_id).is_err());
    assert!(h.svc().forget_item("no-such-event").is_err());
}

#[test]
fn forget_item_frame_reaches_the_writer() {
    let (tx, rx) = mpsc::sync_channel::<Cmd>(4);
    let t = std::thread::spawn(move || {
        if let Ok(Cmd::ForgetItem { event_id, reply }) = rx.recv() {
            let _ = reply.send(if event_id == "e1" { Ok(()) } else { Err("no".into()) });
        }
    });
    let req = Request::ForgetItem { token: "t".into(), event_id: "e1".into() };
    let out = dispatch(req, &tx, &Counters::default(), Instant::now());
    t.join().unwrap();
    assert!(matches!(&out, Dispatch::Reply(l) if l.contains("\"ok\":true")), "{out:?}");
}

/// Security review S2: a secret that reaches the core (an older hook's spool line, another client) is
/// removed before the store sees it, and counted for `aiq status`.
#[test]
fn ingest_removes_secrets_before_storing_and_counts_them() {
    let mut h = H::new();
    let key = "ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
    h.send("s", &format!("push with {key} and password=Tr0ub4dor"));
    h.send("s", "an ordinary prompt");
    let rows = Store::open(&h.path).unwrap().prompts_for_rebuild(0).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| !r.text.contains(key) && !r.text.contains("Tr0ub4dor")), "{rows:?}");
    let counted = h.svc().status().core_counters.into_iter().find(|(k, _)| k == codes::INGEST_SECRET_REMOVED);
    assert_eq!(counted, Some((codes::INGEST_SECRET_REMOVED.to_owned(), 2)));
}

// Rule RC: a claude.ai Projects / Remote Control session is Claude's own work, set aside from its first
// prompt whatever its prompt count; `aiq include` brings the folder's Projects sessions back.
#[test]
fn projects_sessions_record_nothing_until_the_folder_is_included() {
    let mut h = H::new();
    let rc = |h: &mut H, session: &str, text: &str| {
        let mut e = h.prompt(session, text);
        e.cwd = Some("C:\\Repos\\AIQ".into());
        e.projects = true;
        h.svc().ingest(&e, None);
    };
    for t in ["relay brief", "a person's message via the thread", "a peer's message"] {
        rc(&mut h, "rc1", t);
    }
    h.send("mine", "fix the build"); // a terminal session in the same folder: recorded
    assert_eq!(h.prompts(), 1);
    let st = h.svc().status();
    assert_eq!(st.automated_folders, vec![("c:\\repos\\aiq".to_string(), "projects".to_string())]);
    assert!(st.core_counters.iter().any(|(k, n)| k == codes::INGEST_AUTOMATED && *n == 3));
    assert!(h.svc().include_folder("c:/repos/aiq/").unwrap());
    assert!(h.svc().status().automated_folders.is_empty());
    rc(&mut h, "rc2", "relay brief two");
    assert_eq!(h.prompts(), 2, "included: Projects sessions are recorded");
}

// Rule RC, read by the core: the hook sends only the transcript path (opening it cost ~0.4 s of every
// Claude Code hook on the laptop). The core reads the head once per session and remembers the answer.
#[test]
fn the_core_reads_a_prompts_transcript_once_per_session_for_rule_rc() {
    let d = tempfile::tempdir().unwrap();
    let marked = d.path().join("rc.jsonl");
    std::fs::write(
        &marked,
        "{\"type\":\"user\",\"isMeta\":true,\"turnOrigin\":\"system\",\"entrypoint\":\"claude-desktop\"}\n",
    )
    .unwrap();
    let plain = d.path().join("cli.jsonl");
    std::fs::write(&plain, "{\"type\":\"user\",\"turnOrigin\":\"human\",\"entrypoint\":\"cli\"}\n").unwrap();
    let mut h = H::new();
    let send = |h: &mut H, session: &str, t: &std::path::Path| {
        let mut e = h.prompt(session, "do the thing");
        e.cwd = Some("C:\\Repos\\AIQ".into());
        e.transcript = Some(t.to_string_lossy().into_owned());
        h.svc().ingest(&e, None);
    };
    send(&mut h, "rc1", &marked);
    send(&mut h, "cli1", &plain);
    assert_eq!(h.prompts(), 1, "the Projects session is set aside, the terminal one recorded");
    // The answer is remembered: a changed file is not read again for the same session.
    std::fs::write(&plain, "{\"type\":\"user\",\"turnOrigin\":\"peer\"}\n").unwrap();
    send(&mut h, "cli1", &plain);
    assert_eq!(h.prompts(), 2);
    assert_eq!(h.svc().status().automated_folders, vec![("c:\\repos\\aiq".to_string(), "projects".to_string())]);
}

// Laptop 2026-11-02 21:28Z: a Remote Control session's first prompt found its transcript not yet written,
// AIQ remembered "not Projects", and every later event was recorded as the person's work. An unwritten head
// is read again at the next event (prompt or Stop) until it settles the answer.
#[test]
fn an_unwritten_transcript_head_is_read_again_until_it_settles_rule_rc() {
    let d = tempfile::tempdir().unwrap();
    let t = d.path().join("rc.jsonl");
    let mut h = H::new();
    let send = |h: &mut H, session: &str, t: &std::path::Path, text: &str| {
        let mut e = h.prompt(session, text);
        e.cwd = Some("C:\\Repos\\AIQ".into());
        e.transcript = Some(t.to_string_lossy().into_owned());
        h.svc().ingest(&e, None);
    };
    let stop = |h: &mut H, session: &str, t: &std::path::Path| {
        let mut e = h.prompt(session, "unused");
        e.body = EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None });
        e.transcript = Some(t.to_string_lossy().into_owned());
        h.svc().ingest(&e, None);
    };
    send(&mut h, "rc1", &t, "brief one"); // no file yet: recorded, unsettled
    assert_eq!(h.prompts(), 1);
    let queue = "{\"type\":\"queue-operation\",\"operation\":\"enqueue\"}\n";
    std::fs::write(&t, queue).unwrap();
    send(&mut h, "rc1", &t, "brief two"); // still no user record: unsettled
    assert_eq!(h.prompts(), 2);
    let marker = "{\"type\":\"user\",\"isMeta\":true,\"turnOrigin\":\"system\",\"entrypoint\":\"claude-desktop\"}\n";
    std::fs::write(&t, format!("{queue}{marker}")).unwrap();
    stop(&mut h, "rc1", &t);
    send(&mut h, "rc1", &t, "brief three");
    assert_eq!(h.prompts(), 2, "settled at the Stop: the rest of the session is set aside");
    assert_eq!(h.svc().status().automated_folders, vec![("c:\\repos\\aiq".to_string(), "projects".to_string())]);

    // A head that never settles is read at most RC_TRIES (8) times, then the session counts as the person's.
    let never = d.path().join("never.jsonl");
    std::fs::write(&never, queue).unwrap();
    for i in 0..8 {
        send(&mut h, "s2", &never, &format!("task {i}"));
    }
    std::fs::write(&never, format!("{queue}{marker}")).unwrap();
    send(&mut h, "s2", &never, "task 8");
    assert_eq!(h.prompts(), 11, "stopped looking after 8 unsettled reads");
}

// Rule AIQ: sessions in AIQ's own check and probe folders (`<temp>\aiq-*`) are tests of AIQ, set aside
// every prompt; the folder is listed in status and `aiq include` brings it back.
#[test]
fn aiq_check_folder_sessions_record_nothing_until_included() {
    let mut h = H::new();
    let check = r"C:\Users\p\AppData\Local\Temp\aiq-copilot-probe-3glkf9fj\run0";
    let send_in = |h: &mut H, session: &str, text: &str, cwd: &str| {
        let mut e = h.prompt(session, text);
        e.cwd = Some(cwd.into());
        h.svc().ingest(&e, None);
    };
    for t in ["run: echo aiq-probe, then say done", "a second prompt in the same test session"] {
        send_in(&mut h, "probe1", t, check);
    }
    send_in(&mut h, "mine", "fix the build", r"C:\Users\p\source\repos\AIQ");
    assert_eq!(h.prompts(), 1, "only the real work is recorded");
    let st = h.svc().status();
    assert_eq!(st.automated_folders, vec![(check.to_lowercase(), "aiq-check".to_string())]);
    assert!(h.svc().include_folder(check).unwrap());
    send_in(&mut h, "probe2", "now counted", check);
    assert_eq!(h.prompts(), 2, "included: recorded");
}

/// Copilot fires no hook for a message typed while the agent works (`delivery = "steering"`, 7 of 7 on
/// Imran's laptop, 2026-11-03). The core reads it from Copilot's own session log at agentStop.
#[test]
fn copilot_mid_turn_messages_are_read_from_the_session_log_once() {
    let mut h = H::new();
    let home = TempDir::new().unwrap();
    h.svc().set_copilot_home(home.path().to_owned());
    let log = home.path().join("session-state").join("sess-1").join("events.jsonl");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    let line = |delivery: &str, text: &str, ms: i64| {
        serde_json::json!({"type":"user.message","timestamp":format_rfc3339_ms(ms),
            "data":{"content":text,"delivery":delivery}})
        .to_string()
            + "\n"
    };
    let t = h.clock.now().utc_ms;
    // The hook sent the idle prompt; the steering one only reached the log; a secret goes before storing.
    h.cp_send("sess-1", "fix the flaky login test");
    let body = line("idle", "fix the flaky login test", t + MIN)
        + &line(
            "steering",
            "also run the whole suite, token ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            t + MIN + 5_000,
        );
    std::fs::write(&log, &body).unwrap();
    h.cp_stop("sess-1");
    assert_eq!(h.prompts(), 2, "hook prompt + the mid-turn message, the idle one not twice");
    let st = h.svc().status();
    assert!(st.core_counters.iter().any(|(k, n)| k == codes::INGEST_MID_TURN && *n == 1));
    let stored: Vec<String> = h
        .svc()
        .store()
        .raw()
        .prepare("SELECT c.text FROM event e JOIN content c USING (event_id) ORDER BY e.ts_utc_ms")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(stored[0], "fix the flaky login test");
    assert!(stored[1].starts_with("also run the whole suite") && !stored[1].contains("ghp_"), "{}", stored[1]);
    // Next stop: nothing new in the log, nothing new stored. After a restart the log is read from the start
    // again, and the stable ids store nothing twice.
    h.cp_stop("sess-1");
    h.restart();
    h.svc().set_copilot_home(home.path().to_owned());
    h.cp_stop("sess-1");
    assert_eq!(h.prompts(), 2);
    // Off switch: a mid-turn message sent while it is off is never recorded, also after it is turned back on
    // (the log is still read for the turn's actions, so the reader moves past it).
    h.svc().set("copilot_mid_turn", "off").unwrap();
    let body = body + &line("steering", "and update the docs", t + 2 * MIN);
    std::fs::write(&log, &body).unwrap();
    h.cp_stop("sess-1");
    assert_eq!(h.prompts(), 2);
    assert!(h.svc().set("copilot_mid_turn", "sometimes").is_err());
    h.svc().set("copilot_mid_turn", "on").unwrap();
    h.cp_stop("sess-1");
    assert_eq!(h.prompts(), 2);
    std::fs::write(&log, body + &line("steering", "and the changelog", t + 3 * MIN)).unwrap();
    h.cp_stop("sess-1");
    assert_eq!(h.prompts(), 3);
}

#[test]
fn copilot_mid_turn_missing_log_counts_and_records_the_stop() {
    let mut h = H::new();
    let home = TempDir::new().unwrap();
    h.svc().set_copilot_home(home.path().to_owned());
    h.cp_send("sess-2", "hello");
    h.cp_stop("sess-2");
    let st = h.svc().status();
    assert!(st.core_counters.iter().any(|(k, n)| k == codes::INGEST_LOG_READ && *n == 1));
    assert_eq!(st.events_by_kind.iter().find(|(k, _)| k == "stop").map(|(_, n)| *n), Some(1));
}

fn record_popups(h: &mut H) -> Arc<std::sync::Mutex<Vec<String>>> {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let s = seen.clone();
    h.svc().set_popup(Box::new(move |c| s.lock().unwrap().push(c.card_id.clone())));
    seen
}

#[test]
fn copilot_cli_stop_takes_the_card_for_the_conversation_and_the_status_line_keeps_it() {
    let mut h = H::new();
    h.cp_run("c");
    let card = h.cp_take("c").expect("default on: the card goes into the conversation");
    assert_eq!(full(h.tip("c")).card_id, card.card_id, "the status line shows the same delivered tip");
    assert_eq!(h.decisions(), vec![d("shown", "ok", 1)], "one delivery, not two");
}

#[test]
fn copilot_tip_in_chat_off_leaves_delivery_to_the_status_line() {
    let mut h = H::new();
    h.svc().set("copilot_tip_in_chat", "off").unwrap();
    assert!(h.svc().set("copilot_tip_in_chat", "yes").is_err());
    h.cp_run("c");
    assert_eq!(h.cp_take("c"), None);
    assert!(full(h.tip("c")).noticed.contains("reworded"));
}

#[test]
fn popup_shows_once_per_terminal_delivery_and_obeys_its_switch() {
    let mut h = H::new();
    let seen = record_popups(&mut h);
    h.cp_run("c");
    let card = h.cp_take("c").unwrap();
    full(h.tip("c")); // a status-line refresh of the same tip is not a second pop-up
    assert_eq!(*seen.lock().unwrap(), vec![card.card_id]);

    let mut h = H::new();
    let seen = record_popups(&mut h);
    h.svc().set("tip_popup", "off").unwrap();
    h.cp_run("c");
    assert!(h.cp_take("c").is_some());
    assert!(seen.lock().unwrap().is_empty(), "off: no pop-up");
}

/// A VS Code turn that raises a tip, then ends.
fn vscode_run(h: &mut H) {
    for (i, t) in [BASE, R1, R2, R3].iter().enumerate() {
        h.clock.advance(MIN);
        let body = EventBody::Prompt(PromptPayload {
            text: Some((*t).into()),
            chars: t.len() as u64,
            prompt_id: None,
            truncated: false,
        });
        let e = ev_on("vscode", &format!("v{i}"), "v", h.clock.now().utc_ms, body);
        h.svc().ingest(&e, None);
    }
    let stop = EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None });
    let e = ev_on("vscode", "vs", "v", h.clock.now().utc_ms, stop);
    h.svc().ingest(&e, None);
}

// Imran, 2026-11-03: VS Code tips show in the Copilot CLI status line while the AIQ VS Code extension is
// not asking for them (it was not installed on the laptop, so they never showed).
#[test]
fn vscode_tips_fall_back_to_the_status_line_without_the_extension() {
    let mut h = H::new();
    vscode_run(&mut h);
    h.cp_send("c", "what does this function do?");
    h.cp_stop("c");
    let shown = full(h.tip("c"));
    assert!(h.svc().take_card_any("vscode").is_none(), "shown once, on the status line");
    assert_eq!(full(h.tip("c")).card_id, shown.card_id, "it stays until the line's next prompt");
    h.cp_send("c", "thanks");
    assert!(h.tip("c").is_none());
}

#[test]
fn vscode_tips_stay_in_vscode_while_the_extension_asks() {
    let mut h = H::new();
    assert!(h.svc().take_card_any("vscode").is_none()); // the extension is polling
    vscode_run(&mut h);
    h.cp_send("c", "what does this function do?");
    h.cp_stop("c");
    assert!(h.tip("c").is_none(), "the extension asked within 10 minutes");
    h.clock.advance(11 * MIN);
    assert!(h.tip("c").is_some(), "no ask for 10 minutes: the status line shows it");
}

#[test]
fn vscode_delivery_never_pops_up() {
    let mut h = H::new();
    let seen = record_popups(&mut h);
    for (i, t) in [BASE, R1, R2, R3].iter().enumerate() {
        h.clock.advance(MIN);
        let body = EventBody::Prompt(PromptPayload {
            text: Some((*t).into()),
            chars: t.len() as u64,
            prompt_id: None,
            truncated: false,
        });
        let e = ev_on("vscode", &format!("v{i}"), "v", h.clock.now().utc_ms, body);
        h.svc().ingest(&e, None);
    }
    let e = ev_on(
        "vscode",
        "vs",
        "v",
        h.clock.now().utc_ms,
        EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None }),
    );
    h.svc().ingest(&e, None);
    assert!(h.svc().take_card_any("vscode").is_some(), "VS Code got its card");
    assert!(seen.lock().unwrap().is_empty(), "VS Code shows its own notification");
}

#[test]
fn switch_settings_default_on() {
    let mut h = H::new();
    for k in aiq_core::service::SWITCH_SETTINGS {
        assert!(h.svc().switch_on(k), "{k}");
        h.svc().set(k, "off").unwrap();
        assert!(!h.svc().switch_on(k), "{k}");
    }
}

/// A Copilot CLI stop the hook waits on (`take_card`) never reads the log on the hook's clock; the core reads
/// it after the reply (`after_stop`).
#[test]
fn copilot_mid_turn_read_waits_until_after_the_stop_reply() {
    let mut h = H::new();
    let home = TempDir::new().unwrap();
    h.svc().set_copilot_home(home.path().to_owned());
    let log = home.path().join("session-state").join("sess-3").join("events.jsonl");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    h.cp_send("sess-3", "first");
    let ts = format_rfc3339_ms(h.clock.now().utc_ms + 1_000);
    let line =
        serde_json::json!({"type":"user.message","timestamp":ts,"data":{"content":"steer","delivery":"steering"}});
    std::fs::write(&log, format!("{line}\n")).unwrap();
    let id = h.id();
    let stop = ev_on(
        "copilot-cli",
        &id,
        "sess-3",
        h.clock.now().utc_ms,
        EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None }),
    );
    let _ = h.svc().take_card(&stop, None, true);
    assert_eq!(h.prompts(), 1, "nothing read while the hook waits");
    h.svc().after_stop(&stop, None);
    assert_eq!(h.prompts(), 2);
}

// ------------------------------------------------------------------ live agent actions (schema v9)

fn turn_rows(h: &mut H) -> Vec<aiq_store::TurnActionRow> {
    h.svc().store().turn_actions_since(0).unwrap()
}

/// A Claude Code turn as the hook sends it: the prompt and its Stop carry the transcript path; the Stop is taken
/// (the hook waits for a card) and the transcript is read after the reply.
fn cc_turn(h: &mut H, session: &str, transcript: &std::path::Path, text: &str) -> EventV1 {
    let mut p = h.prompt(session, text);
    p.transcript = Some(transcript.to_string_lossy().into_owned());
    h.svc().ingest(&p, None);
    let mut s = h.prompt(session, "unused");
    s.body = EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None });
    s.transcript = p.transcript.clone();
    h.svc().take_card(&s, None, true);
    h.svc().after_stop(&s, None);
    s
}

fn cc_lines(rows: &[serde_json::Value]) -> String {
    rows.iter().map(|r| format!("{r}\n")).collect()
}

fn tool_use(name: &str, input: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":name,"input":input}]}})
}

/// Design live-agent-actions: at each Stop the core reads only the new part of the transcript and stores one
/// counts-only row per turn. Command text and file names never reach the store.
#[test]
fn claude_code_turn_actions_are_counted_per_turn_without_command_text() {
    let d = TempDir::new().unwrap();
    let t = d.path().join("s.jsonl");
    let mut body = cc_lines(&[
        serde_json::json!({"type":"user","message":{"content":"fix the login test"}}),
        tool_use("Read", serde_json::json!({"file_path":"src/secret_plan.rs"})),
        tool_use("Edit", serde_json::json!({"file_path":"src/secret_plan.rs"})),
        tool_use("Bash", serde_json::json!({"command":"cargo test --secret-flag"})),
        tool_use("Task", serde_json::json!({"prompt":"review"})),
    ]);
    std::fs::write(&t, &body).unwrap();
    let mut h = H::new();
    let stop1 = cc_turn(&mut h, "s1", &t, "fix the login test");
    let rows = turn_rows(&mut h);
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!((r.outcome.as_str(), r.why.as_deref()), ("recorded", None));
    assert_eq!(r.stop_event_id, stop1.event_id);
    assert_eq!((r.tool_calls, r.commands, r.files, r.subagents), (Some(4), Some(1), Some(1), Some(1)));
    assert_eq!(r.tools.as_deref(), Some(r#"{"Bash":1,"Edit":1,"Read":1,"Task":1}"#));
    assert!(r.bytes_read > 0);
    let dump: String = h
        .svc()
        .store()
        .raw()
        .prepare("SELECT * FROM turn_action")
        .unwrap()
        .query_map([], |row| Ok(format!("{:?}", (row.get::<_, String>(0)?, row.get::<_, Option<String>>(10)?))))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!dump.contains("secret"), "no file names or command text: {dump}");
    // The next turn counts only its own work.
    body += &cc_lines(&[
        serde_json::json!({"type":"user","message":{"content":"now the docs"}}),
        tool_use("Write", serde_json::json!({"file_path":"README.md"})),
    ]);
    std::fs::write(&t, &body).unwrap();
    cc_turn(&mut h, "s1", &t, "now the docs");
    let rows = turn_rows(&mut h);
    assert_eq!((rows.len(), rows[1].tool_calls, rows[1].tools.as_deref()), (2, Some(1), Some(r#"{"Write":1}"#)));
    // A replayed stop stores nothing new.
    h.svc().after_stop(&stop1, None);
    assert_eq!(turn_rows(&mut h).len(), 2);
}

/// A turn AIQ cannot read is "not recorded" with a reason and a count, never a guess; the switch turns it off.
#[test]
fn unreadable_turns_are_not_recorded_and_the_switch_turns_reading_off() {
    let d = TempDir::new().unwrap();
    let mut h = H::new();
    cc_turn(&mut h, "gone", &d.path().join("missing.jsonl"), "hello");
    let bad = d.path().join("bad.jsonl");
    std::fs::write(&bad, "{\"type\":\"user\",\"message\":{\"content\":\"hi\"}}\n{\"type\":\"user\", broken\n").unwrap();
    cc_turn(&mut h, "bad", &bad, "hi");
    let rows = turn_rows(&mut h);
    let got: Vec<_> = rows.iter().map(|r| (r.outcome.as_str(), r.why.as_deref(), r.tool_calls)).collect();
    assert_eq!(got, [("not_recorded", Some("no_log"), None), ("not_recorded", Some("unparsed"), None)]);
    let st = h.svc().status();
    assert!(st.core_counters.iter().any(|(k, n)| k == codes::INGEST_ACTIONS_NOT_RECORDED && *n == 2));
    let a = st.agent_actions.unwrap();
    assert_eq!((a.enabled, a.turns, a.not_recorded), (true, 2, 2));
    assert_eq!(a.why, [("no_log".to_string(), 1), ("unparsed".to_string(), 1)]);
    assert!(a.p95_ms.is_some() && a.max_bytes.is_some());
    // Off: nothing read, nothing stored.
    h.svc().set("agent_actions", "off").unwrap();
    assert!(h.svc().set("agent_actions", "maybe").is_err());
    cc_turn(&mut h, "gone", &d.path().join("missing.jsonl"), "again");
    assert_eq!(turn_rows(&mut h).len(), 2);
    assert!(!h.svc().status().agent_actions.unwrap().enabled);
}

/// Copilot CLI and VS Code agent mode share the session log; a set-aside session or a paused AIQ stores no row.
#[test]
fn copilot_turn_actions_come_from_the_session_log_and_respect_set_aside_and_pause() {
    let mut h = H::new();
    let home = TempDir::new().unwrap();
    h.svc().set_copilot_home(home.path().to_owned());
    let log = home.path().join("session-state").join("vs-1").join("events.jsonl");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    let patch = "*** Begin Patch\n*** Update File: a.py\n*** Add File: b.py\n*** End Patch";
    let lines = cc_lines(&[
        serde_json::json!({"type":"user.message","data":{"content":"add a parser","delivery":"idle"}}),
        serde_json::json!({"type":"tool.execution_start","data":{"toolName":"apply_patch","arguments":patch}}),
        serde_json::json!({"type":"tool.execution_start","data":{"toolName":"bash","arguments":"{\"command\":\"pytest\"}"}}),
        serde_json::json!({"type":"subagent.started","agentId":"x","data":{}}),
    ]);
    std::fs::write(&log, &lines).unwrap();
    let stop = |h: &mut H, surface: &str, session: &str| {
        h.clock.advance(MIN);
        let id = h.id();
        let body = EventBody::Stop(StopPayload { stop_hook_active: false, prompt_id: None });
        let e = ev_on(surface, &id, session, h.clock.now().utc_ms, body);
        h.svc().ingest(&e, None);
    };
    h.clock.advance(MIN);
    let id = h.id();
    let p = ev_on(
        "vscode",
        &id,
        "vs-1",
        h.clock.now().utc_ms,
        EventBody::Prompt(PromptPayload {
            text: Some("add a parser".into()),
            chars: 12,
            prompt_id: None,
            truncated: false,
        }),
    );
    h.svc().ingest(&p, None);
    stop(&mut h, "vscode", "vs-1");
    let rows = turn_rows(&mut h);
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].surface.as_str(), rows[0].tool_calls, rows[0].commands), ("vscode", Some(2), Some(1)));
    assert_eq!((rows[0].files, rows[0].subagents), (Some(2), Some(1)), "apply_patch names two files");
    // Paused: nothing stored.
    h.svc().set("paused", "true").unwrap();
    stop(&mut h, "vscode", "vs-1");
    assert_eq!(turn_rows(&mut h).len(), 1);
    h.svc().set("paused", "false").unwrap();
    // A stop for a session AIQ never stored (its stop is set aside by the shape check) stores no row.
    stop(&mut h, "copilot-cli", "");
    assert_eq!(turn_rows(&mut h).len(), 1);
}

/// Design live-rung-trend D: with no tip to show, the Copilot CLI status line names where the open task is
/// heading, from the actions recorded at each turn end. Off switch and the end of the task clear it.
#[test]
fn status_line_shows_the_live_trend_between_tips() {
    let mut h = H::new();
    let home = TempDir::new().unwrap();
    h.svc().set_copilot_home(home.path().to_owned());
    let log = home.path().join("session-state").join("t-1").join("events.jsonl");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    let brief = "Add a --since flag to the export command. Context: export dumps the whole timetable and ops want \
                 one day; keep the CSV columns unchanged and reject bad dates";
    let patch = "*** Begin Patch\n*** Update File: src/export.rs\n*** End Patch";
    std::fs::write(
        &log,
        cc_lines(&[
            serde_json::json!({"type":"user.message","data":{"content":brief,"delivery":"idle"}}),
            serde_json::json!({"type":"tool.execution_start","data":{"toolName":"apply_patch","arguments":patch}}),
        ]),
    )
    .unwrap();
    h.cp_send("t-1", brief);
    assert_eq!(h.tip("t-1"), None, "the agent is on its first turn: no estimate yet");
    h.cp_stop("t-1");
    assert_eq!(h.tip("t-1"), Some(TipLineOut::Trend("AIQ: heading for Delegated (3) so far · turn 1".into())));
    h.svc().set("live_trend", "off").unwrap();
    assert_eq!(h.tip("t-1"), None);
    h.svc().set("live_trend", "on").unwrap();
    h.clock.advance(31 * MIN);
    assert_eq!(h.tip("t-1"), None, "idle gap passed: the task ended");
}
