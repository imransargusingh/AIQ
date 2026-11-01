//! `aiq install|uninstall claude-code`: edit only AIQ's own entries in the user's Claude Code settings
//! (`~/.claude/settings.json`, or `$CLAUDE_CONFIG_DIR/settings.json`). Never a repo's `.claude/`.
//! Shows the change, asks, backs up, writes atomically. Exec form only (no shell; S3a: Git Bash ≈ 452 ms).
use crate::cli::confirm;
use aiq_core::paths::Paths;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const EVENTS: [(&str, bool); 2] = [("UserPromptSubmit", true), ("Stop", false)];

pub fn settings_path() -> PathBuf {
    if let Some(d) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d).join("settings.json");
    }
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default();
    home.join(".claude").join("settings.json")
}

/// One AIQ hook entry: `args[0..2] == ["hook", "claude-code"]` is how install and uninstall find their own.
fn is_aiq_hook(h: &Value) -> bool {
    let a = h.get("args").and_then(Value::as_array);
    a.is_some_and(|a| a.len() >= 2 && a[0] == "hook" && a[1] == "claude-code")
}

/// Remove AIQ's entries; drop containers that became empty. Returns true if anything was removed.
pub fn strip_aiq(settings: &mut Value) -> bool {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else { return false };
    let mut changed = false;
    for (_, groups) in hooks.iter_mut() {
        let Some(groups) = groups.as_array_mut() else { continue };
        for g in groups.iter_mut() {
            if let Some(list) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                let before = list.len();
                list.retain(|h| !is_aiq_hook(h));
                changed |= list.len() != before;
            }
        }
        groups.retain(|g| g.get("hooks").and_then(Value::as_array).is_none_or(|l| !l.is_empty()));
    }
    hooks.retain(|_, groups| groups.as_array().is_none_or(|g| !g.is_empty()));
    if hooks.is_empty() {
        settings.as_object_mut().map(|o| o.remove("hooks"));
    }
    changed
}

pub fn add_aiq(settings: &mut Value, exe: &str) {
    let obj = settings.as_object_mut().expect("checked object");
    let hooks = obj.entry("hooks").or_insert_with(|| Value::Object(Map::new()));
    if !hooks.is_object() {
        *hooks = Value::Object(Map::new());
    }
    for (event, is_async) in EVENTS {
        let mut h = json!({ "type": "command", "command": exe, "args": ["hook", "claude-code", event] });
        if is_async {
            h["async"] = json!(true);
        }
        h["timeout"] = json!(5);
        let groups = hooks.as_object_mut().unwrap().entry(event).or_insert_with(|| json!([]));
        if !groups.is_array() {
            *groups = json!([]);
        }
        groups.as_array_mut().unwrap().push(json!({ "hooks": [h] }));
    }
}

pub fn has_aiq(settings: &Value) -> bool {
    let mut c = settings.clone();
    strip_aiq(&mut c)
}

pub fn status_line() -> String {
    let p = settings_path();
    match std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        Some(v) if has_aiq(&v) => format!("installed in {}", p.display()),
        _ => "not installed. Run: aiq install claude-code".into(),
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).expect("json") + "\n"
}

/// Minimal line diff (LCS) for showing the change before asking.
/// Lines that differ only by a trailing comma count as unchanged: adding a key after the last
/// one gives the old last line a comma, and showing it as removed-and-added reads as a change.
pub fn line_diff(old: &str, new: &str) -> String {
    let (oa, ob): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    fn key<'a>(s: &&'a str) -> &'a str {
        s.trim_end_matches(',')
    }
    let (a, b): (Vec<&str>, Vec<&str>) = (oa.iter().map(key).collect(), ob.iter().map(key).collect());
    let mut l = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            l[i][j] = if a[i] == b[j] { l[i + 1][j + 1] + 1 } else { l[i + 1][j].max(l[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, String::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || l[i + 1][j] >= l[i][j + 1]) {
            out += &format!("- {}\n", oa[i]);
            i += 1;
        } else {
            out += &format!("+ {}\n", ob[j]);
            j += 1;
        }
    }
    out
}

#[derive(Serialize, Deserialize)]
struct InstallRecord {
    settings_path: PathBuf,
    existed: bool,
    backup: Option<PathBuf>,
    written: String,
}

fn read_settings(p: &Path) -> Result<(Option<String>, Value), String> {
    match std::fs::read_to_string(p) {
        Ok(t) => match serde_json::from_str::<Value>(&t) {
            Ok(v @ Value::Object(_)) => Ok((Some(t), v)),
            Ok(_) => Err(format!("{} is not a JSON object; nothing changed.", p.display())),
            Err(e) => Err(format!("{} is not valid JSON ({e}); nothing changed.", p.display())),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((None, json!({}))),
        Err(e) => Err(format!("Could not read {}: {e}", p.display())),
    }
}

/// Write via temp file + rename, keeping the original file's permissions.
fn write_settings(p: &Path, text: &str) -> std::io::Result<()> {
    let perms = std::fs::metadata(p).ok().map(|m| m.permissions());
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("json.aiq-tmp");
    std::fs::write(&tmp, text)?;
    if let Some(perms) = perms {
        std::fs::set_permissions(&tmp, perms)?;
    }
    std::fs::rename(&tmp, p)
}

pub fn install(yes: bool) -> ExitCode {
    let p = settings_path();
    let exe = match std::env::current_exe().and_then(|e| e.canonicalize()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Could not find the aiq executable: {e}");
            return ExitCode::from(1);
        }
    };
    let exe_s = exe.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    let (orig, mut v) = match read_settings(&p) {
        Ok(x) => x,
        Err(m) => {
            eprintln!("{m}");
            return ExitCode::from(1);
        }
    };
    let before = v.clone();
    strip_aiq(&mut v);
    add_aiq(&mut v, &exe_s);
    if v == before {
        println!("Already installed.");
        return ExitCode::SUCCESS;
    }
    println!("AIQ will change {}:\n", p.display());
    print!("{}", line_diff(&orig.as_deref().map_or_else(String::new, |_| pretty(&before)), &pretty(&v)));
    println!("\nOnly these two entries are AIQ's. Your other settings and hooks stay as they are.");
    if !confirm("Apply this change?", yes) {
        println!("Nothing changed.");
        return ExitCode::SUCCESS;
    }
    let paths = Paths::from_env();
    // Reinstall over our own untouched install (the binary moved): keep the first record's backup,
    // the person's real pre-AIQ file. A fresh backup would hold the old AIQ hooks (DC-006).
    let prior = read_record(&paths, &p).filter(|r| orig.as_deref() == Some(r.written.as_str()));
    let (existed, backup) = match (prior, &orig) {
        (Some(r), _) => {
            if let Some(b) = &r.backup {
                println!("Backup kept from the first install: {}", b.display());
            }
            (r.existed, r.backup)
        }
        (None, Some(text)) => {
            let b = p.with_extension(format!("json.aiq-backup-{}", chrono::Local::now().format("%Y%m%d%H%M%S")));
            if let Err(e) = std::fs::write(&b, text) {
                eprintln!("Could not write a backup ({e}); nothing changed.");
                return ExitCode::from(1);
            }
            println!("Backup: {}", b.display());
            (true, Some(b))
        }
        (None, None) => (false, None),
    };
    let written = pretty(&v);
    if let Err(e) = write_settings(&p, &written) {
        eprintln!("Could not write {}: {e}", p.display());
        return ExitCode::from(1);
    }
    let _ = paths.ensure_dirs();
    let rec = InstallRecord { settings_path: p.clone(), existed, backup, written };
    let _ = aiq_core::paths::write_user_only(&paths.install_record(), serde_json::to_string(&rec).unwrap().as_bytes());
    println!("Installed. {}", self_test(&exe));
    ExitCode::SUCCESS
}

/// Whole-process p95 budget for the Stop hook (Imran, 2026-11-01; note-20261101-hook-budget-100ms).
const STOP_BUDGET_MS: u128 = 100;

/// Run the installed Stop entry once, the way Claude Code will (direct exec, no shell), and time it.
fn self_test(exe: &Path) -> String {
    use std::io::Write;
    let start = std::time::Instant::now();
    let child = std::process::Command::new(exe)
        .args(["hook", "claude-code", "Stop"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return "Self-test could not start the hook.".into() };
    if let Some(mut i) = child.stdin.take() {
        let _ =
            i.write_all(br#"{"session_id":"aiq-install-self-test","hook_event_name":"Stop","stop_hook_active":true}"#);
    }
    match child.wait_with_output() {
        Ok(o) if o.status.success() => {
            let ms = start.elapsed().as_millis();
            if ms <= STOP_BUDGET_MS {
                format!("Self-test: the Stop hook ran in {ms} ms (budget {STOP_BUDGET_MS} ms).")
            } else {
                format!(
                    "Self-test: the Stop hook ran in {ms} ms, over the {STOP_BUDGET_MS} ms budget. The first run of a new aiq \
                     binary is often slow while the system scans it; `aiq status` shows the times of real runs."
                )
            }
        }
        _ => "Self-test: the hook did not exit cleanly. Run `aiq status`.".into(),
    }
}

fn read_record(paths: &Paths, settings: &Path) -> Option<InstallRecord> {
    std::fs::read_to_string(paths.install_record())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(|r: &InstallRecord| r.settings_path == settings)
}

pub fn uninstall(yes: bool) -> ExitCode {
    let p = settings_path();
    let paths = Paths::from_env();
    let (orig, mut v) = match read_settings(&p) {
        Ok(x) => x,
        Err(m) => {
            eprintln!("{m}");
            return ExitCode::from(1);
        }
    };
    let rec = read_record(&paths, &p);
    let current = orig.clone().unwrap_or_default();
    // Untouched since install: restore the exact bytes we replaced (byte-identical round trip),
    // but never a backup that itself still holds AIQ hooks (DC-006); strip instead.
    let restore: Option<Option<String>> = rec.as_ref().filter(|r| r.written == current).and_then(|r| match &r.backup {
        Some(b) => std::fs::read_to_string(b)
            .ok()
            .filter(|t| !serde_json::from_str::<Value>(t).is_ok_and(|bv| has_aiq(&bv)))
            .map(Some),
        None if !r.existed => Some(None),
        None => None,
    });
    let new_text: Option<String> = match restore {
        Some(t) => t,
        None => {
            if !strip_aiq(&mut v) {
                println!("AIQ's hooks are not installed in {}.", p.display());
                return ExitCode::SUCCESS;
            }
            Some(pretty(&v))
        }
    };
    println!("AIQ will change {}:\n", p.display());
    print!("{}", line_diff(&current, new_text.as_deref().unwrap_or("")));
    if !confirm("Remove AIQ's hooks?", yes) {
        println!("Nothing changed.");
        return ExitCode::SUCCESS;
    }
    let r = match &new_text {
        Some(t) => write_settings(&p, t),
        None => std::fs::remove_file(&p),
    };
    if let Err(e) = r {
        eprintln!("Could not write {}: {e}", p.display());
        return ExitCode::from(1);
    }
    let _ = std::fs::remove_file(paths.install_record());
    println!("Removed AIQ's hooks.");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn other_hooks() -> Value {
        json!({
          "model": "opus",
          "hooks": {
            "Stop": [{ "hooks": [{ "type": "command", "command": "notify-send done" }] }],
            "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "guard.sh" }] }]
          }
        })
    }

    #[test]
    fn add_aiq_entries_are_exec_form_with_args() {
        let mut v = json!({});
        add_aiq(&mut v, "/opt/aiq");
        for (event, is_async) in EVENTS {
            let h = &v["hooks"][event][0]["hooks"][0];
            assert_eq!(h["command"], "/opt/aiq");
            assert_eq!(h["args"], json!(["hook", "claude-code", event]));
            assert_eq!(h.get("async").is_some(), is_async);
            assert!(!h["command"].as_str().unwrap().contains(' '), "no shell form");
        }
    }

    #[test]
    fn strip_aiq_keeps_other_hooks_and_order() {
        let orig = other_hooks();
        let mut v = orig.clone();
        add_aiq(&mut v, "/opt/aiq");
        assert!(has_aiq(&v));
        assert!(strip_aiq(&mut v));
        assert_eq!(v, orig);
        assert_eq!(serde_json::to_string(&v).unwrap(), serde_json::to_string(&orig).unwrap(), "key order kept");
    }

    #[test]
    fn strip_aiq_empty_settings_removes_hooks_key() {
        let mut v = json!({});
        add_aiq(&mut v, "/opt/aiq");
        strip_aiq(&mut v);
        assert_eq!(v, json!({}));
        assert!(!strip_aiq(&mut v));
    }

    #[test]
    fn line_diff_marks_added_and_removed() {
        assert_eq!(line_diff("a\nb\nc\n", "a\nx\nc\n"), "- b\n+ x\n");
        assert_eq!(line_diff("", "a\n"), "+ a\n");
    }

    #[test]
    fn line_diff_ignores_a_line_that_only_gained_a_comma() {
        let old = "{\n  \"theme\": \"dark\"\n}\n";
        let new = "{\n  \"theme\": \"dark\",\n  \"hooks\": {}\n}\n";
        assert_eq!(line_diff(old, new), "+   \"hooks\": {}\n");
    }
}
