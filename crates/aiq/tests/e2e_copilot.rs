//! End to end through the real `aiq` binary (architecture Phase 2 test check): scripted Copilot hook
//! payloads → tip in the status line after the turn ends; budget shared with Claude Code; install round trip.
use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_aiq");
const TEXTS: [&str; 4] = [
    "refactor the login handler to return typed errors",
    "refactor login handler so it returns typed errors",
    "login handler refactor: return typed errors please",
    "please refactor the login handler, typed errors returned",
];

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
    fn copilot(&self) -> PathBuf {
        self.dir.path().join("copilot")
    }
    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args)
            .env("AIQ_HOME", self.home())
            .env("CLAUDE_CONFIG_DIR", self.dir.path().join("claude"))
            .env("COPILOT_HOME", self.copilot())
            .env("NO_COLOR", "1")
            .env_remove("AIQ_INTERNAL")
            .current_dir(self.dir.path());
        c
    }
    fn run(&self, args: &[&str], stdin: &[u8]) -> Output {
        self.run_with(self.cmd(args), stdin)
    }
    fn run_with(&self, mut c: Command, stdin: &[u8]) -> Output {
        let mut ch = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        ch.stdin.take().unwrap().write_all(stdin).unwrap();
        ch.wait_with_output().unwrap()
    }
    fn start_core(&mut self) {
        let c = self.cmd(&["core"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        self.core = Some(c);
        self.wait_for("Tips today");
    }
    fn status(&self) -> String {
        String::from_utf8(self.run(&["status"], b"").stdout).unwrap()
    }
    fn wait_for(&self, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if self.status().contains(needle) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("never saw {needle:?}: {}", self.status());
    }
    fn hook(&self, event: &str, payload: Value) -> Output {
        let o = self.run(&["hook", "copilot", event], payload.to_string().as_bytes());
        assert!(o.status.success() && o.stdout.is_empty() && o.stderr.is_empty(), "Copilot hooks never print");
        o
    }
    fn prompt(&self, session: &str, text: &str) {
        let ts = chrono_ms();
        self.hook(
            "userPromptSubmitted",
            json!({"sessionId": session, "timestamp": ts, "cwd": "/repo", "prompt": text}),
        );
    }
    fn agent_stop(&self, session: &str) {
        let ts = chrono_ms();
        self.hook(
            "agentStop",
            json!({"sessionId": session, "timestamp": ts, "stopReason": "end_turn", "transcriptPath": "/t"}),
        );
    }
    fn statusline(&self, session: Option<&str>) -> String {
        let payload = session.map_or(json!({"cwd": "/repo"}), |s| json!({"session": {"id": s}, "cwd": "/repo"}));
        let o = self.run(&["statusline", "copilot"], payload.to_string().as_bytes());
        assert!(o.status.success() && o.stderr.is_empty());
        String::from_utf8(o.stdout).unwrap()
    }
    fn reprompt_run(&self, session: &str, already: usize) {
        for t in TEXTS {
            self.prompt(session, t);
        }
        self.wait_for(&format!("{} prompt", already + 4));
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Some(mut c) = self.core.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn chrono_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

#[test]
fn e2e_copilot_reword_tip_in_status_line_after_turn_then_clears_on_next_prompt() {
    let mut e = Env::new();
    e.start_core();
    e.reprompt_run("c1", 0);
    assert_eq!(e.statusline(Some("c1")), "", "no tip while the agent works");
    e.agent_stop("c1");
    e.wait_for("1 stop");
    let line = e.statusline(Some("c1"));
    assert!(
        line.starts_with("AIQ tip: you've reworded this request 3 times without adding new details · aiq show "),
        "{line}"
    );
    assert_eq!(line.lines().count(), 1);
    for t in TEXTS {
        assert!(!line.contains(t), "the tip never quotes prompt text");
    }
    assert_eq!(e.statusline(Some("c1")), line, "refresh keeps it");
    assert_eq!(e.statusline(Some("c2")), "", "another session never sees it");
    e.prompt("c1", "thanks, now add a test for the empty case");
    e.wait_for("5 prompt");
    assert_eq!(e.statusline(Some("c1")), "", "default: clears on the next prompt");
    let st = e.status();
    assert!(st.contains("1 shown (ok)") && st.contains("Copilot's own start-up"), "{st}");
}

#[test]
fn e2e_copilot_vscode_turn_never_reaches_the_cli_status_line() {
    // Laptop probe run 2: VS Code agent mode sets COPILOT_CLI_RUN_AS_NODE and not COPILOT_LOADER_PID.
    let mut e = Env::new();
    e.start_core();
    for t in TEXTS {
        let mut c = e.cmd(&["hook", "copilot", "userPromptSubmitted"]);
        c.env("COPILOT_CLI_RUN_AS_NODE", "1").env_remove("COPILOT_LOADER_PID");
        let p = json!({"sessionId": "v1", "timestamp": chrono_ms(), "cwd": "/repo", "prompt": t});
        assert!(e.run_with(c, p.to_string().as_bytes()).stdout.is_empty());
    }
    let mut c = e.cmd(&["hook", "copilot", "agentStop"]);
    c.env("COPILOT_CLI_RUN_AS_NODE", "1").env_remove("COPILOT_LOADER_PID");
    e.run_with(c, json!({"sessionId": "v1", "stopReason": "end_turn"}).to_string().as_bytes());
    e.wait_for("1 stop");
    assert_eq!(e.statusline(Some("v1")), "", "tagged vscode: the extension shows it, not the CLI");
    assert_eq!(e.statusline(None), "", "nor the newest CLI session");
}

#[test]
fn e2e_copilot_tip_stays_until_opened_setting() {
    let mut e = Env::new();
    e.start_core();
    assert!(e.run(&["config", "set", "copilot-tip-stays", "until-opened"], b"").status.success());
    assert!(!e.run(&["config", "set", "copilot-tip-stays", "forever"], b"").status.success());
    e.reprompt_run("c1", 0);
    e.agent_stop("c1");
    e.wait_for("1 stop");
    let line = e.statusline(Some("c1"));
    e.prompt("c1", "next: write the changelog entry");
    e.wait_for("5 prompt");
    assert_eq!(e.statusline(Some("c1")), line);
    let id = line.trim().rsplit("aiq show ").next().unwrap();
    assert!(String::from_utf8(e.run(&["show", id], b"").stdout).unwrap().contains(TEXTS[0]));
    assert_eq!(e.statusline(Some("c1")), "", "opened: gone");
}

#[test]
fn e2e_claude_code_card_then_copilot_run_held_by_quiet_period() {
    let mut e = Env::new();
    e.start_core();
    for (i, t) in TEXTS.iter().enumerate() {
        let p = json!({"session_id": "cc", "hook_event_name": "UserPromptSubmit", "prompt": t, "prompt_id": format!("p{i}")});
        e.run(&["hook", "claude-code", "UserPromptSubmit"], p.to_string().as_bytes());
    }
    e.wait_for("4 prompt");
    let stop = json!({"session_id": "cc", "hook_event_name": "Stop", "stop_hook_active": false, "prompt_id": "p3"});
    let o = e.run(&["hook", "claude-code", "Stop"], stop.to_string().as_bytes());
    assert!(!o.stdout.is_empty(), "Claude Code card shown");
    e.reprompt_run("c1", 4);
    e.agent_stop("c1");
    e.wait_for("2 stop");
    assert_eq!(e.statusline(Some("c1")), "");
    assert!(e.status().contains("1 hold (quiet_period)"), "{}", e.status());
}

#[test]
fn e2e_copilot_paused_nothing_captured() {
    let mut e = Env::new();
    e.start_core();
    assert!(e.run(&["pause"], b"").status.success());
    for t in TEXTS {
        e.prompt("c1", t);
    }
    e.agent_stop("c1");
    assert_eq!(e.statusline(Some("c1")), "");
    assert!(e.status().contains("Events stored: none"));
}

#[test]
fn e2e_copilot_core_down_hook_and_statusline_exit_zero_silently() {
    let e = Env::new();
    let o = e.run(&["statusline", "copilot"], br#"{"session":{"id":"c1"}}"#);
    assert!(o.status.success() && o.stdout.is_empty());
    e.agent_stop("c1"); // agentStop never starts the core
    assert!(e.status().contains("not running"));
    let spool = std::fs::read_to_string(e.home().join("spool.jsonl")).unwrap();
    assert_eq!(spool.lines().count(), 1);
}

#[test]
fn e2e_copilot_hook_drops_claude_shaped_and_internal_runs() {
    let mut e = Env::new();
    e.start_core();
    let claude = json!({"session_id": "s", "hook_event_name": "UserPromptSubmit", "prompt": "hi"});
    e.hook("userPromptSubmitted", claude);
    let mut c = e.cmd(&["hook", "copilot", "userPromptSubmitted"]);
    c.env("AIQ_INTERNAL", "1");
    let o = e.run_with(c, json!({"sessionId": "j", "prompt": "judge this"}).to_string().as_bytes());
    assert!(o.status.success() && o.stdout.is_empty());
    let st = e.status();
    assert!(st.contains("AIQ-HOOK-FOREIGN-HOST 1") && st.contains("Events stored: none"), "{st}");
}

#[test]
fn e2e_copilot_random_bytes_never_print() {
    let e = Env::new();
    for bytes in [&b""[..], b"{", b"[1]", b"\x00\xff\xfe", br#"{"sessionId": 5}"#] {
        for ev in ["userPromptSubmitted", "agentStop"] {
            let o = e.run(&["hook", "copilot", ev], bytes);
            assert!(o.status.success() && o.stdout.is_empty() && o.stderr.is_empty());
        }
        let o = e.run(&["statusline", "copilot"], bytes);
        assert!(o.status.success() && o.stdout.is_empty());
    }
}

#[test]
fn e2e_install_copilot_round_trip_keeps_other_settings() {
    let e = Env::new();
    std::fs::create_dir_all(e.copilot().join("hooks")).unwrap();
    let mine = json!({"version": 1, "hooks": {"preToolUse": [{"type": "command", "exec": "mine"}]}});
    std::fs::write(e.copilot().join("hooks").join("mine.json"), mine.to_string()).unwrap();
    let original = "{\n  \"model\": \"x\",\n  \"experimental\": true\n}\n";
    std::fs::write(e.copilot().join("settings.json"), original).unwrap();
    let o = e.run(&["install", "copilot", "--yes"], b"");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let hooks: Value =
        serde_json::from_str(&std::fs::read_to_string(e.copilot().join("hooks").join("aiq.json")).unwrap()).unwrap();
    assert_eq!(hooks["hooks"]["agentStop"][0]["args"], json!(["hook", "copilot", "agentStop"]));
    let s: Value = serde_json::from_str(&std::fs::read_to_string(e.copilot().join("settings.json")).unwrap()).unwrap();
    let cmd = s["statusLine"]["command"].as_str().unwrap();
    let wrapper = e.copilot().join("aiq-statusline.cmd");
    if cfg!(windows) {
        // The shape laptop probe run 2 proved: a bare path to a one-line .cmd wrapper.
        assert_eq!(cmd, wrapper.to_string_lossy());
        let w = std::fs::read_to_string(&wrapper).unwrap();
        assert!(w.starts_with("@\"") && w.ends_with("\" statusline copilot\r\n"), "{w}");
        let line = Command::new("cmd").args(["/d", "/c", cmd]).env("AIQ_HOME", e.home()).output().unwrap();
        assert!(line.status.success() && line.stderr.is_empty(), "the wrapper runs aiq");
    } else {
        assert!(cmd.ends_with(" statusline copilot") && !wrapper.exists());
    }
    assert_eq!(s["experimental"], true);
    assert_eq!(s["feature_flags"]["enabled"], json!(["STATUS_LINE"]));
    assert_eq!(s["model"], "x");
    let out = String::from_utf8(o.stdout).unwrap();
    assert!(out.contains("Self-test: AIQ's Copilot hook ran in"), "{out}");
    assert!(e.run(&["uninstall", "copilot", "--yes"], b"").status.success());
    assert!(!e.copilot().join("hooks").join("aiq.json").exists());
    assert!(e.copilot().join("hooks").join("mine.json").exists(), "other hook files untouched");
    assert!(!wrapper.exists());
    let after: Value =
        serde_json::from_str(&std::fs::read_to_string(e.copilot().join("settings.json")).unwrap()).unwrap();
    assert_eq!(after, serde_json::from_str::<Value>(original).unwrap());
}

#[test]
fn e2e_install_copilot_reverts_only_what_it_turned_on() {
    let e = Env::new();
    std::fs::create_dir_all(e.copilot()).unwrap();
    let original = json!({"experimental": false, "feature_flags": {"enabled": ["OTHER"]}});
    std::fs::write(e.copilot().join("settings.json"), original.to_string()).unwrap();
    assert!(e.run(&["install", "copilot", "--yes"], b"").status.success());
    assert!(e.run(&["install", "copilot", "--yes"], b"").status.success(), "reinstall keeps the first record");
    let s: Value = serde_json::from_str(&std::fs::read_to_string(e.copilot().join("settings.json")).unwrap()).unwrap();
    assert_eq!((&s["experimental"], &s["feature_flags"]["enabled"]), (&json!(true), &json!(["OTHER", "STATUS_LINE"])));
    assert!(e.run(&["uninstall", "copilot", "--yes"], b"").status.success());
    let after: Value =
        serde_json::from_str(&std::fs::read_to_string(e.copilot().join("settings.json")).unwrap()).unwrap();
    assert_eq!(after, original);
}

#[test]
fn e2e_install_copilot_no_status_line_installs_hooks_only_and_removes_an_earlier_one() {
    let e = Env::new();
    std::fs::create_dir_all(e.copilot()).unwrap();
    let original = json!({"model": "x"});
    std::fs::write(e.copilot().join("settings.json"), original.to_string()).unwrap();
    let o = e.run(&["install", "copilot", "--yes", "--no-status-line"], b"");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8(o.stdout).unwrap();
    assert!(out.contains("No status line (--no-status-line)"), "{out}");
    assert!(e.copilot().join("hooks").join("aiq.json").exists());
    let read = || serde_json::from_str::<Value>(&std::fs::read_to_string(e.copilot().join("settings.json")).unwrap());
    assert_eq!(read().unwrap(), original, "settings untouched: no statusLine, no experimental");
    assert!(!e.copilot().join("aiq-statusline.cmd").exists());
    // Switching off later: a full install, then --no-status-line, puts the settings back as they were.
    assert!(e.run(&["install", "copilot", "--yes"], b"").status.success());
    assert!(read().unwrap().get("statusLine").is_some());
    assert!(e.run(&["install", "copilot", "--yes", "--no-status-line"], b"").status.success());
    assert_eq!(read().unwrap(), original);
    assert!(!e.copilot().join("aiq-statusline.cmd").exists());
    assert!(e.copilot().join("hooks").join("aiq.json").exists(), "hooks stay");
    let again = e.run(&["install", "copilot", "--yes", "--no-status-line"], b"");
    assert!(String::from_utf8(again.stdout).unwrap().contains("Already installed."));
}

#[test]
fn e2e_install_copilot_never_replaces_a_foreign_status_line() {
    let e = Env::new();
    std::fs::create_dir_all(e.copilot()).unwrap();
    let original = json!({"statusLine": {"type": "command", "command": "~/.copilot/mine.sh"}}).to_string();
    std::fs::write(e.copilot().join("settings.json"), &original).unwrap();
    let o = e.run(&["install", "copilot", "--yes"], b"");
    let out = String::from_utf8(o.stdout).unwrap();
    assert!(out.contains("AIQ will not replace it") && out.contains("statusline copilot"), "{out}");
    assert_eq!(std::fs::read_to_string(e.copilot().join("settings.json")).unwrap(), original);
    assert!(e.copilot().join("hooks").join("aiq.json").exists(), "hooks still installed");
}

#[test]
fn e2e_install_copilot_invalid_settings_refused_untouched() {
    let e = Env::new();
    std::fs::create_dir_all(e.copilot()).unwrap();
    std::fs::write(e.copilot().join("settings.json"), "{ not json").unwrap();
    let o = e.run(&["install", "copilot", "--yes"], b"");
    assert!(!o.status.success());
    assert_eq!(std::fs::read_to_string(e.copilot().join("settings.json")).unwrap(), "{ not json");
    assert!(!e.copilot().join("hooks").join("aiq.json").exists());
}
