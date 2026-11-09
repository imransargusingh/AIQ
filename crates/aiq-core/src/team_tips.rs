//! The team tips file (`aiq.team-tips.v1`, see `aiq_contracts::team_tips`): where it is and how it's read.
//!
//! Nobody has to set it up (Imran 2026-11-08: "I don't expect any user to have to run that step"): the core looks
//! for a `team-tips.toml` one level inside the usual code folders at start and every hour, and records what it
//! found in `team-tips-found.txt`. `aiq team-tips add <folder>` (or a choice in Home) writes `team-tips-folder.txt`,
//! which wins; writing `off` there stops it, found or chosen. Hooks never search: they read the two small files. Every
//! reader (the Copilot hook, the status line, `aiq show`, Home) reads `<folder>/team-tips.toml` in place, so a
//! `git pull` of the team's repo updates it. Reading never fails a tip: any problem gives `None` plus a reason
//! that names no tool. The file's text is never written to the store, a log or the core: callers render the
//! line and drop it.
use crate::paths::Paths;
use aiq_contracts::team_tips::{FILE_NAME, MAX_BYTES, TeamTipsV1};
use std::io::Read;
use std::path::{Path, PathBuf};

/// The surfaces allowed to show a team tool (Imran 2026-11-08): Copilot ones may name internal tools,
/// Claude Code may not. Anything not listed is treated as not allowed.
pub const SURFACES: &[&str] = &[crate::service::COPILOT_CLI, "vscode"];

pub fn allowed_on(surface: &str) -> bool {
    SURFACES.contains(&surface)
}

/// Claude Code sets `CLAUDECODE=1` in the shells it starts, so `aiq show` run by (or inside) Claude Code
/// leaves the line out.
pub fn inside_claude_code() -> bool {
    std::env::var_os("CLAUDECODE").is_some_and(|v| !v.is_empty())
}

pub fn parse(text: &str) -> Result<TeamTipsV1, String> {
    let t: TeamTipsV1 = toml::from_str(text).map_err(|e| e.message().to_owned())?;
    t.validate()?;
    Ok(t)
}

/// The pointer's value that means "don't use a team tips file", found or chosen.
pub const OFF: &str = "off";

/// Where the file in use came from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// Nothing chosen and nothing found.
    None,
    /// The person turned team tips off.
    Off,
    /// The person chose this folder (`aiq team-tips add`).
    Chosen(PathBuf),
    /// AIQ found this folder by itself.
    Found(PathBuf),
}

fn first_line(p: &Path) -> Option<String> {
    let text = std::fs::read_to_string(p).ok()?;
    let line = text.lines().next()?.trim().to_owned();
    (!line.is_empty()).then_some(line)
}

pub fn source(paths: &Paths) -> Source {
    match first_line(&paths.team_tips_pointer()) {
        Some(l) if l == OFF => Source::Off,
        Some(l) => Source::Chosen(PathBuf::from(l)),
        None => first_line(&paths.team_tips_found()).map_or(Source::None, |l| Source::Found(PathBuf::from(l))),
    }
}

/// The folder in use, if any.
pub fn folder(paths: &Paths) -> Option<PathBuf> {
    match source(paths) {
        Source::Chosen(p) | Source::Found(p) => Some(p),
        Source::None | Source::Off => None,
    }
}

fn write_or_remove(path: &Path, value: Option<&str>) -> std::io::Result<()> {
    match value {
        Some(v) => crate::paths::write_user_only(path, format!("{v}\n").as_bytes()),
        None => match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

/// The person's choice: a folder, `Some(OFF)`-style off via `set_off`, or `None` to let AIQ find it again.
pub fn set_folder(paths: &Paths, folder: Option<&Path>) -> std::io::Result<()> {
    let _ = paths.ensure_dirs();
    write_or_remove(&paths.team_tips_pointer(), folder.map(|f| f.display().to_string()).as_deref())
}

/// Stop using a team tips file, whether AIQ found it or the person chose it.
pub fn set_off(paths: &Paths) -> std::io::Result<()> {
    let _ = paths.ensure_dirs();
    write_or_remove(&paths.team_tips_pointer(), Some(OFF))
}

/// The usual code folders under a user folder, searched one level deep. Most people clone next to their other
/// repos; Visual Studio's default is `source\repos`.
pub const ROOTS: &[&str] =
    &["source/repos", "source", "repos", "src", "git", "code", "dev", "projects", "Documents/GitHub"];

/// A folder holding a readable team tips file. Only a file that declares the format counts, so an unrelated
/// `team-tips.toml` is never picked up.
fn holds_team_tips(dir: &Path) -> bool {
    let f = dir.join(FILE_NAME);
    f.is_file() && read_file(&f).is_ok()
}

/// The first folder one level inside a usual code folder that holds a team tips file, by name order.
pub fn discover(user_home: &Path) -> Option<PathBuf> {
    for root in ROOTS {
        let root = user_home.join(root);
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        if let Some(d) = dirs.into_iter().find(|d| holds_team_tips(d)) {
            return Some(d);
        }
    }
    None
}

/// The person's user folder: `USERPROFILE` on Windows, `HOME` elsewhere.
pub fn user_home() -> Option<PathBuf> {
    ["USERPROFILE", "HOME"].iter().filter_map(std::env::var_os).find(|v| !v.is_empty()).map(PathBuf::from)
}

/// The team's repo AIQ downloads the file from. Set at build time from the release workflow's secret
/// `AIQ_TEAM_TIPS_REPO` (Imran 2026-11-08, option B: the address ships inside the program but never in this repo's
/// text); the same variable at run time overrides it, for tests. Empty or unset: no download.
pub fn repo_url() -> Option<String> {
    std::env::var("AIQ_TEAM_TIPS_REPO")
        .ok()
        .or_else(|| option_env!("AIQ_TEAM_TIPS_REPO").map(str::to_owned))
        .map(|s| s.trim().to_owned())
        .filter(|s| s.starts_with("https://"))
}

/// How long one git call may take before it's stopped (a slow network must never pile up runs).
pub const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

/// Run git with no window and no prompt. Git Credential Manager uses the person's own sign-in for the host; with
/// none it fails instead of asking. AIQ never sees the sign-in.
fn git(args: &[&str], cwd: Option<&Path>) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let mut c = Command::new("git");
    c.args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS", "")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = c.spawn().map_err(|_| "git_missing".to_owned())?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => return Ok(()),
            Ok(Some(_)) => return Err("git_failed".into()),
            Ok(None) if start.elapsed() > GIT_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("git_timeout".into());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(200)),
            Err(_) => return Err("git_failed".into()),
        }
    }
}

/// Bring AIQ's own copy (`<data>/team-tips`) up to date with the team's repo: a one-commit clone the first time,
/// then fetch and reset. Returns an outcome code for the log (never the address or the file's text).
pub fn download(paths: &Paths, url: &str) -> Result<&'static str, String> {
    let dir = paths.team_tips_download();
    if dir.join(".git").is_dir() {
        git(&["fetch", "--depth", "1", "--quiet", "origin", "HEAD"], Some(&dir))?;
        git(&["reset", "--hard", "--quiet", "FETCH_HEAD"], Some(&dir))?;
        Ok("updated")
    } else {
        let _ = paths.ensure_dirs();
        let tmp = dir.with_extension("new");
        let _ = std::fs::remove_dir_all(&tmp);
        let r = git(&["clone", "--depth", "1", "--quiet", url, &tmp.display().to_string()], None);
        if r.is_err() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        r?;
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::rename(&tmp, &dir).map_err(|_| "git_failed".to_owned())?;
        Ok("downloaded")
    }
}

/// The core's search (at start and every hour). Leaves a choice or "off" alone. First the team's repo (download),
/// then a clone next to the person's other repos. A failed download keeps the last good copy, so going offline
/// never drops the lines. Returns an outcome code for the log line (which never names a tool or the address).
pub fn refresh_found(paths: &Paths, user_home: Option<&Path>) -> &'static str {
    refresh_found_from(paths, user_home, repo_url().as_deref())
}

pub fn refresh_found_from(paths: &Paths, user_home: Option<&Path>, url: Option<&str>) -> &'static str {
    let outcome = search(paths, user_home, url);
    let _ = paths.ensure_dirs();
    let _ = write_or_remove(&paths.team_tips_outcome(), Some(outcome));
    outcome
}

/// The last search's outcome code (`downloaded`, `updated`, `no_access`, `found_clone`, `none`, ...), for Home.
pub fn last_outcome(paths: &Paths) -> Option<String> {
    first_line(&paths.team_tips_outcome())
}

/// The one step a person may need: sign in to the team repo's host once. Runs git with prompts allowed, so Git
/// Credential Manager opens its own sign-in window, then searches again. Started only by the person (Home's
/// button or `aiq team-tips sign-in`), never by the core.
pub fn sign_in(paths: &Paths) -> &'static str {
    let Some(url) = repo_url() else { return "no_repo" };
    use std::process::{Command, Stdio};
    let _ = Command::new("git")
        .args(["ls-remote", "--heads", &url])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    refresh_found(paths, user_home().as_deref())
}

fn search(paths: &Paths, user_home: Option<&Path>, url: Option<&str>) -> &'static str {
    if first_line(&paths.team_tips_pointer()).is_some() {
        return "chosen_or_off";
    }
    let mut outcome = "no_repo";
    if let Some(u) = url {
        outcome = match download(paths, u) {
            Ok(o) => o,
            Err(e) if e == "git_missing" => "git_missing",
            Err(e) if e == "git_timeout" => "git_timeout",
            Err(_) => "no_access",
        };
    }
    let dl = paths.team_tips_download();
    let found = if holds_team_tips(&dl) { Some(dl) } else { user_home.and_then(discover) };
    let _ = paths.ensure_dirs();
    let _ = write_or_remove(&paths.team_tips_found(), found.as_ref().map(|p| p.display().to_string()).as_deref());
    match (&found, outcome) {
        (None, _) => "none",
        (Some(p), o) if *p == paths.team_tips_download() => o,
        (Some(_), _) => "found_clone",
    }
}

/// What reading the file gave. `Problem` carries a reason in AIQ's words and never the file's text.
#[derive(Debug)]
pub enum Status {
    NotSet,
    Problem(String),
    Ok(TeamTipsV1),
}

pub fn read_file(file: &std::path::Path) -> Result<TeamTipsV1, String> {
    let f = std::fs::File::open(file).map_err(|_| format!("AIQ can't open {}", file.display()))?;
    let mut buf = String::new();
    f.take(MAX_BYTES + 1).read_to_string(&mut buf).map_err(|_| "the file isn't plain text".to_owned())?;
    if buf.len() as u64 > MAX_BYTES {
        return Err(format!("the file is bigger than {} KB", MAX_BYTES / 1024));
    }
    parse(&buf)
}

pub fn read(paths: &Paths) -> Status {
    let Some(dir) = folder(paths) else { return Status::NotSet };
    match read_file(&dir.join(FILE_NAME)) {
        Ok(t) => Status::Ok(t),
        Err(e) => Status::Problem(e),
    }
}

/// The file, if set and readable. For renderers: a problem shows nothing extra.
pub fn load(paths: &Paths) -> Option<TeamTipsV1> {
    match read(paths) {
        Status::Ok(t) => Some(t),
        _ => None,
    }
}

/// The "Tool to try" line for a tip, if the tip is a habit's and the file names a tool for that habit.
pub fn line_for_tip(t: &TeamTipsV1, tip_id: &str) -> Option<String> {
    let move_id = tip_id.strip_prefix(&format!("{}.", aiq_domain::coach::OPPORTUNITY))?;
    t.for_move(move_id).map(|x| x.line())
}

/// What a surface's tip gets: the line on a Copilot surface with a matching entry; `None` on Claude Code,
/// with no file, with a broken file, or for a tip that isn't a habit's.
pub fn line_on(paths: &Paths, surface: &str, tip_id: &str) -> Option<String> {
    if !allowed_on(surface) {
        return None;
    }
    load(paths).and_then(|t| line_for_tip(&t, tip_id))
}

/// The tool's name for a tip, for the one-line status line.
pub fn tool_for_tip<'a>(t: &'a TeamTipsV1, tip_id: &str) -> Option<&'a str> {
    let move_id = tip_id.strip_prefix(&format!("{}.", aiq_domain::coach::OPPORTUNITY))?;
    t.for_move(move_id).map(|x| x.tool.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aiq_contracts::team_tips::MAX_TOOL_CHARS;

    const GOOD: &str = r#"
schema = "aiq.team-tips.v1"
[[tools]]
move_id = "check_code"
tool = "Example Test Runner"
how = "runs the whole test suite in one step."
link = "https://example.com/guide"
"#;

    #[test]
    fn good_file_parses_and_gives_a_line_only_for_its_habit() {
        let t = parse(GOOD).unwrap();
        let id = aiq_domain::coach::tip_id("check_code");
        assert_eq!(
            line_for_tip(&t, &id).as_deref(),
            Some("Tool to try: Example Test Runner: runs the whole test suite in one step.")
        );
        assert_eq!(tool_for_tip(&t, &id), Some("Example Test Runner"));
        assert!(line_for_tip(&t, &aiq_domain::coach::tip_id("brief")).is_none());
        assert!(line_for_tip(&t, "reprompt.add-what-changed").is_none());
    }

    #[test]
    fn bad_files_are_refused_with_a_reason() {
        for (bad, why) in [
            (GOOD.replace("aiq.team-tips.v1", "x"), "schema"),
            (GOOD.replace("link = ", "surprise = 1\nlink = "), "unknown field"),
            (GOOD.replace("https://example.com/guide", "http://example.com"), "https"),
            (GOOD.replace("Example Test Runner", &"x".repeat(MAX_TOOL_CHARS + 1)), "longer"),
            (GOOD.replace("in one step.", "in one\\nstep."), "line break"),
            (format!("{GOOD}{}", &GOOD[GOOD.find("[[tools]]").unwrap()..]), "second entry"),
        ] {
            let e = parse(&bad).unwrap_err();
            assert!(e.contains(why), "{why}: {e}");
        }
    }

    #[test]
    fn folder_round_trips_and_problems_never_quote_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path());
        assert!(matches!(read(&paths), Status::NotSet));
        let team = dir.path().join("team");
        std::fs::create_dir_all(&team).unwrap();
        set_folder(&paths, Some(&team)).unwrap();
        assert_eq!(folder(&paths).as_deref(), Some(team.as_path()));
        assert!(matches!(read(&paths), Status::Problem(_)), "no file yet");
        std::fs::write(team.join(FILE_NAME), GOOD.replace("aiq.team-tips.v1", "Secret Tool Name")).unwrap();
        match read(&paths) {
            Status::Problem(e) => assert!(!e.contains("Secret"), "{e}"),
            r => panic!("{r:?}"),
        }
        assert_eq!(line_on(&paths, "copilot-cli", &aiq_domain::coach::tip_id("check_code")), None, "broken file");
        std::fs::write(team.join(FILE_NAME), GOOD).unwrap();
        assert!(load(&paths).is_some());
        let id = aiq_domain::coach::tip_id("check_code");
        assert!(line_on(&paths, "copilot-cli", &id).is_some() && line_on(&paths, "vscode", &id).is_some());
        assert_eq!(line_on(&paths, "claude-code", &id), None, "never on Claude Code");
        set_folder(&paths, None).unwrap();
        assert!(matches!(read(&paths), Status::NotSet));
    }

    #[test]
    fn found_by_itself_next_to_other_repos_and_the_persons_choice_or_off_wins() {
        let home = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let paths = Paths::at(data.path());
        let repos = home.path().join("source/repos");
        for d in ["AIQ", "AIQ-Data", "other"] {
            std::fs::create_dir_all(repos.join(d)).unwrap();
        }
        // An unrelated team-tips.toml (not the format) is never picked up.
        std::fs::write(repos.join("AIQ/team-tips.toml"), "name = 1").unwrap();
        assert_eq!(refresh_found_from(&paths, Some(home.path()), None), "none");
        assert_eq!(source(&paths), Source::None);
        std::fs::write(repos.join("AIQ-Data").join(FILE_NAME), GOOD).unwrap();
        assert_eq!(refresh_found_from(&paths, Some(home.path()), None), "found_clone");
        assert_eq!(source(&paths), Source::Found(repos.join("AIQ-Data")));
        assert!(load(&paths).is_some(), "no step needed");
        // Off sticks, and the hourly search leaves it alone.
        set_off(&paths).unwrap();
        assert_eq!(refresh_found_from(&paths, Some(home.path()), None), "chosen_or_off");
        assert_eq!(source(&paths), Source::Off);
        assert!(load(&paths).is_none());
        // A chosen folder wins over a found one.
        set_folder(&paths, Some(&repos.join("other"))).unwrap();
        assert_eq!(source(&paths), Source::Chosen(repos.join("other")));
        // Back to automatic: found again on the next search; a deleted clone is forgotten.
        set_folder(&paths, None).unwrap();
        assert_eq!(refresh_found_from(&paths, Some(home.path()), None), "found_clone");
        std::fs::remove_file(repos.join("AIQ-Data").join(FILE_NAME)).unwrap();
        assert_eq!(refresh_found_from(&paths, Some(home.path()), None), "none");
        assert_eq!(source(&paths), Source::None);
    }

    /// The team's repo, downloaded with git (a local repo stands in for msft.ghe.com): no clone or step by the
    /// person, updates on the next search, an unreachable repo keeps the last good copy.
    #[test]
    fn downloads_the_teams_file_keeps_it_current_and_keeps_it_when_offline() {
        if std::process::Command::new("git").arg("--version").output().is_err() {
            return; // no git on this machine: the download can't run here
        }
        let g = |dir: &Path, args: &[&str]| {
            let ok = std::process::Command::new("git").args(args).current_dir(dir).output().unwrap().status.success();
            assert!(ok, "git {args:?}");
        };
        let team = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let paths = Paths::at(data.path());
        g(team.path(), &["init", "-q", "-b", "main"]);
        g(team.path(), &["config", "user.email", "t@example.com"]);
        g(team.path(), &["config", "user.name", "t"]);
        std::fs::write(team.path().join(FILE_NAME), GOOD).unwrap();
        g(team.path(), &["add", "."]);
        g(team.path(), &["commit", "-q", "-m", "one"]);
        let url = format!("file://{}", team.path().display());
        assert_eq!(refresh_found_from(&paths, None, Some(&url)), "downloaded");
        assert_eq!(source(&paths), Source::Found(paths.team_tips_download()));
        let id = aiq_domain::coach::tip_id("check_code");
        assert!(line_on(&paths, "copilot-cli", &id).unwrap().contains("Example Test Runner"));
        std::fs::write(team.path().join(FILE_NAME), GOOD.replace("Example Test Runner", "Example Two")).unwrap();
        g(team.path(), &["commit", "-q", "-am", "two"]);
        assert_eq!(refresh_found_from(&paths, None, Some(&url)), "updated");
        assert!(line_on(&paths, "copilot-cli", &id).unwrap().contains("Example Two"));
        // Offline or no access: the last good copy stays in use.
        let gone = format!("file://{}", data.path().join("nowhere").display());
        std::fs::remove_dir_all(paths.team_tips_download().join(".git")).unwrap();
        assert_eq!(refresh_found_from(&paths, None, Some(&gone)), "no_access");
        assert!(line_on(&paths, "copilot-cli", &id).is_some());
    }

    #[test]
    fn only_copilot_surfaces_may_show_a_team_tool() {
        assert!(allowed_on("copilot-cli") && allowed_on("vscode"));
        assert!(!allowed_on("claude-code") && !allowed_on("copilot") && !allowed_on(""));
    }
}
