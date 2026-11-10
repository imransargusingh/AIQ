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
            .env_remove("CLAUDE_CODE_ENTRYPOINT") // tests may run inside Claude Code; set per test
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
    /// A terminal session's transcript as Claude Code has written it by the prompt hook: the prompt's own user record,
    /// typed by a person and stamped now, so rule RC settles "not Projects" at once (DC-035) and the per-turn rule
    /// counts the turn without waiting (DC-043).
    fn transcript(&self) -> String {
        let t = self.dir.path().join("t.jsonl");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        let ts = aiq_core::sysclock::format_rfc3339_ms(now);
        let rec =
            format!("{{\"type\":\"user\",\"turnOrigin\":\"human\",\"entrypoint\":\"cli\",\"timestamp\":\"{ts}\"}}\n");
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&t).unwrap();
        std::io::Write::write_all(&mut f, rec.as_bytes()).unwrap();
        t.to_string_lossy().into_owned()
    }
    fn prompt(&self, session: &str, text: &str, pid: &str) -> Output {
        let p = json!({"session_id": session, "hook_event_name": "UserPromptSubmit", "prompt": text, "prompt_id": pid,
                       "cwd": "/repo", "transcript_path": self.transcript(), "permission_mode": "default"});
        self.run(&["hook", "claude-code", "UserPromptSubmit"], p.to_string().as_bytes())
    }
    fn stop(&self, session: &str, pid: &str) -> Output {
        let p = json!({"session_id": session, "hook_event_name": "Stop", "stop_hook_active": false, "prompt_id": pid,
                       "last_assistant_message": "done", "cwd": "/repo"});
        self.run(&["hook", "claude-code", "Stop"], p.to_string().as_bytes())
    }
    /// A Stop where a card is due. A loaded runner can miss the core's hand-over window at one turn end; the
    /// product keeps the card show-ready for the next (DC-022), so end the turn again, as Claude Code would, for
    /// up to twice the confirm hold. Every miss must leave the card show-ready, so a lost card still fails.
    /// (The same control as `e2e_copilot.rs`; this file was missed by DC-022's sweep: Windows CI, PR #203.)
    fn stop_until_card(&self, stop: impl Fn() -> Output) -> Output {
        let deadline = Instant::now() + 2 * Duration::from_millis(aiq_core::service::HANDOVER_CONFIRM_MS as u64);
        let mut n = 0;
        while Instant::now() < deadline {
            n += 1;
            let o = stop();
            if !o.stdout.is_empty() {
                return o;
            }
            let st = self.status();
            let timings = std::fs::read_to_string(self.home().join("hook-timings.log")).unwrap_or_default();
            assert!(st.contains("show-ready"), "a Stop printed nothing and the card is gone:\n{st}\n{timings}");
            std::thread::sleep(Duration::from_millis(250));
        }
        panic!("no card in {n} Stops: {}", self.status());
    }
    /// Wait until `aiq status` contains every one of `parts` (the hook confirms a printed tip after it exits).
    fn wait_status(&self, parts: &[&str]) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let st = self.status();
            if parts.iter().all(|p| st.contains(p)) || Instant::now() >= deadline {
                return st;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
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

/// The frames the hook spooled, one file each in `spool.d` (DC-048), oldest first.
fn spooled(home: &std::path::Path) -> Vec<String> {
    let mut v: Vec<_> = std::fs::read_dir(home.join("spool.d"))
        .map(|d| d.flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect())
        .unwrap_or_default();
    v.sort();
    v.iter().map(|p| std::fs::read_to_string(p).unwrap()).collect()
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
    let o = e.stop_until_card(|| e.stop("s1", &pid));
    assert_hook_stdout_contract(&o);
    let msg: Value = serde_json::from_slice(&o.stdout).expect("a card at this Stop");
    let text = msg["systemMessage"].as_str().unwrap();
    assert!(
        text.starts_with(
            "\u{1b}[22;33m\u{1b}[1;30;43mAIQ tip:\u{1b}[22;49;33m this request went round 3 times. The agent may need one new detail."
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
    let st = e.wait_status(&["1 shown (ok)", "1 hold (session_budget)"]);
    assert!(st.contains("1 shown (ok)") && st.contains("1 hold (session_budget)"), "{st}");
    // aiq show prints the evidence prompts.
    let id = text.rsplit("aiq show ").next().unwrap().split_whitespace().next().unwrap();
    let show = String::from_utf8(e.run(&["show", id], b"").stdout).unwrap();
    assert!(show.contains(BASE) && show.contains(R3) && show.contains("Status: shown"), "{show}");
}

#[test]
fn e2e_headless_claude_runs_record_nothing() {
    // Import spec rule NI / US-12: `claude -p` (entrypoint sdk-cli) is a program, not a person.
    let mut e = Env::new();
    e.start_core();
    for (i, t) in ["summarize the repo", "summarize the repo again"].iter().enumerate() {
        let p = json!({"session_id": "bot", "hook_event_name": "UserPromptSubmit", "prompt": t, "prompt_id": format!("b{i}"), "cwd": "/repo"});
        let mut c = e.cmd(&["hook", "claude-code", "UserPromptSubmit"]);
        c.env("CLAUDE_CODE_ENTRYPOINT", "sdk-cli").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut ch = c.spawn().unwrap();
        ch.stdin.take().unwrap().write_all(p.to_string().as_bytes()).unwrap();
        let o = ch.wait_with_output().unwrap();
        assert!(o.status.success() && o.stdout.is_empty() && o.stderr.is_empty());
    }
    // AIQ's own check folder (rule AIQ): stored nowhere, and its hook run is left out of the hook times too.
    let p = json!({"session_id": "probe", "hook_event_name": "UserPromptSubmit", "prompt": "run: echo aiq-probe",
                   "prompt_id": "c1", "cwd": "/tmp/aiq-e2e-check", "transcript_path": "/t.jsonl"});
    e.run(&["hook", "claude-code", "UserPromptSubmit"], p.to_string().as_bytes());
    e.prompt("me", "a prompt of my own", "m1");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !e.status().contains("leave out 3 runs") && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let st = e.status();
    assert!(st.contains("Events stored: 1 prompt"), "{st}");
    // Only the person's own run counts against AIQ's hook budget; the three set-aside runs are named.
    assert!(st.contains("Hook time in-process, UserPromptSubmit: p50") && st.contains("(1 runs)"), "{st}");
    assert!(st.contains("Hook times leave out 3 runs from sessions AIQ sets aside"), "{st}");
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
fn e2e_core_down_stop_exits_zero_within_budget_p95_and_spools() {
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
    // Budget of record is 100 ms p95 for the whole process (Imran, 2026-11-01; was 50 ms). Shared Windows
    // CI runners scan every new process, so that job widens it through AIQ_E2E_BUDGET_MS; the laptop run
    // is the Windows measurement of record.
    let budget = std::env::var("AIQ_E2E_BUDGET_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
    assert!(p95 <= Duration::from_millis(budget), "p95 {p95:?} > {budget} ms");
    eprintln!("core-down Stop hook p95: {p95:?} over {} runs", times.len());
    let spool = spooled(&e.home());
    assert_eq!(spool.len(), 50);
    assert!(spool.iter().all(|f| f.lines().count() == 1 && !f.contains("token")));
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
    assert!(spooled(&e.home()).is_empty(), "spool drained");
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
fn e2e_core_logs_its_version_at_start() {
    // A read-only check after an update reads the core's version from aiq.log, without running aiq.exe.
    let mut e = Env::new();
    e.start_core();
    e.stop_core();
    let log = std::fs::read_to_string(e.home().join("aiq.log")).unwrap();
    let started: Vec<Value> = log
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["fields"]["message"] == "core started")
        .collect();
    assert_eq!(started.len(), 1, "one start line per core start:\n{log}");
    assert_eq!(started[0]["fields"]["version"], env!("CARGO_PKG_VERSION"), "start line names the version");
}

#[test]
fn e2e_forget_all_deletes_store_and_spool() {
    let mut e = Env::new();
    e.start_core();
    e.reprompt_run("s", 0);
    std::fs::write(e.home().join("spool.jsonl"), "x\n").unwrap();
    std::fs::create_dir_all(e.home().join("spool.d")).unwrap();
    std::fs::write(e.home().join("spool.d").join("f.jsonl"), "x\n").unwrap();
    let o = e.run(&["forget", "--all", "--yes"], b"");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stdout));
    let _ = e.core.take().map(|mut c| c.wait()); // the core exits by itself
    for f in ["aiq.db", "aiq.db-wal", "spool.jsonl", "spool.d"] {
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
fn e2e_status_reads_the_dev_swap_log_next_to_the_exes() {
    // The laptop's dev swapper logs in <home>\bin, not <home> (measured 2026-11-02 after #66 was installed).
    let e = Env::new();
    assert!(e.status().contains("no upgrade recorded"), "{}", e.status());
    let bin = e.home().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("upgrade-log.txt"), "2026-11-02T14:00:57.7222903-08:00 swapped aiq.exe (5920768 bytes)\n")
        .unwrap();
    let st = e.status();
    assert!(st.contains("since the last upgrade at 2026-11-02 22:00Z"), "{st}");
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
fn e2e_analysis_names_and_old_judge_names_both_work() {
    let mut e = Env::new();
    e.start_core();
    // Users see "analysis"; the old "judge" names stay accepted so earlier commands keep working.
    let set = e.run(&["config", "set", "analysis-daily-tasks", "7"], b"");
    assert!(set.status.success(), "{}", String::from_utf8_lossy(&set.stdout));
    assert!(String::from_utf8_lossy(&set.stdout).contains("Most tasks analyzed per day: 7."));
    assert!(e.run(&["config", "set", "judge-daily-tasks", "8"], b"").status.success());
    let bad = e.run(&["config", "set", "analysis", "maybe"], b"");
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stdout).contains("analysis must be auto or off"));
    for verb in ["analyze", "judge"] {
        let o = e.run(&[verb, "--dry-run"], b"");
        let out = String::from_utf8_lossy(&o.stdout);
        assert!(out.starts_with("AIQ analyzes each task you finish"), "{verb}: {out}");
    }
}

#[test]
fn e2e_no_color_card_is_plain_text() {
    let mut e = Env::new();
    e.start_core();
    let pid = e.reprompt_run("s1", 0);
    let p = json!({"session_id": "s1", "hook_event_name": "Stop", "stop_hook_active": false, "prompt_id": pid});
    let o = e.stop_until_card(|| {
        let mut ch = e
            .cmd(&["hook", "claude-code", "Stop"])
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        ch.stdin.take().unwrap().write_all(p.to_string().as_bytes()).unwrap();
        ch.wait_with_output().unwrap()
    });
    let msg: Value = serde_json::from_slice(&o.stdout).expect("a card at this Stop");
    let text = msg["systemMessage"].as_str().unwrap();
    assert!(text.starts_with("AIQ tip: this request went round"), "{text}");
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
    assert!(e.status().contains("Claude Code: installed"));
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

#[test]
fn e2e_import_dry_run_then_import_status_and_forget_imported() {
    let e = Env::new();
    let dir = e.claude().join("projects").join("C--repos-a");
    std::fs::create_dir_all(&dir).unwrap();
    let rows = [
        json!({"type": "user", "entrypoint": "cli", "cwd": "/repos/a", "timestamp": "2026-10-20T10:00:00Z",
               "message": {"content": "fix the failing test"}}),
        json!({"type": "assistant", "timestamp": "2026-10-20T10:01:00Z",
               "message": {"content": [{"type": "tool_use", "id": "t1", "name": "Edit", "input": {"file_path": "a.rs"}}]}}),
    ];
    let body: String = rows.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(dir.join("s1.jsonl"), body).unwrap();
    let copilot = e.dir.path().join("copilot");
    let run = |args: &[&str]| {
        let o = e.cmd(args).env("COPILOT_HOME", &copilot).stdin(Stdio::null()).output().unwrap();
        assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).into_owned()
    };
    let dry = run(&["import", "--dry-run"]);
    assert!(dry.contains("Dry run") && dry.contains("Claude Code: 1 sessions found, 1 imported"), "{dry}");
    assert!(!run(&["status"]).contains("Imported history"));
    let done = run(&["import"]);
    assert!(done.contains("Import finished."), "{done}");
    assert!(run(&["status"]).contains("Imported history: 1 sessions, 0 automated and not counted"));
    assert!(run(&["import"]).contains("0 imported, 0 automated and not counted, 1 unchanged"));
    assert!(run(&["forget", "--imported", "--yes"]).contains("Deleted 1 imported sessions."));
    assert!(!run(&["status"]).contains("Imported history"));
}

#[test]
fn e2e_spooled_frame_goes_in_at_the_next_command_not_the_idle_tick() {
    // A hook whose send missed while the core was busy spools; the core must take it in before its next
    // command, because a steady stream of commands keeps the 30 s idle tick from ever firing.
    let down = Env::new();
    assert!(down.stop("s", "p0").status.success());
    let frame = spooled(&down.home());
    assert_eq!(frame.len(), 1);
    let mut e = Env::new();
    e.start_core();
    std::fs::create_dir_all(e.home().join("spool.d")).unwrap();
    std::fs::write(e.home().join("spool.d").join("f.jsonl"), &frame[0]).unwrap();
    let st = e.status();
    assert!(st.contains("1 stop"), "taken in at the very next command: {st}");
    assert!(spooled(&e.home()).is_empty());
}

/// DC-022: a hook that took a tip and then never printed it (its wait ran out, or it died) must not use the tip
/// up. Before the fix the core counted it as shown the moment it handed it over: lost, yet it blocked the next.
#[test]
fn e2e_a_tip_no_hook_says_it_printed_is_not_counted_and_comes_back() {
    let mut e = Env::new();
    e.start_core();
    let pid = e.reprompt_run("s", 0);
    let paths = aiq_core::paths::Paths::at(&e.home());
    let token = aiq_core::ipc::read_token(&paths).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    let stop = json!({"schema": aiq_contracts::event::SCHEMA, "event_id": "gone-hook-stop", "surface": "claude-code",
                      "surface_session_id": "s", "seq": null, "ts": aiq_core::sysclock::format_rfc3339_ms(now),
                      "collector_version": "test", "kind": "stop",
                      "payload": {"stop_hook_active": false, "prompt_id": pid}});
    // A hook that takes the tip with a generous wait, then goes away without printing or confirming it.
    let frame = json!({"op": "take_card", "token": token, "trace_id": null, "event": stop, "wait_ms": 2000});
    let reply = aiq_core::ipc::request(&paths, &frame.to_string(), true, Duration::from_secs(3)).unwrap().unwrap();
    assert!(reply.contains("\"card\":{"), "the tip was handed over: {reply}");
    assert!(e.status().contains("Tips today: 0 of"), "not counted: nobody printed it");
    assert!(e.stop("s", &pid).stdout.is_empty(), "held while the first hook may still confirm");
    std::thread::sleep(Duration::from_millis(aiq_core::service::HANDOVER_CONFIRM_MS as u64 + 100));
    // The next turn end gets it. A loaded runner may miss the core's 30 ms window: that miss keeps it ready.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut out = e.stop("s", &pid).stdout;
    while out.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
        out = e.stop("s", &pid).stdout;
    }
    assert!(String::from_utf8(out).unwrap().contains("AIQ tip:"), "ready again: {}", e.status());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !e.status().contains("Tips today: 1 of") && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    // The gone hook's hand-over is counted once. A retry whose hook gave up after the core had taken the tip
    // for it is a real hand-over too and counts as well (`hand_back` or expiry; Windows CI on #185 counted 2),
    // so allow one more per such retry, read from the hooks' own outcomes, and never more.
    let st = e.status();
    let timings = std::fs::read_to_string(e.home().join("hook-timings.log")).unwrap_or_default();
    let missed = timings
        .lines()
        .filter_map(|l| l.strip_prefix("Stop ").and_then(|r| r.split(' ').nth(1)))
        .filter(|o| o.starts_with("stop-timeout") || o.starts_with("card-"))
        .count();
    let unconfirmed = st
        .split("AIQ-CARD-UNCONFIRMED ")
        .nth(1)
        .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(0);
    assert!(st.contains("Tips today: 1 of"), "{st}");
    assert!(
        (1..=1 + missed).contains(&unconfirmed),
        "unconfirmed {unconfirmed}, missed retries {missed}:\n{st}\n{timings}"
    );
}

// ---- the one-time milestone note (Milestones design, Imran 2026-11-10) ----

/// Three analysed "Writing tests" tasks at Delegated: the kind reaches a milestone, and the next Claude Code turn
/// end with no tip says it once, as a systemMessage in the trend's colour; the next turn end says nothing.
#[test]
fn e2e_claude_code_says_a_new_milestone_once() {
    let mut e = Env::new();
    e.start_core();
    for i in 0..3 {
        let o = e.prompt(&format!("old{i}"), "write unit tests for the parser module", &format!("old{i}-p0"));
        assert!(o.status.success() && o.stdout.is_empty(), "UserPromptSubmit never prints");
    }
    e.wait_prompts(3);
    let store = aiq_store::Store::open(&e.home().join("aiq.db")).unwrap();
    for p in store.prompts_for_rebuild(0).unwrap() {
        store.insert_judgment(&delegated_tests(&p.event_id)).unwrap();
    }
    drop(store);
    e.prompt("s1", "keep going", "s1-p0");
    e.wait_prompts(4);
    let o = e.stop("s1", "s1-p0");
    assert_hook_stdout_contract(&o);
    let msg: Value = serde_json::from_slice(&o.stdout).expect("the milestone note at this Stop");
    let text = msg["systemMessage"].as_str().unwrap();
    assert!(text.contains("AIQ milestone: Writing tests reached Delegated · see it in AIQ Home"), "{text}");
    assert!(!text.contains("AIQ tip"), "{text}");
    e.prompt("s1", "and the lexer", "s1-p1");
    e.wait_prompts(5);
    assert!(e.stop("s1", "s1-p1").stdout.is_empty(), "said once");
}

fn delegated_tests(task_id: &str) -> aiq_store::JudgmentRow {
    aiq_store::JudgmentRow {
        judgment_id: format!("j-{task_id}"),
        task_id: task_id.into(),
        origin: "live".into(),
        provider: "claude-code".into(),
        model: None,
        judged_utc_ms: 1,
        job: "code_change".into(),
        job_confidence: 0.9,
        model_rung: 3,
        current_rung: 3,
        current_confidence: 0.85,
        potential_rung: 3,
        potential_confidence: 0.85,
        ceiling_rung: 5,
        state: "right_rung".into(),
        coachable: false,
        facts: "{}".into(),
        evidence: r#"["The agent wrote the tests."]"#.into(),
        gap: String::new(),
        blockers: "[]".into(),
        placement: "[]".into(),
        read: r#"{"prompts":1,"reprompts":0,"corrections":0}"#.into(),
        call_ids: "[]".into(),
        fact_set_version: "fs1".into(),
        rubric_version: "r1".into(),
        judge_version: "j2".into(),
        task_type: Some("tests".into()),
        task_type_confidence: Some(0.9),
    }
}
