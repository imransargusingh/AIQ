//! `aiq install|uninstall copilot` (Phase 2 design, Exposed contract 5; ADR-0005 rules 5, 5a, 6).
//! - Hooks: one whole file, `~/.copilot/hooks/aiq.json` (or `$COPILOT_HOME/hooks/aiq.json`), exec form,
//!   only `userPromptSubmitted` and `agentStop`. AIQ never edits another hook file or a repo's `.github/hooks/`.
//! - Status line: AIQ's `statusLine` in `~/.copilot/settings.json`, added with the Phase 1 rules (diff, ask,
//!   backup, temp-file-and-rename, refuse invalid JSON). An existing status line that is not AIQ's is never
//!   replaced: install prints the one command to add to it instead.
//! - Copilot runs a status line only with `experimental: true` and `STATUS_LINE` in `feature_flags.enabled`
//!   (the settings laptop probe run 2 used; Copilot CLI 1.0.105). Install adds whichever is missing and
//!   records what it changed in AIQ's data folder, so uninstall puts back exactly that and nothing else.
use crate::cli::confirm;
use crate::install::{line_diff, pretty, read_settings, write_settings};
use aiq_core::paths::Paths;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const EVENTS: [&str; 2] = ["userPromptSubmitted", "agentStop"];
pub const HOOK_TIMEOUT_S: u64 = 5;
/// AIQ's own part of a Copilot hook (Imran, 2026-11-01): Copilot's start-up is measured, not counted.
const HOOK_BUDGET_MS: u128 = 100;

pub use aiq_core::tools::copilot_home;

pub fn hook_file(home: &Path) -> PathBuf {
    aiq_core::tools::copilot_hook_file(home)
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

/// AIQ's own one-line wrapper in the Copilot home (Windows only).
pub const WRAPPER: &str = "aiq-statusline.cmd";
pub const STATUS_FLAG: &str = "STATUS_LINE";

pub fn wrapper_file(home: &Path) -> PathBuf {
    home.join(WRAPPER)
}

pub fn wrapper_text(exe: &str) -> String {
    format!("@\"{exe}\" statusline copilot\r\n")
}

/// The command line a person adds to their own status-line script.
pub fn direct_command(exe: &str) -> String {
    format!("\"{exe}\" statusline copilot")
}

/// The `statusLine.command` AIQ installs. Windows: a bare path to a one-line `.cmd` wrapper, the shape laptop
/// probe run 2 proved (Copilot CLI 1.0.105 ran it 16 times, refreshing about 1 s after each `agentStop`).
/// Elsewhere: the quoted executable and its arguments.
/// assume: (Windows) a wrapper path that contains spaces still runs; the probe's path had none. Confirm: a
/// person whose profile path has a space. If false: no tip in the status line; hooks and `aiq show` still work.
/// assume: (macOS, Linux) Copilot runs the command through a shell. Unverified; not the alpha platform.
pub fn statusline_command(exe: &str, home: &Path, windows: bool) -> String {
    if windows { wrapper_file(home).to_string_lossy().into_owned() } else { direct_command(exe) }
}

pub fn is_aiq_statusline(v: &Value) -> bool {
    v.get("statusLine")
        .and_then(|s| s.get("command"))
        .and_then(Value::as_str)
        .is_some_and(|c| c.ends_with(" statusline copilot") || c.ends_with(WRAPPER))
}

/// What install changed to switch Copilot's status line on, so uninstall reverts only that.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone)]
pub struct FeatureRecord {
    /// Some(previous value) when AIQ set `experimental: true`; Some(Null) when the key was absent.
    pub experimental_before: Option<Value>,
    pub added_status_flag: bool,
    /// File only: the key was absent. JSON writes Some(Null) and None both as `null`, so without this a
    /// reinstall record lost "absent" and uninstall left `experimental: true` behind. A record written
    /// before this field reads as before (`null` = not changed), so it never removes a setting AIQ did not add.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    experimental_absent: bool,
}

impl FeatureRecord {
    fn to_file(&self) -> String {
        let f = FeatureRecord { experimental_absent: self.experimental_before == Some(Value::Null), ..self.clone() };
        serde_json::to_string(&f).unwrap_or_default()
    }

    fn from_file(text: &str) -> Option<Self> {
        let f: FeatureRecord = serde_json::from_str(text).ok()?;
        let experimental_before = if f.experimental_absent { Some(Value::Null) } else { f.experimental_before };
        Some(FeatureRecord { experimental_before, added_status_flag: f.added_status_flag, experimental_absent: false })
    }
}

/// Switch on `experimental` and the `STATUS_LINE` flag. Never clobbers a `feature_flags` or `enabled` of
/// another shape (then it leaves them and the person turns the status line on with `/statusline`).
pub fn enable_status_feature(settings: &mut Value) -> FeatureRecord {
    let o = settings.as_object_mut().expect("checked object");
    let mut rec = FeatureRecord::default();
    if o.get("experimental") != Some(&Value::Bool(true)) {
        rec.experimental_before = Some(o.get("experimental").cloned().unwrap_or(Value::Null));
        o.insert("experimental".into(), Value::Bool(true));
    }
    let ff = o.entry("feature_flags").or_insert_with(|| json!({}));
    if let Some(ff) = ff.as_object_mut()
        && let Some(en) = ff.entry("enabled").or_insert_with(|| json!([])).as_array_mut()
        && !en.iter().any(|f| f == STATUS_FLAG)
    {
        en.push(STATUS_FLAG.into());
        rec.added_status_flag = true;
    }
    rec
}

/// Undo exactly what `rec` says install changed; drop `enabled` / `feature_flags` if that left them empty.
pub fn revert_status_feature(settings: &mut Value, rec: &FeatureRecord) {
    let Some(o) = settings.as_object_mut() else { return };
    match &rec.experimental_before {
        Some(Value::Null) => {
            o.remove("experimental");
        }
        Some(v) => {
            o.insert("experimental".into(), v.clone());
        }
        None => {}
    }
    if rec.added_status_flag
        && let Some(ff) = o.get_mut("feature_flags").and_then(Value::as_object_mut)
    {
        if let Some(en) = ff.get_mut("enabled").and_then(Value::as_array_mut) {
            en.retain(|f| f != STATUS_FLAG);
            if en.is_empty() {
                ff.remove("enabled");
            }
        }
        if ff.is_empty() {
            o.remove("feature_flags");
        }
    }
}

fn read_record(paths: &Paths) -> FeatureRecord {
    std::fs::read_to_string(paths.copilot_install_record())
        .ok()
        .and_then(|t| FeatureRecord::from_file(&t))
        .unwrap_or_default()
}

/// Add AIQ's status line. Err(existing command) when the person already has one that is not AIQ's.
pub fn add_statusline(settings: &mut Value, command: &str) -> Result<(), String> {
    if let Some(sl) = settings.get("statusLine").filter(|_| !is_aiq_statusline(settings)) {
        return Err(sl.get("command").and_then(Value::as_str).unwrap_or("(not a command)").to_owned());
    }
    settings
        .as_object_mut()
        .expect("checked object")
        .insert("statusLine".into(), json!({ "type": "command", "command": command }));
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

/// `with_status_line` false (`--no-status-line`): hooks only. Tips then show in `aiq show`, AIQ home and VS
/// Code, never in Copilot CLI's status line; an AIQ status line from an earlier install is taken out (and
/// `experimental` / the flag put back as that install found them).
pub fn install(yes: bool, with_status_line: bool) -> ExitCode {
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
    let windows = cfg!(windows);
    let paths = Paths::from_env();
    let (foreign_sl, rec, dropped_sl) = if with_status_line {
        let foreign = add_statusline(&mut v, &statusline_command(&exe_s, &home, windows)).err();
        let rec = if foreign.is_none() { enable_status_feature(&mut v) } else { FeatureRecord::default() };
        (foreign, rec, false)
    } else {
        let dropped = strip_statusline(&mut v);
        if dropped {
            revert_status_feature(&mut v, &read_record(&paths));
        }
        (None, FeatureRecord::default(), dropped)
    };
    let new_hooks = pretty(&hooks_json(&exe_s));
    let old_hooks = std::fs::read_to_string(&hp).ok();
    let wp = wrapper_file(&home);
    let new_wrapper = (windows && with_status_line && foreign_sl.is_none()).then(|| wrapper_text(&exe_s));
    let stale_wrapper = !with_status_line && wp.exists();
    let wrapper_ok = (new_wrapper.is_none() || std::fs::read_to_string(&wp).ok() == new_wrapper) && !stale_wrapper;
    if v == before && old_hooks.as_deref() == Some(new_hooks.as_str()) && foreign_sl.is_none() && wrapper_ok {
        println!("Already installed.");
        return ExitCode::SUCCESS;
    }
    println!("AIQ will write {} (AIQ's own file):\n", hp.display());
    print!("{}", line_diff(old_hooks.as_deref().unwrap_or(""), &new_hooks));
    if let Some(w) = new_wrapper.as_deref().filter(|_| !wrapper_ok) {
        println!("\nAIQ will write {} (AIQ's own file, the status-line command):\n  {}", wp.display(), w.trim_end());
    }
    if v != before {
        println!("\nAIQ will change {}:\n", sp.display());
        print!("{}", line_diff(&orig.as_deref().map_or_else(String::new, |_| pretty(&before)), &pretty(&v)));
        if rec.experimental_before.is_some() {
            // Security review S11: the switch is global, so say what it really does.
            println!(
                "\nNote: Copilot shows a status line only in its `experimental` mode. Turning it on switches on \
                 ALL of Copilot's experimental features in every Copilot session, not only AIQ's status line. \
                 `aiq uninstall copilot` turns it back off."
            );
        } else if rec != FeatureRecord::default() {
            println!("\n(Copilot shows a status line only with the STATUS_LINE flag; uninstall turns it off again.)");
        }
    }
    if let Some(cmd) = &foreign_sl {
        println!(
            "\nYou already have a Copilot status line ({cmd}). AIQ will not replace it.\nTo see AIQ tips in it, add \
             the output of this command to your status-line script:\n  {}",
            direct_command(&exe_s)
        );
    }
    if stale_wrapper {
        println!("\nAIQ will delete {} (AIQ's own file, the old status-line command).", wp.display());
    }
    if !with_status_line {
        println!(
            "\nNo status line (--no-status-line): AIQ tips will not show in Copilot CLI. Read them with `aiq show`, \
             in AIQ home, or in VS Code. Run `aiq install copilot` again to add the status line later."
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
        if dropped_sl {
            let _ = std::fs::remove_file(paths.copilot_install_record());
        }
        if rec != FeatureRecord::default() {
            let old = read_record(&paths); // a reinstall keeps what the first install changed
            let merged = FeatureRecord {
                experimental_before: old.experimental_before.or(rec.experimental_before),
                added_status_flag: old.added_status_flag || rec.added_status_flag,
                ..Default::default()
            };
            let _ = paths.ensure_dirs();
            let json = merged.to_file();
            if let Err(e) = aiq_core::paths::write_user_only(&paths.copilot_install_record(), json.as_bytes()) {
                eprintln!("Could not record the change ({e}); uninstall will leave `experimental` and the flag on.");
            }
        }
    }
    if stale_wrapper && let Err(e) = std::fs::remove_file(&wp) {
        eprintln!("Could not delete {}: {e}", wp.display());
        return ExitCode::from(1);
    }
    if let Some(w) = new_wrapper.filter(|_| !wrapper_ok)
        && let Err(e) = std::fs::write(&wp, w)
    {
        eprintln!("Could not write {}: {e}", wp.display());
        return ExitCode::from(1);
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
    let paths = Paths::from_env();
    let rec = read_record(&paths);
    let sl = strip_statusline(&mut v);
    if sl {
        revert_status_feature(&mut v, &rec);
    }
    let hooks = hp.exists();
    let wp = wrapper_file(&home);
    let wrapper = wp.exists();
    if !sl && !hooks && !wrapper {
        println!("AIQ is not installed in {}.", home.display());
        return ExitCode::SUCCESS;
    }
    for f in [hooks.then_some(&hp), wrapper.then_some(&wp)].into_iter().flatten() {
        println!("AIQ will delete {} (AIQ's own file).", f.display());
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
    if wrapper && let Err(e) = std::fs::remove_file(&wp) {
        eprintln!("Could not delete {}: {e}", wp.display());
        return ExitCode::from(1);
    }
    if sl {
        let _ = std::fs::remove_file(paths.copilot_install_record());
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
        assert_eq!(add_statusline(&mut v, &direct_command("/x/aiq")).unwrap_err(), "~/.copilot/mine.sh");
        assert_eq!(v, before);
        assert!(!strip_statusline(&mut v));
        assert_eq!(v, before);
    }

    #[test]
    fn statusline_round_trip_leaves_other_settings() {
        let mut v = json!({"model":"x","experimental":true});
        let before = v.clone();
        add_statusline(&mut v, &direct_command("/x/aiq")).unwrap();
        assert!(is_aiq_statusline(&v));
        add_statusline(&mut v, &direct_command("/y/aiq")).unwrap(); // reinstall from a moved binary replaces our own
        assert_eq!(v["statusLine"]["command"], "\"/y/aiq\" statusline copilot");
        assert!(strip_statusline(&mut v));
        assert_eq!(v, before);
    }

    #[test]
    fn statusline_command_windows_is_the_probed_wrapper_path() {
        let home = Path::new(r"C:\Users\p\.copilot");
        let c = statusline_command(r"C:\bin\aiq.exe", home, true);
        assert_eq!(c, home.join(WRAPPER).to_string_lossy());
        assert!(is_aiq_statusline(&json!({"statusLine": {"command": c}})));
        assert_eq!(wrapper_text(r"C:\bin\aiq.exe"), "@\"C:\\bin\\aiq.exe\" statusline copilot\r\n");
        assert_eq!(statusline_command("/x/aiq", home, false), "\"/x/aiq\" statusline copilot");
    }

    #[test]
    fn status_feature_round_trips_exactly() {
        for original in [
            json!({"model": "x"}),
            json!({"experimental": false, "feature_flags": {"enabled": ["OTHER"], "disabled": []}}),
            json!({"experimental": true, "feature_flags": {"enabled": ["STATUS_LINE"]}}),
        ] {
            let mut v = original.clone();
            let rec = enable_status_feature(&mut v);
            assert_eq!(v["experimental"], true);
            assert!(v["feature_flags"]["enabled"].as_array().unwrap().iter().any(|f| f == STATUS_FLAG));
            assert_eq!(enable_status_feature(&mut v.clone()), FeatureRecord::default(), "second run adds nothing");
            revert_status_feature(&mut v, &rec);
            assert_eq!(v, original);
        }
    }

    #[test]
    fn feature_record_keeps_absent_through_its_file() {
        for rec in [
            FeatureRecord { experimental_before: Some(Value::Null), added_status_flag: true, ..Default::default() },
            FeatureRecord { experimental_before: Some(json!(false)), added_status_flag: false, ..Default::default() },
            FeatureRecord::default(),
        ] {
            assert_eq!(FeatureRecord::from_file(&rec.to_file()).unwrap(), rec);
        }
        // A record from an earlier build: `null` meant "not changed" (the old reading), so it stays that way.
        let old = FeatureRecord::from_file(r#"{"experimental_before":null,"added_status_flag":true}"#).unwrap();
        assert_eq!(old.experimental_before, None);
    }

    #[test]
    fn status_feature_never_clobbers_other_shapes() {
        let original = json!({"experimental": true, "feature_flags": "odd"});
        let mut v = original.clone();
        assert_eq!(enable_status_feature(&mut v), FeatureRecord::default());
        assert_eq!(v, original);
    }
}
