#!/usr/bin/env python3
"""git-identity-guard.py -- PK-07's mechanical control: never invent a git identity.

AL0.2 states the rule in prose: neither a pack script nor the agent running it may run
`git config user.*` (or an inline `git -c user.*=...`) to force a blocked commit through;
a failed commit is left uncommitted and the gap is reported. A sweep test
(`tests/docs_explorer/test_no_git_identity.py`) already pins this against the PACK'S OWN
source. This hook is the second half: it runs at the PreToolUse seam in a CONSUMING repo
and refuses the shell command itself, so the rule holds even when the agent forgets it.

It does not filter by tool name (Bash, run_terminal_cmd, ... differ per host and are not
all verified in this pack) -- it inspects whatever command-like string the tool call
carries (`command`/`cmd`/`script`/`shellCommand`/`commandLine`, the shapes already used
elsewhere in this hook family). It splits the text into simple commands with `shlex` and
refuses one that SETS an identity: a `git config` write of user/author/committer name or
email in any scope or `set` form, an inline `-c` or `--config-env` override, `--author` on
`commit`, or a `GIT_AUTHOR_*` / `GIT_COMMITTER_*` assignment (shell or `$env:`). It looks
through git's global options and one `bash -c` / `pwsh -Command` layer, and never inside a
quoted argument. A read (`git config user.name`, `--get`, `get`) never matches, so a
legitimate check-before-you-commit is never refused.

Per-host block form (grounded from the hosts' own contracts the rest of this hook family
already relies on, `pack/adapters/hooks/README.md`):
  Claude Code   PreToolUse: exit 2, reason on stderr -- blocks the call.
  Grok Build    Claude-format PreToolUse: exit 2, reason on stderr -- blocks the call.
  Copilot CLI   preToolUse: exit 2 denies the call (reread-guard.py's own documented
                "never exit 2 on preToolUse: that denies"); reason on stderr.
  Antigravity   has a PreToolUse event (reread-guard's own `view_file` matcher proves
                it fires), but this pack has never observed agy's PreToolUse DENY
                contract (only `{"decision":"allow"}` is verified, by reread-guard).
                Rather than guess a deny shape that might silently fail open OR block
                everything it touches, agy is left unwired here; PK-07 stays prose-only
                for agy (AL0.2, `audit-and-change-log.md`).

Usage: git-identity-guard.py --host claude|grok|copilot
"""
import argparse
import json
import re
import shlex
import sys

# The host pipes a JSON payload in and reads a line out. Same console-encoding guard as
# every other hook in this family (DC-211/PLAT-A): every arm is fail-open.
for _stream, _kw in ((sys.stdin, {"encoding": "utf-8", "errors": "replace"}),
                     (sys.stdout, {"encoding": "utf-8", "errors": "replace"}),
                     (sys.stderr, {"encoding": "utf-8", "errors": "replace"})):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(**_kw)
        except (ValueError, OSError, UnicodeError):
            pass

COMMAND_KEYS = ("command", "cmd", "script", "shellCommand", "commandLine")

# FR-088: the guard reads COMMAND WORDS. A regex over the raw text let six setting forms
# through (`git -C . config`, `--file`, `--worktree`, `config set`, `GIT_AUTHOR_*=`, `--author`)
# and refused `echo '... git config user.name X ...'`, which runs no git at all.
IDENTITY_KEY_RX = re.compile(r"(?:user|author|committer)\.(?:name|email)$", re.IGNORECASE)
IDENTITY_ENV_RX = re.compile(r"(?:\$env:)?GIT_(?:AUTHOR|COMMITTER)_(?:NAME|EMAIL)$", re.IGNORECASE)
OPERATORS = set("();<>|&\n")
SHELLS = {"bash": "-c", "sh": "-c", "zsh": "-c", "dash": "-c", "cmd": "/c",
          "pwsh": "-command", "powershell": "-command"}
GIT_OPTS_WITH_VALUE = {"-C", "-c", "--git-dir", "--work-tree", "--namespace", "--config-env",
                       "--super-prefix", "--exec-path"}
CONFIG_OPTS_WITH_VALUE = {"-f", "--file", "--blob", "--type", "--default", "--comment", "--value"}
CONFIG_READ_OPTS = {"--get", "--get-all", "--get-regexp", "--get-urlmatch", "--list", "-l",
                    "--unset", "--unset-all", "--get-color", "--get-colorbool", "-e", "--edit"}
COMMIT_OPTS_WITH_VALUE = {"-m", "--message", "-F", "--file", "-C", "--reuse-message", "-c",
                          "--reedit-message", "-t", "--template", "--fixup", "--squash",
                          "--trailer", "--cleanup", "-S", "--gpg-sign"}

# The fallback for text shlex cannot split (an unbalanced quote): the narrow pattern this
# guard used before FR-088. Over-matching is the safe error in a command that will not parse.
GIT_IDENTITY_SET_RX = re.compile(
    r"git\s+config\s+(?:--global\s+|--local\s+|--system\s+)?(?:--add\s+)?"
    r"user\.(?:name|email)\s*(?:=|\s+(?![|&;<>]|\d+>)\S)"
    r"|-c\s+user\.(?:name|email)\s*=",
    re.IGNORECASE,
)

REASON = ("git-identity guard (PK-07): this command sets a git identity (user.name/user.email) "
          "to force a commit through. Leave the change uncommitted and report the gap instead "
          "of inventing an author -- AL0.2.")


def _command_text(host, payload):
    """The command-like string from whatever shape this host's tool-call args take. Not
    filtered by tool name (that varies per host and is unverified for several of them);
    any tool call carrying one of COMMAND_KEYS is inspected."""
    if host in ("claude", "grok"):
        event = str(payload.get("hook_event_name") or payload.get("hookEventName") or "")
        if event and event != "PreToolUse":
            return None
        args = payload.get("tool_input") or payload.get("toolInput") or {}
    elif host == "copilot":
        args = payload.get("toolArgs") or {}
    else:
        args = {}
    for key in COMMAND_KEYS:
        value = args.get(key)
        if isinstance(value, str) and value.strip():
            return value
    return None


def _simple_commands(text):
    """The command text split into simple commands (lists of words) at shell operators and
    newlines. Raises ValueError on text shlex cannot split."""
    lex = shlex.shlex(text, posix=True, punctuation_chars="".join(OPERATORS))
    lex.whitespace = " \t\r"
    lex.whitespace_split = True
    commands, words = [], []
    for token in lex:
        if token and set(token) <= OPERATORS:
            if token[0] in "<>" and words and words[-1].isdigit():
                words.pop()                         # the `2` of `2>`: a descriptor, not a value
            commands.append(words)
            words = []
        else:
            words.append(token)
    commands.append(words)
    return [c for c in commands if c]


def _sets_env_identity(word, following):
    """`GIT_AUTHOR_NAME=x`, `$env:GIT_AUTHOR_NAME='x'`, or `$env:GIT_AUTHOR_NAME = 'x'`."""
    name, eq, _ = word.partition("=")
    return bool(IDENTITY_ENV_RX.match(name)) and (bool(eq) or following[:1] == ["="])


def _config_sets_identity(args):
    positional, reading, adding = [], False, False
    i = 0
    while i < len(args):
        arg = args[i]
        if arg in CONFIG_OPTS_WITH_VALUE:
            i += 2
            continue
        if arg.startswith("-"):
            reading = reading or arg.split("=", 1)[0] in CONFIG_READ_OPTS
            adding = adding or arg in ("--add", "--replace-all")
        else:
            positional.append(arg)
        i += 1
    if reading or not positional:
        return False
    if positional[0] == "set":
        return len(positional) > 1 and bool(IDENTITY_KEY_RX.match(positional[1]))
    key = positional[0].split("=", 1)[0]
    if not IDENTITY_KEY_RX.match(key):
        return False
    return "=" in positional[0] or len(positional) > 1 or adding


def _commit_sets_author(args):
    i = 0
    while i < len(args):
        arg = args[i]
        if arg in COMMIT_OPTS_WITH_VALUE:
            i += 2
            continue
        if arg == "--author" or arg.startswith("--author="):
            return True
        i += 1
    return False


def _git_sets_identity(args):
    """`args` follow the git executable: global options, then the subcommand and its own."""
    i = 0
    while i < len(args) and args[i].startswith("-"):
        option, eq, value = args[i].partition("=")
        if option in GIT_OPTS_WITH_VALUE and not eq:
            value = args[i + 1] if i + 1 < len(args) else ""
            i += 1
        if option in ("-c", "--config-env") and IDENTITY_KEY_RX.match(value.split("=", 1)[0]):
            return True
        i += 1
    if i >= len(args):
        return False
    rest = args[i + 1:]
    return {"config": _config_sets_identity, "commit": _commit_sets_author}.get(
        args[i], lambda _: False)(rest)


def _sets_identity(text, depth=0):
    for words in _simple_commands(text):
        i = 0
        while i < len(words):                       # leading assignments, `env`, `export`
            word = words[i]
            if _sets_env_identity(word, words[i + 1:]):
                return True
            if word.lower() in ("env", "export", "set", "setx") or "=" in word:
                i += 1
                continue
            break
        if i >= len(words):
            continue
        program = re.split(r"[\\/]", words[i])[-1].lower()
        program = program[:-4] if program.endswith(".exe") else program
        if program == "git" and _git_sets_identity(words[i + 1:]):
            return True
        flag = SHELLS.get(program)
        if flag and depth < 3:
            args = [w.lower() for w in words[i + 1:]]
            if flag in args and args.index(flag) + 1 < len(args):
                inner = words[i + 1 + args.index(flag) + 1]
                if _sets_identity(inner, depth + 1):
                    return True
    return False


def evaluate(host, payload):
    """Pure decision: returns a reason string to block on, or None to allow. Exercised
    directly by the tests."""
    command = _command_text(host, payload)
    if not command:
        return None
    try:
        found = _sets_identity(command)
    except ValueError:
        found = bool(GIT_IDENTITY_SET_RX.search(command))
    return REASON if found else None


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", choices=["claude", "copilot", "grok"], required=True)
    args = ap.parse_args(argv)
    try:
        raw = sys.stdin.read()
        payload = json.loads(raw) if raw.strip() else {}
    except ValueError:
        return 0  # fail open: an unreadable payload must never block a real tool call
    if not isinstance(payload, dict):
        return 0
    reason = evaluate(args.host, payload)
    if not reason:
        return 0
    print(reason, file=sys.stderr)
    return 2  # claude, grok (Claude-format) and copilot all deny preToolUse on exit 2


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception:  # noqa: BLE001 - a guard must never break the host's tool call
        sys.exit(0)
