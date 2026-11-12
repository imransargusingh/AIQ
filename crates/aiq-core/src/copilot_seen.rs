//! What AIQ's Copilot CLI hook saw of the Copilot that ran it (DC-056): the program behind `COPILOT_LOADER_PID`,
//! the hook folder (`COPILOT_HOOK_DIR`) and `COPILOT_HOME`. The hook runs inside the person's own Copilot, so
//! this is the Copilot that really works, whatever AIQ's own PATH holds. Measured on a second install
//! (2026-11-12, 0.1.42): no `copilot` AIQ could find on PATH or in npm/WinGet ran, while Copilot CLI ran hooks
//! there all day. AIQ calls this program for analysis and labels, and reads session logs from its home.
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE: &str = "copilot-seen.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    /// The loader process's program, full path. None when the hook couldn't read it.
    pub exe: Option<String>,
    /// `COPILOT_HOOK_DIR` as the hook saw it.
    pub hook_dir: Option<String>,
    /// `COPILOT_HOME` as the hook saw it (None when unset).
    pub home_env: Option<String>,
    /// When a hook last wrote this (ms since the epoch).
    pub at_ms: i64,
}

impl Seen {
    /// Copilot's home as the hook saw it: `COPILOT_HOME`, else the folder above the hook folder when that folder
    /// is named `hooks`.
    pub fn home(&self) -> Option<PathBuf> {
        if let Some(h) = self.home_env.as_deref().filter(|h| !h.is_empty()) {
            return Some(PathBuf::from(h));
        }
        let d = Path::new(self.hook_dir.as_deref()?);
        (d.file_name()? == "hooks").then(|| d.parent().map(Path::to_path_buf)).flatten()
    }

    /// The loader program when it is Copilot itself (`copilot`, `copilot.exe`), so AIQ can call it with `-p`.
    /// A loader that is another program (a Node runtime, say) is recorded for diagnostics but not called.
    pub fn copilot_exe(&self) -> Option<PathBuf> {
        let p = PathBuf::from(self.exe.as_deref()?);
        let stem = p.file_stem()?.to_str()?.to_ascii_lowercase();
        (stem == "copilot" && p.is_file()).then_some(p)
    }
}

pub fn read(paths: &Paths) -> Option<Seen> {
    serde_json::from_str(&std::fs::read_to_string(paths.data.join(FILE)).ok()?).ok()
}

/// Keep what the hook saw. Writes only when something other than the time changed, or a day passed, so the
/// hook's cost is one small read on most runs.
pub fn record(paths: &Paths, seen: &Seen) -> std::io::Result<bool> {
    if let Some(old) = read(paths) {
        let same = Seen { at_ms: 0, ..old.clone() } == Seen { at_ms: 0, ..seen.clone() };
        if same && seen.at_ms.saturating_sub(old.at_ms) < 86_400_000 {
            return Ok(false);
        }
    }
    let tmp = paths.data.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, serde_json::to_vec(seen).unwrap_or_default())?;
    std::fs::rename(&tmp, paths.data.join(FILE))?;
    Ok(true)
}

/// Where Copilot's session logs are: `$COPILOT_HOME` for this process, else the home the hook saw, else
/// `~/.copilot` (`tools::copilot_home`).
pub fn log_home(paths: &Paths) -> PathBuf {
    if std::env::var_os("COPILOT_HOME").is_some_and(|v| !v.is_empty()) {
        return crate::tools::copilot_home();
    }
    read(paths).and_then(|s| s.home()).unwrap_or_else(crate::tools::copilot_home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_comes_from_copilot_home_then_the_hook_folder() {
        let s = |env: Option<&str>, dir: Option<&str>| Seen {
            home_env: env.map(Into::into),
            hook_dir: dir.map(Into::into),
            ..Seen::default()
        };
        assert_eq!(s(Some("/h"), Some("/x/hooks")).home(), Some(PathBuf::from("/h")));
        assert_eq!(s(None, Some("/x/hooks")).home(), Some(PathBuf::from("/x")));
        assert_eq!(s(None, Some("/x/other")).home(), None, "not a hooks folder: unknown");
        assert_eq!(s(None, None).home(), None);
    }

    #[test]
    fn only_a_program_named_copilot_is_called_and_records_skip_unchanged_writes() {
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join(if cfg!(windows) { "copilot.exe" } else { "copilot" });
        let node = d.path().join("node.exe");
        std::fs::write(&exe, "").unwrap();
        std::fs::write(&node, "").unwrap();
        let s = |p: &Path| Seen { exe: Some(p.display().to_string()), ..Seen::default() };
        assert_eq!(s(&exe).copilot_exe(), Some(exe.clone()));
        assert_eq!(s(&node).copilot_exe(), None, "a Node loader is recorded, never called");
        assert_eq!(s(&d.path().join("copilot.exe.gone")).copilot_exe(), None);

        let paths = Paths::at(d.path());
        std::fs::create_dir_all(&paths.data).unwrap();
        let a = Seen { at_ms: 1_000, ..s(&exe) };
        assert!(record(&paths, &a).unwrap());
        assert!(!record(&paths, &Seen { at_ms: 2_000, ..a.clone() }).unwrap(), "same, within a day");
        assert!(record(&paths, &Seen { at_ms: 2_000, ..s(&node) }).unwrap(), "changed");
        assert_eq!(read(&paths).unwrap().exe, Some(node.display().to_string()));
        assert!(record(&paths, &Seen { at_ms: 2_000 + 86_400_000, ..s(&node) }).unwrap(), "a day later");
    }
}
