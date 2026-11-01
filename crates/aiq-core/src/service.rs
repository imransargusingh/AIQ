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
use aiq_domain::card::compose;
use aiq_domain::clock::{Clock, deliveries_counted_today};
use aiq_domain::job::{AlwaysUnclassified, JobClassifier};
use aiq_domain::reprompt::{Engine, PromptObs, SignalDraft};
use aiq_domain::text::features;
use aiq_domain::trigger::{DecideInput, Outcome, Reason, decide, may_deliver};
use aiq_store::{NewDecision, NewEvent, NewSignal, Store, StoreError};
use std::sync::Arc;
use tracing::{info, info_span, warn};

pub const SETTING_PAUSED: &str = "paused";
pub const SETTING_DAILY_CAP: &str = "daily_cap";
pub const MAX_DAILY_CAP: u32 = 50;

pub struct Service<C: Clock> {
    store: Store,
    engine: Engine,
    pack: PackV1,
    clock: C,
    classifier: Box<dyn JobClassifier + Send>,
    counters: Arc<Counters>,
}

fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

impl<C: Clock> Service<C> {
    /// Build the service and rebuild the detector working set from stored prompts.
    pub fn new(store: Store, pack: PackV1, clock: C, counters: Arc<Counters>) -> Result<Self, StoreError> {
        let mut s =
            Self { engine: Engine::new(&pack), store, pack, clock, classifier: Box::new(AlwaysUnclassified), counters };
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
        let now = self.clock.now().utc_ms;
        let ts = parse_rfc3339_ms(&ev.ts).unwrap_or(now);
        let (prompt_id, text, chars, meta) = match &ev.body {
            EventBody::Prompt(p) => {
                if p.text.is_none() {
                    self.counters.inc(codes::INGEST_SHAPE);
                }
                let f = p.text.as_deref().map(|t| features(t, &self.pack.reprompt));
                let meta = serde_json::json!({
                    "hook_event": "UserPromptSubmit",
                    "chars": p.chars,
                    "has_text": p.text.is_some(),
                    "truncated": p.truncated,
                    "file_mentions": f.as_ref().map_or(0, |f| f.mentions.len()),
                    "pasted": f.as_ref().is_some_and(|f| f.pasted),
                });
                (p.prompt_id.as_deref(), p.text.as_deref(), Some(p.chars as i64), meta)
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
        let ok = may_deliver(
            self.paused(),
            self.store.delivered_in_session(surface, session)?,
            self.counted_today()?,
            self.daily_cap(),
        );
        if !ok {
            return Ok(None);
        }
        self.store.insert_delivery(&ready.card_id, surface, session, self.clock.now())?;
        info!(aiq.outcome = "delivered", "card delivered");
        Ok(serde_json::from_str(&ready.card_json).ok())
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let v = match key {
            SETTING_PAUSED if value == "true" || value == "false" => value.to_owned(),
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
        }
    }

    pub fn show(&self, card_id: &str) -> ShowReply {
        match self.store.card(card_id) {
            Ok(Some(r)) => ShowReply {
                card: serde_json::from_str(&r.card_json).ok(),
                outcome: Some(r.outcome),
                reason_code: Some(r.reason_code),
                delivered: r.delivered,
                evidence: r
                    .evidence
                    .into_iter()
                    .map(|(ts, text)| EvidencePrompt { ts: format_rfc3339_ms(ts), text })
                    .collect(),
            },
            _ => ShowReply { card: None, outcome: None, reason_code: None, delivered: false, evidence: vec![] },
        }
    }

    pub fn prune(&mut self) {
        let now = self.clock.now().utc_ms;
        self.engine.prune(now);
    }
}
