//! `aiq`: one binary, three roles (design: Shape).
//!   aiq hook claude-code <UserPromptSubmit|Stop>   short-lived hook client (always exits 0)
//!   aiq hook copilot <userPromptSubmitted|agentStop>   Copilot CLI / VS Code agent hook (no output)
//!   aiq statusline copilot                          Copilot CLI status-line command
//!   aiq core                                        long-lived core, one per OS user
//!   aiq install|uninstall claude-code, status, show <id>, pause, resume, config set daily-cap <n>, forget --all
//!   aiq import, forget --imported                   history import (docs/specs/history-import.md)
mod cli;
mod hook;
mod import;
mod install;
mod install_copilot;
mod judge;
mod projects;
mod spawn;
mod statusline;

use std::process::ExitCode;

const HELP: &str = "AIQ — private coaching for how you work with AI agents.

Usage:
  aiq install [--yes]                 Connect every AI tool found here (Claude Code, Copilot); shows each change first
  aiq install claude-code [--yes]     Add AIQ's hooks to ~/.claude/settings.json (shows the change first)
  aiq uninstall claude-code [--yes]   Remove only AIQ's hooks
  aiq status                          What AIQ is doing, cards today, store size, hook times
  aiq show <card-id>                  A card, why it showed, and the prompts it counted
  aiq pause | aiq resume              Stop or restart capture and tips
  aiq endpoint                        Where the core listens (used by the VS Code extension)
  aiq install copilot [--yes]         Add AIQ's hooks and status line to Copilot (~/.copilot)
  aiq uninstall copilot [--yes]       Remove only AIQ's Copilot hooks and status line
  aiq config set daily-cap <n>        Most tips per day, all tools together (0-50, default 5)
  aiq config set copilot-tip-stays <next-prompt|two-prompts-or-5-min|until-opened>
                                      How long a tip stays in Copilot CLI's status line
  aiq import [--dry-run]              Add your past Claude Code and Copilot CLI sessions to AIQ
  aiq include <folder>                Watch a folder AIQ set aside as automated (an app or script's runs)
  aiq helpful|not|snooze|off <card-id>  Tell AIQ what you thought of a tip (snooze: 7 days; off: never again)
  aiq on <tip-id>                     Let AIQ show a tip you turned off
  aiq flag <job>/<type> <move> <reason> [note]  This doesn't fit: stop a next step for one kind of task
  aiq unflag <job>/<type> <move>      Undo a flag
  aiq config set next-rung-tips <auto|on|off>  Next-step tips for kinds of task you repeat (auto: AIQ
                                      turns them off itself if it guesses the kind of task wrong)
  aiq judge [--max <n>] [--only claude-code|copilot-cli] [--dry-run]
                                      Judge your finished tasks through your own Claude Code / Copilot CLI
                                      (--dry-run shows what would be sent, sends nothing)
  aiq config set judge <auto|off>     Judge new tasks in the background (on after your first aiq judge)
  aiq config set judge-daily-tasks <n>  Most tasks judged per day (0-500, default 50)
  aiq config set judge-copilot-sessions <delete|keep>  Delete AIQ's own Copilot judge sessions (default)
  aiq forget --imported [--yes]       Delete only what aiq import added
  aiq forget --all [--yes]            Delete everything AIQ stored on this machine
  aiq core                            Run the background core in this terminal
  aiq version

Everything stays on this machine.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let yes = a.contains(&"--yes") || a.contains(&"-y");
    match a.as_slice() {
        ["hook", host, event, ..] => hook::run(host, event),
        ["core", ..] => cli::core(),
        ["install"] | ["install", "--yes" | "-y"] => install::install_found(yes),
        ["install", "claude-code", ..] => install::install(yes),
        ["uninstall", "claude-code", ..] => install::uninstall(yes),
        ["install", "copilot", ..] => install_copilot::install(yes),
        ["uninstall", "copilot", ..] => install_copilot::uninstall(yes),
        ["status", ..] => cli::status(),
        ["show", id, ..] => cli::show(id),
        ["pause", ..] => cli::pause(true),
        ["resume", ..] => cli::pause(false),
        ["config", "set", name, value, ..] => cli::config_set(name, value),
        ["include", folder, ..] => cli::include_folder(folder),
        [verb @ ("helpful" | "not" | "snooze" | "off" | "on"), id, ..] => cli::tip_feedback(verb, id),
        ["flag", rest @ ..] => cli::flag(rest, false),
        ["unflag", rest @ ..] => cli::flag(rest, true),
        ["statusline", "copilot", ..] => statusline::run(),
        ["endpoint", ..] => {
            println!("{}", aiq_core::paths::Paths::from_env().endpoint_json());
            ExitCode::SUCCESS
        }
        ["import", rest @ ..] => import::cli(rest),
        ["judge", rest @ ..] => judge::cli(rest),
        ["forget", "--imported", ..] | ["forget", _, "--imported", ..] => import::forget(yes),
        ["forget", "--all", ..] | ["forget", _, "--all", ..] => cli::forget_all(yes),
        ["version" | "--version" | "-V", ..] => {
            println!("aiq {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        [] | ["help" | "--help" | "-h", ..] => {
            println!("{HELP}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{HELP}");
            ExitCode::from(2)
        }
    }
}
