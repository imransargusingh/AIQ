//! Which AI tools AIQ is hooked into: one definition for `aiq install`/`aiq status` and AIQ home's setup
//! (design aiq-home-deferred-pages E7). Reads the tools' own settings files; never writes them.
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn user_home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default()
}

/// Claude Code's user settings file: `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json`.
pub fn claude_settings_path() -> PathBuf {
    if let Some(d) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d).join("settings.json");
    }
    user_home().join(".claude").join("settings.json")
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
    /// The tool's own folder exists (it is installed or was used).
    pub present: bool,
    /// The command that adds AIQ's hooks.
    pub install: &'static str,
}

/// What AIQ can see of each tool it supports, from the given settings locations.
pub fn found_at(claude_settings: &Path, copilot: &Path) -> Vec<ToolFound> {
    let claude = std::fs::read_to_string(claude_settings).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    vec![
        ToolFound {
            tool: "claude-code",
            hooked: claude.as_ref().is_some_and(claude_has_aiq),
            present: claude_settings.parent().is_some_and(Path::exists),
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
    found_at(&claude_settings_path(), &copilot_home())
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
        let none = found_at(&claude, &copilot);
        assert!(none.iter().all(|t| !t.hooked && !t.present));

        std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
        std::fs::write(&claude, r#"{"hooks":{"Stop":[{"hooks":[{"command":"x","args":["notify"]}]}]}}"#).unwrap();
        std::fs::create_dir_all(copilot.join("hooks")).unwrap();
        let other = found_at(&claude, &copilot);
        assert_eq!(other.iter().map(|t| (t.hooked, t.present)).collect::<Vec<_>>(), vec![(false, true), (false, true)]);

        let v = json!({"hooks":{"Stop":[{"hooks":[{"command":"aiq","args":["hook","claude-code","Stop"]}]}]}});
        std::fs::write(&claude, v.to_string()).unwrap();
        std::fs::write(copilot_hook_file(&copilot), "{}").unwrap();
        assert!(found_at(&claude, &copilot).iter().all(|t| t.hooked && t.present));
    }

    #[test]
    fn broken_settings_file_is_not_hooked() {
        let d = tempfile::tempdir().unwrap();
        let claude = d.path().join("settings.json");
        std::fs::write(&claude, "{ not json").unwrap();
        assert!(!found_at(&claude, &d.path().join("c"))[0].hooked);
    }
}
