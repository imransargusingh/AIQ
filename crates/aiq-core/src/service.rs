//! The single writer / decision loop's logic: ingest, detect, decide, take-card, expire.
//! Owned by exactly one thread, so the Coaching-session invariant is a sequential check; the unique
//! delivery index in the store backs it up.
use crate::coaching;
use crate::counters::Counters;
use crate::sysclock::{format_rfc3339_ms, parse_rfc3339_ms};
use aiq_contracts::card::CardV1;
use aiq_contracts::codes;
use aiq_contracts::event::{EventBody, EventV1, MAX_SESSION_ID};
use aiq_contracts::frames::{CoachStatus, EvidencePrompt, LeanedOn, ShowReply, StatusReply};
use aiq_contracts::pack::PackV1;
use aiq_domain::automated::{Guard, Verdict, folder_key, is_aiq_check_folder};
use aiq_domain::card::{compose, compose_move};
use aiq_domain::clock::{Clock, deliveries_counted_today};
use aiq_domain::coach::{self, HabitState, TipPref};
use aiq_domain::job::{AlwaysUnclassified, Job, JobClassifier};
use aiq_domain::reprompt::{Engine, PromptObs, SignalDraft};
use aiq_domain::text::features;
use aiq_domain::tipline::{TipLine, TipStays, tip_line_state};
use aiq_domain::trigger::{DecideInput, Outcome, Reason, decide, in_quiet_period, may_deliver};
use aiq_store::{NewDecision, NewEvent, NewSignal, Store, StoreError};
use std::sync::Arc;
use tracing::{info, info_span, warn};

pub const SETTING_PAUSED: &str = "paused";
pub const SETTING_DAILY_CAP: &str = "daily_cap";
pub const SETTING_COPILOT_TIP_STAYS: &str = "copilot_tip_stays";
/// Where tips show (Imran 2026-11-03, "make tips stand out", each one a setting). All "on" or "off",
/// default on. `copilot_tip_in_chat`: a Copilot CLI tip is also printed into the conversation at the
/// turn's end. `tip_popup`: a Windows pop-up when a tip is delivered in a terminal (Claude Code, Copilot
/// CLI). `tip_highlight`: the yellow "AIQ tip:" badge and the VS Code status-bar colour.
pub const SETTING_COPILOT_TIP_IN_CHAT: &str = "copilot_tip_in_chat";
pub const SETTING_TIP_POPUP: &str = "tip_popup";
pub const SETTING_TIP_HIGHLIGHT: &str = "tip_highlight";
pub const SWITCH_SETTINGS: [&str; 4] =
    [SETTING_COPILOT_TIP_IN_CHAT, SETTING_TIP_POPUP, SETTING_TIP_HIGHLIGHT, SETTING_COPILOT_MID_TURN];
/// Surfaces whose tip shows in a terminal, where a pop-up helps when the person has looked away. VS Code
/// shows its own notification, so it gets no pop-up.
pub const POPUP_SURFACES: [&str; 2] = ["claude-code", "copilot-cli"];
/// Called with each card delivered on a pop-up surface while `tip_popup` is on (the core passes
/// `popup::show`; tests record).
pub type Popup = Box<dyn Fn(&CardV1) + Send>;
pub const MAX_DAILY_CAP: u32 = 50;
/// `on` (default) or `off`: read Copilot's own session log at agentStop for messages typed while the agent was
/// working, which fire no hook (`copilot_log`).
pub const SETTING_COPILOT_MID_TURN: &str = "copilot_mid_turn";
const MAX_LOG_OFFSETS: usize = 4096;
/// Copilot CLI's surface name (the hook client uses the same constant).
pub const COPILOT_CLI: &str = "copilot-cli";

/// `project_name.<folder key>`: the name the person gave a project in AIQ home (design project-filter P2).
pub const SETTING_PROJECT_NAME: &str = "project_name.";
/// Longest project name kept, in characters.
pub const MAX_PROJECT_NAME: usize = 60;

/// Validate a project rename: a folder key (1–1024 bytes, as the hooks keep `cwd`) and a name of at most
/// `MAX_PROJECT_NAME` characters with no control characters. Empty (after trimming) means "AIQ names it".
fn project_name(folder: &str, value: &str) -> Result<String, String> {
    if folder.is_empty() || folder.len() > 1024 {
        return Err("name a project folder".into());
    }
    let v = value.trim();
    if v.chars().count() > MAX_PROJECT_NAME || v.chars().any(char::is_control) {
        return Err(format!("a project name is up to {MAX_PROJECT_NAME} characters, on one line"));
    }
    Ok(v.to_owned())
}

pub struct Service<C: Clock> {
    store: Store,
    engine: Engine,
    pack: PackV1,
    clock: C,
    classifier: Box<dyn JobClassifier + Send>,
    counters: Arc<Counters>,
    guard: Guard,
    /// The first prompt of each session the automated rule holds back, in memory only, so a person's session in
    /// a set-aside folder is recorded whole when its second prompt arrives (H ≥ 2). Never written to disk unless
    /// that happens. Bounded: on overflow all are dropped, and those sessions are recorded from their second prompt.
    held: std::collections::HashMap<(String, String), EventV1>,
    /// Folder keys where a claude.ai Projects / Remote Control session was set aside (rule RC), so
    /// "Include them" can be offered there although no folder rule exists. In memory; bounded.
    projects_folders: std::collections::HashSet<String>,
    /// Rule RC result per Claude Code session, read from its transcript head until settled (bounded).
    rc_checked: std::collections::HashMap<String, RcCheck>,
    /// Coaching inputs derived from the store (design coaching-from-rungs); rebuilt when stale.
    coach: Option<coaching::Cache>,
    /// AIQ-COACH-GUESS-LOW is logged once per core run.
    guess_low_logged: bool,
    popup: Option<Popup>,
    /// Starts a background label of a new task's first prompt (`aiq label`); None in tests and without a model.
    label_requester: Option<LabelRequester>,
    /// Copilot's home, for its session logs. None (tests, unless set): mid-turn messages are not read.
    copilot_home: Option<std::path::PathBuf>,
    /// How far each live Copilot session's log has been read (bounded; a forgotten session is read again
    /// from the start, and stable event ids make that store nothing twice).
    log_offsets: std::collections::HashMap<String, u64>,
}

/// A new task whose kind the word guess could not name: label its first prompt in the background.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelRequest {
    pub task_id: String,
    pub surface: String,
    pub session: String,
    pub requested_ms: i64,
}

pub type LabelRequester = Box<dyn Fn(LabelRequest) + Send>;

/// A task's kind, from a word guess or a label, with what it rests on.
struct Kind<'a> {
    job: &'a str,
    task_type: &'a str,
    score: f64,
    evidence: &'a [String],
}

/// What the automated rule says about one live event.
enum Gate {
    Record,
    /// Record nothing (an app or script's session, or a first prompt held back), and the rule that said so.
    Skip(&'static str),
    /// Record this held first prompt, then the event: a person's second prompt in a held session.
    Rescue(Box<EventV1>),
}

/// First prompts held back at once (each a few KB at most, in memory).
const MAX_HELD: usize = 1024;

/// Rule RC per session: a settled answer, or how many reads found the head not yet written.
#[derive(Debug, Clone, Copy)]
enum RcCheck {
    Settled(bool),
    Unsettled(u32),
}

/// Reads of an unsettled transcript head per session before AIQ stops looking (a missing file reads in
/// 0 ms; a written head settles on its first user record, so this bound is rarely reached).
const RC_TRIES: u32 = 8;

/// One signal to decide on: the detector's fields and the card it would show.
struct Pending<'a> {
    task_id: &'a str,
    detector_id: &'a str,
    detector_version: u32,
    opportunity_id: &'a str,
    reprompt_count: u32,
    score: f64,
    evidence: &'a [String],
    job: Job,
    core_opportunity: bool,
    threshold: f64,
    suppressed: Option<Reason>,
    card: CardV1,
}

/// What the Copilot CLI status line shows.
#[derive(Debug, Clone, PartialEq)]
// One value per status-line call, never stored in bulk: the size difference does not matter.
#[allow(clippy::large_enum_variant)]
pub enum TipLineOut {
    Full(CardV1),
    /// The "1 tip waiting" marker, with the card id for `aiq show`.
    Waiting(String),
}

fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

impl<C: Clock> Service<C> {
    /// Build the service and rebuild the detector working set from stored prompts.
    pub fn new(store: Store, pack: PackV1, clock: C, counters: Arc<Counters>) -> Result<Self, StoreError> {
        let mut s = Self {
            engine: Engine::new(&pack),
            store,
            pack,
            clock,
            classifier: Box::new(AlwaysUnclassified),
            counters,
            guard: Guard::new(),
            held: Default::default(),
            projects_folders: Default::default(),
            rc_checked: Default::default(),
            coach: None,
            guess_low_logged: false,
            popup: None,
            label_requester: None,
            copilot_home: None,
            log_offsets: Default::default(),
        };
        s.rebuild()?;
        Ok(s)
    }

    /// Replay prompts of recently active sessions through a fresh engine, without emitting signals.
    /// Signals are a deterministic function of the same prompts, so the result equals the live state.
    pub fn rebuild(&mut self) -> Result<(), StoreError> {
        let now = self.clock.now().utc_ms;
        let mut engine = Engine::new(&self.pack);
        for r in self.store.prompts_for_rebuild(now - engine.idle_gap_ms())? {
            engine.observe(
                &r.surface,
                &r.session,
                PromptObs { event_id: &r.event_id, utc_ms: r.ts_utc_ms, text: &r.text },
            );
        }
        engine.prune(now);
        self.engine = engine;
        Ok(())
    }

    /// Where Copilot keeps its session logs (`tools::copilot_home`); the core process sets it at start.
    pub fn set_copilot_home(&mut self, home: std::path::PathBuf) {
        self.copilot_home = Some(home);
    }

    /// Wire the background labeller (the `aiq` binary does; must return at once, never wait on a model).
    pub fn set_label_requester(&mut self, f: LabelRequester) {
        self.label_requester = Some(f);
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn into_store(self) -> Store {
        self.store
    }

    pub fn paused(&self) -> bool {
        self.store.setting(SETTING_PAUSED).ok().flatten().as_deref() == Some("true")
    }

    pub fn daily_cap(&self) -> u32 {
        self.store
            .setting(SETTING_DAILY_CAP)
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(self.pack.trigger.default_daily_cap)
    }

    pub fn copilot_tip_stays(&self) -> TipStays {
        self.store
            .setting(SETTING_COPILOT_TIP_STAYS)
            .ok()
            .flatten()
            .and_then(|v| TipStays::parse(&v))
            .unwrap_or(TipStays::NextPrompt)
    }

    /// An on/off switch from `SWITCH_SETTINGS`; anything but "off" (including unset) is on.
    pub fn switch_on(&self, key: &str) -> bool {
        self.store.setting(key).ok().flatten().as_deref() != Some("off")
    }

    pub fn set_popup(&mut self, popup: Popup) {
        self.popup = Some(popup);
    }

    fn quiet(&self) -> Result<bool, StoreError> {
        Ok(in_quiet_period(self.store.last_delivery_ms()?, self.clock.now().utc_ms, self.pack.trigger.quiet_min))
    }

    fn counted_today(&self) -> Result<u32, StoreError> {
        Ok(deliveries_counted_today(self.store.delivery_days()?, self.clock.now().local_day()))
    }

    fn fail(&self, e: StoreError) {
        self.counters.inc(e.code);
        warn!(error.code = e.code, "store operation failed");
    }

    /// Ingest one event. Never panics; store failures are counted and logged.
    pub fn ingest(&mut self, ev: &EventV1, trace_id: Option<&str>) {
        // An async stop (VS Code, or a Copilot CLI stop that reached the core by the spool): read the turn's
        // mid-turn messages first, since they were typed before it. A stop the hook waits on (`take_card`)
        // reads them after its reply instead (`after_stop`), off the hook's clock.
        self.mid_turn_messages(ev, trace_id);
        self.ingest_event(ev, trace_id);
    }

    /// The work after a stop's reply is sent: Copilot messages typed while the agent worked.
    pub fn after_stop(&mut self, ev: &EventV1, trace_id: Option<&str>) {
        self.mid_turn_messages(ev, trace_id);
    }

    fn ingest_event(&mut self, ev: &EventV1, trace_id: Option<&str>) {
        let _s = info_span!("core.ingest", trace_id = trace_id.unwrap_or(""), aiq.kind = ev.kind()).entered();
        if let Err(e) = self.ingest_inner(ev, trace_id) {
            self.fail(e);
        }
    }

    fn ingest_inner(&mut self, ev: &EventV1, trace_id: Option<&str>) -> Result<(), StoreError> {
        if self.paused() {
            return Ok(()); // nothing captured while paused
        }
        if ev.surface_session_id.is_empty() || ev.surface_session_id.len() > MAX_SESSION_ID || ev.event_id.is_empty() {
            self.counters.inc(codes::INGEST_SHAPE);
            return Ok(());
        }
        match self.automated(ev)? {
            Gate::Skip(reason) => {
                self.counters.inc(codes::INGEST_AUTOMATED);
                // One line per skipped event, so a lost prompt can be traced to its rule (2026-11-03: six first
                // prompts of hand-typed VS Code sessions were dropped and nothing said why). No text, no path.
                info!(
                    trace_id = trace_id.unwrap_or(""),
                    aiq.reason = reason,
                    aiq.surface = ev.surface.as_str(),
                    aiq.kind = ev.kind(),
                    aiq.dir = ev.cwd.as_deref().and_then(aiq_domain::automated::folder_tag).unwrap_or_default(),
                    "event set aside"
                );
                return Ok(()); // an app or script drove this session: record nothing (import spec US-12)
            }
            // A person in a held session: record its first prompt (no tip now, it is late), then this one.
            Gate::Rescue(first) => self.record(&first, trace_id, false)?,
            Gate::Record => {}
        }
        self.record(ev, trace_id, true)
    }

    /// Store one event and run the detectors. `tips`: false for a held first prompt recorded late, which
    /// must not raise a card.
    fn record(&mut self, ev: &EventV1, trace_id: Option<&str>, tips: bool) -> Result<(), StoreError> {
        let now = self.clock.now().utc_ms;
        let ts = parse_rfc3339_ms(&ev.ts).unwrap_or(now);
        // Security review S2: the hook already removed secrets; this pass covers spool lines written by an
        // older hook and any other client. Markers from either pass are counted, so status shows both.
        let clean = match &ev.body {
            EventBody::Prompt(p) => p.text.as_deref().map(|t| aiq_domain::secrets::redact(t).0),
            EventBody::Stop(_) => None,
        };
        let removed = clean.as_deref().map_or(0, |t| t.matches(aiq_domain::secrets::MARKER).count());
        let (prompt_id, text, chars, meta) = match &ev.body {
            EventBody::Prompt(p) => {
                if p.text.is_none() {
                    self.counters.inc(codes::INGEST_SHAPE);
                }
                let f = clean.as_deref().map(|t| features(t, &self.pack.reprompt));
                let meta = serde_json::json!({
                    "hook_event": "UserPromptSubmit",
                    "chars": p.chars,
                    "has_text": p.text.is_some(),
                    "truncated": p.truncated,
                    "file_mentions": f.as_ref().map_or(0, |f| f.mentions.len()),
                    "pasted": f.as_ref().is_some_and(|f| f.pasted),
                    // Read by AIQ home's Tasks page (design aiq-home-app D7). Non-content: the working folder.
                    "folder": ev.cwd.as_deref().map(folder_key).filter(|k| !k.is_empty()),
                    "secrets_removed": removed,
                });
                (p.prompt_id.as_deref(), clean.as_deref(), Some(p.chars as i64), meta)
            }
            EventBody::Stop(s) => (
                s.prompt_id.as_deref(),
                None,
                None,
                serde_json::json!({ "hook_event": "Stop", "stop_hook_active": s.stop_hook_active }),
            ),
        };
        let inserted = self.store.insert_event(
            &NewEvent {
                event_id: &ev.event_id,
                surface: &ev.surface,
                session: &ev.surface_session_id,
                kind: ev.kind(),
                ts_utc_ms: ts,
                prompt_id,
                trace_id,
                collector_version: &ev.collector_version,
                prompt_chars: chars,
                payload_meta: &meta.to_string(),
                text,
            },
            now,
        )?;
        if !inserted {
            return Ok(()); // replay: idempotent, no second signal
        }
        (0..removed).for_each(|_| self.counters.inc(codes::INGEST_SECRET_REMOVED));
        let Some(text) = text else { return Ok(()) };
        let sig = {
            let _d = info_span!("core.detect", trace_id = trace_id.unwrap_or("")).entered();
            self.engine.observe(
                &ev.surface,
                &ev.surface_session_id,
                PromptObs { event_id: &ev.event_id, utc_ms: ts, text },
            )
        };
        let new_task = self.engine.task_id(&ev.surface, &ev.surface_session_id) == Some(ev.event_id.as_str());
        if !tips {
            return Ok(());
        }
        if let Some(sig) = sig {
            self.on_signal(&ev.surface, &ev.surface_session_id, &sig, trace_id)?;
        }
        if new_task {
            // A live first prompt of a new task (only live events reach here: imports never raise a card, D11).
            self.next_rung(&ev.surface, &ev.surface_session_id, &ev.event_id, text, trace_id)?;
        }
        Ok(())
    }

    /// Copilot messages typed while the agent was working fire no hook; read them from the new part of the
    /// session's own log and ingest each as a prompt. Off the hook's clock: the Copilot hook does not wait for
    /// the core. Stable event ids and a same-text check keep a message the hook did send from counting twice.
    fn mid_turn_messages(&mut self, ev: &EventV1, trace_id: Option<&str>) {
        if !matches!(ev.body, EventBody::Stop(_)) || !matches!(ev.surface.as_str(), "copilot-cli" | "vscode") {
            return;
        }
        let Some(path) =
            self.copilot_home.as_deref().and_then(|h| crate::copilot_log::session_log(h, &ev.surface_session_id))
        else {
            return;
        };
        if !self.switch_on(SETTING_COPILOT_MID_TURN) {
            return;
        }
        let key = format!("{}/{}", ev.surface, ev.surface_session_id);
        let from = self.log_offsets.get(&key).copied().unwrap_or(0);
        let started = std::time::Instant::now();
        let (found, next) = match crate::copilot_log::steering_since(&path, from) {
            Ok(r) => r,
            Err(_) => {
                self.counters.inc(codes::INGEST_LOG_READ);
                return;
            }
        };
        if self.log_offsets.len() >= MAX_LOG_OFFSETS {
            self.log_offsets.clear();
        }
        self.log_offsets.insert(key, next);
        let mut stored = 0;
        for t in &found {
            let clean = aiq_domain::secrets::redact(&t.text).0;
            let text = cap(&clean, aiq_contracts::event::MAX_PROMPT_BYTES);
            if self.store.prompt_text_seen(&ev.surface, &ev.surface_session_id, &text).unwrap_or(false) {
                continue;
            }
            let mut p = ev.clone();
            p.event_id = crate::copilot_log::event_id(&ev.surface, &ev.surface_session_id, t);
            p.ts = t.ts_ms.map(format_rfc3339_ms).unwrap_or_else(|| ev.ts.clone());
            p.body = EventBody::Prompt(aiq_contracts::event::PromptPayload {
                chars: t.text.chars().count() as u64,
                truncated: clean.len() > text.len(),
                text: Some(text.clone()),
                prompt_id: None,
            });
            if let Err(e) = self.ingest_inner(&p, trace_id) {
                self.fail(e);
                continue;
            }
            // Not stored when the session or folder is set aside (that drop logs its own line).
            if self.store.prompt_text_seen(&ev.surface, &ev.surface_session_id, &text).unwrap_or(false) {
                stored += 1;
                self.counters.inc(codes::INGEST_MID_TURN);
            }
        }
        info!(
            trace_id = trace_id.unwrap_or(""),
            found = found.len(),
            stored,
            bytes = next - from.min(next),
            ms = started.elapsed().as_millis() as u64,
            "copilot.mid_turn"
        );
    }

    /// The coaching cache, rebuilt when the store's coaching facts changed or it is an hour old.
    pub fn coach_cache(&mut self) -> Result<&coaching::Cache, StoreError> {
        let now = self.clock.now();
        if self.coach.as_ref().is_none_or(|c| c.stale(&self.store, now.utc_ms)) {
            let c = coaching::Cache::build(&self.store, &self.pack, now)?;
            info!(
                coach.cache_ms = c.build_ms,
                coach.tasks = c.tasks.len(),
                coach.neighbours = c.neighbours.len(),
                "coach.cache"
            );
            self.coach = Some(c);
        }
        Ok(self.coach.as_ref().expect("built above"))
    }

    /// Next-rung tip (design coaching-from-rungs D4–D12): guess the new task's type from the person's own
    /// judged tasks; if that type coaches a move, decide on a card through the existing trigger engine.
    fn next_rung(
        &mut self,
        surface: &str,
        session: &str,
        task_id: &str,
        text: &str,
        trace_id: Option<&str>,
    ) -> Result<(), StoreError> {
        let started = std::time::Instant::now();
        let mode = coaching::mode(&self.store);
        if mode == "off" || self.paused() {
            return Ok(());
        }
        let pack = self.pack.clone();
        self.coach_cache()?;
        // Taken out and put back so the decision can use `self` without copying thousands of neighbours.
        let cache = self.coach.take().expect("built above");
        let r = self.next_rung_with(&cache, &pack, &mode, surface, session, task_id, text, trace_id, started);
        self.coach = Some(cache);
        r
    }

    #[allow(clippy::too_many_arguments)]
    fn next_rung_with(
        &mut self,
        cache: &coaching::Cache,
        pack: &aiq_contracts::pack::PackV1,
        mode: &str,
        surface: &str,
        session: &str,
        task_id: &str,
        text: &str,
        trace_id: Option<&str>,
        started: std::time::Instant,
    ) -> Result<(), StoreError> {
        let (words_on, labels_on) = (cache.words_on(mode, pack), cache.labels_on(mode, pack) && self.labels_enabled());
        if !words_on && !labels_on {
            if !self.guess_low_logged {
                self.guess_low_logged = true;
                self.counters.inc(codes::COACH_GUESS_LOW);
                warn!(
                    error.code = codes::COACH_GUESS_LOW,
                    checked = cache.check.checked,
                    right = cache.check.right,
                    label_checked = cache.label_check.checked,
                    label_right = cache.label_check.right,
                    "next-rung tips turned off by AIQ's self-check"
                );
            }
            return Ok(());
        }
        let words = features(text, &pack.reprompt).words;
        let guess = if words_on { coach::guess_type(&words, &cache.neighbours, &pack.coach) } else { None };
        let Some(g) = guess else {
            // No word match: ask the small model in the background (design kind-label). The core never waits on
            // a model (C9); the answer comes back as `on_kind_label`, before the turn's Stop in most turns.
            let asked = labels_on && self.request_label(surface, session, task_id);
            info!(
                ms = started.elapsed().as_millis() as u64,
                neighbours = cache.neighbours.len(),
                coach.outcome = if asked { "label_requested" } else { "no_guess" },
                "coach.guess"
            );
            return Ok(());
        };
        let now = self.clock.now();
        self.store.insert_type_guess(&aiq_store::TypeGuessRow {
            guess_id: new_id(),
            task_id: task_id.to_owned(),
            surface: surface.to_owned(),
            job: g.job.clone(),
            task_type: g.task_type.clone(),
            similarity: g.similarity,
            neighbours: g.neighbours.clone(),
            utc_ms: now.utc_ms,
            method: "words".into(),
        })?;
        let kind = Kind { job: &g.job, task_type: &g.task_type, score: g.similarity, evidence: &g.neighbours };
        self.decide_kind(cache, pack, mode, surface, session, task_id, kind, "words", trace_id, started)
    }

    /// Labels can run: a labeller is wired, background judging is on and labels are not turned off.
    fn labels_enabled(&self) -> bool {
        self.label_requester.is_some() && coaching::labels_enabled(&self.store)
    }

    /// Ask for a label of this new task's first prompt (labels are enabled). True when asked.
    fn request_label(&mut self, surface: &str, session: &str, task_id: &str) -> bool {
        let Some(req) = self.label_requester.as_ref() else { return false };
        if !matches!(surface, "claude-code" | "copilot-cli" | "vscode") {
            return false;
        }
        self.counters.inc(codes::COACH_LABEL_REQUESTED);
        req(LabelRequest {
            task_id: task_id.to_owned(),
            surface: surface.to_owned(),
            session: session.to_owned(),
            requested_ms: self.clock.now().utc_ms,
        });
        true
    }

    /// A label for `task_id` was stored (`aiq label` says so): decide on a next-step tip from it, exactly as from
    /// a word guess. Dropped when the session has moved to another task, the task already has a guess, the label
    /// is not confident, or AIQ's self-check turned labels off.
    pub fn on_kind_label(&mut self, task_id: &str) -> Result<(), StoreError> {
        let started = std::time::Instant::now();
        let mode = coaching::mode(&self.store);
        if mode == "off" || self.paused() {
            return Ok(());
        }
        let Some(l) = self.store.kind_labels_of(task_id)?.into_iter().rev().find(|l| l.outcome == "ok") else {
            return Ok(());
        };
        let outcome = |o: &str| {
            info!(
                ms = started.elapsed().as_millis() as u64,
                latency_ms = l.latency_ms,
                coach.outcome = o,
                "coach.label"
            )
        };
        if self.engine.task_id(&l.surface, &l.surface_session_id) != Some(task_id) {
            outcome("label_task_moved_on");
            return Ok(());
        }
        if self.store.has_type_guess(task_id)? {
            return Ok(()); // already decided (a repeated notice)
        }
        let label = aiq_domain::label::Label {
            job: l.job.clone().unwrap_or_default(),
            job_confidence: l.job_confidence.unwrap_or(0.0),
            task_type: l.task_type.clone().unwrap_or_default(),
            task_type_confidence: l.task_type_confidence.unwrap_or(0.0),
        };
        let Some((job, task_type)) = aiq_domain::label::accept(&label) else {
            outcome("label_not_confident");
            return Ok(());
        };
        let pack = self.pack.clone();
        self.coach_cache()?;
        let cache = self.coach.take().expect("built above");
        let r = (|| {
            if !cache.labels_on(&mode, &pack) {
                outcome("label_self_check_off");
                return Ok(());
            }
            let score = label.job_confidence.min(label.task_type_confidence);
            self.store.insert_type_guess(&aiq_store::TypeGuessRow {
                guess_id: new_id(),
                task_id: task_id.to_owned(),
                surface: l.surface.clone(),
                job: job.to_owned(),
                task_type: task_type.to_owned(),
                similarity: score,
                neighbours: vec![],
                utc_ms: self.clock.now().utc_ms,
                method: aiq_domain::label::METHOD.into(),
            })?;
            let evidence = [format!("label:{}", l.model.as_deref().unwrap_or(&l.provider))];
            let kind = Kind { job, task_type, score, evidence: &evidence };
            self.decide_kind(
                &cache,
                &pack,
                &mode,
                &l.surface,
                &l.surface_session_id,
                task_id,
                kind,
                "label",
                None,
                started,
            )
        })();
        self.coach = Some(cache);
        r
    }

    /// The next-step decision for a task whose kind is known (from words or a label): the kind's coached move,
    /// the accuracy pause (K6b), the person's answers, cooldown and habit, then the trigger engine.
    #[allow(clippy::too_many_arguments)]
    fn decide_kind(
        &mut self,
        cache: &coaching::Cache,
        pack: &aiq_contracts::pack::PackV1,
        mode: &str,
        surface: &str,
        session: &str,
        task_id: &str,
        g: Kind<'_>,
        method: &str,
        trace_id: Option<&str>,
        started: std::time::Instant,
    ) -> Result<(), StoreError> {
        let now = self.clock.now();
        let row = cache
            .rows
            .iter()
            .find(|r| r.job == g.job && r.task_type == g.task_type && r.state == coach::RowState::Coach);
        let Some((row, m)) = row.and_then(|r| {
            let id = r.move_id.as_deref()?;
            pack.moves.iter().find(|m| m.move_id == id).map(|m| (r, m))
        }) else {
            info!(
                ms = started.elapsed().as_millis() as u64,
                similarity = g.score,
                method,
                coach.outcome = "not_coachable",
                "coach.guess"
            );
            return Ok(());
        };
        let tip = coach::tip_id(&m.move_id);
        let cooldown_ms = pack.coach.move_cooldown_h as i64 * 3_600_000;
        let accuracy = cache.paused_for(&self.store, mode, &row.job, &row.task_type).map(|v| {
            self.counters.inc(codes::COACH_ACCURACY_PAUSED);
            info!(
                scope = v.scope.key().as_str(),
                agree = v.agree.k,
                checked = v.agree.n,
                "next-step tip held: the accuracy check paused this job or kind"
            );
        });
        let suppressed = match coach::tip_pref(&cache.feedback, &tip, now.utc_ms, &pack.coach) {
            _ if accuracy.is_some() => Some(Reason::AccuracyPaused),
            TipPref::Off => Some(Reason::TipOff),
            TipPref::Snoozed(_) => Some(Reason::Snoozed),
            TipPref::Shown => {
                if self.store.deliveries_of_tip(&tip)?.first().is_some_and(|t| now.utc_ms - t < cooldown_ms) {
                    Some(Reason::Cooldown)
                } else if cache
                    .habits
                    .iter()
                    .any(|h| h.job == row.job && h.move_id == m.move_id && h.state == HabitState::Adopted)
                {
                    Some(Reason::HabitAdopted)
                } else {
                    None
                }
            }
        };
        let card = compose_move(m, &row.job, &row.task_type, &new_id(), &pack.version);
        let (job, threshold) = (row.job.clone(), pack.coach.match_threshold);
        self.decide_and_record(
            surface,
            session,
            Pending {
                task_id,
                detector_id: coach::DETECTOR,
                detector_version: coach::DETECTOR_VERSION,
                opportunity_id: coach::OPPORTUNITY,
                reprompt_count: 0,
                score: g.score,
                evidence: g.evidence,
                job: Job::Named(job),
                core_opportunity: false,
                threshold,
                suppressed,
                card,
            },
            trace_id,
        )?;
        info!(
            ms = started.elapsed().as_millis() as u64,
            similarity = g.score,
            method,
            coach.outcome = "decided",
            "coach.guess"
        );
        Ok(())
    }

    /// Rule RC from the transcript the event names: read its head until it settles the answer, then
    /// remember. An unsettled head (not yet written) is read again on the session's next events, up to
    /// `RC_TRIES` reads. A session whose events carry no transcript (older hooks set `projects`
    /// themselves) is never read.
    fn projects_session(&mut self, ev: &EventV1) -> bool {
        use crate::projects::Head;
        let key = format!("{}/{}", ev.surface, ev.surface_session_id);
        match self.rc_checked.get(&key) {
            Some(RcCheck::Settled(v)) => return *v,
            Some(RcCheck::Unsettled(n)) if *n >= RC_TRIES => return false,
            _ => {}
        }
        let Some(t) = ev.transcript.as_deref() else { return false };
        let started = std::time::Instant::now();
        let head = crate::projects::scan(std::path::Path::new(t), crate::projects::HEAD_BYTES);
        let v = head == Head::Projects;
        info!(ms = started.elapsed().as_millis() as u64, projects = v, settled = head != Head::Unknown, "rc.check");
        if self.rc_checked.len() >= 4096 {
            self.rc_checked.clear();
        }
        let check = match (head, self.rc_checked.get(&key)) {
            (Head::Unknown, Some(RcCheck::Unsettled(n))) => RcCheck::Unsettled(n + 1),
            (Head::Unknown, _) => RcCheck::Unsettled(1),
            _ => RcCheck::Settled(v),
        };
        self.rc_checked.insert(key, check);
        v
    }

    /// The automated-session rule, live side. A skip names its rule (`projects`, `aiq-check`, `session`,
    /// `folder`, `template`, `burst`) for the log.
    fn automated(&mut self, ev: &EventV1) -> Result<Gate, StoreError> {
        let (surface, session) = (ev.surface.as_str(), ev.surface_session_id.as_str());
        if ev.projects || self.projects_session(ev) {
            // Rule RC: Claude's own Projects / Remote Control session, every event, unless included.
            let k = ev.cwd.as_deref().map(folder_key).unwrap_or_default();
            if self.store.folder_rules_now()?.get(&k).map(String::as_str) != Some("include") {
                if self.projects_folders.len() < 1024 && !k.is_empty() && self.projects_folders.insert(k) {
                    info!(aiq.reason = "projects", "Projects session set aside");
                }
                return Ok(Gate::Skip("projects"));
            }
        }
        if let Some(k) = ev.cwd.as_deref().filter(|c| is_aiq_check_folder(c)).map(folder_key) {
            // Rule AIQ: AIQ's own check and probe folders, every event, unless included. The folder gets a
            // set-aside rule once, so `aiq status` lists it and `aiq include` brings it back.
            match self.store.folder_rules_now()?.get(&k).map(String::as_str) {
                Some("include") => {}
                Some(_) => return Ok(Gate::Skip("aiq-check")),
                None => {
                    self.store.insert_folder_rule(&k, "set_aside", "aiq-check", "live", self.clock.now().utc_ms)?;
                    info!(aiq.reason = "aiq-check", "folder set aside: AIQ's own checks");
                    return Ok(Gate::Skip("aiq-check"));
                }
            }
        }
        let EventBody::Prompt(p) = &ev.body else {
            return Ok(if self.guard.is_ignored(surface, session) { Gate::Skip("session") } else { Gate::Record });
        };
        if !self.guard.is_ignored(surface, session) && self.store.session_known(surface, session)? {
            return Ok(Gate::Record); // not a first prompt, and not a session being ignored
        }
        let set_aside = match ev.cwd.as_deref().map(folder_key) {
            Some(k) if !k.is_empty() => self.store.set_aside_folders()?.iter().any(|(f, _, _)| *f == k),
            _ => false,
        };
        let now = self.clock.now().utc_ms;
        let verdict = self.guard.on_prompt(
            &self.pack.automated,
            surface,
            session,
            ev.cwd.as_deref(),
            p.text.as_deref(),
            false,
            set_aside,
            now,
        );
        let key = (surface.to_owned(), session.to_owned());
        let (reason, new_folder) = match verdict {
            Verdict::Record => return Ok(Gate::Record),
            Verdict::Rescued => {
                return Ok(self.held.remove(&key).map_or(Gate::Record, |e| Gate::Rescue(Box::new(e))));
            }
            Verdict::Ignore { reason, new_folder } => (reason, new_folder),
        };
        if self.held.len() >= MAX_HELD {
            self.held.clear();
        }
        self.held.insert(key, ev.clone());
        if let Some(folder) = new_folder {
            self.store.insert_folder_rule(&folder, "set_aside", reason.as_str(), "live", now)?;
            info!(aiq.reason = reason.as_str(), "folder set aside: automated sessions");
        }
        Ok(Gate::Skip(reason.as_str()))
    }

    /// Folders whose sessions AIQ ignores now, newest first: (folder, reason).
    pub fn set_aside_folders(&self) -> Vec<(String, String)> {
        self.store.set_aside_folders().unwrap_or_default().into_iter().map(|(f, r, _)| (f, r)).collect()
    }

    /// "Include them": the person says a set-aside folder is their own work. Returns false if it was not set aside.
    pub fn include_folder(&mut self, folder: &str) -> Result<bool, StoreError> {
        let k = folder_key(folder);
        if !self.projects_folders.contains(&k) && !self.store.set_aside_folders()?.iter().any(|(f, _, _)| *f == k) {
            return Ok(false);
        }
        self.store.insert_folder_rule(&k, "include", "person", "person", self.clock.now().utc_ms)?;
        Ok(true)
    }

    fn on_signal(
        &mut self,
        surface: &str,
        session: &str,
        sig: &SignalDraft,
        trace_id: Option<&str>,
    ) -> Result<(), StoreError> {
        let r = self.pack.reprompt.clone();
        let tip = self.pack.tips.iter().find(|t| t.opportunity_id == r.opportunity_id).expect("pack validated").clone();
        let feedback = coaching::feedback(&self.store)?;
        let suppressed = match coach::tip_pref(&feedback, &tip.tip_id, self.clock.now().utc_ms, &self.pack.coach) {
            TipPref::Off => Some(Reason::TipOff),
            TipPref::Snoozed(_) => Some(Reason::Snoozed),
            TipPref::Shown => None,
        };
        let card = compose(&tip, sig.reprompt_count, &new_id(), &self.pack.version);
        self.decide_and_record(
            surface,
            session,
            Pending {
                task_id: &sig.task_id,
                detector_id: &r.detector_id,
                detector_version: r.detector_version,
                opportunity_id: &r.opportunity_id,
                reprompt_count: sig.reprompt_count,
                score: sig.score,
                evidence: &sig.evidence,
                job: self.classifier.classify(&sig.task_id),
                core_opportunity: r.core_opportunity,
                threshold: self.pack.trigger.in_moment_threshold,
                suppressed,
                card,
            },
            trace_id,
        )
    }

    /// Record the signal, decide with the trigger engine, record the decision with its card.
    fn decide_and_record(
        &mut self,
        surface: &str,
        session: &str,
        p: Pending<'_>,
        trace_id: Option<&str>,
    ) -> Result<(), StoreError> {
        let _d = info_span!("core.decide", trace_id = trace_id.unwrap_or("")).entered();
        let now = self.clock.now();
        self.expire_stale()?;
        let signal_id = new_id();
        self.store.insert_signal(
            &NewSignal {
                signal_id: &signal_id,
                task_id: p.task_id,
                surface,
                session,
                detector_id: p.detector_id,
                detector_version: p.detector_version,
                pack_version: &self.pack.version,
                opportunity_id: p.opportunity_id,
                reprompt_count: p.reprompt_count,
                score: p.score,
                evidence: p.evidence,
            },
            now.utc_ms,
        )?;
        let input = DecideInput {
            paused: self.paused(),
            job: p.job,
            core_opportunity: p.core_opportunity,
            score: p.score,
            threshold: p.threshold,
            delivered_in_session: self.store.delivered_in_session(surface, session)?,
            in_quiet_period: self.quiet()?,
            ready_in_session: self.store.ready_card(surface, session)?.is_some(),
            deliveries_counted_today: self.counted_today()?,
            daily_cap: self.daily_cap(),
            suppressed: p.suppressed,
        };
        let (outcome, reason) = decide(&input);
        self.store.insert_decision(
            &NewDecision {
                decision_id: &new_id(),
                card_id: &p.card.card_id,
                signal_id: &signal_id,
                surface,
                session,
                outcome: outcome.as_str(),
                reason_code: reason.as_str(),
                rule_id: &self.pack.trigger.rule_id,
                pack_version: &self.pack.version,
                card_json: &serde_json::to_string(&p.card).expect("card serializes"),
            },
            now.utc_ms,
        )?;
        info!(
            aiq.outcome = outcome.as_str(),
            aiq.reason = reason.as_str(),
            aiq.opportunity = p.opportunity_id,
            aiq.reprompt_count = p.reprompt_count,
            "card decision"
        );
        Ok(())
    }

    /// Expire show-ready cards not delivered within the expiry window, so a stale card never shows in a
    /// later, unrelated turn.
    pub fn expire_stale(&mut self) -> Result<(), StoreError> {
        let now = self.clock.now().utc_ms;
        let before = now - self.pack.trigger.ready_expiry_min as i64 * 60_000;
        for c in self.store.stale_ready(before)? {
            self.store.insert_decision(
                &NewDecision {
                    decision_id: &new_id(),
                    card_id: &c.card_id,
                    signal_id: &c.signal_id,
                    surface: &c.surface,
                    session: &c.session,
                    outcome: Outcome::Drop.as_str(),
                    reason_code: Reason::Expired.as_str(),
                    rule_id: &self.pack.trigger.rule_id,
                    pack_version: &self.pack.version,
                    card_json: &c.card_json,
                },
                now,
            )?;
        }
        Ok(())
    }

    /// Stop: store the stop event, then hand over the session's show-ready card if the budget still allows.
    /// `fresh` is false when the request waited longer than the client will wait; then nothing is
    /// delivered, because a card handed to a client that already left would be lost.
    pub fn take_card(&mut self, ev: &EventV1, trace_id: Option<&str>, fresh: bool) -> Option<CardV1> {
        self.ingest_event(ev, trace_id);
        let _s = info_span!("core.take_card", trace_id = trace_id.unwrap_or("")).entered();
        match self.take_card_inner(ev, fresh) {
            Ok(c) => c,
            Err(e) => {
                self.fail(e);
                None
            }
        }
    }

    fn take_card_inner(&mut self, ev: &EventV1, fresh: bool) -> Result<Option<CardV1>, StoreError> {
        let EventBody::Stop(stop) = &ev.body else { return Ok(None) };
        if stop.stop_hook_active || !fresh {
            return Ok(None);
        }
        self.expire_stale()?;
        let (surface, session) = (ev.surface.as_str(), ev.surface_session_id.as_str());
        if let Some(pid) = stop.prompt_id.as_deref()
            && !self.store.prompt_seen(surface, session, pid)?
        {
            return Ok(None); // this turn's prompt has not arrived yet: the card waits for the next Stop
        }
        if surface == COPILOT_CLI && !self.switch_on(SETTING_COPILOT_TIP_IN_CHAT) {
            return Ok(None); // the status line delivers it instead
        }
        let Some(ready) = self.store.ready_card(surface, session)? else { return Ok(None) };
        self.deliver(&ready.card_id, &ready.card_json, surface, session)
    }

    /// Re-check the budget (pause, session, quiet period, daily cap) and record the hand-over in the
    /// single writer, so no two renderers on any surface can deliver past the budget.
    fn deliver(
        &mut self,
        card_id: &str,
        card_json: &str,
        surface: &str,
        session: &str,
    ) -> Result<Option<CardV1>, StoreError> {
        let ok = may_deliver(
            self.paused(),
            self.store.delivered_in_session(surface, session)?,
            self.quiet()?,
            self.counted_today()?,
            self.daily_cap(),
        );
        if !ok {
            return Ok(None);
        }
        self.store.insert_delivery(card_id, surface, session, self.clock.now())?;
        info!(aiq.outcome = "delivered", aiq.surface = surface, "card delivered");
        let card: Option<CardV1> = serde_json::from_str(card_json).ok();
        if let (Some(c), Some(p)) = (&card, &self.popup)
            && POPUP_SURFACES.contains(&surface)
            && self.switch_on(SETTING_TIP_POPUP)
        {
            p(c);
        }
        Ok(card)
    }

    /// Copilot CLI status line: the tip line for a session, or None. Delivers a show-ready card only after
    /// the session's turn has ended (its last event is a stop). With no session id (the status line may
    /// not send one), the surface's newest session is used. Lifetime is derived from facts every call.
    pub fn tip_line(&mut self, surface: &str, session: Option<&str>) -> Option<TipLineOut> {
        let _s = info_span!("core.tip_line", aiq.surface = surface).entered();
        match self.tip_line_inner(surface, session) {
            Ok(t) => t,
            Err(e) => {
                self.fail(e);
                None
            }
        }
    }

    fn tip_line_inner(&mut self, surface: &str, session: Option<&str>) -> Result<Option<TipLineOut>, StoreError> {
        if self.paused() {
            return Ok(None);
        }
        let session = match session {
            Some(s) if !s.is_empty() && s.len() <= MAX_SESSION_ID => s.to_owned(),
            _ => match self.store.latest_session(surface)? {
                Some(s) => s,
                None => return Ok(None),
            },
        };
        self.expire_stale()?;
        if let Some((card_id, json, at)) = self.store.delivered_card(surface, &session)? {
            let now = self.clock.now().utc_ms;
            let after = self.store.prompts_after(surface, &session, at)?;
            let opened = self.store.opened(&card_id)?;
            let card: Option<CardV1> = serde_json::from_str(&json).ok();
            return Ok(match (tip_line_state(self.copilot_tip_stays(), at, after, opened, now), card) {
                (TipLine::Full, Some(c)) => Some(TipLineOut::Full(c)),
                (TipLine::Waiting, _) => Some(TipLineOut::Waiting(card_id)),
                _ => None,
            });
        }
        if self.store.last_event_kind(surface, &session)?.as_deref() != Some("stop") {
            return Ok(None); // mid-turn: a card never shows while the agent works
        }
        let Some(ready) = self.store.ready_card(surface, &session)? else { return Ok(None) };
        Ok(self.deliver(&ready.card_id, &ready.card_json, surface, &session)?.map(TipLineOut::Full))
    }

    /// A renderer that cannot name its session (the VS Code extension): deliver the oldest show-ready
    /// card of the surface whose session's turn has ended.
    pub fn take_card_any(&mut self, surface: &str) -> Option<CardV1> {
        let _s = info_span!("core.take_card_any", aiq.surface = surface).entered();
        let r = (|| -> Result<Option<CardV1>, StoreError> {
            self.expire_stale()?;
            for c in self.store.ready_cards_on(surface)? {
                if self.store.last_event_kind(surface, &c.session)?.as_deref() == Some("stop") {
                    return self.deliver(&c.card_id, &c.card_json, surface, &c.session);
                }
            }
            Ok(None)
        })();
        r.unwrap_or_else(|e| {
            self.fail(e);
            None
        })
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let v = match key {
            SETTING_PAUSED if value == "true" || value == "false" => value.to_owned(),
            k if SWITCH_SETTINGS.contains(&k) => match value {
                "on" | "off" => value.to_owned(),
                _ => return Err(format!("{} must be on or off", k.replace('_', "-"))),
            },
            SETTING_COPILOT_TIP_STAYS => match TipStays::parse(value) {
                Some(v) => v.as_str().to_owned(),
                None => {
                    let all: Vec<_> = TipStays::ALL.iter().map(|v| v.as_str()).collect();
                    return Err(format!("copilot-tip-stays must be one of: {}", all.join(", ")));
                }
            },
            crate::update::SETTING_UPDATE => crate::update::validate_mode(value)?,
            SETTING_DAILY_CAP => match value.parse::<u32>() {
                Ok(n) if n <= MAX_DAILY_CAP => n.to_string(),
                _ => return Err(format!("daily-cap must be a whole number from 0 to {MAX_DAILY_CAP}")),
            },
            k if k.starts_with(coaching::SETTING_KEEP_ON) => self.keep_on_value(k, value)?,
            k if k.starts_with(SETTING_PROJECT_NAME) => project_name(&k[SETTING_PROJECT_NAME.len()..], value)?,
            _ => match aiq_domain::rung::validate_setting(key, value)
                .or_else(|| coach::validate_setting(key, value))
                .or_else(|| aiq_domain::label::validate_setting(key, value))
            {
                Some(v) => v?,
                None => return Err(format!("unknown setting {key}")),
            },
        };
        self.store.set_setting(key, &v, self.clock.now().utc_ms).map_err(|e| {
            self.counters.inc(e.code);
            e.to_string()
        })
    }

    /// `keep_on.<job>` or `keep_on.<job>/<kind>` (design K6b): `on` keeps a paused job or kind's tips showing
    /// for the evidence it rests on now (stored as that newest check run); `auto` lets AIQ decide again.
    fn keep_on_value(&mut self, key: &str, value: &str) -> Result<String, String> {
        let scope = aiq_domain::check::Scope::parse(&key[coaching::SETTING_KEEP_ON.len()..])
            .ok_or_else(|| "name a job, or a job/kind of task".to_owned())?;
        match value {
            "auto" => Ok("auto".into()),
            "on" => {
                let readings = coaching::readings(&self.store).map_err(|e| e.code.to_string())?;
                aiq_domain::check::verdicts(&readings)
                    .into_iter()
                    .find(|v| v.scope == scope && v.paused)
                    .map(|v| v.newest_run)
                    .ok_or_else(|| format!("AIQ hasn't paused tips for {}", coaching::scope_label(&scope)))
            }
            _ => Err("keep-on must be on or auto".into()),
        }
    }

    /// Delete one prompt's text (Your data, "Delete"), then rebuild the detector working set so the text
    /// also leaves this process's memory (design aiq-home-deferred-pages E2).
    pub fn forget_item(&mut self, event_id: &str) -> Result<(), String> {
        let r = self
            .store
            .forget_content(event_id)
            .and_then(|found| if found { self.rebuild().map(|()| true) } else { Ok(false) });
        match r {
            Ok(true) => Ok(()),
            Ok(false) => Err("that item has no stored text to delete".into()),
            Err(e) => {
                self.counters.inc(e.code);
                Err(e.code.into())
            }
        }
    }

    pub fn status(&mut self) -> StatusReply {
        if let Err(e) = self.coach_cache() {
            self.fail(e);
        }
        StatusReply {
            paused: self.paused(),
            daily_cap: self.daily_cap(),
            cards_today: self.counted_today().unwrap_or(0),
            events_by_kind: self.store.events_by_kind().unwrap_or_default(),
            signals: self.store.signal_count().unwrap_or(0),
            decisions: self.store.decisions().unwrap_or_default(),
            store_bytes: self.store.size_bytes(),
            core_counters: self.counters.snapshot(),
            automated_folders: {
                let mut f = self.set_aside_folders();
                let mut rc: Vec<_> = self.projects_folders.iter().filter(|k| !f.iter().any(|(x, _)| x == *k)).collect();
                rc.sort();
                let rules = self.store.folder_rules_now().unwrap_or_default();
                f.extend(
                    rc.into_iter()
                        .filter(|k| rules.get(*k).map(String::as_str) != Some("include"))
                        .map(|k| (k.clone(), "projects".to_string())),
                );
                f
            },
            coach: self.coach.as_ref().map(|c| {
                let mode = coaching::mode(&self.store);
                CoachStatus {
                    tips_on: c.tips_on(&mode, &self.pack, self.labels_enabled()),
                    mode,
                    checked: c.check.checked,
                    right: c.check.right,
                    coached_types: c.rows.iter().filter(|r| r.state == coach::RowState::Coach).count() as u32,
                    other_pct: c.other_share.map(|x| (x * 100.0).round() as u32),
                    cache_ms: c.build_ms,
                    accuracy_paused: coaching::paused_scopes(&self.store, &c.pauses),
                    labels: coaching::label_status(&self.store, c, self.clock.now()).ok(),
                }
            }),
        }
    }

    /// The person's answer to a tip, from any surface.
    pub fn tip_feedback(
        &mut self,
        card_id: Option<&str>,
        tip_id: Option<&str>,
        answer: &str,
        surface: &str,
    ) -> Result<(), String> {
        coaching::record_feedback(&self.store, card_id, tip_id, answer, surface, self.clock.now().utc_ms).map(|_| ())
    }

    /// "This doesn't fit" or its undo.
    pub fn guidance_flag(
        &mut self,
        job: &str,
        task_type: &str,
        move_id: &str,
        action: &str,
        reason: Option<&str>,
        note: Option<&str>,
    ) -> Result<(), String> {
        let now = self.clock.now().utc_ms;
        coaching::record_flag(&self.store, &self.pack, job, task_type, move_id, action, reason, note, now)
    }

    /// Show a card's details and record that the person opened it (`opened_on`: cli / vscode).
    pub fn show(&mut self, card_id: &str, opened_on: &str) -> ShowReply {
        match self.store.card(card_id) {
            Ok(Some(r)) => {
                if let Err(e) = self.store.insert_open(card_id, opened_on, self.clock.now().utc_ms) {
                    self.fail(e);
                }
                let leaned_on = if r.detector_id == coach::DETECTOR {
                    let tasks = self.coach_cache().map(|c| c.tasks.clone()).unwrap_or_default();
                    r.evidence_ids
                        .iter()
                        .filter_map(|id| tasks.iter().find(|t| &t.task_id == id))
                        .map(|t| LeanedOn {
                            ts: format_rfc3339_ms(t.first_ms),
                            tool: t.tool.clone(),
                            task_type: t
                                .kind()
                                .and_then(|(j, ty)| aiq_contracts::judge::task_type_label(j, ty))
                                .unwrap_or("Other")
                                .to_owned(),
                        })
                        .collect()
                } else {
                    vec![]
                };
                ShowReply {
                    leaned_on,
                    card: serde_json::from_str(&r.card_json).ok(),
                    outcome: Some(r.outcome),
                    reason_code: Some(r.reason_code),
                    delivered: r.delivered,
                    evidence: r
                        .evidence
                        .into_iter()
                        .map(|(ts, text)| EvidencePrompt { ts: format_rfc3339_ms(ts), text })
                        .collect(),
                }
            }
            _ => ShowReply {
                card: None,
                outcome: None,
                reason_code: None,
                delivered: false,
                evidence: vec![],
                leaned_on: vec![],
            },
        }
    }

    pub fn prune(&mut self) {
        let now = self.clock.now().utc_ms;
        self.engine.prune(now);
    }
}

/// At most `max` bytes, cut on a character boundary.
fn cap(s: &str, max: usize) -> String {
    let mut i = s.len().min(max);
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    s[..i].to_owned()
}

#[cfg(test)]
mod project_name_tests {
    use super::*;

    #[test]
    fn project_names_are_short_one_line_and_blank_resets() {
        assert_eq!(project_name(r"c:\repos\aiq", "  AI coach ").unwrap(), "AI coach");
        assert_eq!(project_name(r"c:\repos\aiq", "   ").unwrap(), "");
        assert!(project_name(r"c:\repos\aiq", &"x".repeat(MAX_PROJECT_NAME + 1)).is_err());
        assert!(project_name(r"c:\repos\aiq", "two\nlines").is_err());
        assert!(project_name("", "name").is_err());
    }
}
