//! AIQ home desktop shell (designs `docs/design/aiq-home-app.md`, `aiq-home-deferred-pages.md`). Ten
//! commands, each a thin call (the list is checked against build.rs and the capability file by a test):
//! reads go to the `aiq-home` read model (read-only store), changes go through `aiq_core::control` to the
//! core, the only writer. Each command's time and outcome is logged locally to `<data>/home.log`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use aiq_core::control::{self, ControlError};
use aiq_core::pack::BundledPack;
use aiq_core::paths::Paths;
use aiq_domain::PackSource;
use aiq_domain::clock::Clock;
use aiq_home::Snapshot;
use serde::Serialize;
use std::time::{Duration, Instant};

const CORE_TIMEOUT: Duration = Duration::from_secs(3);
const LOG_MAX: u64 = 256 * 1024;
const NOT_RUNNING: &str =
    "AIQ isn't running, so this can't change right now. It starts with your next prompt in Claude Code or Copilot.";

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Outcome {
    Ok { ok: bool },
    Err { ok: bool, message: String },
}

impl Outcome {
    fn from(r: Result<(), String>) -> Self {
        match r {
            Ok(()) => Outcome::Ok { ok: true },
            Err(message) => Outcome::Err { ok: false, message },
        }
    }
}

fn log(paths: &Paths, cmd: &str, started: Instant, result: &str) {
    let line = format!(
        "{} cmd={cmd} ms={} {result}\n",
        aiq_core::sysclock::format_rfc3339_ms(aiq_core::sysclock::SystemClock.now().utc_ms),
        started.elapsed().as_millis()
    );
    let _ = aiq_core::paths::bounded_append(&paths.data.join("home.log"), &line, LOG_MAX);
}

fn read(paths: &Paths) -> Snapshot {
    let pack = BundledPack.load().expect("bundled pack is validated at build time by aiq-core's tests");
    let s = aiq_home::snapshot(paths, &pack, aiq_core::sysclock::SystemClock.now());
    aiq_home::with_core(s, paths)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn snapshot() -> Result<Snapshot, String> {
    blocking(|| {
        let t = Instant::now();
        let paths = Paths::from_env();
        let s = read(&paths);
        let r = match &s.status.error {
            Some(code) => format!("error={code}"),
            None => format!("ok read_ms={} open_ms={} core={}", s.status.read_ms, s.status.open_ms, s.status.core),
        };
        log(&paths, "snapshot", t, &r);
        s
    })
    .await
}

#[tauri::command]
async fn pause(on: bool) -> Result<Snapshot, String> {
    blocking(move || {
        let t = Instant::now();
        let paths = Paths::from_env();
        let r = control::set_paused(&paths, on, CORE_TIMEOUT);
        log(&paths, "pause", t, &format!("on={on} {}", if r.is_ok() { "ok" } else { "error" }));
        r.map_err(|e| format!("Could not change the pause setting: {e}"))?;
        Ok(read(&paths))
    })
    .await?
}

/// Only the settings AIQ home shows can be changed from it; the core validates the value.
fn allowed_setting(key: &str) -> bool {
    matches!(key, "daily_cap" | "copilot_tip_stays" | "next_rung_tips")
}

#[tauri::command]
async fn set_setting(key: String, value: String) -> Result<Outcome, String> {
    blocking(move || {
        let t = Instant::now();
        let paths = Paths::from_env();
        let r = if !allowed_setting(&key) {
            Err(format!("AIQ home can't change {key}."))
        } else {
            control::set_setting(&paths, &key, &value, CORE_TIMEOUT).map_err(|e| match e {
                ControlError::NotRunning => NOT_RUNNING.to_owned(),
                ControlError::Refused(m) => format!("Not changed: {m}"),
                ControlError::Io(m) => m,
            })
        };
        log(&paths, "set_setting", t, &format!("key={key} {}", if r.is_ok() { "ok" } else { "refused" }));
        Outcome::from(r)
    })
    .await
}

/// Delete everything. The typed word is checked here too, not only in the page (design: STRIDE, R/T).
#[tauri::command]
async fn forget_all(confirm: String) -> Result<Outcome, String> {
    blocking(move || {
        let t = Instant::now();
        let paths = Paths::from_env();
        let r = if confirm != "forget" {
            Err("Type forget to confirm.".to_owned())
        } else {
            control::forget_all(&paths, CORE_TIMEOUT).map_err(|e| match e {
                ControlError::NotRunning => NOT_RUNNING.to_owned(),
                ControlError::Refused(m) | ControlError::Io(m) => format!("Could not delete everything: {m}"),
            })
        };
        log(&paths, "forget_all", t, if r.is_ok() { "ok" } else { "refused" });
        Outcome::from(r)
    })
    .await
}

/// One task with its prompts' text, for the task detail page. None when it is no longer listed.
#[tauri::command]
async fn task(task_id: String) -> Result<Option<aiq_home::TaskDetail>, String> {
    blocking(move || {
        let t = Instant::now();
        let paths = Paths::from_env();
        let pack = BundledPack.load().expect("bundled pack is validated at build time by aiq-core's tests");
        let d = aiq_home::task_detail(&paths, &pack, aiq_core::sysclock::SystemClock.now(), &task_id);
        log(&paths, "task", t, if d.is_some() { "ok" } else { "not_found" });
        d
    })
    .await
}

/// Delete one prompt's text (design aiq-home-deferred-pages E1–E3). The log names the outcome only, never
/// the id's text.
#[tauri::command]
async fn forget_item(event_id: String) -> Result<Outcome, String> {
    blocking(move || {
        let t = Instant::now();
        let paths = Paths::from_env();
        let r = control::forget_item(&paths, &event_id, CORE_TIMEOUT).map_err(|e| match e {
            ControlError::NotRunning => NOT_RUNNING.to_owned(),
            ControlError::Refused(m) => format!("Not deleted: {m}."),
            ControlError::Io(m) => format!("Could not delete it: {m}"),
        });
        log(&paths, "forget_item", t, if r.is_ok() { "ok" } else { "refused" });
        Outcome::from(r)
    })
    .await
}

/// "This doesn't fit" (action `flag` with a reason) or its undo, for one kind of task's next step
/// (design coaching-from-rungs D8). Through the core when it runs, else directly in the store.
#[tauri::command]
async fn guidance_flag(
    job: String,
    task_type: String,
    move_id: String,
    action: String,
    reason: Option<String>,
) -> Result<Outcome, String> {
    blocking(move || {
        let t = Instant::now();
        let paths = Paths::from_env();
        let r =
            control::guidance_flag(&paths, &job, &task_type, &move_id, &action, reason.as_deref(), None, CORE_TIMEOUT)
                .map_err(|e| match e {
                    ControlError::NotRunning => NOT_RUNNING.to_owned(),
                    ControlError::Refused(m) => format!("Not changed: {m}"),
                    ControlError::Io(m) => m,
                });
        log(&paths, "guidance_flag", t, &format!("action={action} {}", if r.is_ok() { "ok" } else { "refused" }));
        Outcome::from(r)
    })
    .await
}

/// "Turn back on" for a tip the person turned off.
#[tauri::command]
async fn tip_on(tip_id: String) -> Result<Outcome, String> {
    blocking(move || {
        let t = Instant::now();
        let paths = Paths::from_env();
        let r = control::tip_feedback(&paths, None, Some(&tip_id), "on", "home", CORE_TIMEOUT).map_err(|e| match e {
            ControlError::NotRunning => NOT_RUNNING.to_owned(),
            ControlError::Refused(m) => format!("Not changed: {m}"),
            ControlError::Io(m) => m,
        });
        log(&paths, "tip_on", t, if r.is_ok() { "ok" } else { "refused" });
        Outcome::from(r)
    })
    .await
}

/// The `aiq` binary beside AIQ home (both live in `<data>/bin`), else `aiq` on PATH.
fn aiq_exe() -> std::path::PathBuf {
    let name = if cfg!(windows) { "aiq.exe" } else { "aiq" };
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(name)))
        .filter(|p| p.exists())
        .unwrap_or_else(|| name.into())
}

/// What `connect_tools` did: the files `aiq install` reports it changed, or why it failed.
#[derive(Debug, Serialize)]
struct Connected {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    changed: Vec<String>,
}

/// The `Changed: <path>` lines `aiq install` ends with.
fn changed_lines(stdout: &str) -> Vec<String> {
    stdout.lines().filter_map(|l| l.strip_prefix("Changed: ")).map(str::to_string).collect()
}

/// Setup connects every tool it found (Imran, 2026-11-02): runs `aiq install --yes`, the same install the
/// CLI runs (diff, backup, self-test), so AIQ home never writes another tool's settings itself.
#[tauri::command]
async fn connect_tools() -> Result<Connected, String> {
    blocking(|| {
        let t = Instant::now();
        let paths = Paths::from_env();
        let mut c = std::process::Command::new(aiq_exe());
        c.args(["install", "--yes"]).stdin(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashes up
        }
        let r = match c.output() {
            Ok(o) => {
                let changed = changed_lines(&String::from_utf8_lossy(&o.stdout));
                let why = (!o.status.success())
                    .then(|| String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("").to_string());
                (changed, why)
            }
            Err(e) => (Vec::new(), Some(format!("Could not run aiq ({e})."))),
        };
        let (changed, why) = r;
        log(
            &paths,
            "connect_tools",
            t,
            &format!("{} changed={}", if why.is_none() { "ok" } else { "error" }, changed.len()),
        );
        Connected {
            ok: why.is_none(),
            message: why
                .map(|m| format!("AIQ couldn't connect your tools. {m} Run aiq install in a terminal to see why.")),
            changed,
        }
    })
    .await
}

/// First-run setup finished: AIQ home opens on Home from now on.
#[tauri::command]
async fn finish_setup() -> Result<Snapshot, String> {
    blocking(|| {
        let t = Instant::now();
        let paths = Paths::from_env();
        let r = control::finish_setup(&paths);
        log(&paths, "finish_setup", t, if r.is_ok() { "ok" } else { "error" });
        r.map_err(|e| format!("Could not save that setup is done: {e}"))?;
        Ok(read(&paths))
    })
    .await?
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            snapshot,
            pause,
            set_setting,
            forget_all,
            task,
            forget_item,
            finish_setup,
            guidance_flag,
            tip_on,
            connect_tools
        ])
        .run(tauri::generate_context!())
        .expect("AIQ home could not start its window");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_shown_settings_can_change() {
        assert!(allowed_setting("daily_cap"));
        assert!(allowed_setting("copilot_tip_stays"));
        assert!(allowed_setting("next_rung_tips"));
        assert!(!allowed_setting("paused"));
        assert!(!allowed_setting("anything"));
    }

    /// Every command the window can call is in generate_handler!, build.rs and the capability file, so a new
    /// command can't be built but denied at run time (guidance_flag and tip_on were missing from both).
    #[test]
    fn handler_build_manifest_and_capability_list_the_same_commands() {
        let src = include_str!("main.rs");
        let start = src.find("generate_handler![").unwrap() + "generate_handler![".len();
        let handler: Vec<&str> =
            src[start..].split(']').next().unwrap().split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        let build = include_str!("../build.rs");
        let cap: serde_json::Value = serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
        let perms: Vec<String> =
            cap["permissions"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect();
        for c in &handler {
            assert!(build.contains(&format!("\"{c}\"")), "{c} missing from build.rs");
            assert!(
                perms.contains(&format!("allow-{}", c.replace('_', "-"))),
                "{c} missing from capabilities/main.json"
            );
        }
        assert_eq!(perms.len(), handler.len(), "capability file allows a command that doesn't exist");
    }

    #[test]
    fn connect_reports_only_the_changed_lines() {
        let out = "GitHub Copilot:\nAIQ will write C:\\x\\aiq.json\nUndo: aiq uninstall copilot\n\nChanged: C:\\x\\aiq.json\n";
        assert_eq!(changed_lines(out), vec![r"C:\x\aiq.json".to_string()]);
        let c = Connected { ok: true, message: None, changed: vec!["a".into()] };
        assert_eq!(serde_json::to_string(&c).unwrap(), r#"{"ok":true,"changed":["a"]}"#);
    }

    #[test]
    fn outcome_wire_shape_matches_the_ui_type() {
        assert_eq!(serde_json::to_string(&Outcome::from(Ok(()))).unwrap(), r#"{"ok":true}"#);
        assert_eq!(serde_json::to_string(&Outcome::from(Err("x".into()))).unwrap(), r#"{"ok":false,"message":"x"}"#);
    }
}
