//! `aiq <command> --help` prints help and does nothing else (DC-093): on 11-15 `aiq import --help` ran a real
//! import on the person's laptop because the command read only `--dry-run` and ignored every other argument.
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_aiq");

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .env("AIQ_HOME", dir.join("home"))
        .env("CLAUDE_CONFIG_DIR", dir.join("claude"))
        .env("COPILOT_HOME", dir.join("copilot"))
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// Every file under `dir`, so a test can show a command wrote nothing at all.
fn files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(files(&p));
            } else {
                out.push(p.display().to_string());
            }
        }
    }
    out
}

#[test]
fn every_command_given_help_prints_its_usage_and_writes_nothing() {
    // The commands that write, spend a request or start something when they run.
    let commands: &[&[&str]] = &[
        &["import"],
        &["import", "--dry-run"],
        &["analyze"],
        &["analyze", "--check"],
        &["judge"],
        &["label", "--eval"],
        &["pause"],
        &["resume"],
        &["install"],
        &["install", "claude-code"],
        &["install", "copilot"],
        &["uninstall", "--all"],
        &["forget", "--all"],
        &["forget", "--imported"],
        &["export"],
        &["update", "--now"],
        &["team-tips", "sign-in"],
        &["include", "somewhere"],
        &["core"],
    ];
    for cmd in commands {
        for flag in ["--help", "-h"] {
            let dir = TempDir::new().unwrap();
            let mut args = cmd.to_vec();
            args.push(flag);
            let o = run(dir.path(), &args);
            let out = String::from_utf8_lossy(&o.stdout);
            assert_eq!(o.status.code(), Some(0), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
            let shown = if cmd[0] == "judge" { "analyze" } else { cmd[0] };
            assert!(out.contains(&format!("aiq {shown}")), "{args:?} printed no usage for {shown}: {out}");
            assert_eq!(files(dir.path()), Vec::<String>::new(), "{args:?} wrote files");
        }
    }
}

#[test]
fn help_for_one_command_shows_only_its_lines() {
    let dir = TempDir::new().unwrap();
    let out = String::from_utf8_lossy(&run(dir.path(), &["import", "--help"]).stdout).to_string();
    assert!(out.contains("aiq import [--dry-run]"), "{out}");
    assert!(!out.contains("aiq analyze"), "{out}");
    let out = String::from_utf8_lossy(&run(dir.path(), &["analyze", "-h"]).stdout).to_string();
    assert!(out.contains("--check [--sample <n>]") && out.contains("second model reads"), "{out}");
    assert!(!out.contains("aiq import"), "{out}");
}

#[test]
fn import_refuses_an_argument_it_does_not_know_before_reading_anything() {
    let dir = TempDir::new().unwrap();
    let o = run(dir.path(), &["import", "--dryrun"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("only --dry-run"));
    assert_eq!(files(dir.path()), Vec::<String>::new(), "a refused import wrote files");
}
