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

/// The AIQ extension's id in VS Code: `<publisher>.<name>` from `extensions/vscode/package.json`.
pub const VSCODE_EXTENSION_ID: &str = "aiq-local.aiq-coach";

/// VS Code's `code` command: on PATH, else in VS Code's own install folders (per-user, then per-machine on
/// Windows). The installer puts `bin` on PATH by default, but a GUI app's PATH can predate that.
pub fn vscode_cli_at(
    path_var: Option<&std::ffi::OsStr>,
    local_app_data: Option<&Path>,
    program_files: Option<&Path>,
) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["code.cmd"] } else { &["code"] };
    let own = [local_app_data.map(|d| d.join("Programs")), program_files.map(Path::to_path_buf)]
        .into_iter()
        .flatten()
        .map(|d| d.join("Microsoft VS Code").join("bin"));
    path_var
        .into_iter()
        .flat_map(std::env::split_paths)
        .chain(own)
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())
}

pub fn vscode_cli() -> Option<PathBuf> {
    vscode_cli_at(
        std::env::var_os("PATH").as_deref(),
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).as_deref(),
        std::env::var_os("ProgramFiles").map(PathBuf::from).as_deref(),
    )
}

/// VS Code's user extensions folder: `$VSCODE_EXTENSIONS`, else `~/.vscode/extensions`.
// assume: VS Code honours VSCODE_EXTENSIONS as its extensions folder and otherwise uses ~/.vscode/extensions
// (stable, not portable mode). Confirm: on the laptop, extensions.json there gains the id after
// `aiq install vscode`. If false, AIQ says "not connected" after a good
// install and setup offers Connect again; nothing is written to the wrong place (AIQ only reads here).
pub fn vscode_extensions_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("VSCODE_EXTENSIONS").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    user_home().join(".vscode").join("extensions")
}

/// VS Code's list of installed extensions in that folder.
pub fn vscode_extensions_list(dir: &Path) -> PathBuf {
    dir.join("extensions.json")
}

/// The AIQ extension is installed: VS Code's `extensions.json` lists its id. Reads a file instead of running
/// `code --list-extensions`, which starts VS Code's runtime (seconds) and runs on every `aiq status`.
// assume: `code --install-extension` adds `{"identifier":{"id":"<publisher>.<name>"},...}` to
// extensions.json before it exits, and `--uninstall-extension` removes it. Confirm: the laptop install.
// If false, AIQ shows the wrong connected state; install and uninstall still run VS Code's own command.
pub fn vscode_has_aiq(dir: &Path) -> bool {
    let Ok(t) = std::fs::read_to_string(vscode_extensions_list(dir)) else { return false };
    let Ok(Value::Array(list)) = serde_json::from_str::<Value>(&t) else { return false };
    list.iter().any(|e| {
        e.pointer("/identifier/id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.eq_ignore_ascii_case(VSCODE_EXTENSION_ID))
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolFound {
    /// claude-code, copilot, vscode
    pub tool: &'static str,
    /// AIQ's hooks are in this tool's settings (VS Code: AIQ's extension is installed in it).
    pub hooked: bool,
    /// The tool is installed or was used on this computer (Claude Code: `claude_installed_at`; Copilot: its
    /// home folder, which Copilot CLI and VS Code agent mode create; VS Code: its `code` command).
    pub present: bool,
    /// The command that adds AIQ's hooks.
    pub install: &'static str,
}

/// What AIQ can see of each tool it supports, from the given settings locations.
pub fn found_at(claude_settings: &Path, claude_installed: bool, copilot: &Path, vscode: &VsCodeAt) -> Vec<ToolFound> {
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
        // After Copilot: VS Code tips come from Copilot's hooks, so `aiq install` connects Copilot first.
        ToolFound {
            tool: "vscode",
            hooked: vscode_has_aiq(&vscode.extensions),
            present: vscode.cli,
            install: "aiq install vscode",
        },
    ]
}

/// Where AIQ looks for VS Code: whether its `code` command was found, and its extensions folder.
pub struct VsCodeAt {
    pub cli: bool,
    pub extensions: PathBuf,
}

pub fn found() -> Vec<ToolFound> {
    let vscode = VsCodeAt { cli: vscode_cli().is_some(), extensions: vscode_extensions_dir() };
    found_at(&claude_settings_path(), claude_installed(), &copilot_home(), &vscode)
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
        let nv = VsCodeAt { cli: false, extensions: d.path().join("ext") };
        let none = found_at(&claude, false, &copilot, &nv);
        assert!(none.iter().all(|t| !t.hooked && !t.present));

        std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
        std::fs::write(&claude, r#"{"hooks":{"Stop":[{"hooks":[{"command":"x","args":["notify"]}]}]}}"#).unwrap();
        std::fs::create_dir_all(copilot.join("hooks")).unwrap();
        let other = found_at(&claude, true, &copilot, &nv);
        assert_eq!(
            other.iter().map(|t| (t.hooked, t.present)).collect::<Vec<_>>(),
            vec![(false, true), (false, true), (false, false)]
        );

        let v = json!({"hooks":{"Stop":[{"hooks":[{"command":"aiq","args":["hook","claude-code","Stop"]}]}]}});
        std::fs::write(&claude, v.to_string()).unwrap();
        std::fs::write(copilot_hook_file(&copilot), "{}").unwrap();
        assert!(found_at(&claude, true, &copilot, &nv)[..2].iter().all(|t| t.hooked && t.present));
        // AIQ's hooks in the settings of a Claude Code that isn't installed: hooked, not present.
        let gone = &found_at(&claude, false, &copilot, &nv)[0];
        assert_eq!((gone.hooked, gone.present), (true, false));
    }

    #[test]
    fn broken_settings_file_is_not_hooked() {
        let d = tempfile::tempdir().unwrap();
        let claude = d.path().join("settings.json");
        std::fs::write(&claude, "{ not json").unwrap();
        let nv = VsCodeAt { cli: false, extensions: d.path().join("ext") };
        assert!(!found_at(&claude, true, &d.path().join("c"), &nv)[0].hooked);
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

    /// VS Code counts as connected only when its own extension list names AIQ's extension; another
    /// extension, a folder named like AIQ's, or a broken list is not connected.
    #[test]
    fn vscode_is_connected_only_when_its_extension_list_names_aiq() {
        let d = tempfile::tempdir().unwrap();
        let ext = d.path().join("extensions");
        let at = |cli| VsCodeAt { cli, extensions: ext.clone() };
        let vs = |a: &VsCodeAt| found_at(&d.path().join("s.json"), false, &d.path().join("c"), a)[2].clone();
        assert_eq!((vs(&at(false)).hooked, vs(&at(false)).present), (false, false));
        assert_eq!(
            (vs(&at(true)).hooked, vs(&at(true)).present, vs(&at(true)).install),
            (false, true, "aiq install vscode")
        );

        std::fs::create_dir_all(ext.join("aiq-local.aiq-coach-0.2.0")).unwrap();
        let other = r#"[{"identifier":{"id":"ms-python.python"},"version":"1.0.0"}]"#;
        std::fs::write(vscode_extensions_list(&ext), other).unwrap();
        assert!(!vs(&at(true)).hooked, "a folder alone, with the list naming another extension");
        std::fs::write(vscode_extensions_list(&ext), "{ not json").unwrap();
        assert!(!vs(&at(true)).hooked, "broken list");
        let ours = r#"[{"identifier":{"id":"ms-python.python"}},{"identifier":{"id":"AIQ-local.aiq-coach","uuid":""},"version":"0.2.0"}]"#;
        std::fs::write(vscode_extensions_list(&ext), ours).unwrap();
        assert!(vs(&at(true)).hooked);
    }

    #[test]
    fn vscode_cli_is_found_on_path_or_in_its_install_folders() {
        let d = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) { "code.cmd" } else { "code" };
        let (path_dir, lad, pf) = (d.path().join("p"), d.path().join("lad"), d.path().join("pf"));
        let path = std::env::join_paths([&path_dir]).unwrap();
        assert_eq!(vscode_cli_at(Some(&path), Some(&lad), Some(&pf)), None);
        let user = lad.join("Programs").join("Microsoft VS Code").join("bin");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(user.join(name), "").unwrap();
        assert_eq!(vscode_cli_at(Some(&path), Some(&lad), Some(&pf)), Some(user.join(name)), "per-user install");
        std::fs::create_dir_all(&path_dir).unwrap();
        std::fs::write(path_dir.join(name), "").unwrap();
        assert_eq!(vscode_cli_at(Some(&path), Some(&lad), Some(&pf)), Some(path_dir.join(name)), "PATH first");
        let machine = pf.join("Microsoft VS Code").join("bin");
        std::fs::create_dir_all(&machine).unwrap();
        std::fs::write(machine.join(name), "").unwrap();
        assert_eq!(vscode_cli_at(None, None, Some(&pf)), Some(machine.join(name)), "per-machine install");
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
