//! Coaching from rung results, the store side (design `docs/design/coaching-from-rungs.md`). Reads the
//! person's judgments into `aiq_domain::coach` inputs, and records their tip answers and "This doesn't fit"
//! flags. Shared by the core (live next-rung tips), the CLI when the core is down, and AIQ home, so every
//! front end derives the same rows, habits and self-check from the same facts.
use aiq_contracts::judge::{parse_read, task_type_label};
use aiq_contracts::pack::PackV1;
use aiq_domain::check::{Reading, Scope, Verdict, verdicts};
use aiq_domain::clock::Now;
use aiq_domain::coach::{self, Feedback, Flag, GuessCheck, Habit, JudgedTask, Neighbour, TypeRow};
use aiq_domain::label;
use aiq_domain::text::features;
use aiq_store::{GuidanceFlagRow, Store, StoreError, TipFeedbackRow};
use std::collections::BTreeMap;
use std::time::Instant;

pub const ANSWERS: [&str; 6] = ["helpful", "not_helpful", "snooze", "off", "on", "no_answer"];
pub const FLAG_REASONS: [&str; 4] = ["evidence_wrong", "not_useful", "blocked", "other"];
pub const MAX_NOTE: usize = 200;

/// Every current judgment as a coaching input, with its first prompt's text (None when deleted, or for a
/// task the store no longer has). Tasks whose first prompt is gone are kept with the judgment's time.
pub fn judged_tasks(store: &Store) -> Result<Vec<(JudgedTask, Option<String>)>, StoreError> {
    let mut out = Vec::new();
    for j in store.current_judgments()?.into_values() {
        let (first_ms, text) = store.task_first_prompt(&j.task_id)?.unwrap_or((j.judged_utc_ms, None));
        let tool = if j.task_id.starts_with("import:") {
            j.task_id.split(':').nth(1).unwrap_or("").to_owned()
        } else {
            j.provider.clone()
        };
        let t = JudgedTask::new(
            &j.task_id,
            j.origin == "live",
            &tool,
            first_ms,
            &j.job,
            j.task_type.as_deref(),
            &j.state,
            j.current_rung,
            j.potential_rung,
            j.coachable,
            &j.facts,
            parse_read(&j.read),
        )
        .with_job_confidence(j.job_confidence);
        out.push((t, text));
    }
    out.sort_by(|a, b| b.0.first_ms.cmp(&a.0.first_ms).then_with(|| a.0.task_id.cmp(&b.0.task_id)));
    Ok(out)
}

pub fn feedback(store: &Store) -> Result<Vec<Feedback>, StoreError> {
    Ok(store
        .tip_feedback_all()?
        .into_iter()
        .map(|f| Feedback { tip_id: f.tip_id, answer: f.answer, utc_ms: f.utc_ms })
        .collect())
}

pub fn flags(store: &Store) -> Result<Vec<Flag>, StoreError> {
    Ok(store
        .guidance_flags_all()?
        .into_iter()
        .map(|f| Flag {
            job: f.job,
            task_type: f.task_type,
            move_id: f.move_id,
            action: f.action,
            reason: f.reason,
            utc_ms: f.utc_ms,
        })
        .collect())
}

/// Settings key prefix for "Keep showing these tips" on a paused job or kind (design K6b): the value is the
/// newest check run the person saw, so a later check lets AIQ decide again; `auto` clears it.
pub const SETTING_KEEP_ON: &str = "keep_on.";

pub fn keep_on_key(scope: &Scope) -> String {
    format!("{SETTING_KEEP_ON}{}", scope.key())
}

/// The person kept this paused scope on, for the evidence it rests on now.
pub fn kept_on(store: &Store, v: &Verdict) -> bool {
    store.setting(&keep_on_key(&v.scope)).ok().flatten().as_deref() == Some(v.newest_run.as_str())
}

/// Every second reading whose judgment is still the task's current one, for the pause rule (K6b). A task
/// judged again since (a new judge version) drops out: the check described the old judge. Unreadable
/// answers are "not checked", never guessed.
pub fn readings(store: &Store) -> Result<Vec<Reading>, StoreError> {
    let current: std::collections::HashSet<String> =
        store.current_judgments()?.into_values().map(|j| j.judgment_id).collect();
    let mut out = Vec::new();
    for run in store.check_runs()? {
        for c in store.checks_of_run(&run.run_id)? {
            if !current.contains(&c.judgment_id) {
                continue;
            }
            let Some(ans) =
                c.answer.as_deref().and_then(|a| serde_json::from_str::<aiq_contracts::check::CheckAnswer>(a).ok())
            else {
                continue;
            };
            let Some(j) = store.judgment(&c.judgment_id)? else { continue };
            out.push(Reading {
                run_id: run.run_id.clone(),
                run_started_ms: run.started_utc_ms,
                judge_job: j.job,
                judge_type: j.task_type,
                reader_job: ans.job,
                reader_type: ans.task_type,
            });
        }
    }
    Ok(out)
}

/// The paused jobs and kinds as people see them, still-paused first, then the ones the person kept on.
pub fn paused_scopes(store: &Store, pauses: &[Verdict]) -> Vec<aiq_contracts::frames::PausedScope> {
    let mut v: Vec<_> = pauses
        .iter()
        .map(|p| aiq_contracts::frames::PausedScope {
            scope: p.scope.key(),
            label: scope_label(&p.scope),
            agreed: p.agree.k,
            checked: p.agree.n,
            kept_on: kept_on(store, p),
        })
        .collect();
    v.sort_by(|a, b| a.kept_on.cmp(&b.kept_on).then_with(|| a.scope.cmp(&b.scope)));
    v
}

/// A paused job or kind, in words: "Run or ship it" or "Code a change: Fixing a bug".
pub fn scope_label(scope: &Scope) -> String {
    let job = aiq_domain::rung::job_label(scope.job()).to_owned();
    match scope {
        Scope::Job(_) => job,
        Scope::Kind(j, t) => format!("{job}: {}", task_type_label(j, t).unwrap_or(t)),
    }
}

/// The next-rung tips setting: auto (default), on or off.
pub fn mode(store: &Store) -> String {
    store
        .setting(coach::SETTING_NEXT_RUNG_TIPS)
        .ok()
        .flatten()
        .filter(|v| ["auto", "on", "off"].contains(&v.as_str()))
        .unwrap_or_else(|| "auto".into())
}

/// Everything the live next-rung decision reads, derived from stored facts. Rebuilt when the store's
/// coaching fingerprint changes or the window has moved an hour.
// simplify: an in-memory cache rebuilt whole. Ceiling: a few thousand judgments. Upgrade trigger: the
// rebuild time (`coach.cache` in core.log) passes 50 ms.
#[derive(Debug, Clone)]
pub struct Cache {
    pub fingerprint: (i64, i64, i64, i64),
    pub guesses: usize,
    pub built_ms: i64,
    pub build_ms: u64,
    pub tasks: Vec<JudgedTask>,
    pub neighbours: Vec<Neighbour>,
    pub rows: Vec<TypeRow>,
    /// One row per pack job over all its kinds (`coach::job_rows`): coaching when the kind is not known.
    pub job_rows: Vec<TypeRow>,
    pub habits: Vec<Habit>,
    /// D10 self-check of word guesses (`type_guess.method = words`) against the later judgment.
    pub check: GuessCheck,
    /// The same self-check of labels (`method = label`), held to the same bar separately, so a weak word guess
    /// never turns labels off, nor the other way round.
    pub label_check: GuessCheck,
    /// Per pack job: the job-confidence bar its own labels earned (`label::job_bar`), and whether labels of that
    /// job may coach (`label::earned`).
    pub label_jobs: BTreeMap<String, (f64, bool)>,
    pub feedback: Vec<Feedback>,
    /// Share of judged, typed tasks in the window whose type is `other` (the list's fit, design assume line).
    pub other_share: Option<f64>,
    /// Jobs and kinds of task the latest accuracy checks paused (design K6b), before the person's overrides.
    pub pauses: Vec<Verdict>,
}

pub const REBUILD_EVERY_MS: i64 = 3_600_000;

impl Cache {
    pub fn build(store: &Store, pack: &PackV1, now: Now) -> Result<Self, StoreError> {
        let started = Instant::now();
        let p = &pack.coach;
        let since = now.utc_ms - p.window_days as i64 * coach::DAY_MS;
        let all = judged_tasks(store)?;
        let neighbours = all
            .iter()
            .filter(|(t, _)| t.first_ms >= since && !t.uncertain())
            .filter_map(|(t, text)| {
                let (job, ty) = t.kind()?;
                let words = features(text.as_deref()?, &pack.reprompt).words;
                (!words.is_empty()).then(|| Neighbour {
                    task_id: t.task_id.clone(),
                    job: job.to_owned(),
                    task_type: ty.to_owned(),
                    first_ms: t.first_ms,
                    words,
                })
            })
            .collect();
        let tasks: Vec<JudgedTask> = all.into_iter().map(|(t, _)| t).collect();
        let fl = flags(store)?;
        let habits = coach::habits(&tasks, &pack.moves, now.utc_ms, now.offset_min, p);
        let adopted = coach::adopted(&habits);
        let rows = coach::type_rows(&tasks, &pack.moves, &fl, &adopted, now.utc_ms, p);
        let job_rows = coach::job_rows(&tasks, &pack.moves, &fl, &adopted, now.utc_ms, p);
        let guesses = store.type_guesses()?;
        let by_id: BTreeMap<&str, &JudgedTask> = tasks.iter().map(|t| (t.task_id.as_str(), t)).collect();
        let (check, label_check) = self_checks(&guesses, &by_id, &pack.coach);
        let label_jobs = label_jobs(store, &by_id, &pack.coach)?;
        let typed: Vec<&JudgedTask> =
            tasks.iter().filter(|t| t.first_ms >= since && !t.uncertain() && t.kind().is_some()).collect();
        let other = typed.iter().filter(|t| t.task_type.as_deref() == Some("other")).count();
        Ok(Cache {
            fingerprint: store.coach_fingerprint()?,
            guesses: guesses.len(),
            built_ms: now.utc_ms,
            build_ms: started.elapsed().as_millis() as u64,
            other_share: (!typed.is_empty()).then(|| other as f64 / typed.len() as f64),
            tasks,
            neighbours,
            rows,
            job_rows,
            habits,
            check,
            label_check,
            label_jobs,
            feedback: feedback(store)?,
            pauses: verdicts(&readings(store)?).into_iter().filter(|v| v.paused).collect(),
        })
    }

    /// The job-confidence bar for labels of `job`, and whether that job's labels may coach.
    pub fn label_job(&self, job: &str) -> (f64, bool) {
        self.label_jobs
            .get(job)
            .copied()
            .unwrap_or((aiq_domain::rung::COACH_THRESHOLD, !label::EARN_JOBS.contains(&job)))
    }

    pub fn stale(&self, store: &Store, now_ms: i64) -> bool {
        now_ms - self.built_ms > REBUILD_EVERY_MS || store.coach_fingerprint().ok() != Some(self.fingerprint)
    }

    /// The pause that stops a next-step tip for a task guessed as (job, kind), if any: only under `auto`, and
    /// not one the person kept on for the same evidence. The job's pause wins over the kind's.
    pub fn paused_for(&self, store: &Store, mode: &str, job: &str, task_type: &str) -> Option<&Verdict> {
        if mode != "auto" {
            return None;
        }
        let job_scope = Scope::Job(job.to_owned());
        let kind_scope = Scope::Kind(job.to_owned(), task_type.to_owned());
        [job_scope, kind_scope]
            .iter()
            .filter_map(|s| self.pauses.iter().find(|v| &v.scope == s))
            .find(|v| !kept_on(store, v))
    }

    /// Next-step tips are on now, from word guesses or (when labels can run) from labels.
    pub fn tips_on(&self, mode: &str, pack: &PackV1, labels_enabled: bool) -> bool {
        self.words_on(mode, pack) || (labels_enabled && self.labels_on(mode, pack))
    }

    /// Tips from word guesses: off when the person turned tips off, or (auto) AIQ's self-check of word guesses did.
    pub fn words_on(&self, mode: &str, pack: &PackV1) -> bool {
        on(mode, &self.check, pack)
    }

    /// Tips from labels: the same rule over the labels' own self-check.
    pub fn labels_on(&self, mode: &str, pack: &PackV1) -> bool {
        on(mode, &self.label_check, pack)
    }
}

/// The two self-checks (D10), words and labels, over stored guesses: one definition for the core and AIQ home
/// (DC-036). Labels coach by job, so a label is checked on its job only.
pub fn self_checks(
    guesses: &[aiq_store::TypeGuessRow],
    by_id: &BTreeMap<&str, &JudgedTask>,
    p: &aiq_contracts::pack::CoachParams,
) -> (GuessCheck, GuessCheck) {
    let check_of = |method: &str| {
        coach::check_guesses(
            guesses
                .iter()
                .filter(|g| g.method == method)
                .map(|g| (g.task_id.as_str(), g.job.as_str(), g.task_type.as_str())),
            by_id,
            method == label::METHOD,
            p,
        )
    };
    (check_of("words"), check_of(label::METHOD))
}

/// Per pack job, from its own labels (the newest answered label of each task, newest `2 × guess_check_min`
/// first) scored against the task's analysis: the bar it earned and whether it may coach. Every label counts here,
/// sure or not, so a lower bar is measured before it is used.
pub fn label_jobs(
    store: &Store,
    by_id: &BTreeMap<&str, &JudgedTask>,
    p: &aiq_contracts::pack::CoachParams,
) -> Result<BTreeMap<String, (f64, bool)>, StoreError> {
    let mut labels = store.kind_labels_since(0)?;
    labels.retain(|l| l.outcome == "ok");
    labels.sort_by_key(|l| std::cmp::Reverse(l.utc_ms));
    let mut seen = std::collections::BTreeSet::new();
    let mut points: BTreeMap<String, Vec<label::Point>> = BTreeMap::new();
    for l in labels {
        let (Some(job), Some(c)) = (l.job.as_deref(), l.job_confidence) else { continue };
        if !seen.insert(l.task_id.clone()) {
            continue;
        }
        let Some(t) = by_id.get(l.task_id.as_str()) else { continue };
        if !t.answer_key() || !aiq_domain::rung::PACK_JOBS.contains(&t.job.as_str()) {
            continue;
        }
        let v = points.entry(job.to_owned()).or_default();
        if v.len() < 2 * p.guess_check_min as usize {
            v.push((c, t.job == job));
        }
    }
    let (min, precision) = (p.guess_check_min, p.guess_precision);
    Ok(aiq_domain::rung::PACK_JOBS
        .iter()
        .map(|job| {
            let pts = points.get(*job).map(Vec::as_slice).unwrap_or(&[]);
            let bar = label::live_bar(job, pts, min, precision);
            (job.to_string(), (bar, label::earned(job, pts, bar, min, precision)))
        })
        .collect())
}

fn on(mode: &str, check: &GuessCheck, pack: &PackV1) -> bool {
    match mode {
        "off" => false,
        "on" => true,
        _ => !check.auto_off(&pack.coach),
    }
}

fn plain(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_owned()
}

/// Record one answer to a tip. `card_id` names the card (its tip id is read from it), or `tip_id` names the
/// tip directly (Settings "Turn back on"). Returns the tip id.
pub fn record_feedback(
    store: &Store,
    card_id: Option<&str>,
    tip_id: Option<&str>,
    answer: &str,
    surface: &str,
    now_ms: i64,
) -> Result<String, String> {
    if !ANSWERS.contains(&answer) {
        return Err(format!("answer must be one of: {}", ANSWERS.join(", ")));
    }
    let tip = match (card_id, tip_id) {
        (Some(c), _) => {
            let rec = store.card(c).map_err(|e| e.code.to_string())?.ok_or_else(|| format!("no card {c}"))?;
            serde_json::from_str::<aiq_contracts::card::CardV1>(&rec.card_json)
                .map_err(|_| format!("card {c} is unreadable"))?
                .tip_id
        }
        (None, Some(t))
            if !t.is_empty() && t.len() <= 80 && t.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) =>
        {
            t.to_owned()
        }
        _ => return Err("name a card or a tip".into()),
    };
    store
        .insert_tip_feedback(&TipFeedbackRow {
            card_id: card_id.map(str::to_owned),
            tip_id: tip.clone(),
            answer: answer.to_owned(),
            surface: surface.to_owned(),
            utc_ms: now_ms,
        })
        .map_err(|e| e.code.to_string())?;
    Ok(tip)
}

/// Record "This doesn't fit" (action `flag`) or its undo, for one (job, task type, move).
#[allow(clippy::too_many_arguments)]
pub fn record_flag(
    store: &Store,
    pack: &PackV1,
    job: &str,
    task_type: &str,
    move_id: &str,
    action: &str,
    reason: Option<&str>,
    note: Option<&str>,
    now_ms: i64,
) -> Result<(), String> {
    let job_level = task_type == coach::ANY_KIND && aiq_domain::rung::PACK_JOBS.contains(&job);
    if task_type_label(job, task_type).is_none() && !job_level {
        return Err(format!("{job}/{task_type} is not a task type"));
    }
    if !pack.moves.iter().any(|m| m.move_id == move_id) {
        return Err(format!("no move {move_id}"));
    }
    let reason = match action {
        "flag" => match reason {
            Some(r) if FLAG_REASONS.contains(&r) => Some(r.to_owned()),
            _ => return Err(format!("reason must be one of: {}", FLAG_REASONS.join(", "))),
        },
        "undo" => None,
        _ => return Err("action must be flag or undo".into()),
    };
    let note = note.map(plain).filter(|n| !n.is_empty());
    if note.as_ref().is_some_and(|n| n.chars().count() > MAX_NOTE) {
        return Err(format!("a note is at most {MAX_NOTE} characters"));
    }
    store
        .insert_guidance_flag(&GuidanceFlagRow {
            job: job.to_owned(),
            task_type: task_type.to_owned(),
            move_id: move_id.to_owned(),
            action: action.to_owned(),
            reason,
            note,
            utc_ms: now_ms,
        })
        .map_err(|e| e.code.to_string())
}

/// Today's kind labels and their self-check, counted from stored rows (design kind-label).
pub fn label_status(store: &Store, cache: &Cache, now: Now) -> Result<aiq_contracts::frames::LabelStatus, StoreError> {
    let rows = store.kind_labels_since(now.local_midnight_utc_ms())?;
    let mut ms: Vec<u64> = rows.iter().map(|r| r.latency_ms.max(0) as u64).collect();
    ms.sort_unstable();
    let ok: Vec<_> = rows.iter().filter(|r| r.outcome == "ok").collect();
    let confident = ok
        .iter()
        .filter(|r| {
            let job = r.job.clone().unwrap_or_default();
            let (bar, earned) = cache.label_job(&job);
            earned
                && label::accept(
                    &label::Label {
                        job,
                        job_confidence: r.job_confidence.unwrap_or(0.0),
                        task_type: r.task_type.clone().unwrap_or_default(),
                        task_type_confidence: r.task_type_confidence.unwrap_or(0.0),
                    },
                    bar,
                )
                .is_some()
        })
        .count();
    Ok(aiq_contracts::frames::LabelStatus {
        enabled: labels_enabled(store),
        today: rows.len() as u32,
        daily: daily_labels(store),
        answered: ok.len() as u32,
        confident: confident as u32,
        median_ms: (!ms.is_empty()).then(|| ms[ms.len() / 2]),
        checked: cache.label_check.checked,
        right: cache.label_check.right,
    })
}

/// The daily label budget (setting `kind_label_daily`, default `label::DEFAULT_DAILY`).
pub fn daily_labels(store: &Store) -> u32 {
    store
        .setting(aiq_domain::label::SETTING_LABEL_DAILY)
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(aiq_domain::label::DEFAULT_DAILY)
}

/// Labels can run by the person's settings: background judging is on and labels are not turned off.
pub fn labels_enabled(store: &Store) -> bool {
    let get = |k: &str| store.setting(k).ok().flatten();
    aiq_domain::label::enabled(
        get(aiq_domain::rung::SETTING_JUDGE).as_deref(),
        get(aiq_domain::label::SETTING_LABEL).as_deref(),
    )
}
