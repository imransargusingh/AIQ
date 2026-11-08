//! CLI commands. Output is plain text, no colour, copyable commands (design: UI and interaction design).
use aiq_contracts::frames::{OkReply, Request, ShowReply, StatusReply};
use aiq_core::control::ControlError;
use aiq_core::paths::Paths;
use aiq_core::server;
use std::io::{BufRead, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CLI_TIMEOUT: Duration = Duration::from_secs(5);
const NOT_RUNNING: &str = "AIQ is not running. It starts with your next Claude Code prompt, or run `aiq core`.";

pub fn core() -> ExitCode {
    let paths = Paths::from_env();
    crate::judge::start_background(paths.clone());
    aiq_core::update::start_background(paths.clone());
    // A tool installed after setup is connected at the next core start (or AIQ home open), not left waiting.
    std::thread::spawn(|| crate::install::install_new(true));
    match server::run_with(&paths, Some(crate::label::requester())) {
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

pub fn status(rest: &[&str]) -> ExitCode {
    let paths = Paths::from_env();
    let (out, folders) = status_text(&paths);
    if !rest.contains(&"--report") {
        print!("{out}");
        return ExitCode::SUCCESS;
    }
    let homes: Vec<String> =
        ["USERPROFILE", "HOME"].iter().filter_map(|k| std::env::var(k).ok()).filter(|h| !h.is_empty()).collect();
    let now = chrono::Utc::now();
    let report = format!(
        "AIQ help report, {} UTC\nAIQ {} on {} {}\n\n{}",
        now.format("%Y-%m-%d %H:%M"),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        redact(&out, &folders, &homes)
    );
    let file = paths.data.join(format!("aiq-report-{}.txt", now.format("%Y%m%d-%H%M%S")));
    let _ = paths.ensure_dirs();
    match aiq_store::create_user_only(&file).and_then(|()| std::fs::write(&file, &report)) {
        Ok(()) => {
            println!("Wrote {}", file.display());
            println!(
                "It holds versions, counts, timings and error codes. No prompt text; folders show as \
                 folder-1, folder-2 and your home folder as ~. Read it first. Send it only if you choose to."
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("Couldn't write the report to {}: {e}", file.display());
            ExitCode::from(1)
        }
    }
}

/// The help report's text with every named folder shown as `folder-<n>` and the home folder as `~`
/// (private-alpha design A8: no folder names). Longest name first, so a folder inside another is
/// replaced whole. Status text holds no prompt text to begin with: only counts, times and codes.
pub fn redact(text: &str, folders: &[String], homes: &[String]) -> String {
    let mut names: Vec<(String, String)> =
        folders.iter().enumerate().map(|(i, f)| (f.clone(), format!("folder-{}", i + 1))).collect();
    for h in homes {
        names.push((h.clone(), "~".to_owned()));
    }
    names.retain(|(n, _)| !n.is_empty());
    names.sort_by_key(|(n, _)| std::cmp::Reverse(n.len()));
    let mut out = text.to_owned();
    for (n, r) in &names {
        out = out.replace(n.as_str(), r);
        // Windows paths may also print with forward slashes.
        out = out.replace(&n.replace('\\', "/"), r);
    }
    out
}

/// `aiq status` as text, and the folder names it mentions (for the help report's redaction).
fn status_text(paths: &Paths) -> (String, Vec<String>) {
    let paths = paths.clone();
    let mut folders: Vec<String> = Vec::new();
    let reply =
        ask_core(&paths, |token| Request::Status { token }).and_then(|l| serde_json::from_str::<StatusReply>(&l).ok());
    let mut out = String::new();
    match &reply {
        Some(s) => {
            let events: u64 = s.events_by_kind.iter().map(|(_, n)| n).sum();
            if s.paused {
                out += "AIQ is paused. It saves nothing new and shows no tips. Resume with: aiq resume\n";
            } else if events == 0 {
                out += "No activity yet. Use Claude Code or Copilot as usual; AIQ runs quietly in the background.\n";
            } else {
                out += "AIQ is running.\n";
            }
            out += &format!("Tips today: {} of {}\n", s.cards_today, s.daily_cap);
            if let Some(f) = &s.tip_funnel {
                out += &render_funnel(f);
            }
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
            folders.extend(s.automated_folders.iter().map(|(f, _)| f.clone()));
            for (folder, reason) in &s.automated_folders {
                if reason == "projects" {
                    out += &format!(
                        "Sessions Claude ran for you through claude.ai Projects or Remote Control aren't counted in \
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
                    "Automated, not counted: {folder} ({why}). If that is your own work: aiq include \"{folder}\"\n"
                );
            }
            if let Some(c) = &s.coach {
                let state = match (c.mode.as_str(), c.tips_on) {
                    ("off", _) => "off (you turned them off)".to_owned(),
                    (_, false) => format!(
                        "off: AIQ guessed the kind of task right {} of {} times, so it stopped them. \
                         Turn back on: aiq config set next-rung-tips on",
                        c.right, c.checked
                    ),
                    _ => "on".to_owned(),
                };
                out += &format!("Next-step tips: {state}\n");
                out += &format!(
                    "Kind-of-task guesses checked: right {} of {}. Kinds of task with a next step: {}\n",
                    c.right, c.checked, c.coached_types
                );
                if let Some(o) = c.other_pct {
                    out += &format!("Analyzed tasks typed \"other\": {o}%\n");
                }
                out += &render_accuracy_paused(&c.accuracy_paused);
                if let Some(l) = &c.labels {
                    out += &render_labels(l);
                }
            }
            if let Some(a) = &s.agent_actions {
                out += &render_actions(a);
            }
            if let Some((_, n)) = ignored {
                out += &format!("Automated prompts not counted since the core started: {n}\n");
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
    if let Some(l) = crate::install_vscode::status_line() {
        out += &format!("VS Code: {l}\n");
    }
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    let up = aiq_core::update::view(&paths, aiq_core::update::stored_mode(&paths).as_deref(), now_ms);
    if let Some(l) = update_status_line(&up) {
        out += &l;
    }
    let spooled = std::fs::read_to_string(paths.spool()).map(|s| s.lines().count()).unwrap_or(0);
    if spooled > 0 {
        out += &format!("Queued while the core was down: {spooled} events\n");
    }
    let aside = SetAside::from_folders(reply.iter().flat_map(|s| &s.automated_folders));
    // The previous fill first, so the lines stay in time order.
    let text = [paths.hook_timings_prev(), paths.hook_timings()]
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap_or_default())
        .collect::<String>();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    out += &hook_time_lines(&text, now, &aside);
    // upgrade.ps1 logs in the AIQ home; the laptop's dev swapper logs next to the exes. Newest of both.
    let upgrade = [paths.data.join(UPGRADE_LOG), paths.data.join("bin").join(UPGRADE_LOG)]
        .iter()
        .filter_map(|p| last_upgrade(&decode_log(&std::fs::read(p).ok()?)?))
        .max();
    out += &hook_budget_lines(&text, &aside, upgrade);
    let errs = hook_error_counts(&paths);
    if !errs.is_empty() {
        out += &format!("Hook notes: {}\n", errs.join(", "));
    }
    (out, folders)
}

fn human_bytes(b: u64) -> String {
    match b {
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.0} KB", b as f64 / (1 << 10) as f64),
        b => format!("{b} bytes"),
    }
}

/// p95 of each phase of a hook run over the last 24 h, per event, from lines that carry phase fields.
/// Older lines have no `at=` and are left out rather than guessed into a window.
pub fn hook_phase_lines(text: &str, now: u64) -> String {
    let mut by: std::collections::BTreeMap<&str, [Vec<u64>; 3]> = Default::default();
    for l in text.lines() {
        let mut it = l.split_whitespace();
        let (Some(ev), Some(total)) = (it.next(), it.next().and_then(|u| u.parse::<u64>().ok())) else { continue };
        let field = |k: &str| l.split_whitespace().find_map(|w| w.strip_prefix(k)).and_then(|m| m.parse::<u64>().ok());
        let (Some(at), Some(read), Some(prep)) = (field("at="), field("read_us="), field("prep_us=")) else { continue };
        if now.saturating_sub(at) > 24 * 3600 {
            continue;
        }
        let v = by.entry(ev).or_default();
        v[0].push(read);
        v[1].push(prep);
        v[2].push(total.saturating_sub(read + prep));
    }
    let p95 = |v: &mut Vec<u64>| {
        v.sort_unstable();
        v[((v.len() as f64 * 0.95).ceil() as usize).saturating_sub(1).min(v.len() - 1)] as f64 / 1000.0
    };
    by.into_iter()
        .map(|(ev, mut v)| {
            let n = v[0].len();
            let (r, p, s) = (p95(&mut v[0]), p95(&mut v[1]), p95(&mut v[2]));
            format!("  {ev}, last 24 h ({n} runs), p95 by phase: reading input {r:.1} ms, preparing {p:.1} ms, sending {s:.1} ms\n")
        })
        .collect()
}

/// The upgrade script's log (`packaging/windows/upgrade.ps1`), in the AIQ home: `<yyyy-mm-dd hh:mm:ssZ> <message>`.
pub const UPGRADE_LOG: &str = "upgrade-log.txt";

/// An upgrade log's text. Windows PowerShell 5.1's `Tee-Object`/`Out-File` write UTF-16 LE with a BOM (the
/// laptop's bin\upgrade-log.txt, measured 2026-11-02: starts FF FE, half its bytes zero), which
/// `read_to_string` rejects; UTF-16 BE and UTF-8 (with or without a BOM) are read too. None when it is
/// none of these, so the budget line says no upgrade is recorded rather than guessing.
pub fn decode_log(bytes: &[u8]) -> Option<String> {
    let utf16 = |b: &[u8], le: bool| {
        let units: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| if le { u16::from_le_bytes(*c) } else { u16::from_be_bytes(*c) })
            .collect();
        String::from_utf16(&units).ok()
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8(rest.to_vec()).ok(),
        _ => String::from_utf8(bytes.to_vec()).ok(),
    }
}
/// The hook speed budget of record: AIQ's own part of each hook run, p95 (Imran, 2026-11-01; alpha gate G1).
pub const HOOK_BUDGET_MS: f64 = 100.0;

/// Unix seconds of the last time the exes changed, from one upgrade log. Two writers, two line shapes and
/// two places (`<home>\upgrade-log.txt` and `<home>\bin\upgrade-log.txt`; `status` reads both):
/// `upgrade.ps1` writes `yyyy-mm-dd hh:mm:ssZ Upgraded AIQ…` / `… Rolled back to AIQ…`, `install.ps1` writes
/// `… Installed AIQ…` (lines where nothing
/// changed are skipped); the laptop's dev swapper writes `<RFC 3339 with offset> swapped aiq.exe (<n> bytes)`
/// (measured 2026-11-02: `2026-11-02T14:00:57.7222903-08:00 swapped aiq.exe (5920768 bytes)`).
/// assume: every exe change goes through one of the two. If false: the budget line says no upgrade is
/// recorded, or counts from an older swap (never a guessed window).
pub fn last_upgrade(log: &str) -> Option<u64> {
    log.lines()
        .filter_map(|l| {
            let m = l.get(21..).unwrap_or("");
            if ["Upgraded AIQ", "Rolled back to AIQ", "Installed AIQ"].iter().any(|p| m.starts_with(p)) {
                let t = chrono::NaiveDateTime::parse_from_str(l.get(..19)?, "%Y-%m-%d %H:%M:%S").ok()?;
                return Some(t.and_utc().timestamp());
            }
            let (ts, rest) = l.split_once(' ')?;
            if !rest.starts_with("swapped aiq.exe") {
                return None;
            }
            chrono::DateTime::parse_from_rfc3339(ts).ok().map(|t| t.timestamp())
        })
        .map(|t| t.max(0) as u64)
        .max()
}

/// The figure that counts against the hook budget: AIQ's own part of each run (total less reading the
/// host's input, which is the host's wait), p95 per event, over runs since the last upgrade, so a fix shows
/// the day it is installed instead of when older lines age out. Same exclusions as the other hook figures.
pub fn hook_budget_lines(text: &str, aside: &SetAside, since: Option<u64>) -> String {
    let Some(since) = since else {
        return format!(
            "Hook budget ({HOOK_BUDGET_MS:.0} ms p95, AIQ's own part): no upgrade recorded in {UPGRADE_LOG}, so \
             no since-upgrade figure\n"
        );
    };
    let at = chrono::DateTime::from_timestamp(since as i64, 0).map(|t| t.format("%Y-%m-%d %H:%MZ").to_string());
    let mut out = format!(
        "Hook budget ({HOOK_BUDGET_MS:.0} ms p95 of AIQ's own part, the figure for alpha gate G1), since the last \
         upgrade at {}:\n",
        at.unwrap_or_default()
    );
    let (kept, _) = split_hook_runs(text, aside);
    let mut by: std::collections::BTreeMap<&str, Vec<u64>> = Default::default();
    for l in kept {
        let mut it = l.split_whitespace();
        let (Some(ev), Some(total)) = (it.next(), it.next().and_then(|u| u.parse::<u64>().ok())) else { continue };
        let field = |k: &str| l.split_whitespace().find_map(|w| w.strip_prefix(k)).and_then(|m| m.parse::<u64>().ok());
        let (Some(t), Some(read)) = (field("at="), field("read_us=")) else { continue };
        if t >= since {
            by.entry(ev).or_default().push(total.saturating_sub(read));
        }
    }
    if by.is_empty() {
        out += "  no hook runs since then yet\n";
    }
    for (ev, mut v) in by {
        v.sort_unstable();
        let p95 = v[((v.len() as f64 * 0.95).ceil() as usize).saturating_sub(1).min(v.len() - 1)] as f64 / 1000.0;
        let verdict = if p95 <= HOOK_BUDGET_MS { "within" } else { "OVER" };
        out += &format!("  {ev}: p95 {p95:.1} ms over {} runs, {verdict} budget\n", v.len());
    }
    out
}

/// Hook outcomes where AIQ did no work for a person: a headless `claude -p` / Agent SDK run (`automated`)
/// and AIQ's own judge calls (`internal`). Their time is the host's stdin wait, not AIQ's.
const NOT_A_PERSON: [&str; 2] = ["automated", "internal"];

/// The set-aside folders, as the timing log's folder tags, split the way the task list splits them (PR #87).
#[derive(Debug, Default)]
pub struct SetAside {
    /// Every run left out: Claude's own Projects / Remote Control folders (`projects`) and AIQ's own check folders
    /// (`aiq-check`), whose sessions the core records nothing for, however many prompts they send.
    pub whole: Vec<String>,
    /// Folders set aside as automated (burst, template, or the import's rule): a run counts when its session sent
    /// two or more prompts, the rule's H ≥ 2, the same test that keeps a person's task there.
    pub by_session: Vec<String>,
}

impl SetAside {
    /// From the core's `(folder, reason)` list.
    pub fn from_folders<'a>(folders: impl IntoIterator<Item = &'a (String, String)>) -> Self {
        let mut s = Self::default();
        for (f, reason) in folders {
            let Some(tag) = aiq_domain::automated::folder_tag(f) else { continue };
            if matches!(reason.as_str(), "projects" | "aiq-check") { s.whole.push(tag) } else { s.by_session.push(tag) }
        }
        s
    }
}

/// Prompt events, by host: a session with two or more of these in the log is a person's.
const PROMPT_EVENTS: [&str; 2] = ["UserPromptSubmit", "userPromptSubmitted"];

/// Splits the timing log into the runs that count against AIQ's hook budget and the number left out: runs
/// with a `NOT_A_PERSON` outcome, runs in a `whole` folder, and runs in a `by_session` folder whose session has
/// fewer than two prompt runs in the log (or no session tag: builds before 0.1.4).
/// Lines from builds before the folder tag are kept unless their outcome says otherwise.
/// assume: the log holds a session's earlier prompts. If false (the log rotated twice since): that session's
/// runs are left out until it sends two more, which the left-out count shows.
pub fn split_hook_runs<'a>(text: &'a str, aside: &SetAside) -> (Vec<&'a str>, usize) {
    let field = |l: &'a str, k: &str| l.split_whitespace().find_map(|w| w.strip_prefix(k));
    let mut prompts: std::collections::HashMap<&str, u32> = Default::default();
    for l in text.lines().filter(|l| l.split_whitespace().next().is_some_and(|e| PROMPT_EVENTS.contains(&e))) {
        if let Some(s) = field(l, "s=") {
            *prompts.entry(s).or_default() += 1;
        }
    }
    let mut left_out = 0;
    let kept = text
        .lines()
        .filter(|l| {
            let outcome = l.split_whitespace().nth(2).unwrap_or("");
            let in_folder = |set: &[String]| field(l, "dir=").is_some_and(|d| set.iter().any(|a| a == d));
            let person = field(l, "s=").is_some_and(|s| prompts.get(s).is_some_and(|n| *n >= 2));
            let out =
                NOT_A_PERSON.contains(&outcome) || in_folder(&aside.whole) || (in_folder(&aside.by_session) && !person);
            left_out += usize::from(out);
            !out
        })
        .collect();
    (kept, left_out)
}

/// p50 / p95 in-process hook time per event, from the hook's own timing log, over the runs that count
/// against AIQ's budget, plus one line naming how many runs were left out.
pub fn hook_time_lines(text: &str, now: u64, aside: &SetAside) -> String {
    let (kept, left_out) = split_hook_runs(text, aside);
    let text = kept.join("\n");
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
    out += &hook_phase_lines(&text, now);
    if !host.is_empty() {
        let (p50, p95) = pct(&mut host);
        out += &format!(
            "Copilot's own start-up before AIQ's hook runs: p50 {p50:.0} ms, p95 {p95:.0} ms ({} runs; not counted \
             against AIQ's budget)\n",
            host.len()
        );
    }
    if left_out > 0 {
        out += &format!(
            "Hook times leave out {left_out} runs from sessions AIQ sets aside (automated sessions and AIQ's own \
             checks)\n"
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
    if r.card.is_none() && card_id.is_empty() {
        println!("No tips yet.");
        return ExitCode::from(1);
    }
    // The team's "Tool to try" line, unless Claude Code ran this (team tips: never shown on Claude Code).
    let tool = if aiq_core::team_tips::inside_claude_code() {
        None
    } else {
        let tip_id = r.card.as_ref().map(|c| c.tip_id.clone()).unwrap_or_default();
        aiq_core::team_tips::load(&paths).and_then(|t| aiq_core::team_tips::line_for_tip(&t, &tip_id))
    };
    print!("{}", render_show_with_tool(&r, tool.as_deref()));
    if r.card.is_none() { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

pub fn render_show_with_tool(r: &ShowReply, tool_line: Option<&str>) -> String {
    let Some(c) = &r.card else { return "No tip with that id.\n".into() };
    let tool = tool_line.map(|l| format!("{l}\n")).unwrap_or_default();
    let mut out = format!("AIQ tip: {}\nWhy: {}\nTry this: {}\n{tool}\n", c.noticed, c.why, c.try_this);
    let status =
        if r.delivered { "shown".to_string() } else { r.outcome.clone().unwrap_or_default().replace('_', "-") };
    out += &format!("Status: {status} (reason: {})\n", r.reason_code.clone().unwrap_or_default());
    if c.opportunity_id == aiq_domain::coach::OPPORTUNITY {
        // Next-rung tip: a starter to copy, and the kinds of task it leaned on (never their text).
        if let Some(st) = &c.starter {
            out += &format!("\nStarter prompt (copy and fill in; AIQ never sends it):\n  {st}\n");
        }
        out += "\nAIQ guessed the kind of task from these earlier tasks of yours:\n";
        if r.leaned_on.is_empty() {
            out += "  (none left: they were deleted)\n";
        }
        for (i, l) in r.leaned_on.iter().enumerate() {
            out += &format!("  {}. {}  {}  {}\n", i + 1, l.ts, l.tool, l.task_type);
        }
        out += &format!(
            "\nWas this useful?  aiq helpful {id} | aiq not {id} | aiq snooze {id} | aiq off {id}\n",
            id = c.card_id
        );
        return out;
    }
    if r.evidence.iter().all(|e| e.text.is_none()) {
        out += "The prompts behind this tip were deleted.\n";
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
            "AIQ is paused. It saves nothing new and shows no tips. Resume with: aiq resume"
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
            println!("AIQ will include sessions in {folder} again.");
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

/// Live agent actions today (design live-agent-actions): turns read, turns not recorded and why, and what the
/// reading cost per turn. All counted from stored rows.
pub fn render_actions(a: &aiq_contracts::frames::ActionsStatus) -> String {
    if !a.enabled {
        return "What the agent did per turn: off (aiq config set agent-actions on)\n".into();
    }
    if a.turns == 0 {
        return "What the agent did per turn: on, no turn read today\n".into();
    }
    let why: Vec<String> = a.why.iter().map(|(w, n)| format!("{w} {n}")).collect();
    let missed = if a.not_recorded == 0 {
        String::new()
    } else {
        format!(", not recorded {} ({})", a.not_recorded, why.join(", "))
    };
    format!(
        "What the agent did per turn: {} turns read today{missed}. Reading cost per turn: p95 {} ms, max {} ms, \
         largest read {} KB\n",
        a.turns,
        a.p95_ms.unwrap_or(0),
        a.max_ms.unwrap_or(0),
        a.max_bytes.unwrap_or(0).div_ceil(1024)
    )
}

/// Jobs and kinds the accuracy check paused, and the one command that keeps each showing (design K6b).
/// Kind labels (design kind-label): today's calls against the budget, and AIQ's own check of them.
/// The tip funnel (design tip-funnel): a dry day first, then the week from new tasks to tips opened, and why
/// tips were missed or held. Empty with no task on the tip path this week.
pub fn render_funnel(f: &aiq_contracts::frames::TipFunnels) -> String {
    use aiq_contracts::frames::FunnelCount;
    let list = |v: &[FunnelCount]| v.iter().map(|c| format!("{} {}", c.count, c.words)).collect::<Vec<_>>().join("; ");
    let mut out = String::new();
    for d in &f.droughts {
        out += &format!("No tips on {} across {} tasks.", d.day, d.tasks);
        if let Some(r) = &d.main_reason {
            out += &format!(" Main reason: {} ({} tasks).", r.words, r.count);
        }
        out += "\n";
    }
    for (name, p) in [("last 7 days", &f.week), ("today", &f.today)] {
        if p.tasks == 0 && p.tips == 0 {
            continue;
        }
        let shown: u32 = p.shown.iter().map(|c| c.count).sum();
        out += &format!(
            "Tips, {name}: {} new tasks, kind known {}, tip found {}. Tips {}, shown {shown}, opened {}, answered {}\n",
            p.tasks, p.kind_known, p.tip_found, p.tips, p.opened, p.answered
        );
        if name == "today" {
            continue; // the week's detail covers today
        }
        if !p.no_tip.is_empty() {
            out += &format!("  No tip: {}\n", list(&p.no_tip));
        }
        if !p.held.is_empty() {
            out += &format!("  Held: {}\n", list(&p.held));
        }
        if p.expired > 0 {
            out += &format!("  Ready, but the session ended first: {}\n", p.expired);
        }
        if p.waiting > 0 {
            out += &format!("  Ready and waiting for the turn to end: {}\n", p.waiting);
        }
        if !p.shown.is_empty() {
            out += &format!("  Shown in: {}\n", list(&p.shown));
        }
    }
    out
}

pub fn render_labels(l: &aiq_contracts::frames::LabelStatus) -> String {
    if !l.enabled {
        return "Kind-of-task labels: off (on while background analysis is on; aiq config set kind-label auto)\n"
            .into();
    }
    let ms = l.median_ms.map_or(String::new(), |m| format!(", median {:.1} s", m as f64 / 1000.0));
    format!(
        "Kind-of-task labels today: {} of {} (answered {}, confident {}{ms}). Labels checked against the analysis: \
         right {} of {}\n",
        l.today, l.daily, l.answered, l.confident, l.right, l.checked
    )
}

pub fn render_accuracy_paused(p: &[aiq_contracts::frames::PausedScope]) -> String {
    let mut out = String::new();
    for s in p {
        if s.kept_on {
            out += &format!(
                "Next-step tips for {}: kept on by you, though a second model agreed with AIQ's analysis only {} of {} times. \
                 Let AIQ decide again: aiq config set keep-on.{} auto\n",
                s.label, s.agreed, s.checked, s.scope
            );
        } else {
            out += &format!(
                "Next-step tips for {}: paused by AIQ. A second model agreed with AIQ's analysis only {} of {} times \
                 (under 8 in 10). They come back when a later check clears the bar (aiq analyze --check). \
                 Keep them showing: aiq config set keep-on.{} on\n",
                s.label, s.agreed, s.checked, s.scope
            );
        }
    }
    out
}

/// `aiq config set <name> <value>`: one of the settings listed in the unknown-setting message.
pub fn config_set(name: &str, value: &str) -> ExitCode {
    // `analysis*` is the name users see; the stored keys and the old `judge*` names stay as they are.
    let renamed = name.strip_prefix("analysis").map(|rest| format!("judge{rest}"));
    let name = renamed.as_deref().unwrap_or(name);
    let keep_on = name.strip_prefix("keep-on.").map(|s| format!("keep_on.{s}"));
    let (key, done) = match name {
        _ if keep_on.is_some() => {
            (keep_on.as_deref().unwrap_or_default(), format!("Next-step tips for {}: {value}.", &name[8..]))
        }
        "daily-cap" => ("daily_cap", format!("Daily tip limit set to {value}.")),
        "copilot-tip-stays" => ("copilot_tip_stays", format!("Copilot CLI tip stays: {value}.")),
        "judge" => ("judge", format!("Background analysis: {value}.")),
        "judge-daily-tasks" => ("judge_daily_tasks", format!("Most tasks analyzed per day: {value}.")),
        "judge-claude-model" => ("judge_claude_model", format!("Claude analysis model: {value}.")),
        "judge-copilot-model" => ("judge_copilot_model", format!("Copilot analysis model: {value}.")),
        "judge-copilot-sessions" => ("judge_copilot_sessions", format!("Copilot analysis sessions: {value}.")),
        "judge-calls" => ("judge_calls", format!("Model calls per analyzed task: {value}.")),
        "judge-copilot-second-model" => {
            ("judge_copilot_second_model", format!("Copilot analysis, second call's model: {value}."))
        }
        "next-rung-tips" => ("next_rung_tips", format!("Next-step tips: {value}.")),
        "copilot-tip-in-chat" => ("copilot_tip_in_chat", format!("Copilot CLI tip in the conversation: {value}.")),
        "tip-popup" => ("tip_popup", format!("Windows pop-up for tips: {value}.")),
        "tip-highlight" => ("tip_highlight", format!("Tip highlight: {value}.")),
        "check-claude-model" => ("check_claude_model", format!("Accuracy check, Claude model: {value}.")),
        "check-copilot-model" => ("check_copilot_model", format!("Accuracy check, Copilot model: {value}.")),
        "update" => ("update", format!("Updates: {value}.")),
        "copilot-mid-turn" => ("copilot_mid_turn", format!("Copilot messages sent while the agent works: {value}.")),
        "agent-actions" => ("agent_actions", format!("What the agent did in each turn: {value}.")),
        "live-trend" => ("live_trend", format!("Where an open task is heading: {value}.")),
        "kind-label" => ("kind_label", format!("Kind-of-task labels: {value}.")),
        "kind-label-daily" => ("kind_label_daily", format!("Most kind labels per day: {value}.")),
        "kind-label-claude-model" => ("kind_label_claude_model", format!("Kind label, Claude model: {value}.")),
        "kind-label-copilot-model" => ("kind_label_copilot_model", format!("Kind label, Copilot model: {value}.")),
        _ => {
            println!(
                "Unknown setting {name}. Settings: daily-cap, copilot-tip-stays, copilot-tip-in-chat, tip-popup, \
                 tip-highlight, analysis, analysis-daily-tasks, \
                 analysis-claude-model, analysis-copilot-model, analysis-copilot-sessions, analysis-calls, analysis-copilot-second-model, next-rung-tips, check-claude-model, \
                 check-copilot-model, update, kind-label, kind-label-daily, kind-label-claude-model, kind-label-copilot-model, \
                 copilot-mid-turn, agent-actions, live-trend, keep-on.<job or job/kind>."
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

/// `aiq helpful|not|snooze|off <card>` and `aiq on <tip>`: the person's answer to a tip (coaching D8).
pub fn tip_feedback(verb: &str, id: &str) -> ExitCode {
    let (answer, done) = match verb {
        "helpful" => ("helpful", "Thanks: AIQ noted this tip helped."),
        "not" => ("not_helpful", "Noted. Two of these in 30 days and AIQ shows this tip last."),
        "snooze" => ("snooze", "Snoozed for 7 days on every tool."),
        "off" => ("off", "AIQ won't show this tip again on any tool. Turn it back on: aiq on <tip>"),
        "on" => ("on", "AIQ may show this tip again."),
        _ => return ExitCode::from(2),
    };
    let paths = Paths::from_env();
    let (card, tip) = if verb == "on" { (None, Some(id)) } else { (Some(id), None) };
    match aiq_core::control::tip_feedback(&paths, card, tip, answer, "cli", CLI_TIMEOUT) {
        Ok(()) => {
            println!("{done}");
            ExitCode::SUCCESS
        }
        Err(ControlError::Refused(m)) => {
            println!("Not changed: {m}");
            ExitCode::from(2)
        }
        Err(e) => {
            println!("Not changed: {e:?}");
            ExitCode::from(1)
        }
    }
}

/// `aiq flag <job>/<type> <move> <reason> [note]` and `aiq unflag <job>/<type> <move>` ("This doesn't fit").
pub fn flag(args: &[&str], undo: bool) -> ExitCode {
    let usage = "Usage: aiq flag <job>/<type> <move> <evidence_wrong|not_useful|blocked|other> [note]\n       aiq unflag <job>/<type> <move>";
    let (Some(kind), Some(move_id)) = (args.first(), args.get(1)) else {
        println!("{usage}");
        return ExitCode::from(2);
    };
    let Some((job, ty)) = kind.split_once('/') else {
        println!("{usage}");
        return ExitCode::from(2);
    };
    let reason = if undo { None } else { args.get(2).copied() };
    if !undo && reason.is_none() {
        println!("{usage}");
        return ExitCode::from(2);
    }
    let note = (!undo && args.len() > 3).then(|| args[3..].join(" "));
    let paths = Paths::from_env();
    let action = if undo { "undo" } else { "flag" };
    match aiq_core::control::guidance_flag(&paths, job, ty, move_id, action, reason, note.as_deref(), CLI_TIMEOUT) {
        Ok(()) => {
            println!(
                "{}",
                if undo {
                    "Undone: AIQ may suggest this again for this kind of task."
                } else {
                    "Noted. AIQ won't suggest this for this kind of task again. Your rung is unchanged. Undo: aiq unflag"
                }
            );
            ExitCode::SUCCESS
        }
        Err(ControlError::Refused(m)) => {
            println!("Not changed: {m}");
            ExitCode::from(2)
        }
        Err(e) => {
            println!("Not changed: {e:?}");
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
    if !confirm("Delete every prompt, tip and count AIQ saved on this computer? This cannot be undone.", yes) {
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
                println!("Also deleted {n} Copilot session(s) AIQ's analysis had left.");
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
    fn report_shows_no_folder_name_and_no_home() {
        let folders = vec![r"C:\Users\canary\src\secret-project".to_owned(), r"C:\Users\canary\src".to_owned()];
        let homes = vec![r"C:\Users\canary".to_owned()];
        let text = "Automated, not counted: C:\\Users\\canary\\src\\secret-project (burst).\n\
                    Automated, not counted: C:\\Users\\canary\\src (burst).\n\
                    Copilot: installed in C:/Users/canary/.copilot\n\
                    Claude Code: installed in C:\\Users\\canary\\.claude\\settings.json\n";
        let out = redact(text, &folders, &homes);
        assert!(!out.contains("canary") && !out.contains("secret-project"), "{out}");
        assert!(
            out.contains("not counted: folder-1 (burst)") && out.contains("not counted: folder-2 (burst)"),
            "{out}"
        );
        assert!(out.contains("installed in ~/.copilot"), "{out}");
    }

    #[test]
    fn hook_phases_cover_the_last_day_and_skip_old_lines() {
        let now = 100_000;
        let text = "UserPromptSubmit 500000 sent\n\
            UserPromptSubmit 10000 sent read_us=1000 prep_us=2000 at=99000 v=0.1.0\n\
            UserPromptSubmit 400000 sent read_us=390000 prep_us=5000 at=99500 v=0.1.0\n\
            UserPromptSubmit 9000 sent read_us=1 prep_us=1 at=1 v=0.1.0\n";
        let out = hook_phase_lines(text, now);
        assert_eq!(
            out,
            "  UserPromptSubmit, last 24 h (2 runs), p95 by phase: reading input 390.0 ms, preparing 5.0 ms, sending 7.0 ms\n"
        );
    }

    #[test]
    fn hook_times_leave_out_set_aside_runs_and_say_how_many() {
        let now = 100_000;
        let tacit = aiq_domain::automated::folder_tag(r"c:\repos\tacit").unwrap();
        let text = format!(
            "UserPromptSubmit 1000 sent read_us=400 prep_us=300 dir=0000beef at=99000 v=0.1.0\n\
             UserPromptSubmit 495000 automated read_us=495000 at=99100 v=0.1.0\n\
             UserPromptSubmit 900000 internal read_us=900000 at=99200 v=0.1.0\n\
             UserPromptSubmit 300000 sent host_ms=900 read_us=290000 prep_us=300 dir={tacit} at=99300 v=0.1.0\n\
             statusline 2000 none\n"
        );
        let out = hook_time_lines(&text, now, &SetAside { by_session: vec![tacit], ..Default::default() });
        assert_eq!(
            out,
            "Hook time in-process, UserPromptSubmit: p50 1.0 ms, p95 1.0 ms (1 runs)\n\
             Hook time in-process, statusline: p50 2.0 ms, p95 2.0 ms (1 runs)\n  \
             UserPromptSubmit, last 24 h (1 runs), p95 by phase: reading input 0.4 ms, preparing 0.3 ms, sending 0.3 ms\n\
             Hook times leave out 3 runs from sessions AIQ sets aside (automated sessions and AIQ's own checks)\n"
        );
        // Core not running: no folder list, so only the outcome rule applies; nothing left out says nothing.
        assert!(hook_time_lines(&text, now, &SetAside::default()).contains("leave out 2 runs"));
        assert!(!hook_time_lines("Stop 200 no-card\n", now, &SetAside::default()).contains("leave out"));
    }

    #[test]
    fn upgrade_logs_are_read_in_the_encodings_powershell_writes() {
        let line = "2026-11-02T15:19:56.9874094-08:00 swapped aiq.exe (5992960 bytes)\r\n";
        // Windows PowerShell 5.1 Tee-Object -Append: UTF-16 LE with a BOM (the laptop's file, 2026-11-02).
        let mut le = vec![0xFF, 0xFE];
        le.extend(line.encode_utf16().flat_map(u16::to_le_bytes));
        let mut be = vec![0xFE, 0xFF];
        be.extend(line.encode_utf16().flat_map(u16::to_be_bytes));
        let mut bom8 = vec![0xEF, 0xBB, 0xBF];
        bom8.extend(line.as_bytes());
        let want = last_upgrade(line);
        assert!(want.is_some());
        for bytes in [le, be, bom8, line.as_bytes().to_vec()] {
            assert_eq!(decode_log(&bytes).as_deref().and_then(last_upgrade), want);
        }
        assert_eq!(decode_log(&[0xFF, 0xFE, 0x00, 0xD8]), None, "broken UTF-16: not recorded, never guessed");
        assert_eq!(decode_log(&[0xC3]), None);
    }

    #[test]
    fn last_upgrade_reads_only_upgrades_that_changed_the_exes() {
        let log = "2026-11-02 15:14:00Z Upgraded AIQ 0.1.0 to 0.1.0 ((local zip)). Your data and settings are kept.\n\
                   2026-11-02 18:00:00Z AIQ-UPGRADE-CHECKSUM: aiq.zip does not match SHA256SUMS. Nothing changed.\n\
                   2026-11-02 19:00:00Z Rolled back to AIQ 0.1.0. Run -Rollback again to return to the newer one.\n\
                   2026-11-02 20:00:00Z AIQ 0.1.0 is the newest release. Nothing to do.\n\
                   garbage line\n";
        assert_eq!(last_upgrade(log), Some(1_793_646_000)); // 2026-11-02 19:00:00Z
        assert_eq!(last_upgrade(""), None);
        // The laptop's dev swapper, local time with an offset; a swapped aiq-home.exe alone does not count.
        let dev = format!(
            "{log}2026-11-02T14:00:57.7222903-08:00 swapped aiq.exe (5920768 bytes)\n\
             2026-11-02T14:30:00-08:00 swapped aiq-home.exe (100 bytes)\n"
        );
        assert_eq!(last_upgrade(&dev), Some(1_793_656_857)); // 2026-11-02 22:00:57Z
        // install.ps1 (a release install or reinstall) also changes the exes: measured 2026-11-03, the
        // laptop's budget window kept counting from an older dev swap after a 04:09Z install.
        let installed = format!("{dev}2026-11-03 04:09:00Z Installed AIQ 0.1.0 (install.ps1).\n");
        assert_eq!(last_upgrade(&installed), Some(1_793_678_940)); // 2026-11-03 04:09:00Z
    }

    #[test]
    fn hook_budget_counts_aiqs_own_part_since_the_upgrade() {
        let since = 1_000;
        let text = "UserPromptSubmit 460000 sent read_us=1000 prep_us=455000 at=900 v=0.1.0\n\
                    UserPromptSubmit 400000 sent read_us=399400 prep_us=300 at=1100 v=0.1.0\n\
                    UserPromptSubmit 2000 sent read_us=1500 prep_us=200 at=1200 v=0.1.0\n\
                    UserPromptSubmit 900000 automated read_us=900000 at=1300 v=0.1.0\n\
                    Stop 150000 no-card read_us=1000 prep_us=100 at=1400 v=0.1.0\n";
        assert_eq!(
            hook_budget_lines(text, &SetAside::default(), Some(since)),
            "Hook budget (100 ms p95 of AIQ's own part, the figure for alpha gate G1), since the last upgrade at \
             1970-01-01 00:16Z:\n  \
             Stop: p95 149.0 ms over 1 runs, OVER budget\n  \
             UserPromptSubmit: p95 0.6 ms over 2 runs, within budget\n"
        );
        assert!(hook_budget_lines("", &SetAside::default(), Some(since)).ends_with("  no hook runs since then yet\n"));
        assert!(hook_budget_lines(text, &SetAside::default(), None).contains("no upgrade recorded"));
    }

    #[test]
    fn hook_budget_counts_a_persons_session_in_a_template_folder_like_tasks_do() {
        // Shaped like Imran's laptop on 2026-11-04: foresight set aside by a template rule, the AIQ repo by rule RC.
        let folders = [
            (r"c:\repos\foresight".to_string(), "template".to_string()),
            (r"c:\repos\aiq".to_string(), "projects".to_string()),
            (r"c:\users\x\appdata\local\temp\aiq-check".to_string(), "aiq-check".to_string()),
        ];
        let aside = SetAside::from_folders(&folders);
        assert_eq!((aside.whole.len(), aside.by_session.len()), (2, 1));
        let tag = |f: &str| aiq_domain::automated::folder_tag(f).unwrap();
        let (fs, aiq) = (tag(r"c:\repos\foresight"), tag(r"c:\repos\aiq"));
        let s = |id: &str| aiq_domain::automated::session_tag(id).unwrap();
        let (person, app, rc) = (s("person"), s("app"), s("rc"));
        let text = format!(
            "userPromptSubmitted 3000 sent read_us=1000 dir={fs} s={person} at=2000 v=0.1.4\n\
             agentStop 5000 sent read_us=1000 dir={fs} s={person} at=2010 v=0.1.4\n\
             userPromptSubmitted 4000 sent read_us=1000 dir={fs} s={person} at=2020 v=0.1.4\n\
             userPromptSubmitted 90000 sent read_us=1000 dir={fs} s={app} at=2030 v=0.1.4\n\
             agentStop 90000 sent read_us=1000 dir={fs} s={app} at=2040 v=0.1.4\n\
             UserPromptSubmit 90000 sent read_us=1000 dir={aiq} s={rc} at=2050 v=0.1.4\n\
             UserPromptSubmit 90000 sent read_us=1000 dir={aiq} s={rc} at=2060 v=0.1.4\n\
             userPromptSubmitted 90000 sent read_us=1000 dir={fs} at=2070 v=0.1.2\n\
             UserPromptSubmit 90000 automated read_us=1000 s={person} at=2080 v=0.1.4\n"
        );
        let (kept, left_out) = split_hook_runs(&text, &aside);
        assert_eq!((kept.len(), left_out), (3, 6), "only the person's three foresight runs count");
        assert_eq!(
            hook_budget_lines(&text, &aside, Some(1_000)),
            "Hook budget (100 ms p95 of AIQ's own part, the figure for alpha gate G1), since the last upgrade at \
             1970-01-01 00:16Z:\n  \
             agentStop: p95 4.0 ms over 1 runs, within budget\n  \
             userPromptSubmitted: p95 3.0 ms over 2 runs, within budget\n"
        );
    }

    #[test]
    fn sanitize_strips_ansi_escape_and_controls() {
        assert_eq!(sanitize("a\u{1b}[31mred\u{7}\u{9b}b\nok"), "a[31mredb\nok");
    }

    #[test]
    fn render_show_unknown_and_deleted_states() {
        let none = ShowReply {
            card: None,
            outcome: None,
            reason_code: None,
            delivered: false,
            evidence: vec![],
            leaned_on: vec![],
            team_tool: None,
        };
        assert_eq!(render_show_with_tool(&none, None), "No tip with that id.\n");
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
            starter: None,
        };
        let r = ShowReply {
            card: Some(card.clone()),
            outcome: Some("show_ready".into()),
            reason_code: Some("ok".into()),
            delivered: true,
            evidence: vec![EvidencePrompt { ts: "t".into(), text: Some("\u{1b}]0;pwned\u{7}hi".into()) }],
            leaned_on: vec![],
            team_tool: None,
        };
        let out = render_show_with_tool(&r, None);
        assert!(!out.contains("Tool to try"));
        let with = render_show_with_tool(&r, Some("Tool to try: X: does Y."));
        assert!(with.contains("Try this: x\nTool to try: X: does Y.\n\n"), "{with}");
        assert!(!out.contains('\u{1b}') && !out.contains('\u{7}'));
        assert!(out.contains("Status: shown"));
        // A next-rung card prints the starter and the kinds of task leaned on, never prompt text.
        let mut nc = card.clone();
        nc.opportunity_id = "next_rung".into();
        nc.starter = Some("Write tests for <function>.".into());
        let nr = ShowReply {
            card: Some(nc),
            evidence: vec![],
            leaned_on: vec![aiq_contracts::frames::LeanedOn {
                ts: "2026-11-01T10:00:00Z".into(),
                tool: "copilot-cli".into(),
                task_type: "Writing tests".into(),
            }],
            ..r.clone()
        };
        let out = render_show_with_tool(&nr, None);
        assert!(out.contains("Write tests for <function>.") && out.contains("copilot-cli  Writing tests"));
        assert!(out.contains("aiq helpful c") && !out.contains("pwned"));
        let gone = ShowReply { evidence: vec![EvidencePrompt { ts: "t".into(), text: None }], ..r };
        assert!(render_show_with_tool(&gone, None).contains("The prompts behind this tip were deleted."));
    }
}

/// `aiq tips`: every next-step tip, whether it is on, and how far it reaches on the person's own judged tasks
/// in the coaching window. Measured from stored judgments; no model call, no core needed.
pub fn tips() -> ExitCode {
    let paths = Paths::from_env();
    let store = match crate::import::open_store(&paths) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let pack = match aiq_domain::PackSource::load(&aiq_core::pack::BundledPack) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Could not load AIQ's rules: {e}");
            return ExitCode::from(1);
        }
    };
    let utc_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    let now = aiq_domain::clock::Now { utc_ms, offset_min: 0 };
    let cache = match aiq_core::coaching::Cache::build(&store, &pack, now) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Could not read AIQ's store ({}): {}", e.code, e.detail);
            return ExitCode::from(1);
        }
    };
    use aiq_domain::coach;
    let p = &pack.coach;
    let reach = coach::reach(&cache.tasks, &cache.rows, &pack.moves, utc_ms, p);
    let f4 = coach::f4_missing(&cache.tasks, utc_ms, 0.8, p);
    let off: Vec<bool> = reach
        .iter()
        .map(|r| coach::tip_pref(&cache.feedback, &coach::tip_id(&r.move_id), utc_ms, p) == coach::TipPref::Off)
        .collect();
    let mode = aiq_core::coaching::mode(&store);
    print!(
        "{}",
        render_tips(
            &reach,
            &off,
            &f4,
            &mode,
            cache.tips_on(&mode, &pack, aiq_core::coaching::labels_enabled(&store)),
            p.window_days
        )
    );
    ExitCode::SUCCESS
}

/// `aiq progress`: each kind of task's typical rung in the last 14 days against the 14 before, read with
/// the same snapshot AIQ home reads (read-only, no model call), so both show the same numbers.
pub fn progress() -> ExitCode {
    let paths = Paths::from_env();
    let pack = match aiq_domain::PackSource::load(&aiq_core::pack::BundledPack) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Could not load AIQ's rules: {e}");
            return ExitCode::from(1);
        }
    };
    let utc_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    let s = aiq_home::snapshot(&paths, &pack, aiq_domain::clock::Now { utc_ms, offset_min: 0 });
    if let Some(e) = &s.status.error {
        eprintln!("Could not read AIQ's store: {e}");
        return ExitCode::from(1);
    }
    print!("{}", render_progress(&s.coaching));
    ExitCode::SUCCESS
}

fn render_progress(c: &aiq_home::Coaching) -> String {
    use aiq_domain::rung::{job_label, rung_name};
    let (d, min) = (c.progress_days, c.progress_min);
    let mut o = format!(
        "Rung progress by kind of task: the typical rung (most common) of your analyzed tasks in the last {d} days\n\
         against the {d} days before. Task counts are measured; rungs are AIQ's analysis. A kind needs\n\
         at least {min} analyzed tasks in each half, and a change bigger than chance, before AIQ says it moved.\n\n"
    );
    let up = c.progress.iter().filter(|r| r.trend == "up").count();
    let down = c.progress.iter().filter(|r| r.trend == "down").count();
    if c.progress.is_empty() {
        o += &format!("No analyzed tasks with a kind in the last {} days yet.\n", 2 * d);
        return o;
    }
    o += &format!("Moved up a rung: {up}. Moved down: {down}.\n\n");
    let rung = |r: Option<u8>| r.map_or("none".to_owned(), |r| format!("{} ({r})", rung_name(r)));
    for r in &c.progress {
        let what = match r.trend {
            "up" | "down" => {
                let steps = r.recent_rung.unwrap_or(0).abs_diff(r.earlier_rung.unwrap_or(0));
                format!(
                    "moved {} {} {}: {} -> {}",
                    r.trend,
                    steps,
                    if steps == 1 { "rung" } else { "rungs" },
                    rung(r.earlier_rung),
                    rung(r.recent_rung)
                )
            }
            "same" if r.recent_rung != r.earlier_rung => {
                format!("holding: {} -> {} is within chance", rung(r.earlier_rung), rung(r.recent_rung))
            }
            "same" => format!("same rung: {}", rung(r.recent_rung)),
            _ => "too few tasks to say".to_owned(),
        };
        o += &format!(
            "{} [{}]: {what}\n  {} analyzed in the last {d} days, {} in the {d} before{}\n",
            r.label,
            job_label(&r.job),
            r.recent,
            r.earlier,
            if r.uncertain > 0 { format!("; {} AIQ isn't sure about left out", r.uncertain) } else { String::new() }
        );
    }
    o += &render_habit_changes(c);
    o
}

/// Habits whose use in one kind of task changed by more than chance: the steps toward a rung move.
fn render_habit_changes(c: &aiq_home::Coaching) -> String {
    let d = c.progress_days;
    if c.habit_changes.is_empty() {
        return format!(
            "\nHabits: none changed by more than chance yet (each needs {} tasks where it fits in each half).\n",
            c.habit_change_min
        );
    }
    let mut o = format!("\nHabits that changed, last {d} days against the {d} before (bigger than chance):\n");
    for (dir, head) in [("up", "Growing"), ("down", "Less often lately")] {
        let rows: Vec<_> = c.habit_changes.iter().filter(|h| h.dir == dir).collect();
        if rows.is_empty() {
            continue;
        }
        o += &format!("{head}\n");
        for h in rows {
            o += &format!(
                "  {}: {}\n    {} of {} tasks before, {} of {} now\n",
                h.label, h.habit, h.earlier_seen, h.earlier_eligible, h.recent_seen, h.recent_eligible
            );
        }
    }
    o
}

fn render_tips(
    reach: &[aiq_domain::coach::Reach],
    off: &[bool],
    f4: &std::collections::BTreeMap<String, (u32, u32)>,
    mode: &str,
    on_now: bool,
    days: u32,
) -> String {
    let mut o = format!(
        "Next-step tips, on your analyzed tasks from the last {days} days (measured from your analyses, no model call).\n\
         A tip shows in the moment only for a kind of task you repeat whose next step it is.\n\
         Next-step tips: {mode}{}\n\n",
        if on_now { "" } else { " (not showing now)" }
    );
    let jobs: Vec<String> = f4.iter().map(|(j, (n, k))| format!("{j} {k} of {n}")).collect();
    o += &format!(
        "Requests AIQ's analysis was sure had no goal, context or limits (any rung): {}\n\n",
        if jobs.is_empty() { "no analyzed tasks yet".to_owned() } else { jobs.join(", ") }
    );
    for (r, is_off) in reach.iter().zip(off) {
        let kinds: Vec<String> = r.kinds.iter().map(|(l, n)| format!("{l} ({n} tasks)")).collect();
        o += &format!(
            "{} [{}{}]\n  applies to {} analyzed {}; can show now for: {}\n",
            r.habit,
            if r.job == "any" { "every job" } else { r.job.as_str() },
            if *is_off { ", turned off" } else { "" },
            r.tasks,
            if r.tasks == 1 { "task" } else { "tasks" },
            if kinds.is_empty() { "none yet".to_owned() } else { kinds.join(", ") }
        );
    }
    o += "\nTurn one off or on: aiq off <card> from a tip, aiq on next_rung.<id>, or Settings in AIQ home.\n";
    o
}

#[cfg(test)]
mod tips_tests {
    use super::*;

    #[test]
    fn funnel_leads_with_a_dry_day_and_names_every_loss_in_plain_words() {
        use aiq_contracts::frames::{FunnelCount, TipDrought, TipFunnel, TipFunnels};
        let c =
            |code: &str, n| FunnelCount { code: code.into(), words: aiq_domain::funnel::words(code).into(), count: n };
        let week = TipFunnel {
            days: 7,
            tasks: 20,
            kind_known: 5,
            tip_found: 1,
            no_tip: vec![c("no_match", 15), c("kind_too_few", 4)],
            tips: 6,
            held: vec![c("cooldown", 1)],
            waiting: 0,
            expired: 3,
            shown: vec![FunnelCount { code: "copilot-cli".into(), words: "Copilot CLI".into(), count: 2 }],
            opened: 0,
            answered: 0,
        };
        let f = TipFunnels {
            today: TipFunnel { days: 1, ..TipFunnel::default() },
            week,
            droughts: vec![TipDrought { day: "2026-11-07".into(), tasks: 9, main_reason: Some(c("no_match", 7)) }],
        };
        let out = render_funnel(&f);
        let first = out.lines().next().unwrap();
        assert_eq!(first, "No tips on 2026-11-07 across 9 tasks. Main reason: kind of task not recognised (7 tasks).");
        assert!(out.contains("Tips, last 7 days: 20 new tasks, kind known 5, tip found 1. Tips 6, shown 2"));
        assert!(out.contains("No tip: 15 kind of task not recognised; 4 too few tasks of this kind analyzed yet"));
        assert!(out.contains("Held: 1 same tip shown in the last 72 hours"));
        assert!(out.contains("Ready, but the session ended first: 3"));
        assert!(out.contains("Shown in: 2 Copilot CLI"));
        assert!(!out.contains("Tips, today"), "an empty day prints nothing");
        assert!(!out.to_lowercase().contains("judge"));
        assert!(render_funnel(&TipFunnels::default()).is_empty());
    }

    #[test]
    fn progress_report_names_moves_counts_and_too_few() {
        let row = |ty: &str, trend: &'static str, recent, earlier, rr, er, uncertain| aiq_home::ProgressRow {
            job: "code_change".into(),
            task_type: ty.into(),
            label: aiq_contracts::judge::task_type_label("code_change", ty).unwrap(),
            recent,
            earlier,
            uncertain,
            recent_rung: rr,
            earlier_rung: er,
            trend,
        };
        let c = aiq_home::Coaching {
            progress: vec![
                row("tests", "up", 4, 3, Some(3), Some(2), 1),
                row("fix_bug", "same", 3, 3, Some(1), Some(1), 0),
                row("refactor", "too_few", 1, 2, Some(2), Some(4), 0),
            ],
            progress_days: 14,
            progress_min: 3,
            ..Default::default()
        };
        let o = render_progress(&c);
        assert!(o.contains("last 14 days") && o.contains("at least 3 analyzed tasks in each half"), "{o}");
        assert!(o.contains("Moved up a rung: 1. Moved down: 0."), "{o}");
        assert!(
            o.contains("Writing tests [Code a change]: moved up 1 rung: Directed (2) -> Delegated (3)\n  4 analyzed in the last 14 days, 3 in the 14 before; 1 AIQ isn't sure about left out"),
            "{o}"
        );
        assert!(o.contains("same rung: Assisted (1)"), "{o}");
        assert!(o.contains("Refactoring or cleaning up [Code a change]: too few tasks to say\n  1 analyzed"), "{o}");
        assert!(o.contains("Habits: none changed by more than chance yet (each needs 0 tasks"), "{o}");
        let empty = aiq_home::Coaching { progress_days: 14, progress_min: 3, ..Default::default() };
        assert!(render_progress(&empty).contains("No analyzed tasks with a kind in the last 28 days yet."));
    }

    #[test]
    fn progress_report_says_holding_within_chance_and_lists_habit_changes() {
        let hc = |ty: &str, habit: &str, es, ee, rs, re, dir| aiq_home::HabitChangeRow {
            job: "code_change".into(),
            task_type: ty.into(),
            label: aiq_contracts::judge::task_type_label("code_change", ty).unwrap(),
            move_id: "m".into(),
            habit: habit.into(),
            earlier_seen: es,
            earlier_eligible: ee,
            recent_seen: rs,
            recent_eligible: re,
            dir,
        };
        let c = aiq_home::Coaching {
            progress: vec![aiq_home::ProgressRow {
                job: "code_change".into(),
                task_type: "feature".into(),
                label: "Adding a feature",
                recent: 15,
                earlier: 12,
                uncertain: 0,
                recent_rung: Some(3),
                earlier_rung: Some(1),
                trend: "same",
            }],
            habit_changes: vec![
                hc("feature", "Keep an instructions file in the repo and point to it", 2, 12, 9, 14, "up"),
                hc("fix_bug", "Paste the error or name the files", 9, 12, 4, 15, "down"),
            ],
            progress_days: 30,
            progress_min: 3,
            habit_change_min: 5,
            ..Default::default()
        };
        let o = render_progress(&c);
        assert!(
            o.contains("Adding a feature [Code a change]: holding: Assisted (1) -> Delegated (3) is within chance"),
            "{o}"
        );
        assert!(o.contains("Moved up a rung: 0. Moved down: 0."), "{o}");
        assert!(o.contains("Habits that changed, last 30 days against the 30 before (bigger than chance):\nGrowing\n  Adding a feature: Keep an instructions file in the repo and point to it\n    2 of 12 tasks before, 9 of 14 now\nLess often lately\n  Fixing a bug or failing test: Paste the error or name the files\n    9 of 12 tasks before, 4 of 15 now\n"), "{o}");
        for w in ["judge", "watch", "monitor"] {
            assert!(!o.to_lowercase().contains(w), "{w} in {o}");
        }
    }

    #[test]
    fn tips_report_names_reach_off_state_and_the_f4_ceiling() {
        let r = vec![
            aiq_domain::coach::Reach {
                move_id: "brief".into(),
                job: "code_change".into(),
                habit: "Give the agent the goal".into(),
                tasks: 4,
                kinds: vec![("Writing tests", 6)],
            },
            aiq_domain::coach::Reach {
                move_id: "written_brief".into(),
                job: "any".into(),
                habit: "Give a written brief".into(),
                tasks: 1,
                kinds: vec![],
            },
        ];
        let f4 = [("code_change".to_owned(), (32, 5))].into_iter().collect();
        let o = render_tips(&r, &[false, true], &f4, "auto", true, 30);
        assert!(o.contains("last 30 days"));
        assert!(o.contains("no goal, context or limits (any rung): code_change 5 of 32"));
        assert!(o.contains("Give the agent the goal [code_change]\n  applies to 4 analyzed tasks; can show now for: Writing tests (6 tasks)"));
        assert!(o.contains(
            "Give a written brief [every job, turned off]\n  applies to 1 analyzed task; can show now for: none yet"
        ));
        assert!(
            render_tips(&r, &[false, false], &Default::default(), "off", false, 30).contains("off (not showing now)")
        );
    }

    #[test]
    fn accuracy_pause_lines_say_why_and_give_one_command() {
        let p = |kept| aiq_contracts::frames::PausedScope {
            scope: "run_ship".into(),
            label: "Run or ship it".into(),
            agreed: 6,
            checked: 10,
            kept_on: kept,
        };
        let out = render_accuracy_paused(&[p(false)]);
        assert!(
            out.contains("paused by AIQ")
                && out.contains("6 of 10")
                && out.contains("aiq config set keep-on.run_ship on")
        );
        let out = render_accuracy_paused(&[p(true)]);
        assert!(out.contains("kept on by you") && out.contains("aiq config set keep-on.run_ship auto"));
        assert!(render_accuracy_paused(&[]).is_empty());
    }
}

/// `aiq update [--now]` (design auto-update U6): what the updater last did, and start a check now.
pub fn update(rest: &[&str]) -> ExitCode {
    use aiq_core::update::{self, Action};
    let paths = Paths::from_env();
    let mode = update::stored_mode(&paths);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    let v = update::view(&paths, mode.as_deref(), now);
    if rest.contains(&"--now") {
        // The script waits for this process to end, so it is not counted as AIQ being busy. An update already
        // downloaded is installed as it is, not downloaded again.
        let action = if v.note == "waiting" { Action::Apply } else { Action::Check { apply: true } };
        return match update::start(&paths, action, Some(std::process::id()), false) {
            Ok(()) => {
                if action == Action::Apply {
                    println!(
                        "Installing AIQ {} as soon as AIQ is idle (AIQ home closed). Run `aiq update` in a minute \
                         to see the result.",
                        v.available.as_deref().unwrap_or("update").trim_start_matches('v')
                    );
                } else {
                    println!(
                        "Checking for a newer AIQ now. If there is one, it installs as soon as AIQ is idle \
                         (AIQ home closed). Run `aiq update` in a minute to see the result."
                    );
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                println!("Could not start the update: {e}.");
                ExitCode::from(1)
            }
        };
    }
    print!("{}", update_text(&v, now));
    ExitCode::SUCCESS
}

/// `aiq status`'s one line about a newer AIQ, only while one is downloaded and waiting (design auto-update U6).
fn update_status_line(v: &aiq_core::update::View) -> Option<String> {
    if !v.supported || v.note != "waiting" {
        return None;
    }
    let ver = v.available.as_deref().unwrap_or("A newer version").trim_start_matches('v');
    let when = if v.mode == "auto" { " It installs when AIQ home is closed." } else { "" };
    Some(format!("Update: AIQ {ver} is ready.{when} Install now: aiq update --now\n"))
}

fn update_text(v: &aiq_core::update::View, now: i64) -> String {
    let ago = |ms: i64| {
        let h = (now - ms).max(0) / 3_600_000;
        if h < 1 {
            "less than an hour ago".to_owned()
        } else if h < 48 {
            format!("{h} h ago")
        } else {
            format!("{} days ago", h / 24)
        }
    };
    let mut o = format!("AIQ {}.\n", v.running);
    if !v.supported {
        o.push_str("This copy was not installed with install.ps1, so it does not update itself.\n");
        return o;
    }
    o.push_str(match v.mode.as_str() {
        "auto" => "Updates: automatic (aiq config set update ask: tell me first; off: never check).\n",
        "off" => "Updates: off. AIQ never checks by itself. Check now: aiq update --now (aiq config set update auto: turn it back on).\n",
        _ => "Updates: tell me first (aiq config set update auto: install by itself; off: never check).\n",
    });
    match v.checked_ms {
        Some(t) => o.push_str(&format!("Last checked {}.", ago(t))),
        None => o.push_str("Not checked yet. The first check runs a few minutes after AIQ starts."),
    }
    if let Some(e) = &v.check_error {
        o.push_str(&format!(" That check failed ({e}); it tries again within the hour."));
    }
    o.push('\n');
    let avail = v.available.as_deref().unwrap_or("A newer version");
    match v.note {
        "waiting" if v.mode == "auto" => o.push_str(&format!(
            "{avail} is downloaded and checked. It installs when AIQ is idle{}.\n",
            v.blocked.as_deref().map(|b| format!(" (last try: {b})")).unwrap_or_default()
        )),
        "waiting" => o.push_str(&format!("{avail} is downloaded and checked. Install it: aiq update --now\n")),
        "applied" => o.push_str(&format!(
            "Updated from {} {}.\n",
            v.from.as_deref().unwrap_or("the previous version"),
            v.at_ms.map(ago).unwrap_or_default()
        )),
        "failed" => {
            o.push_str(&format!("The last update did not install: {}.\n", v.message.as_deref().unwrap_or("unknown")))
        }
        "rolled_back" => o.push_str(&format!(
            "You went back from {}. AIQ won't install that version again by itself.\n",
            v.from.as_deref().unwrap_or("the newer version")
        )),
        _ => {}
    }
    if v.can_rollback {
        o.push_str("Go back to the previous version: powershell -ExecutionPolicy Bypass -File %USERPROFILE%\\.aiq\\bin\\upgrade.ps1 -Rollback\n");
    }
    o
}

#[cfg(test)]
mod update_tests {
    use super::update_text;
    use aiq_core::update::View;

    fn v() -> View {
        View {
            supported: true,
            mode: "auto".into(),
            running: "0.1.0".into(),
            note: "none",
            available: None,
            blocked: None,
            from: None,
            at_ms: None,
            message: None,
            checked_ms: None,
            check_error: None,
            latest: None,
            can_rollback: false,
        }
    }

    #[test]
    fn dev_copy_says_it_does_not_update() {
        let t = update_text(&View { supported: false, ..v() }, 0);
        assert!(t.contains("does not update itself"));
    }

    #[test]
    fn waiting_names_what_it_waits_for() {
        let w = View {
            note: "waiting",
            available: Some("v0.2.0".into()),
            blocked: Some("AIQ home is open".into()),
            checked_ms: Some(0),
            ..v()
        };
        let t = update_text(&w, 3_600_000 * 3);
        assert!(t.contains("Last checked 3 h ago."), "{t}");
        assert!(
            t.contains("v0.2.0 is downloaded and checked. It installs when AIQ is idle (last try: AIQ home is open)."),
            "{t}"
        );
        let off = update_text(&View { mode: "off".into(), ..w.clone() }, 0);
        assert!(off.contains("Updates: off. AIQ never checks by itself."), "{off}");
        let ask = update_text(&View { mode: "ask".into(), ..w }, 0);
        assert!(ask.contains("Install it: aiq update --now"), "{ask}");
    }

    #[test]
    fn status_names_a_waiting_update_and_how_to_install_it() {
        use super::update_status_line;
        assert_eq!(update_status_line(&v()), None, "nothing waiting: no line");
        let w = View { note: "waiting", available: Some("v0.1.1".into()), ..v() };
        assert_eq!(
            update_status_line(&w).as_deref(),
            Some("Update: AIQ 0.1.1 is ready. It installs when AIQ home is closed. Install now: aiq update --now\n")
        );
        let ask = View { mode: "ask".into(), ..w.clone() };
        assert_eq!(
            update_status_line(&ask).as_deref(),
            Some("Update: AIQ 0.1.1 is ready. Install now: aiq update --now\n")
        );
        assert_eq!(update_status_line(&View { supported: false, ..w }), None, "a dev copy never updates");
    }

    #[test]
    fn failed_check_says_when_it_retries() {
        let t = update_text(&View { checked_ms: Some(0), check_error: Some("404".into()), ..v() }, 0);
        assert!(t.contains("That check failed (404); it tries again within the hour."), "{t}");
    }
}
