//! What the agent did in a turn, read from the tool's own log: tool calls by name, files touched, commands run,
//! sub-agents started. One summariser for both readers, so a live turn and an imported turn are counted the
//! same way: `aiq import` reads whole files, the core reads the new part of a live session's log at each stop.
//!
//! Sources, read-only: Claude Code's transcript (`assistant` records' `tool_use` blocks; sub-agent records carry
//! `isSidechain`) and Copilot's session log (`tool.execution_start`, `subagent.started`; sub-agent lines carry
//! `agentId`). Copilot CLI and VS Code agent mode (Copilot target) write the same log.
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Most files and commands kept per turn for import's judge summary (the counts stop there too).
pub const MAX_FILES: usize = 50;
pub const MAX_COMMANDS: usize = 30;
/// Longest command kept by import, in bytes.
pub const MAX_CMD: usize = 200;
/// Most bytes one stop reads; a longer backlog makes that turn "not recorded" and the next read goes on.
pub const MAX_READ_BYTES: u64 = 16 * 1024 * 1024;
/// Bytes a session's first read since the core started takes from the end of its log, before widening
/// (`read_back_to_prompt`). Laptop 2026-11-04 01:28Z (0.1.2): a fixed 16 MB first read took 296 ms in the core's
/// single writer, where other sessions' stops wait; the other reads were tens of KB. A turn whose prompt lies
/// more than `MAX_READ_BYTES` back is "not recorded" (`no_start`).
pub const TAIL_READ_BYTES: u64 = 2 * 1024 * 1024;

/// Text as AIQ stores it: secrets removed (security review S2), then cut to `max` bytes on a char boundary.
pub fn clean_text(raw: &str, max: usize) -> String {
    let s = aiq_domain::secrets::redact(raw).0;
    if s.len() <= max {
        return s;
    }
    let mut i = max;
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    s[..i].to_owned()
}

/// Per-turn summary of what the agent did.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Actions {
    pub tools: BTreeMap<String, u32>,
    pub files: Vec<String>,
    pub commands: Vec<String>,
    /// Every command run, also past `MAX_COMMANDS`.
    pub commands_run: u32,
    pub subagents: u32,
}

/// Files a freeform patch names (`*** Update File: p`, `*** Add File: p`, `*** Delete File: p`, `*** Move to: p`).
/// Copilot's GPT models edit with `apply_patch`, whose argument is the patch text, not JSON with a `path`: before
/// this, those edits counted 0 files (phase 0 fact check 2026-11-02: 5 of 15 tasks; intent thread 2026-11-03).
/// assume: Copilot logs the patch as the raw `arguments` string or under `input`/`patch`. Confirm: a live turn
/// that ran apply_patch shows files > 0 in `aiq status`. If false: those turns count 0 files, as before.
fn patch_files(patch: &str) -> impl Iterator<Item = &str> {
    patch.lines().filter_map(|l| {
        ["*** Update File: ", "*** Add File: ", "*** Delete File: ", "*** Move to: "]
            .iter()
            .find_map(|p| l.strip_prefix(p))
            .map(str::trim)
            .filter(|p| !p.is_empty())
    })
}

impl Actions {
    /// One tool call, with its arguments (a JSON object, or the raw text when the tool takes freeform text).
    pub fn note(&mut self, name: &str, args: &Value) {
        *self.tools.entry(name.to_owned()).or_default() += 1;
        let path = ["file_path", "path", "notebook_path"].iter().find_map(|k| args.get(*k).and_then(Value::as_str));
        let patch = match args {
            Value::String(s) => Some(s.as_str()),
            v => ["input", "patch"].iter().find_map(|k| v.get(*k).and_then(Value::as_str)),
        }
        .filter(|s| s.contains("*** "));
        for p in path.into_iter().chain(patch.into_iter().flat_map(patch_files)) {
            if self.files.len() < MAX_FILES && !self.files.iter().any(|f| f == p) {
                self.files.push(p.to_owned());
            }
        }
        if let Some(c) = args.get("command").and_then(Value::as_str) {
            self.commands_run += 1;
            if self.commands.len() < MAX_COMMANDS {
                self.commands.push(clean_text(c, MAX_CMD));
            }
        }
        if matches!(name, "Task" | "Agent") {
            self.subagents += 1;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty() && self.subagents == 0
    }

    pub fn tool_calls(&self) -> u32 {
        self.tools.values().sum()
    }

    /// Import's stored shape (the judge reads it), or None when the agent did nothing.
    pub fn take_json(&mut self) -> Option<String> {
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

/// A Claude Code transcript record's tool calls (main line only), noted into `acts`.
pub fn claude_record(r: &Value, acts: &mut Actions) {
    if r.get("isSidechain").and_then(Value::as_bool) == Some(true) || r["type"] != "assistant" {
        return;
    }
    for b in r["message"]["content"].as_array().into_iter().flatten() {
        if b["type"] == "tool_use" {
            acts.note(b["name"].as_str().unwrap_or("?"), &b["input"]);
        }
    }
}

/// A Claude Code transcript record that is a typed prompt: a main-line, non-meta `user` record whose content is
/// a string (tool results are arrays).
pub fn claude_typed(r: &Value) -> bool {
    r["type"] == "user"
        && r.get("isSidechain").and_then(Value::as_bool) != Some(true)
        && r.get("isMeta").and_then(Value::as_bool) != Some(true)
        && r["message"]["content"].as_str().is_some_and(|t| !t.trim().is_empty())
}

/// A Copilot log record's tool call or sub-agent start, noted into `acts`. Main-line tool calls only: a
/// sub-agent's own calls are its work, counted once as the sub-agent.
pub fn copilot_record(e: &Value, acts: &mut Actions) {
    let main = e.get("agentId").is_none_or(Value::is_null);
    match e["type"].as_str() {
        Some("tool.execution_start") if main => {
            let d = &e["data"];
            let args = match &d["arguments"] {
                Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone())),
                v => v.clone(),
            };
            acts.note(d["toolName"].as_str().unwrap_or("?"), &args);
        }
        Some("subagent.started") => acts.subagents += 1,
        _ => {}
    }
}

/// Which log a live turn is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Claude,
    Copilot,
}

/// One read of the new part of a live session's log at a stop.
#[derive(Debug, Default)]
pub struct LogRead {
    /// The agent's actions since the last typed prompt in the part read (or since `from`, when no prompt is in it).
    pub actions: Actions,
    /// Copilot messages typed into the running turn (`copilot_log`), for the mid-turn reader.
    pub steering: Vec<crate::copilot_log::Typed>,
    /// Where the next read starts.
    pub next: u64,
    pub bytes: u64,
    /// Whole lines that looked relevant but did not parse as JSON.
    pub bad_lines: u32,
    /// More than `MAX_READ_BYTES` was waiting: the turn's actions are incomplete.
    pub capped: bool,
    /// A typed prompt was in the part read, so the actions start at the turn's start.
    pub saw_prompt: bool,
}

/// A session's first read since the core started: the last `TAIL_READ_BYTES`, then twice as far back each time
/// the part read holds no typed prompt, up to `MAX_READ_BYTES` or the file's start. Laptop 2026-11-04 (0.1.3):
/// a 22-minute VS Code turn wrote 2.7 MB after its prompt, so a 2 MB tail alone stored it as `no_start`. The
/// prompt's own arrival can't mark the turn's start: a spooled prompt reaches the core late, after the agent's
/// first calls.
pub fn read_back_to_prompt(path: &Path, kind: LogKind) -> std::io::Result<LogRead> {
    let len = std::fs::metadata(path)?.len();
    let mut back = TAIL_READ_BYTES;
    loop {
        let r = read_since(path, len.saturating_sub(back), kind)?;
        if r.saw_prompt || back >= len || back >= MAX_READ_BYTES {
            return Ok(r);
        }
        back = (back * 2).min(MAX_READ_BYTES);
    }
}

/// Read the log from byte `from` on: whole lines only (a line still being written waits for the next read), at
/// most `MAX_READ_BYTES`. A shorter file than `from` was replaced, so it is read from the start. The actions
/// restart at each typed prompt (a Copilot `steering` message stays in its turn), so a first read from 0 after a
/// core restart counts only the last turn, not the whole session.
pub fn read_since(path: &Path, from: u64, kind: LogKind) -> std::io::Result<LogRead> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let from = if from > len { 0 } else { from };
    let aligned = from == 0 || starts_line(&mut f, from)?;
    f.seek(SeekFrom::Start(from))?;
    let mut buf = Vec::new();
    f.take(MAX_READ_BYTES.min(len - from)).read_to_end(&mut buf)?;
    let whole = buf.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    // Mid-line start (a tail read): skip to the next line.
    let first = if !aligned { buf[..whole].iter().position(|b| *b == b'\n').map_or(whole, |i| i + 1) } else { 0 };
    let mut out = LogRead {
        next: from + whole as u64,
        bytes: whole as u64,
        capped: len - from > MAX_READ_BYTES,
        ..Default::default()
    };
    // Cheap filter first: most bytes are tool output, and only these lines can matter.
    let wanted: &[&[u8]] = match kind {
        LogKind::Claude => &[b"\"tool_use\"", b"\"user\""],
        LogKind::Copilot => &[b"\"user.message\"", b"\"tool.execution_start\"", b"\"subagent.started\""],
    };
    let has = |l: &[u8], w: &[u8]| l.windows(w.len()).any(|x| x == w);
    for l in buf[first..whole].split(|b| *b == b'\n').filter(|l| wanted.iter().any(|w| has(l, w))) {
        let Ok(v) = serde_json::from_slice::<Value>(l) else {
            out.bad_lines += 1;
            continue;
        };
        match kind {
            LogKind::Claude => {
                if claude_typed(&v) {
                    out.saw_prompt = true;
                    out.actions = Actions::default();
                }
                claude_record(&v, &mut out.actions);
            }
            LogKind::Copilot => {
                if let Some(t) = crate::copilot_log::typed_message(&v) {
                    if t.delivery == "steering" {
                        if !t.text.trim().is_empty() {
                            out.steering.push(t);
                        }
                    } else {
                        out.saw_prompt = true;
                        out.actions = Actions::default();
                    }
                }
                copilot_record(&v, &mut out.actions);
            }
        }
    }
    Ok(out)
}

/// The byte before `at` is a newline.
fn starts_line(f: &mut std::fs::File, at: u64) -> std::io::Result<bool> {
    let mut b = [0u8; 1];
    f.seek(SeekFrom::Start(at - 1))?;
    f.read_exact(&mut b)?;
    Ok(b[0] == b'\n')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write(lines: &[Value], tail: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let d = tempfile::TempDir::new().unwrap();
        let p = d.path().join("log.jsonl");
        let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&p, format!("{body}{tail}")).unwrap();
        (d, p)
    }

    #[test]
    fn apply_patch_counts_the_files_it_names() {
        let patch = "*** Begin Patch\n*** Update File: src/a.rs\n@@\n-x\n+y\n*** Add File: b.md\n+hi\n*** End Patch";
        for args in [json!(patch), json!({"input": patch})] {
            let mut a = Actions::default();
            a.note("apply_patch", &args);
            assert_eq!(a.files, ["src/a.rs", "b.md"], "{args}");
        }
        // A copilot record whose arguments are the raw patch (not JSON) keeps it.
        let mut a = Actions::default();
        copilot_record(
            &json!({"type":"tool.execution_start","data":{"toolName":"apply_patch","arguments":patch}}),
            &mut a,
        );
        assert_eq!((a.tool_calls(), a.files.len()), (1, 2));
    }

    #[test]
    fn note_keeps_paths_commands_and_subagents() {
        let mut a = Actions::default();
        a.note("Edit", &json!({"file_path": "x.rs"}));
        a.note("Edit", &json!({"file_path": "x.rs"}));
        a.note("Bash", &json!({"command": "cargo test"}));
        a.note("Task", &json!({}));
        assert_eq!((a.tool_calls(), a.files.len(), a.commands.len(), a.subagents), (4, 1, 1, 1));
        let j: Value = serde_json::from_str(&a.take_json().unwrap()).unwrap();
        assert_eq!(j["tools"]["Edit"], 2);
        assert!(a.is_empty());
    }

    #[test]
    fn claude_read_counts_since_the_last_prompt_and_skips_sidechains() {
        let tu = |n: &str, i: Value| json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":n,"input":i}]}});
        let (_d, p) = write(
            &[
                json!({"type":"user","message":{"content":"first"}}),
                tu("Bash", json!({"command":"ls"})),
                json!({"type":"user","message":{"content":"second"}}),
                tu("Edit", json!({"file_path":"a.rs"})),
                json!({"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]}}),
                tu("Task", json!({})),
                json!({"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"x"}}]}}),
            ],
            "{\"type\":\"assist",
        );
        let r = read_since(&p, 0, LogKind::Claude).unwrap();
        assert_eq!(r.actions.tools.keys().collect::<Vec<_>>(), ["Edit", "Task"]);
        assert_eq!((r.actions.files.len(), r.actions.subagents, r.bad_lines, r.capped), (1, 1, 0, false));
        assert_eq!(r.next, std::fs::metadata(&p).unwrap().len() - 15, "the half-written line waits");
        assert!(r.saw_prompt);
        // Starting mid-line (a tail read) skips the partial line; no prompt in the part read.
        let mid = read_since(&p, 5, LogKind::Claude).unwrap();
        assert!(mid.saw_prompt, "later prompts are still seen");
        let last =
            std::fs::read(&p).unwrap().iter().enumerate().filter(|(_, b)| **b == b'\n').map(|(i, _)| i).nth(4).unwrap();
        let tail = read_since(&p, last as u64 - 3, LogKind::Claude).unwrap();
        assert!(!tail.saw_prompt);
        assert_eq!(tail.actions.tools.keys().collect::<Vec<_>>(), ["Task"], "the cut line is skipped");
        // Nothing new: an empty turn.
        let again = read_since(&p, r.next, LogKind::Claude).unwrap();
        assert!(again.actions.is_empty() && again.bytes == 0);
    }

    #[test]
    fn copilot_read_keeps_steering_in_the_turn_and_counts_bad_lines() {
        let m = |d: &str, t: &str| json!({"type":"user.message","data":{"content":t,"delivery":d}});
        let start =
            |n: &str| json!({"type":"tool.execution_start","data":{"toolName":n,"arguments":"{\"path\":\"p.py\"}"}});
        let (_d, p) = write(
            &[
                m("idle", "go"),
                start("view"),
                m("steering", "also the tests"),
                start("edit"),
                json!({"type":"tool.execution_start","agentId":"s1","data":{"toolName":"bash","arguments":"{}"}}),
                json!({"type":"subagent.started","agentId":"s1","data":{}}),
            ],
            "not json \"user.message\"\n",
        );
        let r = read_since(&p, 0, LogKind::Copilot).unwrap();
        assert_eq!((r.actions.tool_calls(), r.actions.subagents, r.actions.files.len()), (2, 1, 1));
        assert_eq!(r.steering.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["also the tests"]);
        assert_eq!(r.bad_lines, 1);
    }
}
