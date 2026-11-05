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
    /// With a PATH that holds no `claude`, so a Claude Code installed on the machine running the tests
    /// doesn't count as found (tool detection reads PATH).
    fn run_clean(&self, args: &[&str]) -> Output {
        let bin = self.dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let mut c = self.cmd(args);
        c.env("PATH", &bin);
        self.run_with(c, b"")
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
        assert!(o.status.success() && o.stderr.is_empty(), "Copilot hooks never write stderr");
        assert!(event == "agentStop" || o.stdout.is_empty(), "only agentStop may print (the tip)");
        o
    }
    fn prompt(&self, session: &str, text: &str) {
        let ts = chrono_ms();
        self.hook(
            "userPromptSubmitted",
            json!({"sessionId": session, "timestamp": ts, "cwd": "/repo", "prompt": text}),
        );
    }
    /// The agentStop hook's stdout: empty, or the one progress line with the tip.
    fn agent_stop(&self, session: &str) -> String {
        let ts = chrono_ms();
        let o = self.hook(
            "agentStop",
            json!({"sessionId": session, "timestamp": ts, "stopReason": "end_turn", "transcriptPath": "/t"}),
        );
        String::from_utf8(o.stdout).unwrap()
    }
    /// At a turn end the core offers a ready card for 30 ms (STOP_WAIT_MS) and the hook waits 55 ms. On a
    /// loaded runner the hook can miss that window and print nothing; the card then stays show-ready for the
    /// next turn end, as in the product (e2e_concurrent_stops_deliver_once). Measured 2026-11-03 on a loaded
    /// 4-core runner: 16 of 150 suite runs missed the first turn end, hook outcome `no-card` 13 and
    /// `stop-timeout` 3. When the hook gave up after the core handed the card over, the card is held for that
    /// hook's confirmation for HANDOVER_CONFIRM_MS, then ready again (DC-022). So end the turn until the card
    /// prints, for up to twice that hold; every miss must keep the card show-ready, so a lost card still fails.
    /// Returns the hook's stdout and how many turn ends it took.
    fn stop_until_card(&self, stop: impl Fn() -> String) -> (String, usize) {
        let hold = Duration::from_millis(aiq_core::service::HANDOVER_CONFIRM_MS as u64);
        let deadline = Instant::now() + 2 * hold;
        let mut n = 0;
        while Instant::now() < deadline {
            n += 1;
            let out = stop();
            if !out.is_empty() {
                return (out, n);
            }
            let st = self.status();
            let timings = std::fs::read_to_string(self.home().join("hook-timings.log")).unwrap_or_default();
            assert!(st.contains("show-ready"), "a turn end printed nothing and the card is gone:\n{st}\n{timings}");
            std::thread::sleep(Duration::from_millis(250));
        }
        panic!("no card in {n} turn ends over {:?}: {}", 2 * hold, self.status());
    }
    /// The status line where a tip is expected. Copilot refreshes it about once a second, and one refresh may
    /// miss the 80 ms budget on a loaded runner (Windows CI, 2026-11-03), so look as Copilot does: a few
    /// refreshes over up to 3 s.
    fn tip_line(&self, session: &str, colour: bool) -> String {
        let read = || {
            let mut c = self.cmd(&["statusline", "copilot"]);
            if colour {
                c.env_remove("NO_COLOR");
            }
            let p = json!({"session": {"id": session}, "cwd": "/repo"});
            String::from_utf8(self.run_with(c, p.to_string().as_bytes()).stdout).unwrap()
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut line = read();
        while line.is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
            line = read();
        }
        line
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
    let (out, stops) = e.stop_until_card(|| e.agent_stop("c1"));
    assert_eq!(out.lines().count(), 1, "one progress line: {out}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 2, "only type and message: {out}");
    assert_eq!(v["type"], "progress");
    let msg = v["message"].as_str().unwrap();
    assert!(msg.starts_with("AIQ tip: this request went round 3 times") && msg.contains("Try this: "), "{msg}");
    for t in TEXTS {
        assert!(!msg.contains(t), "the tip never quotes prompt text");
    }
    e.wait_for(&format!("{stops} stop"));
    e.wait_for("Tips today: 1 of"); // the hook's "printed" note is its own send, after the turn end (DC-022)
    let line = e.tip_line("c1", false);
    assert!(
        line.starts_with("AIQ tip: this request went round 3 times. The agent may need one new detail · aiq show "),
        "{line}"
    );
    assert_eq!(line.lines().count(), 1);
    for t in TEXTS {
        assert!(!line.contains(t), "the tip never quotes prompt text");
    }
    assert_eq!(e.tip_line("c1", false), line, "refresh keeps it");
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
    let (_, stops) = e.stop_until_card(|| e.agent_stop("c1"));
    e.wait_for(&format!("{stops} stop"));
    e.wait_for("Tips today: 1 of"); // the hook's "printed" note is its own send, after the turn end (DC-022)
    let line = e.tip_line("c1", false);
    e.prompt("c1", "next: write the changelog entry");
    e.wait_for("5 prompt");
    assert_eq!(e.tip_line("c1", false), line);
    let id = line.trim().rsplit("aiq show ").next().unwrap();
    assert!(String::from_utf8(e.run(&["show", id], b"").stdout).unwrap().contains(TEXTS[0]));
    assert_eq!(e.statusline(Some("c1")), "", "opened: gone");
}

#[test]
fn e2e_copilot_tip_in_chat_off_leaves_it_to_the_status_line() {
    let mut e = Env::new();
    e.start_core();
    assert!(e.run(&["config", "set", "copilot-tip-in-chat", "off"], b"").status.success());
    assert!(!e.run(&["config", "set", "copilot-tip-in-chat", "maybe"], b"").status.success());
    e.reprompt_run("c1", 0);
    assert_eq!(e.agent_stop("c1"), "", "off: nothing in the conversation");
    e.wait_for("1 stop");
    let line = e.tip_line("c1", false);
    let timings = std::fs::read_to_string(e.home().join("hook-timings.log")).unwrap_or_default();
    assert!(
        line.starts_with("AIQ tip: this request went round"),
        "the status line still shows it: {line:?}\n{timings}"
    );
}

#[test]
fn e2e_tip_highlight_off_renders_plain_even_with_colour_allowed() {
    let mut e = Env::new();
    e.start_core();
    assert!(e.run(&["config", "set", "tip-highlight", "off"], b"").status.success());
    e.reprompt_run("c1", 0);
    let (out, stops) = e.stop_until_card(|| {
        let mut c = e.cmd(&["hook", "copilot", "agentStop"]);
        c.env_remove("NO_COLOR");
        let o = e.run_with(c, json!({"sessionId": "c1", "stopReason": "end_turn"}).to_string().as_bytes());
        String::from_utf8(o.stdout).unwrap()
    });
    assert!(out.contains("AIQ tip:") && !out.contains("\\u001b") && !out.contains('\u{1b}'), "{out}");
    e.wait_for(&format!("{stops} stop"));
    e.wait_for("Tips today: 1 of"); // the hook's "printed" note is its own send, after the turn end (DC-022)
    let line = e.tip_line("c1", true);
    assert!(line.starts_with("AIQ tip:") && !line.contains('\u{1b}'), "{line}");
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
    let (_, stops) = e.stop_until_card(|| {
        String::from_utf8(e.run(&["hook", "claude-code", "Stop"], stop.to_string().as_bytes()).stdout).unwrap()
    });
    e.reprompt_run("c1", 4);
    e.agent_stop("c1");
    e.wait_for(&format!("{} stop", stops + 1));
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

#[test]
fn e2e_install_with_no_tool_connects_every_tool_found_and_names_the_undo() {
    let e = Env::new();
    let claude = e.home().parent().unwrap().join("claude");
    // Nothing found: says so and changes nothing.
    let o = e.run_clean(&["install", "--yes"]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("No supported AI tool found"));
    assert!(!String::from_utf8_lossy(&o.stdout).contains("Analysis"), "nothing connected: analysis untouched");
    assert!(!e.copilot().exists() && !claude.exists());

    // A bare Claude Code folder (other tools and people make one) is not Claude Code: nothing written to it.
    std::fs::create_dir_all(claude.join("skills")).unwrap();
    let o = e.run_clean(&["install", "--yes"]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("No supported AI tool found"));
    assert!(!claude.join("settings.json").exists(), "no settings created for a tool that isn't there");

    // Both tools present (Copilot's folder; Claude Code's own transcripts): both get AIQ's hooks.
    std::fs::create_dir_all(e.copilot()).unwrap();
    std::fs::create_dir_all(claude.join("projects").join("C--repos-a")).unwrap();
    let o = e.run_clean(&["install", "--yes"]);
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(o.status.success(), "{out}\n{}", String::from_utf8_lossy(&o.stderr));
    assert!(e.copilot().join("hooks").join("aiq.json").exists());
    let s: Value = serde_json::from_str(&std::fs::read_to_string(claude.join("settings.json")).unwrap()).unwrap();
    assert!(aiq_core::tools::claude_has_aiq(&s));
    assert!(out.contains("Undo: aiq uninstall claude-code") && out.contains("Undo: aiq uninstall copilot"), "{out}");
    // Copilot's experimental mode was off, so connect-all wrote only AIQ's own hook file there.
    assert!(!e.copilot().join("settings.json").exists(), "Copilot settings untouched");
    assert!(!e.copilot().join("aiq-statusline.cmd").exists());
    assert!(out.contains("To add the status line, run: aiq install copilot"), "{out}");
    let changed: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix("Changed: ")).collect();
    let want = [claude.join("settings.json"), e.copilot().join("hooks").join("aiq.json")];
    assert_eq!(changed, want.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>(), "{out}");
    // A tool is connected, so background judging is on, with what is sent and how to undo it.
    assert!(out.contains("Analysis: turned on.") && out.contains("What is sent:"), "{out}");
    assert!(out.contains("up to 500 new tasks a day") && out.contains("Undo: aiq config set analysis off"), "{out}");

    // Run again: nothing left to connect.
    let again = String::from_utf8_lossy(&e.run_clean(&["install", "--yes"]).stdout).to_string();
    assert!(
        again.contains("Claude Code is already connected.") && again.contains("GitHub Copilot is already connected."),
        "{again}"
    );
    assert!(!again.contains("Undo:"), "{again}");
    assert!(again.contains("Analysis is already on"), "{again}");

    // The person turns judging off: a later install keeps it off. Written to the store directly, because
    // `aiq config set` starts a core this test would not stop (on Windows it then locks aiq.exe).
    let store = aiq_store::Store::open(&e.home().join("aiq.db")).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    store.set_setting("judge", "off", now + 1_000).unwrap();
    drop(store);
    let off = String::from_utf8_lossy(&e.run_clean(&["install", "--yes"]).stdout).to_string();
    assert!(off.contains("Analysis stays off, as you set it."), "{off}");
}

/// After setup, a tool installed later is connected once by `aiq install --new` (AIQ home and the core run it
/// at start); one the person disconnected stays disconnected. Hooks AIQ 0.1.4 wrote for a Claude Code that
/// isn't installed are reported as such, not as connected.
#[test]
fn e2e_install_new_connects_a_tool_installed_after_setup_once() {
    let e = Env::new();
    let claude = e.home().parent().unwrap().join("claude");
    std::fs::create_dir_all(e.copilot()).unwrap();
    // Before setup is done, --new does nothing (setup connects).
    assert!(e.run_clean(&["install", "--new", "--yes"]).status.success());
    assert!(!e.copilot().join("hooks").join("aiq.json").exists());

    assert!(e.run_clean(&["install", "--yes"]).status.success(), "setup connects Copilot");
    std::fs::write(e.home().join("setup-done"), "done\n").unwrap();
    assert!(e.run_clean(&["uninstall", "copilot", "--yes"]).status.success(), "the person disconnects Copilot");
    let o = e.run_clean(&["install", "--new", "--yes"]);
    assert!(o.stdout.is_empty(), "nothing new: {}", String::from_utf8_lossy(&o.stdout));
    assert!(!e.copilot().join("hooks").join("aiq.json").exists(), "a disconnected tool stays disconnected");

    // Claude Code installed after setup: connected once, and only it.
    std::fs::create_dir_all(claude.join("projects").join("C--repos-a")).unwrap();
    let out = String::from_utf8_lossy(&e.run_clean(&["install", "--new", "--yes"]).stdout).to_string();
    let s: Value = serde_json::from_str(&std::fs::read_to_string(claude.join("settings.json")).unwrap()).unwrap();
    assert!(aiq_core::tools::claude_has_aiq(&s), "{out}");
    assert!(out.contains(&format!("Changed: {}", claude.join("settings.json").display())), "{out}");
    assert!(!e.copilot().join("hooks").join("aiq.json").exists(), "{out}");

    // The person disconnects it: the next start leaves it alone.
    assert!(e.run_clean(&["uninstall", "claude-code", "--yes"]).status.success());
    assert!(e.run_clean(&["install", "--new", "--yes"]).stdout.is_empty());
    assert!(!claude.join("settings.json").exists());
}

#[test]
fn e2e_hooks_left_in_a_claude_folder_without_claude_code_are_not_called_connected() {
    let e = Env::new();
    let claude = e.home().parent().unwrap().join("claude");
    std::fs::create_dir_all(&claude).unwrap();
    // What AIQ 0.1.4's setup wrote on a computer with only a bare folder.
    let v =
        json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"aiq","args":["hook","claude-code","Stop"]}]}]}});
    std::fs::write(claude.join("settings.json"), v.to_string()).unwrap();
    let out = String::from_utf8_lossy(&e.run_clean(&["install", "--yes"]).stdout).to_string();
    assert!(!out.contains("Claude Code is already connected"), "{out}");
    assert!(
        out.contains("Claude Code isn't installed on this computer") && out.contains("aiq uninstall claude-code"),
        "{out}"
    );
}

#[test]
fn e2e_uninstall_all_puts_every_tool_back_and_keeps_the_data() {
    let e = Env::new();
    let claude = e.home().parent().unwrap().join("claude");
    std::fs::create_dir_all(e.copilot()).unwrap();
    std::fs::create_dir_all(claude.join("projects").join("C--repos-a")).unwrap();
    let mine = r#"{"model":"opus"}"#;
    std::fs::write(claude.join("settings.json"), mine).unwrap();
    assert!(e.run_clean(&["install", "--yes"]).status.success());
    std::fs::create_dir_all(e.home()).unwrap();
    std::fs::write(e.home().join("keep.txt"), "data").unwrap();

    let o = e.run(&["uninstall", "--all", "--yes"], b"");
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(o.status.success(), "{out}\n{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read_to_string(claude.join("settings.json")).unwrap(), mine, "Claude settings byte for byte");
    assert!(!e.copilot().join("hooks").join("aiq.json").exists());
    let changed: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix("Changed: ")).collect();
    let want = [claude.join("settings.json"), e.copilot().join("hooks").join("aiq.json")];
    assert_eq!(changed, want.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>(), "{out}");
    assert!(out.contains("Your data stays in") && out.contains("aiq forget --all"), "{out}");
    assert!(e.home().join("keep.txt").exists(), "data kept");

    // Again: nothing connected, nothing changed.
    let again = String::from_utf8_lossy(&e.run(&["uninstall", "--all", "--yes"], b"").stdout).to_string();
    assert!(again.contains("Nothing changed."), "{again}");
}

#[test]
fn e2e_install_with_no_tool_adds_the_status_line_only_when_experimental_is_already_on() {
    let e = Env::new();
    std::fs::create_dir_all(e.copilot()).unwrap();
    let original = json!({"experimental": true, "feature_flags": {"enabled": ["OTHER"]}});
    std::fs::write(e.copilot().join("settings.json"), original.to_string()).unwrap();
    let o = e.run(&["install", "--yes"], b"");
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(o.status.success(), "{out}");
    let s: Value = serde_json::from_str(&std::fs::read_to_string(e.copilot().join("settings.json")).unwrap()).unwrap();
    assert!(s["statusLine"]["command"].as_str().unwrap().contains("statusline"));
    assert_eq!(s["feature_flags"]["enabled"], json!(["OTHER", "STATUS_LINE"]), "existing flags kept, one added");
    assert_eq!(s["experimental"], true);
    assert!(out.lines().any(|l| l == format!("Changed: {}", e.copilot().join("settings.json").display())), "{out}");
    assert!(e.run(&["uninstall", "copilot", "--yes"], b"").status.success());
    let after: Value =
        serde_json::from_str(&std::fs::read_to_string(e.copilot().join("settings.json")).unwrap()).unwrap();
    assert_eq!(after, original, "uninstall puts back exactly what was there");
}
