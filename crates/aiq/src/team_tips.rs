//! `aiq team-tips add <folder> | check | remove`: point AIQ at a team tips file (aiq.team-tips.v1) and check it.
//! Output names habits by id and counts entries; it never prints a tool's name, link or text, so running it
//! from Claude Code or pasting it in a bug report shows nothing internal.
use aiq_contracts::team_tips::FILE_NAME;
use aiq_core::paths::Paths;
use aiq_core::team_tips::{self, Status};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "Usage: aiq team-tips add <folder> | aiq team-tips check | aiq team-tips remove";

pub fn cli(rest: &[&str]) -> ExitCode {
    let paths = Paths::from_env();
    match rest {
        ["add", folder, ..] => add(&paths, Path::new(folder)),
        ["check", ..] | [] => check(&paths),
        ["remove", ..] => remove(&paths),
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

fn remove(paths: &Paths) -> ExitCode {
    match team_tips::set_folder(paths, None) {
        Ok(()) => {
            println!("AIQ no longer reads a team tips file.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("Could not change it: {e}");
            ExitCode::from(1)
        }
    }
}

/// The report `check` prints, so tests can read it without the file system.
pub fn report(status: &Status, folder: Option<&Path>, habit_ids: &[&str]) -> (String, bool) {
    match status {
        Status::NotSet => ("No team tips file. Add one with: aiq team-tips add <folder>\n".into(), true),
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
    let (out, ok) = report(&team_tips::read(paths), team_tips::folder(paths).as_deref(), &ids);
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
        let (out, ok) = report(&Status::Ok(t), None, &["brief", "check_code"]);
        assert!(!ok);
        assert!(out.contains("Team tools for 1 of 2 habits.") && out.contains("gone_habit"), "{out}");
        assert!(!out.contains("Secret") && !out.contains("secret"), "{out}");
        let (out, ok) = report(&Status::Problem("schema must be x".into()), Some(Path::new("/t")), &[]);
        assert!(!ok && out.contains("Tips still show as usual"), "{out}");
    }
}
