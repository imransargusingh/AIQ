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
use aiq_domain::funnel::Step;
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
/// `on` (default) or `off`: show where the open task is heading (design live-rung-trend): the Right now card on
/// AIQ home and the trend in the Copilot CLI status line between tips. Imran 2026-11-03 "build C and D".
pub const SETTING_LIVE_TREND: &str = "live_trend";
pub const SWITCH_SETTINGS: [&str; 6] = [
    SETTING_COPILOT_TIP_IN_CHAT,
    SETTING_TIP_POPUP,
    SETTING_TIP_HIGHLIGHT,
    SETTING_COPILOT_MID_TURN,
    SETTING_AGENT_ACTIONS,
    SETTING_LIVE_TREND,
];
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
/// `on` (default) or `off`: at each turn's end, read the new part of the tool's own log and store what the agent
/// did as counts (`agent_actions`, schema v9 `turn_action`). Imran 2026-11-03 "Let's do it", after the downsides.
pub const SETTING_AGENT_ACTIONS: &str = "agent_actions";
const MAX_LOG_OFFSETS: usize = 4096;
/// Copilot CLI's surface name (the hook client uses the same constant).
pub const COPILOT_CLI: &str = "copilot-cli";
pub const VSCODE: &str = "vscode";
pub const CLAUDE_CODE: &str = "claude-code";
/// The VS Code extension asks for a card every 2 s while VS Code is focused. With no ask for this long, VS
/// Code tips show in the Copilot CLI status line instead (Imran, 2026-11-03: the extension was not
/// installed on the laptop and its tips never showed).
pub const VSCODE_FALLBACK_AFTER_MS: i64 = 10 * 60_000;

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
    /// Prompts of a Claude Code session that arrived while rule RC was unsettled (its transcript head not yet
    /// written), in memory only. Recorded in order at the session's next event that is not held (its Stop at
    /// the latest) unless the head then shows a Projects session; then dropped, so a Remote Control session never
    /// leaves its first prompt behind as the person's task (DC-035). At most `RC_TRIES` per session (each event
    /// is one read). Bounded like `held`.
    rc_held: std::collections::HashMap<(String, String), Vec<EventV1>>,
    /// Folder keys where a claude.ai Projects / Remote Control session was set aside (rule RC), so
    /// "Include them" can be offered there although no folder rule exists. In memory; bounded.
    projects_folders: std::collections::HashSet<String>,
    /// Rule RC result per Claude Code session, read from its transcript head until settled (bounded).
    rc_checked: std::collections::HashMap<String, RcCheck>,
    /// Rule RC per turn: whether the current turn of a Projects session is the person's own (its prompt was typed by
    /// them), so its Stop goes with it. In memory; bounded.
    rc_own: std::collections::HashMap<(String, String), bool>,
    /// Per surface, when `note_aside` last wrote and for which rule.
    aside_noted: std::collections::HashMap<String, AsideNoted>,
    /// Held prompts whose record never showed outside a Projects session: counted as the person's (`typed_gate`).
    rc_force_own: std::collections::HashSet<String>,
    /// Per session, the times of the transcript records its recent prompts were matched to (`projects::turn_at`).
    rc_matched: std::collections::HashMap<(String, String), Vec<i64>>,
    /// When `recheck_held` last re-read sessions whose head is unwritten (core clock).
    head_rechecked_ms: i64,
    /// Project messages already counted as a person's turn, by hash, oldest first (`DELIVERED_KEY`); None until read.
    rc_delivered: Option<std::collections::VecDeque<String>>,
    /// When held prompts older than `TURN_WAIT_MS` were last re-read (`recheck_held`).
    stale_rechecked_ms: i64,
    /// When the VS Code extension last asked for a card (core clock), in this core run.
    vscode_asked_ms: Option<i64>,
    /// Coaching inputs derived from the store (design coaching-from-rungs); rebuilt when stale.
    coach: Option<coaching::Cache>,
    /// AIQ-COACH-GUESS-LOW is logged once per core run.
    guess_low_logged: bool,
    /// The label self-check's hold is logged once per core run, as the words' switch-off is.
    label_low_logged: bool,
    popup: Option<Popup>,
    /// Tips handed to a turn-end hook that has not yet said it printed them (DC-022), by card id. In memory
    /// only: a core restart forgets them, and those tips are ready again. Bounded by the daily cap.
    handovers: std::collections::HashMap<String, Handover>,
    /// Starts a background label of a new task's first prompt (`aiq label`); None in tests and without a model.
    label_requester: Option<LabelRequester>,
    /// Tasks whose first prompt was sent to be labelled, until the label settles: where each stands on the one
    /// second try from a later prompt (design kind-label, "Second try"). In memory; bounded by `MAX_LABEL_WAIT`.
    label_wait: std::collections::HashMap<String, LabelWait>,
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
    /// The later prompt to label instead of the first (the one second try when the first label was not sure).
    pub prompt_id: Option<String>,
}

/// Where a labelled task stands on its second try (design kind-label, "Second try").
#[derive(Debug, Clone, PartialEq)]
enum LabelWait {
    /// The first prompt's label is not back yet; the newest fuller later prompt seen meanwhile, if any.
    Asked { later: Option<String> },
    /// The first label was not sure enough; the next fuller prompt of the task is labelled.
    Unsure,
    /// The second label was asked for; no third.
    Retried,
}

/// Tasks waiting on a label at once (each one short id). Cleared whole past this.
/// simplify: cleared, not evicted oldest-first. Ceiling: 1024 live tasks waiting at once. Upgrade trigger: a
/// laptop with more than ~100 new tasks a day (each settles within minutes, so the map rarely holds more than a few).
const MAX_LABEL_WAIT: usize = 1024;

/// A later prompt is full enough for a second label try with at least this many content words (`features`).
/// assume: 4 words separates a real ask ("fix the login redirect bug in auth.ts" = 5) from a reply ("yes", "ok go
/// ahead" = 1-2). Confirm: the funnel's `label_retry` rows and their outcomes on the laptop. If false: too low
/// spends labels on replies (bounded: one try per task, daily label budget); too high leaves short asks untried.
pub const RETRY_MIN_WORDS: usize = 4;

pub type LabelRequester = Box<dyn Fn(LabelRequest) + Send>;

/// A task's kind, from a word guess or a label, with what it rests on.
struct Kind<'a> {
    job: &'a str,
    /// None: the kind is not sure, so the job's own row coaches (`coach::job_rows`).
    task_type: Option<&'a str>,
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

/// One turn's read of the tool's log: the actions, or why they are not recorded; and what the read cost.
struct TurnRead {
    acts: Option<crate::agent_actions::Actions>,
    why: Option<&'static str>,
    bytes: u64,
    ms: u64,
}

/// First prompts held back at once (each a few KB at most, in memory).
const MAX_HELD: usize = 1024;
/// A turn-end hook that took a tip confirms it printed it within this long, or the tip is ready again for the
/// next turn end (DC-022). The confirmation normally lands within the hook's 20 ms send.
/// assume: 5 s covers a hook delayed between print and confirm on a loaded machine (measured: hook runs took at
/// most ~0.12 s on a loaded 4-core runner, 2026-11-03). If false: a printed tip whose confirmation comes later is
/// still recorded (`card_shown` accepts a late one) but may also show once more at the next turn end.
pub const HANDOVER_CONFIRM_MS: i64 = 5_000;

/// A tip handed to a turn-end hook, waiting for the hook to say it printed it.
struct Handover {
    at_ms: i64,
    surface: String,
    session: String,
    card_json: String,
}

/// A show-ready card whose turn end is never seen expires this long after its decision.
/// assume: a turn ends within 4 h (longest measured on the laptop: 42 min, 2026-11-02). Confirm: no
/// `expired` card in `card_decision` whose session's next stop came after 4 h. If false: that long turn's
/// tip is dropped (no wrong tip shown).
const READY_TURN_MAX_MS: i64 = 4 * 3_600_000;

/// Rule RC per session: a settled answer, or how many reads found the head not yet written.
#[derive(Debug, Clone, Copy)]
enum RcCheck {
    Settled(bool),
    Unsettled(u32),
}

/// Reads of an unsettled transcript head per session before AIQ stops looking (a missing file reads in
/// 0 ms; a written head settles on its first user record, so this bound is rarely reached).
const RC_TRIES: u32 = 8;

/// What `note_aside` last wrote for one surface, and today's count so far.
struct AsideNoted {
    at: i64,
    reason: String,
    day: i64,
    n: u32,
}

/// An `aside.<surface>` value: (utc_ms, reason, local day, prompts set aside that day). Values written before the
/// count existed have only the first two. Malformed parts read as None / 0, never as a guess.
pub fn parse_aside(v: &str) -> (Option<i64>, String, Option<i64>, u32) {
    let mut it = v.split(' ');
    let ms = it.next().and_then(|x| x.parse().ok());
    let reason = it.next().unwrap_or_default().to_owned();
    let day = it.next().and_then(|x| x.parse().ok());
    let n = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    (ms, reason, day, n)
}

/// Each surface's `aside.<surface>` value with today's counted prompts, for Home and `aiq status`. A malformed value
/// is left out, never guessed.
pub fn aside_now(
    store: &Store,
    now: aiq_domain::clock::Now,
) -> Result<Vec<aiq_contracts::frames::AsideNow>, StoreError> {
    if !store.has_table("setting_change")? {
        return Ok(vec![]);
    }
    let counted = store.prompts_by_surface_since(now.local_midnight_utc_ms())?;
    Ok(store
        .settings_with_prefix(ASIDE_KEY)?
        .into_iter()
        .filter_map(|(k, v)| {
            let (ms, reason, day, n) = parse_aside(&v);
            let surface = k[ASIDE_KEY.len()..].to_owned();
            Some(aiq_contracts::frames::AsideNow {
                counted_today: counted.get(&surface).copied().unwrap_or(0),
                surface,
                last_ms: ms?,
                reason: Some(reason).filter(|r| !r.is_empty())?,
                left_out_today: if day == Some(now.local_day()) { n } else { 0 },
            })
        })
        .collect())
}

/// Setting key prefix for a surface's newest set-aside activity (`note_aside`); Home reads it.
pub const ASIDE_KEY: &str = "aside.";
/// A held prompt outside a Projects session whose record has not shown by then counts as the person's.
const TURN_WAIT_MS: i64 = 10_000;
/// How often held prompts' records are re-read while any wait (`held_wait`).
const RECHECK_MS: u64 = 300;
/// How often a held prompt whose session head is unwritten is re-read (`recheck_held`).
const HEAD_RECHECK_MS: i64 = 1_000;
/// How often a held prompt older than `TURN_WAIT_MS` is re-read. Laptop 2026-11-09 19:07Z: a relayed brief with no
/// record of its own was re-read every ~316 ms for 12 minutes until its Stop (2,256 reads); a record is written at most
/// 2.1 s after the hook (measured 2026-11-08), so after 10 s a slow re-read loses nothing but a few seconds.
const STALE_RECHECK_MS: i64 = 5_000;
/// Setting key: hashes of the project messages already counted as a person's turn (`rc_redelivered`).
const DELIVERED_KEY: &str = "rc.delivered";
/// The newest milestone already said once (its `earned_ms`), so each milestone gets one note (Milestones design,
/// Imran 2026-11-10 "Build as mocked").
const MILESTONE_SEEN_KEY: &str = "milestone.seen_ms";
/// Where the current milestone note was said ({surface, session, at_ms, text}): that session keeps it on its
/// status line until its next prompt. "" when none.
const MILESTONE_NOTE_KEY: &str = "milestone.note";

/// The one line a milestone note reads, on every surface.
pub fn milestone_note_text(words: &str) -> String {
    format!("AIQ milestone: {words} · see it in AIQ Home")
}

/// A milestone note claimed for one place, with the seen mark it replaced so a note that never reached the person
/// can be put back (`unclaim_note`).
#[derive(Debug, Clone, PartialEq)]
pub struct NoteClaim {
    pub text: String,
    pub prev_seen: Option<String>,
}
/// How many hashes `DELIVERED_KEY` keeps (236 person records in 848 transcripts, measured 2026-11-09).
const DELIVERED_KEEP: usize = 1_024;
/// How often `note_aside` rewrites an unchanged surface's time.
const ASIDE_EVERY_MS: i64 = 5 * 60_000;

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
    /// No tip to show: where the open task is heading (design live-rung-trend D), plain text.
    Trend(String),
    /// No tip to show and a milestone just earned: its one note (`milestone_note_text`), plain text.
    Milestone(String),
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
            rc_held: Default::default(),
            projects_folders: Default::default(),
            rc_checked: Default::default(),
            rc_own: Default::default(),
            aside_noted: Default::default(),
            rc_force_own: Default::default(),
            rc_matched: Default::default(),
            head_rechecked_ms: 0,
            stale_rechecked_ms: 0,
            rc_delivered: None,
            vscode_asked_ms: None,
            coach: None,
            guess_low_logged: false,
            label_low_logged: false,
            popup: None,
            handovers: std::collections::HashMap::new(),
            label_requester: None,
            label_wait: Default::default(),
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
        let turn = self.read_turn_log(ev, trace_id);
        self.ingest_event(ev, trace_id);
        self.record_turn_actions(ev, turn, trace_id);
    }

    /// The log a live session's turns are read from, and its kind. None: this surface has no log AIQ reads (or
    /// Copilot's home is unknown); `Some((kind, None))`: the hook sent no transcript path.
    fn turn_log(&self, ev: &EventV1) -> Option<(crate::agent_actions::LogKind, Option<std::path::PathBuf>)> {
        use crate::agent_actions::LogKind;
        match ev.surface.as_str() {
            "claude-code" => Some((LogKind::Claude, ev.transcript.as_deref().map(std::path::PathBuf::from))),
            // Copilot CLI and VS Code agent mode (Copilot target) write the same session log.
            COPILOT_CLI | "vscode" => Some((
                LogKind::Copilot,
                Some(crate::copilot_log::session_log(self.copilot_home.as_deref()?, &ev.surface_session_id)?),
            )),
            _ => None,
        }
    }

    /// The work after a stop's reply is sent: Copilot messages typed while the agent worked, and what the agent
    /// did in the turn.
    pub fn after_stop(&mut self, ev: &EventV1, trace_id: Option<&str>) {
        let turn = self.read_turn_log(ev, trace_id);
        self.record_turn_actions(ev, turn, trace_id);
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
                if reason != "rc-pending" {
                    self.counters.inc(codes::INGEST_AUTOMATED); // a held event is not set aside yet
                }
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
                if reason != "rc-pending" {
                    self.note_aside(&ev.surface, reason, matches!(ev.body, EventBody::Prompt(_)));
                }
                // Home counts set-aside sessions per folder (one row per session, first prompt's time). A failed write
                // only leaves the count short; it never stops the skip.
                if let (EventBody::Prompt(_), "aiq-check" | "folder" | "template" | "burst", Some(folder)) =
                    (&ev.body, reason, ev.cwd.as_deref().map(folder_key).filter(|k| !k.is_empty()))
                {
                    let now = self.clock.now().utc_ms;
                    if let Err(e) =
                        self.store.insert_set_aside_session(&ev.surface, &ev.surface_session_id, &folder, reason, now)
                    {
                        warn!(error.code = e.code, "set-aside session not noted");
                    }
                }
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
            if new_task {
                self.tip_step(&ev.event_id, &ev.surface, Step::LateFirstPrompt);
            }
            return Ok(());
        }
        if let Some(sig) = sig {
            self.on_signal(&ev.surface, &ev.surface_session_id, &sig, trace_id)?;
        }
        if new_task {
            // A live first prompt of a new task (only live events reach here: imports never raise a card, D11).
            self.next_rung(&ev.surface, &ev.surface_session_id, &ev.event_id, text, trace_id)?;
        } else {
            self.later_prompt(&ev.surface, &ev.surface_session_id, &ev.event_id, text);
        }
        Ok(())
    }

    /// A later prompt of a task whose first prompt was sent to be labelled: when the first label was not sure
    /// enough (a "hi", or an ask too short to name), label this fuller prompt instead, once per task. If the first
    /// label is still on its way, remember the prompt for when it comes back.
    fn later_prompt(&mut self, surface: &str, session: &str, event_id: &str, text: &str) {
        let Some(task) = self.engine.task_id(surface, session).map(str::to_owned) else { return };
        let Some(wait) = self.label_wait.get(&task) else { return };
        if features(&aiq_domain::text::own_words(text), &self.pack.reprompt).words.len() < RETRY_MIN_WORDS {
            return;
        }
        match wait {
            LabelWait::Asked { .. } => {
                self.label_wait.insert(task, LabelWait::Asked { later: Some(event_id.to_owned()) });
            }
            LabelWait::Unsure => self.retry_label(surface, session, &task, event_id),
            LabelWait::Retried => {}
        }
    }

    /// Ask for the task's second and last label, from `prompt_id`.
    fn retry_label(&mut self, surface: &str, session: &str, task_id: &str, prompt_id: &str) {
        let ok = coaching::mode(&self.store) != "off" && !self.paused() && self.labels_enabled();
        if !ok || !self.request_label(surface, session, task_id, Some(prompt_id)) {
            self.label_wait.remove(task_id);
            return;
        }
        self.label_wait.insert(task_id.to_owned(), LabelWait::Retried);
        self.tip_step(task_id, surface, Step::LabelRetry);
        info!(coach.outcome = "label_retry_requested", "coach.label");
    }

    /// At a stop, read the new part of the tool's own log, read-only and off the hook's clock (the hook never
    /// opens it: ~0.4 s on Imran's laptop, measured 2026-11-02). Copilot messages typed while the agent was working
    /// fire no hook, so each is ingested as a prompt here (stable event ids and a same-text check keep a message
    /// the hook did send from counting twice). The turn's actions are returned for `record_turn_actions`, which
    /// stores them once the stop itself is stored. None: nothing to read (not a stop, paused, both switches off).
    fn read_turn_log(&mut self, ev: &EventV1, trace_id: Option<&str>) -> Option<TurnRead> {
        if !matches!(ev.body, EventBody::Stop(_)) || self.paused() {
            return None;
        }
        use crate::agent_actions::{LogKind, read_back_to_prompt, read_since};
        let (kind, path) = self.turn_log(ev)?;
        let mid_turn = kind == LogKind::Copilot && self.switch_on(SETTING_COPILOT_MID_TURN);
        let actions = self.switch_on(SETTING_AGENT_ACTIONS);
        if !mid_turn && !actions {
            return None;
        }
        let not_recorded = |why: &'static str, bytes: u64, ms: u64| {
            actions.then_some(TurnRead { acts: None, why: Some(why), bytes, ms })
        };
        let Some(path) = path else { return not_recorded("no_log", 0, 0) }; // an older hook sent no transcript
        let key = format!("{}/{}", ev.surface, ev.surface_session_id);
        let known = self.log_offsets.get(&key).copied();
        let started = std::time::Instant::now();
        let read = match known {
            Some(from) => read_since(&path, from, kind),
            None => read_back_to_prompt(&path, kind),
        };
        let ms = started.elapsed().as_millis() as u64;
        let read = match read {
            Ok(r) => r,
            Err(e) => {
                self.counters.inc(codes::INGEST_LOG_READ);
                let why = if e.kind() == std::io::ErrorKind::NotFound { "no_log" } else { "read_error" };
                return not_recorded(why, 0, ms);
            }
        };
        if self.log_offsets.len() >= MAX_LOG_OFFSETS {
            self.log_offsets.clear();
        }
        self.log_offsets.insert(key, read.next);
        if mid_turn {
            self.store_mid_turn(ev, &read.steering, trace_id);
        }
        if !actions {
            return None;
        }
        // Never a guess: a turn read only in part is "not recorded".
        let why = if read.capped {
            Some("too_large")
        } else if read.bad_lines > 0 {
            Some("unparsed")
        } else if known.is_none() && !read.saw_prompt {
            Some("no_start") // first read since the core started, and the turn began before the part read
        } else {
            None
        };
        Some(TurnRead { acts: why.is_none().then_some(read.actions), why, bytes: read.bytes, ms })
    }

    /// Ingest Copilot messages typed into the running turn, read from its log.
    fn store_mid_turn(&mut self, ev: &EventV1, found: &[crate::copilot_log::Typed], trace_id: Option<&str>) {
        let mut stored = 0;
        for t in found {
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
        info!(trace_id = trace_id.unwrap_or(""), found = found.len(), stored, "copilot.mid_turn");
    }

    /// Store the turn's actions as one `turn_action` row, keyed by its stop: counts and tool names only. Only for
    /// a stop the store kept (a set-aside or paused session records nothing). A replayed stop stores nothing.
    fn record_turn_actions(&mut self, ev: &EventV1, turn: Option<TurnRead>, trace_id: Option<&str>) {
        let Some(t) = turn else { return };
        if !self.store.has_event(&ev.event_id).unwrap_or(false) {
            return;
        }
        let now = self.clock.now().utc_ms;
        let n = |x: u32| Some(i64::from(x));
        let a = t.acts.as_ref();
        let row = aiq_store::TurnActionRow {
            stop_event_id: ev.event_id.clone(),
            surface: ev.surface.clone(),
            surface_session_id: ev.surface_session_id.clone(),
            ts_utc_ms: parse_rfc3339_ms(&ev.ts).unwrap_or(now),
            outcome: if a.is_some() { "recorded" } else { "not_recorded" }.into(),
            why: t.why.map(str::to_owned),
            tool_calls: a.and_then(|a| n(a.tool_calls())),
            commands: a.and_then(|a| n(a.commands_run)),
            files: a.and_then(|a| n(a.files.len() as u32)),
            subagents: a.and_then(|a| n(a.subagents)),
            tools: a.map(|a| serde_json::to_string(&a.tools).unwrap_or_default()),
            bytes_read: t.bytes as i64,
            read_ms: t.ms as i64,
            utc_ms: now,
        };
        match self.store.insert_turn_action(&row) {
            Ok(true) => {
                if t.why.is_some() {
                    self.counters.inc(codes::INGEST_ACTIONS_NOT_RECORDED);
                }
                info!(
                    trace_id = trace_id.unwrap_or(""),
                    aiq.surface = ev.surface.as_str(),
                    outcome = row.outcome.as_str(),
                    why = t.why.unwrap_or(""),
                    tool_calls = row.tool_calls.unwrap_or(0),
                    bytes = t.bytes,
                    ms = t.ms,
                    "agent.actions"
                );
            }
            Ok(false) => {}
            Err(e) => self.fail(e),
        }
    }

    /// Live agent actions today, for `aiq status`: counted from stored rows.
    fn actions_status(&self) -> aiq_contracts::frames::ActionsStatus {
        let rows = self.store.turn_actions_since(self.clock.now().local_midnight_utc_ms()).unwrap_or_default();
        let mut ms: Vec<u64> = rows.iter().map(|r| r.read_ms.max(0) as u64).collect();
        ms.sort_unstable();
        let mut why = std::collections::BTreeMap::<String, u32>::new();
        for r in rows.iter().filter(|r| r.outcome != "recorded") {
            *why.entry(r.why.clone().unwrap_or_default()).or_default() += 1;
        }
        aiq_contracts::frames::ActionsStatus {
            enabled: self.switch_on(SETTING_AGENT_ACTIONS),
            turns: rows.len() as u32,
            not_recorded: why.values().sum(),
            why: why.into_iter().collect(),
            p95_ms: (!ms.is_empty()).then(|| ms[((ms.len() * 95).div_ceil(100)).saturating_sub(1)]),
            max_ms: ms.last().copied(),
            max_bytes: rows.iter().map(|r| r.bytes_read.max(0) as u64).max(),
        }
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
        if mode == "off" {
            self.tip_step(task_id, surface, Step::TipsOff);
            return Ok(());
        }
        if self.paused() {
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
        // A self-check that holds tips never stops its own measuring (DC-036): word guesses are still made and
        // stored, and labels still asked for, so each check keeps scoring and turns its tips back on. Before,
        // a check below 8 in 10 stopped the guessing, so it could never recover (laptop 2026-11-04 to 11-08:
        // labels 6 of 10 right, then no label and no tip on any surface for four days).
        let words_on = cache.words_on(mode, pack);
        let labels_on = self.labels_enabled();
        if labels_on && !cache.labels_on(mode, pack) {
            self.log_label_hold(cache);
        }
        let words = features(&aiq_domain::text::own_words(text), &pack.reprompt).words;
        let guess = coach::guess_type(&words, &cache.neighbours, &pack.coach);
        if !words_on {
            if let Some(g) = &guess {
                self.store.insert_type_guess(&aiq_store::TypeGuessRow {
                    guess_id: new_id(),
                    task_id: task_id.to_owned(),
                    surface: surface.to_owned(),
                    job: g.job.clone(),
                    task_type: g.task_type.clone(),
                    similarity: g.similarity,
                    neighbours: g.neighbours.clone(),
                    utc_ms: self.clock.now().utc_ms,
                    method: "words".into(),
                })?;
            }
            if !self.guess_low_logged {
                self.guess_low_logged = true;
                self.counters.inc(codes::COACH_GUESS_LOW);
                warn!(
                    error.code = codes::COACH_GUESS_LOW,
                    checked = cache.check.checked,
                    right = cache.check.right,
                    label_checked = cache.label_check.checked,
                    label_right = cache.label_check.right,
                    "next-step tips from word guesses held by AIQ's self-check; word guesses are still made and checked"
                );
            }
        }
        let held_guess = !words_on && guess.is_some();
        let Some(g) = guess.filter(|_| words_on) else {
            // No word match: ask the small model in the background (design kind-label). The core never waits on
            // a model (C9); the answer comes back as `on_kind_label`, before the turn's Stop in most turns.
            let asked = labels_on && self.request_label(surface, session, task_id, None);
            let step = match (asked, held_guess) {
                (true, _) => Step::LabelAsked,
                (false, true) => Step::GuessHeld,
                (false, false) => Step::NoMatch,
            };
            self.tip_step(task_id, surface, step);
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
        let kind = Kind { job: &g.job, task_type: Some(&g.task_type), score: g.similarity, evidence: &g.neighbours };
        self.decide_kind(cache, pack, mode, surface, session, task_id, kind, "words", trace_id, started)
    }

    /// Log once per core run that the label self-check holds label tips (it was silent before).
    fn log_label_hold(&mut self, cache: &coaching::Cache) {
        if self.label_low_logged {
            return;
        }
        self.label_low_logged = true;
        self.counters.inc(codes::COACH_GUESS_LOW);
        warn!(
            error.code = codes::COACH_GUESS_LOW,
            label_checked = cache.label_check.checked,
            label_right = cache.label_check.right,
            "next-step tips from kind labels held by AIQ's self-check; labels are still asked for and checked"
        );
    }

    /// Record where this task's tip path ended or began to wait (design tip-funnel). A failed write only leaves
    /// the funnel short; it never stops a tip.
    fn tip_step(&self, task_id: &str, surface: &str, step: Step) {
        if let Err(e) = self.store.insert_tip_step(task_id, surface, step.as_str(), self.clock.now()) {
            self.counters.inc(e.code);
            warn!(error.code = e.code, step = step.as_str(), "tip step not noted");
        }
    }

    /// Labels can run: a labeller is wired, background judging is on and labels are not turned off.
    fn labels_enabled(&self) -> bool {
        self.label_requester.is_some() && coaching::labels_enabled(&self.store)
    }

    /// Ask for a label of this new task's first prompt, or of `prompt_id` on the second try (labels are
    /// enabled). True when asked.
    fn request_label(&mut self, surface: &str, session: &str, task_id: &str, prompt_id: Option<&str>) -> bool {
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
            prompt_id: prompt_id.map(str::to_owned),
        });
        if prompt_id.is_none() {
            if self.label_wait.len() >= MAX_LABEL_WAIT {
                self.label_wait.clear();
            }
            self.label_wait.insert(task_id.to_owned(), LabelWait::Asked { later: None });
        }
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
        let labels = self.store.kind_labels_of(task_id)?;
        let retried = self.label_wait.get(task_id) == Some(&LabelWait::Retried);
        // On the second try only the newest label counts: a failed retry is a failure, not the first unsure one.
        let ok = |l: &&aiq_store::KindLabelRow| l.outcome == "ok";
        let usable = if retried { labels.last().filter(ok) } else { labels.iter().rev().find(ok) };
        let Some(l) = usable.cloned() else {
            if let Some(l) = labels.last() {
                self.label_wait.remove(task_id);
                self.tip_step(task_id, &l.surface, Step::LabelFailed);
            }
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
            self.label_wait.remove(task_id);
            self.tip_step(task_id, &l.surface, Step::LabelLate);
            return Ok(());
        }
        if self.store.has_type_guess(task_id, aiq_domain::label::METHOD)? {
            self.label_wait.remove(task_id);
            return Ok(()); // already decided (a repeated notice)
        }
        let label = aiq_domain::label::Label {
            job: l.job.clone().unwrap_or_default(),
            job_confidence: l.job_confidence.unwrap_or(0.0),
            task_type: l.task_type.clone().unwrap_or_default(),
            task_type_confidence: l.task_type_confidence.unwrap_or(0.0),
        };
        let pack = self.pack.clone();
        self.coach_cache()?;
        let cache = self.coach.take().expect("built above");
        let r = self.decide_label(&cache, &pack, &mode, task_id, &l, &label, &outcome, started);
        self.coach = Some(cache);
        r
    }

    /// The label's half of `on_kind_label`, with the coaching cache in hand: the job's own bar and earned rule
    /// (`label::job_bar`, `label::earned`), then the decision by job, worded by the kind when that is sure.
    #[allow(clippy::too_many_arguments)]
    fn decide_label(
        &mut self,
        cache: &coaching::Cache,
        pack: &aiq_contracts::pack::PackV1,
        mode: &str,
        task_id: &str,
        l: &aiq_store::KindLabelRow,
        label: &aiq_domain::label::Label,
        outcome: &dyn Fn(&str),
        started: std::time::Instant,
    ) -> Result<(), StoreError> {
        let (bar, earned) = cache.label_job(&label.job);
        let Some((job, task_type)) = aiq_domain::label::accept(label, bar) else {
            outcome("label_not_confident");
            self.tip_step(task_id, &l.surface, Step::LabelUnsure);
            // Not sure: the task's next fuller prompt gets the one second try (now, if one came meanwhile).
            match self.label_wait.remove(task_id) {
                Some(LabelWait::Asked { later: Some(p) }) => {
                    self.retry_label(&l.surface, &l.surface_session_id, task_id, &p)
                }
                Some(LabelWait::Asked { later: None }) => {
                    self.label_wait.insert(task_id.to_owned(), LabelWait::Unsure);
                }
                _ => {}
            }
            return Ok(());
        };
        self.label_wait.remove(task_id);
        if !earned {
            // Sure of a job whose labels have not earned tips (questions, measured mostly wrong): no guess is
            // stored, so it never counts against the check, as before.
            outcome("label_job_not_coached");
            self.tip_step(task_id, &l.surface, Step::LabelNotCoached);
            return Ok(());
        }
        let score = label.job_confidence;
        let stored_type = task_type.unwrap_or(coach::ANY_KIND);
        self.store.insert_type_guess(&aiq_store::TypeGuessRow {
            guess_id: new_id(),
            task_id: task_id.to_owned(),
            surface: l.surface.clone(),
            job: job.to_owned(),
            task_type: stored_type.to_owned(),
            similarity: score,
            neighbours: vec![],
            utc_ms: self.clock.now().utc_ms,
            method: aiq_domain::label::METHOD.into(),
        })?;
        // Stored either way, so the self-check sees it once the task is analyzed; no tip while it holds.
        if !cache.labels_on(mode, pack) {
            self.log_label_hold(cache);
            outcome("label_held_by_self_check");
            self.tip_step(task_id, &l.surface, Step::LabelHeld);
            return Ok(());
        }
        let evidence = [format!("label:{}", l.model.as_deref().unwrap_or(&l.provider))];
        let kind = Kind { job, task_type, score, evidence: &evidence };
        self.decide_kind(cache, pack, mode, &l.surface, &l.surface_session_id, task_id, kind, "label", None, started)
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
        // The kind's own row when the kind is sure; the job's row when it is not, or when the kind has too few
        // tasks to say (TooFew, NotSure). A kind at its right rung, blocked or flagged is never overruled by its job.
        let kind_row = g.task_type.and_then(|t| cache.rows.iter().find(|r| r.job == g.job && r.task_type == t));
        let use_job = kind_row.is_none_or(|r| matches!(r.state, coach::RowState::TooFew | coach::RowState::NotSure));
        let job_row = || cache.job_rows.iter().find(|r| r.job == g.job);
        let chosen = if use_job { job_row() } else { kind_row };
        let row = chosen.filter(|r| r.state == coach::RowState::Coach);
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
            self.tip_step(task_id, surface, Step::for_kind(chosen.map(|r| r.state)));
            return Ok(());
        };
        let tip = coach::tip_id(&m.move_id);
        let cooldown_ms = pack.coach.move_cooldown_h as i64 * 3_600_000;
        let accuracy = cache.paused_for(&self.store, mode, &row.job, g.task_type.unwrap_or(&row.task_type)).map(|v| {
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
        // The tip names the kind when it is sure, even when the job's row picked the habit.
        let card = compose_move(m, &row.job, g.task_type.unwrap_or(&row.task_type), &new_id(), &pack.version);
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
        self.tip_step(task_id, surface, Step::Decided);
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

    /// A prompt whose session rule RC has read without an answer (head not yet written), with reads left. Only
    /// prompts wait: by a Stop Claude Code has written the head, so a Stop that still finds none (a missing or
    /// unreadable transcript) releases what waits rather than holding a whole session back.
    fn rc_unsettled(&self, ev: &EventV1) -> bool {
        let key = format!("{}/{}", ev.surface, ev.surface_session_id);
        matches!(ev.body, EventBody::Prompt(_))
            && ev.transcript.is_some()
            && matches!(self.rc_checked.get(&key), Some(RcCheck::Unsettled(n)) if *n < RC_TRIES)
    }

    /// Home's Right now card says when a tool's newest activity was left out as automated (Imran 2026-11-08: the card
    /// read "Nothing in progress" and showed Claude Code idle for a day while every one of his turns was set aside),
    /// and warns when most of a tool's prompts today were (Imran 2026-11-09). A setting per surface,
    /// `aside.<surface>` = "<utc_ms> <reason> <local day> <prompts set aside that day>".
    /// simplify: written at most every `ASIDE_EVERY_MS` per surface unless the reason or the day changes, so the card's
    /// time and count are up to 5 minutes stale. Ceiling: the setting log grows ~12 rows an hour per busy surface.
    /// Upgrade trigger: the setting_change table passes 100k rows, or a person asks for the exact time.
    fn note_aside(&mut self, surface: &str, reason: &str, prompt: bool) {
        let now = self.clock.now();
        let day = now.local_day();
        let key = format!("{ASIDE_KEY}{surface}");
        if !self.aside_noted.contains_key(surface) {
            if self.aside_noted.len() >= 64 {
                return;
            }
            // After a restart, today's count goes on from what was written.
            let stored = self.store.setting(&key).ok().flatten().map(|v| parse_aside(&v)).unwrap_or_default();
            let n = if stored.2 == Some(day) { stored.3 } else { 0 };
            self.aside_noted.insert(surface.to_owned(), AsideNoted { at: i64::MIN, reason: String::new(), day, n });
        }
        let Some(a) = self.aside_noted.get_mut(surface) else { return };
        if a.day != day {
            (a.day, a.n) = (day, 0);
            a.at = i64::MIN; // a new day is written at once
        }
        a.n += u32::from(prompt);
        // Saturating: `at` is i64::MIN until the first write and after a new day. A plain subtraction wrapped in the
        // release build, so after local midnight nothing was noted until the core restarted (laptop 2026-11-11:
        // no row from 05:28Z to 22:24Z though turns were set aside at 16:16Z; the card said "Yesterday 21:28").
        if a.reason == reason && now.utc_ms.saturating_sub(a.at) < ASIDE_EVERY_MS {
            return;
        }
        let value = format!("{} {reason} {day} {}", now.utc_ms, a.n);
        (a.at, a.reason) = (now.utc_ms, reason.to_owned());
        if let Err(e) = self.store.set_setting(&key, &value, now.utc_ms) {
            warn!(error.code = e.code, "set-aside activity not noted");
        }
    }

    /// The person said "count this folder": every turn there counts, as before rule RC.
    fn rc_included(&self, ev: &EventV1) -> Result<bool, StoreError> {
        let k = ev.cwd.as_deref().map(folder_key).unwrap_or_default();
        Ok(self.store.folder_rules_now()?.get(&k).map(String::as_str) == Some("include"))
    }

    /// The per-turn rule. `None`: the person's turn, on to the other rules. `Some(skip)`: not typed by a person, or
    /// waiting for its record. Prompts held earlier are decided first, in order. In a Projects session (`marker`) a
    /// turn with no sign of a person does not count, and one whose record is still missing at its Stop is dropped.
    /// Elsewhere a record with no `turnOrigin` (older Claude Code) counts, and a missing record counts at the Stop or
    /// after `TURN_WAIT_MS`, as before this rule: a person's turn is never lost to a slow write.
    fn typed_gate(&mut self, ev: &EventV1, marker: bool) -> Result<Option<Gate>, StoreError> {
        use crate::projects::Turn;
        let key = (ev.surface.clone(), ev.surface_session_id.clone());
        let stop = matches!(ev.body, EventBody::Stop(_));
        let now = self.clock.now().utc_ms;
        let mut again = vec![];
        for e in self.rc_held.remove(&key).unwrap_or_default() {
            let aged = parse_rfc3339_ms(&e.ts).is_some_and(|t| now - t >= TURN_WAIT_MS);
            match self.rc_prompt_turn(&e, marker) {
                Turn::Own => {
                    self.rc_force_own.insert(e.event_id.clone()); // decided: not read a second time on the way in
                    self.ingest_inner(&e, None)?;
                }
                Turn::Unknown if !stop && (marker || !aged) => again.push(e),
                Turn::Unknown if !marker => {
                    self.rc_force_own.insert(e.event_id.clone());
                    self.ingest_inner(&e, None)?;
                }
                _ => {
                    // Decided as not the person's at its Stop (a relayed brief's record often lands only then): left
                    // out like any other, so the card's time and today's count move (laptop 2026-11-11 22:22Z: the
                    // brief that started a Remote Control session was dropped here and the card kept yesterday's time).
                    self.rc_own.insert(key.clone(), false);
                    let reason = if marker { "projects" } else { "not-typed" };
                    info!(
                        aiq.reason = reason,
                        aiq.surface = e.surface.as_str(),
                        aiq.kind = e.kind(),
                        aiq.dir = e.cwd.as_deref().and_then(aiq_domain::automated::folder_tag).unwrap_or_default(),
                        "event set aside"
                    );
                    self.counters.inc(codes::INGEST_AUTOMATED);
                    self.note_aside(&e.surface, reason, true);
                }
            }
        }
        if !again.is_empty() {
            self.rc_held.insert(key.clone(), again);
        }
        let own = match &ev.body {
            EventBody::Prompt(_) => match self.rc_prompt_turn(ev, marker) {
                Turn::Own => true,
                Turn::Other | Turn::Unmarked => false,
                // Re-read by `recheck_held` past the wait: outside a Projects session, the person's.
                Turn::Unknown if !marker && parse_rfc3339_ms(&ev.ts).is_some_and(|t| now - t >= TURN_WAIT_MS) => true,
                Turn::Unknown => {
                    if marker {
                        self.note_projects_folder(ev.cwd.as_deref().map(folder_key).unwrap_or_default());
                    }
                    if self.rc_held.len() >= MAX_HELD {
                        self.rc_held.clear();
                    }
                    self.rc_held.entry(key).or_default().push(ev.clone());
                    return Ok(Some(Gate::Skip("rc-pending")));
                }
            },
            // A turn end belongs to the turn its prompt started. With no verdict (a core restart mid-turn), the
            // session's default: a Projects session's turns are Claude's, any other's the person's.
            EventBody::Stop(_) => self.rc_own.get(&key).copied().unwrap_or(!marker),
        };
        if !stop {
            if self.rc_own.len() >= 4096 {
                self.rc_own.clear();
            }
            self.rc_own.insert(key, own);
        }
        if own {
            return Ok(None);
        }
        if marker {
            self.note_projects_folder(ev.cwd.as_deref().map(folder_key).unwrap_or_default());
            return Ok(Some(Gate::Skip("projects")));
        }
        Ok(Some(Gate::Skip("not-typed")))
    }

    /// How long the core waits before re-reading held prompts' records (`recheck_held`), while any wait. None when
    /// nothing waits. Measured 2026-11-09 on 0.1.27: a turn's record was not in the file when the core first read it
    /// (2 of 2), though stamped before the hook, so the first read is often "unknown".
    pub fn held_wait(&self) -> Option<std::time::Duration> {
        let now = self.clock.now().utc_ms;
        if self.rc_held.iter().any(|(k, held)| self.head_decided(k) && Self::held_fresh(held, now)) {
            Some(std::time::Duration::from_millis(RECHECK_MS))
        } else if !self.rc_held.is_empty() {
            Some(std::time::Duration::from_millis(HEAD_RECHECK_MS as u64))
        } else {
            None
        }
    }

    /// True while any of a session's held prompts is younger than `TURN_WAIT_MS`: its record may still be written.
    fn held_fresh(held: &[EventV1], now: i64) -> bool {
        held.iter().any(|e| parse_rfc3339_ms(&e.ts).is_none_or(|t| now - t < TURN_WAIT_MS))
    }

    /// Rule RC's head check is done for this session: settled, or out of reads.
    fn head_decided(&self, k: &(String, String)) -> bool {
        match self.rc_checked.get(&format!("{}/{}", k.0, k.1)) {
            Some(RcCheck::Settled(_)) => true,
            Some(RcCheck::Unsettled(n)) => *n >= RC_TRIES,
            None => false,
        }
    }

    /// Re-read held prompts, so a person's turn shows within a second instead of at its Stop. A session whose head is
    /// still unwritten is re-read at most every `HEAD_RECHECK_MS`, so its `RC_TRIES` reads span about 8 s (measured
    /// 2026-11-02: the head is written 64 ms to 3.7 s before the first hook).
    pub fn recheck_held(&mut self) {
        let now = self.clock.now().utc_ms;
        let heads = now - self.head_rechecked_ms >= HEAD_RECHECK_MS;
        if heads {
            self.head_rechecked_ms = now;
        }
        let stale = now - self.stale_rechecked_ms >= STALE_RECHECK_MS;
        if stale {
            self.stale_rechecked_ms = now;
        }
        let keys: Vec<_> = self
            .rc_held
            .iter()
            .filter(|(k, held)| if self.head_decided(k) { stale || Self::held_fresh(held, now) } else { heads })
            .map(|(k, _)| k.clone())
            .collect();
        for k in keys {
            for e in self.rc_held.remove(&k).unwrap_or_default() {
                self.ingest_event(&e, None);
            }
        }
    }

    /// A folder where a Projects session's turns are left out, so "Include them" can be offered there.
    fn note_projects_folder(&mut self, k: String) {
        if self.projects_folders.len() < 1024 && !k.is_empty() && self.projects_folders.insert(k) {
            info!(aiq.reason = "projects", "Projects session set aside");
        }
    }

    /// Rule RC per turn: who typed this prompt, from its own record near the transcript's end (`projects::turn_at`).
    /// Measured 2026-11-08: the record is written 28 ms to 2.1 s before the core gets the prompt (101 of 102 turns).
    /// An event with no transcript (an older hook that set `projects` itself) cannot be read: not the person's.
    fn rc_prompt_turn(&mut self, ev: &EventV1, marker: bool) -> crate::projects::Turn {
        use crate::projects::{TAIL_BYTES, Turn, turn_at};
        if self.rc_force_own.remove(&ev.event_id) {
            return Turn::Own; // decided already: a missing record counts for a person outside Projects sessions
        }
        let Some(t) = ev.transcript.as_deref() else { return Turn::Other };
        let at = parse_rfc3339_ms(&ev.ts).unwrap_or_else(|| self.clock.now().utc_ms);
        let started = std::time::Instant::now();
        let key = (ev.surface.clone(), ev.surface_session_id.clone());
        let taken = self.rc_matched.get(&key).map(Vec::as_slice).unwrap_or_default();
        let (mut turn, matched, delivered) = turn_at(std::path::Path::new(t), at, taken, TAIL_BYTES);
        if let Some(m) = matched {
            if self.rc_matched.len() >= 4096 {
                self.rc_matched.clear();
            }
            // Prompts can reach the core out of order, so keep the session's recent matches, not only the last.
            let seen = self.rc_matched.entry(key).or_default();
            if seen.len() >= 32 {
                seen.remove(0);
            }
            seen.push(m);
        }
        if turn == Turn::Own && delivered.is_some_and(|id| self.rc_redelivered(&id, &ev.event_id)) {
            // A later session handed an older project message again (laptop 2026-11-09: 3 of 206): counted once.
            info!(aiq.reason = "redelivered", "rc.turn redelivered");
            turn = Turn::Other;
        }
        if turn == Turn::Unmarked {
            // No `turnOrigin` at all: an older Claude Code. Inside a Projects session that is Claude's own relay
            // (the brief that starts a Remote Control session has none); elsewhere it is a person typing.
            turn = if marker { Turn::Other } else { Turn::Own };
        }
        let name = match turn {
            Turn::Own => "own",
            Turn::Other | Turn::Unmarked => "other",
            Turn::Unknown => "unknown",
        };
        info!(ms = started.elapsed().as_millis() as u64, turn = name, projects = marker, "rc.turn");
        turn
    }

    /// True when this project message was already counted as a person's turn; else remembers it. Kept in the store,
    /// so a message handed again after the core restarts (hours later, across an update) is still known.
    /// The same event read again (a replay) is not a second delivery: each entry is `<message hash>.<event hash>`.
    fn rc_redelivered(&mut self, id: &str, event_id: &str) -> bool {
        let fnv = |s: &str| s.bytes().fold(0xcbf29ce484222325u64, |h, x| (h ^ x as u64).wrapping_mul(0x100000001b3));
        let (h, e) = (format!("{:016x}", fnv(id)), format!("{:016x}", fnv(event_id)));
        if self.rc_delivered.is_none() {
            let v = self.store.setting(DELIVERED_KEY).ok().flatten().unwrap_or_default();
            self.rc_delivered = Some(v.split_whitespace().map(str::to_owned).collect());
        }
        let seen = self.rc_delivered.get_or_insert_with(Default::default);
        if let Some(was) = seen.iter().find_map(|v| v.strip_prefix(&h).and_then(|r| r.strip_prefix('.'))) {
            return was != e;
        }
        if seen.len() >= DELIVERED_KEEP {
            seen.pop_front();
        }
        seen.push_back(format!("{h}.{e}"));
        let v = seen.iter().map(String::as_str).collect::<Vec<_>>().join(" ");
        if let Err(e) = self.store.set_setting(DELIVERED_KEY, &v, self.clock.now().utc_ms) {
            warn!(error.code = e.code, "delivered message not noted");
        }
        false
    }

    /// The automated-session rule, live side. A skip names its rule (`projects`, `aiq-check`, `session`,
    /// `folder`, `template`, `burst`) for the log.
    fn automated(&mut self, ev: &EventV1) -> Result<Gate, StoreError> {
        let (surface, session) = (ev.surface.as_str(), ev.surface_session_id.as_str());
        let rc_key = (surface.to_owned(), session.to_owned());
        let marker = ev.projects || self.projects_session(ev);
        // Who typed the turn, from its own transcript record (Claude Code only: the only tool whose hook names a
        // transcript). Imran 2026-11-08, "regardless of the surface": a turn a person typed counts on any device; a
        // relayed brief, another session's message, a scheduled run or a turn the desktop app started does not
        // (829 transcripts on the laptop, 2026-11-09: 1 `scheduled`, 14 desktop `sdk` turns in sessions with no
        // Projects marker were counted). A session whose head is still unwritten waits below (`rc_unsettled`).
        if (marker || (ev.transcript.is_some() && !self.rc_unsettled(ev)))
            && !self.rc_included(ev)?
            && let Some(gate) = self.typed_gate(ev, marker)?
        {
            return Ok(gate);
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
        if self.rc_unsettled(ev) {
            if self.rc_held.len() >= MAX_HELD {
                self.rc_held.clear();
            }
            self.rc_held.entry(rc_key).or_default().push(ev.clone());
            return Ok(Gate::Skip("rc-pending"));
        }
        if let Some(earlier) = self.rc_held.remove(&rc_key) {
            // Settled as the person's session: its held prompts go in first, through every other rule.
            for e in &earlier {
                self.ingest_inner(e, None)?;
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
        // A tip a hook is printing right now counts as shown, as at the hand-out (DC-022).
        self.expire_handovers();
        let printing = self.handovers.len() as u32;
        let input = DecideInput {
            paused: self.paused(),
            job: p.job,
            core_opportunity: p.core_opportunity,
            score: p.score,
            threshold: p.threshold,
            delivered_in_session: self.handovers.values().any(|h| h.surface == surface && h.session == session)
                || self.store.delivered_in_session(surface, session)?,
            in_quiet_period: printing > 0 || self.quiet()?,
            ready_in_session: self.store.ready_card(surface, session)?.is_some(),
            deliveries_counted_today: self.counted_today()? + printing,
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

    /// Expire show-ready cards not delivered within the expiry window after their turn ended, so a stale
    /// card never shows in a later, unrelated turn. The window runs from the turn's end, not the decision:
    /// on the laptop (2026-11-03 14:18Z) a 37-minute Copilot turn lost its tip 5 minutes before it ended.
    pub fn expire_stale(&mut self) -> Result<(), StoreError> {
        let now = self.clock.now().utc_ms;
        let ended_before = now - self.pack.trigger.ready_expiry_min as i64 * 60_000;
        for c in self.store.stale_ready(ended_before, now - READY_TURN_MAX_MS)? {
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
        self.hand_over(&ready.card_id, &ready.card_json, surface, session)
    }

    /// A turn-end hook takes the tip but it counts as shown only when the hook says it printed it
    /// (`card_shown`): a hook that gave up waiting must not use up the tip (DC-022, measured 10 lost in 150
    /// loaded test runs before this). Same budget checks as `deliver`.
    fn hand_over(
        &mut self,
        card_id: &str,
        card_json: &str,
        surface: &str,
        session: &str,
    ) -> Result<Option<CardV1>, StoreError> {
        if !self.may_hand_out(card_id, surface, session)? {
            return Ok(None);
        }
        let h = Handover {
            at_ms: self.clock.now().utc_ms,
            surface: surface.to_owned(),
            session: session.to_owned(),
            card_json: card_json.to_owned(),
        };
        self.handovers.insert(card_id.to_owned(), h);
        info!(aiq.outcome = "handed_over", aiq.surface = surface, "card handed to the hook");
        Ok(serde_json::from_str(card_json).ok())
    }

    /// The turn-end hook printed the tip it took: record it as shown. A confirmation after the wait ran out is
    /// still the truth and is recorded while the tip is still ready; anything else is counted and ignored.
    pub fn card_shown(&mut self, card_id: &str, surface: &str, session: &str) {
        let _s = info_span!("core.card_shown", aiq.surface = surface).entered();
        let r = (|| -> Result<(), StoreError> {
            let card_json = match self.handovers.remove(card_id) {
                Some(h) if h.surface == surface && h.session == session => h.card_json,
                Some(h) => {
                    self.handovers.insert(card_id.to_owned(), h);
                    self.counters.inc(codes::CARD_SHOWN_UNKNOWN);
                    return Ok(());
                }
                None => match self.store.ready_card(surface, session)? {
                    Some(r) if r.card_id == card_id => r.card_json,
                    _ => {
                        self.counters.inc(codes::CARD_SHOWN_UNKNOWN);
                        return Ok(());
                    }
                },
            };
            self.record_delivery(card_id, &card_json, surface, session).map(|_| ())
        })();
        if let Err(e) = r {
            self.fail(e);
        }
    }

    /// The core could not pass a taken tip back to its hook (the hook's wait had ended): it is ready again now.
    pub fn hand_back(&mut self, card_id: &str) {
        if self.handovers.remove(card_id).is_some() {
            self.counters.inc(codes::CARD_UNCONFIRMED);
        }
    }

    /// Forget hand-overs whose hook never confirmed in time; each is counted, and its tip is ready again.
    fn expire_handovers(&mut self) {
        let now = self.clock.now().utc_ms;
        let before = self.handovers.len();
        self.handovers.retain(|_, h| now - h.at_ms < HANDOVER_CONFIRM_MS);
        for _ in self.handovers.len()..before {
            self.counters.inc(codes::CARD_UNCONFIRMED);
        }
    }

    /// The budget (pause, one per session, quiet period, daily cap), with a tip a hook is printing right now
    /// counted as shown, so no renderer gets a second tip while the first one's confirmation is on its way.
    fn may_hand_out(&mut self, card_id: &str, surface: &str, session: &str) -> Result<bool, StoreError> {
        self.expire_handovers();
        if self.handovers.contains_key(card_id) {
            return Ok(false);
        }
        let printing = self.handovers.len() as u32;
        let in_session = self.handovers.values().any(|h| h.surface == surface && h.session == session);
        Ok(may_deliver(
            self.paused(),
            in_session || self.store.delivered_in_session(surface, session)?,
            printing > 0 || self.quiet()?,
            self.counted_today()? + printing,
            self.daily_cap(),
        ))
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
        if !self.may_hand_out(card_id, surface, session)? {
            return Ok(None);
        }
        self.record_delivery(card_id, card_json, surface, session)
    }

    /// Record the tip as shown and raise the Windows pop-up where it is on.
    fn record_delivery(
        &mut self,
        card_id: &str,
        card_json: &str,
        surface: &str,
        session: &str,
    ) -> Result<Option<CardV1>, StoreError> {
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
                _ => self.quiet_line(surface, &session)?,
            });
        }
        if self.store.last_event_kind(surface, &session)?.as_deref() != Some("stop") {
            return self.trend_line(surface, &session); // mid-turn: a card never shows while the agent works
        }
        let ready = match self.store.ready_card(surface, &session)? {
            Some(r) => Some(r),
            None if surface == COPILOT_CLI && !self.vscode_extension_asking() => self.vscode_ready()?,
            None => None,
        };
        let Some(ready) = ready else { return self.quiet_line(surface, &session) };
        // A VS Code tip shown here is delivered to this status line's session, so its lifetime follows
        // this session's prompts like the line's own tips.
        Ok(self.deliver(&ready.card_id, &ready.card_json, surface, &session)?.map(TipLineOut::Full))
    }

    /// After a turn ends with no tip to show: the milestone note if there is one for this session, else the trend.
    fn quiet_line(&mut self, surface: &str, session: &str) -> Result<Option<TipLineOut>, StoreError> {
        if self.store.last_event_kind(surface, session)?.as_deref() == Some("stop")
            && let Some(n) = self.milestone_note(surface, session)?
        {
            return Ok(Some(TipLineOut::Milestone(n.text)));
        }
        self.trend_line(surface, session)
    }

    /// The milestone note for this session: the one it already holds until its next prompt, else the newest
    /// milestone not yet said (claimed here, so no other place says it). None when there is nothing new.
    pub fn milestone_note(&mut self, surface: &str, session: &str) -> Result<Option<NoteClaim>, StoreError> {
        if let Some(v) = self.store.setting(MILESTONE_NOTE_KEY)?.filter(|v| !v.is_empty())
            && let Ok(n) = serde_json::from_str::<serde_json::Value>(&v)
            && n["surface"] == surface
            && n["session"] == session
            && let (Some(at), Some(text)) = (n["at_ms"].as_i64(), n["text"].as_str())
            && self.store.prompts_after(surface, session, at)? == 0
        {
            return Ok(Some(NoteClaim { text: text.to_owned(), prev_seen: None }));
        }
        let now = self.clock.now().utc_ms;
        let prev_seen = self.store.setting(MILESTONE_SEEN_KEY)?;
        let seen = prev_seen.as_deref().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        let Some((earned, words)) = coach::milestone_to_note(&self.coach_cache()?.milestones, seen, now)
            .map(|m| (m.earned_ms, coach::milestone_words(m)))
        else {
            return Ok(None);
        };
        let text = milestone_note_text(&words);
        let note = serde_json::json!({ "surface": surface, "session": session, "at_ms": now, "text": text });
        self.store.set_setting(MILESTONE_SEEN_KEY, &earned.to_string(), now)?;
        self.store.set_setting(MILESTONE_NOTE_KEY, &note.to_string(), now)?;
        info!(aiq.surface = surface, aiq.outcome = "milestone_note", "milestone note claimed");
        Ok(Some(NoteClaim { text, prev_seen }))
    }

    /// A claimed note never reached the person (the hook's wait ran out): put the seen mark back so the next
    /// turn end says it. A note the session already held stays as it was.
    pub fn unclaim_note(&mut self, n: &NoteClaim) {
        let now = self.clock.now().utc_ms;
        let back = n.prev_seen.clone().unwrap_or_else(|| "0".into());
        if let Err(e) = self
            .store
            .set_setting(MILESTONE_SEEN_KEY, &back, now)
            .and_then(|_| self.store.set_setting(MILESTONE_NOTE_KEY, "", now))
        {
            self.fail(e);
        }
    }

    /// Claude Code's turn end with no tip: the milestone note, said in the conversation (`systemMessage`).
    /// Copilot CLI says it on its status line instead; turn ends with the agent still running say nothing.
    pub fn stop_note(&mut self, ev: &EventV1) -> Option<NoteClaim> {
        let EventBody::Stop(stop) = &ev.body else { return None };
        if ev.surface != CLAUDE_CODE || stop.stop_hook_active || self.paused() {
            return None;
        }
        self.milestone_note(&ev.surface, &ev.surface_session_id).unwrap_or_else(|e| {
            self.fail(e);
            None
        })
    }

    /// VS Code polls with no tip waiting: the milestone note, once its newest session's turn has ended.
    pub fn vscode_note(&mut self) -> Option<NoteClaim> {
        if self.paused() {
            return None;
        }
        let r = (|| -> Result<Option<NoteClaim>, StoreError> {
            let Some(session) = self.store.latest_session(VSCODE)? else { return Ok(None) };
            if self.store.last_event_kind(VSCODE, &session)?.as_deref() != Some("stop") {
                return Ok(None);
            }
            self.milestone_note(VSCODE, &session)
        })();
        r.unwrap_or_else(|e| {
            self.fail(e);
            None
        })
    }

    /// The open task's trend for the status line, when the person has it on and there is an estimate. Uses the
    /// coaching rows already built (never rebuilds them on a status-line refresh) for "could reach".
    fn trend_line(&self, surface: &str, session: &str) -> Result<Option<TipLineOut>, StoreError> {
        if !self.switch_on(SETTING_LIVE_TREND) {
            return Ok(None);
        }
        let started = std::time::Instant::now();
        let now = self.clock.now().utc_ms;
        let t = crate::live_trend::open_task(&self.store, surface, session, now, self.engine.idle_gap_ms())?;
        let could = t.as_ref().and_then(|t| self.coach.as_ref().and_then(|c| crate::live_trend::could(c, t)));
        let line = t.as_ref().and_then(|t| crate::live_trend::status_text(t, could));
        info!(
            aiq.surface = surface,
            open = t.is_some(),
            shown = line.is_some(),
            ms = started.elapsed().as_millis() as u64,
            "core.trend_line"
        );
        Ok(line.map(TipLineOut::Trend))
    }

    fn vscode_extension_asking(&self) -> bool {
        self.vscode_asked_ms.is_some_and(|t| self.clock.now().utc_ms - t < VSCODE_FALLBACK_AFTER_MS)
    }

    /// The oldest show-ready VS Code card whose turn has ended.
    fn vscode_ready(&self) -> Result<Option<aiq_store::ReadyCard>, StoreError> {
        for c in self.store.ready_cards_on(VSCODE)? {
            if self.store.last_event_kind(VSCODE, &c.session)?.as_deref() == Some("stop") {
                return Ok(Some(c));
            }
        }
        Ok(None)
    }

    /// A renderer that cannot name its session (the VS Code extension): deliver the oldest show-ready
    /// card of the surface whose session's turn has ended.
    pub fn take_card_any(&mut self, surface: &str) -> Option<CardV1> {
        let _s = info_span!("core.take_card_any", aiq.surface = surface).entered();
        if surface == VSCODE {
            self.vscode_asked_ms = Some(self.clock.now().utc_ms);
        }
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
            agent_actions: Some(self.actions_status()),
            tip_funnel: crate::funnel::tip_funnels(&self.store, self.clock.now()).ok(),
            aside: aside_now(&self.store, self.clock.now()).unwrap_or_default(),
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

    /// Show a card's details and record that the person opened it (`opened_on`: cli / vscode). An empty
    /// `card_id` means the tip delivered last, which the Copilot status line's `aiq show` opens.
    /// simplify: the latest tip on any surface, not this session's. Ceiling: two tools getting tips within
    /// minutes of each other can open the other one's tip. Upgrade trigger: a person reports `aiq show`
    /// opened the wrong tip; then the status line passes its session.
    pub fn show(&mut self, card_id: &str, opened_on: &str) -> ShowReply {
        let latest;
        let card_id = if card_id.is_empty() {
            latest = self.store.last_delivered_card_id().ok().flatten().unwrap_or_default();
            latest.as_str()
        } else {
            card_id
        };
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
                    team_tool: None,
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
                team_tool: None,
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
