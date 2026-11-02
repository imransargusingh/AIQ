//! `aiq judge`: the rung judge runner (design `docs/design/rung-judge.md`).
//!
//! Works through closed, unjudged tasks (live and imported), newest first, within the person's daily
//! budget. Each task goes only to the provider that already saw it (US-5), through the person's own
//! signed-in tool, sealed: Claude Code `claude -p` in an empty folder with user settings only, hooks off,
//! no tools and no saved session; Copilot CLI `copilot -p` with no tools, no custom instructions and every
//! MCP server off. Two runs per task; `aiq_domain::rung::place` decides the rung. Every call and every
//! judgment is an append-only row. AIQ never reads or stores a sign-in.
use crate::import;
use aiq_contracts::codes;
use aiq_contracts::judge::{ActionSummary, Response, SCHEMA, extract_object, parse_response, response_schema};
use aiq_core::paths::{Paths, bounded_append};
use aiq_domain::clock::{DAY_MS, Now};
use aiq_domain::rung::{self, Turn};
use aiq_home::TaskRow;
use aiq_store::{CallOutcome, JudgmentRow, NewJudgeCall, Store};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A call that runs longer is killed (spec: 60 s p95 per judgment; one slow call must not hold the run).
pub const CALL_TIMEOUT: Duration = Duration::from_secs(120);
/// Back-off after the Nth failed call of a task (D8); the task is given up after `GIVE_UP` failed calls.
const BACKOFF_MS: [i64; 5] = [3_600_000, 6 * 3_600_000, DAY_MS, DAY_MS, DAY_MS];
const GIVE_UP: usize = 6;
/// A lock older than this belongs to a run that died.
const STALE_LOCK: Duration = Duration::from_secs(30 * 60);
const LOG_MAX: u64 = 256 * 1024;
const CLAUDE: &str = "claude-code";
const COPILOT: &str = "copilot-cli";

// ------------------------------------------------------------------ providers

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Ok(Value),
    Schema(String),
    Error(String),
    Timeout,
    NotSignedIn(String),
    Missing,
}

impl Outcome {
    fn label(&self) -> (&'static str, Option<&'static str>) {
        match self {
            Outcome::Ok(_) => ("ok", None),
            Outcome::Schema(_) => ("schema", Some(codes::JUDGE_SCHEMA)),
            Outcome::Error(_) => ("error", Some(codes::JUDGE_ERROR)),
            Outcome::Timeout => ("timeout", Some(codes::JUDGE_TIMEOUT)),
            Outcome::NotSignedIn(_) => ("not_signed_in", Some(codes::JUDGE_SIGNIN)),
            Outcome::Missing => ("missing", Some(codes::JUDGE_MISSING)),
        }
    }

    /// Why a call failed, for judge.log (local, owner-only, bounded); never stored in the database.
    fn detail(&self) -> &str {
        match self {
            Outcome::Schema(d) | Outcome::Error(d) | Outcome::NotSignedIn(d) => d,
            _ => "",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallResult {
    pub outcome: Outcome,
    pub model: Option<String>,
    pub latency_ms: i64,
    pub cost_usd: Option<f64>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
}

impl CallResult {
    fn failed(outcome: Outcome, latency_ms: i64) -> Self {
        CallResult { outcome, model: None, latency_ms, cost_usd: None, tokens_in: None, tokens_out: None }
    }
}

pub struct CallReq<'a> {
    pub system: &'a str,
    pub user: &'a str,
    pub schema: &'a Value,
    /// Copilot: the id AIQ gives the new session, recorded before the call (US-11).
    pub session_id: &'a str,
}

/// The person's own tool for one provider. Tests use a fake.
pub trait Provider {
    fn call(&self, req: &CallReq<'_>) -> CallResult;
}

/// Find a program on PATH the way Windows does for a bare name: `.exe` first, then `.cmd`.
fn find_exe(name: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat"] } else { &[""] };
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for e in exts {
            let p = dir.join(format!("{name}{e}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

struct Child {
    status: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
    ms: i64,
}

/// Run a child with `stdin`, a hard timeout, `AIQ_INTERNAL=1` (AIQ's hooks drop anything it does) and no window.
fn run_child(mut cmd: Command, stdin: &str, timeout: Duration) -> std::io::Result<Child> {
    let t0 = Instant::now();
    cmd.env(crate::hook::INTERNAL_ENV, "1").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: a background judge must not flash a console
    }
    let mut child = cmd.spawn()?;
    if let Some(mut si) = child.stdin.take() {
        let input = stdin.to_owned();
        std::thread::spawn(move || {
            let _ = si.write_all(input.as_bytes());
        });
    }
    let mut so = child.stdout.take().expect("piped");
    let mut se = child.stderr.take().expect("piped");
    let out = std::thread::spawn(move || {
        // Bytes, then lossy: one non-UTF-8 byte must not empty the whole answer.
        let mut b = Vec::new();
        let _ = so.read_to_end(&mut b);
        String::from_utf8_lossy(&b).into_owned()
    });
    let err = std::thread::spawn(move || {
        // Bytes, then lossy: one non-UTF-8 byte must not empty the whole answer.
        let mut b = Vec::new();
        let _ = se.read_to_end(&mut b);
        String::from_utf8_lossy(&b).into_owned()
    });
    let mut timed_out = false;
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st.code();
        }
        if t0.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            timed_out = true;
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    Ok(Child {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
        timed_out,
        ms: t0.elapsed().as_millis() as i64,
    })
}

fn signed_out(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    ["not logged in", "please run /login", "to authenticate", "authentication", "invalid api key", "login required"]
        .iter()
        .any(|w| t.contains(w))
}

fn short(s: &str) -> String {
    aiq_contracts::judge::plain(s, 300)
}

/// The person's own Claude Code, headless and sealed (ADR-0007; spec TR-10).
pub struct ClaudeCli {
    pub exe: Option<PathBuf>,
    pub model: String,
    pub scratch: PathBuf,
}

impl ClaudeCli {
    /// Every argument after the program. The system prompt and settings go in files in the empty folder.
    pub fn args(&self, schema: &Value, system_file: &Path) -> Vec<String> {
        vec![
            "-p".into(),
            "--model".into(),
            self.model.clone(),
            "--setting-sources".into(),
            "user".into(),
            "--settings".into(),
            r#"{"disableAllHooks":true,"autoMemoryEnabled":false}"#.into(),
            "--tools".into(),
            "".into(),
            "--strict-mcp-config".into(),
            "--mcp-config".into(),
            r#"{"mcpServers":{}}"#.into(),
            "--disable-slash-commands".into(),
            "--no-session-persistence".into(),
            "--output-format".into(),
            "json".into(),
            "--json-schema".into(),
            schema.to_string(),
            "--system-prompt-file".into(),
            system_file.display().to_string(),
        ]
    }
}

impl Provider for ClaudeCli {
    fn call(&self, req: &CallReq<'_>) -> CallResult {
        let Some(exe) = &self.exe else { return CallResult::failed(Outcome::Missing, 0) };
        // A fresh empty folder per call: no project CLAUDE.md, settings or hooks can load (TR-10).
        let dir = self.scratch.join(format!("aiq-judge-{}", uuid::Uuid::new_v4().simple()));
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return CallResult::failed(Outcome::Error(format!("scratch folder: {e}")), 0);
        }
        let sys = dir.join("system.txt");
        let r = std::fs::write(&sys, req.system).and_then(|_| {
            let mut c = Command::new(exe);
            c.args(self.args(req.schema, &sys)).current_dir(&dir);
            run_child(c, req.user, CALL_TIMEOUT)
        });
        let _ = std::fs::remove_dir_all(&dir);
        let run = match r {
            Ok(r) => r,
            Err(e) => return CallResult::failed(Outcome::Error(e.to_string()), 0),
        };
        if run.timed_out {
            return CallResult::failed(Outcome::Timeout, run.ms);
        }
        let d: Value = match serde_json::from_str(run.stdout.trim()) {
            Ok(v) => v,
            Err(_) => {
                let text = format!("{} {}", run.stderr, run.stdout);
                let o =
                    if signed_out(&text) { Outcome::NotSignedIn(short(&text)) } else { Outcome::Error(short(&text)) };
                return CallResult::failed(o, run.ms);
            }
        };
        let u = &d["usage"];
        let sum = |k: &[&str]| -> Option<i64> {
            let v: Vec<i64> = k.iter().filter_map(|k| u.get(*k).and_then(Value::as_i64)).collect();
            (!v.is_empty()).then(|| v.iter().sum())
        };
        let model = d["modelUsage"].as_object().and_then(|m| m.keys().next().cloned());
        let mut res = CallResult {
            outcome: Outcome::Error(String::new()),
            model,
            latency_ms: run.ms,
            cost_usd: d["total_cost_usd"].as_f64(),
            tokens_in: sum(&["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"]),
            tokens_out: sum(&["output_tokens"]),
        };
        res.outcome = if d["is_error"].as_bool() == Some(true) {
            let t = d["result"].as_str().unwrap_or("").to_owned();
            if signed_out(&t) { Outcome::NotSignedIn(short(&t)) } else { Outcome::Error(short(&t)) }
        } else {
            match d.get("structured_output") {
                Some(v) if v.is_object() => Outcome::Ok(v.clone()),
                _ => Outcome::Schema("no structured_output".into()),
            }
        };
        res
    }
}

/// The person's own Copilot CLI, sealed by running it with its own empty home (laptop probe 2026-11-02).
/// With `COPILOT_HOME` pointing at a fresh AIQ-owned folder, Copilot still signs in (the token lives in the
/// OS credential store) but loads none of the person's plugins, plugin MCP servers, agents, skills or MCP
/// config; `--disable-mcp-server` per name had missed a plugin's server. The judge's session, session
/// database and logs land in that folder too, never in `~/.copilot`, and AIQ deletes the folder after
/// reading the answer unless the person keeps judge sessions (Imran, 2026-11-02: delete by default).
pub struct CopilotCli {
    pub exe: Option<PathBuf>,
    pub model: Option<String>,
    /// AIQ's own working folder for Copilot judge calls (never a repo). Each call gets `copilot-<id>` in it.
    pub work: PathBuf,
    pub keep_sessions: bool,
}

impl CopilotCli {
    /// The sealed home for one call.
    pub fn call_home(&self, session_id: &str) -> PathBuf {
        self.work.join(format!("copilot-{session_id}"))
    }

    /// The prompt goes on stdin, never on the command line, where other local users could read it and
    /// Windows caps the length (review S6). Verified on the laptop: piped stdin runs one prompt and exits.
    pub fn args(&self, session_id: &str, usage_file: &Path) -> Vec<String> {
        let mut a: Vec<String> = vec![
            "-s".into(),
            "--session-id".into(),
            session_id.into(),
            "--available-tools=".into(),
            "--no-custom-instructions".into(),
            "--no-ask-user".into(),
            "--disable-builtin-mcps".into(),
            "--sync-session=off".into(),
            "--no-auto-update".into(),
            "--disable-client-telemetry".into(),
            "--no-color".into(),
            "--stream".into(),
            "off".into(),
            "--output-format".into(),
            "json".into(),
            "--log-dir".into(),
            self.call_home(session_id).join("logs").display().to_string(),
            "--usage-output-file".into(),
            usage_file.display().to_string(),
        ];
        if let Some(m) = &self.model {
            a.push("--model".into());
            a.push(m.clone());
        }
        a
    }
}

/// Read Copilot's JSON-lines output: the final answer's text, or why the call can't be used. A call where any
/// MCP server connected or the model asked for a tool is refused, so a seal that silently fails is a failed
/// call, never a judgment.
pub fn copilot_answer(stdout: &str) -> Result<String, Outcome> {
    let mut answer: Option<(bool, String)> = None;
    for line in stdout.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
        let ty = v["type"].as_str().unwrap_or("");
        if ty.starts_with("session.mcp_server") {
            let connected = line.contains("\"connected\"");
            let tools = v["data"]["toolCount"].as_u64().unwrap_or(0) > 0;
            if connected || tools {
                return Err(Outcome::Error(format!("not sealed: an MCP server was loaded ({})", short(line))));
            }
        }
        if ty == "assistant.message" {
            if v["data"]["toolRequests"].as_array().is_some_and(|t| !t.is_empty()) {
                return Err(Outcome::Error("not sealed: the model asked for a tool".into()));
            }
            let fin = v["data"]["phase"].as_str() == Some("final_answer");
            if let Some(c) = v["data"]["content"].as_str()
                && (fin || answer.as_ref().is_none_or(|(f, _)| !f))
            {
                answer = Some((fin, c.to_owned()));
            }
        }
    }
    answer.map(|(_, c)| c).ok_or_else(|| {
        Outcome::Schema(format!("no answer in Copilot's output ({} bytes): {}", stdout.len(), short(stdout)))
    })
}

impl Provider for CopilotCli {
    fn call(&self, req: &CallReq<'_>) -> CallResult {
        let Some(exe) = &self.exe else { return CallResult::failed(Outcome::Missing, 0) };
        let home = self.call_home(req.session_id);
        if let Err(e) = std::fs::create_dir_all(&home) {
            return CallResult::failed(Outcome::Error(format!("work folder: {e}")), 0);
        }
        let usage = self.work.join(format!("usage-{}.json", req.session_id));
        let prompt = format!(
            "{}\n\n{}\n\nReply with ONLY one JSON object that matches this JSON Schema, and nothing else:\n{}",
            req.system, req.user, req.schema
        );
        let mut c = Command::new(exe);
        c.args(self.args(req.session_id, &usage)).env("COPILOT_HOME", &home).current_dir(&home);
        let run = run_child(c, &prompt, CALL_TIMEOUT);
        let u: Value =
            std::fs::read_to_string(&usage).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let _ = std::fs::remove_file(&usage);
        if !self.keep_sessions {
            let _ = std::fs::remove_dir_all(&home);
        }
        let run = match run {
            Ok(r) => r,
            Err(e) => return CallResult::failed(Outcome::Error(e.to_string()), 0),
        };
        let model = u["modelMetrics"].as_object().and_then(|m| m.keys().next().cloned()).or_else(|| self.model.clone());
        let mut res = CallResult {
            outcome: Outcome::Error(String::new()),
            model,
            latency_ms: run.ms,
            // Copilot reports premium requests, not money: cost stays "not recorded".
            cost_usd: None,
            tokens_in: u["lastCallInputTokens"].as_i64().filter(|n| *n > 0),
            tokens_out: u["lastCallOutputTokens"].as_i64().filter(|n| *n > 0),
        };
        res.outcome = if run.timed_out {
            Outcome::Timeout
        } else if run.status != Some(0) {
            let text = format!("{} {}", run.stderr, run.stdout);
            if signed_out(&text) { Outcome::NotSignedIn(short(&text)) } else { Outcome::Error(short(&text)) }
        } else {
            match copilot_answer(&run.stdout) {
                Ok(text) => match extract_object(&text) {
                    Some(v) => Outcome::Ok(v),
                    None => Outcome::Schema(format!("answer is not one JSON object: {}", short(&text))),
                },
                Err(o) => o,
            }
        };
        res
    }
}

// ------------------------------------------------------------------ settings and queue

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// None until the person first runs `aiq judge` (consent by action, D4).
    pub mode: Option<String>,
    pub daily_tasks: u32,
    pub claude_model: String,
    pub copilot_model: Option<String>,
    pub keep_copilot_sessions: bool,
}

pub fn settings(store: &Store) -> Settings {
    let get = |k: &str| store.setting(k).ok().flatten();
    Settings {
        mode: get(rung::SETTING_JUDGE),
        daily_tasks: get(rung::SETTING_DAILY_TASKS).and_then(|v| v.parse().ok()).unwrap_or(rung::DEFAULT_DAILY_TASKS),
        claude_model: get(rung::SETTING_CLAUDE_MODEL).unwrap_or_else(|| rung::DEFAULT_CLAUDE_MODEL.into()),
        copilot_model: get(rung::SETTING_COPILOT_MODEL).filter(|m| m != "default"),
        keep_copilot_sessions: get(rung::SETTING_COPILOT_SESSIONS).as_deref() == Some("keep"),
    }
}

/// The provider that already saw a task (US-5, T-B5-1). VS Code agent mode fires Copilot's hooks (J1).
pub fn provider_for(tool: &str) -> Option<&'static str> {
    match tool {
        "claude-code" => Some(CLAUDE),
        "copilot-cli" | "vscode" => Some(COPILOT),
        _ => None,
    }
}

fn current_version(j: &JudgmentRow) -> bool {
    j.fact_set_version == rung::FACT_SET_VERSION
        && j.rubric_version == rung::RUBRIC_VERSION
        && j.judge_version == rung::JUDGE_VERSION
}

/// Tasks to judge now, best first: closed, not judged at the current versions, not waiting out a retry,
/// not given up. Unjudged before outdated; newest first within each.
pub fn queue<'a>(
    tasks: &'a [TaskRow],
    judged: &HashMap<String, JudgmentRow>,
    calls: &[CallOutcome],
    now_ms: i64,
    idle_gap_ms: i64,
    only: Option<&str>,
) -> Vec<&'a TaskRow> {
    let mut fails: HashMap<&str, (usize, i64)> = HashMap::new();
    for c in calls.iter().filter(|c| c.outcome != "ok") {
        let after_judgment = judged.get(&c.task_id).is_none_or(|j| c.started_utc_ms > j.judged_utc_ms);
        if after_judgment {
            let e = fails.entry(c.task_id.as_str()).or_default();
            e.0 += 1;
            e.1 = e.1.max(c.started_utc_ms);
        }
    }
    let mut q: Vec<(bool, &TaskRow)> = tasks
        .iter()
        .filter(|t| t.last_ms + idle_gap_ms < now_ms)
        .filter(|t| provider_for(&t.tool).is_some_and(|p| only.is_none_or(|o| o == p)))
        .filter(|t| judged.get(&t.task_id).is_none_or(|j| !current_version(j)))
        .filter(|t| match fails.get(t.task_id.as_str()) {
            None => true,
            Some((n, _)) if *n >= GIVE_UP => false,
            Some((n, last)) => now_ms >= last + BACKOFF_MS[(n - 1).min(BACKOFF_MS.len() - 1)],
        })
        .map(|t| (judged.contains_key(&t.task_id), t))
        .collect();
    q.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.first_ms.cmp(&a.1.first_ms)));
    q.into_iter().map(|(_, t)| t).collect()
}

/// Tasks that had a judge call since local midnight (the budget counts tasks, D5).
pub fn tasks_today(calls: &[CallOutcome], midnight_ms: i64) -> usize {
    calls.iter().filter(|c| c.started_utc_ms >= midnight_ms).map(|c| c.task_id.as_str()).collect::<HashSet<_>>().len()
}

// ------------------------------------------------------------------ task content

/// Where the tools keep their transcripts (the same places `aiq import` reads).
pub struct Homes {
    pub claude: PathBuf,
    pub copilot: PathBuf,
}

impl Homes {
    pub fn from_env() -> Self {
        Homes {
            claude: import::tool_home("CLAUDE_CONFIG_DIR", ".claude"),
            copilot: import::tool_home("COPILOT_HOME", ".copilot"),
        }
    }
}

fn mtime(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64)
}

/// The tool's own transcript of a live session, read with `aiq import`'s readers. None when it is gone.
fn transcript(homes: &Homes, tool: &str, session: &str) -> Option<import::Parsed> {
    if session.is_empty() || session.contains(['/', '\\']) || session.contains("..") {
        return None;
    }
    match provider_for(tool)? {
        CLAUDE => {
            let projects = homes.claude.join("projects");
            for d in std::fs::read_dir(&projects).ok()?.flatten() {
                let p = d.path().join(format!("{session}.jsonl"));
                if p.is_file() {
                    let slug = d.file_name().to_string_lossy().into_owned();
                    return import::read_claude(&p, session, mtime(&p), &slug).ok();
                }
            }
            None
        }
        _ => {
            let p = homes.copilot.join("session-state").join(session).join("events.jsonl");
            p.is_file().then(|| import::read_copilot(&p, session, mtime(&p)).ok()).flatten()
        }
    }
}

/// The task's turns (prompt text and the agent's actions after it) and whether actions were available.
/// Text the person deleted is skipped; an empty result means there is nothing left to judge.
pub fn task_turns(store: &Store, homes: &Homes, t: &TaskRow) -> aiq_store::Result<(Vec<Turn>, bool)> {
    let c = store.raw();
    if t.origin == "imported" {
        let mut turns = Vec::new();
        for (key, at) in &t.members {
            let mut parts = key.splitn(4, ':');
            let (_, tool, sid, seq) = (parts.next(), parts.next(), parts.next(), parts.next());
            let (Some(tool), Some(sid), Some(seq)) = (tool, sid, seq.and_then(|s| s.parse::<i64>().ok())) else {
                continue;
            };
            let text: Option<String> = c
                .query_row(
                    "SELECT text FROM import_event WHERE tool = ?1 AND source_session_id = ?2 AND seq = ?3",
                    rusqlite::params![tool, sid, seq],
                    |r| r.get(0),
                )
                .ok()
                .flatten();
            let actions: Option<String> = c
                .query_row(
                    "SELECT actions FROM import_event WHERE tool = ?1 AND source_session_id = ?2 AND seq = ?3 AND kind = 'stop'",
                    rusqlite::params![tool, sid, seq + 1],
                    |r| r.get(0),
                )
                .ok()
                .flatten();
            if let Some(text) = text.filter(|t| !t.trim().is_empty()) {
                turns.push(Turn { at_ms: *at, prompt: text, actions: actions.and_then(|a| ActionSummary::parse(&a)) });
            }
        }
        return Ok((turns, true));
    }
    let mut turns = Vec::new();
    for (id, at) in &t.members {
        let text: Option<String> = c.query_row("SELECT text FROM content WHERE event_id = ?1", [id], |r| r.get(0)).ok();
        if let Some(text) = text.filter(|t| !t.trim().is_empty()) {
            turns.push(Turn { at_ms: *at, prompt: text, actions: None });
        }
    }
    // Live events carry no actions: read them from the tool's own transcript, matching each prompt to the
    // transcript prompt nearest in time (within 2 minutes).
    let Some(p) = transcript(homes, &t.tool, &t.session) else { return Ok((turns, false)) };
    let mut tx: Vec<(i64, Option<ActionSummary>)> = Vec::new();
    for (i, ev) in p.events.iter().enumerate() {
        if ev.kind == "prompt"
            && let Some(ts) = ev.ts_utc_ms
        {
            let acts = p
                .events
                .get(i + 1)
                .filter(|n| n.kind == "stop")
                .and_then(|n| n.actions.as_deref())
                .and_then(ActionSummary::parse);
            tx.push((ts, acts));
        }
    }
    for turn in turns.iter_mut() {
        if let Some((_, a)) =
            tx.iter().filter(|(ts, _)| (ts - turn.at_ms).abs() <= 120_000).min_by_key(|(ts, _)| (ts - turn.at_ms).abs())
        {
            turn.actions = a.clone();
        }
    }
    Ok((turns, true))
}

// ------------------------------------------------------------------ one task

pub struct Judged {
    pub row: JudgmentRow,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn record_judge_session(paths: &Paths, id: &str) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(paths.data.join("judge-sessions.txt"))?;
    writeln!(f, "{id}")
}

/// Same-job tasks judged in the last 30 days (F11, the Factory repetition rule).
fn same_job_recent(judged: &HashMap<String, JudgmentRow>, job: &str, now_ms: i64) -> u32 {
    judged.values().filter(|j| j.job == job && j.judged_utc_ms >= now_ms - 30 * DAY_MS).count() as u32
}

/// Judge one task: two runs through `provider`, then placement. Returns None when no run answered.
/// Every call is recorded, answered or not.
#[allow(clippy::too_many_arguments)]
pub fn judge_task(
    store: &Store,
    paths: &Paths,
    provider: &dyn Provider,
    provider_name: &str,
    t: &TaskRow,
    turns: &[Turn],
    actions_known: bool,
    judged: &HashMap<String, JudgmentRow>,
    reprompt: &aiq_contracts::pack::RepromptParams,
) -> aiq_store::Result<(Option<Judged>, Vec<CallResult>)> {
    let log = rung::log_facts(turns, actions_known);
    let fence = format!("AIQ-QUOTE-{}", uuid::Uuid::new_v4().simple());
    let user = rung::envelope(&t.tool, turns, &log, &fence);
    let schema = response_schema();
    let mut runs: Vec<Response> = Vec::new();
    let mut results = Vec::new();
    let mut call_ids = Vec::new();
    let mut model = None;
    for run in 1..=2u8 {
        let session_id = uuid::Uuid::new_v4().to_string();
        if provider_name == COPILOT && record_judge_session(paths, &session_id).is_err() {
            // Never start a Copilot call AIQ could later mistake for the person's own session (US-11).
            break;
        }
        let started = now_ms();
        let mut r =
            provider.call(&CallReq { system: rung::SYSTEM, user: &user, schema: &schema, session_id: &session_id });
        if let Outcome::Ok(v) = &r.outcome {
            match parse_response(v.clone()) {
                Ok(resp) => runs.push(resp),
                Err(why) => r.outcome = Outcome::Schema(why),
            }
        }
        let (outcome, code) = r.outcome.label();
        let call_id = uuid::Uuid::now_v7().to_string();
        store.insert_judge_call(&NewJudgeCall {
            call_id: &call_id,
            task_id: &t.task_id,
            run,
            provider: provider_name,
            model: r.model.as_deref(),
            started_utc_ms: started,
            latency_ms: r.latency_ms,
            outcome,
            code,
            input_chars: (rung::SYSTEM.len() + user.len()) as i64,
            tool_session_id: (provider_name == COPILOT).then_some(session_id.as_str()),
            cost_usd: r.cost_usd,
            tokens_in: r.tokens_in,
            tokens_out: r.tokens_out,
        })?;
        call_ids.push(call_id);
        model = model.or(r.model.clone());
        let stop = matches!(r.outcome, Outcome::Missing | Outcome::NotSignedIn(_));
        results.push(r);
        if stop {
            break;
        }
    }
    if runs.is_empty() {
        return Ok((None, results));
    }
    let job = runs[0].job.clone();
    let (facts, mut p) = rung::place(&runs, &log, same_job_recent(judged, &job, now_ms()));
    if runs.len() < 2 {
        // Consistency needs two runs; one answer is shown, never coached.
        p.uncertain = true;
        p.coachable = false;
        p.state = rung::State::Uncertain;
        p.notes.push("Only one of the two judge runs answered, so AIQ marks this uncertain.".into());
        p.evidence.push(p.notes.last().cloned().unwrap_or_default());
    }
    let facts_json: serde_json::Map<String, Value> = facts
        .all()
        .into_iter()
        .map(|f| {
            (
                f.id.to_owned(),
                json!({"value": f.value, "confidence": f.confidence, "source": match f.source { rung::Source::Log => "log", rung::Source::Model => "model" }, "disputed": f.disputed}),
            )
        })
        .chain([("F3".to_owned(), json!({"value": facts.f3.map(|c| c.as_str()), "source": "log"}))])
        .collect();
    let chars: usize = turns.iter().map(|t| t.prompt.len()).sum();
    let read = json!({
        "schema": SCHEMA,
        "account": provider_name,
        "prompts": turns.len(),
        "chars": chars,
        "actions": turns.iter().any(|t| t.actions.is_some()),
        "keys": t.members.iter().map(|(k, _)| k).collect::<Vec<_>>(),
        "sent_chars": rung::SYSTEM.len() + user.len(),
        // Log facts for coaching (design coaching-from-rungs): re-prompts, and those that corrected the agent.
        "reprompts": log.reprompts,
        "corrections": rung::corrections(turns, reprompt),
    });
    let row = JudgmentRow {
        judgment_id: uuid::Uuid::now_v7().to_string(),
        task_id: t.task_id.clone(),
        origin: t.origin.to_owned(),
        provider: provider_name.to_owned(),
        model,
        judged_utc_ms: now_ms(),
        job: p.job.clone(),
        job_confidence: p.job_confidence,
        model_rung: p.model_rung,
        current_rung: p.current,
        current_confidence: p.current_confidence,
        potential_rung: p.potential,
        potential_confidence: p.potential_confidence,
        ceiling_rung: p.ceiling,
        state: p.state.as_str().to_owned(),
        coachable: p.coachable,
        facts: Value::Object(facts_json).to_string(),
        evidence: json!(p.evidence).to_string(),
        gap: p.gap.clone(),
        blockers: json!(p.blockers).to_string(),
        placement: json!(p.notes).to_string(),
        read: read.to_string(),
        call_ids: json!(call_ids).to_string(),
        fact_set_version: rung::FACT_SET_VERSION.into(),
        rubric_version: rung::RUBRIC_VERSION.into(),
        judge_version: rung::JUDGE_VERSION.into(),
        task_type: p.task_type.clone(),
        task_type_confidence: p.task_type.as_ref().map(|_| p.task_type_confidence),
    };
    store.insert_judgment(&row)?;
    Ok((Some(Judged { row }), results))
}

// ------------------------------------------------------------------ the run

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Report {
    pub judged: u32,
    pub uncertain: u32,
    pub failed: u32,
    pub skipped_no_text: u32,
    pub waiting: u32,
    pub budget_left: u32,
    /// Providers that could not be used this run, with why.
    pub unavailable: Vec<(String, String)>,
    pub cost_usd: f64,
}

pub struct Run<'a> {
    pub store: &'a Store,
    pub paths: &'a Paths,
    pub homes: &'a Homes,
    pub claude: &'a dyn Provider,
    pub copilot: &'a dyn Provider,
    pub now: Now,
    pub max: Option<u32>,
    /// `--only claude-code|copilot-cli`: judge only tasks that go to this provider.
    pub only: Option<&'a str>,
}

/// One pass over the queue within today's budget.
pub fn run(r: &Run<'_>) -> Result<Report, String> {
    let pack = aiq_core::pack::parse(aiq_core::pack::BUNDLED_TOML).map_err(|e| e.to_string())?;
    let gap = pack.idle_gap_min as i64 * 60_000;
    let set = settings(r.store);
    let tasks = aiq_home::tasks(r.store, &pack, 0)?;
    let judged = r.store.current_judgments().map_err(|e| e.to_string())?;
    let calls = r.store.judge_calls_since(r.now.utc_ms - 7 * DAY_MS).map_err(|e| e.to_string())?;
    let q = queue(&tasks, &judged, &calls, r.now.utc_ms, gap, r.only);
    let used = tasks_today(&calls, r.now.local_midnight_utc_ms()) as u32;
    let mut rep = Report { budget_left: set.daily_tasks.saturating_sub(used), ..Default::default() };
    let mut allowed = rep.budget_left.min(r.max.unwrap_or(u32::MAX));
    let mut down: HashSet<&str> = HashSet::new();
    let mut judged_now = judged.clone();
    for t in &q {
        let Some(name) = provider_for(&t.tool) else { continue };
        if allowed == 0 {
            rep.waiting += 1;
            continue;
        }
        if down.contains(name) {
            rep.waiting += 1;
            continue;
        }
        let (turns, known) = task_turns(r.store, r.homes, t).map_err(|e| e.to_string())?;
        if turns.is_empty() {
            rep.skipped_no_text += 1;
            continue;
        }
        let provider = if name == CLAUDE { r.claude } else { r.copilot };
        let (j, results) = judge_task(r.store, r.paths, provider, name, t, &turns, known, &judged_now, &pack.reprompt)
            .map_err(|e| e.to_string())?;
        allowed -= 1;
        rep.budget_left = rep.budget_left.saturating_sub(1);
        rep.cost_usd += results.iter().filter_map(|c| c.cost_usd).sum::<f64>();
        let line = match &j {
            Some(j) => format!(
                "judged {} {} rung={} potential={} state={} ms={}",
                t.task_id,
                name,
                j.row.current_rung,
                j.row.potential_rung,
                j.row.state,
                results.iter().map(|c| c.latency_ms).sum::<i64>()
            ),
            None => format!(
                "failed {} {} {:?}",
                t.task_id,
                name,
                results
                    .iter()
                    .map(|c| format!("{} {}", c.outcome.label().0, c.outcome.detail()).trim().to_owned())
                    .collect::<Vec<_>>()
            ),
        };
        let _ = bounded_append(&r.paths.data.join("judge.log"), &line, LOG_MAX);
        match j {
            Some(j) => {
                rep.judged += 1;
                if j.row.state == "uncertain" {
                    rep.uncertain += 1;
                }
                judged_now.insert(j.row.task_id.clone(), j.row);
            }
            None => {
                rep.failed += 1;
                if let Some(last) = results.last() {
                    match &last.outcome {
                        Outcome::Missing => {
                            down.insert(name);
                            rep.unavailable.push((name.to_owned(), "not installed (not found on PATH)".into()));
                        }
                        Outcome::NotSignedIn(why) => {
                            down.insert(name);
                            rep.unavailable.push((name.to_owned(), format!("not signed in: {why}")));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(rep)
}

// ------------------------------------------------------------------ lock

struct Lock(PathBuf);

impl Lock {
    fn take(path: PathBuf) -> Option<Self> {
        let fresh = |p: &Path| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|e| e < STALE_LOCK)
        };
        for _ in 0..2 {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    let _ = writeln!(f, "{}", std::process::id());
                    return Some(Lock(path));
                }
                Err(_) if !fresh(&path) => {
                    let _ = std::fs::remove_file(&path);
                }
                Err(_) => return None,
            }
        }
        None
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn lock_path(paths: &Paths) -> PathBuf {
    paths.data.join("judge.lock")
}

/// `aiq forget`: delete any Copilot session folder AIQ's own judge calls left (review S1), then the id list.
pub fn forget_judge_sessions(paths: &Paths, homes: &Homes) -> usize {
    let mut n = 0;
    for id in import::judge_ids(paths) {
        let dir = homes.copilot.join("session-state").join(&id);
        if uuid::Uuid::parse_str(&id).is_ok() && dir.is_dir() && std::fs::remove_dir_all(&dir).is_ok() {
            n += 1;
        }
    }
    // Judge calls since the sealed-home change keep their sessions in AIQ's own folder; kept ones go too.
    if let Ok(rd) = std::fs::read_dir(paths.data.join("judge-work")) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with("copilot-") && std::fs::remove_dir_all(e.path()).is_ok() {
                n += 1;
            }
        }
    }
    let _ = std::fs::remove_file(paths.data.join("judge-sessions.txt"));
    n
}

/// First background check after the core starts, then the interval between checks (D3).
const BACKGROUND_FIRST: Duration = Duration::from_secs(120);
const BACKGROUND_EVERY: Duration = Duration::from_secs(600);

/// The core's judge kicker: every 10 minutes, when judging is on, AIQ is not paused and no judge is running,
/// start `aiq judge --auto` detached. The core itself never waits on a model (C9).
pub fn start_background(paths: Paths) {
    std::thread::spawn(move || {
        std::thread::sleep(BACKGROUND_FIRST);
        loop {
            let on = Store::open_read_only(&paths.db())
                .ok()
                .and_then(|s| s.setting(rung::SETTING_JUDGE).ok().flatten())
                .is_some_and(|v| v == "auto");
            let busy = std::fs::metadata(lock_path(&paths))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|e| e < STALE_LOCK);
            if on && !busy && !aiq_core::control::is_paused(&paths) {
                let _ = crate::spawn::detached(&["judge", "--auto"]);
            }
            std::thread::sleep(BACKGROUND_EVERY);
        }
    });
}

// ------------------------------------------------------------------ CLI

const DISCLOSURE: &str = "AIQ judges each task you finish with AI: the rung it was done at and the next step.
What is sent: the task's prompts and a summary of what the agent did (tools, files, commands). Never your
own edits or diffs. Where: only to the tool that already saw the task, through your own sign-in:
  Claude Code tasks → your Claude Code (`claude -p`, no tools, no hooks, no saved session)
  Copilot tasks     → your Copilot CLI (no tools, none of your plugins or MCP servers; it runs in AIQ's
                      own folder, which AIQ deletes after each call unless you keep them:
                      aiq config set judge-copilot-sessions keep)
Judge calls use your plan's usage (two calls per task). A work account's admin policies, logging and
retention apply to them. AIQ never reads or stores your sign-in.";

fn now() -> Now {
    let utc = now_ms();
    let offset = chrono::Local::now().offset().local_minus_utc() / 60;
    Now { utc_ms: utc, offset_min: offset }
}

fn providers(paths: &Paths, set: &Settings) -> (ClaudeCli, CopilotCli) {
    (
        ClaudeCli { exe: find_exe("claude"), model: set.claude_model.clone(), scratch: std::env::temp_dir() },
        CopilotCli {
            exe: find_exe("copilot"),
            model: set.copilot_model.clone(),
            work: paths.data.join("judge-work"),
            keep_sessions: set.keep_copilot_sessions,
        },
    )
}

/// `aiq judge [--max <n>] [--only claude-code|copilot-cli] [--dry-run] [--auto]`
pub fn cli(args: &[&str]) -> ExitCode {
    let paths = Paths::from_env();
    let auto = args.contains(&"--auto");
    let dry = args.contains(&"--dry-run");
    let max = args.iter().position(|a| *a == "--max").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok());
    let only = args.iter().position(|a| *a == "--only").and_then(|i| args.get(i + 1)).copied();
    if only.is_some_and(|o| o != "claude-code" && o != "copilot-cli") {
        eprintln!("--only takes claude-code or copilot-cli.");
        return ExitCode::from(2);
    }
    if aiq_core::control::is_paused(&paths) {
        if !auto {
            println!("AIQ is paused, so it isn't judging. Resume with: aiq resume");
        }
        return ExitCode::SUCCESS;
    }
    let store = match import::open_store(&paths) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let set = settings(&store);
    if auto && set.mode.as_deref() != Some("auto") {
        return ExitCode::SUCCESS;
    }
    let homes = Homes::from_env();
    let n = now();
    if dry {
        return dry_run(&store, &homes, n, only);
    }
    let Some(_lock) = Lock::take(lock_path(&paths)) else {
        if !auto {
            println!("AIQ is already judging in the background. Try again in a few minutes.");
        }
        return ExitCode::SUCCESS;
    };
    if !auto && set.mode.is_none() {
        println!("{DISCLOSURE}\n");
        let _ = store.set_setting(rung::SETTING_JUDGE, "auto", n.utc_ms);
        println!(
            "From now on AIQ judges new tasks in the background, up to {} a day. Turn it off: aiq config set judge off\n",
            set.daily_tasks
        );
    }
    let (claude, copilot) = providers(&paths, &set);
    let rep = run(&Run {
        store: &store,
        paths: &paths,
        homes: &homes,
        claude: &claude,
        copilot: &copilot,
        now: n,
        max,
        only,
    });
    match rep {
        Ok(rep) => {
            let line = format!(
                "run judged={} uncertain={} failed={} waiting={} budget_left={} cost_usd={:.4}",
                rep.judged, rep.uncertain, rep.failed, rep.waiting, rep.budget_left, rep.cost_usd
            );
            let _ = bounded_append(&paths.data.join("judge.log"), &line, LOG_MAX);
            if !auto {
                print!("{}", render(&rep, set.daily_tasks));
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            let _ = bounded_append(&paths.data.join("judge.log"), &format!("error {e}"), LOG_MAX);
            eprintln!("AIQ could not judge: {e}");
            ExitCode::from(1)
        }
    }
}

pub fn render(rep: &Report, daily: u32) -> String {
    let mut o = format!("Judged {} task{}", rep.judged, if rep.judged == 1 { "" } else { "s" });
    if rep.uncertain > 0 {
        o += &format!(" ({} uncertain: shown, never coached)", rep.uncertain);
    }
    o += ".\n";
    if rep.failed > 0 {
        o += &format!("{} could not be judged this time; AIQ tries again later.\n", rep.failed);
    }
    if rep.skipped_no_text > 0 {
        o += &format!(
            "{} had no prompt text left (you deleted it), so there is nothing to judge.\n",
            rep.skipped_no_text
        );
    }
    for (p, why) in &rep.unavailable {
        o += &format!("{p}: {why}. Its tasks wait until it works.\n");
    }
    if rep.waiting > 0 {
        o += &format!(
            "{} more wait{} for tomorrow's budget ({daily} tasks a day; change it: aiq config set judge-daily-tasks <n>).\n",
            rep.waiting,
            if rep.waiting == 1 { "s" } else { "" }
        );
    }
    if rep.cost_usd > 0.0 {
        o += &format!("Cost Claude Code reported for these calls: ${:.2}.\n", rep.cost_usd);
    }
    o += "See them in AIQ home, Tasks.\n";
    o
}

fn dry_run(store: &Store, homes: &Homes, n: Now, only: Option<&str>) -> ExitCode {
    let pack = match aiq_core::pack::parse(aiq_core::pack::BUNDLED_TOML) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    let set = settings(store);
    let tasks = aiq_home::tasks(store, &pack, 0).unwrap_or_default();
    let judged = store.current_judgments().unwrap_or_default();
    let calls = store.judge_calls_since(n.utc_ms - 7 * DAY_MS).unwrap_or_default();
    let q = queue(&tasks, &judged, &calls, n.utc_ms, pack.idle_gap_min as i64 * 60_000, only);
    let used = tasks_today(&calls, n.local_midnight_utc_ms()) as u32;
    println!("{DISCLOSURE}\n");
    println!(
        "{} tasks are waiting to be judged; today's budget has {} of {} left. Nothing was sent.",
        q.len(),
        set.daily_tasks.saturating_sub(used),
        set.daily_tasks
    );
    if let Some(t) = q.first() {
        let (turns, known) = task_turns(store, homes, t).unwrap_or_default();
        let log = rung::log_facts(&turns, known);
        println!(
            "\nThe next task ({}, {}) would be sent to your {} like this:\n",
            t.task_id,
            t.tool,
            provider_for(&t.tool).unwrap_or("?")
        );
        println!("{}", rung::envelope(&t.tool, &turns, &log, "AIQ-QUOTE-xxxx"));
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use aiq_store::{ImportedEvent, ImportedSession, NewEvent};
    use std::cell::RefCell;
    use tempfile::TempDir;

    const MIN: i64 = 60_000;
    const NOW: i64 = 100 * DAY_MS;

    /// Answers every call with `answer` (or `fail`), and records what it was sent.
    struct Fake {
        answer: Option<Value>,
        fail: Option<Outcome>,
        sent: RefCell<Vec<(String, String)>>,
    }

    impl Fake {
        fn ok(rung: u8) -> Self {
            let f = json!({"value": true, "confidence": 0.9});
            let n = json!({"value": false, "confidence": 0.9});
            Fake {
                answer: Some(json!({
                    "job": "code_change", "job_confidence": 0.9, "task_type": "fix_bug", "task_type_confidence": 0.9,
                    "f4_goal_context_constraints": f, "f5_reprompts_added_information": f, "f6_output_checked": f,
                    "f7_written_brief": n, "f12_step_by_step": n, "f13_whole_outcome_handed_over": f, "fit": f,
                    "current_rung": rung, "current_confidence": 0.9, "potential_rung": rung + 1,
                    "potential_confidence": 0.9, "ceiling_rung": 5,
                    "evidence": ["You gave the agent the failing test."], "gap": "Let the agent run the tests.",
                    "blockers": []
                })),
                fail: None,
                sent: RefCell::new(vec![]),
            }
        }
        fn failing(o: Outcome) -> Self {
            Fake { answer: None, fail: Some(o), sent: RefCell::new(vec![]) }
        }
        fn calls(&self) -> usize {
            self.sent.borrow().len()
        }
    }

    impl Provider for Fake {
        fn call(&self, req: &CallReq<'_>) -> CallResult {
            self.sent.borrow_mut().push((req.user.to_owned(), req.session_id.to_owned()));
            let outcome = match (&self.answer, &self.fail) {
                (Some(v), _) => Outcome::Ok(v.clone()),
                (None, Some(o)) => o.clone(),
                _ => Outcome::Error("x".into()),
            };
            CallResult {
                outcome,
                model: Some("fake".into()),
                latency_ms: 5,
                cost_usd: Some(0.01),
                tokens_in: None,
                tokens_out: None,
            }
        }
    }

    fn prompt(s: &mut Store, id: &str, surface: &str, session: &str, at: i64, text: &str) {
        s.insert_event(
            &NewEvent {
                event_id: id,
                surface,
                session,
                kind: "prompt",
                ts_utc_ms: at,
                prompt_id: None,
                trace_id: None,
                collector_version: "t",
                prompt_chars: Some(text.len() as i64),
                payload_meta: "{}",
                text: Some(text),
            },
            at,
        )
        .unwrap();
    }

    struct Env {
        _d: TempDir,
        paths: Paths,
        homes: Homes,
        store: Store,
    }

    fn env() -> Env {
        let d = TempDir::new().unwrap();
        let paths = Paths::at(&d.path().join("aiq"));
        paths.ensure_dirs().unwrap();
        let homes = Homes { claude: d.path().join("claude"), copilot: d.path().join("copilot") };
        let mut store = Store::open(&paths.db()).unwrap();
        // Three closed live tasks (two Claude Code, one VS Code) and one still open.
        prompt(&mut store, "a1", "claude-code", "sa", NOW - 5 * 60 * MIN, "fix the flaky upload test");
        prompt(&mut store, "a2", "claude-code", "sa", NOW - 5 * 60 * MIN + MIN, "also run cargo test");
        prompt(&mut store, "b1", "claude-code", "sb", NOW - 3 * 60 * MIN, "write the ADR for parquet");
        prompt(&mut store, "v1", "vscode", "sv", NOW - 2 * 60 * MIN, "refactor the parser");
        prompt(&mut store, "o1", "claude-code", "so", NOW - 5 * MIN, "still working on this");
        // One imported Copilot task with the agent's actions.
        store
            .put_import_session(
                "run",
                &ImportedSession {
                    tool: "copilot-cli".into(),
                    source_session_id: "cs".into(),
                    source_mtime_ms: 1,
                    folder: "/repo".into(),
                    started_utc_ms: Some(NOW - 10 * DAY_MS),
                    human_turns: 1,
                    non_interactive: None,
                    template_key: None,
                    outcome: "imported".into(),
                    why: String::new(),
                    events: vec![
                        ImportedEvent {
                            kind: "prompt".into(),
                            ts_utc_ms: Some(NOW - 10 * DAY_MS),
                            text: Some("migrate the config loader".into()),
                            actions: None,
                        },
                        ImportedEvent {
                            kind: "stop".into(),
                            ts_utc_ms: Some(NOW - 10 * DAY_MS + MIN),
                            text: None,
                            actions: Some(r#"{"tools":{"edit":3,"bash":1},"files":["src/cfg.rs"],"commands":["cargo test"],"subagents":0}"#.into()),
                        },
                    ],
                },
            )
            .unwrap();
        Env { _d: d, paths, homes, store }
    }

    fn go(e: &Env, claude: &dyn Provider, copilot: &dyn Provider, max: Option<u32>) -> Report {
        go_only(e, claude, copilot, max, None)
    }

    fn go_only(e: &Env, claude: &dyn Provider, copilot: &dyn Provider, max: Option<u32>, only: Option<&str>) -> Report {
        run(&Run {
            store: &e.store,
            paths: &e.paths,
            homes: &e.homes,
            claude,
            copilot,
            now: Now { utc_ms: NOW, offset_min: 0 },
            max,
            only,
        })
        .unwrap()
    }

    #[test]
    fn only_judges_tasks_for_the_named_provider() {
        let e = env();
        let (c, p) = (Fake::ok(2), Fake::ok(2));
        let rep = go_only(&e, &c, &p, None, Some("copilot-cli"));
        assert_eq!(rep.judged, 2, "{rep:?}");
        assert_eq!((c.calls(), p.calls()), (0, 4));
    }

    #[test]
    fn judges_closed_tasks_through_the_provider_that_saw_them() {
        let e = env();
        let (c, p) = (Fake::ok(2), Fake::ok(2));
        let rep = go(&e, &c, &p, None);
        assert_eq!(rep.judged, 4, "{rep:?}");
        assert_eq!(c.calls(), 4, "two Claude Code tasks × two runs; the open task waits");
        assert_eq!(p.calls(), 4, "VS Code and the imported Copilot task go to Copilot");
        assert!(c.sent.borrow().iter().all(|(u, _)| !u.contains("refactor the parser") && !u.contains("migrate")));
        let j = e.store.current_judgments().unwrap();
        assert_eq!(j.len(), 4);
        assert!(!j.contains_key("o1"), "an open task is not judged");
        assert_eq!(j["a1"].current_rung, 2);
        assert_eq!(j["a1"].provider, "claude-code");
        assert_eq!(j["import:copilot-cli:cs:0"].origin, "imported");
        assert_eq!(j["v1"].provider, "copilot-cli");
        let read: Value = serde_json::from_str(&j["a1"].read).unwrap();
        assert_eq!(read["prompts"], 2);
        assert_eq!(e.store.judge_calls_since(0).unwrap().len(), 8);
        // Copilot session ids are recorded before the call, so import never reads them (US-11).
        let ids = import::judge_ids(&e.paths);
        assert!(p.sent.borrow().iter().all(|(_, sid)| ids.contains(sid)));
        // A second run finds nothing to do.
        let (c2, p2) = (Fake::ok(2), Fake::ok(2));
        assert_eq!(go(&e, &c2, &p2, None).judged, 0);
        assert_eq!(c2.calls() + p2.calls(), 0);
    }

    #[test]
    fn imported_actions_reach_the_log_facts_and_the_envelope() {
        let e = env();
        let (c, p) = (Fake::failing(Outcome::Missing), Fake::ok(3));
        go(&e, &c, &p, None);
        let sent = p.sent.borrow();
        let imp = sent.iter().find(|(u, _)| u.contains("migrate the config loader")).unwrap();
        assert!(imp.0.contains("agent edited files: yes (3 edits)"));
        assert!(imp.0.contains("checks (tests, build, lint) run by: agent"));
        assert!(imp.0.contains("AIQ-QUOTE-"));
    }

    #[test]
    fn newest_first_within_the_daily_budget() {
        let e = env();
        e.store.set_setting(rung::SETTING_DAILY_TASKS, "1", 0).unwrap();
        let (c, p) = (Fake::ok(2), Fake::ok(2));
        let rep = go(&e, &c, &p, None);
        assert_eq!((rep.judged, rep.waiting, rep.budget_left), (1, 3, 0));
        let j = e.store.current_judgments().unwrap();
        assert!(j.contains_key("v1"), "the newest closed task goes first");
        let rep = go(&e, &Fake::ok(2), &Fake::ok(2), None);
        assert_eq!(rep.judged, 0, "budget spent for today");
    }

    #[test]
    fn failures_wait_out_a_back_off_and_a_missing_tool_stops_its_provider() {
        let e = env();
        let c = Fake::failing(Outcome::Error("boom".into()));
        let p = Fake::failing(Outcome::Missing);
        let rep = go(&e, &c, &p, None);
        assert_eq!(rep.judged, 0);
        assert_eq!(c.calls(), 4, "each Claude task tried twice");
        assert_eq!(p.calls(), 1, "Copilot missing: one call, then its tasks wait");
        assert!(rep.unavailable.iter().any(|(n, _)| n == "copilot-cli"));
        assert!(e.store.current_judgments().unwrap().is_empty());
        // Straight after, the Claude tasks are in back-off: no new calls.
        let c2 = Fake::ok(2);
        go(&e, &c2, &Fake::ok(2), None);
        assert_eq!(c2.calls(), 0);
    }

    #[test]
    fn a_bad_answer_is_discarded() {
        let e = env();
        e.store.set_setting(rung::SETTING_DAILY_TASKS, "1", 0).unwrap();
        // Copilot gets the newest task (v1); its answer fails the schema.
        let mut bad = Fake::ok(2);
        bad.answer.as_mut().unwrap()["current_rung"] = json!(9);
        let rep = go(&e, &Fake::ok(2), &bad, None);
        assert_eq!((rep.judged, rep.failed), (0, 1));
        let calls = e.store.judge_calls_since(0).unwrap();
        assert!(calls.iter().all(|c| c.outcome == "schema"));
    }

    /// Answers the first call, times out on the second.
    struct Once(Fake, RefCell<u32>);
    impl Provider for Once {
        fn call(&self, req: &CallReq<'_>) -> CallResult {
            *self.1.borrow_mut() += 1;
            if *self.1.borrow() == 1 { self.0.call(req) } else { CallResult::failed(Outcome::Timeout, 120_000) }
        }
    }

    #[test]
    fn one_good_run_is_shown_as_uncertain_never_coached() {
        let e = env();
        e.store.set_setting(rung::SETTING_DAILY_TASKS, "1", 0).unwrap();
        let p = Once(Fake::ok(2), RefCell::new(0));
        let rep = go(&e, &Fake::ok(2), &p, None);
        assert_eq!(rep.judged, 1);
        let j = &e.store.current_judgments().unwrap()["v1"];
        assert_eq!(j.state, "uncertain");
        assert!(!j.coachable);
        assert!(j.evidence.contains("Only one of the two judge runs answered"));
    }

    #[test]
    fn queue_order_retry_and_give_up_rules() {
        let row = |id: &str, first: i64| TaskRow {
            task_id: id.into(),
            tool: "claude-code".into(),
            folder: None,
            first_ms: first,
            last_ms: first,
            prompts: 1,
            origin: "live",
            job: None,
            rung: None,
            judgment: None,
            session: "s".into(),
            members: vec![],
        };
        let tasks = vec![row("old", 1_000), row("new", 2_000), row("open", NOW - MIN), row("gone", 3_000)];
        let fail = |at: i64| CallOutcome {
            task_id: "gone".into(),
            started_utc_ms: at,
            outcome: "error".into(),
            cost_usd: None,
        };
        let calls: Vec<CallOutcome> = (0..6).map(|i| fail(NOW - DAY_MS * 3 + i)).collect();
        let q = queue(&tasks, &HashMap::new(), &calls, NOW, 30 * MIN, None);
        let ids: Vec<&str> = q.iter().map(|t| t.task_id.as_str()).collect();
        assert_eq!(ids, vec!["new", "old"], "open is not closed; gone gave up after 6 failed calls");
        let one = vec![fail(NOW - 30 * MIN)];
        let q = queue(&tasks, &HashMap::new(), &one, NOW, 30 * MIN, None);
        assert!(!q.iter().any(|t| t.task_id == "gone"), "within the first hour's back-off");
        let q = queue(&tasks, &HashMap::new(), &one, NOW + 31 * MIN, 30 * MIN, None);
        assert!(q.iter().any(|t| t.task_id == "gone"));
    }

    #[test]
    fn claude_call_is_sealed() {
        let c = ClaudeCli { exe: None, model: "sonnet".into(), scratch: PathBuf::from("/tmp") };
        let a = c.args(&json!({"type": "object"}), Path::new("/tmp/x/system.txt"));
        let has = |pair: &[&str]| a.windows(pair.len()).any(|w| w.iter().zip(pair).all(|(x, y)| x == y));
        assert!(has(&["--setting-sources", "user"]));
        assert!(has(&["--settings", r#"{"disableAllHooks":true,"autoMemoryEnabled":false}"#]));
        assert!(has(&["--tools", ""]));
        assert!(has(&["--no-session-persistence"]));
        assert!(has(&["--strict-mcp-config", "--mcp-config", r#"{"mcpServers":{}}"#]));
        assert!(has(&["--disable-slash-commands"]));
        assert!(has(&["--model", "sonnet"]));
        assert!(has(&["--output-format", "json"]));
        assert!(has(&["--json-schema"]));
        assert!(!a.iter().any(|x| x.contains("dangerously") || x.contains("allow")));
        assert_eq!(
            c.call(&CallReq { system: "", user: "", schema: &json!({}), session_id: "x" }).outcome,
            Outcome::Missing
        );
    }

    #[test]
    fn copilot_call_is_sealed_by_its_own_home() {
        let d = TempDir::new().unwrap();
        let c = CopilotCli { exe: None, model: None, work: d.path().join("work"), keep_sessions: false };
        let id = "0cb916db-26aa-40f2-86b5-1ba81b225fd2";
        let a = c.args(id, Path::new("u.json"));
        assert!(!a.contains(&"-p".to_string()), "the prompt never goes on the command line (S6)");
        let has = |pair: &[&str]| a.windows(pair.len()).any(|w| w.iter().zip(pair).all(|(x, y)| x == y));
        assert!(has(&["--available-tools="]));
        assert!(has(&["--no-custom-instructions"]));
        assert!(has(&["--disable-builtin-mcps"]));
        assert!(has(&["--output-format", "json"]));
        assert!(has(&["--session-id", id]));
        assert!(has(&["--no-ask-user"]) && has(&["--sync-session=off"]) && has(&["--no-auto-update"]));
        let logs = c.call_home(id).join("logs").display().to_string();
        assert!(has(&["--log-dir", logs.as_str()]), "Copilot's logs stay in the call's own folder");
        assert!(!a.iter().any(|x| x.contains("allow-all")));
        assert!(!a.contains(&"--model".to_string()), "the person's default model unless set");
    }

    fn line(ty: &str, data: Value) -> String {
        json!({ "type": ty, "data": data }).to_string()
    }

    #[test]
    fn copilot_output_gives_the_final_answer_or_refuses_an_unsealed_call() {
        let ans = r#"{"a":1}"#;
        let ok = [
            line("session.mcp_servers_loaded", json!({"servers": [{"name": "ado", "status": "disabled"}]})),
            line("assistant.message", json!({"content": "thinking", "phase": "commentary", "toolRequests": []})),
            line("assistant.message", json!({"content": ans, "phase": "final_answer", "toolRequests": []})),
            "not json".into(),
            line("result", json!({"exitCode": 0})),
        ]
        .join("\n");
        assert_eq!(copilot_answer(&ok).unwrap(), ans);
        let mcp = format!(
            "{}\n{}",
            line("session.mcp_servers_loaded", json!({"servers": [{"name": "workiq", "status": "connected"}]})),
            line("assistant.message", json!({"content": ans, "phase": "final_answer"}))
        );
        assert!(matches!(copilot_answer(&mcp), Err(Outcome::Error(e)) if e.contains("not sealed")));
        let tools = line("session.mcp_server_tools_loaded", json!({"name": "x", "toolCount": 12}));
        assert!(matches!(copilot_answer(&tools), Err(Outcome::Error(_))));
        let asked = line("assistant.message", json!({"content": ans, "toolRequests": [{"name": "bash"}]}));
        assert!(matches!(copilot_answer(&asked), Err(Outcome::Error(e)) if e.contains("tool")));
        assert!(matches!(copilot_answer(ans), Err(Outcome::Schema(_))), "plain text is not Copilot's JSON output");
    }

    #[test]
    fn copilot_gets_the_prompt_on_stdin_and_a_sealed_home_that_is_deleted_by_default() {
        let d = TempDir::new().unwrap();
        let id = "0cb916db-26aa-40f2-86b5-1ba81b225fd2";
        let mk = |keep: bool| CopilotCli {
            exe: Some(fake_copilot(d.path())),
            model: None,
            work: d.path().join("work"),
            keep_sessions: keep,
        };
        for keep in [false, true] {
            let c = mk(keep);
            let r = c.call(&CallReq { system: "s", user: "u", schema: &json!({}), session_id: id });
            assert!(matches!(r.outcome, Outcome::Ok(_)), "{:?}", r.outcome);
            assert_eq!(c.call_home(id).exists(), keep);
        }
    }

    #[cfg(unix)]
    fn fake_copilot(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("copilot");
        // Answers only when the prompt arrives on stdin, AIQ_INTERNAL=1 is set (S3, S6) and COPILOT_HOME is
        // the call's own existing folder; writes a session there as the real one does.
        std::fs::write(
            &p,
            "#!/bin/sh\nread -r first\n[ -n \"$first\" ] && [ \"$AIQ_INTERNAL\" = 1 ] && [ -d \"$COPILOT_HOME\" ] \
             && case \"$COPILOT_HOME\" in */work/copilot-*) true;; *) false;; esac \
             && mkdir -p \"$COPILOT_HOME/session-state/x\" \
             && echo '{\"type\":\"assistant.message\",\"data\":{\"content\":\"{\\\"ok\\\": true}\",\"phase\":\"final_answer\",\"toolRequests\":[]}}' \
             || echo nope\n",
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[cfg(windows)]
    fn fake_copilot(dir: &Path) -> PathBuf {
        let p = dir.join("copilot.cmd");
        std::fs::write(
            &p,
            "@if \"%AIQ_INTERNAL%\"==\"1\" if exist \"%COPILOT_HOME%\" (echo {\"type\":\"assistant.message\",\"data\":{\"content\":\"{\\\"ok\\\": true}\",\"phase\":\"final_answer\",\"toolRequests\":[]}}) else (echo nope)\r\n",
        )
        .unwrap();
        p
    }

    #[test]
    fn forget_removes_leftover_judge_sessions_and_only_those() {
        let e = env();
        let id = "0cb916db-26aa-40f2-86b5-1ba81b225fd2";
        record_judge_session(&e.paths, id).unwrap();
        let kept = e.paths.data.join("judge-work").join(format!("copilot-{id}"));
        std::fs::create_dir_all(&kept).unwrap();
        let mine = e.homes.copilot.join("session-state").join(id);
        let theirs = e.homes.copilot.join("session-state").join("1cb916db-26aa-40f2-86b5-1ba81b225fd2");
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::create_dir_all(&theirs).unwrap();
        assert_eq!(forget_judge_sessions(&e.paths, &e.homes), 2);
        assert!(!mine.exists() && theirs.exists() && !kept.exists());
        assert!(import::judge_ids(&e.paths).is_empty());
    }

    #[test]
    fn signed_out_text_is_recognised() {
        assert!(signed_out("Error: Authentication token found but could not be validated. To authenticate, ..."));
        assert!(signed_out("Not logged in · Please run /login"));
        assert!(!signed_out("rate limited"));
    }
}

/// Measured accuracy on the Phase 0 held-out tasks (each written at a known rung), through the real
/// `claude -p` on this machine. Costs real usage, so it runs only on request:
/// `AIQ_JUDGE_EVAL=prototypes/phase0-spike/data/heldout_scenarios.json cargo test -p aiq --bin aiq judge_eval -- --ignored --nocapture`
/// `AIQ_JUDGE_EVAL_CACHE=<file>` saves the model's answers on the first run and replays them after, so a
/// change to placement is measured on the same answers at no cost.
#[cfg(test)]
mod eval {
    use super::*;
    use std::collections::BTreeMap;

    fn turns(steps: &[Value]) -> Vec<Turn> {
        let mut out: Vec<Turn> = Vec::new();
        for (i, s) in steps.iter().enumerate() {
            match s["kind"].as_str() {
                Some("user") | Some("command") => out.push(Turn {
                    at_ms: i as i64,
                    prompt: s["text"].as_str().or(s["command"].as_str()).unwrap_or("").to_owned(),
                    actions: Some(ActionSummary::default()),
                }),
                Some("tool") => {
                    if let Some(t) = out.last_mut() {
                        let a = t.actions.get_or_insert_with(ActionSummary::default);
                        let name = s["tool"].as_str().unwrap_or("?");
                        *a.tools.entry(name.to_owned()).or_default() += 1;
                        if let Some(p) = s["path"].as_str() {
                            a.files.push(p.to_owned());
                        }
                        if let Some(c) = s["command"].as_str() {
                            a.commands.push(c.to_owned());
                        }
                        if name == "Task" {
                            a.subagents += 1;
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    #[test]
    #[ignore = "calls the real claude -p; set AIQ_JUDGE_EVAL"]
    fn judge_eval() {
        let path = std::env::var("AIQ_JUDGE_EVAL").expect("AIQ_JUDGE_EVAL=<heldout_scenarios.json>");
        let model = std::env::var("AIQ_JUDGE_MODEL").unwrap_or_else(|_| rung::DEFAULT_CLAUDE_MODEL.into());
        let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let p = ClaudeCli { exe: find_exe("claude"), model, scratch: std::env::temp_dir() };
        let schema = response_schema();
        let cache_path = std::env::var("AIQ_JUDGE_EVAL_CACHE").ok();
        let mut cache: BTreeMap<String, Vec<Value>> = cache_path
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|s| serde_json::from_str(&s).unwrap())
            .unwrap_or_default();
        let (mut exact, mut within, mut job_ok, mut n, mut cost, mut ms) = (0, 0, 0, 0, 0.0, Vec::new());
        let mut rows = Vec::new();
        for c in &cases {
            let t = turns(c["steps"].as_array().unwrap());
            let log = rung::log_facts(&t, true);
            let user = rung::envelope("claude-code", &t, &log, &format!("AIQ-QUOTE-{}", uuid::Uuid::new_v4().simple()));
            let id = c["id"].as_str().unwrap().to_owned();
            let answers = cache.entry(id).or_insert_with(|| {
                let mut got = Vec::new();
                for _ in 0..2 {
                    let r = p.call(&CallReq { system: rung::SYSTEM, user: &user, schema: &schema, session_id: "" });
                    cost += r.cost_usd.unwrap_or(0.0);
                    ms.push(r.latency_ms);
                    if let Outcome::Ok(v) = r.outcome {
                        got.push(v);
                    }
                }
                got
            });
            let runs: Vec<Response> = answers.iter().filter_map(|v| parse_response(v.clone()).ok()).collect();
            if runs.is_empty() {
                rows.push(format!("{} no answer", c["id"]));
                continue;
            }
            let (_, pl) = rung::place(&runs, &log, 0);
            let want = c["rung"].as_u64().unwrap() as u8;
            let want_job = c["job"].as_str().unwrap();
            let got_job = if pl.job == "unclassified" { "other" } else { pl.job.as_str() };
            n += 1;
            exact += (pl.current == want) as u32;
            within += (pl.current.abs_diff(want) <= 1) as u32;
            job_ok += (got_job == want_job || (want_job == "other" && runs[0].job == "other")) as u32;
            rows.push(format!(
                "{} want {} got {} (model {}) state {} job {}/{}",
                c["id"],
                want,
                pl.current,
                pl.model_rung,
                pl.state.as_str(),
                want_job,
                runs[0].job
            ));
            rows.extend(pl.notes.iter().map(|n| format!("    {n}")));
        }
        if let Some(f) = &cache_path {
            std::fs::write(f, serde_json::to_string(&cache).unwrap()).unwrap();
        }
        ms.sort();
        let pct = |q: usize| ms.get((ms.len().max(1) - 1) * q / 100).map_or("replayed".to_owned(), |m| m.to_string());
        for r in &rows {
            println!("{r}");
        }
        println!(
            "EVAL n={n} exact={exact} within_one={within} job={job_ok} cost_usd={cost:.3} call_ms_p50={} p95={}",
            pct(50),
            pct(95)
        );
    }
}
