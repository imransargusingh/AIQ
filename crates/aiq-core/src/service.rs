//! The single writer / decision loop's logic: ingest, detect, decide, take-card, expire.
//! Owned by exactly one thread, so the Coaching-session invariant is a sequential check; the unique
//! delivery index in the store backs it up.
use crate::counters::Counters;
use crate::sysclock::{format_rfc3339_ms, parse_rfc3339_ms};
use aiq_contracts::card::CardV1;
use aiq_contracts::codes;
use aiq_contracts::event::{EventBody, EventV1, MAX_SESSION_ID};
use aiq_contracts::frames::{EvidencePrompt, ShowReply, StatusReply};
use aiq_contracts::pack::PackV1;
use aiq_domain::automated::{Guard, Verdict, folder_key};
use aiq_domain::card::compose;
use aiq_domain::clock::{Clock, deliveries_counted_today};
use aiq_domain::job::{AlwaysUnclassified, JobClassifier};
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
pub const MAX_DAILY_CAP: u32 = 50;

pub struct Service<C: Clock> {
    store: Store,
    engine: Engine,
    pack: PackV1,
    clock: C,
    classifier: Box<dyn JobClassifier + Send>,
    counters: Arc<Counters>,
    guard: Guard,
}

/// What the Copilot CLI status line shows.
#[derive(Debug, Clone, PartialEq)]
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
        if self.automated(ev)? {
            self.counters.inc(codes::INGEST_AUTOMATED);
            return Ok(()); // an app or script drove this session: record nothing (import spec US-12)
        }
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
        if let Some(sig) = sig {
            self.on_signal(&ev.surface, &ev.surface_session_id, &sig, trace_id)?;
        }
        Ok(())
    }

    /// The automated-session rule, live side. True: record nothing for this event.
    fn automated(&mut self, ev: &EventV1) -> Result<bool, StoreError> {
        let (surface, session) = (ev.surface.as_str(), ev.surface_session_id.as_str());
        let EventBody::Prompt(p) = &ev.body else { return Ok(self.guard.is_ignored(surface, session)) };
        if !self.guard.is_ignored(surface, session) && self.store.session_known(surface, session)? {
            return Ok(false); // not a first prompt, and not a session being ignored
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
        let Verdict::Ignore { reason, new_folder } = verdict else { return Ok(false) };
        if let Some(folder) = new_folder {
            self.store.insert_folder_rule(&folder, "set_aside", reason.as_str(), "live", now)?;
            info!(aiq.reason = reason.as_str(), "folder set aside: automated sessions");
        }
        Ok(true)
    }

    /// Folders whose sessions AIQ ignores now, newest first: (folder, reason).
    pub fn set_aside_folders(&self) -> Vec<(String, String)> {
        self.store.set_aside_folders().unwrap_or_default().into_iter().map(|(f, r, _)| (f, r)).collect()
    }

    /// "Include them": the person says a set-aside folder is their own work. Returns false if it was not set aside.
    pub fn include_folder(&mut self, folder: &str) -> Result<bool, StoreError> {
        let k = folder_key(folder);
        if !self.store.set_aside_folders()?.iter().any(|(f, _, _)| *f == k) {
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
        let _d = info_span!("core.decide", trace_id = trace_id.unwrap_or("")).entered();
        let now = self.clock.now();
        self.expire_stale()?;
        let r = &self.pack.reprompt;
        let signal_id = new_id();
        self.store.insert_signal(
            &NewSignal {
                signal_id: &signal_id,
                task_id: &sig.task_id,
                surface,
                session,
                detector_id: &r.detector_id,
                detector_version: r.detector_version,
                pack_version: &self.pack.version,
                opportunity_id: &r.opportunity_id,
                reprompt_count: sig.reprompt_count,
                score: sig.score,
                evidence: &sig.evidence,
            },
            now.utc_ms,
        )?;
        let input = DecideInput {
            paused: self.paused(),
            job: self.classifier.classify(&sig.task_id),
            core_opportunity: r.core_opportunity,
            score: sig.score,
            threshold: self.pack.trigger.in_moment_threshold,
            delivered_in_session: self.store.delivered_in_session(surface, session)?,
            in_quiet_period: self.quiet()?,
            ready_in_session: self.store.ready_card(surface, session)?.is_some(),
            deliveries_counted_today: self.counted_today()?,
            daily_cap: self.daily_cap(),
        };
        let (outcome, reason) = decide(&input);
        let card_id = new_id();
        let tip = self.pack.tips.iter().find(|t| t.opportunity_id == r.opportunity_id).expect("pack validated");
        let card = compose(tip, sig.reprompt_count, &card_id, &self.pack.version);
        self.store.insert_decision(
            &NewDecision {
                decision_id: &new_id(),
                card_id: &card_id,
                signal_id: &signal_id,
                surface,
                session,
                outcome: outcome.as_str(),
                reason_code: reason.as_str(),
                rule_id: &self.pack.trigger.rule_id,
                pack_version: &self.pack.version,
                card_json: &serde_json::to_string(&card).expect("card serializes"),
            },
            now.utc_ms,
        )?;
        info!(
            aiq.outcome = outcome.as_str(),
            aiq.reason = reason.as_str(),
            aiq.reprompt_count = sig.reprompt_count,
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
        self.ingest(ev, trace_id);
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
        Ok(serde_json::from_str(card_json).ok())
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
            SETTING_COPILOT_TIP_STAYS => match TipStays::parse(value) {
                Some(v) => v.as_str().to_owned(),
                None => {
                    let all: Vec<_> = TipStays::ALL.iter().map(|v| v.as_str()).collect();
                    return Err(format!("copilot-tip-stays must be one of: {}", all.join(", ")));
                }
            },
            SETTING_DAILY_CAP => match value.parse::<u32>() {
                Ok(n) if n <= MAX_DAILY_CAP => n.to_string(),
                _ => return Err(format!("daily-cap must be a whole number from 0 to {MAX_DAILY_CAP}")),
            },
            _ => return Err(format!("unknown setting {key}")),
        };
        self.store.set_setting(key, &v, self.clock.now().utc_ms).map_err(|e| {
            self.counters.inc(e.code);
            e.to_string()
        })
    }

    pub fn status(&self) -> StatusReply {
        StatusReply {
            paused: self.paused(),
            daily_cap: self.daily_cap(),
            cards_today: self.counted_today().unwrap_or(0),
            events_by_kind: self.store.events_by_kind().unwrap_or_default(),
            signals: self.store.signal_count().unwrap_or(0),
            decisions: self.store.decisions().unwrap_or_default(),
            store_bytes: self.store.size_bytes(),
            core_counters: self.counters.snapshot(),
            automated_folders: self.set_aside_folders(),
        }
    }

    /// Show a card's details and record that the person opened it (`opened_on`: cli / vscode).
    pub fn show(&mut self, card_id: &str, opened_on: &str) -> ShowReply {
        match self.store.card(card_id) {
            Ok(Some(r)) => {
                if let Err(e) = self.store.insert_open(card_id, opened_on, self.clock.now().utc_ms) {
                    self.fail(e);
                }
                ShowReply {
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
            _ => ShowReply { card: None, outcome: None, reason_code: None, delivered: false, evidence: vec![] },
        }
    }

    pub fn prune(&mut self) {
        let now = self.clock.now().utc_ms;
        self.engine.prune(now);
    }
}
