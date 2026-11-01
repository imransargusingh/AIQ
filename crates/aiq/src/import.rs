//! `aiq import`: seed AIQ with AI sessions that ran before it was installed (docs/specs/history-import.md).
//!
//! Reads the transcripts the tools already keep on this machine, read-only:
//!   Claude Code   `~/.claude/projects/<folder>/<session>.jsonl` (or `$CLAUDE_CONFIG_DIR`)
//!   Copilot CLI   `~/.copilot/session-state/<session>/events.jsonl` (or `$COPILOT_HOME`)
//! Automated sessions are set aside by the spec's rule (H4, no question asked). Everything else is
//! stored in the import tables, which the live coaching loop never reads, so imports raise no cards (US-5).
//! Judging imported tasks waits for the rung judge; nothing here calls a model or the network.
use aiq_core::paths::Paths;
use aiq_store::{ImportRunCounts, ImportedEvent, ImportedSession, Store};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const CLAUDE: &str = "claude-code";
pub const COPILOT: &str = "copilot-cli";
/// Template rule: a first prompt shared by this many sessions in one folder (spec: 11+).
const TEMPLATE_MIN: usize = 11;
/// Burst rule: this many sessions started in one folder within ±30 minutes.
const BURST_MIN: usize = 50;
const BURST_WINDOW_MS: i64 = 30 * 60 * 1000;
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
    pub events: Vec<ImportedEvent>,
}

impl Parsed {
    pub fn human_turns(&self) -> u32 {
        self.events.iter().filter(|e| e.kind == "prompt").count() as u32
    }
    fn first_prompt(&self) -> Option<&str> {
        self.events.iter().find(|e| e.kind == "prompt").and_then(|e| e.text.as_deref())
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

/// Normalise a first prompt so runs of one template match: lower case, URLs, paths, GUIDs and numbers
/// replaced, first 200 chars. None for an empty prompt (empty prompts never form a template group).
pub fn template_key(prompt: &str) -> Option<u64> {
    let toks: Vec<String> = prompt
        .split_whitespace()
        .map(|t| {
            let t = t.to_lowercase();
            if t.starts_with("http://") || t.starts_with("https://") {
                "<url>".into()
            } else if t.contains('/') || t.contains('\\') {
                "<path>".into()
            } else if t.len() >= 32 && t.chars().filter(|c| *c == '-').count() == 4 {
                "<guid>".into()
            } else {
                let mut o = String::with_capacity(t.len());
                for c in t.chars() {
                    if c.is_ascii_digit() {
                        if !o.ends_with('0') {
                            o.push('0');
                        }
                    } else {
                        o.push(c);
                    }
                }
                o
            }
        })
        .collect();
    let s: String = toks.join(" ").chars().take(200).collect();
    if s.is_empty() {
        return None;
    }
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    Some(h.finish())
}

/// The spec's automated rule. Returns, per session index, the signals that set it aside (empty = kept).
/// A session is automated when it has at most one human prompt AND a second signal:
/// NI non-interactive start, T shared template (11+ in the folder), B burst (50+ in ±30 min in the folder),
/// F folder prior (folder over 80% automated). `include` folders are never set aside ("Include them").
pub fn automated(sessions: &[Parsed], include: &HashSet<String>) -> Vec<Vec<&'static str>> {
    let keys: Vec<Option<u64>> = sessions.iter().map(|s| s.first_prompt().and_then(template_key)).collect();
    let mut groups: HashMap<(&str, u64), usize> = HashMap::new();
    let mut starts: HashMap<&str, Vec<i64>> = HashMap::new();
    for (s, k) in sessions.iter().zip(&keys) {
        if let Some(k) = k {
            *groups.entry((s.folder.as_str(), *k)).or_default() += 1;
        }
        if let Some(t) = s.started_ms {
            starts.entry(s.folder.as_str()).or_default().push(t);
        }
    }
    for v in starts.values_mut() {
        v.sort_unstable();
    }
    let mut why: Vec<Vec<&'static str>> = sessions
        .iter()
        .zip(&keys)
        .map(|(s, k)| {
            if s.human_turns() > 1 || include.contains(&s.folder.to_lowercase()) {
                return vec![];
            }
            let mut w = vec![];
            if s.non_interactive == Some(true) {
                w.push("NI");
            }
            if k.is_some_and(|k| groups[&(s.folder.as_str(), k)] >= TEMPLATE_MIN) {
                w.push("T");
            }
            if let (Some(t), Some(v)) = (s.started_ms, starts.get(s.folder.as_str())) {
                let lo = v.partition_point(|x| *x < t - BURST_WINDOW_MS);
                let hi = v.partition_point(|x| *x <= t + BURST_WINDOW_MS);
                if hi - lo >= BURST_MIN {
                    w.push("B");
                }
            }
            w
        })
        .collect();
    let mut per_folder: HashMap<&str, (usize, usize)> = HashMap::new();
    for (s, w) in sessions.iter().zip(&why) {
        let e = per_folder.entry(s.folder.as_str()).or_default();
        e.0 += 1;
        e.1 += usize::from(!w.is_empty());
    }
    for (i, s) in sessions.iter().enumerate() {
        let (n, a) = per_folder[s.folder.as_str()];
        if why[i].is_empty()
            && s.human_turns() <= 1
            && !include.contains(&s.folder.to_lowercase())
            && a as f64 / n as f64 > FOLDER_PRIOR
        {
            why[i].push("F");
        }
    }
    why
}

// ------------------------------------------------------------------ run

#[derive(Debug, Default)]
pub struct Report {
    pub counts: ImportRunCounts,
    /// Per tool: (found, imported, automated, unchanged, errors).
    pub by_tool: BTreeMap<&'static str, [u64; 5]>,
    pub set_aside: BTreeMap<String, u64>,
}

/// Session ids AIQ's own judge started (`~/.aiq/judge-sessions.txt`, US-11); never imported.
fn judge_ids(paths: &Paths) -> HashSet<String> {
    std::fs::read_to_string(paths.data.join("judge-sessions.txt"))
        .map(|s| s.lines().map(|l| l.trim().to_owned()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

pub fn include_folders(store: &Store) -> HashSet<String> {
    store
        .setting("import.include_folders")
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|f| f.to_lowercase())
        .collect()
}

/// Read every source, apply the rule over all of them (the rule needs folder-wide counts), and write the
/// sessions that are new or changed since the last run. `dry_run` writes nothing.
pub fn run(
    store: &mut Store,
    sources: &[Source],
    skip_ids: &HashSet<String>,
    include: &HashSet<String>,
    dry_run: bool,
    started_ms: i64,
) -> aiq_store::Result<Report> {
    let mut rep = Report::default();
    let mut parsed = Vec::with_capacity(sources.len());
    let mut failed = Vec::new();
    for s in sources.iter().filter(|s| !skip_ids.contains(&s.id)) {
        let t = rep.by_tool.entry(s.tool).or_default();
        t[0] += 1;
        match read(s) {
            Ok(p) => parsed.push(p),
            Err(_) => failed.push(s),
        }
    }
    let why = automated(&parsed, include);
    let run_id = uuid::Uuid::now_v7().to_string();
    for (p, w) in parsed.iter().zip(why) {
        let t = rep.by_tool.entry(p.tool).or_default();
        let auto = !w.is_empty();
        if auto {
            *rep.set_aside.entry(p.folder.clone()).or_default() += 1;
        }
        if store.import_mtime(p.tool, &p.id)? == Some(p.mtime_ms) && !include.contains(&p.folder.to_lowercase()) {
            t[3] += 1;
            continue;
        }
        if p.human_turns() == 0 {
            t[3] += 1; // nothing typed: nothing to import (e.g. an empty or aborted session)
            continue;
        }
        t[if auto { 2 } else { 1 }] += 1;
        if dry_run {
            continue;
        }
        store.put_import_session(
            &run_id,
            &ImportedSession {
                tool: p.tool.to_owned(),
                source_session_id: p.id.clone(),
                source_mtime_ms: p.mtime_ms,
                folder: p.folder.clone(),
                started_utc_ms: p.started_ms,
                human_turns: p.human_turns(),
                outcome: if auto { "automated" } else { "imported" }.into(),
                why: w.join(","),
                // Set-aside sessions keep no content: only the fact that they were seen.
                events: if auto { vec![] } else { p.events.clone() },
            },
        )?;
    }
    for s in failed {
        rep.by_tool.entry(s.tool).or_default()[4] += 1;
    }
    for t in rep.by_tool.values() {
        rep.counts.found += t[0];
        rep.counts.imported += t[1];
        rep.counts.automated += t[2];
        rep.counts.unchanged += t[3];
        rep.counts.errors += t[4];
    }
    if !dry_run {
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
    aside.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (folder, n) in aside.iter().take(10) {
        out +=
            &format!("{folder}: {n} automated sessions set aside. Include them: aiq import --include \"{folder}\"\n");
    }
    out += "Imported sessions show in your history; they never raise tips. Remove them with: aiq forget --imported\n";
    out
}

fn tool_home(var: &str, dir: &str) -> PathBuf {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home_dir().join(dir))
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn open_store(paths: &Paths) -> Result<Store, ExitCode> {
    let _ = paths.ensure_dirs();
    Store::open(&paths.db()).map_err(|e| {
        eprintln!("Could not open AIQ's store ({}): {}", e.code, e.detail);
        ExitCode::from(1)
    })
}

/// `aiq import [--dry-run] [--include <folder>]`
pub fn cli(args: &[&str]) -> ExitCode {
    let paths = Paths::from_env();
    let mut store = match open_store(&paths) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let dry_run = args.contains(&"--dry-run");
    if let Some(i) = args.iter().position(|a| *a == "--include") {
        let Some(folder) = args.get(i + 1) else {
            eprintln!("Usage: aiq import --include <folder>");
            return ExitCode::from(2);
        };
        let mut v: Vec<String> = include_folders(&store).into_iter().collect();
        if !v.contains(&folder.to_lowercase()) {
            v.push(folder.to_lowercase());
        }
        v.sort();
        if let Err(e) =
            store.set_setting("import.include_folders", &serde_json::to_string(&v).unwrap_or_default(), now_ms())
        {
            eprintln!("Could not save the setting ({}).", e.code);
            return ExitCode::from(1);
        }
        println!("{folder}: its sessions will be imported as your own work.");
    }
    let sources = discover(&tool_home("CLAUDE_CONFIG_DIR", ".claude"), &tool_home("COPILOT_HOME", ".copilot"));
    let include = include_folders(&store);
    match run(&mut store, &sources, &judge_ids(&paths), &include, dry_run, now_ms()) {
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
        let why = automated(&v, &HashSet::new());
        assert!(why[..11].iter().all(|w| w.contains(&"T")));
        assert!(why[11..61].iter().all(|w| w.contains(&"B")));
        assert_eq!(why[61], vec!["NI"]);
        assert!(why[62].is_empty() && why[63].is_empty());
        let inc: HashSet<String> = ["pipe".to_string()].into();
        assert!(automated(&v, &inc)[..11].iter().all(Vec::is_empty));
    }

    #[test]
    fn folder_prior_takes_the_rest_and_empty_prompts_never_form_a_template() {
        let day = 24 * 3_600_000;
        let mut v: Vec<Parsed> = (0..11).map(|i| sess("p", &["run the nightly extract"], i * day, None)).collect();
        v.push(sess("p", &["something unique"], 40 * day, None));
        let why = automated(&v, &HashSet::new());
        assert_eq!(why[11], vec!["F"]);
        assert_eq!(template_key("   "), None);
        assert_eq!(template_key("Item 12 at C:\\x\\y"), template_key("item 7 at /tmp/z"));
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
        let rep = run(&mut store, &src, &skip, &HashSet::new(), false, 1).unwrap();
        assert_eq!(rep.by_tool[CLAUDE], [2, 1, 1, 0, 0]);
        assert!(!rep.by_tool.contains_key(COPILOT));
        let n: i64 = store
            .raw()
            .query_row("SELECT count(*) FROM import_event WHERE source_session_id = 'sdk'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "set-aside sessions keep no content");
        let live: i64 = store.raw().query_row("SELECT count(*) FROM event", [], |r| r.get(0)).unwrap();
        assert_eq!(live, 0);
        assert!(store.prompts_for_rebuild(0).unwrap().is_empty(), "imports never reach the detector");
        let again = run(&mut store, &src, &skip, &HashSet::new(), false, 2).unwrap();
        assert_eq!(again.by_tool[CLAUDE], [2, 0, 0, 2, 0]);
        let dry = run(&mut store, &src, &skip, &HashSet::new(), true, 3).unwrap();
        assert_eq!(dry.counts.imported, 0);
        assert_eq!(store.import_counts().unwrap().len(), 2);
        assert_eq!(store.forget_imported().unwrap(), 2);
        assert!(store.import_counts().unwrap().is_empty());
        assert!(render(&rep, false).contains("1 imported, 1 set aside"));
    }
}
