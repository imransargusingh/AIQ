//! Where AIQ keeps its files. All of them are readable only by the OS user.
//!
//! Windows: `%USERPROFILE%\.aiq`, beside Claude Code's own `%USERPROFILE%\.claude` (per-user; the
//! profile's inherited ACL grants only the user, SYSTEM and Administrators). Not `%LOCALAPPDATA%`:
//! packaged (MSIX) apps such as the Claude desktop app get their AppData writes redirected to a
//! private copy, so hooks started there and hooks started from a terminal would split into two
//! homes with two tokens (DC-007, found on the dogfood laptop 2026-11-01).
//! Linux/macOS: data in `$XDG_DATA_HOME/aiq` (default `~/.local/share/aiq`), runtime files (socket,
//! token) in `$XDG_RUNTIME_DIR/aiq` when set, otherwise the data dir. Directories are 0700.
//! `AIQ_HOME` overrides both (tests, portable runs).
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Paths {
    pub data: PathBuf,
    pub runtime: PathBuf,
}

/// The Windows default home: outside AppData so packaged and unpackaged processes share it (DC-007).
pub fn windows_home(profile: Option<PathBuf>) -> PathBuf {
    profile.unwrap_or_else(std::env::temp_dir).join(".aiq")
}

impl Paths {
    pub fn from_env() -> Self {
        if let Some(h) = std::env::var_os("AIQ_HOME").filter(|v| !v.is_empty()) {
            let p = PathBuf::from(h);
            return Self { data: p.clone(), runtime: p };
        }
        #[cfg(windows)]
        {
            let p = windows_home(std::env::var_os("USERPROFILE").filter(|v| !v.is_empty()).map(PathBuf::from));
            Self { data: p.clone(), runtime: p }
        }
        #[cfg(not(windows))]
        {
            let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
            let data = std::env::var_os("XDG_DATA_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share"))
                .join("aiq");
            let runtime = std::env::var_os("XDG_RUNTIME_DIR")
                .filter(|v| !v.is_empty())
                .map(|r| PathBuf::from(r).join("aiq"))
                .unwrap_or_else(|| data.clone());
            Self { data, runtime }
        }
    }

    pub fn at(dir: &Path) -> Self {
        Self { data: dir.to_owned(), runtime: dir.to_owned() }
    }

    pub fn db(&self) -> PathBuf {
        self.data.join("aiq.db")
    }
    /// The folder holding the person's team tips file (`aiq team-tips add`); one line, a path.
    pub fn team_tips_pointer(&self) -> PathBuf {
        self.data.join("team-tips-folder.txt")
    }
    /// The folder the core found by itself holding a team tips file; one line, a path.
    pub fn team_tips_found(&self) -> PathBuf {
        self.data.join("team-tips-found.txt")
    }
    /// AIQ's own copy of the team's repo, downloaded from the address built into the release.
    pub fn team_tips_download(&self) -> PathBuf {
        self.data.join("team-tips")
    }
    /// The last team tips search's outcome code; one line.
    pub fn team_tips_outcome(&self) -> PathBuf {
        self.data.join("team-tips-outcome.txt")
    }
    pub fn spool(&self) -> PathBuf {
        self.data.join("spool.jsonl")
    }
    /// One file per spooled frame (`spool::append_frame`).
    pub fn spool_dir(&self) -> PathBuf {
        self.data.join("spool.d")
    }
    pub fn spool_draining(&self) -> PathBuf {
        self.data.join("spool.draining.jsonl")
    }
    pub fn pause_flag(&self) -> PathBuf {
        self.data.join("paused")
    }
    /// AIQ home's first-run setup finished (design aiq-home-deferred-pages E5). App state, not the
    /// person's data: delete everything keeps it.
    pub fn setup_flag(&self) -> PathBuf {
        self.data.join("setup-done")
    }
    /// AIQ home's tour was taken, skipped or declined once, so it is not offered again. App state, like
    /// `setup_flag`: delete everything keeps it.
    pub fn tour_flag(&self) -> PathBuf {
        self.data.join("tour-seen")
    }
    pub fn log(&self) -> PathBuf {
        self.data.join("aiq.log")
    }
    pub fn hook_timings(&self) -> PathBuf {
        self.data.join("hook-timings.log")
    }
    /// The timing log's previous fill, kept when the log rotates (`rotating_append`).
    pub fn hook_timings_prev(&self) -> PathBuf {
        self.data.join("hook-timings.1.log")
    }
    pub fn hook_errors(&self) -> PathBuf {
        self.data.join("hook-errors.log")
    }
    pub fn core_error(&self) -> PathBuf {
        self.data.join("core-error")
    }
    pub fn start_lock(&self) -> PathBuf {
        self.data.join("core-start.lock")
    }
    /// The tools AIQ has seen installed since setup (`tools::newly_installed`): a tool that appears later is
    /// connected once by itself; one seen before is never reconnected after the person disconnects it.
    pub fn tools_seen(&self) -> PathBuf {
        self.data.join("tools-seen.json")
    }
    pub fn install_record(&self) -> PathBuf {
        self.data.join("claude-code-install.json")
    }
    /// The last time AIQ added its VS Code extension: when, which AIQ, and each build's result or error.
    pub fn vscode_install_record(&self) -> PathBuf {
        self.data.join("vscode-install.json")
    }
    /// What `aiq install copilot` changed in Copilot's settings to switch the status line on.
    pub fn copilot_install_record(&self) -> PathBuf {
        self.data.join("copilot-install.json")
    }
    pub fn core_lock(&self) -> PathBuf {
        self.runtime.join("core.lock")
    }
    pub fn token(&self) -> PathBuf {
        self.runtime.join("token")
    }
    #[cfg(not(windows))]
    pub fn socket(&self) -> PathBuf {
        self.runtime.join("core.sock")
    }

    /// Pipe name on Windows: one per OS user and runtime directory. Derived from `self` (never from the
    /// environment), so every caller holding the same `Paths` reaches the same core. A second user's core
    /// can never take over this name (first-instance flag + user-only DACL, spike S1).
    #[cfg(windows)]
    pub fn pipe_name(&self) -> String {
        let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
        let clean: String = user.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        format!("aiq-core-{clean}-{:016x}", fnv1a(self.runtime.to_string_lossy().to_lowercase().as_bytes()))
    }

    /// How another program on this machine reaches the core (the VS Code extension reads this once, so the
    /// socket path and pipe name are defined only here). `pipe` is a Windows pipe path for Node's `net`.
    pub fn endpoint_json(&self) -> String {
        #[cfg(windows)]
        let (key, addr) = ("pipe", windows_pipe_path(&self.pipe_name()));
        #[cfg(not(windows))]
        let (key, addr) = ("socket", self.socket().to_string_lossy().into_owned());
        serde_json::json!({
            key: addr,
            "token": self.token(),
            "paused_flag": self.pause_flag(),
        })
        .to_string()
    }

    /// Create the data and runtime directories, owner-only.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for d in [&self.data, &self.runtime] {
            std::fs::create_dir_all(d)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        Ok(())
    }
}

/// Node's `net.connect` path for a named pipe: `\\.\pipe\<name>`.
pub fn windows_pipe_path(name: &str) -> String {
    format!(r"\\.\pipe\{name}")
}

#[cfg(windows)]
fn fnv1a(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, x| (h ^ *x as u64).wrapping_mul(0x100000001b3))
}

/// Append one line to a small user-only log, truncating it first once it passes `max` bytes.
/// Used on the hook's hot path for timings and error counters: one open, one write.
pub fn bounded_append(path: &Path, line: &str, max: u64) -> std::io::Result<()> {
    use std::io::Write;
    let too_big = std::fs::metadata(path).map(|m| m.len() > max).unwrap_or(false);
    let mut o = std::fs::OpenOptions::new();
    o.create(true);
    if too_big {
        o.write(true).truncate(true);
    } else {
        o.append(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o.open(path)?;
    f.write_all(format!("{line}\n").as_bytes())
}

/// Like `bounded_append`, but a full log is renamed to `prev` (replacing the older fill) instead of emptied, so
/// readers that read `prev` then `path` always see between one and two fills of history.
/// assume: two writers rarely cross the cap in the same instant. If false: the second rename moves a one-line
/// log over `prev` and that fill is lost, the same loss the plain truncate had every time. A failed rename
/// appends anyway (the file runs a little over the cap) and the next run tries again.
pub fn rotating_append(path: &Path, prev: &Path, line: &str, max: u64) -> std::io::Result<()> {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > max) {
        let _ = std::fs::rename(path, prev);
    }
    bounded_append(path, line, u64::MAX)
}

/// Write a whole file atomically (temp + rename), owner-only.
pub fn write_user_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    {
        let mut o = std::fs::OpenOptions::new();
        o.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        let mut f = o.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotating_append_keeps_the_previous_fill() {
        let d = tempfile::tempdir().unwrap();
        let (p, prev) = (d.path().join("t.log"), d.path().join("t.1.log"));
        for i in 0..30 {
            rotating_append(&p, &prev, &format!("line {i:02}"), 64).unwrap();
        }
        let (old, new) = (std::fs::read_to_string(&prev).unwrap(), std::fs::read_to_string(&p).unwrap());
        assert!(new.ends_with("line 29\n"));
        // Every line since the older fill began is still there, in order, and nothing was emptied.
        let both = format!("{old}{new}");
        let n: Vec<u32> = both.lines().map(|l| l[5..].parse().unwrap()).collect();
        assert!(n.windows(2).all(|w| w[1] == w[0] + 1), "{both}");
        assert!(n.len() > 64 / 8, "more than one fill kept: {both}");
    }

    #[test]
    fn windows_pipe_path_is_nodes_pipe_form() {
        assert_eq!(windows_pipe_path("aiq-core-u-1"), r"\\.\pipe\aiq-core-u-1");
    }

    #[cfg(not(windows))]
    #[test]
    fn endpoint_json_names_socket_token_and_pause_flag() {
        let v: serde_json::Value = serde_json::from_str(&Paths::at(Path::new("/h")).endpoint_json()).unwrap();
        assert_eq!(v["socket"], "/h/core.sock");
        assert_eq!(v["token"], "/h/token");
        assert_eq!(v["paused_flag"], "/h/paused");
    }

    /// DC-007: the Windows home must not sit under AppData, which packaged apps redirect.
    #[test]
    fn windows_home_is_in_the_profile_not_appdata() {
        let h = windows_home(Some(PathBuf::from(r"C:\Users\someone")));
        assert_eq!(h, PathBuf::from(r"C:\Users\someone").join(".aiq"));
        assert!(!h.to_string_lossy().to_lowercase().contains("appdata"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_default_paths_use_the_profile_home() {
        if std::env::var_os("AIQ_HOME").is_none() {
            let p = Paths::from_env();
            assert!(!p.data.to_string_lossy().to_lowercase().contains("appdata"), "{}", p.data.display());
            assert!(p.data.ends_with(".aiq"));
        }
    }
}
