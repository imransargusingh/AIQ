//! The diagnostics file (design docs/design/diagnostics-file.md): how AIQ ran on this computer over the last
//! [`DAYS`] days, in one JSON file the person can read and choose to share. AIQ sends it nowhere.
//!
//! What it never holds, whatever the person chooses: the analysis's own sentences about a task (evidence, gap,
//! placement, what was read), team tip text, folder and file names (each folder is a letter that
//! changes with every file), the person's user, account, email and computer names, and any web address but the
//! project's public repository. Prompts go in only when the person ticks "Include my prompts", already stripped of
//! secrets when stored and scrubbed again here. Every free-text line passes through [`aiq_domain::scrub`].
//!
//! Reads the store read-only, in one transaction, like the snapshot; works whether or not the core runs.
use aiq_contracts::pack::PackV1;
use aiq_core::paths::Paths;
use aiq_domain::clock::Now;
use aiq_domain::scrub::Scrub;
use aiq_store::Store;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// The window: the last 14 days, enough for a week-on-week look and a fresh install's whole history.
pub const DAYS: i64 = 14;
/// The format tag; a reader checks it before reading on.
pub const FORMAT: &str = "aiq.diagnostics.v1";
/// Longest log line or tool message kept (characters), and longest prompt when prompts are included.
pub const LINE_MAX: usize = 300;
pub const PROMPT_MAX: usize = 4000;
/// Log lines kept per log file, newest last.
pub const LOG_LINES: usize = 200;

/// What the person chose.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// "Include my prompts": each task's prompt text, scrubbed. Off unless the person ticks it.
    pub prompts: bool,
}

/// The file's text and the numbers the dialog shows beside it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Diagnostics {
    pub text: String,
    pub tasks: usize,
    /// Prompts written into the file (0 unless the person chose them).
    pub prompts: usize,
    pub bytes: usize,
    /// The file name it will be saved under (in Downloads).
    pub name: String,
}

/// What is never in the file, in the words the dialog and the file's own `about` use.
pub const LEFT_OUT: &[&str] = &[
    "the agent's replies",
    "what the analysis wrote about a task (its evidence, gap and placement sentences)",
    "file and folder names (each folder is a letter, new in every file)",
    "your user name, account, email and computer name",
    "team tip text (a team tip shows as its tip id only)",
    "web addresses other than github.com/imransargusingh",
];

/// The person's names from the environment, and the team tips address in every form, for [`Scrub`].
pub fn scrub_from_env() -> Scrub {
    let var = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    let (home, user, computer) = if cfg!(windows) {
        (var("USERPROFILE"), var("USERNAME"), var("COMPUTERNAME"))
    } else {
        (var("HOME"), var("USER"), var("HOSTNAME"))
    };
    Scrub { home, user, computer, never: never_forms(aiq_core::team_tips::repo_url().as_deref()) }
}

/// An address, its host, and its path with and without `.git`: none may appear in the file.
fn never_forms(url: Option<&str>) -> Vec<String> {
    let Some(u) = url.map(str::trim).filter(|u| !u.is_empty()) else { return vec![] };
    let mut out = vec![u.to_owned()];
    let rest = u.split_once("://").map_or(u, |(_, r)| r);
    let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
    if let Some((host, path)) = rest.split_once('/') {
        out.push(host.to_owned());
        let p = path.trim_end_matches('/');
        out.push(p.to_owned());
        out.push(p.trim_end_matches(".git").to_owned());
    } else {
        out.push(rest.to_owned());
    }
    out.retain(|s| s.len() >= 4);
    out.sort();
    out.dedup();
    out
}

/// The person's Downloads folder, else their profile folder.
pub fn downloads() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    match home {
        Some(h) if h.join("Downloads").is_dir() => h.join("Downloads"),
        Some(h) => h,
        None => std::env::temp_dir(),
    }
}

/// `aiq-diagnostics-2026-11-10-1640.json`, in local time.
pub fn file_name(now: Now) -> String {
    let (y, mo, d, h, mi, _) = civil(now.utc_ms + now.offset_min as i64 * 60_000);
    format!("aiq-diagnostics-{y:04}-{mo:02}-{d:02}-{h:02}{mi:02}.json")
}

/// Write the file into `dir` under `name`, never over an existing file (`-2`, `-3`… are added). Returns its path.
pub fn save(dir: &Path, name: &str, text: &str) -> std::io::Result<PathBuf> {
    use std::io::Write;
    let stem = name.trim_end_matches(".json");
    for n in 1..100 {
        let p = dir.join(if n == 1 { name.to_owned() } else { format!("{stem}-{n}.json") });
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(mut f) => {
                f.write_all(text.as_bytes())?;
                return Ok(p);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "99 files of this name already exist"))
}

/// Build the file. `core` is whether the core answered just now (None: not asked).
pub fn build(
    paths: &Paths,
    pack: &PackV1,
    now: Now,
    opts: Options,
    scrub: &Scrub,
    core: Option<bool>,
) -> Result<Diagnostics, String> {
    let since = now.utc_ms - DAYS * aiq_domain::clock::DAY_MS;
    let mut root = Map::new();
    root.insert(
        "about".into(),
        json!({
            "format": FORMAT,
            "made": iso(now.utc_ms),
            "days": DAYS,
            "prompts": opts.prompts,
            "left_out": if opts.prompts { LEFT_OUT.to_vec() } else {
                std::iter::once("your prompts").chain(LEFT_OUT.iter().copied()).collect::<Vec<_>>()
            },
            "note": "AIQ sends this file nowhere. Read it before you share it.",
        }),
    );
    root.insert(
        "machine".into(),
        json!({ "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "utc_offset_min": now.offset_min }),
    );
    let update = aiq_core::update::view(paths, None, now.utc_ms);
    root.insert(
        "aiq".into(),
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "core": match core { Some(true) => "answering", Some(false) => "not answering", None => "not asked" },
            "paused": aiq_core::control::is_paused(paths),
            "setup_done": aiq_core::control::setup_done(paths),
            "update": {
                "running": update.running,
                "note": update.note,
                "available": update.available,
                "checked": update.checked_ms.map(iso),
                "check_error": update.check_error.as_deref().map(|e| scrub.line(e, LINE_MAX)),
                "reason": update.reason.as_deref().map(|e| scrub.line(e, LINE_MAX)),
                "state_file": update_state_file(paths, scrub),
            },
            "folder": scrub.line(&paths.data.display().to_string(), LINE_MAX),
        }),
    );
    root.insert(
        "tools".into(),
        Value::Array(
            aiq_core::tools::found()
                .iter()
                .map(|t| json!({ "tool": t.tool, "present": t.present, "connected": t.hooked }))
                .collect(),
        ),
    );
    // Which Copilot CLI analysis and label calls run (the judge's pick, DC-055), user folder scrubbed. Null until
    // a call has picked one.
    let pick = std::fs::read_to_string(paths.data.join("copilot-exe.txt")).ok();
    root.insert("copilot_exe".into(), pick.map_or(Value::Null, |p| scrub.line(p.trim(), LINE_MAX).into()));
    root.insert("copilot".into(), copilot_facts(paths, scrub));

    let db = paths.db();
    let (mut tasks_n, mut prompts_n) = (0, 0);
    if db.exists() {
        let store = Store::open_read_only(&db).map_err(|e| format!("{}: {}", e.code, e.detail))?;
        store.raw().execute_batch("BEGIN").map_err(|e| e.to_string())?;
        let r = from_store(&store, pack, since, opts, scrub, &mut root);
        let _ = store.raw().execute_batch("COMMIT");
        (tasks_n, prompts_n) = r?;
    } else {
        root.insert("store".into(), json!("AIQ has stored nothing yet"));
    }

    let mut logs = Map::new();
    for (name, keep) in [
        ("label.log", None),
        ("judge.log", None),
        ("aiq.log", Some(&["\"WARN\"", "\"ERROR\"", "core started"][..])),
        ("hook-errors.log", None),
        // What each update check and install did, and the lines the log couldn't take (AIQ-UPDATE-LOG-HELD): a failed
        // update is diagnosable from this file alone (devbox, 2026-11-13: 0.1.55 failed on every Check for updates,
        // and the error was only in upgrade-log.txt, which the person would have had to find and send by hand).
        ("upgrade-log.txt", None),
        ("upgrade-log-held.txt", None),
    ] {
        logs.insert(name.into(), Value::Array(tail(&paths.data.join(name), keep, scrub)));
    }
    root.insert("logs".into(), Value::Object(logs));

    let text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())? + "\n";
    Ok(Diagnostics { bytes: text.len(), text, tasks: tasks_n, prompts: prompts_n, name: file_name(now) })
}

/// update.json as upgrade.ps1 wrote it, every text value scrubbed (the error words can hold a folder or an address);
/// null when there is none, and a note when it isn't JSON.
fn update_state_file(paths: &Paths, scrub: &Scrub) -> Value {
    let Ok(b) = std::fs::read(aiq_core::update::state_path(paths)) else { return Value::Null };
    let b = b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&b);
    match serde_json::from_slice::<Value>(b) {
        Ok(Value::Object(m)) => Value::Object(
            m.into_iter()
                .map(|(k, v)| {
                    let v = match v {
                        Value::String(t) => Value::from(scrub.line(&t, LINE_MAX)),
                        Value::Array(_) | Value::Object(_) => Value::from("(not kept)"),
                        v => v,
                    };
                    (scrub.line(&k, 80), v)
                })
                .collect(),
        ),
        _ => Value::from("update.json is not a JSON object"),
    }
}

/// Each folder as a letter, in first-seen order: A … Z, AA, AB …
#[derive(Default)]
struct Letters(HashMap<String, String>);

impl Letters {
    fn of(&mut self, folder: Option<&str>) -> Value {
        let Some(f) = folder.filter(|f| !f.is_empty()) else { return Value::Null };
        let n = self.0.len();
        self.0
            .entry(f.to_owned())
            .or_insert_with(|| {
                let mut s = String::new();
                let mut k = n;
                loop {
                    s.insert(0, (b'A' + (k % 26) as u8) as char);
                    if k < 26 {
                        break;
                    }
                    k = k / 26 - 1;
                }
                s
            })
            .clone()
            .into()
    }
}

fn q<T>(r: rusqlite::Result<T>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

fn has(store: &Store, t: &str) -> Result<bool, String> {
    store.has_table(t).map_err(|e| e.to_string())
}

/// Every store section. Returns (tasks, prompts written).
fn from_store(
    store: &Store,
    pack: &PackV1,
    since: i64,
    opts: Options,
    scrub: &Scrub,
    root: &mut Map<String, Value>,
) -> Result<(usize, usize), String> {
    let c = store.raw();
    let mut letters = Letters::default();

    // Settings: the newest value of each. A project's name is keyed by its folder, so only their count goes in.
    let mut settings = Map::new();
    let mut named = 0;
    {
        let mut st = q(c.prepare("SELECT key, value FROM current_setting ORDER BY key"))?;
        let rows = q(st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))))?;
        for r in rows {
            let (k, v) = q(r)?;
            if k.starts_with(aiq_core::service::SETTING_PROJECT_NAME) {
                named += 1;
            } else {
                settings.insert(scrub.line(&k, 80), scrub.line(&v, 120).into());
            }
        }
    }
    if named > 0 {
        settings.insert("project names set".into(), named.into());
    }
    root.insert("settings".into(), Value::Object(settings));

    let tasks = crate::tasks(store, pack, since)?;
    let schema = store.user_version().map_err(|e| e.to_string())?;
    let events: i64 = q(c.query_row("SELECT count(*) FROM event WHERE ts_utc_ms >= ?1", [since], |r| r.get(0)))?;
    let set_aside: Vec<(String, String, String, i64)> = if has(store, "set_aside_session")? {
        let mut st = q(c.prepare(
            "SELECT surface, folder, reason, utc_ms FROM set_aside_session WHERE utc_ms >= ?1 ORDER BY utc_ms",
        ))?;
        let rows = q(st.query_map([since], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))))?;
        q(rows.collect())?
    } else {
        vec![]
    };
    root.insert(
        "counts".into(),
        json!({
            "schema": schema,
            "events": events,
            "tasks": tasks.len(),
            "set_aside_sessions": set_aside.len(),
            "store_kb": store.size_bytes() / 1024,
        }),
    );

    // Label calls, each with the tool's own words from label.log ("why <task> <message>").
    let labels =
        if has(store, "kind_label")? { store.kind_labels_since(since).map_err(|e| e.to_string())? } else { vec![] };
    let why = label_why(&store.path().with_file_name("label.log"), scrub);
    let mut label_by: BTreeMap<(String, String, String), u32> = BTreeMap::new();
    for l in &labels {
        let msg = if l.outcome == "ok" { String::new() } else { why.get(&l.task_id).cloned().unwrap_or_default() };
        *label_by.entry((l.provider.clone(), l.outcome.clone(), msg)).or_default() += 1;
    }
    root.insert(
        "label_calls".into(),
        json!({
            "total": labels.len(),
            "by_outcome": label_by.iter().map(|((p, o, m), n)| json!({"count": n, "tool": p, "outcome": o, "message": m})).collect::<Vec<_>>(),
        }),
    );

    // Analysis calls: outcome and stable code only (the tool's words are in judge.log below).
    let mut calls: HashMap<String, Vec<Value>> = HashMap::new();
    let mut call_by: BTreeMap<(String, String, String), u32> = BTreeMap::new();
    if has(store, "judge_call")? {
        let mut st = q(c.prepare(
            "SELECT task_id, provider, model, outcome, code, latency_ms FROM judge_call WHERE started_utc_ms >= ?1
             ORDER BY started_utc_ms, rowid",
        ))?;
        let rows = q(st.query_map([since], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
            ))
        }))?;
        for r in rows {
            let (task, provider, model, outcome, code, ms) = q(r)?;
            *call_by.entry((provider.clone(), outcome.clone(), code.clone().unwrap_or_default())).or_default() += 1;
            calls
                .entry(task)
                .or_default()
                .push(json!({"tool": provider, "model": model, "outcome": outcome, "code": code, "ms": ms}));
        }
    }
    root.insert(
        "analysis_calls".into(),
        json!({
            "total": call_by.values().sum::<u32>(),
            "by_outcome": call_by.iter().map(|((p, o, k), n)| json!({"count": n, "tool": p, "outcome": o, "code": k})).collect::<Vec<_>>(),
        }),
    );

    // Tip path: every step per task, and how many tasks ended at each.
    let steps = if has(store, "tip_path")? { store.tip_steps_since(since).map_err(|e| e.to_string())? } else { vec![] };
    let mut path_of: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut last: BTreeMap<String, String> = BTreeMap::new();
    for s in &steps {
        path_of.entry(s.task_id.clone()).or_default().push(json!({"step": s.step, "at": iso(s.utc_ms)}));
        last.insert(s.task_id.clone(), s.step.clone());
    }
    let mut ended: BTreeMap<String, u32> = BTreeMap::new();
    for s in last.values() {
        *ended.entry(s.clone()).or_default() += 1;
    }
    root.insert("tip_path_ends".into(), json!(ended));

    let guesses: HashMap<String, Vec<Value>> = if has(store, "type_guess")? {
        let mut m: HashMap<String, Vec<Value>> = HashMap::new();
        for g in store.type_guesses().map_err(|e| e.to_string())? {
            m.entry(g.task_id.clone()).or_default().push(json!({
                "method": g.method, "job": g.job, "kind": g.task_type, "similarity": round(g.similarity),
            }));
        }
        m
    } else {
        HashMap::new()
    };
    let mut labels_of: HashMap<String, Vec<Value>> = HashMap::new();
    for l in &labels {
        labels_of.entry(l.task_id.clone()).or_default().push(json!({
            "tool": l.provider, "model": l.model, "outcome": l.outcome,
            "job": l.job, "job_confidence": l.job_confidence.map(round),
            "kind": l.task_type, "kind_confidence": l.task_type_confidence.map(round),
            "ms": l.latency_ms, "wait_ms": l.utc_ms - l.requested_utc_ms,
            "message": if l.outcome == "ok" { None } else { why.get(&l.task_id).cloned() },
        }));
    }
    let judged = store.current_judgments().map_err(|e| e.to_string())?;
    let tips = tips_of(store, since, scrub)?;

    // One entry per task in the window, plus any task with a tip path or label that the task list leaves out.
    let mut out: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for t in &tasks {
        seen.insert(t.task_id.clone());
        let mut e = Map::new();
        e.insert("task".into(), t.task_id.clone().into());
        e.insert("tool".into(), t.tool.clone().into());
        e.insert("origin".into(), t.origin.into());
        e.insert("started".into(), iso(t.first_ms).into());
        e.insert("folder".into(), letters.of(t.folder.as_deref()));
        e.insert("prompts".into(), t.prompts.into());
        if opts.prompts {
            let mut texts = Vec::new();
            for (key, _) in &t.members {
                let text = store.task_first_prompt(key).map_err(|e| e.to_string())?.and_then(|(_, t)| t);
                if let Some(text) = text {
                    texts.push(Value::from(scrub.line(&text, PROMPT_MAX)));
                } else {
                    texts.push(Value::Null);
                }
            }
            e.insert("prompt_text".into(), Value::Array(texts));
        }
        fill(&mut e, &t.task_id, &guesses, &labels_of, &judged, &calls, &path_of, &tips);
        out.push(Value::Object(e));
    }
    let extra: std::collections::BTreeSet<&String> =
        path_of.keys().chain(labels_of.keys()).filter(|k| !seen.contains(*k)).collect();
    for id in extra {
        let mut e = Map::new();
        e.insert("task".into(), id.clone().into());
        e.insert("note".into(), "not in the task list (set aside, or its prompt was deleted)".into());
        fill(&mut e, id, &guesses, &labels_of, &judged, &calls, &path_of, &tips);
        out.push(Value::Object(e));
    }
    let prompts_written = if opts.prompts {
        out.iter()
            .filter_map(|t| t.get("prompt_text").and_then(Value::as_array))
            .flatten()
            .filter(|v| v.is_string())
            .count()
    } else {
        0
    };
    root.insert("tasks".into(), Value::Array(out));
    root.insert(
        "set_aside".into(),
        Value::Array(
            set_aside
                .iter()
                .map(|(s, f, r, at)| json!({"tool": s, "folder": letters.of(Some(f)), "reason": r, "at": iso(*at)}))
                .collect(),
        ),
    );
    Ok((tasks.len(), prompts_written))
}

#[allow(clippy::too_many_arguments)]
fn fill(
    e: &mut Map<String, Value>,
    id: &str,
    guesses: &HashMap<String, Vec<Value>>,
    labels: &HashMap<String, Vec<Value>>,
    judged: &HashMap<String, aiq_store::JudgmentRow>,
    calls: &HashMap<String, Vec<Value>>,
    path: &BTreeMap<String, Vec<Value>>,
    tips: &HashMap<String, Vec<Value>>,
) {
    e.insert("guess".into(), guesses.get(id).cloned().map_or(Value::Null, Value::Array));
    e.insert("labels".into(), labels.get(id).cloned().map_or(Value::Null, Value::Array));
    e.insert("analysis".into(), judged.get(id).map_or(Value::Null, analysis));
    e.insert("analysis_calls".into(), calls.get(id).cloned().map_or(Value::Null, Value::Array));
    e.insert("tip_path".into(), path.get(id).cloned().map_or(Value::Null, Value::Array));
    e.insert("tips".into(), tips.get(id).cloned().map_or(Value::Null, Value::Array));
}

/// A judgment's numbers and codes. Its sentences (evidence, gap, placement, blockers, read) are left out: the
/// model wrote them about the person's work and they can name it.
fn analysis(j: &aiq_store::JudgmentRow) -> Value {
    let facts: Map<String, Value> = aiq_contracts::judge::parse_stored_facts(&j.facts)
        .into_iter()
        .map(|(k, f)| (k, json!({"value": f.value, "checker": f.checker, "confidence": round(f.confidence)})))
        .collect();
    json!({
        "origin": j.origin, "tool": j.provider, "model": j.model, "at": iso(j.judged_utc_ms),
        "job": j.job, "job_confidence": round(j.job_confidence),
        "kind": j.task_type, "kind_confidence": j.task_type_confidence.map(round),
        "rung": j.current_rung, "rung_confidence": round(j.current_confidence),
        "next_rung": j.potential_rung, "ceiling": j.ceiling_rung,
        "state": j.state, "coachable": j.coachable, "facts": facts,
        "versions": { "facts": j.fact_set_version, "rubric": j.rubric_version, "analysis": j.judge_version },
    })
}

/// Each tip decided for a task: what matched (detector, opportunity, score), the decision and its reason code, the
/// tip as the card held it (pack wording with the person's count or kind filled in; it never quotes a prompt), and
/// whether it was shown, opened and answered. A team tip's "Tool to try" line is added only when a card is drawn and
/// is never stored, so it can't be here.
fn tips_of(store: &Store, since: i64, scrub: &Scrub) -> Result<HashMap<String, Vec<Value>>, String> {
    let c = store.raw();
    let mut out: HashMap<String, Vec<Value>> = HashMap::new();
    let mut st = q(c.prepare(
        "SELECT s.task_id, d.card_id, d.outcome, d.reason_code, d.rule_id, d.card_json,
                s.detector_id, s.opportunity_id, s.score, d.pack_version, d.utc_ms, (SELECT delivered_utc_ms FROM card_delivery v WHERE v.card_id = d.card_id),
                EXISTS (SELECT 1 FROM card_open o WHERE o.card_id = d.card_id)
         FROM card_decision d JOIN signal s ON s.signal_id = d.signal_id
         WHERE d.utc_ms >= ?1 ORDER BY d.utc_ms, d.rowid",
    ))?;
    let rows = q(st.query_map([since], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            (r.get::<_, String>(6)?, r.get::<_, String>(7)?, r.get::<_, f64>(8)?, r.get::<_, String>(9)?),
            r.get::<_, i64>(10)?,
            r.get::<_, Option<i64>>(11)?,
            r.get::<_, bool>(12)?,
        ))
    }))?;
    let answers: HashMap<String, String> = if store.has_table("tip_feedback").map_err(|e| e.to_string())? {
        store
            .tip_feedback_all()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter_map(|f| f.card_id.map(|c| (c, f.answer)))
            .collect()
    } else {
        HashMap::new()
    };
    for r in rows {
        let (task, card, outcome, reason, rule, card_json, (detector, opportunity, score, pack), at, shown, opened) =
            q(r)?;
        let c: Value = serde_json::from_str(&card_json).unwrap_or_default();
        let words = |k: &str| c.get(k).and_then(Value::as_str).map(|t| scrub.line(t, LINE_MAX));
        out.entry(task).or_default().push(json!({
            "matched": { "detector": detector, "opportunity": opportunity, "score": round(score) },
            "decision": outcome, "reason": reason, "rule": rule, "pack": pack, "at": iso(at),
            "tip": c.get("tip_id"),
            "noticed": words("noticed"), "why": words("why"), "try_this": words("try_this"), "starter": words("starter"),
            "shown": shown.map(iso), "opened": opened, "answer": answers.get(&card),
        }));
    }
    Ok(out)
}

/// label.log "why <task> <message>" lines, newest per task, scrubbed.
fn label_why(log: &Path, scrub: &Scrub) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for l in read_lines(log) {
        if let Some(rest) = l.strip_prefix("why ")
            && let Some((task, msg)) = rest.split_once(' ')
        {
            m.insert(task.to_owned(), scrub.line(msg, LINE_MAX));
        }
    }
    m
}

/// The file and its one rotated copy (`<name>.1` or `<stem>.1.log`), oldest first.
fn read_lines(p: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let s = p.display().to_string();
    let prev = [format!("{s}.1"), s.strip_suffix(".log").map(|b| format!("{b}.1.log")).unwrap_or_default()];
    for f in prev.iter().filter(|f| !f.is_empty()).map(PathBuf::from).chain(std::iter::once(p.to_path_buf())) {
        if let Ok(b) = std::fs::read(&f) {
            out.extend(String::from_utf8_lossy(&b).lines().map(str::to_owned));
        }
    }
    out
}

/// The last [`LOG_LINES`] lines of a log (only those holding one of `keep`, when given), scrubbed.
fn tail(p: &Path, keep: Option<&[&str]>, scrub: &Scrub) -> Vec<Value> {
    let lines: Vec<String> =
        read_lines(p).into_iter().filter(|l| keep.is_none_or(|k| k.iter().any(|w| l.contains(w)))).collect();
    let from = lines.len().saturating_sub(LOG_LINES);
    lines[from..].iter().map(|l| Value::from(scrub.line(l, LINE_MAX))).collect()
}

fn round(x: f64) -> Value {
    serde_json::Number::from_f64((x * 100.0).round() / 100.0).map_or(Value::Null, Value::Number)
}

/// `2026-11-10T23:40:12Z`
pub fn iso(ms: i64) -> String {
    let (y, mo, d, h, mi, s) = civil(ms);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Milliseconds since 1970 to (year, month, day, hour, minute, second), proleptic Gregorian (Hinnant's days_from_civil
/// inverse).
fn civil(ms: i64) -> (i64, u32, u32, u32, u32, u32) {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d, (rem / 3600) as u32, (rem % 3600 / 60) as u32, (rem % 60) as u32)
}

/// Where AIQ looks for Copilot CLI and its logs (DC-056), paths scrubbed, nothing run: what the Copilot hook saw
/// of the Copilot running it, every `copilot` on this process's PATH, the home AIQ reads session logs from, and
/// that home's session folders (a count and the newest one's file names, never their content).
fn copilot_facts(paths: &Paths, scrub: &Scrub) -> Value {
    let p = |x: &std::path::Path| Value::from(scrub.line(&x.display().to_string(), LINE_MAX));
    let seen = aiq_core::copilot_seen::read(paths);
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat"] } else { &[""] };
    let on_path: Vec<Value> = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect::<Vec<_>>())
        .unwrap_or_default()
        .iter()
        .flat_map(|d| exts.iter().map(move |e| d.join(format!("copilot{e}"))))
        .filter(|f| f.is_file())
        .map(|f| p(&f))
        .collect();
    let home = aiq_core::copilot_seen::log_home(paths);
    let state = home.join("session-state");
    let mut dirs: Vec<(std::time::SystemTime, std::path::PathBuf)> = std::fs::read_dir(&state)
        .map(|r| {
            r.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()))
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    let newest_files: Vec<Value> = dirs
        .last()
        .and_then(|(_, d)| std::fs::read_dir(d).ok())
        .map(|r| {
            let mut n: Vec<String> = r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            n.sort();
            n.into_iter().take(20).map(|f| Value::from(scrub.line(&f, 80))).collect()
        })
        .unwrap_or_default();
    json!({
        "hook_saw": seen.as_ref().map(|s| json!({
            "exe": s.exe.as_deref().map(|e| p(std::path::Path::new(e))),
            "hook_dir": s.hook_dir.as_deref().map(|e| p(std::path::Path::new(e))),
            "home_env": s.home_env.as_deref().map(|e| p(std::path::Path::new(e))),
            "callable": s.copilot_exe().is_some(),
            "at": iso(s.at_ms),
        })),
        "on_path": on_path,
        "log_home": p(&home),
        "session_folders": dirs.len(),
        "newest_session_files": newest_files,
    })
}
