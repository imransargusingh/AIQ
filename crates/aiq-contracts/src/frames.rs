//! Local-API frames: newline-delimited JSON over the user-only socket / pipe, one frame per connection.
use crate::card::CardV1;
use crate::event::EventV1;
use serde::{Deserialize, Serialize};

/// Frames larger than this are refused (design: STRIDE B1 D).
pub const MAX_FRAME_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Ingest {
        token: String,
        trace_id: Option<String>,
        event: EventV1,
    },
    TakeCard {
        token: String,
        trace_id: Option<String>,
        event: EventV1,
        wait_ms: u64,
    },
    /// The turn-end hook printed the tip it took with `TakeCard` (DC-022). No reply.
    Shown {
        token: String,
        card_id: String,
        surface: String,
        session: String,
    },
    Status {
        token: String,
    },
    /// `opened_on`: where the person opened it (`cli`, `vscode`); recorded as a `card_open` fact.
    Show {
        token: String,
        card_id: String,
        #[serde(default)]
        opened_on: Option<String>,
    },
    /// Copilot CLI status line. `session` is None when the status line sends no session id.
    TipLine {
        token: String,
        surface: String,
        session: Option<String>,
    },
    /// A renderer that cannot name its session (VS Code extension): any show-ready card whose turn ended.
    TakeCardAny {
        token: String,
        surface: String,
    },
    Setting {
        token: String,
        key: String,
        value: String,
    },
    Forget {
        token: String,
    },
    /// "Include them": the person says a folder AIQ set aside as automated is their own work.
    IncludeFolder {
        token: String,
        folder: String,
    },
    /// Delete one prompt's text (AIQ home, Your data). The event itself stays.
    ForgetItem {
        token: String,
        event_id: String,
    },
    /// The person's answer to a tip (coaching-from-rungs D8): `card_id` names the card, or `tip_id` the tip.
    TipFeedback {
        token: String,
        #[serde(default)]
        card_id: Option<String>,
        #[serde(default)]
        tip_id: Option<String>,
        answer: String,
        surface: String,
    },
    /// `aiq label` stored a label for this live task: decide its next-step tip now (design kind-label). No reply.
    KindLabel {
        token: String,
        task_id: String,
    },
    /// "This doesn't fit" (`action` flag, with a reason) or its undo, for one (job, task type, move).
    GuidanceFlag {
        token: String,
        job: String,
        task_type: String,
        move_id: String,
        action: String,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        note: Option<String>,
    },
}

impl Request {
    pub fn token(&self) -> &str {
        match self {
            Request::Ingest { token, .. }
            | Request::TakeCard { token, .. }
            | Request::Shown { token, .. }
            | Request::Status { token }
            | Request::Show { token, .. }
            | Request::TipLine { token, .. }
            | Request::TakeCardAny { token, .. }
            | Request::Setting { token, .. }
            | Request::Forget { token }
            | Request::IncludeFolder { token, .. }
            | Request::ForgetItem { token, .. }
            | Request::TipFeedback { token, .. }
            | Request::KindLabel { token, .. }
            | Request::GuidanceFlag { token, .. } => token,
        }
    }
}

/// One spool line: an ingest frame without the token (design: hook client step 5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpoolLine {
    pub trace_id: Option<String>,
    pub event: EventV1,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TakeCardReply {
    pub card: Option<CardV1>,
    /// The person turned the tip highlight off (setting `tip_highlight`): render without colour.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plain: bool,
    /// VS Code only: the team's "Tool to try" line for this tip (team tips file), read in place, never stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_tool: Option<String>,
}

/// Status-line reply: the full card, or only the id of a card that is waiting to be opened, or neither.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TipLineReply {
    pub card: Option<CardV1>,
    pub waiting_card_id: Option<String>,
    /// No tip: where the open task is heading, one plain line (design live-rung-trend D). Older cores omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trend: Option<String>,
    /// As `TakeCardReply::plain`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plain: bool,
    /// The line the status line prints, rendered by the core (colour per the highlight setting), so
    /// `aiq-statusline.exe` never changes when a tip's look does (design auto-update U5n). Older cores omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// The same line without colour, for a person who set `NO_COLOR`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_plain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StatusReply {
    pub paused: bool,
    pub daily_cap: u32,
    pub cards_today: u32,
    pub events_by_kind: Vec<(String, u64)>,
    pub signals: u64,
    pub decisions: Vec<(String, String, u64)>,
    pub store_bytes: u64,
    pub core_counters: Vec<(String, u64)>,
    /// Folders whose sessions AIQ ignores as automated, newest first: (folder, reason).
    #[serde(default)]
    pub automated_folders: Vec<(String, String)>,
    /// Next-rung tips and AIQ's self-check of its type guesses (coaching-from-rungs D10).
    #[serde(default)]
    pub coach: Option<CoachStatus>,
    /// Live agent actions read at each turn's end (design live-agent-actions). None with an older core.
    #[serde(default)]
    pub agent_actions: Option<ActionsStatus>,
    /// Where new tasks' tips went: today and the last 7 days (design tip-funnel). None with an older core.
    #[serde(default)]
    pub tip_funnel: Option<TipFunnels>,
}

/// The tip funnel over two periods, and the recent working days with no tip (design tip-funnel). Measured.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TipFunnels {
    pub today: TipFunnel,
    pub week: TipFunnel,
    /// Days before today, in the last 7, with 3 or more new tasks and no tip shown. Newest first.
    #[serde(default)]
    pub droughts: Vec<TipDrought>,
}

/// One period of the tip funnel: new tasks down to tips opened. Counted from stored rows (measured); tasks before
/// the funnel was installed (store v11) are not counted.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TipFunnel {
    /// 1 = since local midnight; 7 = the last 7 days.
    pub days: u32,
    /// New tasks (a person's first prompt) that reached the tip step.
    pub tasks: u32,
    /// Of those, tasks whose kind of task AIQ knew.
    pub kind_known: u32,
    /// Of those, tasks a next-step tip was decided for.
    pub tip_found: u32,
    /// Why the other tasks got no tip, most first.
    pub no_tip: Vec<FunnelCount>,
    /// Tips decided (next-step tips plus tips that fire mid-task).
    pub tips: u32,
    /// Tips held back, by reason, most first.
    pub held: Vec<FunnelCount>,
    /// Tips ready and still waiting for the turn to end.
    pub waiting: u32,
    /// Tips that were ready, but the session ended before any tool showed them.
    pub expired: u32,
    /// Tips shown, by tool (`code` is the surface id), most first.
    pub shown: Vec<FunnelCount>,
    /// Shown tips the person opened, and answered (Helpful, Not helpful, Snooze, Don't show again).
    pub opened: u32,
    pub answered: u32,
}

/// A count with its code and plain words (one definition of the words: `aiq_domain::funnel::words`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FunnelCount {
    pub code: String,
    pub words: String,
    pub count: u32,
}

/// A working day with no tip shown.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TipDrought {
    /// Local date, YYYY-MM-DD.
    pub day: String,
    pub tasks: u32,
    /// The most common reason that day's tasks got no tip.
    pub main_reason: Option<FunnelCount>,
}

/// Live agent actions since local midnight, counted from stored rows (measured).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ActionsStatus {
    /// The `agent_actions` switch.
    pub enabled: bool,
    /// Turns read today, and of those, turns AIQ could not read ("not recorded").
    pub turns: u32,
    pub not_recorded: u32,
    /// Why turns were not recorded today: (reason, turns).
    #[serde(default)]
    pub why: Vec<(String, u32)>,
    /// Read cost per turn today: 95th percentile and largest, ms; and the largest read, bytes. None with no turn.
    pub p95_ms: Option<u64>,
    pub max_ms: Option<u64>,
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CoachStatus {
    /// auto, on or off (the setting).
    pub mode: String,
    /// Next-rung tips may show now.
    pub tips_on: bool,
    /// Type guesses checked against the later judgment, and how many were right. Measured.
    pub checked: u32,
    pub right: u32,
    /// Task types that coach a move now.
    pub coached_types: u32,
    /// Share of judged, typed tasks whose type is `other`, in percent; None with no typed tasks.
    pub other_pct: Option<u32>,
    /// Measured: last cache rebuild.
    pub cache_ms: u64,
    /// Jobs and kinds of task the accuracy check paused (design K6b), with the person's overrides.
    #[serde(default)]
    pub accuracy_paused: Vec<PausedScope>,
    /// Labels of new tasks' kinds by a small model (design kind-label). None before v8 or with the core down.
    #[serde(default)]
    pub labels: Option<LabelStatus>,
}

/// Kind labels today and their self-check. Every number is counted from stored rows (measured).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LabelStatus {
    /// Labels may run: background judging is on and labels are not turned off.
    pub enabled: bool,
    /// Label calls since local midnight, and the daily budget.
    pub today: u32,
    pub daily: u32,
    /// Of today's calls: answered, and answered confidently enough to coach from.
    pub answered: u32,
    pub confident: u32,
    /// Median call time today, in ms; None with no call today.
    pub median_ms: Option<u64>,
    /// Labels checked against the later judgment, and how many were right (D10 for labels).
    pub checked: u32,
    pub right: u32,
}

/// One job or kind of task whose next-step tips the accuracy check paused (design K6b).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PausedScope {
    /// `run_ship` or `code_change/fix_bug`: what `aiq config set keep-on.<scope> on` takes.
    pub scope: String,
    /// "Run or ship it" or "Code a change: Fixing a bug".
    pub label: String,
    /// The second reading agreed with the judge `agreed` of `checked` times (Inferred: a second model).
    pub agreed: u32,
    pub checked: u32,
    /// The person chose "Keep showing" for this evidence, so tips still show.
    pub kept_on: bool,
}

/// A judged task a next-rung guess leaned on: when, which tool, which kind. Never its text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeanedOn {
    pub ts: String,
    pub tool: String,
    pub task_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidencePrompt {
    pub ts: String,
    /// None when the person deleted the evidence.
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowReply {
    pub card: Option<CardV1>,
    pub outcome: Option<String>,
    pub reason_code: Option<String>,
    pub delivered: bool,
    pub evidence: Vec<EvidencePrompt>,
    /// Next-rung tips: the judged tasks the guess leaned on (time, tool, kind; never text).
    #[serde(default)]
    pub leaned_on: Vec<LeanedOn>,
    /// Opened in VS Code only: the team's "Tool to try" line (team tips file), read in place, never stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_tool: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkReply {
    pub ok: bool,
    pub message: Option<String>,
}
