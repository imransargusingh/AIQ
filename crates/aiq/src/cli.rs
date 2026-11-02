//! CLI commands. Output is plain text, no colour, copyable commands (design: UI and interaction design).
use aiq_contracts::frames::{OkReply, Request, ShowReply, StatusReply};
use aiq_core::control::ControlError;
use aiq_core::paths::Paths;
use aiq_core::server;
use std::io::{BufRead, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

const CLI_TIMEOUT: Duration = Duration::from_secs(5);
const NOT_RUNNING: &str = "AIQ is not running. It starts with your next Claude Code prompt, or run `aiq core`.";

pub fn core() -> ExitCode {
    let paths = Paths::from_env();
    crate::judge::start_background(paths.clone());
    match server::run(&paths) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.code == aiq_contracts::codes::CORE_ALREADY_RUNNING => {
            eprintln!("AIQ core is already running ({}).", e.code);
            ExitCode::from(3)
        }
        Err(e) => {
            eprintln!("AIQ core could not start ({}): {}", e.code, e.detail);
            ExitCode::from(1)
        }
    }
}

fn ask_core(paths: &Paths, req: impl FnOnce(String) -> Request) -> Option<String> {
    aiq_core::control::ask(paths, CLI_TIMEOUT, req)
}

/// Reach the core, starting it if needed (CLI only; never on the hook path).
fn ensure_core(paths: &Paths) -> bool {
    if ask_core(paths, |token| Request::Status { token }).is_some() {
        return true;
    }
    let _ = paths.ensure_dirs();
    if crate::spawn::core_detached().is_err() {
        return false;
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if ask_core(paths, |token| Request::Status { token }).is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn core_error(paths: &Paths) -> Option<String> {
    std::fs::read_to_string(paths.core_error()).ok().map(|s| s.trim().to_owned())
}

pub fn status() -> ExitCode {
    let paths = Paths::from_env();
    let reply =
        ask_core(&paths, |token| Request::Status { token }).and_then(|l| serde_json::from_str::<StatusReply>(&l).ok());
    let mut out = String::new();
    match &reply {
        Some(s) => {
            let events: u64 = s.events_by_kind.iter().map(|(_, n)| n).sum();
            if s.paused {
                out += "AIQ is paused. Nothing is captured and no tips show. Resume with: aiq resume\n";
            } else if events == 0 {
                out += "No activity yet. Use Claude Code or Copilot as usual; AIQ watches quietly.\n";
            } else {
                out += "AIQ is running.\n";
            }
            out += &format!("Tips today: {} of {}\n", s.cards_today, s.daily_cap);
            let kinds: Vec<String> = s.events_by_kind.iter().map(|(k, n)| format!("{n} {k}")).collect();
            out += &format!("Events stored: {}\n", if kinds.is_empty() { "none".into() } else { kinds.join(", ") });
            let dec: Vec<String> =
                s.decisions.iter().map(|(o, r, n)| format!("{n} {} ({r})", o.replace('_', "-"))).collect();
            out += &format!(
                "Signals: {}. Cards: {}\n",
                s.signals,
                if dec.is_empty() { "none".into() } else { dec.join(", ") }
            );
            out += &format!("Store size: {}\n", human_bytes(s.store_bytes));
            let ignored = s.core_counters.iter().find(|(k, _)| k == aiq_contracts::codes::INGEST_AUTOMATED);
            for (folder, reason) in &s.automated_folders {
                if reason == "projects" {
                    out += &format!(
                        "Sessions Claude ran for you through claude.ai Projects or Remote Control are ignored in \
                         {folder}. To count them: aiq include \"{folder}\"\n"
                    );
                    continue;
                }
                let why = if reason == "burst" {
                    "many sessions started at once"
                } else {
                    "the same first prompt over and over"
                };
                out += &format!(
                    "Automated sessions ignored in {folder} ({why}). If that is your own work: aiq include \"{folder}\"\n"
                );
            }
            if let Some((_, n)) = ignored {
                out += &format!("Automated prompts ignored since the core started: {n}\n");
            }
            if let Some((_, n)) = s.core_counters.iter().find(|(k, _)| k == aiq_contracts::codes::INGEST_SECRET_REMOVED)
            {
                out += &format!("Secrets removed from prompts before storing, since the core started: {n}\n");
            }
            let not_errors = [aiq_contracts::codes::INGEST_AUTOMATED, aiq_contracts::codes::INGEST_SECRET_REMOVED];
            if s.core_counters.iter().any(|(k, _)| !not_errors.contains(&k.as_str())) {
                let c: Vec<String> = s
                    .core_counters
                    .iter()
                    .filter(|(k, _)| !not_errors.contains(&k.as_str()))
                    .map(|(k, n)| format!("{k} {n}"))
                    .collect();
                out += &format!("Core errors: {}\n", c.join(", "));
                if s.core_counters.iter().any(|(k, _)| k == aiq_contracts::codes::API_AUTH) {
                    out += &format!(
                        "Some hooks could not talk to AIQ: their key did not match the running core's. \
                         This happens when two AIQ homes exist, for example one under the Claude desktop \
                         app's private AppData. This AIQ's home: {}\n",
                        paths.data.display()
                    );
                }
            }
        }
        None => {
            out += NOT_RUNNING;
            out += "\n";
            if let Some(e) = core_error(&paths) {
                out += &format!("Last start failed: {e}\n");
            }
        }
    }
    if let Some(l) = crate::import::status_line(&paths) {
        out += &l;
    }
    out += &format!("Claude Code: {}\n", crate::install::status_line());
    out += &format!("Copilot: {}\n", crate::install_copilot::status_line());
    let spooled = std::fs::read_to_string(paths.spool()).map(|s| s.lines().count()).unwrap_or(0);
    if spooled > 0 {
        out += &format!("Queued while the core was down: {spooled} events\n");
    }
    out += &hook_time_lines(&paths);
    let errs = hook_error_counts(&paths);
    if !errs.is_empty() {
        out += &format!("Hook notes: {}\n", errs.join(", "));
    }
    print!("{out}");
    ExitCode::SUCCESS
}

fn human_bytes(b: u64) -> String {
    match b {
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.0} KB", b as f64 / (1 << 10) as f64),
        b => format!("{b} bytes"),
    }
}

/// p50 / p95 in-process hook time per event, from the hook's own timing log.
fn hook_time_lines(paths: &Paths) -> String {
    let text = std::fs::read_to_string(paths.hook_timings()).unwrap_or_default();
    let mut by: std::collections::BTreeMap<&str, Vec<u64>> = Default::default();
    let mut host: Vec<u64> = Vec::new();
    for l in text.lines() {
        let mut it = l.split_whitespace();
        if let (Some(ev), Some(us)) = (it.next(), it.next().and_then(|u| u.parse::<u64>().ok())) {
            by.entry(ev).or_default().push(us);
        }
        if let Some(ms) =
            l.split_whitespace().find_map(|w| w.strip_prefix("host_ms=")).and_then(|m| m.parse::<u64>().ok())
        {
            host.push(ms * 1000);
        }
    }
    fn pct(v: &mut [u64]) -> (f64, f64) {
        v.sort_unstable();
        let p = |q: f64| v[((v.len() as f64 * q).ceil() as usize).saturating_sub(1).min(v.len() - 1)] as f64 / 1000.0;
        (p(0.5), p(0.95))
    }
    let mut out: String = by
        .into_iter()
        .map(|(ev, mut v)| {
            let (p50, p95) = pct(&mut v);
            format!("Hook time in-process, {ev}: p50 {p50:.1} ms, p95 {p95:.1} ms ({} runs)\n", v.len())
        })
        .collect();
    if !host.is_empty() {
        let (p50, p95) = pct(&mut host);
        out += &format!(
            "Copilot's own start-up before AIQ's hook runs: p50 {p50:.0} ms, p95 {p95:.0} ms ({} runs; not counted \
             against AIQ's budget)\n",
            host.len()
        );
    }
    out
}

fn hook_error_counts(paths: &Paths) -> Vec<String> {
    let text = std::fs::read_to_string(paths.hook_errors()).unwrap_or_default();
    let mut by: std::collections::BTreeMap<&str, u64> = Default::default();
    for l in text.lines().filter(|l| !l.is_empty()) {
        *by.entry(l.trim()).or_default() += 1;
    }
    by.into_iter().map(|(k, n)| format!("{k} {n}")).collect()
}

/// Strip terminal control sequences before printing stored prompt text (STRIDE B2).
pub fn sanitize(s: &str) -> String {
    s.chars().filter(|c| *c == '\n' || *c == '\t' || !c.is_control()).collect()
}

pub fn show(card_id: &str) -> ExitCode {
    let paths = Paths::from_env();
    if !ensure_core(&paths) {
        println!("AIQ can't open its data right now. Run `aiq status`.");
        return ExitCode::from(1);
    }
    let Some(r) =
        ask_core(&paths, |token| Request::Show { token, card_id: card_id.to_owned(), opened_on: Some("cli".into()) })
            .and_then(|l| serde_json::from_str::<ShowReply>(&l).ok())
    else {
        println!("AIQ can't open its data right now. Run `aiq status`.");
        return ExitCode::from(1);
    };
    print!("{}", render_show(&r));
    if r.card.is_none() { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

pub fn render_show(r: &ShowReply) -> String {
    let Some(c) = &r.card else { return "No card with that id.\n".into() };
    let mut out = format!("AIQ tip: {}\nWhy: {}\nTry this: {}\n\n", c.noticed, c.why, c.try_this);
    let status =
        if r.delivered { "shown".to_string() } else { r.outcome.clone().unwrap_or_default().replace('_', "-") };
    out += &format!("Status: {status} (reason: {})\n", r.reason_code.clone().unwrap_or_default());
    if r.evidence.iter().all(|e| e.text.is_none()) {
        out += "The evidence for this card was deleted.\n";
        return out;
    }
    out += "The prompts it counted:\n";
    for (i, e) in r.evidence.iter().enumerate() {
        let text = e.text.as_deref().map(sanitize).unwrap_or_else(|| "(deleted)".into());
        out += &format!("  {}. {}  {}\n", i + 1, e.ts, text.replace('\n', "\n     "));
    }
    out
}

pub fn pause(on: bool) -> ExitCode {
    let paths = Paths::from_env();
    if let Err(e) = aiq_core::control::set_paused(&paths, on, CLI_TIMEOUT) {
        eprintln!("Could not change the pause setting: {e}");
        return ExitCode::from(1);
    }
    println!(
        "{}",
        if on {
            "AIQ is paused. Nothing is captured and no tips show. Resume with: aiq resume"
        } else {
            "AIQ resumed."
        }
    );
    ExitCode::SUCCESS
}

/// `aiq include <folder>`: undo AIQ's automated-session call for a folder ("Include them").
pub fn include_folder(folder: &str) -> ExitCode {
    let paths = Paths::from_env();
    if !ensure_core(&paths) {
        println!("{NOT_RUNNING}");
        return ExitCode::from(1);
    }
    let r = ask_core(&paths, |token| Request::IncludeFolder { token, folder: folder.into() })
        .and_then(|l| serde_json::from_str::<OkReply>(&l).ok());
    match r {
        Some(OkReply { ok: true, .. }) => {
            println!("AIQ will watch sessions in {folder} again.");
            ExitCode::SUCCESS
        }
        Some(OkReply { message, .. }) => {
            println!("Not changed: {}", message.unwrap_or_default());
            ExitCode::from(2)
        }
        None => {
            println!("{NOT_RUNNING}");
            ExitCode::from(1)
        }
    }
}

/// `aiq config set <name> <value>`: `daily-cap` or `copilot-tip-stays`.
pub fn config_set(name: &str, value: &str) -> ExitCode {
    let (key, done) = match name {
        "daily-cap" => ("daily_cap", format!("Daily tip limit set to {value}.")),
        "copilot-tip-stays" => ("copilot_tip_stays", format!("Copilot CLI tip stays: {value}.")),
        "judge" => ("judge", format!("Background judging: {value}.")),
        "judge-daily-tasks" => ("judge_daily_tasks", format!("Most tasks judged per day: {value}.")),
        "judge-claude-model" => ("judge_claude_model", format!("Claude judge model: {value}.")),
        "judge-copilot-model" => ("judge_copilot_model", format!("Copilot judge model: {value}.")),
        "judge-copilot-sessions" => ("judge_copilot_sessions", format!("Copilot judge sessions: {value}.")),
        _ => {
            println!(
                "Unknown setting {name}. Settings: daily-cap, copilot-tip-stays, judge, judge-daily-tasks, \
                 judge-claude-model, judge-copilot-model, judge-copilot-sessions."
            );
            return ExitCode::from(2);
        }
    };
    let paths = Paths::from_env();
    if !ensure_core(&paths) {
        println!("{NOT_RUNNING}");
        return ExitCode::from(1);
    }
    match aiq_core::control::set_setting(&paths, key, value, CLI_TIMEOUT) {
        Ok(()) => {
            println!("{done}");
            ExitCode::SUCCESS
        }
        Err(ControlError::Refused(message)) => {
            println!("Not changed: {message}");
            ExitCode::from(2)
        }
        Err(_) => {
            println!("{NOT_RUNNING}");
            ExitCode::from(1)
        }
    }
}

pub fn confirm(question: &str, yes: bool) -> bool {
    if yes {
        return true;
    }
    print!("{question} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    matches!(line.trim(), "y" | "Y" | "yes" | "Yes")
}

pub fn forget_all(yes: bool) -> ExitCode {
    let paths = Paths::from_env();
    if !confirm("Delete every prompt, signal and card AIQ stored on this machine? This cannot be undone.", yes) {
        println!("Nothing deleted.");
        return ExitCode::SUCCESS;
    }
    let r = aiq_core::control::forget_all(&paths, CLI_TIMEOUT).map_err(|e| match e {
        ControlError::Refused(m) | ControlError::Io(m) => m,
        ControlError::NotRunning => NOT_RUNNING.to_owned(),
    });
    match r {
        Ok(()) => {
            let n = crate::judge::forget_judge_sessions(&paths, &crate::judge::Homes::from_env());
            if n > 0 {
                println!("Also deleted {n} Copilot session(s) AIQ's judge had left.");
            }
            println!("Deleted. AIQ starts fresh with your next Claude Code prompt.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("Could not delete everything: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aiq_contracts::frames::EvidencePrompt;

    #[test]
    fn sanitize_strips_ansi_escape_and_controls() {
        assert_eq!(sanitize("a\u{1b}[31mred\u{7}\u{9b}b\nok"), "a[31mredb\nok");
    }

    #[test]
    fn render_show_unknown_and_deleted_states() {
        let none = ShowReply { card: None, outcome: None, reason_code: None, delivered: false, evidence: vec![] };
        assert_eq!(render_show(&none), "No card with that id.\n");
    }

    #[test]
    fn render_show_prompt_with_ansi_prints_sanitised() {
        let card = aiq_contracts::card::CardV1 {
            schema: "aiq.card.v1".into(),
            card_id: "c".into(),
            opportunity_id: "o".into(),
            tip_id: "t".into(),
            noticed: "n".into(),
            why: "w".into(),
            try_this: "x".into(),
            evidence_ref: "aiq show c".into(),
            pack_version: "p".into(),
        };
        let r = ShowReply {
            card: Some(card.clone()),
            outcome: Some("show_ready".into()),
            reason_code: Some("ok".into()),
            delivered: true,
            evidence: vec![EvidencePrompt { ts: "t".into(), text: Some("\u{1b}]0;pwned\u{7}hi".into()) }],
        };
        let out = render_show(&r);
        assert!(!out.contains('\u{1b}') && !out.contains('\u{7}'));
        assert!(out.contains("Status: shown"));
        let gone = ShowReply { evidence: vec![EvidencePrompt { ts: "t".into(), text: None }], ..r };
        assert!(render_show(&gone).contains("The evidence for this card was deleted."));
    }
}
