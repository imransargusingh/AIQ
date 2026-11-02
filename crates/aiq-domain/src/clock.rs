//! Time as values. The core owns the real clock; tests use `FakeClock`.
use std::cell::Cell;

pub const DAY_MS: i64 = 86_400_000;

/// A moment: UTC milliseconds plus the local UTC offset in force at that moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    pub utc_ms: i64,
    pub offset_min: i32,
}

impl Now {
    pub fn local_day(&self) -> i64 {
        local_day(self.utc_ms, self.offset_min)
    }
    /// UTC time of the start of today in local time.
    pub fn local_midnight_utc_ms(&self) -> i64 {
        self.local_day() * DAY_MS - self.offset_min as i64 * 60_000
    }
}

pub trait Clock {
    fn now(&self) -> Now;
}

/// Local calendar day number (days since 1970-01-01 in local time).
pub fn local_day(utc_ms: i64, offset_min: i32) -> i64 {
    (utc_ms + offset_min as i64 * 60_000).div_euclid(DAY_MS)
}

/// Deliveries that count against today's cap: every delivery whose local day is today **or later**.
/// "Or later" is what stops a clock or time-zone moving backwards from re-opening a used budget;
/// the budget resets only when the local date moves past every recorded delivery.
pub fn deliveries_counted_today(delivery_days: impl IntoIterator<Item = i64>, today: i64) -> u32 {
    delivery_days.into_iter().filter(|d| *d >= today).count() as u32
}

#[derive(Debug)]
pub struct FakeClock(Cell<Now>);

impl FakeClock {
    pub fn new(utc_ms: i64, offset_min: i32) -> Self {
        Self(Cell::new(Now { utc_ms, offset_min }))
    }
    pub fn set(&self, utc_ms: i64, offset_min: i32) {
        self.0.set(Now { utc_ms, offset_min });
    }
    pub fn advance_ms(&self, ms: i64) {
        let n = self.0.get();
        self.0.set(Now { utc_ms: n.utc_ms + ms, ..n });
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Now {
        self.0.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn local_day_offset_moves_midnight() {
        // 2026-10-31T23:30Z is still Oct 31 in UTC, but Nov 1 at UTC+1.
        let t = 1_793_489_400_000; // 2026-10-31T23:30:00Z
        assert_eq!(local_day(t, 60), local_day(t, 0) + 1);
        assert_eq!(local_day(t, -300), local_day(t, 0));
    }

    #[test]
    fn now_local_day_matches_free_function() {
        let n = Now { utc_ms: 1_793_489_400_000, offset_min: 60 };
        assert_eq!(n.local_day(), local_day(n.utc_ms, n.offset_min));
        assert_eq!(n.local_day(), 20_758);
    }

    #[test]
    fn fake_clock_set_and_advance() {
        let c = FakeClock::new(1_000, 0);
        c.advance_ms(500);
        assert_eq!(c.now(), Now { utc_ms: 1_500, offset_min: 0 });
        c.set(7, 60);
        assert_eq!(c.now(), Now { utc_ms: 7, offset_min: 60 });
    }

    #[test]
    fn local_midnight_is_the_start_of_the_local_day() {
        let n = Now { utc_ms: 10 * DAY_MS + 3_600_000, offset_min: 120 };
        assert_eq!(n.local_midnight_utc_ms(), 10 * DAY_MS - 7_200_000);
        assert_eq!(local_day(n.local_midnight_utc_ms(), 120), n.local_day());
        let w = Now { utc_ms: 10 * DAY_MS + 3_600_000, offset_min: -300 };
        assert_eq!(w.local_midnight_utc_ms(), 9 * DAY_MS + 300 * 60_000);
    }

    #[test]
    fn local_day_before_epoch_floors() {
        assert_eq!(local_day(-1, 0), -1);
    }

    #[test]
    fn counted_today_clock_moved_back_keeps_future_deliveries() {
        assert_eq!(deliveries_counted_today([10, 10, 9], 9), 3);
        assert_eq!(deliveries_counted_today([10, 10], 11), 0);
    }

    proptest! {
        /// D2: for any sequence of clock moves, a cap-respecting delivery loop never puts more than
        /// `cap` deliveries on any one local day.
        #[test]
        fn cap_never_exceeded_for_any_clock_moves(
            cap in 0u32..6,
            moves in proptest::collection::vec((-3i64..4, -720i32..841), 1..80),
        ) {
            let mut days: Vec<i64> = Vec::new();
            let mut t: i64 = 1_793_400_000_000;
            for (dday, off) in moves {
                t += dday * DAY_MS / 2;
                let today = local_day(t, off);
                if deliveries_counted_today(days.iter().copied(), today) < cap {
                    days.push(today);
                }
            }
            for d in &days {
                prop_assert!(days.iter().filter(|x| *x == d).count() as u32 <= cap);
            }
        }
    }
}
