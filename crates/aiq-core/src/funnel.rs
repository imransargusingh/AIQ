//! The tip funnel read from the store (design tip-funnel): one reader for `aiq status` and Home.
use aiq_contracts::frames::TipFunnels;
use aiq_domain::clock::{DAY_MS, Now};
use aiq_domain::funnel;
use aiq_store::{Store, StoreError};

/// Today's funnel, the last 7 days' funnel and the working days with no tip.
pub fn tip_funnels(store: &Store, now: Now) -> Result<TipFunnels, StoreError> {
    let midnight = now.local_midnight_utc_ms();
    let week = midnight - 6 * DAY_MS;
    // A day more than the week, so the drought check sees whole local days.
    let steps = store.tip_steps_since(week - DAY_MS)?;
    let cards = store.card_funnel_since(week)?;
    let deliveries = store.deliveries_since(week - DAY_MS)?;
    let mut f = TipFunnels {
        today: funnel::build(&steps, &cards, midnight, 1),
        week: funnel::build(&steps, &cards, week, 7),
        droughts: funnel::droughts(&steps, &deliveries, now, 7),
    };
    for t in f.today.lost.iter_mut().chain(f.week.lost.iter_mut()) {
        // A text that can't be read leaves the line without words ("not recorded"), never a wrong one.
        if let Ok(Some((_, Some(text)))) = store.task_first_prompt(&t.task_id) {
            t.opening = funnel::opening(&text);
        }
    }
    Ok(f)
}
