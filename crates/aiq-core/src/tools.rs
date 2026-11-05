//! Which AI tools AIQ is hooked into: one definition for `aiq install`/`aiq status` and AIQ home's setup
//! (design aiq-home-deferred-pages E7). Reads the tools' own settings files; never writes them.
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn user_home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default()
}

/// Claude Code's config folder: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn claude_config_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    user_home().join(".claude")
}

/// Claude Code's user settings file: `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json`.
pub fn claude_settings_path() -> PathBuf {
    claude_config_dir().join("settings.json")
}

/// Claude Code is installed or was used here: its `claude` program is on PATH or in one of its own install
/// folders, or its config folder holds session transcripts (`projects/`), which only Claude Code writes.
/// A bare `~/.claude` folder is not enough: other tools and people create it (skills, agents, CLAUDE.md),
/// and AIQ 0.1.4 counted it, so setup "connected" Claude Code on a computer that never had it.
pub fn claude_installed_at(config: &Path, home: &Path, path_var: Option<&std::ffi::OsStr>) -> bool {
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd", "claude"] } else { &["claude"] };
    let on_path =
        path_var.into_iter().flat_map(std::env::split_paths).any(|d| names.iter().any(|n| d.join(n).is_file()));
    // The native installer's folder (often not on PATH in a GUI app's environment) and the old "local" install.
    let own = [home.join(".local").join("bin"), config.join("local")]
        .iter()
        .any(|d| names.iter().any(|n| d.join(n).is_file()));
    let used = std::fs::read_dir(config.join("projects")).is_ok_and(|mut r| r.next().is_some());
    on_path || own || used
}

fn claude_installed() -> bool {
    claude_installed_at(&claude_config_dir(), &user_home(), std::env::var_os("PATH").as_deref())
}

/// One AIQ Claude Code hook entry: `args[0..2] == ["hook", "claude-code"]` is how AIQ finds its own.
pub fn is_aiq_claude_hook(h: &Value) -> bool {
    let a = h.get("args").and_then(Value::as_array);
    a.is_some_and(|a| a.len() >= 2 && a[0] == "hook" && a[1] == "claude-code")
}

/// The Claude Code settings hold at least one AIQ hook.
pub fn claude_has_aiq(settings: &Value) -> bool {
    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else { return false };
    hooks
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .any(|g| g.get("hooks").and_then(Value::as_array).is_some_and(|l| l.iter().any(is_aiq_claude_hook)))
}

/// Copilot's home (CLI and VS Code agent mode share it): `$COPILOT_HOME`, else `~/.copilot`.
pub fn copilot_home() -> PathBuf {
    if let Some(d) = std::env::var_os("COPILOT_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    user_home().join(".copilot")
}

/// AIQ's own Copilot hook file.
pub fn copilot_hook_file(home: &Path) -> PathBuf {
    home.join("hooks").join("aiq.json")
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolFound {
    /// claude-code, copilot
    pub tool: &'static str,
    /// AIQ's hooks are in this tool's settings.
    pub hooked: bool,
    /// The tool is installed or was used on this computer (Claude Code: `claude_installed_at`; Copilot: its
    /// home folder, which Copilot CLI and VS Code agent mode create).
    pub present: bool,
    /// The command that adds AIQ's hooks.
    pub install: &'static str,
}

/// What AIQ can see of each tool it supports, from the given settings locations.
pub fn found_at(claude_settings: &Path, claude_installed: bool, copilot: &Path) -> Vec<ToolFound> {
    let claude = std::fs::read_to_string(claude_settings).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    vec![
        ToolFound {
            tool: "claude-code",
            hooked: claude.as_ref().is_some_and(claude_has_aiq),
            present: claude_installed,
            install: "aiq install claude-code",
        },
        ToolFound {
            tool: "copilot",
            hooked: copilot_hook_file(copilot).exists(),
            present: copilot.exists(),
            install: "aiq install copilot",
        },
    ]
}

pub fn found() -> Vec<ToolFound> {
    found_at(&claude_settings_path(), claude_installed(), &copilot_home())
}

/// Tools installed since AIQ last looked, for connecting once by themselves (Imran, 2026-11-02: AIQ connects
/// what it finds; the person shouldn't have to run setup again). The first look after setup only records what
/// is installed, so a tool the person disconnected (installed, so already seen) is never reconnected. Returns
/// tools present and not hooked that weren't seen before; records every present tool as seen.
pub fn newly_installed(seen_file: &Path, tools: &[ToolFound]) -> Vec<&'static str> {
    let present: Vec<&'static str> = tools.iter().filter(|t| t.present).map(|t| t.tool).collect();
    let seen: Option<Vec<String>> = std::fs::read_to_string(seen_file).ok().and_then(|t| serde_json::from_str(&t).ok());
    let new: Vec<&'static str> = match &seen {
        Some(s) => {
            tools.iter().filter(|t| t.present && !t.hooked && !s.iter().any(|x| x == t.tool)).map(|t| t.tool).collect()
        }
        None => Vec::new(),
    };
    let mut all: Vec<String> = seen.unwrap_or_default();
    all.extend(present.iter().filter(|p| !all.iter().any(|x| x == *p)).map(|p| p.to_string()).collect::<Vec<_>>());
    let _ = crate::paths::write_user_only(seen_file, serde_json::to_string(&all).unwrap_or_default().as_bytes());
    new
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_hooked_present_and_missing_tools() {
        let d = tempfile::tempdir().unwrap();
        let claude = d.path().join(".claude").join("settings.json");
        let copilot = d.path().join(".copilot");
        let none = found_at(&claude, false, &copilot);
        assert!(none.iter().all(|t| !t.hooked && !t.present));

        std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
        std::fs::write(&claude, r#"{"hooks":{"Stop":[{"hooks":[{"command":"x","args":["notify"]}]}]}}"#).unwrap();
        std::fs::create_dir_all(copilot.join("hooks")).unwrap();
        let other = found_at(&claude, true, &copilot);
        assert_eq!(other.iter().map(|t| (t.hooked, t.present)).collect::<Vec<_>>(), vec![(false, true), (false, true)]);

        let v = json!({"hooks":{"Stop":[{"hooks":[{"command":"aiq","args":["hook","claude-code","Stop"]}]}]}});
        std::fs::write(&claude, v.to_string()).unwrap();
        std::fs::write(copilot_hook_file(&copilot), "{}").unwrap();
        assert!(found_at(&claude, true, &copilot).iter().all(|t| t.hooked && t.present));
        // AIQ's hooks in the settings of a Claude Code that isn't installed: hooked, not present.
        assert_eq!(
            (found_at(&claude, false, &copilot)[0].hooked, found_at(&claude, false, &copilot)[0].present),
            (true, false)
        );
    }

    #[test]
    fn broken_settings_file_is_not_hooked() {
        let d = tempfile::tempdir().unwrap();
        let claude = d.path().join("settings.json");
        std::fs::write(&claude, "{ not json").unwrap();
        assert!(!found_at(&claude, true, &d.path().join("c"))[0].hooked);
    }

    /// The 0.1.4 bug (reported 2026-11-05): a `~/.claude` folder with no Claude Code counted as installed, so
    /// setup wrote AIQ's hooks there and said "connected". Only the program or Claude Code's own transcripts count.
    #[test]
    fn a_bare_claude_folder_is_not_claude_code() {
        let d = tempfile::tempdir().unwrap();
        let (home, bin) = (d.path().join("home"), d.path().join("bin"));
        let config = home.join(".claude");
        std::fs::create_dir_all(config.join("skills")).unwrap();
        std::fs::create_dir_all(config.join("projects")).unwrap();
        std::fs::write(config.join("CLAUDE.md"), "notes").unwrap();
        std::fs::write(config.join("settings.json"), "{}").unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        let path = std::env::join_paths([&bin]).unwrap();
        assert!(!claude_installed_at(&config, &home, Some(&path)), "folder, settings, skills, empty projects");
        assert!(!claude_installed_at(&config, &home, None));

        let exe = if cfg!(windows) { "claude.exe" } else { "claude" };
        std::fs::write(bin.join(exe), "").unwrap();
        assert!(claude_installed_at(&config, &home, Some(&path)), "on PATH");
        std::fs::remove_file(bin.join(exe)).unwrap();

        std::fs::create_dir_all(home.join(".local").join("bin")).unwrap();
        std::fs::write(home.join(".local").join("bin").join(exe), "").unwrap();
        assert!(claude_installed_at(&config, &home, Some(&path)), "native installer folder");
        std::fs::remove_file(home.join(".local").join("bin").join(exe)).unwrap();

        std::fs::create_dir_all(config.join("projects").join("C--repos-a")).unwrap();
        assert!(claude_installed_at(&config, &home, None), "Claude Code wrote transcripts here");
    }

    fn tool(tool: &'static str, hooked: bool, present: bool) -> ToolFound {
        ToolFound { tool, hooked, present, install: "" }
    }

    #[test]
    fn a_tool_installed_after_setup_is_new_once_and_a_disconnected_one_never() {
        let d = tempfile::tempdir().unwrap();
        let seen = d.path().join("tools-seen.json");
        // First look: records Copilot (installed, disconnected by the person) and connects nothing.
        assert!(newly_installed(&seen, &[tool("claude-code", false, false), tool("copilot", false, true)]).is_empty());
        assert!(newly_installed(&seen, &[tool("claude-code", false, false), tool("copilot", false, true)]).is_empty());
        // Claude Code installed later: new once.
        let now = [tool("claude-code", false, true), tool("copilot", false, true)];
        assert_eq!(newly_installed(&seen, &now), vec!["claude-code"]);
        assert!(newly_installed(&seen, &now).is_empty(), "seen now, so a later disconnect sticks");
    }
}
