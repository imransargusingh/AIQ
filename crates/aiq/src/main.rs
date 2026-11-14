//! `aiq`: one binary, three roles (design: Shape).
//!   aiq hook claude-code <UserPromptSubmit|Stop>   short-lived hook client (always exits 0)
//!   aiq hook copilot <userPromptSubmitted|agentStop>   Copilot CLI / VS Code agent hook (no output)
//!   aiq statusline copilot                          Copilot CLI status-line command
//!   aiq core                                        long-lived core, one per OS user
//!   aiq install|uninstall claude-code, status, show <id>, pause, resume, config set daily-cap <n>, forget --all
//!   aiq import, forget --imported                   history import (docs/specs/history-import.md)
mod cli;
mod export;
mod hook;
mod import;
mod install;
mod install_copilot;
mod install_vscode;
mod judge;
mod judge_eval;
mod label;
mod spawn;
mod statusline;
mod team_tips;

use std::process::ExitCode;

const HELP: &str = "AIQ - private coaching for how you work with AI agents.

Usage:
  aiq install [--yes]                 Connect every AI tool installed here (Claude Code, Copilot, VS Code); shows each change first
  aiq install --new [--yes]           After setup, connect only a tool installed since AIQ last looked (AIQ home
                                      and the core run this at start; a tool you disconnected stays disconnected)
  aiq install claude-code [--yes]     Add AIQ's hooks to ~/.claude/settings.json (shows the change first)
  aiq uninstall claude-code [--yes]   Remove only AIQ's hooks
  aiq uninstall --all [--yes]         Disconnect AIQ from every tool; your data stays (aiq forget --all deletes it)
  aiq status                          What AIQ is doing, tips today, store size, hook times
  aiq status --report                 Write that to a file you can send for help (no prompt text,
                                      folders shown as folder-1, folder-2)
  aiq show [card-id]                  A tip, why it showed, and the prompts behind it (no id: the latest)
  aiq pause | aiq resume              Pause or resume AIQ (saving and tips)
  aiq endpoint                        Where the core listens (used by the VS Code extension)
  aiq install copilot [--yes]         Add AIQ's hooks and status line to Copilot (~/.copilot)
  aiq uninstall copilot [--yes]       Remove only AIQ's Copilot hooks and status line
  aiq install vscode [--yes]          Add AIQ's VS Code extension (tips after a Copilot agent turn) with VS Code's code command
  aiq uninstall vscode [--yes]        Remove AIQ's VS Code extension
  aiq config set daily-cap <n>        Most tips per day, all tools together (0-50, default 5)
  aiq config set copilot-tip-stays <next-prompt|two-prompts-or-5-min|until-opened>
  aiq config set copilot-tip-in-chat <on|off>   Copilot CLI tip also shown in the conversation (default on)
  aiq config set tip-popup <on|off>             Windows pop-up when a terminal tip shows (default on)
  aiq config set tip-highlight <on|off>         Yellow \"AIQ tip:\" label and VS Code colour (default on)
                                      How long a tip stays in Copilot CLI's status line
  aiq import [--dry-run]              Add your past Claude Code and Copilot CLI sessions to AIQ
  aiq include <folder>                Count a folder again that AIQ marked automated, not counted
  aiq team-tips                       Where your team's Tool to try lines come from (AIQ gets them by itself);
                                      also: sign-in, off, auto, add <folder>
  aiq helpful|not|snooze|off <card-id>  Tell AIQ what you thought of a tip (snooze: 7 days; off: never again)
  aiq on <tip-id>                     Let AIQ show a tip you turned off
  aiq flag <job>/<type> <move> <reason> [note]  This doesn't fit: stop a next step for one kind of task
  aiq unflag <job>/<type> <move>      Undo that: AIQ may suggest the step again
  aiq config set next-rung-tips <auto|on|off>  Next-step tips for kinds of task you repeat (auto: AIQ
                                      turns them off itself if it guesses the kind of task wrong)
  aiq config set copilot-mid-turn <on|off>  Count Copilot messages you send while the agent is working
                                      (read from Copilot's own history file on this computer)
  aiq config set agent-actions <on|off>  Count what the agent did in each turn: tool calls, commands,
                                      files, sub-agents (counts only, read from the tool's own log)
  aiq config set keep-on.<job or job/type> <on|auto>  Keep next-step tips showing for a job or kind the
                                      accuracy check paused (auto: AIQ decides again)
  aiq tips                            Each next-step tip, whether it is on, and how often it would show
                                      on your analyzed tasks (measured, no model call)
  aiq progress                        Each kind of task's typical rung, last 14 days against the 14
                                      before: which moved up or down (read-only, no model call)
  aiq analyze [--max <n>] [--only claude-code|copilot-cli] [--dry-run]
                                      Analyze your finished tasks through your own Claude Code / Copilot CLI
                                      (--dry-run shows what would be sent, sends nothing)
  aiq analyze --check [--sample <n>] [--no-job | --job <job>] [--dry-run]
                                      Check the analysis: a second model reads n analyzed tasks blind (default 30)
                                      and AIQ reports how often they agree (--dry-run: sample and cost only)
  aiq analyze --check-report            The latest check's numbers again, and what AIQ measured itself
  aiq analyze --survey-other [--sample <n>] [--dry-run]
                                      What kind of work your tasks with no job are (counts and generic labels only)
  aiq analyze --eval [--runs <n>] [--provider claude-code|copilot-cli] [--dry-run]
                                      Measure the analysis on AIQ's fixed set of made-up tasks with known answers,
                                      n runs each (default 2): accuracy and whether runs agree (no data of yours)
  aiq analyze --eval --replay           The same score from answers recorded in the repo (free, no model call)
  aiq analyze --eval-report             The latest eval again, and what moved since the one before
  aiq analyze --eval --compare-calls [file] [--with file2]  One call vs two calls per task, scored on
                                      recorded answers (files from --eval --runs n --record <file>; free).
                                      --with: a second model's answers, alone and paired one each
  aiq config set analysis <auto|off>     Analyze new tasks in the background (aiq install turns it on)
  aiq config set analysis-daily-tasks <n>  Most tasks analyzed per day (0-500, default 50)
  aiq config set analysis-copilot-sessions <delete|keep>  Delete AIQ's own Copilot analysis sessions (default)
  aiq config set analysis-calls <1|2>  Model calls per analyzed task (default 2)
  aiq config set analysis-copilot-model <id|default>  Model for Copilot tasks (default gpt-6.1-sol,
                                      then Copilot's own pick if it is not offered; default = Copilot's pick)
  aiq config set analysis-copilot-second-model <id|same>  Model for a Copilot task's second call
                                      (default gpt-6-luna; same = the first call's model)
  aiq config set kind-label <auto|off>  When word matching can't tell a new task's kind, a small model
                                      names it from the first prompt, in the background (on while analysis is on)
  aiq label --eval [--sample <n>] [--only claude-code|copilot-cli] [--dry-run]
                                      Measure kind labels against AIQ's analysis on your analyzed tasks (stores nothing)
  aiq export [<folder>]               Copy everything AIQ stored to plain JSON files you can read
                                      (default: a new folder under AIQ's own folder)
  aiq export --diagnostics [--prompts] [<folder>]
                                      One file on how AIQ ran these 14 days, to read and share if you
                                      choose (default: Downloads). No prompt text unless --prompts
  aiq forget --imported [--yes]       Delete only what aiq import added
  aiq forget --all [--yes]            Delete everything AIQ stored on this machine
  aiq update [--now]                  What AIQ's updater last did; --now checks for a newer release now
  aiq config set update <auto|ask|off>  Install updates by itself when idle (auto, default), tell you
                                      first (ask), or never check by itself (off)
  aiq core                            Run the background core in this terminal
  aiq version

Your data stays on this machine. Analysis calls go only through your own Copilot or Claude Code, and
once a day AIQ asks GitHub for a newer release (unless updates are off). Privacy and terms: docs/privacy-and-terms.md in the AIQ repo.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let yes = a.contains(&"--yes") || a.contains(&"-y");
    match a.as_slice() {
        ["hook", host, event, ..] => hook::run(host, event),
        ["core", ..] => cli::core(),
        ["install"] | ["install", "--yes" | "-y"] => install::install_found(yes),
        ["install", "--new", ..] => install::install_new(yes),
        ["install", "claude-code", ..] => install::install(yes),
        ["uninstall", "--all", ..] | ["uninstall", _, "--all", ..] => install::uninstall_all(yes),
        ["uninstall", "claude-code", ..] => install::uninstall(yes),
        ["install", "copilot", ..] => install_copilot::install(yes),
        ["uninstall", "copilot", ..] => install_copilot::uninstall(yes),
        ["install", "vscode", ..] => install_vscode::install(yes),
        ["uninstall", "vscode", ..] => install_vscode::uninstall(yes),
        ["status", rest @ ..] => cli::status(rest),
        ["show", id, ..] => cli::show(id),
        ["show"] => cli::show(""),
        ["pause", ..] => cli::pause(true),
        ["resume", ..] => cli::pause(false),
        ["config", "set", name, value, ..] => cli::config_set(name, value),
        ["include", folder, ..] => cli::include_folder(folder),
        ["team-tips", rest @ ..] => team_tips::cli(rest),
        [verb @ ("helpful" | "not" | "snooze" | "off" | "on"), id, ..] => cli::tip_feedback(verb, id),
        ["tips", ..] => cli::tips(),
        ["progress", ..] => cli::progress(),
        ["flag", rest @ ..] => cli::flag(rest, false),
        ["unflag", rest @ ..] => cli::flag(rest, true),
        ["statusline", "copilot", ..] => statusline::run(),
        ["endpoint", ..] => {
            println!("{}", aiq_core::paths::Paths::from_env().endpoint_json());
            ExitCode::SUCCESS
        }
        ["update", rest @ ..] => cli::update(rest),
        ["import", rest @ ..] => import::cli(rest),
        ["export", rest @ ..] => export::cli(rest),
        // `analyze` is the name users see; `judge` stays as a hidden alias so older commands and scripts keep working.
        ["analyze" | "judge", rest @ ..] => judge::cli(rest),
        ["label", rest @ ..] => label::cli(rest),
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
