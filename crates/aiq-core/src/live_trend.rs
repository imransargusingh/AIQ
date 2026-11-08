//! The open task's live rung trend, the store side (design `docs/design/live-rung-trend.md`). Reads one
//! session's live prompts and the agent actions recorded at each turn end (`turn_action`), and runs the pure
//! estimate (`aiq_domain::trend`). Shared by the core (the status-line trend, D) and AIQ home (the Right now
//! card, C), so both show the same estimate. Read-only; no model call.
use aiq_contracts::card::CardV1;
use aiq_contracts::judge::ActionSummary;
use aiq_domain::reprompt::starts_new_task;
use aiq_domain::rung::{JOB_THRESHOLD, TASK_TYPE_THRESHOLD, Turn};
use aiq_domain::trend::{briefed, trend_with};
use aiq_store::{SessionEvent, Store, StoreError};
use serde::Serialize;
use std::collections::BTreeMap;

/// How far back a session is read to find the open task's first prompt. A task longer than this shows its
/// last day only. simplify: a fixed look-back; upgrade when a task runs past a day.
pub const LOOK_BACK_MS: i64 = 24 * 3_600_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepOut {
    pub turn: u32,
    pub at_ms: i64,
    /// Estimated rung after this turn; None when the turns so far cannot settle it.
    pub rung: Option<u8>,
    pub recorded: bool,
}

/// The tip delivered in this task, pinned at the turn it showed after.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TipAt {
    pub card_id: String,
    pub turn: u32,
    pub try_this: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveTrend {
    pub surface: String,
    pub session: String,
    pub first_ms: i64,
    pub last_ms: i64,
    pub prompts: u32,
    /// The agent is working on the last prompt (no turn end yet): that turn has no estimate yet.
    pub working: bool,
    /// From the first-prompt label at or above the judge's bars; None when unknown.
    pub job: Option<String>,
    pub task_type: Option<String>,
    pub steps: Vec<StepOut>,
    /// The latest estimate (the last step that has one).
    pub rung: Option<u8>,
    /// The rung judged tasks of the same kind could reach (coaching row); filled by the caller.
    pub could: Option<u8>,
    /// Reasons, in words-ready facts. `did_turns`: turns where the agent made the edits (or looked things up,
    /// for an answer or run job: `did_is_lookup`).
    pub did_turns: Vec<u32>,
    pub did_is_lookup: bool,
    pub brief: Option<bool>,
    pub unrecorded: Vec<u32>,
    /// For each of `unrecorded`, the reason: no_log, read_error, too_large, unparsed, no_start, or not_read.
    pub unrecorded_why: Vec<String>,
    pub subagents: u32,
    pub tip: Option<TipAt>,
}

/// The open task of a session, or None when its last event is more than `idle_gap_ms` before `now_ms` (the
/// task ended) or the session has no live prompt.
pub fn open_task(
    store: &Store,
    surface: &str,
    session: &str,
    now_ms: i64,
    idle_gap_ms: i64,
) -> Result<Option<LiveTrend>, StoreError> {
    let evs = store.session_events(surface, session, now_ms - LOOK_BACK_MS)?;
    let Some(last) = evs.last() else { return Ok(None) };
    if now_ms - last.ts_utc_ms > idle_gap_ms {
        return Ok(None);
    }
    // The task's first prompt: the last prompt that came more than the idle gap after the one before it.
    let prompts: Vec<usize> = (0..evs.len()).filter(|&i| evs[i].kind == "prompt").collect();
    let Some(&first) = prompts
        .iter()
        .zip(prompts.iter().skip(1))
        .rev()
        .find(|(a, b)| starts_new_task(evs[**a].ts_utc_ms, evs[**b].ts_utc_ms, idle_gap_ms))
        .map(|(_, b)| b)
        .or(prompts.first())
    else {
        return Ok(None);
    };
    let evs = &evs[first..];
    let (turns, whys, working) = turns_of(evs);
    let first_text = evs[0].text.as_deref();
    let (job, task_type) = label(store, surface, session, evs[0].ts_utc_ms)?;
    let (steps, why) = trend_with(&turns, job.as_deref(), first_text.map(briefed));
    let prompts = evs.iter().filter(|e| e.kind == "prompt").count() as u32;
    let tip = match store.delivered_card(surface, session)? {
        Some((card_id, json, at)) if at >= evs[0].ts_utc_ms => {
            let turn = evs.iter().filter(|e| e.kind == "prompt" && e.ts_utc_ms <= at).count() as u32;
            serde_json::from_str::<CardV1>(&json).ok().map(|c| TipAt { card_id, turn, try_this: c.try_this })
        }
        _ => None,
    };
    let (did_is_lookup, did_turns) = why.did.unwrap_or((false, vec![]));
    Ok(Some(LiveTrend {
        surface: surface.to_owned(),
        session: session.to_owned(),
        first_ms: evs[0].ts_utc_ms,
        last_ms: evs.last().map_or(0, |e| e.ts_utc_ms),
        prompts,
        working,
        job,
        task_type,
        rung: steps.iter().rev().find_map(|s| s.rung),
        steps: steps
            .into_iter()
            .map(|s| StepOut { turn: s.turn, at_ms: s.at_ms, rung: s.rung, recorded: s.recorded })
            .collect(),
        could: None,
        did_turns,
        did_is_lookup,
        brief: why.brief,
        unrecorded_why: why.unrecorded.iter().map(|t| whys[*t as usize - 1].clone().unwrap_or_default()).collect(),
        unrecorded: why.unrecorded,
        subagents: why.subagents,
        tip,
    }))
}

/// Turns that have ended, each with its recorded actions (None when a stop has no recorded row) and why it was
/// not recorded (the stored `why`, or `not_read` for a stop with no row), and whether the agent is still working
/// on the last prompt. Several stops in one turn (a Copilot steering message) add up; one unrecorded stop makes
/// the turn unrecorded.
fn turns_of(evs: &[SessionEvent]) -> (Vec<Turn>, Vec<Option<String>>, bool) {
    let mut turns: Vec<(Turn, Option<String>, bool)> = Vec::new();
    for e in evs {
        match e.kind.as_str() {
            "prompt" => turns.push((
                Turn {
                    at_ms: e.ts_utc_ms,
                    prompt: e.text.clone().unwrap_or_default(),
                    actions: Some(ActionSummary::default()),
                },
                None,
                false,
            )),
            "stop" => {
                let Some((t, why, ended)) = turns.last_mut() else { continue };
                *ended = true;
                if e.action_outcome.as_deref() == Some("recorded") {
                    if let Some(a) = t.actions.as_mut() {
                        let tools: BTreeMap<String, u32> =
                            e.tools.as_deref().and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default();
                        for (k, n) in tools {
                            *a.tools.entry(k).or_default() += n;
                        }
                        a.subagents += e.subagents.unwrap_or(0).max(0) as u32;
                    }
                } else {
                    t.actions = None;
                    why.get_or_insert_with(|| e.why.clone().unwrap_or_else(|| "not_read".into()));
                }
            }
            _ => {}
        }
    }
    let working = turns.last().is_some_and(|(_, _, ended)| !ended);
    let (turns, whys) = turns.into_iter().filter(|(_, _, ended)| *ended).map(|(t, w, _)| (t, w)).unzip();
    (turns, whys, working)
}

/// The session's first-prompt label at or after the task's start, kept only at the judge's own bars.
fn label(
    store: &Store,
    surface: &str,
    session: &str,
    since_ms: i64,
) -> Result<(Option<String>, Option<String>), StoreError> {
    if !store.has_table("kind_label")? {
        return Ok((None, None));
    }
    let l = store
        .kind_labels_since(since_ms)?
        .into_iter()
        .rfind(|l| l.surface == surface && l.surface_session_id == session && l.outcome == "ok");
    let Some(l) = l else { return Ok((None, None)) };
    let job = l.job.filter(|_| l.job_confidence.unwrap_or(0.0) >= JOB_THRESHOLD);
    let ty = l.task_type.filter(|_| job.is_some() && l.task_type_confidence.unwrap_or(0.0) >= TASK_TYPE_THRESHOLD);
    Ok((job, ty))
}

/// The rung judged tasks of this kind could reach, when it is above the estimate: the coaching row's
/// potential (a coached move one rung up from the kind's usual rung). None without a known kind.
pub fn could(cache: &crate::coaching::Cache, t: &LiveTrend) -> Option<u8> {
    let (job, ty) = (t.job.as_deref()?, t.task_type.as_deref()?);
    let p = cache.rows.iter().find(|r| r.job == job && r.task_type == ty)?.potential?;
    (p > t.rung?).then_some(p)
}

/// One status line: "AIQ: heading for Directed (2) so far · could reach Delegated (3) · turn 4". None with no
/// estimate (nothing is shown rather than a guess).
pub fn status_text(t: &LiveTrend, could: Option<u8>) -> Option<String> {
    use aiq_domain::rung::rung_name;
    let r = t.rung?;
    let turn = t.steps.last().map_or(0, |s| s.turn);
    let mut s = format!("AIQ: heading for {} ({r}) so far", rung_name(r));
    if let Some(c) = could {
        s += &format!(" · could reach {} ({c})", rung_name(c));
    }
    s += &format!(" · turn {turn}");
    Some(s)
}
