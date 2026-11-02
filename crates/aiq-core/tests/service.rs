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
        self.svc().take_card(&e, None, true)
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

#[test]
fn ready_card_expires_after_30_minutes() {
    let mut h = H::new();
    let pid = h.reprompt_run("s");
    h.clock.advance(31 * MIN);
    assert!(h.stop("s", Some(&pid)).is_none());
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
fn automated_folder_person_session_recorded_from_second_prompt() {
    let mut h = H::new();
    h.svc().store().insert_folder_rule("/ep", "set_aside", "template", "live", T0).unwrap();
    for t in [BASE, R1, R2, R3, "and one more reword of the same thing"] {
        h.cp_in("/ep", "person", t, MIN);
    }
    assert_eq!(h.prompts(), 4, "first prompt ignored (H = 1 so far), then a person: recorded");
    assert!(h.svc().status().core_counters.iter().any(|(k, n)| k == codes::INGEST_AUTOMATED && *n == 1));
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
