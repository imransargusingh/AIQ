//! `aiq analyze --trial`: what this build's analysis says about the tasks whose stored analysis has no sure
//! rung, with nothing stored.
//!
//! It answers "would a changed prompt or kinds list give these tasks a rung?" by measuring. Each task goes
//! through the same envelope, the same sealed calls and the same placement as `aiq analyze`, on the person's own
//! tools, to the provider that already saw it. The store is opened read-only. The answers go to a results file as
//! ids, kinds, rungs and numbers: never a prompt, and never text a model or a tool wrote. Run once from a build of
//! today's analysis and once from a build of a proposed one, the two files compare task by task
//! (`--trial-report <file> --against <file>`), so a task that only got a rung because it was asked again is told
//! apart from one the change gave a rung.
use crate::judge::{self, Asked, CLAUDE, COPILOT, ClaudeCli, CopilotCli, Homes, Outcome, Provider};
use aiq_contracts::judge::Response;
use aiq_core::paths::Paths;
use aiq_domain::clock::DAY_MS;
use aiq_domain::rung;
use aiq_store::{JudgmentRow, Store};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Builds one worker's own tools on its stack and hands them to `work`: Claude Code, Copilot's first call, and
/// Copilot's second call when it goes to another model. A Copilot model chain is never shared between threads.
pub(crate) type WithTools<'a> = dyn Fn(&mut Work<'_>) + Sync + 'a;

/// What one worker does with its own tools: Claude Code, Copilot's first call, Copilot's second call.
pub(crate) type Work<'a> = dyn FnMut(&dyn Provider, &dyn Provider, Option<&dyn Provider>) + 'a;

/// Which tasks a trial asks about.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Pick<'a> {
    /// `--only claude-code|copilot-cli`.
    pub only: Option<&'a str>,
    /// At most this many tasks through each tool, taken in the fixed order (`--max-claude`, `--max-copilot`).
    pub max_claude: Option<usize>,
    pub max_copilot: Option<usize>,
}

/// One task ready to ask about: everything the store and the tools' logs hold on it, read on the main thread.
struct Item {
    task_id: String,
    tool: String,
    provider: &'static str,
    origin: &'static str,
    prompts: usize,
    user: String,
    log: rung::LogFacts,
    old: Value,
}

pub(crate) struct Plan {
    items: Vec<Item>,
    /// This computer's tasks whose stored analysis has no sure rung, by provider, before any cap.
    pub found: BTreeMap<&'static str, usize>,
    /// Answered by an earlier run into the same results file: not asked again.
    pub already: usize,
    /// No prompt text left (the person deleted it): nothing to ask about.
    pub no_text: usize,
    /// Left out by `--max-claude` or `--max-copilot`.
    pub capped: usize,
    same_job: HashMap<String, u32>,
    calls: u8,
}

impl Plan {
    pub fn asks(&self, provider: &str) -> usize {
        self.items.iter().filter(|i| i.provider == provider).count()
    }
}

/// FNV-1a: a fixed order of task ids that is the same in every build and on every computer, so a capped run
/// from another build asks about the same tasks.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
}

const FACT_IDS: [&str; 12] = ["F1", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F12", "F13", "F14", "F15"];

/// Why a placement has no sure rung, as fixed words: no job (and why), each fact AIQ couldn't tell, one run.
fn why(job: &str, job_runs: &[rung::JobRun], notes: &[String]) -> Vec<String> {
    let mut w: Vec<String> = Vec::new();
    if job == "unclassified" {
        w.push(
            match rung::no_job_cause(job_runs) {
                Some(rung::NoJob::AllOther) => "no_job:other",
                Some(rung::NoJob::Disagree) => "no_job:disagree",
                Some(rung::NoJob::LowConfidence) => "no_job:low_confidence",
                None => "no_job:not_recorded",
            }
            .to_owned(),
        );
    }
    for n in notes {
        if let Some((_, rest)) = n.split_once(": AIQ couldn't tell whether ") {
            let words = rest.trim_end_matches('.');
            let id = FACT_IDS.iter().find(|id| rung::fact_words(id) == words).copied().unwrap_or("fact");
            let code = format!("untold:{id}");
            if !w.contains(&code) {
                w.push(code);
            }
        }
        if n == judge::ONE_RUN_NOTE {
            w.push("one_run".to_owned());
        }
    }
    w
}

fn job_runs(read: &Value) -> Vec<rung::JobRun> {
    read["job_runs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            Some(rung::JobRun { job: r["job"].as_str()?.to_owned(), confidence: r["confidence"].as_f64()? })
        })
        .collect()
}

/// The stored analysis of a task, as the results file keeps it: ids and numbers only.
fn old(j: &JudgmentRow) -> Value {
    let read: Value = serde_json::from_str(&j.read).unwrap_or_default();
    let notes: Vec<String> = serde_json::from_str(&j.placement).unwrap_or_default();
    json!({
        "job": j.job, "task_type": j.task_type, "state": j.state, "rung": j.current_rung,
        "model_rung": j.model_rung, "analysis": j.judge_version, "why": why(&j.job, &job_runs(&read), &notes),
    })
}

/// The tasks a trial asks about: this computer's tasks whose stored analysis is uncertain, in a fixed order,
/// without the ones `answered` already.
pub(crate) fn plan(
    store: &Store,
    homes: &Homes,
    pick: Pick<'_>,
    answered: &HashSet<String>,
    now_ms: i64,
) -> Result<Plan, String> {
    let pack = aiq_core::pack::parse(aiq_core::pack::BUNDLED_TOML).map_err(|e| e.to_string())?;
    let tasks = aiq_home::own_tasks(store, &pack, 0)?;
    let judged = store.current_judgments().map_err(|e| e.to_string())?;
    let mut picked: Vec<_> = tasks
        .iter()
        .filter_map(|t| {
            let j = judged.get(&t.task_id).filter(|j| j.state == "uncertain")?;
            let p = judge::provider_for(&t.tool).filter(|p| pick.only.is_none_or(|o| o == *p))?;
            Some((t, j, p))
        })
        .collect();
    picked.sort_by_key(|(t, _, _)| fnv1a(t.task_id.as_bytes()));
    let mut found: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut left: HashMap<&str, usize> = HashMap::from([
        (CLAUDE, pick.max_claude.unwrap_or(usize::MAX)),
        (COPILOT, pick.max_copilot.unwrap_or(usize::MAX)),
    ]);
    let (mut items, mut already, mut no_text, mut capped) = (Vec::new(), 0, 0, 0);
    for (t, j, p) in picked {
        *found.entry(p).or_default() += 1;
        let room = left.entry(p).or_insert(usize::MAX);
        if *room == 0 {
            capped += 1;
            continue;
        }
        *room -= 1;
        if answered.contains(&t.task_id) {
            already += 1;
            continue;
        }
        let turns = judge::task_turns(store, homes, t).map_err(|e| e.to_string())?;
        if turns.is_empty() {
            no_text += 1;
            continue;
        }
        let log = rung::log_facts(&turns);
        items.push(Item {
            task_id: t.task_id.clone(),
            tool: t.tool.clone(),
            provider: p,
            origin: t.origin,
            prompts: turns.len(),
            user: judge::task_envelope(&t.tool, &turns, &log),
            log,
            old: old(j),
        });
    }
    // The same count `aiq analyze` gives placement (F11): stored analyses of the job in the last 30 days.
    let mut same_job: HashMap<String, u32> = HashMap::new();
    for j in judged.values().filter(|j| j.judged_utc_ms >= now_ms - 30 * DAY_MS) {
        *same_job.entry(j.job.clone()).or_default() += 1;
    }
    Ok(Plan { items, found, already, no_text, capped, same_job, calls: judge::settings(store).calls })
}

fn is_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 24 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A failed call's cause as a fixed word, or the rejected job or kind id. Never the tool's own text: an error
/// message can quote the task.
fn cause(o: &Outcome) -> Option<String> {
    match o {
        Outcome::Schema(d) => Some(match d.split_once(':') {
            Some((k @ ("job" | "task_type"), v)) if is_id(v.trim()) => format!("{k}:{}", v.trim()),
            Some((k, _)) if is_id(k) => k.to_owned(),
            _ => "unreadable".to_owned(),
        }),
        Outcome::Error(d) if d.contains("from --model flag is not available") => Some("model_not_available".into()),
        Outcome::Error(d) if d.starts_with("not sealed") => Some("not_sealed".into()),
        _ => None,
    }
}

fn call_record(c: &Asked<'_>) -> Value {
    json!({
        "run": c.run, "outcome": c.result.outcome.label().0, "cause": cause(&c.result.outcome),
        "model": c.result.model, "ms": c.result.latency_ms, "cost_usd": c.result.cost_usd,
    })
}

/// One results line: the task, its stored analysis, and what this build's analysis says now.
fn result_line(item: &Item, calls: u8, runs: &[Response], asked: Vec<Value>, same_job: &HashMap<String, u32>) -> Value {
    let new = if runs.is_empty() {
        json!({"answered": 0})
    } else {
        let (_, p) = judge::placed(runs, &item.log, same_job.get(&runs[0].job).copied().unwrap_or(0), calls);
        let picks: Vec<rung::JobRun> =
            runs.iter().map(|r| rung::JobRun { job: r.job.clone(), confidence: r.job_confidence }).collect();
        json!({
            "answered": runs.len(), "job": p.job, "task_type": p.task_type, "state": p.state.as_str(),
            "rung": p.current, "model_rung": p.model_rung, "potential": p.potential, "coachable": p.coachable,
            "job_runs": runs.iter().map(|r| json!({"job": r.job, "confidence": r.job_confidence})).collect::<Vec<_>>(),
            "type_runs": runs
                .iter()
                .map(|r| json!({"task_type": r.task_type, "confidence": r.task_type_confidence}))
                .collect::<Vec<_>>(),
            "rungs": runs.iter().map(|r| r.current_rung).collect::<Vec<_>>(),
            "why": why(&p.job, &picks, &p.notes),
        })
    };
    json!({
        "task_id": item.task_id, "tool": item.tool, "provider": item.provider, "origin": item.origin,
        "prompts": item.prompts, "analysis": rung::JUDGE_VERSION, "facts": rung::FACT_SET_VERSION,
        "rubric": rung::RUBRIC_VERSION, "old": item.old, "new": new, "calls": asked,
    })
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Ran {
    pub asked: u32,
    /// Tasks left unasked because their tool stopped working mid-run (not installed, or signed out).
    pub not_asked: u32,
    pub down: Vec<&'static str>,
}

/// Ask about every planned task, `workers` at a time, and append one line per task to `out`. Nothing else is
/// written but the Copilot session ids `aiq analyze` records too, so AIQ never takes its own call for the
/// person's session.
pub(crate) fn run(
    paths: &Paths,
    plan: Plan,
    with_tools: &WithTools<'_>,
    workers: usize,
    out: &Path,
) -> Result<Ran, String> {
    let Plan { items, same_job, calls, .. } = plan;
    let total = items.len();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out)
        .map_err(|e| format!("{}: {e}", out.display()))?;
    let (file, queue) = (Mutex::new(file), Mutex::new(VecDeque::from(items)));
    let down = [AtomicBool::new(false), AtomicBool::new(false)];
    let (asked, not_asked) = (AtomicU32::new(0), AtomicU32::new(0));
    let failed: Mutex<Option<String>> = Mutex::new(None);
    std::thread::scope(|s| {
        for _ in 0..workers.clamp(1, 8) {
            s.spawn(|| {
                with_tools(&mut |claude: &dyn Provider, copilot: &dyn Provider, second: Option<&dyn Provider>| {
                    loop {
                        let next = queue.lock().ok().and_then(|mut q| q.pop_front());
                        let Some(item) = next else { break };
                        let at = usize::from(item.provider == COPILOT);
                        if down[at].load(Ordering::Relaxed) {
                            not_asked.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        let (provider, second) =
                            if item.provider == CLAUDE { (claude, None) } else { (copilot, second) };
                        let mut heard = Vec::new();
                        let (runs, results) =
                            judge::ask(paths, provider, item.provider, second, &item.user, calls, |c| {
                                heard.push(call_record(c));
                                Ok::<(), std::convert::Infallible>(())
                            })
                            .unwrap_or_else(|never| match never {});
                        if runs.is_empty()
                            && results
                                .last()
                                .is_some_and(|r| matches!(r.outcome, Outcome::Missing | Outcome::NotSignedIn(_)))
                        {
                            down[at].store(true, Ordering::Relaxed);
                        }
                        let line = format!("{}\n", result_line(&item, calls, &runs, heard, &same_job));
                        let wrote = file.lock().map_err(|e| e.to_string()).and_then(|mut f| {
                            f.write_all(line.as_bytes()).and_then(|()| f.flush()).map_err(|e| e.to_string())
                        });
                        if let Err(e) = wrote {
                            if let Ok(mut slot) = failed.lock() {
                                slot.get_or_insert(e);
                            }
                            break;
                        }
                        let n = asked.fetch_add(1, Ordering::Relaxed) + 1;
                        if n.is_multiple_of(5) || n as usize == total {
                            println!("asked about {n} of {total}");
                        }
                    }
                });
            });
        }
    });
    if let Some(e) = failed.into_inner().ok().flatten() {
        return Err(format!("{}: {e}", out.display()));
    }
    Ok(Ran {
        asked: asked.into_inner(),
        not_asked: not_asked.into_inner(),
        down: [CLAUDE, COPILOT]
            .into_iter()
            .zip(&down)
            .filter(|(_, d)| d.load(Ordering::Relaxed))
            .map(|(p, _)| p)
            .collect(),
    })
}

// ------------------------------------------------------------------ the results file

/// A results file: every line (each is one try at a task, and its calls were paid for), and each task's last
/// line (a task no run answered is asked again by the next run, so its last line stands).
#[derive(Debug, Default)]
pub(crate) struct Results {
    pub lines: Vec<Value>,
    pub tasks: BTreeMap<String, Value>,
}

impl Results {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut r = Results::default();
        for (i, l) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
            let v: Value = serde_json::from_str(l).map_err(|e| format!("line {}: not JSON ({e})", i + 1))?;
            let id = v["task_id"].as_str().ok_or_else(|| format!("line {}: no task_id", i + 1))?.to_owned();
            r.tasks.insert(id, v.clone());
            r.lines.push(v);
        }
        Ok(r)
    }

    /// A missing file is an empty one: the first run starts it.
    pub fn read(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(t) => Self::parse(&t).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Results::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Tasks at least one run answered: a later run does not ask about them again.
    pub fn answered(&self) -> HashSet<String> {
        self.tasks.iter().filter(|(_, v)| answers(v) > 0).map(|(k, _)| k.clone()).collect()
    }
}

fn answers(v: &Value) -> u64 {
    v["new"]["answered"].as_u64().unwrap_or(0)
}

/// Whether the line's new analysis gives a sure rung. None when no run answered.
fn sure(v: &Value) -> Option<bool> {
    (answers(v) > 0).then(|| v["new"]["state"].as_str() != Some("uncertain"))
}

fn codes(v: &Value) -> Vec<&str> {
    v["new"]["why"].as_array().into_iter().flatten().filter_map(Value::as_str).collect()
}

fn tool_name(provider: &str) -> &'static str {
    if provider == CLAUDE { "Claude Code" } else { "Copilot CLI" }
}

fn job_name(job: &str) -> &'static str {
    if job == "unclassified" { "no job fits" } else { rung::job_label(job) }
}

/// "a 3, b 1": counts, largest first, then by name.
fn tally<K: AsRef<str>>(counts: &BTreeMap<K, u32>) -> String {
    let mut v: Vec<(&str, u32)> = counts.iter().map(|(k, n)| (k.as_ref(), *n)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    if v.is_empty() {
        "none".to_owned()
    } else {
        v.iter().map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join(", ")
    }
}

fn rungs_line(tasks: &[&Value]) -> String {
    let n = |r: u64| tasks.iter().filter(|v| v["new"]["rung"].as_u64() == Some(r)).count();
    rung::RUNG_NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| format!("{name} {}", n(i as u64 + 1)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// "sure 5 (Assisted 4, …), no sure rung 2, not answered 0" for one group of tasks.
fn split(tasks: &[&Value]) -> String {
    let now_sure: Vec<&Value> = tasks.iter().copied().filter(|v| sure(v) == Some(true)).collect();
    let unsure = tasks.iter().filter(|v| sure(v) == Some(false)).count();
    let none = tasks.iter().filter(|v| sure(v).is_none()).count();
    format!("a sure rung {} ({}), no sure rung {unsure}, not answered {none}", now_sure.len(), rungs_line(&now_sure))
}

/// What the stored analysis said, as the three groups Home shows.
fn was(v: &Value) -> &'static str {
    match (v["old"]["job"].as_str(), v["old"]["task_type"].as_str()) {
        (Some("unclassified"), _) => "no job fits",
        (_, None | Some("other")) => "a job, kind Other",
        _ => "a job, a named kind",
    }
}

/// The counts in a results file, as text. No task id, and nothing a model or a tool wrote.
pub(crate) fn report(r: &Results) -> String {
    let tasks: Vec<&Value> = r.tasks.values().collect();
    let str_of = |v: &'_ Value, k: &str| v[k].as_str().unwrap_or("?").to_owned();
    let versions: BTreeMap<String, u32> = tasks.iter().fold(BTreeMap::new(), |mut m, v| {
        *m.entry(format!(
            "{} (facts {}, rubric {})",
            str_of(v, "analysis"),
            str_of(v, "facts"),
            str_of(v, "rubric")
        ))
        .or_default() += 1;
        m
    });
    let by = |f: &dyn Fn(&Value) -> String| -> BTreeMap<String, Vec<&Value>> {
        let mut m: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for v in &tasks {
            m.entry(f(v)).or_default().push(v);
        }
        m
    };
    let mut o = format!(
        "Analysis trial: {} tasks whose stored analysis has no sure rung, asked again. Nothing was stored.\n",
        tasks.len()
    );
    o += &format!("Analysis that answered: {}\n", tally(&versions));
    // Calls: every line counts, a retried task's earlier tries too.
    let mut outcomes: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
    let (mut models, mut causes): (BTreeMap<String, u32>, BTreeMap<String, u32>) = Default::default();
    let mut cost = 0.0;
    for (line, c) in r.lines.iter().flat_map(|l| l["calls"].as_array().into_iter().flatten().map(move |c| (l, c))) {
        let ok = c["outcome"].as_str() == Some("ok");
        *outcomes.entry(str_of(line, "provider")).or_default().entry(str_of(c, "outcome")).or_default() += 1;
        if ok {
            *models.entry(c["model"].as_str().unwrap_or("not recorded").to_owned()).or_default() += 1;
        }
        if let Some(k) = c["cause"].as_str() {
            *causes.entry(k.to_owned()).or_default() += 1;
        }
        cost += c["cost_usd"].as_f64().unwrap_or(0.0);
    }
    for (p, m) in &outcomes {
        o += &format!("Calls through {}: {} ({})\n", tool_name(p), m.values().sum::<u32>(), tally(m));
    }
    o += &format!("Models that answered: {}\n", tally(&models));
    if !causes.is_empty() {
        o += &format!("Calls that failed, by cause: {}\n", tally(&causes));
    }
    o += &format!("Cost Claude Code reported: ${cost:.2}\n");
    let n_answers = |n: u64| tasks.iter().filter(|v| answers(v) == n).count();
    o += &format!("Answers per task: two {}, one {}, none {}\n\n", n_answers(2), n_answers(1), n_answers(0));

    o += &format!("Now: {}\n", split(&tasks));
    let unsure: Vec<&Value> = tasks.iter().copied().filter(|v| sure(v) == Some(false)).collect();
    let first = |v: &Value, prefix: &str| codes(v).iter().any(|c| c.starts_with(prefix));
    let no_job: Vec<&&Value> = unsure.iter().filter(|v| first(v, "no_job")).collect();
    let untold = unsure.iter().filter(|v| !first(v, "no_job") && first(v, "untold")).count();
    let one_run = unsure.iter().filter(|v| !first(v, "no_job") && !first(v, "untold")).count();
    let has = |c: &str| no_job.iter().filter(|v| codes(v).contains(&c)).count();
    o += &format!(
        "  no sure rung because: no job fits {} (the analysis said other {}, the two runs named different jobs {}, \
         a job named but not confidently {}); a job, but a fact AIQ couldn't tell {untold}; only one run answered {one_run}\n",
        no_job.len(),
        has("no_job:other"),
        has("no_job:disagree"),
        has("no_job:low_confidence"),
    );
    let mut reasons: BTreeMap<String, u32> = BTreeMap::new();
    for c in unsure.iter().flat_map(|v| codes(v)) {
        *reasons.entry(c.to_owned()).or_default() += 1;
    }
    o += &format!("  every reason counted (a task can have several): {}\n\n", tally(&reasons));

    o += "By what the stored analysis said:\n";
    for (k, v) in by(&|v| was(v).to_owned()) {
        o += &format!("  {k} ({}): {}\n", v.len(), split(&v));
    }
    o += "By tool:\n";
    for (k, v) in by(&|v| tool_name(v["provider"].as_str().unwrap_or("")).to_owned()) {
        o += &format!("  {k} ({}): {}\n", v.len(), split(&v));
    }
    let answered: Vec<&Value> = tasks.iter().copied().filter(|v| answers(v) > 0).collect();
    let mut jobs: BTreeMap<&str, u32> = BTreeMap::new();
    let mut kinds: BTreeMap<String, u32> = BTreeMap::new();
    for v in &answered {
        let job = v["new"]["job"].as_str().unwrap_or("unclassified");
        *jobs.entry(job_name(job)).or_default() += 1;
        if let Some(k) = v["new"]["task_type"].as_str() {
            *kinds.entry(format!("{job}/{k}")).or_default() += 1;
        }
    }
    o += &format!("Job now: {}\n", tally(&jobs));
    let named: u32 = kinds.iter().filter(|(k, _)| !k.ends_with("/other")).map(|(_, n)| n).sum();
    o += &format!(
        "Kind now, tasks with a job ({}): a named kind {named}, Other {}\n",
        kinds.values().sum::<u32>(),
        kinds.values().sum::<u32>() - named
    );
    o += &format!("  {}\n", tally(&kinds));
    let now_sure: Vec<&Value> = tasks.iter().copied().filter(|v| sure(v) == Some(true)).collect();
    let against = |f: fn(u64, u64) -> bool| {
        now_sure
            .iter()
            .filter(|v| f(v["new"]["rung"].as_u64().unwrap_or(0), v["old"]["rung"].as_u64().unwrap_or(0)))
            .count()
    };
    o += &format!(
        "Rung now against the stored guess, tasks with a sure rung now ({}): same {}, higher {}, lower {}\n",
        now_sure.len(),
        against(|n, o| n == o),
        against(|n, o| n > o),
        against(|n, o| n < o),
    );
    o
}

/// The same tasks in two results files (A, then B), task by task: which give a sure rung in each.
pub(crate) fn compare(a: &Results, b: &Results) -> String {
    let both: Vec<(&Value, &Value)> = a
        .tasks
        .iter()
        .filter_map(|(id, x)| b.tasks.get(id).map(|y| (x, y)))
        .filter(|(x, y)| answers(x) > 0 && answers(y) > 0)
        .collect();
    let version = |r: &Results| {
        let v: HashSet<&str> = r.tasks.values().filter_map(|v| v["analysis"].as_str()).collect();
        let mut v: Vec<&str> = v.into_iter().collect();
        v.sort_unstable();
        v.join("+")
    };
    let cell = |set: &[&(&Value, &Value)], x: bool, y: bool| {
        set.iter().filter(|(p, q)| sure(p) == Some(x) && sure(q) == Some(y)).count()
    };
    let all: Vec<&(&Value, &Value)> = both.iter().collect();
    let pct = |n: usize| if all.is_empty() { 0 } else { (n * 100 + all.len() / 2) / all.len() };
    let (a_sure, b_sure) =
        (cell(&all, true, true) + cell(&all, true, false), cell(&all, true, true) + cell(&all, false, true));
    let mut o = format!(
        "The same tasks under two analyses: A is {}, B is {}. Tasks answered in both: {}.\n",
        version(a),
        version(b),
        both.len()
    );
    o += &format!("A gives a sure rung to {a_sure} ({}%), B to {b_sure} ({}%).\n", pct(a_sure), pct(b_sure));
    let row = |set: &[&(&Value, &Value)]| {
        format!(
            "sure in both {}, only in A {}, only in B {}, in neither {}",
            cell(set, true, true),
            cell(set, true, false),
            cell(set, false, true),
            cell(set, false, false)
        )
    };
    o += &format!("  all ({}): {}\n", all.len(), row(&all));
    for p in [CLAUDE, COPILOT] {
        let set: Vec<&(&Value, &Value)> = both.iter().filter(|(x, _)| x["provider"].as_str() == Some(p)).collect();
        o += &format!("  {} ({}): {}\n", tool_name(p), set.len(), row(&set));
    }
    for w in ["no job fits", "a job, kind Other", "a job, a named kind"] {
        let set: Vec<&(&Value, &Value)> = both.iter().filter(|(x, _)| was(x) == w).collect();
        o += &format!("  stored analysis said {w} ({}): {}\n", set.len(), row(&set));
    }
    let agree: Vec<&(&Value, &Value)> =
        both.iter().filter(|(x, y)| sure(x) == Some(true) && sure(y) == Some(true)).collect();
    let rung = |v: &Value| v["new"]["rung"].as_u64().unwrap_or(0);
    o += &format!(
        "Sure in both ({}): same rung {}, B higher {}, B lower {}; same job {}\n",
        agree.len(),
        agree.iter().filter(|(x, y)| rung(x) == rung(y)).count(),
        agree.iter().filter(|(x, y)| rung(y) > rung(x)).count(),
        agree.iter().filter(|(x, y)| rung(y) < rung(x)).count(),
        agree.iter().filter(|(x, y)| x["new"]["job"] == y["new"]["job"]).count(),
    );
    o
}

// ------------------------------------------------------------------ CLI

/// `aiq analyze --trial --out <file> [--only claude-code|copilot-cli] [--max-claude <n>] [--max-copilot <n>]
/// [--workers <n>] [--dry-run]` and `aiq analyze --trial-report <file> [--against <file>]`.
pub fn cli(args: &[&str], paths: &Paths) -> ExitCode {
    let value = |flag: &str| args.iter().position(|a| *a == flag).map(|i| args.get(i + 1).copied());
    let number = |flag: &str| match value(flag) {
        None => Ok(None),
        Some(v) => v.and_then(|v| v.parse::<usize>().ok()).map(Some).ok_or(format!("{flag} takes a whole number.")),
    };
    if let Some(file) = value("--trial-report") {
        let Some(file) = file else {
            eprintln!("--trial-report takes a results file.");
            return ExitCode::from(2);
        };
        let read = |f: &str| {
            Results::read(Path::new(f))
                .and_then(|r| if r.tasks.is_empty() { Err(format!("{f}: no results")) } else { Ok(r) })
        };
        let text = match value("--against").flatten() {
            None => read(file).map(|r| report(&r)),
            Some(other) => read(file).and_then(|a| read(other).map(|b| compare(&a, &b))),
        };
        return match text {
            Ok(t) => {
                print!("{t}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(1)
            }
        };
    }
    let Some(Some(out)) = value("--out") else {
        eprintln!("--trial needs --out <file>: the results go there, and nothing is stored.");
        return ExitCode::from(2);
    };
    let out = Path::new(out);
    let (max_claude, max_copilot, workers) =
        match (number("--max-claude"), number("--max-copilot"), number("--workers")) {
            (Ok(a), Ok(b), Ok(w)) => (a, b, w.unwrap_or(3)),
            (a, b, w) => {
                for e in [a.err(), b.err(), w.err()].into_iter().flatten() {
                    eprintln!("{e}");
                }
                return ExitCode::from(2);
            }
        };
    if aiq_core::control::is_paused(paths) {
        println!("AIQ is paused, so it isn't analyzing. Resume with: aiq resume");
        return ExitCode::SUCCESS;
    }
    let done = match Results::read(out) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    let store = match Store::open_read_only(&paths.db()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("AIQ could not open its store: {}", e.detail);
            return ExitCode::from(1);
        }
    };
    let only = value("--only").flatten();
    let pick = Pick { only, max_claude, max_copilot };
    let plan = match plan(&store, &Homes::from_env(), pick, &done.answered(), judge::now_ms()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("AIQ could not read the tasks: {e}");
            return ExitCode::from(1);
        }
    };
    let found = |p: &str| plan.found.get(p).copied().unwrap_or(0);
    println!(
        "Trial of analysis {} (facts {}, rubric {}): {} of this computer's tasks have no sure rung ({} through Claude Code, {} through Copilot CLI).",
        rung::JUDGE_VERSION,
        rung::FACT_SET_VERSION,
        rung::RUBRIC_VERSION,
        found(CLAUDE) + found(COPILOT),
        found(CLAUDE),
        found(COPILOT)
    );
    println!(
        "Already answered in {}: {}. No prompt text left: {}. Left out by a --max: {}.",
        out.display(),
        plan.already,
        plan.no_text,
        plan.capped
    );
    let calls = usize::from(plan.calls);
    println!(
        "To ask: {} tasks, {calls} call(s) each: {} Claude Code calls and {} Copilot calls (about one premium request each). Nothing is stored.",
        plan.asks(CLAUDE) + plan.asks(COPILOT),
        plan.asks(CLAUDE) * calls,
        plan.asks(COPILOT) * calls
    );
    if args.contains(&"--dry-run") {
        println!("Nothing was sent.");
        return ExitCode::SUCCESS;
    }
    let set = judge::settings(&store);
    // The trial's own Copilot folder, beside its results: `aiq analyze` sweeps its `judge-work` while it runs.
    let work = out.with_extension("work");
    let _ = std::fs::remove_dir_all(&work);
    let (claude_exe, copilot_exe) = (judge::find_exe("claude"), judge::find_copilot(paths));
    let with_tools = |work_with: &mut Work<'_>| {
        let claude =
            ClaudeCli { exe: claude_exe.clone(), model: set.claude_model.clone(), scratch: std::env::temp_dir() };
        let base = CopilotCli {
            exe: copilot_exe.clone(),
            model: set.copilot_model.clone(),
            work: work.clone(),
            // A trial keeps no Copilot session: its folder holds the task's prompts.
            keep_sessions: false,
        };
        let second = judge::analysis_copilot_second_models(&set).map(|m| judge::copilot_chain(&base, m));
        let first = judge::copilot_chain(&base, judge::analysis_copilot_models(&set));
        work_with(&claude, &first, second.as_ref().map(|p| p as &dyn Provider));
    };
    let ran = run(paths, plan, &with_tools, workers, out);
    let _ = std::fs::remove_dir_all(&work);
    match ran.and_then(|ran| Results::read(out).map(|r| (ran, r))) {
        Ok((ran, r)) => {
            for p in &ran.down {
                println!(
                    "{} stopped answering (not installed, or signed out): {} tasks were not asked.",
                    tool_name(p),
                    ran.not_asked
                );
            }
            print!("\n{}", report(&r));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("AIQ could not finish the trial: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::tests::{Fake, env, go};
    use std::sync::atomic::AtomicUsize;

    /// A fake answer like `Fake::ok`, with one field changed.
    fn fake(rung: u8, key: &str, value: Value) -> Fake {
        let mut f = Fake::ok(rung);
        if let Some(a) = f.answer.as_mut() {
            a[key] = value;
        }
        f
    }

    fn rows(store: &Store, table: &str) -> i64 {
        store.raw().query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    const NOW: i64 = 100 * DAY_MS;

    #[test]
    fn a_trial_asks_about_every_task_with_no_sure_rung_and_stores_nothing() {
        let e = env();
        // Today's analysis is unsure of the job on all four closed tasks: no job, so no sure rung.
        let unsure = fake(2, "job_confidence", json!(0.5));
        go(&e, &unsure, &unsure, None);
        let (judgments, calls) = (rows(&e.store, "judgment"), rows(&e.store, "judge_call"));
        assert_eq!((judgments, calls), (4, 8));
        let sessions =
            || std::fs::read_to_string(e.paths.data.join("judge-sessions.txt")).unwrap_or_default().lines().count();
        let recorded = sessions();

        let out = e.paths.data.join("trial").join("results.jsonl");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        let store = Store::open_read_only(&e.paths.db()).unwrap();
        let p = plan(&store, &e.homes, Pick::default(), &HashSet::new(), NOW).unwrap();
        assert_eq!((p.found[CLAUDE], p.found[COPILOT], p.asks(CLAUDE), p.asks(COPILOT)), (2, 2, 2, 2));
        // The trial's analysis is sure: each worker's own fakes, counted together.
        let asked = AtomicUsize::new(0);
        let tools = |work: &mut Work<'_>| {
            let (c, p) = (Fake::ok(2), Fake::ok(2));
            work(&c, &p, None);
            asked.fetch_add(c.calls() + p.calls(), Ordering::Relaxed);
        };
        let ran = run(&e.paths, p, &tools, 3, &out).unwrap();
        assert_eq!((ran.asked, ran.not_asked, ran.down.len(), asked.load(Ordering::Relaxed)), (4, 0, 0, 8));

        // Nothing stored: no analysis, no call. Only the Copilot session ids, as `aiq analyze` records them.
        assert_eq!((rows(&e.store, "judgment"), rows(&e.store, "judge_call")), (judgments, calls));
        assert_eq!(sessions(), recorded + 4, "two Copilot tasks, two calls each");

        let r = Results::read(&out).unwrap();
        assert_eq!(r.tasks.len(), 4);
        for v in r.tasks.values() {
            assert_eq!(v["old"]["job"], "unclassified");
            assert_eq!(v["old"]["why"], json!(["no_job:low_confidence"]));
            assert_eq!(
                (&v["new"]["job"], &v["new"]["state"], &v["new"]["rung"]),
                (&json!("code_change"), &json!("judged"), &json!(2))
            );
            assert_eq!(v["new"]["why"], json!([]));
            assert_eq!(v["calls"].as_array().unwrap().len(), 2);
            assert_eq!(v["analysis"], rung::JUDGE_VERSION);
        }
        // The file holds ids, kinds and numbers: no prompt, and nothing the model wrote.
        let text = std::fs::read_to_string(&out).unwrap();
        for private in ["flaky", "ADR", "parser", "config loader", "failing test", "Let the agent"] {
            assert!(!text.contains(private), "{private}");
        }
        let rep = report(&r);
        assert!(rep.contains("Now: a sure rung 4 (Assisted 0, Directed 4, Delegated 0, Orchestrated 0, Factory 0), no sure rung 0, not answered 0"), "{rep}");
        assert!(rep.contains("  no job fits (4): a sure rung 4"), "{rep}");
        assert!(
            rep.contains("Calls through Claude Code: 4 (ok 4)") && rep.contains("Calls through Copilot CLI: 4 (ok 4)"),
            "{rep}"
        );
        assert!(rep.contains("Job now: Code a change 4") && rep.contains("code_change/fix_bug 4"), "{rep}");

        // A second run finds them answered and asks nothing.
        let again = plan(&store, &e.homes, Pick::default(), &r.answered(), NOW).unwrap();
        assert_eq!((again.already, again.asks(CLAUDE) + again.asks(COPILOT)), (4, 0));
    }

    #[test]
    fn a_cap_takes_the_same_tasks_every_time_and_a_tool_that_is_signed_out_stops_its_tasks() {
        let e = env();
        let unsure = fake(2, "job_confidence", json!(0.5));
        go(&e, &unsure, &unsure, None);
        let store = Store::open_read_only(&e.paths.db()).unwrap();
        let pick = Pick { max_copilot: Some(1), ..Pick::default() };
        let ids = |p: &Plan| p.items.iter().map(|i| i.task_id.clone()).collect::<Vec<_>>();
        let p = plan(&store, &e.homes, pick, &HashSet::new(), NOW).unwrap();
        assert_eq!((p.asks(CLAUDE), p.asks(COPILOT), p.capped), (2, 1, 1));
        assert_eq!(ids(&p), ids(&plan(&store, &e.homes, pick, &HashSet::new(), NOW).unwrap()));
        let only =
            plan(&store, &e.homes, Pick { only: Some(COPILOT), ..Pick::default() }, &HashSet::new(), NOW).unwrap();
        assert_eq!((only.asks(CLAUDE), only.asks(COPILOT)), (0, 2));

        // Copilot is signed out: its first task records the failed call, the rest are not asked, Claude's go on.
        let out = e.paths.data.join("results.jsonl");
        let tools = |work: &mut Work<'_>| {
            work(&Fake::ok(1), &Fake::failing(Outcome::NotSignedIn("Please run /login for acme-corp".into())), None);
        };
        let all = plan(&store, &e.homes, Pick::default(), &HashSet::new(), NOW).unwrap();
        let ran = run(&e.paths, all, &tools, 1, &out).unwrap();
        assert_eq!((ran.asked, ran.not_asked, ran.down.as_slice()), (3, 1, [COPILOT].as_slice()));
        let r = Results::read(&out).unwrap();
        assert_eq!(r.answered().len(), 2, "the two Claude Code tasks");
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains("not_signed_in") && !text.contains("acme-corp"), "the tool's own words stay out");
        assert!(report(&r).contains("not answered 1"));
    }

    fn line(id: &str, provider: &str, old_job: &str, new: Value) -> Value {
        json!({
            "task_id": id, "tool": provider, "provider": provider, "origin": "live", "prompts": 1,
            "analysis": "j7", "facts": "fs5", "rubric": "r3",
            "old": {"job": old_job, "task_type": if old_job == "unclassified" { Value::Null } else { json!("other") },
                    "state": "uncertain", "rung": 1, "model_rung": 1, "analysis": "j6", "why": []},
            "new": new,
            "calls": [{"run": 1, "outcome": "ok", "cause": null, "model": "m", "ms": 1, "cost_usd": 0.01}],
        })
    }

    fn placed(job: &str, kind: Option<&str>, state: &str, rung: u8, why: &[&str]) -> Value {
        json!({"answered": 2, "job": job, "task_type": kind, "state": state, "rung": rung, "why": why})
    }

    fn results(lines: &[Value]) -> Results {
        Results::parse(&lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n")).unwrap()
    }

    #[test]
    fn the_report_counts_each_task_once_by_its_last_line_and_every_call() {
        let r = results(&[
            line("a", CLAUDE, "unclassified", json!({"answered": 0})),
            line("a", CLAUDE, "unclassified", placed("run_ship", Some("release"), "right_rung", 1, &[])),
            line(
                "b",
                COPILOT,
                "unclassified",
                placed("unclassified", None, "uncertain", 1, &["no_job:disagree", "untold:F6"]),
            ),
            line("c", COPILOT, "code_change", placed("code_change", Some("other"), "uncertain", 2, &["untold:F6"])),
            line("d", COPILOT, "code_change", placed("ask_research", Some("review"), "judged", 3, &[])),
            line("e", CLAUDE, "unclassified", json!({"answered": 0})),
        ]);
        assert_eq!((r.lines.len(), r.tasks.len(), r.answered().len()), (6, 5, 4));
        let rep = report(&r);
        for want in [
            "Analysis trial: 5 tasks",
            "Calls through Claude Code: 3 (ok 3)",
            "Cost Claude Code reported: $0.06",
            "Answers per task: two 4, one 0, none 1",
            "Now: a sure rung 2 (Assisted 1, Directed 0, Delegated 1, Orchestrated 0, Factory 0), no sure rung 2, not answered 1",
            "no job fits 1 (the analysis said other 0, the two runs named different jobs 1, a job named but not confidently 0); a job, but a fact AIQ couldn't tell 1; only one run answered 0",
            "every reason counted (a task can have several): untold:F6 2, no_job:disagree 1",
            "  no job fits (3): a sure rung 1 (Assisted 1,",
            "  a job, kind Other (2): a sure rung 1 (Assisted 0, Directed 0, Delegated 1,",
            "Job now: Ask or research 1, Code a change 1, Run or ship it 1, no job fits 1",
            "Kind now, tasks with a job (3): a named kind 2, Other 1",
            "Rung now against the stored guess, tasks with a sure rung now (2): same 1, higher 1, lower 0",
        ] {
            assert!(rep.contains(want), "{want}\n{rep}");
        }
    }

    #[test]
    fn two_files_compare_task_by_task_on_the_tasks_both_answered() {
        let today = results(&[
            line("a", CLAUDE, "unclassified", placed("unclassified", None, "uncertain", 1, &["no_job:other"])),
            line("b", COPILOT, "unclassified", placed("run_ship", Some("other"), "right_rung", 1, &[])),
            line("c", COPILOT, "code_change", placed("code_change", Some("other"), "judged", 2, &[])),
            line("x", COPILOT, "unclassified", json!({"answered": 0})),
        ]);
        let proposed = results(&[
            line("a", CLAUDE, "unclassified", placed("run_ship", Some("release"), "right_rung", 1, &[])),
            line("b", COPILOT, "unclassified", placed("run_ship", Some("release"), "judged", 2, &[])),
            line("c", COPILOT, "code_change", placed("unclassified", None, "uncertain", 1, &["no_job:disagree"])),
            line("x", COPILOT, "unclassified", placed("run_ship", Some("release"), "right_rung", 1, &[])),
            line("y", COPILOT, "unclassified", placed("run_ship", Some("release"), "right_rung", 1, &[])),
        ]);
        let c = compare(&today, &proposed);
        for want in [
            "Tasks answered in both: 3.",
            "A gives a sure rung to 2 (67%), B to 2 (67%).",
            "  all (3): sure in both 1, only in A 1, only in B 1, in neither 0",
            "  Claude Code (1): sure in both 0, only in A 0, only in B 1, in neither 0",
            "  stored analysis said no job fits (2): sure in both 1, only in A 0, only in B 1, in neither 0",
            "Sure in both (1): same rung 0, B higher 1, B lower 0; same job 1",
        ] {
            assert!(c.contains(want), "{want}\n{c}");
        }
    }

    #[test]
    fn a_failed_call_is_named_by_a_fixed_word_never_by_the_tools_own_text() {
        assert_eq!(cause(&Outcome::Schema("task_type: build_fix".into())).as_deref(), Some("task_type:build_fix"));
        assert_eq!(cause(&Outcome::Schema("task_type: Fix the Contoso build".into())).as_deref(), Some("task_type"));
        assert_eq!(cause(&Outcome::Schema("evidence: empty".into())).as_deref(), Some("evidence"));
        assert_eq!(
            cause(&Outcome::Schema("answer is not one JSON object: Sure! Contoso".into())).as_deref(),
            Some("unreadable")
        );
        assert_eq!(cause(&Outcome::Schema("no structured_output".into())).as_deref(), Some("unreadable"));
        let refused = "Model \"x\" from --model flag is not available.";
        assert_eq!(cause(&Outcome::Error(refused.into())).as_deref(), Some("model_not_available"));
        assert_eq!(cause(&Outcome::Error("Contoso said no".into())), None);
        assert_eq!(cause(&Outcome::Timeout), None);
    }

    #[test]
    fn why_names_no_job_each_untold_fact_once_and_one_run() {
        let runs = |a: (&str, f64), b: (&str, f64)| {
            [a, b].map(|(job, confidence)| rung::JobRun { job: job.into(), confidence })
        };
        assert_eq!(why("unclassified", &runs(("other", 0.9), ("other", 0.9)), &[]), ["no_job:other"]);
        assert_eq!(why("unclassified", &runs(("run_ship", 0.9), ("other", 0.9)), &[]), ["no_job:disagree"]);
        assert_eq!(why("unclassified", &runs(("run_ship", 0.6), ("run_ship", 0.9)), &[]), ["no_job:low_confidence"]);
        assert_eq!(why("unclassified", &[], &[]), ["no_job:not_recorded"]);
        let untold = format!("Not Delegated: AIQ couldn't tell whether {}.", rung::fact_words("F6"));
        let notes = [
            untold.clone(),
            untold,
            judge::ONE_RUN_NOTE.to_owned(),
            "Not Directed: AIQ couldn't see that x.".to_owned(),
        ];
        assert_eq!(why("code_change", &[], &notes), ["untold:F6", "one_run"]);
    }
}
