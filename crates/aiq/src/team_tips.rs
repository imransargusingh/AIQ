//! `aiq team-tips [check] | add <folder> | off | auto | sign-in`: see and change where AIQ gets the team tips file
//! (aiq.team-tips.v1). Nobody needs these by default: the core downloads the team's file or finds a clone by itself.
//! Output names habits by id and counts entries; it never prints a tool's name, link or text, so running it
//! from Claude Code or pasting it in a bug report shows nothing internal.
use aiq_contracts::team_tips::FILE_NAME;
use aiq_core::paths::Paths;
use aiq_core::team_tips::{self, Status};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "Usage: aiq team-tips [check] | add <folder> | off | auto | sign-in";

pub fn cli(rest: &[&str]) -> ExitCode {
    let paths = Paths::from_env();
    match rest {
        ["add", folder, ..] => add(&paths, Path::new(folder)),
        ["check", ..] | [] => check(&paths),
        ["off" | "remove", ..] => off(&paths),
        ["auto", ..] => auto(&paths),
        ["sign-in", ..] => {
            println!("A sign-in window may open. Finish it there.");
            team_tips::sign_in(&paths);
            check(&paths)
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn add(paths: &Paths, folder: &Path) -> ExitCode {
    let folder: PathBuf = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_owned());
    if !folder.join(FILE_NAME).is_file() {
        println!("No {FILE_NAME} in {}. Nothing changed.", folder.display());
        return ExitCode::from(1);
    }
    if let Err(e) = team_tips::set_folder(paths, Some(&folder)) {
        println!("Could not save the folder: {e}");
        return ExitCode::from(1);
    }
    println!("AIQ will read team tips from {}.", folder.display());
    check(paths)
}

fn auto(paths: &Paths) -> ExitCode {
    if let Err(e) = team_tips::set_folder(paths, None) {
        println!("Could not change it: {e}");
        return ExitCode::from(1);
    }
    team_tips::refresh_found(paths, team_tips::user_home().as_deref());
    check(paths)
}

fn off(paths: &Paths) -> ExitCode {
    match team_tips::set_off(paths) {
        Ok(()) => {
            println!("Team tips are off. Turn them back on with: aiq team-tips auto");
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("Could not change it: {e}");
            ExitCode::from(1)
        }
    }
}

/// The report `check` prints, so tests can read it without the file system.
/// Where the file comes from, in words (no address, no tool names).
pub fn source_line(source: &team_tips::Source, outcome: Option<&str>) -> String {
    match source {
        team_tips::Source::Off => "Team tips are off. Turn them back on with: aiq team-tips auto\n".into(),
        team_tips::Source::Chosen(p) => format!("Team tips from the folder you chose: {}\n", p.display()),
        team_tips::Source::Found(p) if outcome == Some("no_access") => {
            format!(
                "Team tips from AIQ's last download (it couldn't reach your team's repo just now): {}\n",
                p.display()
            )
        }
        team_tips::Source::Found(p) => format!("Team tips found by AIQ: {}\n", p.display()),
        team_tips::Source::None if outcome == Some("no_access") => {
            "AIQ couldn't reach your team's tips repo. Sign in once with: aiq team-tips sign-in\n".into()
        }
        team_tips::Source::None if outcome == Some("git_missing") => {
            "AIQ needs Git for Windows to get your team's tips.\n".into()
        }
        team_tips::Source::None => "No team tips file.\n".into(),
    }
}

pub fn report_from(
    status: &Status,
    folder: Option<&Path>,
    habit_ids: &[&str],
    source: &team_tips::Source,
    outcome: Option<&str>,
) -> (String, bool) {
    let head = source_line(source, outcome);
    let (body, ok) = report_body(status, folder, habit_ids);
    (format!("{head}{body}"), ok)
}

fn report_body(status: &Status, folder: Option<&Path>, habit_ids: &[&str]) -> (String, bool) {
    match status {
        Status::NotSet => (String::new(), true),
        Status::Problem(e) => {
            let at = folder.map(|f| f.join(FILE_NAME).display().to_string()).unwrap_or_default();
            (format!("AIQ can't use {at}: {e}.\nTips still show as usual, without team tools.\n"), false)
        }
        Status::Ok(t) => {
            let unknown: Vec<&str> =
                t.tools.iter().map(|x| x.move_id.as_str()).filter(|id| !habit_ids.contains(id)).collect();
            let used = t.tools.len() - unknown.len();
            let mut out = format!("Team tools for {used} of {} habits.\n", habit_ids.len());
            if !unknown.is_empty() {
                out +=
                    &format!("Skipped {} entries for habits AIQ doesn't have: {}\n", unknown.len(), unknown.join(", "));
            }
            out += "They show on Copilot CLI, VS Code and Home. Never on Claude Code.\n";
            (out, unknown.is_empty())
        }
    }
}

fn check(paths: &Paths) -> ExitCode {
    let pack =
        aiq_domain::PackSource::load(&aiq_core::pack::BundledPack).unwrap_or_else(|e| panic!("bundled pack: {e}"));
    let ids: Vec<&str> = pack.moves.iter().map(|m| m.move_id.as_str()).collect();
    let (out, ok) = report_from(
        &team_tips::read(paths),
        team_tips::folder(paths).as_deref(),
        &ids,
        &team_tips::source(paths),
        team_tips::last_outcome(paths).as_deref(),
    );
    print!("{out}");
    if ok { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_reports_counts_and_ids_never_tool_text() {
        let t = team_tips::parse(
            r#"schema = "aiq.team-tips.v1"
[[tools]]
move_id = "check_code"
tool = "Secret Runner"
how = "secret words."
[[tools]]
move_id = "gone_habit"
tool = "Secret Two"
how = "more secret words."
"#,
        )
        .unwrap();
        let (out, ok) = report_from(&Status::Ok(t), None, &["brief", "check_code"], &team_tips::Source::None, None);
        assert!(!ok);
        assert!(out.contains("Team tools for 1 of 2 habits.") && out.contains("gone_habit"), "{out}");
        assert!(!out.contains("Secret") && !out.contains("secret"), "{out}");
        let (out, ok) = report_from(
            &Status::Problem("schema must be x".into()),
            Some(Path::new("/t")),
            &[],
            &team_tips::Source::None,
            None,
        );
        assert!(!ok && out.contains("Tips still show as usual"), "{out}");
        let (out, _) = report_from(&Status::NotSet, None, &[], &team_tips::Source::None, Some("no_access"));
        assert!(out.contains("aiq team-tips sign-in"), "{out}");
        let (out, _) = report_from(&Status::NotSet, None, &[], &team_tips::Source::Off, None);
        assert!(out.contains("aiq team-tips auto"), "{out}");
    }
}
