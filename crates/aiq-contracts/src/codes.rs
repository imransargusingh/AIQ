//! Stable error codes (one registry). Design: "Telemetry → Stable error codes".
pub const HOOK_PARSE: &str = "AIQ-HOOK-PARSE";
pub const HOOK_FOREIGN_HOST: &str = "AIQ-HOOK-FOREIGN-HOST";
pub const HOOK_SPOOLED: &str = "AIQ-HOOK-SPOOLED";
pub const HOOK_STOP_TIMEOUT: &str = "AIQ-HOOK-STOP-TIMEOUT";
pub const SPOOL_DROPPED: &str = "AIQ-SPOOL-DROPPED";
pub const SPOOL_BAD_LINE: &str = "AIQ-SPOOL-BAD-LINE";
pub const API_AUTH: &str = "AIQ-API-AUTH";
pub const INGEST_SHAPE: &str = "AIQ-INGEST-SHAPE";
pub const INGEST_BACKPRESSURE: &str = "AIQ-INGEST-BACKPRESSURE";
/// A prompt or stop of an automated session (import spec US-12): recorded nothing.
pub const INGEST_AUTOMATED: &str = "AIQ-INGEST-AUTOMATED";
/// A Copilot message typed while the agent was working, read from Copilot's own session log because no hook
/// fires for it (`copilot_log`). A count, not an error.
pub const INGEST_MID_TURN: &str = "AIQ-INGEST-MID-TURN";
/// Copilot's session log could not be read at agentStop (missing file, locked). A count; nothing is lost but
/// that read's mid-turn messages.
pub const INGEST_LOG_READ: &str = "AIQ-INGEST-LOG-READ";
/// A live turn whose agent actions AIQ could not read from the tool's log (no log, read error, a line that
/// did not parse, more than 16 MB waiting). Stored as "not recorded", never guessed.
pub const INGEST_ACTIONS_NOT_RECORDED: &str = "AIQ-INGEST-ACTIONS-NOT-RECORDED";
/// A secret (API key, token, private key, password value) removed from a stored prompt (security review S2).
/// A count, not an error.
pub const INGEST_SECRET_REMOVED: &str = "AIQ-INGEST-SECRET-REMOVED";
pub const DELIVERY_CONFLICT: &str = "AIQ-DELIVERY-CONFLICT";
pub const STORE_OPEN: &str = "AIQ-STORE-OPEN";
pub const STORE_MIGRATION: &str = "AIQ-STORE-MIGRATION";
pub const STORE_WRITE: &str = "AIQ-STORE-WRITE";
pub const STORE_APPEND_ONLY: &str = "AIQ-STORE-APPEND-ONLY";
pub const CORE_ALREADY_RUNNING: &str = "AIQ-CORE-ALREADY-RUNNING";
pub const CORE_START_BACKOFF: &str = "AIQ-CORE-START-BACKOFF";
/// AIQ home could not read the store (locked past the busy timeout, or unreadable).
pub const HOME_STORE_READ: &str = "AIQ-HOME-STORE-READ";
/// `aiq export` could not write its folder (not writable, disk full); the partial folder is removed.
pub const EXPORT_WRITE: &str = "AIQ-EXPORT-WRITE";

/// Rung judge (design docs/design/rung-judge.md, Failure-mode analysis).
/// AIQ guessed the kind of task right fewer than 8 in 10 times and turned next-rung tips off (coaching D10).
pub const COACH_GUESS_LOW: &str = "AIQ-COACH-GUESS-LOW";
/// A next-step tip was held back because the accuracy check paused its job or kind (design K6b). A count.
pub const COACH_ACCURACY_PAUSED: &str = "AIQ-COACH-ACCURACY-PAUSED";
/// A new task's kind was not matched by words, so AIQ asked a small model to label it (design kind-label). A count.
pub const COACH_LABEL_REQUESTED: &str = "AIQ-COACH-LABEL-REQUESTED";
pub const JUDGE_MISSING: &str = "AIQ-JUDGE-MISSING";
pub const JUDGE_SIGNIN: &str = "AIQ-JUDGE-SIGNIN";
pub const JUDGE_SCHEMA: &str = "AIQ-JUDGE-SCHEMA";
pub const JUDGE_TIMEOUT: &str = "AIQ-JUDGE-TIMEOUT";
pub const JUDGE_ERROR: &str = "AIQ-JUDGE-ERROR";

/// Every code, for status output and the registry test.
pub const ALL: &[&str] = &[
    COACH_GUESS_LOW,
    COACH_ACCURACY_PAUSED,
    COACH_LABEL_REQUESTED,
    HOOK_PARSE,
    HOOK_FOREIGN_HOST,
    HOOK_SPOOLED,
    HOOK_STOP_TIMEOUT,
    SPOOL_DROPPED,
    SPOOL_BAD_LINE,
    API_AUTH,
    INGEST_SHAPE,
    INGEST_BACKPRESSURE,
    INGEST_AUTOMATED,
    INGEST_MID_TURN,
    INGEST_LOG_READ,
    INGEST_ACTIONS_NOT_RECORDED,
    INGEST_SECRET_REMOVED,
    DELIVERY_CONFLICT,
    STORE_OPEN,
    STORE_MIGRATION,
    STORE_WRITE,
    STORE_APPEND_ONLY,
    CORE_ALREADY_RUNNING,
    CORE_START_BACKOFF,
    HOME_STORE_READ,
    EXPORT_WRITE,
    JUDGE_MISSING,
    JUDGE_SIGNIN,
    JUDGE_SCHEMA,
    JUDGE_TIMEOUT,
    JUDGE_ERROR,
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_codes_unique_and_prefixed() {
        let mut v = ALL.to_vec();
        v.sort();
        v.dedup();
        assert_eq!(v.len(), ALL.len());
        assert!(ALL.iter().all(|c| c.starts_with("AIQ-")));
    }
}
