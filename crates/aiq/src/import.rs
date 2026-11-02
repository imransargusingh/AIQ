//! `aiq import`: seed AIQ with AI sessions that ran before it was installed (docs/specs/history-import.md).
//!
//! Reads the transcripts the tools already keep on this machine, read-only:
//!   Claude Code   `~/.claude/projects/<folder>/<session>.jsonl` (or `$CLAUDE_CONFIG_DIR`)
//!   Copilot CLI   `~/.copilot/session-state/<session>/events.jsonl` (or `$COPILOT_HOME`)
//! Automated sessions are set aside by the spec's rule (H4, no question asked). Folders where a burst or a
//! repeated first prompt shows an app at work get the same `folder_rule` row the live hooks write, so live
//! capture ignores them too (US-12), and `aiq include` undoes both. Everything else is stored in the import
//! tables, which the live coaching loop never reads, so imports raise no cards (US-5).
//! Judging imported tasks waits for the rung judge; nothing here calls a model or the network.
use aiq_contracts::pack::AutomatedParams;
use aiq_core::paths::Paths;
use aiq_domain::automated::{folder_key, template_of};
use aiq_store::{ImportRunCounts, ImportedEvent, ImportedSession, KnownImport, Store};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const CLAUDE: &str = "claude-code";
pub const COPILOT: &str = "copilot-cli";
/// Folder prior: a folder over this share automated takes its remaining one-prompt sessions too.
const FOLDER_PRIOR: f64 = 0.8;
const MAX_TEXT: usize = 64 * 1024;
const MAX_CMD: usize = 200;

/// One source session, read into memory.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parsed {
    pub tool: &'static str,
    pub id: String,
    pub mtime_ms: i64,
    pub folder: String,
    pub started_ms: Option<i64>,
    /// `Some(true)` when the tool marked a non-interactive start (Claude Code `entrypoint = sdk-cli`).
    pub non_interactive: Option<bool>,
    /// A claude.ai Projects / Remote Control session (`projects::is_marker` on any record).
    pub projects: bool,
    pub events: Vec<ImportedEvent>,
}

impl Parsed {
    pub fn human_turns(&self) -> u32 {
        self.events.iter().filter(|e| e.kind == "prompt").count() as u32
    }
    fn first_prompt(&self) -> Option<&str> {
        self.events.iter().find(|e| e.kind == "prompt").and_then(|e| e.text.as_deref())
    }
    pub fn rule_input(&self, p: &AutomatedParams) -> RuleInput {
        RuleInput {
            folder: folder_key(&self.folder),
            started_ms: self.started_ms,
            human_turns: self.human_turns(),
            non_interactive: self.non_interactive,
            projects: self.projects,
            template: self.first_prompt().and_then(|t| template_hash(t, p)),
        }
    }
}

/// What the automated rule reads about one session. Stored with each imported session, so a re-run applies
/// the rule to unchanged sessions without re-reading their files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RuleInput {
    /// `automated::folder_key`: Windows paths compare case-insensitively.
    pub folder: String,
    pub started_ms: Option<i64>,
    pub human_turns: u32,
    pub non_interactive: Option<bool>,
    /// Rule RC. Not a column of its own: an earlier run's `why` holds it.
    pub projects: bool,
    pub template: Option<u64>,
}

impl From<&KnownImport> for RuleInput {
    fn from(k: &KnownImport) -> Self {
        RuleInput {
            folder: folder_key(&k.folder),
            started_ms: k.started_utc_ms,
            human_turns: k.human_turns,
            non_interactive: k.non_interactive,
            projects: k.why.split(',').any(|w| w == "RC"),
            template: k.template_key.map(|v| v as u64),
        }
    }
}

// ------------------------------------------------------------------ readers

fn ts_ms(v: &Value) -> Option<i64> {
    v.as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp_millis())
}

fn cap(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut i = max;
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    s[..i].to_owned()
}

/// Per-turn summary of what the agent did: tool counts, files touched, commands run, sub-agents.
#[derive(Default)]
struct Actions {
    tools: BTreeMap<String, u32>,
    files: Vec<String>,
    commands: Vec<String>,
    subagents: u32,
}

impl Actions {
    fn note(&mut self, name: &str, args: &Value) {
        *self.tools.entry(name.to_owned()).or_default() += 1;
        let path = ["file_path", "path", "notebook_path"].iter().find_map(|k| args.get(*k).and_then(Value::as_str));
        if let Some(p) = path
            && self.files.len() < 50
            && !self.files.iter().any(|f| f == p)
        {
            self.files.push(p.to_owned());
        }
        if let Some(c) = args.get("command").and_then(Value::as_str)
            && self.commands.len() < 30
        {
            self.commands.push(cap(c, MAX_CMD));
        }
        if matches!(name, "Task" | "Agent") {
            self.subagents += 1;
        }
    }
    fn take(&mut self) -> Option<String> {
        if self.tools.is_empty() {
            return None;
        }
        let a = std::mem::take(self);
        Some(
            serde_json::json!({"tools": a.tools, "files": a.files, "commands": a.commands, "subagents": a.subagents})
                .to_string(),
        )
    }
}

fn lines(path: &Path) -> std::io::Result<impl Iterator<Item = Value>> {
    let f = std::fs::File::open(path)?;
    Ok(BufReader::new(f).lines().map_while(Result::ok).filter_map(|l| serde_json::from_str::<Value>(&l).ok()))
}

/// Claude Code transcript. A typed prompt is a `user` record whose content is a string (not tool results),
/// not a sidechain (sub-agent) and not meta. A turn ends at the next prompt or the end of the file.
pub fn read_claude(path: &Path, id: &str, mtime_ms: i64, folder_slug: &str) -> std::io::Result<Parsed> {
    let mut p =
        Parsed { tool: CLAUDE, id: id.to_owned(), mtime_ms, folder: folder_slug.to_owned(), ..Default::default() };
    let mut acts = Actions::default();
    let mut last_ts = None;
    let mut open = false;
    for r in lines(path)? {
        if r.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        p.projects |= crate::projects::is_marker(&r);
        if let Some(cwd) = r.get("cwd").and_then(Value::as_str)
            && p.folder == folder_slug
        {
            p.folder = cwd.to_owned();
        }
        if p.non_interactive.is_none()
            && let Some(ep) = r.get("entrypoint").and_then(Value::as_str)
        {
            p.non_interactive = Some(ep == "sdk-cli");
        }
        let ts = ts_ms(&r["timestamp"]);
        let content = &r["message"]["content"];
        match r.get("type").and_then(Value::as_str) {
            Some("user") if r.get("isMeta").and_then(Value::as_bool) != Some(true) => {
                if let Some(text) = content.as_str().filter(|t| !t.trim().is_empty()) {
                    if open {
                        p.events.push(ImportedEvent {
                            kind: "stop".into(),
                            ts_utc_ms: last_ts,
                            text: None,
                            actions: acts.take(),
                        });
                    }
                    p.started_ms = p.started_ms.or(ts);
                    p.events.push(ImportedEvent {
                        kind: "prompt".into(),
                        ts_utc_ms: ts,
                        text: Some(cap(text, MAX_TEXT)),
                        actions: None,
                    });
                    open = true;
                }
            }
            Some("assistant") if open => {
                for b in content.as_array().into_iter().flatten() {
                    if b["type"] == "tool_use" {
                        acts.note(b["name"].as_str().unwrap_or("?"), &b["input"]);
                    }
                }
            }
            _ => {}
        }
        if ts.is_some() {
            last_ts = ts;
        }
    }
    if open {
        p.events.push(ImportedEvent { kind: "stop".into(), ts_utc_ms: last_ts, text: None, actions: acts.take() });
    }
    Ok(p)
}

/// Copilot CLI event stream. A typed prompt is a main-line `user.message` delivered `idle` or `steering`
/// (sub-agent events carry `agentId`); the turn ends at `assistant.turn_end` or the next prompt.
pub fn read_copilot(path: &Path, id: &str, mtime_ms: i64) -> std::io::Result<Parsed> {
    let mut p = Parsed { tool: COPILOT, id: id.to_owned(), mtime_ms, folder: "(none)".into(), ..Default::default() };
    let mut acts = Actions::default();
    let mut open = false;
    let mut last_ts = None;
    for e in lines(path)? {
        let main = e.get("agentId").is_none_or(Value::is_null);
        let d = &e["data"];
        let ts = ts_ms(&e["timestamp"]);
        match e.get("type").and_then(Value::as_str) {
            Some("session.start") => {
                if let Some(cwd) = d["context"]["cwd"].as_str() {
                    p.folder = cwd.to_owned();
                }
                p.started_ms = p.started_ms.or(ts_ms(&d["startTime"])).or(ts);
            }
            Some("user.message") if main && matches!(d["delivery"].as_str(), Some("idle" | "steering")) => {
                if open {
                    p.events.push(ImportedEvent {
                        kind: "stop".into(),
                        ts_utc_ms: last_ts,
                        text: None,
                        actions: acts.take(),
                    });
                }
                p.started_ms = p.started_ms.or(ts);
                p.events.push(ImportedEvent {
                    kind: "prompt".into(),
                    ts_utc_ms: ts,
                    text: Some(cap(d["content"].as_str().unwrap_or(""), MAX_TEXT)),
                    actions: None,
                });
                open = true;
            }
            Some("tool.execution_start") if main && open => {
                let args = match &d["arguments"] {
                    Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
                    v => v.clone(),
                };
                acts.note(d["toolName"].as_str().unwrap_or("?"), &args);
            }
            Some("subagent.started") if open => acts.subagents += 1,
            Some("assistant.turn_end") if main && open => {
                p.events.push(ImportedEvent { kind: "stop".into(), ts_utc_ms: ts, text: None, actions: acts.take() });
                open = false;
            }
            _ => {}
        }
        if ts.is_some() {
            last_ts = ts;
        }
    }
    if open {
        p.events.push(ImportedEvent { kind: "stop".into(), ts_utc_ms: last_ts, text: None, actions: acts.take() });
    }
    Ok(p)
}

// ------------------------------------------------------------------ discovery

/// A source session file, before reading it.
#[derive(Debug, Clone)]
pub struct Source {
    pub tool: &'static str,
    pub id: String,
    pub path: PathBuf,
    pub mtime_ms: i64,
    pub folder_hint: String,
}

fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64)
}

pub fn discover(claude_home: &Path, copilot_home: &Path) -> Vec<Source> {
    let mut out = Vec::new();
    if let Ok(dirs) = std::fs::read_dir(claude_home.join("projects")) {
        for d in dirs.flatten().filter(|d| d.path().is_dir()) {
            let slug = d.file_name().to_string_lossy().into_owned();
            for f in std::fs::read_dir(d.path()).into_iter().flatten().flatten() {
                let path = f.path();
                if path.extension().is_some_and(|x| x == "jsonl") {
                    let id = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    out.push(Source { tool: CLAUDE, id, mtime_ms: mtime_ms(&path), path, folder_hint: slug.clone() });
                }
            }
        }
    }
    if let Ok(dirs) = std::fs::read_dir(copilot_home.join("session-state")) {
        for d in dirs.flatten() {
            let path = d.path().join("events.jsonl");
            if path.is_file() {
                let id = d.file_name().to_string_lossy().into_owned();
                out.push(Source { tool: COPILOT, id, mtime_ms: mtime_ms(&path), path, folder_hint: String::new() });
            }
        }
    }
    out.sort_by(|a, b| (a.tool, &a.id).cmp(&(b.tool, &b.id)));
    out
}

pub fn read(s: &Source) -> std::io::Result<Parsed> {
    if s.tool == CLAUDE {
        read_claude(&s.path, &s.id, s.mtime_ms, &s.folder_hint)
    } else {
        read_copilot(&s.path, &s.id, s.mtime_ms)
    }
}

// ------------------------------------------------------------------ automated rule

/// A stable hash of the live side's template normalisation (`automated::template_of`). Stable across builds
/// (FNV-1a, not std's `DefaultHasher`), because it is stored and compared with later runs' hashes.
pub fn template_hash(prompt: &str, p: &AutomatedParams) -> Option<u64> {
    let t = template_of(prompt, p.template_chars)?;
    Some(t.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)))
}

/// The spec's automated rule, over a whole history at once. Returns, per session, the signals that set it
/// aside (empty = kept). A session is automated when it has at most one human prompt AND a second signal:
/// NI non-interactive start, T shared first-prompt template (`template_sessions`+ in the folder),
/// B burst (`burst_sessions`+ started in the folder within ±`burst_min` minutes),
/// F folder prior (folder over 80% automated). RC, a claude.ai Projects / Remote Control session, is set aside
/// whatever its prompt count (Claude's own session, `projects.rs`). Folders in `include` are never set aside
/// ("Include them").
/// The live side (`automated::Guard`) uses the same thresholds; T there counts within `template_window_h`
/// because live it sees only recent sessions, while import sees the folder's whole history (the tested rule).
pub fn automated(sessions: &[RuleInput], include: &HashSet<String>, p: &AutomatedParams) -> Vec<Vec<&'static str>> {
    let mut groups: HashMap<(&str, u64), usize> = HashMap::new();
    let mut starts: HashMap<&str, Vec<i64>> = HashMap::new();
    for s in sessions {
        if let Some(k) = s.template {
            *groups.entry((s.folder.as_str(), k)).or_default() += 1;
        }
        if let Some(t) = s.started_ms {
            starts.entry(s.folder.as_str()).or_default().push(t);
        }
    }
    for v in starts.values_mut() {
        v.sort_unstable();
    }
    let window = i64::from(p.burst_min) * 60_000;
    let mut why: Vec<Vec<&'static str>> = sessions
        .iter()
        .map(|s| {
            if include.contains(&s.folder) {
                return vec![];
            }
            if s.projects {
                return vec!["RC"];
            }
            if s.human_turns > 1 {
                return vec![];
            }
            let mut w = vec![];
            if s.non_interactive == Some(true) {
                w.push("NI");
            }
            if s.template.is_some_and(|k| groups[&(s.folder.as_str(), k)] >= p.template_sessions as usize) {
                w.push("T");
            }
            if let (Some(t), Some(v)) = (s.started_ms, starts.get(s.folder.as_str())) {
                let lo = v.partition_point(|x| *x < t - window);
                let hi = v.partition_point(|x| *x <= t + window);
                if hi - lo >= p.burst_sessions as usize {
                    w.push("B");
                }
            }
            w
        })
        .collect();
    let mut per_folder: HashMap<&str, (usize, usize)> = HashMap::new();
    for (s, w) in sessions.iter().zip(&why) {
        let e = per_folder.entry(s.folder.as_str()).or_default();
        if !s.projects {
            e.0 += 1;
            e.1 += usize::from(!w.is_empty());
        }
    }
    for (s, w) in sessions.iter().zip(why.iter_mut()) {
        let (n, a) = per_folder[s.folder.as_str()];
        if w.is_empty()
            && !s.projects
            && s.human_turns <= 1
            && !include.contains(&s.folder)
            && a as f64 / n as f64 > FOLDER_PRIOR
        {
            w.push("F");
        }
    }
    why
}

// ------------------------------------------------------------------ run

/// Folders whose sessions this run set aside: (sessions set aside, reason the folder rule records).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SetAside {
    pub sessions: u64,
    /// Of `sessions`, the claude.ai Projects / Remote Control sessions (rule RC).
    pub projects: u64,
    /// `template` or `burst` when the folder shows an app at work (it gets a `folder_rule`, and
    /// `aiq include` applies); None when only single sessions were automated (e.g. `claude -p` runs).
    pub folder_reason: Option<&'static str>,
}

#[derive(Debug, Default)]
pub struct Report {
    pub counts: ImportRunCounts,
    /// Per tool: (found, imported, automated, unchanged, errors).
    pub by_tool: BTreeMap<&'static str, [u64; 5]>,
    /// Keyed by folder key. Only sessions this run stored as automated.
    pub set_aside: BTreeMap<String, SetAside>,
}

/// Session ids AIQ's own judge started (`~/.aiq/judge-sessions.txt`, US-11); never imported.
pub(crate) fn judge_ids(paths: &Paths) -> HashSet<String> {
    std::fs::read_to_string(paths.data.join("judge-sessions.txt"))
        .map(|s| s.lines().map(|l| l.trim().to_owned()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

/// Read the sources that are new or changed, apply the rule over all sessions (it needs folder-wide counts),
/// and write the new or changed ones. A folder the person included (`aiq include`) is never set aside, and
/// its sessions an earlier run set aside are read again and imported. `dry_run` writes nothing.
pub fn run(
    store: &mut Store,
    sources: &[Source],
    skip_ids: &HashSet<String>,
    p: &AutomatedParams,
    dry_run: bool,
    started_ms: i64,
) -> aiq_store::Result<Report> {
    let mut rep = Report::default();
    let rules = store.folder_rules_now()?;
    let include: HashSet<String> = rules.iter().filter(|(_, a)| *a == "include").map(|(f, _)| f.clone()).collect();
    let known: HashMap<(String, String), KnownImport> =
        store.known_imports()?.into_iter().map(|k| ((k.tool.clone(), k.source_session_id.clone()), k)).collect();
    // (tool, the file read this run or None when unchanged since the last run, rule input)
    let mut rows: Vec<(&'static str, Option<Parsed>, RuleInput)> = Vec::with_capacity(sources.len());
    let mut failed = Vec::new();
    for s in sources.iter().filter(|s| !skip_ids.contains(&s.id)) {
        rep.by_tool.entry(s.tool).or_default()[0] += 1;
        match known.get(&(s.tool.to_owned(), s.id.clone())) {
            Some(k)
                if k.source_mtime_ms == s.mtime_ms
                    && !(k.outcome == "automated" && include.contains(&folder_key(&k.folder))) =>
            {
                rows.push((s.tool, None, RuleInput::from(k)));
            }
            _ => match read(s) {
                Ok(parsed) => {
                    let input = parsed.rule_input(p);
                    rows.push((s.tool, Some(parsed), input));
                }
                Err(_) => failed.push(s.tool),
            },
        }
    }
    let inputs: Vec<RuleInput> = rows.iter().map(|r| r.2.clone()).collect();
    let why = automated(&inputs, &include, p);
    // Folders where a burst or a template fired get the live side's folder rule (template wins, as live).
    let mut folder_reason: HashMap<&str, &'static str> = HashMap::new();
    for (i, w) in inputs.iter().zip(&why) {
        let r = if w.contains(&"T") {
            "template"
        } else if w.contains(&"B") {
            "burst"
        } else {
            continue;
        };
        let e = folder_reason.entry(i.folder.as_str()).or_insert(r);
        if r == "template" {
            *e = r;
        }
    }
    let run_id = uuid::Uuid::now_v7().to_string();
    for ((tool, parsed, input), w) in rows.iter().zip(&why) {
        let t = rep.by_tool.entry(tool).or_default();
        let Some(parsed) = parsed else {
            t[3] += 1;
            continue;
        };
        if input.human_turns == 0 {
            t[3] += 1; // nothing typed: nothing to import (e.g. an empty or aborted session)
            continue;
        }
        let auto = !w.is_empty();
        t[if auto { 2 } else { 1 }] += 1;
        if auto {
            let e = rep.set_aside.entry(input.folder.clone()).or_default();
            e.sessions += 1;
            e.projects += u64::from(w.contains(&"RC"));
            e.folder_reason = folder_reason.get(input.folder.as_str()).copied();
        }
        if dry_run {
            continue;
        }
        store.put_import_session(
            &run_id,
            &ImportedSession {
                tool: (*tool).to_owned(),
                source_session_id: parsed.id.clone(),
                source_mtime_ms: parsed.mtime_ms,
                folder: parsed.folder.clone(),
                started_utc_ms: parsed.started_ms,
                human_turns: input.human_turns,
                non_interactive: input.non_interactive,
                template_key: input.template.map(|v| v as i64),
                outcome: if auto { "automated" } else { "imported" }.into(),
                why: w.join(","),
                // Set-aside sessions keep no content: only the fact that they were seen.
                events: if auto { vec![] } else { parsed.events.clone() },
            },
        )?;
    }
    for tool in failed {
        rep.by_tool.entry(tool).or_default()[4] += 1;
    }
    for t in rep.by_tool.values() {
        rep.counts.found += t[0];
        rep.counts.imported += t[1];
        rep.counts.automated += t[2];
        rep.counts.unchanged += t[3];
        rep.counts.errors += t[4];
    }
    if !dry_run {
        // A folder with no rule yet gets one; a person's `include` or an existing set-aside is left alone.
        for (folder, reason) in &folder_reason {
            if !rules.contains_key(*folder) {
                store.insert_folder_rule(folder, "set_aside", reason, "import", now_ms())?;
            }
        }
        store.insert_import_run(&run_id, started_ms, now_ms(), &rep.counts)?;
    }
    Ok(rep)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn render(rep: &Report, dry_run: bool) -> String {
    let mut out = String::new();
    out += if dry_run { "Dry run: nothing was stored.\n" } else { "Import finished.\n" };
    for (tool, t) in &rep.by_tool {
        let name = if *tool == CLAUDE { "Claude Code" } else { "Copilot CLI" };
        out += &format!(
            "{name}: {} sessions found, {} imported, {} set aside as automated, {} unchanged or empty{}\n",
            t[0],
            t[1],
            t[2],
            t[3],
            if t[4] > 0 { format!(", {} could not be read", t[4]) } else { String::new() }
        );
    }
    if rep.by_tool.is_empty() {
        out += "No Claude Code or Copilot CLI sessions found on this machine.\n";
    }
    let mut aside: Vec<_> = rep.set_aside.iter().collect();
    aside.sort_by(|a, b| b.1.sessions.cmp(&a.1.sessions).then(a.0.cmp(b.0)));
    for (folder, s) in aside.iter().take(10) {
        if s.projects > 0 {
            out += &format!(
                "{folder}: {} sessions Claude ran for you through claude.ai Projects or Remote Control set aside. \
                 To count them: aiq include \"{folder}\", then aiq import\n",
                s.projects
            );
        }
        if s.sessions == s.projects {
            continue;
        }
        let rest = s.sessions - s.projects;
        out += &match s.folder_reason {
            Some(_) => format!(
                "{folder}: {} automated sessions set aside; AIQ ignores this folder's app runs from now on. \
                 If that is your own work: aiq include \"{folder}\", then aiq import\n",
                rest
            ),
            None => {
                format!("{folder}: {} sessions started by a script (one prompt, no one typing) set aside\n", rest)
            }
        };
    }
    out += "Imported sessions show in your history; they never raise tips. Remove them with: aiq forget --imported\n";
    out
}

pub(crate) fn tool_home(var: &str, dir: &str) -> PathBuf {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home_dir().join(dir))
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn open_store(paths: &Paths) -> Result<Store, ExitCode> {
    let _ = paths.ensure_dirs();
    Store::open(&paths.db()).map_err(|e| {
        eprintln!("Could not open AIQ's store ({}): {}", e.code, e.detail);
        ExitCode::from(1)
    })
}

/// `aiq import [--dry-run]`
pub fn cli(args: &[&str]) -> ExitCode {
    let paths = Paths::from_env();
    let mut store = match open_store(&paths) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let dry_run = args.contains(&"--dry-run");
    let params = match aiq_domain::PackSource::load(&aiq_core::pack::BundledPack) {
        Ok(pack) => pack.automated,
        Err(e) => {
            eprintln!("Could not load AIQ's rules: {e}");
            return ExitCode::from(1);
        }
    };
    let sources = discover(&tool_home("CLAUDE_CONFIG_DIR", ".claude"), &tool_home("COPILOT_HOME", ".copilot"));
    match run(&mut store, &sources, &judge_ids(&paths), &params, dry_run, now_ms()) {
        Ok(rep) => {
            print!("{}", render(&rep, dry_run));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Import stopped ({}): {}", e.code, e.detail);
            ExitCode::from(1)
        }
    }
}

/// `aiq forget --imported [--yes]`
pub fn forget(yes: bool) -> ExitCode {
    let paths = Paths::from_env();
    if !paths.db().exists() {
        println!("Nothing imported.");
        return ExitCode::SUCCESS;
    }
    if !crate::cli::confirm("Delete every session AIQ imported from your history? Live data stays.", yes) {
        println!("Nothing deleted.");
        return ExitCode::SUCCESS;
    }
    let mut store = match open_store(&paths) {
        Ok(s) => s,
        Err(c) => return c,
    };
    match store.forget_imported() {
        Ok(n) => {
            println!("Deleted {n} imported sessions.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("Could not delete ({}): {}", e.code, e.detail);
            ExitCode::from(1)
        }
    }
}

/// One `aiq status` line, or None when nothing was ever imported.
pub fn status_line(paths: &Paths) -> Option<String> {
    if !paths.db().exists() {
        return None;
    }
    // Read-only and best-effort: before the first import (schema < v3) the query fails and nothing shows.
    let store = Store::open_read_only(&paths.db()).ok()?;
    let rows = store.import_counts().ok()?;
    if rows.is_empty() {
        return None;
    }
    let n = |o: &str| rows.iter().filter(|r| r.1 == o).map(|r| r.2).sum::<u64>();
    Some(format!("Imported history: {} sessions, {} set aside as automated\n", n("imported"), n("automated")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn write_lines(path: &Path, rows: &[Value]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = std::fs::File::create(path).unwrap();
        for r in rows {
            writeln!(f, "{r}").unwrap();
        }
    }

    fn claude_session(home: &Path, slug: &str, id: &str, entry: &str, prompts: &[&str]) {
        let mut rows = vec![];
        for (i, p) in prompts.iter().enumerate() {
            let ts = format!("2026-10-20T10:{:02}:00Z", i * 2);
            rows.push(json!({"type": "user", "entrypoint": entry, "cwd": "C:\\repos\\a", "timestamp": ts, "message": {"content": p}}));
            rows.push(json!({"type": "assistant", "timestamp": ts, "message": {"content": [
                {"type": "tool_use", "id": format!("t{i}"), "name": "Edit", "input": {"file_path": "src/a.rs"}},
                {"type": "tool_use", "id": format!("b{i}"), "name": "Bash", "input": {"command": "cargo test"}}]}}));
            rows.push(json!({"type": "user", "timestamp": ts, "message": {"content": [{"type": "tool_result", "tool_use_id": format!("t{i}"), "content": "ok"}]}}));
        }
        rows.push(json!({"type": "user", "isSidechain": true, "timestamp": "2026-10-20T11:00:00Z", "message": {"content": "sub-agent prompt"}}));
        rows.push(json!({"type": "user", "isMeta": true, "timestamp": "2026-10-20T11:00:00Z", "message": {"content": "caveat"}}));
        write_lines(&home.join("projects").join(slug).join(format!("{id}.jsonl")), &rows);
    }

    fn copilot_session(home: &Path, id: &str, cwd: &str, start: &str, prompts: &[&str]) {
        let mut rows = vec![
            json!({"type": "session.start", "timestamp": start, "data": {"startTime": start, "context": {"cwd": cwd}}}),
        ];
        for p in prompts {
            rows.push(json!({"type": "user.message", "timestamp": start, "data": {"content": p, "delivery": "idle"}}));
            rows.push(json!({"type": "tool.execution_start", "timestamp": start, "data": {"toolName": "edit", "arguments": "{\"path\": \"x.py\"}"}}));
            rows.push(json!({"type": "user.message", "agentId": "sub1", "timestamp": start, "data": {"content": "to sub-agent", "delivery": "idle"}}));
            rows.push(json!({"type": "subagent.started", "agentId": "sub1", "timestamp": start, "data": {}}));
            rows.push(json!({"type": "assistant.turn_end", "timestamp": start, "data": {}}));
        }
        write_lines(&home.join("session-state").join(id).join("events.jsonl"), &rows);
    }

    #[test]
    fn claude_reader_keeps_typed_prompts_and_summarises_actions() {
        let d = tempfile::tempdir().unwrap();
        claude_session(d.path(), "C--repos-a", "s1", "cli", &["fix the bug", "now add a test"]);
        let src = discover(d.path(), &d.path().join("none"));
        let p = read(&src[0]).unwrap();
        assert_eq!(p.tool, CLAUDE);
        assert_eq!(p.folder, "C:\\repos\\a");
        assert_eq!(p.non_interactive, Some(false));
        let kinds: Vec<&str> = p.events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["prompt", "stop", "prompt", "stop"]);
        assert_eq!(p.human_turns(), 2);
        let a: Value = serde_json::from_str(p.events[1].actions.as_deref().unwrap()).unwrap();
        assert_eq!(a["tools"]["Edit"], 1);
        assert_eq!(a["files"][0], "src/a.rs");
        assert_eq!(a["commands"][0], "cargo test");
    }

    #[test]
    fn copilot_reader_ignores_sub_agent_lines() {
        let d = tempfile::tempdir().unwrap();
        copilot_session(d.path(), "c1", "C:\\repos\\b", "2026-09-01T10:00:00Z", &["add a flag"]);
        let src = discover(&d.path().join("none"), d.path());
        let p = read(&src[0]).unwrap();
        assert_eq!((p.tool, p.folder.as_str(), p.human_turns()), (COPILOT, "C:\\repos\\b", 1));
        let a: Value = serde_json::from_str(p.events[1].actions.as_deref().unwrap()).unwrap();
        assert_eq!(
            (a["tools"]["edit"].as_u64(), a["files"][0].as_str(), a["subagents"].as_u64()),
            (Some(1), Some("x.py"), Some(1))
        );
    }

    fn sess(folder: &str, prompts: &[&str], start: i64, ni: Option<bool>) -> Parsed {
        Parsed {
            tool: COPILOT,
            id: format!("{folder}-{start}"),
            folder: folder.into(),
            started_ms: Some(start),
            non_interactive: ni,
            events: prompts
                .iter()
                .map(|p| ImportedEvent {
                    kind: "prompt".into(),
                    ts_utc_ms: Some(start),
                    text: Some((*p).into()),
                    actions: None,
                })
                .collect(),
            ..Default::default()
        }
    }

    fn rule(v: &[Parsed], include: &[&str]) -> Vec<Vec<&'static str>> {
        let p = AutomatedParams::default();
        let inputs: Vec<RuleInput> = v.iter().map(|s| s.rule_input(&p)).collect();
        automated(&inputs, &include.iter().map(|f| folder_key(f)).collect(), &p)
    }

    #[test]
    fn rule_sets_aside_templates_bursts_and_sdk_runs_but_never_multi_turn_work() {
        let hour = 3_600_000;
        let mut v = vec![];
        // template: 11 one-prompt sessions, same prompt shape, spread over days (no burst)
        for i in 0..11 {
            v.push(sess("pipe", &[&format!("Extract record {i} from https://x/{i}")], i * 24 * hour, None));
        }
        // burst: 50 different one-prompt sessions within an hour
        for i in 0..50 {
            v.push(sess(
                "burst",
                &[&format!("question number {} about topic {}", i, "abcdefghij".repeat(i as usize % 3 + 1))],
                i * 30_000,
                None,
            ));
        }
        v.push(sess("work", &["Extract record 1 from https://x/1"], 0, Some(true))); // NI
        v.push(sess("work", &["fix the build", "now run tests"], 0, Some(true))); // two prompts: kept
        v.push(sess("work", &["a one-off question"], 0, None)); // kept
        let why = rule(&v, &[]);
        assert!(why[..11].iter().all(|w| w.contains(&"T")));
        assert!(why[11..61].iter().all(|w| w.contains(&"B")));
        assert_eq!(why[61], vec!["NI"]);
        assert!(why[62].is_empty() && why[63].is_empty());
        assert!(rule(&v, &["pipe"])[..11].iter().all(Vec::is_empty));
    }

    #[test]
    fn folder_prior_takes_the_rest_and_empty_prompts_never_form_a_template() {
        let day = 24 * 3_600_000;
        let mut v: Vec<Parsed> = (0..11).map(|i| sess("p", &["run the nightly extract"], i * day, None)).collect();
        v.push(sess("p", &["something unique"], 40 * day, None));
        assert_eq!(rule(&v, &[])[11], vec!["F"]);
        let p = AutomatedParams::default();
        assert_eq!(template_hash("   ", &p), None);
        assert_eq!(template_hash("Item 12 at C:\\x\\y", &p), template_hash("item 7 at /tmp/z", &p));
        // Stored hashes must stay comparable across builds: pin one value.
        assert_eq!(template_hash("hello", &p), Some(0xa430_d846_80aa_bd0b));
    }

    #[test]
    fn windows_folders_differing_only_in_case_are_one_folder() {
        let day = 24 * 3_600_000;
        let v: Vec<Parsed> = (0..11)
            .map(|i| {
                let f = if i % 2 == 0 { "C:\\Repos\\Foresight" } else { "c:\\repos\\foresight\\" };
                sess(f, &["run the nightly extract"], i * day, None)
            })
            .collect();
        assert!(rule(&v, &[]).iter().all(|w| w == &vec!["T"]));
    }

    #[test]
    fn projects_sessions_are_set_aside_whatever_their_prompt_count_unless_included() {
        let d = tempfile::tempdir().unwrap();
        let (claude, copilot) = (d.path().join("claude"), d.path().join("copilot"));
        let rows = vec![
            json!({"type": "user", "isMeta": true, "turnOrigin": "system", "entrypoint": "claude-desktop", "cwd": "C:\\repos\\a", "timestamp": "2026-10-20T10:00:00Z", "message": {"content": "brief"}}),
            json!({"type": "user", "turnOrigin": "human", "projectsUserTurn": true, "entrypoint": "claude-desktop", "timestamp": "2026-10-20T10:01:00Z", "message": {"content": "do the thing"}}),
            json!({"type": "user", "turnOrigin": "peer", "entrypoint": "claude-desktop", "timestamp": "2026-10-20T10:02:00Z", "message": {"content": "relay"}}),
        ];
        write_lines(&claude.join("projects").join("C--repos-a").join("rc.jsonl"), &rows);
        claude_session(&claude, "C--repos-a", "mine", "claude-desktop", &["fix the bug", "add a test"]);
        let mut store = Store::open(&d.path().join("aiq.db")).unwrap();
        let src = discover(&claude, &copilot);
        let p = AutomatedParams::default();
        let rep = run(&mut store, &src, &HashSet::new(), &p, false, 1).unwrap();
        assert_eq!(rep.by_tool[CLAUDE], [2, 1, 1, 0, 0]);
        assert_eq!(
            rep.set_aside[&folder_key("C:\\repos\\a")],
            SetAside { sessions: 1, projects: 1, folder_reason: None }
        );
        assert!(render(&rep, false).contains("claude.ai Projects or Remote Control"));
        // A re-run keeps it aside without re-reading (RC comes back from the stored `why`).
        let k = store.known_imports().unwrap().into_iter().find(|k| k.source_session_id == "rc").unwrap();
        assert!(RuleInput::from(&k).projects);
        let include: HashSet<String> = [folder_key("C:\\repos\\a")].into();
        assert!(automated(&[RuleInput::from(&k)], &include, &p)[0].is_empty(), "Include them");
    }

    #[test]
    fn stored_rule_input_matches_the_file_read() {
        let d = tempfile::tempdir().unwrap();
        let (claude, copilot) = (d.path().join("claude"), d.path().join("copilot"));
        claude_session(&claude, "C--repos-a", "sdk", "sdk-cli", &["Headless call 42"]);
        let mut store = Store::open(&d.path().join("aiq.db")).unwrap();
        let src = discover(&claude, &copilot);
        let p = AutomatedParams::default();
        run(&mut store, &src, &HashSet::new(), &p, false, 1).unwrap();
        let k = &store.known_imports().unwrap()[0];
        assert_eq!(RuleInput::from(k), read(&src[0]).unwrap().rule_input(&p));
    }

    #[test]
    fn run_imports_once_keeps_no_content_for_automated_and_never_touches_live_events() {
        let d = tempfile::tempdir().unwrap();
        let (claude, copilot) = (d.path().join("claude"), d.path().join("copilot"));
        claude_session(&claude, "C--repos-a", "mine", "cli", &["fix the bug", "add a test"]);
        claude_session(&claude, "C--repos-a", "sdk", "sdk-cli", &["headless call"]);
        copilot_session(&copilot, "judge", "C:\\x", "2026-09-01T10:00:00Z", &["judge this"]);
        let mut store = Store::open(&d.path().join("aiq.db")).unwrap();
        let src = discover(&claude, &copilot);
        let skip: HashSet<String> = ["judge".to_string()].into();
        let p = AutomatedParams::default();
        let rep = run(&mut store, &src, &skip, &p, false, 1).unwrap();
        assert_eq!(rep.by_tool[CLAUDE], [2, 1, 1, 0, 0]);
        assert!(!rep.by_tool.contains_key(COPILOT));
        assert_eq!(
            rep.set_aside[&folder_key("C:\\repos\\a")],
            SetAside { sessions: 1, projects: 0, folder_reason: None }
        );
        assert!(store.set_aside_folders().unwrap().is_empty(), "a lone claude -p run sets no folder rule");
        let n: i64 = store
            .raw()
            .query_row("SELECT count(*) FROM import_event WHERE source_session_id = 'sdk'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "set-aside sessions keep no content");
        let live: i64 = store.raw().query_row("SELECT count(*) FROM event", [], |r| r.get(0)).unwrap();
        assert_eq!(live, 0);
        assert!(store.prompts_for_rebuild(0).unwrap().is_empty(), "imports never reach the detector");
        let again = run(&mut store, &src, &skip, &p, false, 2).unwrap();
        assert_eq!(again.by_tool[CLAUDE], [2, 0, 0, 2, 0]);
        assert!(again.set_aside.is_empty(), "a re-run lists only what it set aside");
        let dry = run(&mut store, &src, &skip, &p, true, 3).unwrap();
        assert_eq!(dry.counts.imported, 0);
        assert_eq!(store.import_counts().unwrap().len(), 2);
        assert_eq!(store.forget_imported().unwrap(), 2);
        assert!(store.import_counts().unwrap().is_empty());
        assert!(render(&rep, false).contains("1 imported, 1 set aside"));
    }

    #[test]
    fn template_folder_gets_the_live_folder_rule_and_include_brings_its_sessions_in() {
        let d = tempfile::tempdir().unwrap();
        let (claude, copilot) = (d.path().join("claude"), d.path().join("copilot"));
        for i in 0..11 {
            let start = format!("2026-09-{:02}T10:00:00Z", i + 1);
            copilot_session(
                &copilot,
                &format!("ep{i}"),
                "C:\\Users\\X\\EP",
                &start,
                &[&format!("collect profile {i}")],
            );
        }
        let mut store = Store::open(&d.path().join("aiq.db")).unwrap();
        let src = discover(&claude, &copilot);
        let p = AutomatedParams::default();
        let dry = run(&mut store, &src, &HashSet::new(), &p, true, 1).unwrap();
        assert!(store.set_aside_folders().unwrap().is_empty(), "a dry run writes no folder rule");
        assert!(render(&dry, true).contains("aiq include \"c:\\users\\x\\ep\""));
        let rep = run(&mut store, &src, &HashSet::new(), &p, false, 2).unwrap();
        assert_eq!(rep.by_tool[COPILOT], [11, 0, 11, 0, 0]);
        let rules = store.set_aside_folders().unwrap();
        assert_eq!((rules[0].0.as_str(), rules[0].1.as_str()), ("c:\\users\\x\\ep", "template"));
        run(&mut store, &src, &HashSet::new(), &p, false, 3).unwrap();
        assert_eq!(store.set_aside_folders().unwrap().len(), 1, "no duplicate rule on a re-run");
        store.insert_folder_rule("c:\\users\\x\\ep", "include", "person", "person", 4).unwrap();
        let inc = run(&mut store, &src, &HashSet::new(), &p, false, 5).unwrap();
        assert_eq!(inc.by_tool[COPILOT], [11, 11, 0, 0, 0]);
        assert!(store.set_aside_folders().unwrap().is_empty(), "the person's include stands");
    }
}
