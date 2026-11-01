//! `aiq install|uninstall copilot` (Phase 2 design, Exposed contract 5; ADR-0005 rules 5, 5a, 6).
//! - Hooks: one whole file, `~/.copilot/hooks/aiq.json` (or `$COPILOT_HOME/hooks/aiq.json`), exec form,
//!   only `userPromptSubmitted` and `agentStop`. AIQ never edits another hook file or a repo's `.github/hooks/`.
//! - Status line: AIQ's `statusLine` in `~/.copilot/settings.json`, added with the Phase 1 rules (diff, ask,
//!   backup, temp-file-and-rename, refuse invalid JSON). An existing status line that is not AIQ's is never
//!   replaced: install prints the one command to add to it instead.
use crate::cli::confirm;
use crate::install::{line_diff, pretty, read_settings, write_settings};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const EVENTS: [&str; 2] = ["userPromptSubmitted", "agentStop"];
pub const HOOK_TIMEOUT_S: u64 = 5;
/// AIQ's own part of a Copilot hook (Imran, 2026-11-01): Copilot's start-up is measured, not counted.
const HOOK_BUDGET_MS: u128 = 100;

pub fn copilot_home() -> PathBuf {
    if let Some(d) = std::env::var_os("COPILOT_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default();
    home.join(".copilot")
}

pub fn hook_file(home: &Path) -> PathBuf {
    home.join("hooks").join("aiq.json")
}

pub fn hooks_json(exe: &str) -> Value {
    let mut hooks = serde_json::Map::new();
    for e in EVENTS {
        hooks.insert(
            e.into(),
            json!([{ "type": "command", "exec": exe, "args": ["hook", "copilot", e], "timeoutSec": HOOK_TIMEOUT_S }]),
        );
    }
    json!({ "version": 1, "hooks": hooks })
}

/// `assume:` Copilot runs `statusLine.command` as one command line (third-party setups pass a path, and
/// copilot-cli 1.0.11 expands `~` and env vars in it). Probe P1 confirms; if it takes a bare path only,
/// install writes a one-line wrapper script instead.
pub fn statusline_command(exe: &str) -> String {
    format!("\"{exe}\" statusline copilot")
}

pub fn is_aiq_statusline(v: &Value) -> bool {
    v.get("statusLine")
        .and_then(|s| s.get("command"))
        .and_then(Value::as_str)
        .is_some_and(|c| c.ends_with(" statusline copilot"))
}

/// Add AIQ's status line. Err(existing command) when the person already has one that is not AIQ's.
pub fn add_statusline(settings: &mut Value, exe: &str) -> Result<(), String> {
    if let Some(sl) = settings.get("statusLine").filter(|_| !is_aiq_statusline(settings)) {
        return Err(sl.get("command").and_then(Value::as_str).unwrap_or("(not a command)").to_owned());
    }
    settings
        .as_object_mut()
        .expect("checked object")
        .insert("statusLine".into(), json!({ "type": "command", "command": statusline_command(exe) }));
    Ok(())
}

pub fn strip_statusline(settings: &mut Value) -> bool {
    is_aiq_statusline(settings) && settings.as_object_mut().is_some_and(|o| o.remove("statusLine").is_some())
}

fn exe_path() -> Result<PathBuf, String> {
    std::env::current_exe()
        .and_then(|e| e.canonicalize())
        .map_err(|e| format!("Could not find the aiq executable: {e}"))
}

fn backup(p: &Path, text: &str) -> std::io::Result<PathBuf> {
    let b = p.with_extension(format!("json.aiq-backup-{}", chrono::Local::now().format("%Y%m%d%H%M%S")));
    std::fs::write(&b, text).map(|_| b)
}

pub fn status_line() -> String {
    let home = copilot_home();
    let hooks = hook_file(&home).exists();
    let sl = std::fs::read_to_string(home.join("settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .is_some_and(|v| is_aiq_statusline(&v));
    match (hooks, sl) {
        (true, true) => format!("installed in {}", home.display()),
        (true, false) => "hooks installed; status line is not AIQ's (tips show with `aiq show` only)".into(),
        _ => "not installed. Run: aiq install copilot".into(),
    }
}

pub fn install(yes: bool) -> ExitCode {
    let exe = match exe_path() {
        Ok(e) => e,
        Err(m) => {
            eprintln!("{m}");
            return ExitCode::from(1);
        }
    };
    let exe_s = exe.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    let home = copilot_home();
    let hp = hook_file(&home);
    let sp = home.join("settings.json");
    let (orig, mut v) = match read_settings(&sp) {
        Ok(x) => x,
        Err(m) => {
            eprintln!("{m}");
            return ExitCode::from(1);
        }
    };
    let before = v.clone();
    let foreign_sl = add_statusline(&mut v, &exe_s).err();
    let new_hooks = pretty(&hooks_json(&exe_s));
    let old_hooks = std::fs::read_to_string(&hp).ok();
    if v == before && old_hooks.as_deref() == Some(new_hooks.as_str()) && foreign_sl.is_none() {
        println!("Already installed.");
        return ExitCode::SUCCESS;
    }
    println!("AIQ will write {} (AIQ's own file):\n", hp.display());
    print!("{}", line_diff(old_hooks.as_deref().unwrap_or(""), &new_hooks));
    if v != before {
        println!("\nAIQ will change {}:\n", sp.display());
        print!("{}", line_diff(&orig.as_deref().map_or_else(String::new, |_| pretty(&before)), &pretty(&v)));
    }
    if let Some(cmd) = &foreign_sl {
        println!(
            "\nYou already have a Copilot status line ({cmd}). AIQ will not replace it.\nTo see AIQ tips in it, add \
             the output of this command to your status-line script:\n  {}",
            statusline_command(&exe_s)
        );
    }
    println!("\nYour other Copilot settings and hooks stay as they are.");
    if !confirm("Apply this change?", yes) {
        println!("Nothing changed.");
        return ExitCode::SUCCESS;
    }
    if v != before {
        if let Some(text) = &orig {
            match backup(&sp, text) {
                Ok(b) => println!("Backup: {}", b.display()),
                Err(e) => {
                    eprintln!("Could not write a backup ({e}); nothing changed.");
                    return ExitCode::from(1);
                }
            }
        }
        if let Err(e) = write_settings(&sp, &pretty(&v)) {
            eprintln!("Could not write {}: {e}", sp.display());
            return ExitCode::from(1);
        }
    }
    if let Err(e) = write_settings(&hp, &new_hooks) {
        eprintln!("Could not write {}: {e}", hp.display());
        return ExitCode::from(1);
    }
    println!("Installed. {}", self_test(&exe));
    ExitCode::SUCCESS
}

/// Run the installed agentStop entry once, as Copilot will (direct exec, no shell), and time it. It stores
/// one stop event for a synthetic session and shows nothing.
fn self_test(exe: &Path) -> String {
    use std::io::Write;
    let start = std::time::Instant::now();
    let child = std::process::Command::new(exe)
        .args(["hook", "copilot", "agentStop"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let Ok(mut child) = child else { return "Self-test could not start the hook.".into() };
    if let Some(mut i) = child.stdin.take() {
        let _ = i.write_all(br#"{"sessionId":"aiq-install-self-test","stopReason":"self-test"}"#);
    }
    match child.wait_with_output() {
        Ok(o) if o.status.success() && o.stdout.is_empty() && o.stderr.is_empty() => {
            let ms = start.elapsed().as_millis();
            let verdict = if ms <= HOOK_BUDGET_MS { "within" } else { "over" };
            format!(
                "Self-test: AIQ's Copilot hook ran in {ms} ms ({verdict} the {HOOK_BUDGET_MS} ms budget). Copilot adds its \
                 own start-up on top; `aiq status` shows real runs."
            )
        }
        _ => "Self-test: the hook did not exit cleanly and silently. Run `aiq status`.".into(),
    }
}

pub fn uninstall(yes: bool) -> ExitCode {
    let home = copilot_home();
    let hp = hook_file(&home);
    let sp = home.join("settings.json");
    let (orig, mut v) = match read_settings(&sp) {
        Ok(x) => x,
        Err(m) => {
            eprintln!("{m}");
            return ExitCode::from(1);
        }
    };
    let before = pretty(&v);
    let sl = strip_statusline(&mut v);
    let hooks = hp.exists();
    if !sl && !hooks {
        println!("AIQ is not installed in {}.", home.display());
        return ExitCode::SUCCESS;
    }
    if hooks {
        println!("AIQ will delete {} (AIQ's own file).", hp.display());
    }
    if sl {
        println!("AIQ will change {}:\n", sp.display());
        print!("{}", line_diff(&before, &pretty(&v)));
    }
    if !confirm("Remove AIQ from Copilot?", yes) {
        println!("Nothing changed.");
        return ExitCode::SUCCESS;
    }
    if sl
        && orig.is_some()
        && let Err(e) = write_settings(&sp, &pretty(&v))
    {
        eprintln!("Could not write {}: {e}", sp.display());
        return ExitCode::from(1);
    }
    if hooks && let Err(e) = std::fs::remove_file(&hp) {
        eprintln!("Could not delete {}: {e}", hp.display());
        return ExitCode::from(1);
    }
    println!("Removed AIQ from Copilot.");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_json_exec_form_two_events_only() {
        let v = hooks_json("/x/aiq");
        assert_eq!(v["version"], 1);
        let hooks = v["hooks"].as_object().unwrap();
        let mut keys: Vec<_> = hooks.keys().collect();
        keys.sort();
        assert_eq!(keys, ["agentStop", "userPromptSubmitted"]);
        for (e, list) in hooks {
            let h = &list[0];
            assert_eq!(h["exec"], "/x/aiq");
            assert_eq!(h["args"], json!(["hook", "copilot", e]));
            assert!(h.get("command").is_none() && h.get("powershell").is_none() && h.get("bash").is_none());
        }
    }

    #[test]
    fn add_statusline_never_replaces_a_foreign_one() {
        let mut v = json!({"model":"x","statusLine":{"type":"command","command":"~/.copilot/mine.sh"}});
        let before = v.clone();
        assert_eq!(add_statusline(&mut v, "/x/aiq").unwrap_err(), "~/.copilot/mine.sh");
        assert_eq!(v, before);
        assert!(!strip_statusline(&mut v));
        assert_eq!(v, before);
    }

    #[test]
    fn statusline_round_trip_leaves_other_settings() {
        let mut v = json!({"model":"x","experimental":true});
        let before = v.clone();
        add_statusline(&mut v, "/x/aiq").unwrap();
        assert!(is_aiq_statusline(&v));
        add_statusline(&mut v, "/y/aiq").unwrap(); // reinstall from a moved binary replaces our own
        assert_eq!(v["statusLine"]["command"], "\"/y/aiq\" statusline copilot");
        assert!(strip_statusline(&mut v));
        assert_eq!(v, before);
    }
}
