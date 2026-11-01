//! Real clock: UTC milliseconds plus the local offset in force now.
use aiq_domain::clock::{Clock, Now};

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Now {
        let local = chrono::Local::now();
        Now { utc_ms: local.timestamp_millis(), offset_min: local.offset().local_minus_utc() / 60 }
    }
}

/// Parse an RFC 3339 timestamp to UTC milliseconds.
pub fn parse_rfc3339_ms(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

pub fn format_rfc3339_ms(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default()
}
