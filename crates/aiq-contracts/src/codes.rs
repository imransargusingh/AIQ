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
pub const DELIVERY_CONFLICT: &str = "AIQ-DELIVERY-CONFLICT";
pub const STORE_OPEN: &str = "AIQ-STORE-OPEN";
pub const STORE_MIGRATION: &str = "AIQ-STORE-MIGRATION";
pub const STORE_WRITE: &str = "AIQ-STORE-WRITE";
pub const STORE_APPEND_ONLY: &str = "AIQ-STORE-APPEND-ONLY";
pub const CORE_ALREADY_RUNNING: &str = "AIQ-CORE-ALREADY-RUNNING";
pub const CORE_START_BACKOFF: &str = "AIQ-CORE-START-BACKOFF";

/// Every code, for status output and the registry test.
pub const ALL: &[&str] = &[
    HOOK_PARSE,
    HOOK_FOREIGN_HOST,
    HOOK_SPOOLED,
    HOOK_STOP_TIMEOUT,
    SPOOL_DROPPED,
    SPOOL_BAD_LINE,
    API_AUTH,
    INGEST_SHAPE,
    INGEST_BACKPRESSURE,
    DELIVERY_CONFLICT,
    STORE_OPEN,
    STORE_MIGRATION,
    STORE_WRITE,
    STORE_APPEND_ONLY,
    CORE_ALREADY_RUNNING,
    CORE_START_BACKOFF,
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
