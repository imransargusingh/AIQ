//! `aiq label`: name a new task's kind from its first prompt with a small model (design
//! `docs/design/kind-label.md`).
//!
//! The core starts `aiq label --task <id> --surface <s> --session <id> --requested <ms>` detached when its word
//! guess finds no kind, and never waits for it (C9). This process makes one call through the person's own tool
//! that already saw the task, sealed exactly like the judge (same `Provider`s), stores one `kind_label` row,
//! and tells the core, which decides the next-step tip as it would from a word guess. Within a daily budget
//! of its own (`kind-label-daily`), and only while background judging is on.
//!
//! `aiq label --eval [--sample <n>] [--only claude-code|copilot-cli] [--dry-run]` measures the label against
//! the judge on already-judged tasks before anyone relies on it. It stores nothing.
use crate::judge::{self, CallReq, ClaudeCli, CopilotCli, Outcome, Provider};
use aiq_contracts::frames::Request;
use aiq_core::paths::{Paths, bounded_append};
use aiq_domain::clock::Now;
use aiq_domain::label;
use aiq_domain::rung;
use aiq_store::{KindLabelRow, Store};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LOG_MAX: u64 = 256 * 1024;
/// A Copilot call home left in `label-work` older than this belongs to no live call (calls time out at 120 s).
const STALE_MS: i64 = 10 * 60_000;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn now() -> Now {
    let offset = chrono::Local::now().offset().local_minus_utc() / 60;
    Now { utc_ms: now_ms(), offset_min: offset }
}

/// The models labels use: Claude's small model unless the person names one; for Copilot the models analysis
/// uses (`copilot_models`), so a label and the analysis it is checked against read the task on one model.
pub fn providers(paths: &Paths, store: &Store) -> (ClaudeCli, CopilotCli) {
    let get = |k: &str| store.setting(k).ok().flatten();
    let set = judge::settings(store);
    let (mut claude, mut copilot) = judge::providers(paths, &set);
    claude.model = get(label::SETTING_LABEL_CLAUDE_MODEL).unwrap_or_else(|| label::DEFAULT_CLAUDE_MODEL.into());
    copilot.model = copilot_models(store).into_iter().next().flatten();
    // Its own work folder: the judge sweeps `judge-work` while it holds its lock, which a label never takes.
    copilot.work = paths.data.join("label-work");
    (claude, copilot)
}

/// The Copilot models a label tries, in order: the person's label model alone (`default` = Copilot's own
/// pick), else the analysis models (AIQ's default model, then Copilot's own pick). Before this change a person
/// with no Copilot model set got Copilot's own pick for labels while analysis used AIQ's default (DC-039).
pub fn copilot_models(store: &Store) -> Vec<Option<String>> {
    match store.setting(label::SETTING_LABEL_COPILOT_MODEL).ok().flatten() {
        Some(m) if m == "default" => vec![None],
        Some(m) => vec![Some(m)],
        None => judge::analysis_copilot_models(&judge::settings(store)),
    }
}

/// What one request asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct Req {
    pub task_id: String,
    pub surface: String,
    pub session: String,
    pub requested_ms: i64,
    /// A later prompt of the task to label instead of its first: the one second try when the first label was not
    /// sure enough (design kind-label, "Second try").
    pub prompt_id: Option<String>,
}

/// Labels per task at most: the first prompt's, and one second try.
pub const MAX_LABELS_PER_TASK: usize = 2;

/// Why no call was made, for label.log.
#[derive(Debug, Clone, PartialEq)]
pub enum Skip {
    Off,
    Budget(u32),
    AlreadyLabelled,
    NoText,
    NoProvider,
}

/// One label: checks, one call, one stored row. The row is stored whatever the call's outcome.
pub fn label_task(
    store: &Store,
    paths: &Paths,
    claude: &dyn Provider,
    copilot: &dyn Provider,
    r: &Req,
    n: Now,
) -> aiq_store::Result<Result<KindLabelRow, Skip>> {
    let get = |k: &str| store.setting(k).ok().flatten();
    if !label::enabled(get(rung::SETTING_JUDGE).as_deref(), get(label::SETTING_LABEL).as_deref()) {
        return Ok(Err(Skip::Off));
    }
    let daily = aiq_core::coaching::daily_labels(store);
    if store.kind_labels_since(n.local_midnight_utc_ms())?.len() as u32 >= daily {
        return Ok(Err(Skip::Budget(daily)));
    }
    let have = store.kind_labels_of(&r.task_id)?.len();
    if have >= if r.prompt_id.is_some() { MAX_LABELS_PER_TASK } else { 1 } {
        return Ok(Err(Skip::AlreadyLabelled));
    }
    let Some(name) = judge::provider_for(&r.surface) else { return Ok(Err(Skip::NoProvider)) };
    // A live prompt's text is found by its event id, as the task's first prompt is by the task id.
    let Some((_, Some(text))) = store.task_first_prompt(r.prompt_id.as_deref().unwrap_or(&r.task_id))? else {
        return Ok(Err(Skip::NoText));
    };
    let provider = if name == "claude-code" { claude } else { copilot };
    let session_id = uuid::Uuid::new_v4().to_string();
    if name == "copilot-cli" && judge::record_judge_session(paths, &session_id).is_err() {
        // Never start a Copilot call AIQ could later mistake for the person's own session (US-11).
        return Ok(Err(Skip::NoProvider));
    }
    let fence = format!("AIQ-QUOTE-{}", uuid::Uuid::new_v4().simple());
    let (system, user, schema) = (label::system(), label::envelope(&r.surface, &text, &fence), label::schema());
    // The call starts as this process starts (`n`), so the budget's day is the request's day.
    let started = n.utc_ms;
    let res = provider.call(&CallReq { system: &system, user: &user, schema: &schema, session_id: &session_id });
    let (outcome, parsed) = match &res.outcome {
        Outcome::Ok(v) => match label::parse(v) {
            Ok(l) => ("ok", Some(l)),
            Err(_) => ("schema", None),
        },
        o => (outcome_name(o), None),
    };
    let row = KindLabelRow {
        label_id: uuid::Uuid::now_v7().to_string(),
        task_id: r.task_id.clone(),
        surface: r.surface.clone(),
        surface_session_id: r.session.clone(),
        provider: name.to_owned(),
        model: res.model.clone(),
        outcome: outcome.to_owned(),
        job: parsed.as_ref().map(|l| l.job.clone()),
        job_confidence: parsed.as_ref().map(|l| l.job_confidence),
        task_type: parsed.as_ref().map(|l| l.task_type.clone()),
        task_type_confidence: parsed.as_ref().map(|l| l.task_type_confidence),
        requested_utc_ms: r.requested_ms,
        started_utc_ms: started,
        latency_ms: res.latency_ms,
        tokens_in: res.tokens_in,
        tokens_out: res.tokens_out,
        cost_usd: res.cost_usd,
        utc_ms: started + res.latency_ms,
    };
    store.insert_kind_label(&row)?;
    Ok(Ok(row))
}

fn outcome_name(o: &Outcome) -> &'static str {
    match o {
        Outcome::Ok(_) => "ok",
        Outcome::Schema(_) => "schema",
        Outcome::Error(_) => "error",
        Outcome::Timeout => "timeout",
        Outcome::NotSignedIn(_) => "not_signed_in",
        Outcome::Missing => "missing",
    }
}

/// Remove Copilot call homes and usage files a killed label call left (they hold the first prompt's text).
fn sweep(paths: &Paths, now: i64) -> usize {
    let Ok(rd) = std::fs::read_dir(paths.data.join("label-work")) else { return 0 };
    let mut n = 0;
    for e in rd.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .is_some_and(|d| now - d.as_millis() as i64 > STALE_MS);
        let name = e.file_name().to_string_lossy().into_owned();
        if !old {
            continue;
        }
        let gone = if name.starts_with("copilot-") {
            std::fs::remove_dir_all(e.path()).is_ok()
        } else if name.starts_with("usage-") {
            std::fs::remove_file(e.path()).is_ok()
        } else {
            false
        };
        n += usize::from(gone);
    }
    n
}

fn arg<'a>(args: &[&'a str], name: &str) -> Option<&'a str> {
    args.iter().position(|a| *a == name).and_then(|i| args.get(i + 1)).copied()
}

/// `aiq label …`
pub fn cli(args: &[&str]) -> ExitCode {
    let paths = Paths::from_env();
    let store = match Store::open(&paths.db()) {
        Ok(s) => s,
        Err(e) => {
            println!("AIQ's store could not be opened ({}).", e.code);
            return ExitCode::from(1);
        }
    };
    if args.contains(&"--eval") && args.contains(&"--golden") {
        return golden_cli(args, &paths, &store);
    }
    if args.contains(&"--eval") {
        return eval_cli(args, &paths, &store);
    }
    let (Some(task_id), Some(surface), Some(session)) =
        (arg(args, "--task"), arg(args, "--surface"), arg(args, "--session"))
    else {
        println!(
            "aiq label is started by AIQ itself. To measure labels on your analyzed tasks: aiq label --eval [--sample <n>]"
        );
        return ExitCode::from(2);
    };
    let n = now();
    let _ = sweep(&paths, n.utc_ms);
    let req = Req {
        task_id: task_id.to_owned(),
        surface: surface.to_owned(),
        session: session.to_owned(),
        requested_ms: arg(args, "--requested").and_then(|v| v.parse().ok()).unwrap_or(n.utc_ms),
        prompt_id: arg(args, "--prompt").map(str::to_owned),
    };
    let (claude, copilot) = providers(&paths, &store);
    let copilot = judge::copilot_chain(&copilot, copilot_models(&store));
    let line = match label_task(&store, &paths, &claude, &copilot, &req, n) {
        Ok(Ok(row)) => {
            notify_core(&paths, &row.task_id);
            format!(
                "label {} {} {} {}/{} ms={} wait_ms={}",
                row.task_id,
                row.provider,
                row.outcome,
                row.job.as_deref().unwrap_or("-"),
                row.task_type.as_deref().unwrap_or("-"),
                row.latency_ms,
                row.utc_ms - row.requested_utc_ms
            )
        }
        Ok(Err(skip)) => format!("label {} skipped {skip:?}", req.task_id),
        Err(e) => format!("label {} failed {}", req.task_id, e.code),
    };
    let _ = bounded_append(&paths.data.join("label.log"), &line, LOG_MAX);
    ExitCode::SUCCESS
}

/// What the core calls for a new task the word guess could not name: start `aiq label` detached, on a thread
/// of its own so the core's writer never waits on process start.
pub fn requester() -> aiq_core::service::LabelRequester {
    Box::new(|r: aiq_core::service::LabelRequest| {
        std::thread::spawn(move || {
            let at = r.requested_ms.to_string();
            let mut args = vec![
                "label",
                "--task",
                &r.task_id,
                "--surface",
                &r.surface,
                "--session",
                &r.session,
                "--requested",
                &at,
            ];
            if let Some(p) = &r.prompt_id {
                args.extend(["--prompt", p.as_str()]);
            }
            let _ = crate::spawn::detached(&args);
        });
    })
}

/// Tell the core a label is stored. No reply is awaited; with the core down the row simply waits unused.
fn notify_core(paths: &Paths, task_id: &str) {
    let Some(token) = aiq_core::ipc::read_token(paths) else { return };
    let req = Request::KindLabel { token, task_id: task_id.to_owned() };
    if let Ok(frame) = serde_json::to_string(&req) {
        let _ = aiq_core::ipc::request(paths, &frame, false, Duration::from_secs(2));
    }
}

// ------------------------------------------------------------------ offline measure

/// One judged task to label in the measure: the judge's answer and the first prompt.
#[derive(Debug, Clone)]
pub struct EvalCase {
    pub provider: &'static str,
    pub job: String,
    pub task_type: String,
    pub text: String,
}

/// The newest `n` current, certain, typed judgments of tasks whose first prompt text AIQ still has.
pub fn eval_cases(store: &Store, n: usize, only: Option<&str>) -> aiq_store::Result<Vec<EvalCase>> {
    let mut js: Vec<_> = store
        .current_judgments()?
        .into_values()
        .filter(|j| j.state != "uncertain" && j.task_type.is_some())
        .filter(|j| only.is_none_or(|o| o == j.provider))
        .collect();
    js.sort_by_key(|j| std::cmp::Reverse(j.judged_utc_ms));
    let mut out = Vec::new();
    for j in js {
        if out.len() >= n {
            break;
        }
        let Some(p) = judge::provider_for(&j.provider) else { continue };
        let Some((_, Some(text))) = store.task_first_prompt(&j.task_id)? else { continue };
        out.push(EvalCase { provider: p, job: j.job, task_type: j.task_type.unwrap_or_default(), text });
    }
    Ok(out)
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Eval {
    pub sent: u32,
    pub answered: u32,
    pub job_right: u32,
    pub kind_right: u32,
    /// Labels confident enough to coach from (`label::accept`), and how many of those named the judge's kind.
    pub confident: u32,
    pub confident_right: u32,
    pub latencies: Vec<i64>,
    /// Each confident label that named another kind than the analysis: (label, analysis), as `job/kind`.
    pub misses: Vec<(String, String)>,
}

/// The bar a label must clear to coach: the live self-check's 8 in 10 (`coach::check_guesses`), over at least
/// `BAR_MIN` confident labels so a handful of lucky answers can't pass.
pub const BAR_MIN: u32 = 10;

impl Eval {
    /// None: too few confident labels to say.
    pub fn passes(&self) -> Option<bool> {
        (self.confident >= BAR_MIN).then_some(self.confident_right * 10 >= self.confident * 8)
    }
}

pub fn run_eval(cases: &[EvalCase], claude: &dyn Provider, copilot: &dyn Provider) -> Eval {
    let mut e = Eval::default();
    let (system, schema) = (label::system(), label::schema());
    for c in cases {
        let p = if c.provider == "claude-code" { claude } else { copilot };
        let fence = format!("AIQ-QUOTE-{}", uuid::Uuid::new_v4().simple());
        let user = label::envelope(c.provider, &c.text, &fence);
        let sid = uuid::Uuid::new_v4().to_string();
        e.sent += 1;
        let r = p.call(&CallReq { system: &system, user: &user, schema: &schema, session_id: &sid });
        e.latencies.push(r.latency_ms);
        let Outcome::Ok(v) = &r.outcome else { continue };
        let Ok(l) = label::parse(v) else { continue };
        e.answered += 1;
        let job_ok = l.job == c.job;
        let kind_ok = job_ok && l.task_type == c.task_type;
        e.job_right += u32::from(job_ok);
        e.kind_right += u32::from(kind_ok);
        if label::accept(&l).is_some() {
            e.confident += 1;
            e.confident_right += u32::from(kind_ok);
            if !kind_ok {
                e.misses.push((format!("{}/{}", l.job, l.task_type), format!("{}/{}", c.job, c.task_type)));
            }
        }
    }
    e
}

pub fn eval_report(e: &Eval) -> String {
    let bar = match e.passes() {
        Some(true) => "Clears the 8-in-10 bar: yes\n".to_owned(),
        Some(false) => "Clears the 8-in-10 bar: NO\n".to_owned(),
        None => format!("Clears the 8-in-10 bar: can't say yet (fewer than {BAR_MIN} confident labels)\n"),
    };
    let misses: String = e.misses.iter().map(|(l, a)| format!("  labelled {l}, analysis said {a}\n")).collect();
    let mut ms = e.latencies.clone();
    ms.sort_unstable();
    let med = ms.get(ms.len() / 2).map_or("-".to_owned(), |m| format!("{:.1} s", *m as f64 / 1000.0));
    let max = ms.last().map_or("-".to_owned(), |m| format!("{:.1} s", *m as f64 / 1000.0));
    format!(
        "Labelled {} of your analyzed tasks from their first prompt only (measured; the analysis read the whole task).\n\
         Answered: {} of {}\n\
         Job the same as the analysis: {} of {}\n\
         Kind the same as the analysis: {} of {}\n\
         Confident enough to coach from (both at least 0.8, a named kind): {}; of those, the analyzed kind: {} of {}\n\
         Call time: median {med}, longest {max}\n\
         {bar}{misses}\
         Live, a confident label coaches only while AIQ's own check of labels stays at 8 in 10 or better.\n",
        e.sent,
        e.answered,
        e.sent,
        e.job_right,
        e.answered,
        e.kind_right,
        e.answered,
        e.confident,
        e.confident_right,
        e.confident
    )
}

fn eval_cli(args: &[&str], paths: &Paths, store: &Store) -> ExitCode {
    let n = arg(args, "--sample").and_then(|v| v.parse::<usize>().ok()).unwrap_or(20).clamp(1, 200);
    let only = arg(args, "--only");
    if only.is_some_and(|o| o != "claude-code" && o != "copilot-cli") {
        println!("--only takes claude-code or copilot-cli.");
        return ExitCode::from(2);
    }
    let cases = match eval_cases(store, n, only) {
        Ok(c) => c,
        Err(e) => {
            println!("Couldn't read your analyzed tasks ({}).", e.code);
            return ExitCode::from(1);
        }
    };
    let copilot = cases.iter().filter(|c| c.provider == "copilot-cli").count();
    println!(
        "AIQ would label {} analyzed tasks from their first prompt: {} through your Copilot CLI (one premium request \
         each), {} through your Claude Code. Outside the daily budgets; nothing is stored.",
        cases.len(),
        copilot,
        cases.len() - copilot
    );
    if args.contains(&"--dry-run") || cases.is_empty() {
        println!("Nothing was sent.");
        return ExitCode::SUCCESS;
    }
    let (claude, mut copilot_cli) = providers(paths, store);
    copilot_cli.keep_sessions = false;
    let copilot_cli = judge::copilot_chain(&copilot_cli, copilot_models(store));
    let e = run_eval(&cases, &claude, &copilot_cli);
    let report = eval_report(&e);
    print!("{report}");
    let code = if e.passes() == Some(false) { ExitCode::from(3) } else { ExitCode::SUCCESS };
    let _ = bounded_append(
        &paths.data.join("label.log"),
        &format!(
            "eval {} sent={} answered={} job={} kind={} confident={}/{}",
            now_ms(),
            e.sent,
            e.answered,
            e.job_right,
            e.kind_right,
            e.confident_right,
            e.confident
        ),
        LOG_MAX,
    );
    let _ = sweep(paths, now_ms() + 2 * STALE_MS);
    code
}

// ------------------------------------------------------------------ synthetic golden set

/// The label's golden set: synthetic first prompts with the job and kind the whole task turns out to be.
pub const GOLDEN: &str = include_str!("../../../evals/label/golden.json");
/// Recorded label answers for the golden set, replayed by CI (`label_replay_gate`).
#[cfg(test)]
pub const REPLAY: &str = include_str!("../../../evals/label/replay.json");

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Golden {
    pub version: String,
    pub cases: Vec<GoldenCase>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GoldenCase {
    pub id: String,
    pub tool: String,
    pub prompt: String,
    pub expect: GoldenExpect,
}

/// Accepted values. A kind of `["other"]` means no named kind is right, so a confident label is wrong.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GoldenExpect {
    pub job: Vec<String>,
    pub kind: Vec<String>,
}

pub fn golden() -> Golden {
    serde_json::from_str(GOLDEN).expect("evals/label/golden.json parses")
}

/// Score answers to golden cases (case id, answer) the way the live self-check scores a label: only confident
/// labels count toward the bar; a confident label is right when its job and kind are both accepted.
pub fn score_golden(g: &Golden, answers: &[(String, serde_json::Value)]) -> Eval {
    let by_id: std::collections::HashMap<&str, &GoldenCase> = g.cases.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut e = Eval::default();
    for (id, v) in answers {
        let Some(c) = by_id.get(id.as_str()) else { continue };
        e.sent += 1;
        let Ok(l) = label::parse(v) else { continue };
        e.answered += 1;
        let job_ok = c.expect.job.contains(&l.job);
        let kind_ok = job_ok && c.expect.kind.contains(&l.task_type);
        e.job_right += u32::from(job_ok);
        e.kind_right += u32::from(kind_ok);
        if label::accept(&l).is_some() {
            e.confident += 1;
            e.confident_right += u32::from(kind_ok);
            if !kind_ok {
                e.misses.push((format!("{id}: {}/{}", l.job, l.task_type), c.expect.kind.join("|")));
            }
        }
    }
    e
}

/// `aiq label --eval --golden --with copilot-cli|claude-code [--runs n] [--record <path>]`: label every golden
/// case through one of the person's tools, `runs` times. Reads and writes nothing in the store.
fn golden_cli(args: &[&str], paths: &Paths, store: &Store) -> ExitCode {
    let with = arg(args, "--with").unwrap_or("copilot-cli");
    if with != "claude-code" && with != "copilot-cli" {
        println!("--with takes claude-code or copilot-cli.");
        return ExitCode::from(2);
    }
    let runs = arg(args, "--runs").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).clamp(1, 5);
    let g = golden();
    println!(
        "AIQ would label {} synthetic first prompts {runs} time(s) through your {with}{}.",
        g.cases.len(),
        if with == "copilot-cli" { " (one premium request each)" } else { "" }
    );
    if args.contains(&"--dry-run") {
        println!("Nothing was sent.");
        return ExitCode::SUCCESS;
    }
    let (claude, mut copilot_cli) = providers(paths, store);
    copilot_cli.keep_sessions = false;
    let copilot = judge::copilot_chain(&copilot_cli, copilot_models(store));
    let p: &dyn Provider = if with == "claude-code" { &claude } else { &copilot };
    let (system, schema) = (label::system(), label::schema());
    let mut answers = Vec::new();
    let mut models = std::collections::BTreeSet::new();
    let mut latencies = Vec::new();
    for _ in 0..runs {
        for c in &g.cases {
            let fence = format!("AIQ-QUOTE-{}", uuid::Uuid::new_v4().simple());
            let user = label::envelope(&c.tool, &c.prompt, &fence);
            let sid = uuid::Uuid::new_v4().to_string();
            if with == "copilot-cli" && judge::record_judge_session(paths, &sid).is_err() {
                println!("Couldn't record AIQ's own session id; nothing more was sent.");
                return ExitCode::from(1);
            }
            let r = p.call(&CallReq { system: &system, user: &user, schema: &schema, session_id: &sid });
            latencies.push(r.latency_ms);
            if let Some(m) = &r.model {
                models.insert(m.clone());
            }
            let v = match r.outcome {
                Outcome::Ok(v) => v,
                _ => serde_json::Value::Null,
            };
            answers.push((c.id.clone(), v));
        }
    }
    let mut e = score_golden(&g, &answers);
    e.latencies = latencies;
    let report = eval_report(&e)
        .replace("of your analyzed tasks", "synthetic golden prompts")
        .replace("the same as the analysis", "as expected")
        .replace("the analyzed kind", "the expected kind")
        .replace("analysis said", "expected")
        .replace(" (measured; the analysis read the whole task)", "");
    print!("{report}");
    if let Some(path) = arg(args, "--record") {
        let rec = replay_file(&g, with, &models.into_iter().collect::<Vec<_>>().join(","), runs, &answers, &e);
        if let Err(err) = std::fs::write(path, serde_json::to_string_pretty(&rec).unwrap_or_default() + "\n") {
            println!("Couldn't write {path} ({err}).");
            return ExitCode::from(1);
        }
        println!("Recorded to {path}.");
    }
    let _ = sweep(paths, now_ms() + 2 * STALE_MS);
    if e.passes() == Some(false) { ExitCode::from(3) } else { ExitCode::SUCCESS }
}

fn totals_json(e: &Eval) -> serde_json::Value {
    serde_json::json!({
        "sent": e.sent, "answered": e.answered, "job_right": e.job_right, "kind_right": e.kind_right,
        "confident": e.confident, "confident_right": e.confident_right,
    })
}

/// What `--record` writes: the prompt it was recorded with (CI refuses the answers once the prompt changes),
/// the answers by case, and their totals.
pub fn replay_file(
    g: &Golden,
    provider: &str,
    model: &str,
    runs: usize,
    answers: &[(String, serde_json::Value)],
    e: &Eval,
) -> serde_json::Value {
    let mut by: std::collections::BTreeMap<&str, Vec<&serde_json::Value>> = Default::default();
    for (id, v) in answers {
        by.entry(id.as_str()).or_default().push(v);
    }
    serde_json::json!({
        "about": "Recorded label answers for evals/label/golden.json, replayed by CI through today's parser and \
                  acceptance rule (label_replay_gate). Re-record after any label prompt change with: aiq label \
                  --eval --golden --with copilot-cli --runs 2 --record evals/label/replay.json",
        "recorded_utc": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "prompt_version": label::PROMPT_VERSION,
        "golden": g.version,
        "provider": provider, "model": model, "runs": runs,
        "system": label::system(),
        "totals": totals_json(e),
        "answers": by,
    })
}

/// The recorded answers as (case id, answer) pairs.
#[cfg(test)]
pub fn replay_answers(rec: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
    let mut out = Vec::new();
    if let Some(m) = rec["answers"].as_object() {
        for (id, vs) in m {
            for v in vs.as_array().into_iter().flatten() {
                out.push((id.clone(), v.clone()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::CallResult;
    use aiq_domain::clock::DAY_MS;
    use aiq_store::NewEvent;
    use serde_json::{Value, json};
    use std::cell::RefCell;
    use tempfile::TempDir;

    const NOW: i64 = 100 * DAY_MS;

    struct Fake {
        answer: Outcome,
        sent: RefCell<Vec<String>>,
    }

    impl Fake {
        fn ok(v: Value) -> Self {
            Fake { answer: Outcome::Ok(v), sent: RefCell::new(vec![]) }
        }
    }

    impl Provider for Fake {
        fn call(&self, req: &CallReq<'_>) -> CallResult {
            self.sent.borrow_mut().push(format!("{}\n{}", req.system, req.user));
            CallResult {
                outcome: self.answer.clone(),
                model: Some("small".into()),
                latency_ms: 4_000,
                cost_usd: None,
                tokens_in: Some(900),
                tokens_out: Some(30),
            }
        }
    }

    fn setup() -> (TempDir, Paths, Store) {
        let d = TempDir::new().unwrap();
        let paths = Paths::at(d.path());
        let mut store = Store::open(&d.path().join("aiq.db")).unwrap();
        store.set_setting(rung::SETTING_JUDGE, "auto", NOW).unwrap();
        for (id, s) in [("t1", "s1"), ("t2", "s2"), ("t3", "s3")] {
            store
                .insert_event(
                    &NewEvent {
                        event_id: id,
                        surface: "copilot-cli",
                        session: s,
                        kind: "prompt",
                        ts_utc_ms: NOW,
                        prompt_id: None,
                        trace_id: None,
                        collector_version: "t",
                        prompt_chars: Some(30),
                        payload_meta: "{}",
                        text: Some("the login test fails since the upgrade, fix it"),
                    },
                    NOW,
                )
                .unwrap();
        }
        (d, paths, store)
    }

    fn req(id: &str) -> Req {
        Req {
            task_id: id.into(),
            surface: "copilot-cli".into(),
            session: "s1".into(),
            requested_ms: NOW,
            prompt_id: None,
        }
    }

    fn n() -> Now {
        Now { utc_ms: NOW + 1000, offset_min: 0 }
    }

    fn answer() -> Value {
        json!({"job": "code_change", "job_confidence": 0.9, "task_type": "fix_bug", "task_type_confidence": 0.85})
    }

    #[test]
    fn one_call_to_the_tool_that_saw_the_task_one_stored_row_with_ids_and_numbers_only() {
        let (_d, paths, store) = setup();
        let (claude, copilot) = (Fake::ok(answer()), Fake::ok(answer()));
        let row = label_task(&store, &paths, &claude, &copilot, &req("t1"), n()).unwrap().unwrap();
        assert_eq!((claude.sent.borrow().len(), copilot.sent.borrow().len()), (0, 1), "Copilot task → Copilot");
        assert_eq!(row.outcome, "ok");
        assert_eq!((row.job.as_deref(), row.task_type.as_deref()), (Some("code_change"), Some("fix_bug")));
        assert_eq!(row.latency_ms, 4_000);
        let sent = copilot.sent.borrow()[0].clone();
        assert!(sent.contains("the login test fails") && sent.contains("START OF QUOTED FIRST PROMPT"));
        assert_eq!(store.kind_labels_of("t1").unwrap(), vec![row]);
        // The task id was recorded as AIQ's own Copilot session before the call (US-11).
        assert!(std::fs::read_to_string(paths.data.join("judge-sessions.txt")).unwrap().lines().count() == 1);
        assert_eq!(
            label_task(&store, &paths, &claude, &copilot, &req("t1"), n()).unwrap(),
            Err(Skip::AlreadyLabelled),
            "a task is labelled once"
        );
    }

    #[test]
    fn a_second_try_labels_the_later_prompt_once_and_never_a_third_time() {
        let (_d, paths, mut store) = setup();
        store
            .insert_event(
                &NewEvent {
                    event_id: "t1-later",
                    surface: "copilot-cli",
                    session: "s1",
                    kind: "prompt",
                    ts_utc_ms: NOW + 60_000,
                    prompt_id: None,
                    trace_id: None,
                    collector_version: "t",
                    prompt_chars: Some(40),
                    payload_meta: "{}",
                    text: Some("the checkout page double-charges on retry, fix the payment handler"),
                },
                NOW,
            )
            .unwrap();
        let f = Fake::ok(answer());
        label_task(&store, &paths, &f, &f, &req("t1"), n()).unwrap().unwrap();
        let again = Req { prompt_id: Some("t1-later".into()), ..req("t1") };
        let row = label_task(&store, &paths, &f, &f, &again, n()).unwrap().unwrap();
        assert_eq!(row.task_id, "t1", "the second label is the task's, so its self-check reads the task's analysis");
        assert!(f.sent.borrow()[1].contains("double-charges on retry"), "the later prompt is what was sent");
        assert!(!f.sent.borrow()[1].contains("the login test fails"));
        assert_eq!(store.kind_labels_of("t1").unwrap().len(), 2);
        assert_eq!(label_task(&store, &paths, &f, &f, &again, n()).unwrap(), Err(Skip::AlreadyLabelled), "no third");
        assert_eq!(f.sent.borrow().len(), 2);
    }

    #[test]
    fn nothing_is_sent_with_judging_off_labels_off_or_the_budget_spent() {
        let (_d, paths, store) = setup();
        let f = Fake::ok(answer());
        store.set_setting(label::SETTING_LABEL, "off", NOW).unwrap();
        assert_eq!(label_task(&store, &paths, &f, &f, &req("t1"), n()).unwrap(), Err(Skip::Off));
        store.set_setting(label::SETTING_LABEL, "auto", NOW).unwrap();
        store.set_setting(rung::SETTING_JUDGE, "off", NOW).unwrap();
        assert_eq!(label_task(&store, &paths, &f, &f, &req("t1"), n()).unwrap(), Err(Skip::Off));
        store.set_setting(rung::SETTING_JUDGE, "auto", NOW).unwrap();
        store.set_setting(label::SETTING_LABEL_DAILY, "1", NOW).unwrap();
        assert!(label_task(&store, &paths, &f, &f, &req("t1"), n()).unwrap().is_ok());
        assert_eq!(label_task(&store, &paths, &f, &f, &req("t2"), n()).unwrap(), Err(Skip::Budget(1)));
        // Tomorrow the budget is new.
        let tomorrow = Now { utc_ms: NOW + DAY_MS, offset_min: 0 };
        assert!(label_task(&store, &paths, &f, &f, &req("t2"), tomorrow).unwrap().is_ok());
        assert_eq!(f.sent.borrow().len(), 2);
        store.set_setting(label::SETTING_LABEL_DAILY, "5", NOW).unwrap();
        assert_eq!(label_task(&store, &paths, &f, &f, &req("nope"), tomorrow).unwrap(), Err(Skip::NoText));
    }

    #[test]
    fn a_failed_or_malformed_answer_is_stored_as_such_and_counts_against_the_budget() {
        let (_d, paths, store) = setup();
        let bad = Fake::ok(
            json!({"job": "code_change", "job_confidence": 0.9, "task_type": "release", "task_type_confidence": 0.9}),
        );
        let row = label_task(&store, &paths, &bad, &bad, &req("t1"), n()).unwrap().unwrap();
        assert_eq!((row.outcome.as_str(), row.job.as_deref()), ("schema", None));
        let down = Fake { answer: Outcome::Timeout, sent: RefCell::new(vec![]) };
        let row = label_task(&store, &paths, &down, &down, &req("t2"), n()).unwrap().unwrap();
        assert_eq!(row.outcome, "timeout");
        assert_eq!(store.kind_labels_since(0).unwrap().len(), 2);
    }

    #[test]
    fn eval_counts_job_kind_and_the_confident_subset() {
        let case = |_id: &str, job: &str, ty: &str| EvalCase {
            provider: "copilot-cli",
            job: job.into(),
            task_type: ty.into(),
            text: "x".into(),
        };
        let cases =
            [case("a", "code_change", "fix_bug"), case("b", "code_change", "tests"), case("c", "write_doc", "spec")];
        let f = Fake::ok(answer());
        let e = run_eval(&cases, &f, &f);
        assert_eq!((e.sent, e.answered, e.job_right, e.kind_right), (3, 3, 2, 1));
        assert_eq!((e.confident, e.confident_right), (3, 1));
        let r = eval_report(&e);
        assert!(r.contains("Kind the same as the analysis: 1 of 3") && r.contains("median 4.0 s"), "{r}");
    }

    #[test]
    fn sweep_removes_only_old_leftovers() {
        let d = TempDir::new().unwrap();
        let paths = Paths::at(d.path());
        let w = d.path().join("label-work");
        std::fs::create_dir_all(w.join("copilot-a")).unwrap();
        std::fs::write(w.join("usage-a.json"), "{}").unwrap();
        std::fs::write(w.join("notes.txt"), "x").unwrap();
        assert_eq!(sweep(&paths, now_ms()), 0, "fresh: may belong to a live call");
        assert_eq!(sweep(&paths, now_ms() + 2 * STALE_MS), 2);
        assert!(w.join("notes.txt").exists());
    }
    /// CI's label replay gate. The recorded answers must come from today's label prompt and golden set (else
    /// re-record), every one must still parse, today's acceptance rule must give the recorded totals (bless an
    /// intended move with `AIQ_EVAL_BLESS=1 cargo test -p aiq label_replay_gate`), and the confident labels must
    /// clear the live 8-in-10 bar while still covering at least a third of the prompts a label may coach, so a prompt that is
    /// right only because it never commits cannot pass either.
    #[test]
    fn label_replay_gate() {
        let rec: Value = serde_json::from_str(REPLAY).unwrap();
        let g = golden();
        assert_eq!(
            (rec["prompt_version"].as_str(), rec["golden"].as_str()),
            (Some(label::PROMPT_VERSION), Some(g.version.as_str())),
            "label prompt version or golden set changed: re-record evals/label/replay.json"
        );
        assert_eq!(
            rec["system"].as_str(),
            Some(label::system().as_str()),
            "the label prompt changed without a PROMPT_VERSION bump and a new recording"
        );
        let answers = replay_answers(&rec);
        let e = score_golden(&g, &answers);
        assert_eq!(e.answered, e.sent, "a recorded answer no longer parses");
        assert_eq!(e.sent as usize, g.cases.len() * rec["runs"].as_u64().unwrap() as usize, "every case recorded");
        if std::env::var("AIQ_EVAL_BLESS").is_ok() {
            let mut out = rec.clone();
            out["totals"] = totals_json(&e);
            let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/label/replay.json");
            std::fs::write(p, serde_json::to_string_pretty(&out).unwrap() + "\n").unwrap();
            return;
        }
        assert_eq!(totals_json(&e), rec["totals"], "scores moved on recorded answers");
        assert_eq!(e.passes(), Some(true), "confident labels under 8 in 10: {:?}", e.misses);
        // Coverage over the prompts whose task a label may coach (a question's task never coaches from a label).
        let coachable = answers
            .iter()
            .filter(|(id, _)| {
                g.cases.iter().any(|c| &c.id == id && !label::NOT_COACHED_JOBS.contains(&c.expect.job[0].as_str()))
            })
            .count() as u32;
        assert!(e.confident * 3 >= coachable, "confident on only {} of {coachable}", e.confident);
        // The injection case must never come back as the kind its text asks for.
        let injected = answers.iter().filter(|(id, _)| id == "p29").any(|(_, v)| v["task_type"] == "spec");
        assert!(!injected, "a label followed an instruction inside the fence");
    }

    #[test]
    fn golden_set_is_valid() {
        let g = golden();
        let schema = label::schema();
        let jobs: Vec<&str> =
            schema["properties"]["job"]["enum"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        let kinds: Vec<&str> =
            schema["properties"]["task_type"]["enum"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        let mut ids = std::collections::HashSet::new();
        for c in &g.cases {
            assert!(ids.insert(c.id.as_str()), "duplicate {}", c.id);
            assert!(!c.prompt.trim().is_empty() && !c.expect.job.is_empty() && !c.expect.kind.is_empty(), "{}", c.id);
            assert!(c.expect.job.iter().all(|j| jobs.contains(&j.as_str())), "{} job", c.id);
            assert!(c.expect.kind.iter().all(|k| kinds.contains(&k.as_str())), "{} kind", c.id);
            assert!(["claude-code", "copilot-cli", "vscode"].contains(&c.tool.as_str()), "{} tool", c.id);
        }
        // Every pack job is in the set, and the measured miss shape (problem reports) is well covered.
        for j in rung::PACK_JOBS {
            assert!(g.cases.iter().any(|c| c.expect.job.first().map(String::as_str) == Some(j)), "{j}");
        }
        assert!(g.cases.iter().filter(|c| c.expect.kind.iter().any(|k| k == "fix_bug")).count() >= 8);
    }

    #[test]
    fn golden_scoring_counts_only_confident_labels_toward_the_bar() {
        let g = golden();
        let id = g.cases[0].id.clone();
        let want_job = g.cases[0].expect.job[0].clone();
        let want_kind = g.cases[0].expect.kind[0].clone();
        let right =
            json!({"job": want_job, "job_confidence": 0.9, "task_type": want_kind, "task_type_confidence": 0.9});
        let unsure = json!({"job": "other", "job_confidence": 0.5, "task_type": "other", "task_type_confidence": 0.5});
        let e = score_golden(&g, &[(id.clone(), right), (id.clone(), unsure), (id, Value::Null)]);
        assert_eq!((e.sent, e.answered, e.confident, e.confident_right), (3, 2, 1, 1));
        assert_eq!(e.passes(), None, "one confident label is too few to judge the bar");
    }

    #[test]
    fn the_bar_needs_ten_confident_labels_and_eight_in_ten_right() {
        let mut e = Eval { confident: 10, confident_right: 8, ..Eval::default() };
        assert_eq!(e.passes(), Some(true));
        e.confident_right = 7;
        assert_eq!(e.passes(), Some(false));
        e.confident = 9;
        assert_eq!(e.passes(), None);
    }

    #[test]
    fn labels_use_the_analysis_models_unless_the_person_names_one() {
        let (_d, _paths, store) = setup();
        assert_eq!(copilot_models(&store), vec![Some(rung::DEFAULT_COPILOT_MODEL.to_owned()), None]);
        store.set_setting(rung::SETTING_COPILOT_MODEL, "gpt-6-luna", NOW).unwrap();
        assert_eq!(copilot_models(&store), vec![Some("gpt-6-luna".to_owned())]);
        store.set_setting(label::SETTING_LABEL_COPILOT_MODEL, "default", NOW).unwrap();
        assert_eq!(copilot_models(&store), vec![None]);
        store.set_setting(label::SETTING_LABEL_COPILOT_MODEL, "gpt-6.1-sol", NOW).unwrap();
        assert_eq!(copilot_models(&store), vec![Some("gpt-6.1-sol".to_owned())]);
    }
    /// Real labels measured on the laptop, replayed through today's acceptance rule: a rule change that lets the
    /// measured misses coach again fails here.
    #[test]
    fn measured_labels_clear_the_bar() {
        let m: Value = serde_json::from_str(include_str!("../../../evals/label/measured.json")).unwrap();
        let mut e = Eval::default();
        for r in m["rows"].as_array().unwrap() {
            let (job, ty) = r["label"].as_str().unwrap().split_once('/').unwrap();
            let l = label::Label {
                job: job.into(),
                job_confidence: r["jc"].as_f64().unwrap(),
                task_type: ty.into(),
                task_type_confidence: r["tc"].as_f64().unwrap(),
            };
            if label::accept(&l).is_some() {
                e.confident += 1;
                e.confident_right += u32::from(r["label"] == r["analysis"]);
            }
        }
        assert_eq!(e.passes(), Some(true), "{} of {} confident measured labels right", e.confident_right, e.confident);
    }
}
