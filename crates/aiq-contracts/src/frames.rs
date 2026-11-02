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
}

impl Request {
    pub fn token(&self) -> &str {
        match self {
            Request::Ingest { token, .. }
            | Request::TakeCard { token, .. }
            | Request::Status { token }
            | Request::Show { token, .. }
            | Request::TipLine { token, .. }
            | Request::TakeCardAny { token, .. }
            | Request::Setting { token, .. }
            | Request::Forget { token }
            | Request::IncludeFolder { token, .. } => token,
        }
    }
}

/// One spool line: an ingest frame without the token (design: hook client step 5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpoolLine {
    pub trace_id: Option<String>,
    pub event: EventV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakeCardReply {
    pub card: Option<CardV1>,
}

/// Status-line reply: the full card, or only the id of a card that is waiting to be opened, or neither.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TipLineReply {
    pub card: Option<CardV1>,
    pub waiting_card_id: Option<String>,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkReply {
    pub ok: bool,
    pub message: Option<String>,
}
