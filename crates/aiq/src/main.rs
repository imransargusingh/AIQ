//! `aiq`: one binary, three roles (design: Shape).
//!   aiq hook claude-code <UserPromptSubmit|Stop>   short-lived hook client (always exits 0)
//!   aiq hook copilot <userPromptSubmitted|agentStop>   Copilot CLI / VS Code agent hook (no output)
//!   aiq statusline copilot                          Copilot CLI status-line command
//!   aiq core                                        long-lived core, one per OS user
//!   aiq install|uninstall claude-code, status, show <id>, pause, resume, config set daily-cap <n>, forget --all
mod cli;
mod hook;
mod install;
mod install_copilot;
mod spawn;
mod statusline;

use std::process::ExitCode;

const HELP: &str = "AIQ — private coaching for how you work with AI agents.

Usage:
  aiq install claude-code [--yes]     Add AIQ's hooks to ~/.claude/settings.json (shows the change first)
  aiq uninstall claude-code [--yes]   Remove only AIQ's hooks
  aiq status                          What AIQ is doing, cards today, store size, hook times
  aiq show <card-id>                  A card, why it showed, and the prompts it counted
  aiq pause | aiq resume              Stop or restart capture and tips
  aiq install copilot [--yes]         Add AIQ's hooks and status line to Copilot (~/.copilot)
  aiq uninstall copilot [--yes]       Remove only AIQ's Copilot hooks and status line
  aiq config set daily-cap <n>        Most tips per day, all tools together (0-50, default 5)
  aiq config set copilot-tip-stays <next-prompt|two-prompts-or-5-min|until-opened>
                                      How long a tip stays in Copilot CLI's status line
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
        ["install", "claude-code", ..] => install::install(yes),
        ["uninstall", "claude-code", ..] => install::uninstall(yes),
        ["install", "copilot", ..] => install_copilot::install(yes),
        ["uninstall", "copilot", ..] => install_copilot::uninstall(yes),
        ["status", ..] => cli::status(),
        ["show", id, ..] => cli::show(id),
        ["pause", ..] => cli::pause(true),
        ["resume", ..] => cli::pause(false),
        ["config", "set", name, value, ..] => cli::config_set(name, value),
        ["statusline", "copilot", ..] => statusline::run(),
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
