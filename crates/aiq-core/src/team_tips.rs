//! The team tips file (`aiq.team-tips.v1`, see `aiq_contracts::team_tips`): where it is and how it's read.
//!
//! `aiq team-tips add <folder>` writes the folder's path to `team-tips-folder.txt` in AIQ's data folder; every
//! reader (the Copilot hook, the status line, `aiq show`, Home) reads `<folder>/team-tips.toml` in place, so a
//! `git pull` of the team's repo updates it. Reading never fails a tip: any problem gives `None` plus a reason
//! that names no tool. The file's text is never written to the store, a log or the core: callers render the
//! line and drop it.
use crate::paths::Paths;
use aiq_contracts::team_tips::{FILE_NAME, MAX_BYTES, TeamTipsV1};
use std::io::Read;
use std::path::PathBuf;

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

/// The folder the person chose, if any.
pub fn folder(paths: &Paths) -> Option<PathBuf> {
    let text = std::fs::read_to_string(paths.team_tips_pointer()).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| PathBuf::from(line))
}

pub fn set_folder(paths: &Paths, folder: Option<&std::path::Path>) -> std::io::Result<()> {
    let _ = paths.ensure_dirs();
    match folder {
        Some(f) => crate::paths::write_user_only(&paths.team_tips_pointer(), format!("{}\n", f.display()).as_bytes()),
        None => match std::fs::remove_file(paths.team_tips_pointer()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
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
    fn only_copilot_surfaces_may_show_a_team_tool() {
        assert!(allowed_on("copilot-cli") && allowed_on("vscode"));
        assert!(!allowed_on("claude-code") && !allowed_on("copilot") && !allowed_on(""));
    }
}
