//! End to end through the real `aiq` binary and a real core (architecture Phase 1 test check):
//! scripted Claude Code hook payloads → expected card / hold; core down → hook exits 0 within budget.
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_aiq");
const BASE: &str = "refactor the login handler to return typed errors";
const R1: &str = "refactor login handler so it returns typed errors";
const R2: &str = "login handler refactor: return typed errors please";
const R3: &str = "please refactor the login handler, typed errors returned";

struct Env {
    dir: TempDir,
    core: Option<Child>,
}

impl Env {
    fn new() -> Self {
        Self { dir: TempDir::new().unwrap(), core: None }
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }
    fn claude(&self) -> PathBuf {
        self.dir.path().join("claude")
    }
    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args)
            .env("AIQ_HOME", self.home())
            .env("CLAUDE_CONFIG_DIR", self.claude())
            .env_remove("NO_COLOR")
            .current_dir(self.dir.path());
        c
    }
    fn run(&self, args: &[&str], stdin: &[u8]) -> Output {
        let mut ch =
            self.cmd(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        ch.stdin.take().unwrap().write_all(stdin).unwrap();
        ch.wait_with_output().unwrap()
    }
    fn start_core(&mut self) {
        let c = self.cmd(&["core"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        self.core = Some(c);
        self.wait_running();
    }
    fn wait_running(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if self.status().contains("Tips today") {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("core did not start: {}", self.status());
    }
    fn stop_core(&mut self) {
        if let Some(mut c) = self.core.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
    fn status(&self) -> String {
        String::from_utf8(self.run(&["status"], b"").stdout).unwrap()
    }
    fn prompt(&self, session: &str, text: &str, pid: &str) -> Output {
        let p = json!({"session_id": session, "hook_event_name": "UserPromptSubmit", "prompt": text, "prompt_id": pid,
                       "cwd": "/repo", "transcript_path": "/t.jsonl", "permission_mode": "default"});
        self.run(&["hook", "claude-code", "UserPromptSubmit"], p.to_string().as_bytes())
    }
    fn stop(&self, session: &str, pid: &str) -> Output {
        let p = json!({"session_id": session, "hook_event_name": "Stop", "stop_hook_active": false, "prompt_id": pid,
                       "last_assistant_message": "done", "cwd": "/repo"});
        self.run(&["hook", "claude-code", "Stop"], p.to_string().as_bytes())
    }
    /// Ingest is fire-and-forget; wait until the core has stored `n` prompts.
    fn wait_prompts(&self, n: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.status().contains(&format!("{n} prompt")) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("prompts not stored: {}", self.status());
    }
    fn reprompt_run(&self, session: &str, start: usize) -> String {
        self.reprompt_texts(session, start, [BASE, R1, R2, R3])
    }
    fn reprompt_texts(&self, session: &str, start: usize, texts: [&str; 4]) -> String {
        for (i, t) in texts.iter().enumerate() {
            let o = self.prompt(session, t, &format!("{session}-p{}", start + i));
            assert!(o.status.success() && o.stdout.is_empty(), "UserPromptSubmit never prints");
        }
        self.wait_prompts(start + 4);
        format!("{session}-p{}", start + 3)
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        self.stop_core();
    }
}

fn assert_hook_stdout_contract(o: &Output) {
    assert!(o.status.success(), "hook must exit 0");
    if o.stdout.is_empty() {
        return;
    }
    let s = String::from_utf8(o.stdout.clone()).unwrap();
    assert_eq!(s.lines().count(), 1, "one line: {s}");
    let v: Value = serde_json::from_str(&s).unwrap();
    let obj = v.as_object().unwrap();
    assert_eq!(obj.keys().collect::<Vec<_>>(), vec!["systemMessage"], "only systemMessage: {s}");
}

#[test]
fn e2e_reword_three_times_one_card_second_run_held() {
    let mut e = Env::new();
    e.start_core();
    let pid = e.reprompt_run("s1", 0);
    let o = e.stop("s1", &pid);
    assert_hook_stdout_contract(&o);
    let msg: Value = serde_json::from_slice(&o.stdout).expect("a card at this Stop");
    let text = msg["systemMessage"].as_str().unwrap();
    assert!(
        text.starts_with(
            "\u{1b}[22;33m\u{1b}[1mAIQ tip:\u{1b}[22m you've reworded this request 3 times without adding new details."
        ),
        "{text}"
    );
    assert!(text.contains("Try this: ") && text.contains("aiq pause"));
    for t in [BASE, R1, R2, R3] {
        assert!(!text.contains(t), "the card never quotes prompt text");
    }
    // A second qualifying run in the same session is held.
    let other = "write unit tests for the payment retry backoff schedule";
    let pid2 = e.reprompt_texts("s1", 4, [other, other, other, other]);
    let o2 = e.stop("s1", &pid2);
    assert_hook_stdout_contract(&o2);
    assert!(o2.stdout.is_empty(), "second card in the session must be held");
    let st = e.status();
    assert!(st.contains("1 shown (ok)") && st.contains("1 hold (session_budget)"), "{st}");
    // aiq show prints the evidence prompts.
    let id = text.rsplit("aiq show ").next().unwrap().split_whitespace().next().unwrap();
    let show = String::from_utf8(e.run(&["show", id], b"").stdout).unwrap();
    assert!(show.contains(BASE) && show.contains(R3) && show.contains("Status: shown"), "{show}");
}

#[test]
fn e2e_stop_without_signal_prints_nothing() {
    let mut e = Env::new();
    e.start_core();
    e.prompt("s", "hello there, explain this module", "p0");
    e.wait_prompts(1);
    let o = e.stop("s", "p0");
    assert!(o.status.success() && o.stdout.is_empty());
}

#[test]
fn e2e_paused_nothing_captured_no_card() {
    let mut e = Env::new();
    e.start_core();
    assert!(e.run(&["pause"], b"").status.success());
    for (i, t) in [BASE, R1, R2, R3].iter().enumerate() {
        e.prompt("s", t, &format!("p{i}"));
    }
    let o = e.stop("s", "p3");
    assert!(o.stdout.is_empty());
    let st = e.status();
    assert!(st.contains("AIQ is paused") && st.contains("Events stored: none"), "{st}");
    assert!(e.run(&["resume"], b"").status.success());
    assert!(!e.status().contains("paused"));
}

#[test]
fn e2e_core_down_stop_exits_zero_within_50ms_p95_and_spools() {
    let e = Env::new();
    let mut times = Vec::new();
    for i in 0..50 {
        let t = Instant::now();
        let o = e.stop("s", &format!("p{i}"));
        times.push(t.elapsed());
        assert!(o.status.success() && o.stdout.is_empty());
    }
    times.sort();
    let p95 = times[(times.len() * 95).div_ceil(100) - 1];
    // Budget of record is 50 ms (design). Shared Windows CI runners scan every new process, so that job
    // widens it through AIQ_E2E_BUDGET_MS; the laptop run is the Windows measurement of record.
    let budget = std::env::var("AIQ_E2E_BUDGET_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(50);
    assert!(p95 <= Duration::from_millis(budget), "p95 {p95:?} > {budget} ms");
    eprintln!("core-down Stop hook p95: {p95:?} over {} runs", times.len());
    let spool = std::fs::read_to_string(e.home().join("spool.jsonl")).unwrap();
    assert_eq!(spool.lines().count(), 50);
    assert!(!spool.contains("token"));
    assert!(!e.home().join("core.lock").exists() || e.status().contains("not running"), "Stop never starts the core");
}

#[test]
fn e2e_core_down_prompt_starts_core_once_core_survives_and_drains_spool() {
    let e = Env::new();
    let o = e.prompt("s", BASE, "p0");
    assert!(o.status.success() && o.stdout.is_empty());
    e.prompt("s", R1, "p1"); // within 10 s back-off: no second start
    e.wait_running();
    e.wait_prompts(2);
    let errs = std::fs::read_to_string(e.home().join("hook-errors.log")).unwrap_or_default();
    assert!(errs.lines().filter(|l| *l == "AIQ-CORE-START-BACKOFF").count() <= 1, "{errs}");
    assert!(!e.home().join("spool.jsonl").exists(), "spool drained");
    // Clean up the detached core.
    assert!(e.run(&["forget", "--all", "--yes"], b"").status.success());
}

#[test]
fn e2e_random_bytes_never_print_always_exit_zero() {
    let e = Env::new();
    let mut x: u64 = 0x9e3779b97f4a7c15;
    for n in [0usize, 1, 7, 64, 1000, 300_000] {
        let bytes: Vec<u8> = (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect();
        for ev in ["UserPromptSubmit", "Stop"] {
            let o = e.run(&["hook", "claude-code", ev], &bytes);
            assert!(o.status.success() && o.stdout.is_empty() && o.stderr.is_empty());
        }
    }
    let o = e.run(&["hook", "claude-code", "Stop"], br#"[1,2,3]"#);
    assert!(o.status.success() && o.stdout.is_empty());
}

#[test]
fn e2e_copilot_shaped_payload_dropped_and_counted() {
    let mut e = Env::new();
    e.start_core();
    let o = e.run(&["hook", "claude-code", "UserPromptSubmit"], br#"{"sessionId":"x","prompt":"hi","timestamp":1}"#);
    assert!(o.status.success() && o.stdout.is_empty());
    let st = e.status();
    assert!(st.contains("AIQ-HOOK-FOREIGN-HOST 1") && st.contains("Events stored: none"), "{st}");
}

#[test]
fn e2e_concurrent_stops_deliver_once() {
    let mut e = Env::new();
    e.start_core();
    let pid = e.reprompt_run("s", 0);
    let p =
        json!({"session_id": "s", "hook_event_name": "Stop", "stop_hook_active": false, "prompt_id": pid}).to_string();
    let children: Vec<Child> = (0..8)
        .map(|_| {
            let mut c =
                e.cmd(&["hook", "claude-code", "Stop"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
            c.stdin.take().unwrap().write_all(p.as_bytes()).unwrap();
            c
        })
        .collect();
    let cards = children.into_iter().map(|c| c.wait_with_output().unwrap()).filter(|o| !o.stdout.is_empty()).count();
    assert!(cards <= 1, "{cards} cards");
    // If every racer timed out (slow CI), the card is still ready for the next Stop: never two.
    if cards == 0 {
        assert!(!e.stop("s", &pid).stdout.is_empty());
    }
    assert!(e.stop("s", &pid).stdout.is_empty());
}

#[test]
fn e2e_wrong_token_frame_closed_nothing_stored() {
    let mut e = Env::new();
    e.start_core();
    let paths = aiq_core::paths::Paths::at(&e.home());
    let frame = r#"{"op":"status","token":"0000"}"#;
    let r = aiq_core::ipc::request(&paths, frame, true, Duration::from_secs(2)).unwrap();
    assert_eq!(r, None, "closed without reply");
    assert!(e.status().contains("AIQ-API-AUTH 1"));
}

#[test]
fn e2e_second_core_refuses_to_start() {
    let mut e = Env::new();
    e.start_core();
    let o = e.run(&["core"], b"");
    assert_eq!(o.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&o.stderr).contains("AIQ-CORE-ALREADY-RUNNING"));
}

#[test]
fn e2e_logs_never_contain_prompt_text() {
    let mut e = Env::new();
    e.start_core();
    let canary = "canary-7f3a9 refactor the login handler to return typed errors";
    for (i, t) in [canary, canary, canary, canary].iter().enumerate() {
        e.prompt("s", t, &format!("p{i}"));
    }
    e.wait_prompts(4);
    e.stop("s", "p3");
    e.stop_core();
    for f in ["aiq.log", "hook-timings.log", "hook-errors.log"] {
        let t = std::fs::read_to_string(e.home().join(f)).unwrap_or_default();
        assert!(!t.contains("canary-7f3a9"), "{f} leaked prompt text");
    }
    let log = std::fs::read_to_string(e.home().join("aiq.log")).unwrap();
    assert!(log.contains("card decision") && log.contains("trace_id"), "decisions are logged with trace ids");
}

#[test]
fn e2e_forget_all_deletes_store_and_spool() {
    let mut e = Env::new();
    e.start_core();
    e.reprompt_run("s", 0);
    std::fs::write(e.home().join("spool.jsonl"), "x\n").unwrap();
    let o = e.run(&["forget", "--all", "--yes"], b"");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stdout));
    let _ = e.core.take().map(|mut c| c.wait()); // the core exits by itself
    for f in ["aiq.db", "aiq.db-wal", "spool.jsonl"] {
        assert!(!e.home().join(f).exists(), "{f} still exists");
    }
}

#[test]
fn e2e_forget_all_needs_confirmation() {
    let mut e = Env::new();
    e.start_core();
    let o = e.run(&["forget", "--all"], b"n\n");
    assert!(String::from_utf8_lossy(&o.stdout).contains("Nothing deleted."));
    assert!(e.home().join("aiq.db").exists());
}

#[test]
fn e2e_status_first_run_and_core_down_states() {
    let mut e = Env::new();
    let down = e.status();
    assert!(down.contains("AIQ is not running"), "{down}");
    e.start_core();
    assert!(e.status().contains("No activity yet."));
}

#[test]
fn e2e_config_daily_cap_validates() {
    let mut e = Env::new();
    e.start_core();
    assert!(e.run(&["config", "set", "daily-cap", "2"], b"").status.success());
    assert!(e.status().contains("of 2"));
    let bad = e.run(&["config", "set", "daily-cap", "lots"], b"");
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stdout).contains("Not changed"));
}

#[test]
fn e2e_no_color_card_is_plain_text() {
    let mut e = Env::new();
    e.start_core();
    let pid = e.reprompt_run("s1", 0);
    let p = json!({"session_id": "s1", "hook_event_name": "Stop", "stop_hook_active": false, "prompt_id": pid});
    let mut ch = e
        .cmd(&["hook", "claude-code", "Stop"])
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    ch.stdin.take().unwrap().write_all(p.to_string().as_bytes()).unwrap();
    let o = ch.wait_with_output().unwrap();
    let msg: Value = serde_json::from_slice(&o.stdout).expect("a card at this Stop");
    let text = msg["systemMessage"].as_str().unwrap();
    assert!(text.starts_with("AIQ tip: you've reworded"), "{text}");
    assert!(!text.contains('\u{1b}'), "NO_COLOR means no escape codes");
}

// ---- install / uninstall (STRIDE B4) ----

fn settings(e: &Env) -> PathBuf {
    e.claude().join("settings.json")
}

const ODD_SETTINGS: &str = "{\n    \"model\": \"opus\",\n    \"hooks\": {\"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"notify-send done\"}]}]}\n}\n";

#[test]
fn e2e_install_uninstall_round_trip_byte_identical() {
    let e = Env::new();
    std::fs::create_dir_all(e.claude()).unwrap();
    std::fs::write(settings(&e), ODD_SETTINGS).unwrap();
    let o = e.run(&["install", "claude-code", "--yes"], b"");
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success() && out.contains("Backup: ") && out.contains("Self-test"), "{out}");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(settings(&e)).unwrap()).unwrap();
    assert_eq!(v["model"], "opus");
    assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "notify-send done", "other hooks kept first");
    for (ev, is_async) in [("UserPromptSubmit", true), ("Stop", false)] {
        let groups = v["hooks"][ev].as_array().unwrap();
        let h = &groups.last().unwrap()["hooks"][0];
        assert_eq!(h["args"], json!(["hook", "claude-code", ev]), "exec form with args");
        assert!(Path::new(h["command"].as_str().unwrap()).is_absolute());
        assert_eq!(h.get("async") == Some(&json!(true)), is_async);
    }
    assert!(
        String::from_utf8_lossy(&e.run(&["install", "claude-code", "--yes"], b"").stdout)
            .contains("Already installed.")
    );
    assert!(e.status().contains("Hooks: installed"));
    assert!(e.run(&["uninstall", "claude-code", "--yes"], b"").status.success());
    assert_eq!(std::fs::read_to_string(settings(&e)).unwrap(), ODD_SETTINGS);
    assert!(!e.dir.path().join(".claude").exists(), "never writes to the working directory");
}

/// Found on the Windows laptop check: installing again from a moved aiq binary must not make
/// uninstall "restore" a backup that still holds the first install's AIQ hooks (DC-006).
#[test]
fn e2e_reinstall_from_moved_binary_then_uninstall_restores_original() {
    let e = Env::new();
    std::fs::create_dir_all(e.claude()).unwrap();
    std::fs::write(settings(&e), ODD_SETTINGS).unwrap();
    assert!(e.run(&["install", "claude-code", "--yes"], b"").status.success());
    let moved = e.dir.path().join("bin").join(Path::new(BIN).file_name().unwrap());
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::copy(BIN, &moved).unwrap();
    // A freshly copied binary can be briefly "text file busy" on Linux: another test thread's fork
    // may still hold the copy's write fd until its exec. Retry only that error.
    let mut tries = 0;
    let o = loop {
        match Command::new(&moved)
            .args(["install", "claude-code", "--yes"])
            .env("AIQ_HOME", e.home())
            .env("CLAUDE_CONFIG_DIR", e.claude())
            .current_dir(e.dir.path())
            .output()
        {
            Err(err) if err.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 50 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
            r => break r.unwrap(),
        }
    };
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: Value = serde_json::from_str(&std::fs::read_to_string(settings(&e)).unwrap()).unwrap();
    for ev in ["UserPromptSubmit", "Stop"] {
        let aiq: Vec<_> = v["hooks"][ev]
            .as_array()
            .unwrap()
            .iter()
            .filter(|g| g["hooks"][0]["args"].get(0) == Some(&json!("hook")))
            .collect();
        assert_eq!(aiq.len(), 1, "{ev}: one AIQ entry after reinstall");
        let cmd = Path::new(aiq[0]["hooks"][0]["command"].as_str().unwrap()).canonicalize().unwrap();
        assert_eq!(cmd, moved.canonicalize().unwrap(), "{ev}: repointed to the moved binary");
    }
    assert!(e.run(&["uninstall", "claude-code", "--yes"], b"").status.success());
    assert_eq!(std::fs::read_to_string(settings(&e)).unwrap(), ODD_SETTINGS, "no stale AIQ hooks left behind");
}

/// Edited between installs: the reinstall's fresh backup holds AIQ hooks, so uninstall strips
/// instead of restoring it (DC-006).
#[test]
fn e2e_reinstall_after_edit_then_uninstall_strips_all_aiq() {
    let e = Env::new();
    std::fs::create_dir_all(e.claude()).unwrap();
    std::fs::write(settings(&e), ODD_SETTINGS).unwrap();
    e.run(&["install", "claude-code", "--yes"], b"");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(settings(&e)).unwrap()).unwrap();
    v["theme"] = json!("dark");
    v["hooks"]["Stop"][1]["hooks"][0]["command"] = json!("/old/place/aiq");
    std::fs::write(settings(&e), serde_json::to_string_pretty(&v).unwrap()).unwrap();
    assert!(e.run(&["install", "claude-code", "--yes"], b"").status.success());
    assert!(e.run(&["uninstall", "claude-code", "--yes"], b"").status.success());
    let after: Value = serde_json::from_str(&std::fs::read_to_string(settings(&e)).unwrap()).unwrap();
    assert_eq!(after["theme"], "dark");
    assert_eq!(after["hooks"], json!({"Stop": [{"hooks": [{"type": "command", "command": "notify-send done"}]}]}));
}

#[test]
fn e2e_install_no_settings_then_uninstall_removes_file() {
    let e = Env::new();
    assert!(e.run(&["install", "claude-code", "--yes"], b"").status.success());
    assert!(settings(&e).exists());
    assert!(e.run(&["uninstall", "claude-code", "--yes"], b"").status.success());
    assert!(!settings(&e).exists());
}

#[test]
fn e2e_install_invalid_json_refused_untouched() {
    let e = Env::new();
    std::fs::create_dir_all(e.claude()).unwrap();
    std::fs::write(settings(&e), "{ not json").unwrap();
    let o = e.run(&["install", "claude-code", "--yes"], b"");
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("not valid JSON"));
    assert_eq!(std::fs::read_to_string(settings(&e)).unwrap(), "{ not json");
}

#[test]
fn e2e_install_declined_changes_nothing() {
    let e = Env::new();
    std::fs::create_dir_all(e.claude()).unwrap();
    std::fs::write(settings(&e), ODD_SETTINGS).unwrap();
    let o = e.run(&["install", "claude-code"], b"n\n");
    assert!(String::from_utf8_lossy(&o.stdout).contains("Nothing changed."));
    assert_eq!(std::fs::read_to_string(settings(&e)).unwrap(), ODD_SETTINGS);
}

#[test]
fn e2e_uninstall_after_user_edit_strips_only_aiq() {
    let e = Env::new();
    std::fs::create_dir_all(e.claude()).unwrap();
    std::fs::write(settings(&e), ODD_SETTINGS).unwrap();
    e.run(&["install", "claude-code", "--yes"], b"");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(settings(&e)).unwrap()).unwrap();
    v["theme"] = json!("dark"); // the person edits after install
    std::fs::write(settings(&e), serde_json::to_string_pretty(&v).unwrap()).unwrap();
    e.run(&["uninstall", "claude-code", "--yes"], b"");
    let after: Value = serde_json::from_str(&std::fs::read_to_string(settings(&e)).unwrap()).unwrap();
    assert_eq!(after["theme"], "dark");
    assert_eq!(after["hooks"], json!({"Stop": [{"hooks": [{"type": "command", "command": "notify-send done"}]}]}));
}
