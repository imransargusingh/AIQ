//! AIQ home desktop shell (design `docs/design/aiq-home-app.md`). Four commands, each a thin call:
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
            None => format!("ok read_ms={} core={}", s.status.read_ms, s.status.core),
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
    matches!(key, "daily_cap" | "copilot_tip_stays")
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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![snapshot, pause, set_setting, forget_all])
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
        assert!(!allowed_setting("paused"));
        assert!(!allowed_setting("anything"));
    }

    #[test]
    fn outcome_wire_shape_matches_the_ui_type() {
        assert_eq!(serde_json::to_string(&Outcome::from(Ok(()))).unwrap(), r#"{"ok":true}"#);
        assert_eq!(serde_json::to_string(&Outcome::from(Err("x".into()))).unwrap(), r#"{"ok":false,"message":"x"}"#);
    }
}
