#!/usr/bin/env python3
"""pack-doctor.py — AI-Forward install-health check (deployable; runs in a TARGET repo).

Reports whether THIS repo has the pack installed and healthy: the installed revision, both
tool surfaces present, the managed blocks intact, and the knowledge graph valid + fresh.
One PASS/WARN/FAIL line per check with a suggested fix; exit 1 if any FAIL, or if any WARN
is present under --strict.

Distinct from tools/check-consistency.py (which validates the pack SOURCE — pack/ == docs).
A target repo has no pack/, so this checks INSTALL health, not source consistency.
Design: docs/design/pack-doctor.md. Stdlib only; composes docs-graph.py for the graph half.

Usage
  pack-doctor.py [--root <repo>] [--json] [--strict]
Exit: 0 all PASS/WARN (or all PASS under --strict) · 1 any FAIL/strict WARN.
"""
import argparse, json, os, re, shutil, subprocess, sys

from bounded_process import run_bounded

# Windows consoles default to cp1252, which cannot encode the box/arrow glyphs this
# tool prints - `prompt-log.py --help` crashed outright with UnicodeEncodeError (FR-047).
# The other scripts survived only because their glyphs happen to exist in cp1252, which is
# luck rather than an invariant, so the guard is applied uniformly.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass


PASS, WARN, FAIL = "PASS", "WARN", "FAIL"


def _result(name, status, detail, fix=""):
    return {"name": name, "status": status, "detail": detail, "fix": fix}


def check_installed(root):
    p = os.path.join(root, "docs", "ai-forward-pack", "INSTALL.md")
    if not os.path.exists(p):
        return _result("pack installed", FAIL,
                       "docs/ai-forward-pack/INSTALL.md not found",
                       "run /addpacktorepo (this repo has no pack installed)"), None
    with open(p, encoding="utf-8", errors="replace") as install_file:
        text = install_file.read()
    m = re.search(r"^revision:\s*(\d+)", text, re.M)
    if not m:
        return _result("pack installed", WARN, "INSTALL.md present but revision unreadable",
                       "re-run /updatepack to restamp the revision"), None
    rev = m.group(1)
    bv = re.search(r"^bundle_version:\s*'([^']+)'", text, re.M)
    detail = f"revision {rev}" + (f" ({bv.group(1)})" if bv else "")
    return _result("pack installed", PASS, detail), rev


def check_surface(root, label, subdirs):
    missing = [d for d in subdirs if not os.path.exists(os.path.join(root, *d.split("/")))]
    names = {".claude": "Claude Code", ".github": "Copilot", ".grok": "Grok Build", ".agents": "Antigravity"}
    surface = names.get(label, label)
    if not os.path.isdir(os.path.join(root, label)):
        return _result(f"{surface} surface", FAIL, f"{label}/ not present",
                       "run /updatepack (or pwsh tools/sync-pack.ps1 in the source repo)")
    if missing:
        return _result(f"{surface} surface", FAIL, f"missing: {', '.join(missing)}",
                       "run /updatepack to restore the full surface")
    return _result(f"{surface} surface", PASS, f"{label}/{{{','.join(s.split('/')[-1] for s in subdirs)}}} present")


IMPORT_RX = re.compile(r"^\s*@AGENTS\.md\s*$", re.M)
MARKER_RX = re.compile(r"AI-FORWARD-PACK:BEGIN")


def _read(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return None


def check_codex(root):
    """Check the deployed pack contract, never claim a running Codex catalog was read."""
    name = "Codex repository readiness"
    fix = "run $updatepack (or pwsh tools/sync-pack.ps1 in the source repo); see docs/ai-forward-pack/codex.md"
    problems = []
    required = ["AGENTS.md", "docs/ai-forward-pack/codex.md",
                "docs/ai-forward-pack/scripts/audit-log.py",
                "docs/ai-forward-pack/scripts/prompt-log.py",
                "docs/ai-forward-pack/scripts/docs-graph.py"]
    required += [".claude/knowledge/" + doc + ".md" for doc in (
        "agent-body-of-knowledge", "agent-rules-of-the-road", "agent-persona-catalog",
        "layered-optimized-architecture", "engineering-governance")]
    for rel in required:
        if not (_read(os.path.join(root, rel)) or "").strip():
            problems.append("missing or empty " + rel)
    agents = _read(os.path.join(root, "AGENTS.md")) or ""
    if "docs/ai-forward-pack/codex.md" not in agents:
        problems.append("AGENTS.md does not direct Codex to its grounding guide")

    manifest = "docs/ai-forward-pack/codex-skills.json"
    try:
        inventory = json.loads(_read(os.path.join(root, manifest)) or "null")
    except ValueError:
        inventory = None
    if not isinstance(inventory, dict) or not inventory:
        return _result(name, FAIL, "; ".join(problems + ["missing or invalid " + manifest]), fix)
    for skill, files in inventory.items():
        # Only the pack's portable relative-path inventory is admitted. Local extra
        # skills are allowed and do not substitute for an absent pack-owned skill.
        if (not re.fullmatch(r"[a-z0-9-]+", skill) or not isinstance(files, list)
                or "SKILL.md" not in files or any(
                    not isinstance(f, str) or not f or "\\" in f
                    or f.startswith("/") or ":" in f or ".." in f.split("/") for f in files)):
            problems.append("invalid skill inventory entry " + skill)
            continue
        for rel in files:
            target = ".agents/skills/" + skill + "/" + rel
            if not os.path.isfile(os.path.join(root, target)):
                problems.append("missing " + target)
        target = ".agents/skills/" + skill + "/SKILL.md"
        text = _read(os.path.join(root, target)) or ""
        header = re.match(r"\A---\r?\n(.*?)\r?\n---(?:\r?\n|$)", text, re.S)
        fields = {}
        if header:
            for key in ("name", "description"):
                match = re.search(r"^" + key + r":[ \t]*([^\r\n]+)", header.group(1), re.M)
                fields[key] = match.group(1).strip().strip("\"'") if match else ""
        # The pack emits single-line scalar metadata. This is a pack contract
        # check, not a general YAML validator for arbitrary user skills.
        if fields.get("name") != skill or not fields.get("description", "").strip():
            problems.append("invalid name/description in " + target)
    if problems:
        return _result(name, FAIL, "; ".join(problems), fix)
    if (_read(os.path.join(root, "AGENTS.override.md")) or "").strip():
        return _result(name, WARN, "AGENTS.override.md takes precedence over AGENTS.md; pack grounding may be shadowed",
                       "review the override with its owner; do not overwrite it automatically")
    return _result(name, PASS,
                   "%d pack skills, companion files, constitution and scripts present; filesystem only, Codex catalog not queried"
                   % len(inventory))


def check_muse(root):
    """Check the deployed Muse contract, never claim a running Muse catalog was read.

    Muse discovers the shared .agents/skills surface (the Codex/Antigravity
    tree) and reads AGENTS.md; its own contribution is the path map
    docs/ai-forward-pack/muse-surface.md plus the AGENTS.md pointer to it.
    The skill inventory is the shared codex-skills.json manifest."""
    name = "Muse repository readiness"
    fix = "run $updatepack (or pwsh tools/sync-pack.ps1 in the source repo); see docs/ai-forward-pack/muse-surface.md"
    problems = []
    required = ["AGENTS.md", "docs/ai-forward-pack/muse-surface.md",
                "docs/ai-forward-pack/scripts/audit-log.py",
                "docs/ai-forward-pack/scripts/prompt-log.py",
                "docs/ai-forward-pack/scripts/docs-graph.py"]
    required += [".claude/knowledge/" + doc + ".md" for doc in (
        "agent-body-of-knowledge", "agent-rules-of-the-road", "agent-persona-catalog",
        "layered-optimized-architecture", "engineering-governance")]
    for rel in required:
        if not (_read(os.path.join(root, rel)) or "").strip():
            problems.append("missing or empty " + rel)
    agents = _read(os.path.join(root, "AGENTS.md")) or ""
    if "docs/ai-forward-pack/muse-surface.md" not in agents:
        problems.append("AGENTS.md does not direct Muse to its grounding guide")

    manifest = "docs/ai-forward-pack/codex-skills.json"
    try:
        inventory = json.loads(_read(os.path.join(root, manifest)) or "null")
    except ValueError:
        inventory = None
    if not isinstance(inventory, dict) or not inventory:
        return _result(name, FAIL, "; ".join(problems + ["missing or invalid " + manifest]), fix)
    for skill, files in inventory.items():
        if (not re.fullmatch(r"[a-z0-9-]+", skill) or not isinstance(files, list)
                or "SKILL.md" not in files or any(
                    not isinstance(f, str) or not f or "\\" in f
                    or f.startswith("/") or ":" in f or ".." in f.split("/") for f in files)):
            problems.append("invalid skill inventory entry " + skill)
            continue
        for rel in files:
            target = ".agents/skills/" + skill + "/" + rel
            if not os.path.isfile(os.path.join(root, target)):
                problems.append("missing " + target)
        target = ".agents/skills/" + skill + "/SKILL.md"
        text = _read(os.path.join(root, target)) or ""
        header = re.match(r"\A---\r?\n(.*?)\r?\n---(?:\r?\n|$)", text, re.S)
        fields = {}
        if header:
            for key in ("name", "description"):
                match = re.search(r"^" + key + r":[ \t]*([^\r\n]+)", header.group(1), re.M)
                fields[key] = match.group(1).strip().strip("\"'") if match else ""
        if fields.get("name") != skill or not fields.get("description", "").strip():
            problems.append("invalid name/description in " + target)
    if problems:
        return _result(name, FAIL, "; ".join(problems), fix)
    return _result(name, PASS,
                   "%d pack skills, companion files, constitution and scripts present; filesystem only, Muse catalog not queried"
                   % len(inventory))


def check_claude_md_import(root):
    """CTX-B / F-01. Copilot CLI loads BOTH AGENTS.md and CLAUDE.md as custom instructions
    (measured: two ~58 KB <custom_instruction> blocks in one captured prefix), while Claude
    Code reads only CLAUDE.md and expands an `@AGENTS.md` import in place. The pack's
    byte-identical parity therefore pays the managed block twice on every Copilot request.
    The fix is structural, so the check is too: CLAUDE.md must be the import stub."""
    agents = _read(os.path.join(root, "AGENTS.md"))
    claude = _read(os.path.join(root, "CLAUDE.md"))
    if agents is None and claude is None:
        return _result("claude-md import", FAIL, "neither AGENTS.md nor CLAUDE.md present",
                       "paste the managed block into AGENTS.md and make CLAUDE.md `@AGENTS.md` (INSTALL 1.1)")
    if claude is None:
        return _result("claude-md import", WARN, "CLAUDE.md absent - Claude Code will not read AGENTS.md",
                       "create CLAUDE.md containing `@AGENTS.md` (Claude Code expands the import; INSTALL 1.1)")
    if agents is None:
        return _result("claude-md import", WARN, "AGENTS.md absent - Copilot has no instruction file",
                       "move the managed block to AGENTS.md and make CLAUDE.md `@AGENTS.md` (INSTALL 1.1)")
    if IMPORT_RX.search(claude):
        return _result("claude-md import", PASS, "CLAUDE.md imports AGENTS.md ({0:,} bytes); the block is loaded once per host".format(len(claude)))
    both_blocks = bool(MARKER_RX.search(agents)) and bool(MARKER_RX.search(claude))
    detail = "CLAUDE.md is a {0:,}-byte copy beside a {1:,}-byte AGENTS.md{2}".format(
        len(claude), len(agents), " - both carry the managed block" if both_blocks else "")
    return _result("claude-md import", WARN, detail + "; Copilot CLI loads both, so ~{0:,} est. tokens are paid twice per request (CTX-B)".format(int(len(claude) / 3.54)),
                   "replace CLAUDE.md's copy with a single `@AGENTS.md` line plus the short Claude Code addendum block (INSTALL 1.1)")


def check_copilot_settings():
    """F-09 / WT1a. `contextTier: long_context` and `effortLevel: high` as GLOBAL defaults let a
    session grow without a compaction and pay maximum reasoning on every T0 turn - the profiled
    23-hour session went 159k -> 564k tokens with zero compactions. Both are per-phase
    choices (GO19), so a global setting is reported, not assumed."""
    home = os.environ.get("COPILOT_HOME") or os.path.join(os.path.expanduser("~"), ".copilot")
    path = os.path.join(home, "settings.json")
    if not os.path.isfile(path):
        return _result("copilot settings", PASS, "no Copilot CLI settings.json on this machine (not installed, or defaults)")
    try:
        with open(path, encoding="utf-8") as fh:
            cfg = json.load(fh)
    except (OSError, ValueError) as exc:
        return _result("copilot settings", WARN, "cannot read {0}: {1}".format(path, exc), "fix the file; the check is fail-open")
    tier, effort = cfg.get("contextTier"), cfg.get("effortLevel")
    flags = []
    if tier == "long_context":
        flags.append("contextTier=long_context (never compacts; context accretes across tasks - CTX-A)")
    if effort == "high":
        flags.append("effortLevel=high as the global default (tail latency on every T0 step; per-phase per GO19)")
    if flags:
        return _result("copilot settings", WARN, "; ".join(flags),
                       "set these per session/phase (`/model`, `--effort`) rather than in ~/.copilot/settings.json; start a new session per task (WT1a)")
    return _result("copilot settings", PASS, "contextTier={0}, effortLevel={1}".format(tier or "default", effort or "default"))


def check_claude_settings(root):
    """F-12 / IO14. `showThinkingSummaries` is the richest thinking display Claude Code offers; a
    project that profiles its sessions wants it on. Project scope (.claude/settings.json) so it
    travels with the repo; the user file is reported but never edited."""
    text = _read(os.path.join(root, ".claude", "settings.json"))
    if text is None:
        return _result("claude settings", WARN, ".claude/settings.json absent",
                       "create it from adapters/hooks/claude-code.settings.hooks.json (hooks + showThinkingSummaries)")
    try:
        cfg = json.loads(text)
    except ValueError as exc:
        return _result("claude settings", WARN, "cannot parse .claude/settings.json: {0}".format(exc), "fix the JSON")
    if cfg.get("showThinkingSummaries") is True:
        return _result("claude settings", PASS, "showThinkingSummaries=true (the richest thinking display the host offers)")
    return _result("claude settings", WARN, "showThinkingSummaries not set - thinking is shown as a collapsed stub",
                   "set \"showThinkingSummaries\": true in .claude/settings.json (INSTALL 1.6); it changes the display, not the billed tokens")


def check_hooks(root):
    """F-07 / CTX-D. The re-read guard is a control only when a host runs it."""
    cop = os.path.join(root, ".github", "hooks", "ai-forward.json")
    grok = os.path.join(root, ".grok", "hooks", "ai-forward.json")
    agy = os.path.join(root, ".agents", "hooks.json")
    cc = _read(os.path.join(root, ".claude", "settings.json")) or ""
    have = []
    if os.path.isfile(cop):
        have.append("Copilot (.github/hooks/ai-forward.json)")
    if os.path.isfile(grok):
        have.append("Grok Build (.grok/hooks/ai-forward.json)")
    if os.path.isfile(agy):
        have.append("Antigravity (.agents/hooks.json)")
    # B15: the guard runs inside the per-event dispatcher (dispatch.py); older installs name it directly.
    if "reread-guard" in cc or "dispatch.py --host claude" in cc:
        have.append("Claude Code (.claude/settings.json)")
    hooks_dir = os.path.join(root, "docs", "ai-forward-pack", "hooks")
    missing = [f for f in ("reread-guard.py", "dispatch.py") if not os.path.isfile(os.path.join(hooks_dir, f))]
    if have and not missing:
        return _result("re-read guard hook", PASS, "installed for " + ", ".join(have))
    if have:
        return _result("re-read guard hook", FAIL, "hook config present but docs/ai-forward-pack/hooks/{0} is missing"
                       .format(" and ".join(missing)),
                       "copy adapters/hooks/reread-guard.py and dispatch.py to docs/ai-forward-pack/hooks/ (INSTALL 1.5)")
    return _result("re-read guard hook", WARN, "not installed on any host",
                   "copy adapters/hooks/copilot.ai-forward-hooks.json to .github/hooks/ai-forward.json, grok.ai-forward-hooks.json to .grok/hooks/ai-forward.json, agy.ai-forward-hooks.json to .agents/hooks.json, and merge adapters/hooks/claude-code.settings.hooks.json into .claude/settings.json (INSTALL 1.4 / 1.7 / 1.8)")


def check_hook_launchers(root):
    """Scan five config files recursively for legacy command tokens; no custom allowlist."""
    import shutil
    import subprocess
    files = [".claude/settings.json", ".github/hooks/ai-forward.json",
             ".agents/hooks.json", ".codex/hooks.json"]
    grok_hooks = os.path.join(root, ".grok", "hooks")
    if os.path.isdir(grok_hooks):
        files.extend(".grok/hooks/" + name for name in sorted(os.listdir(grok_hooks)) if name.endswith(".json"))
    configured = False

    def walk(value):
        if isinstance(value, dict):
            for key, item in value.items():
                if key in ("command", "bash", "powershell") and isinstance(item, str):
                    yield item
                else:
                    yield from walk(item)
        elif isinstance(value, list):
            for item in value:
                yield from walk(item)

    for relative in files:
        try:
            with open(os.path.join(root, relative), encoding="utf-8") as handle:
                text = handle.read()
            config = json.loads(text)
        except FileNotFoundError:
            continue
        except (OSError, ValueError):
            return _result("hook launchers", FAIL, "HOOK-CONFIG-INVALID: " + relative,
                           "repair JSON/UTF-8 encoding without discarding local settings")
        for command in walk(config):
            if "py=$(python3" in command:
                return _result("hook launchers", FAIL, "HOOK-SHELL: Bash-only command in " + relative,
                               "refresh the pack; inspect custom wrappers separately")
            configured = configured or "run-hook.sh" in command
    if not configured:
        return _result("hook launchers", WARN, "portable pack launcher not configured", "refresh the pack")
    git = shutil.which("git")
    if not git or not os.path.isfile(os.path.join(root, "docs", "ai-forward-pack", "hooks", "run-hook.sh")):
        return _result("hook launchers", FAIL, "HOOK-RUNTIME-MISSING: Git or run-hook.sh absent",
                       "install Git and Python 3; refresh the complete hook bundle")
    try:
        result = subprocess.run(
            [git, "-c", "alias.aif-hook=!sh", "aif-hook",
             "docs/ai-forward-pack/hooks/run-hook.sh", "reread-guard.py", "--help"],
            cwd=root, capture_output=True, text=True, encoding="utf-8", timeout=10,
        )
    except (OSError, subprocess.TimeoutExpired):
        return _result("hook launchers", FAIL, "HOOK-RUNTIME-FAILED: launcher probe could not finish",
                       "check Git's shell and Python installations")
    if result.returncode != 0 or "usage:" not in result.stdout.lower():
        return _result("hook launchers", FAIL, "HOOK-RUNTIME-FAILED: hook help did not execute",
                       "check Git's shell, Python, and the installed hook bundle")
    return _result("hook launchers", PASS,
                   "Git launcher executed the Python hook; native event enforcement is not qualified here")


def check_block(root, fname):
    p = os.path.join(root, fname)
    if not os.path.exists(p):
        return _result(f"{fname} managed block", WARN, f"{fname} not present",
                       "run /updatepack to add the managed block")
    with open(p, encoding="utf-8", errors="replace") as instruction_file:
        text = instruction_file.read()
    begins = text.count("AI-FORWARD-PACK:BEGIN")
    ends = text.count("AI-FORWARD-PACK:END")
    if fname == "CLAUDE.md" and IMPORT_RX.search(text) and begins == 0:
        # The import form (INSTALL 1.1): the block lives once, in AGENTS.md; CLAUDE.md pulls it in.
        return _result(f"{fname} managed block", PASS, "import form (`@AGENTS.md`); the block is checked on AGENTS.md")
    if begins == 1 and ends == 1:
        return _result(f"{fname} managed block", PASS, "intact (1 block)")
    if begins == 0:
        return _result(f"{fname} managed block", FAIL, "no AI-FORWARD-PACK block",
                       "run /updatepack to paste the managed block")
    return _result(f"{fname} managed block", FAIL, f"{begins} begin / {ends} end markers (expected 1/1)",
                   "remove duplicate blocks; keep one BEGIN/END pair")


def check_graph(root):
    bundle = os.path.join(root, "docs", "ai-forward-pack", "scripts", "docs-graph.py")
    if not os.path.exists(bundle):
        return _result("knowledge graph", WARN, "docs-graph.py not found",
                       "run /updatepack to restore the script bundle")
    env = dict(os.environ, PYTHONIOENCODING="utf-8")
    try:
        result = run_bounded(
            [sys.executable, bundle, "inventory"],
            cwd=root,
            env=env,
            timeout_seconds=30,
        )
    except Exception as e:
        return _result("knowledge graph", WARN, f"graph tool unavailable ({e})",
                       "check Python availability")
    if not getattr(result, "contained", True):
        detail = getattr(result, "containment_error", None) or "subprocess containment unavailable"
        return _result("knowledge graph", WARN, f"graph validation not run safely ({detail})",
                       "run docs-graph.py validate manually in a contained shell")
    if result.timed_out or result.limit_exceeded:
        detail = "timed out" if result.timed_out else f"exceeded {result.limit_exceeded} limit"
        return _result("knowledge graph", WARN, f"graph validation {detail}",
                       "inspect docs graph resource use")
    try:
        if result.returncode != 0:
            diagnostic = result.stderr.strip() or f"exit {result.returncode}"
            return _result(
                "knowledge graph",
                WARN,
                f"graph validation inconclusive ({diagnostic[:240]})",
                "run docs-graph.py validate manually",
            )
        info = json.loads(result.stdout)
        problems = info.get("problems", [])
        if problems:
            return _result("knowledge graph", FAIL, f"{len(problems)} graph problem(s)",
                           "run docs-graph.py validate for detail; fix frontmatter")
        n = info.get("artifacts", 0)
        if n == 0:
            return _result("knowledge graph", WARN, "no graph yet (0 artifacts)",
                           "the first skill run creates docs/docs-index.js")
        health_count = sum(len(info.get(key, [])) for key in ("stale", "flagged", "orphans"))
        if health_count:
            return _result("knowledge graph", WARN, "valid but has stale/flagged/orphan nodes",
                           "review review-by dates; run docs-graph.py freshness for detail")
        return _result("knowledge graph", PASS, "schema-valid, no dangling links, fresh")
    except Exception as exc:
        diagnostic = result.stderr.strip() or type(exc).__name__
        return _result("knowledge graph", WARN, f"graph validation inconclusive ({diagnostic[:240]})",
                       "run docs-graph.py validate manually")



def _git_lines(root, *args):
    """git output lines, or None when git is unavailable or root is not a checkout."""
    try:
        proc = subprocess.run(["git", "-C", root, *args], capture_output=True, text=True,
                              encoding="utf-8", timeout=30, check=False)
    except (OSError, subprocess.SubprocessError):
        return None
    if proc.returncode not in (0, 1):
        return None
    return proc.returncode, [line for line in proc.stdout.splitlines() if line.strip()]


def _jsonl_rows(path):
    rows = []
    try:
        with open(path, encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if not line:
                    continue
                try:
                    row = json.loads(line)
                except ValueError:
                    continue
                if isinstance(row, dict):
                    rows.append(row)
    except OSError:
        return rows
    return rows


def check_mail(root):
    """The message layer's invariants (design-message-layer section 9; D10).

    mail dir         FAIL when anything under .agents/mail/ is tracked (bodies would reach git)
    ledger tracking  FAIL when .agents/log/ is ignored (D10 tracks the ledgers by default)
    mail twins       FAIL naming every state-changing mail id with no {"type":"mail"} ledger twin
    doorbells        one line per harness from .agents/harness-status.json; absent -> "not recorded"
    """
    results = []
    agents = os.path.join(root, ".agents")
    tracked = _git_lines(root, "ls-files", "--", ".agents/mail")
    if tracked is None:
        results.append(_result("mail dir", PASS, "not a git checkout here (nothing can be tracked)"))
    elif tracked[1]:
        results.append(_result("mail dir", FAIL, "tracked: " + ", ".join(tracked[1][:3]),
                               "git rm --cached -r .agents/mail && add `.agents/mail/` to .gitignore "
                               "(mail bodies are machine-local; pack-apply writes the line)"))
    else:
        results.append(_result("mail dir", PASS, ".agents/mail/ is not tracked"))
    ignored = _git_lines(root, "check-ignore", "-q", "--", ".agents/log/probe.jsonl")
    if ignored is None:
        results.append(_result("ledger tracking", PASS, "not a git checkout here (nothing is ignored)"))
    elif ignored[0] == 0:
        results.append(_result("ledger tracking", FAIL,
                               ".agents/log/ is ignored - D10 tracks the coord ledgers by default so git "
                               "carries the mail twins across machines",
                               "add `!.agents/log/` after the `.agents/*` line in .gitignore (D10; pack-apply writes it)"))
    else:
        results.append(_result("ledger tracking", PASS, ".agents/log/ is tracked (D10)"))
    mail_dir = os.path.join(agents, "mail")
    inbox_only = {"note", "ack", "nack"}
    if not os.path.isdir(mail_dir):
        results.append(_result("mail twins", PASS, "no inboxes present (nothing to check)"))
    else:
        twins = set()
        log_dir = os.path.join(agents, "log")
        if os.path.isdir(log_dir):
            for name in sorted(os.listdir(log_dir)):
                if name.endswith(".jsonl"):
                    for row in _jsonl_rows(os.path.join(log_dir, name)):
                        if row.get("type") == "mail" and row.get("mail_id"):
                            twins.add(row["mail_id"])
        missing = []
        for name in sorted(os.listdir(mail_dir)):
            if not name.endswith(".jsonl"):
                continue
            for row in _jsonl_rows(os.path.join(mail_dir, name)):
                kind = row.get("kind")
                if kind and kind not in inbox_only and row.get("id") and row["id"] not in twins:
                    missing.append("{0} ({1})".format(row["id"], kind))
        if missing:
            results.append(_result("mail twins", FAIL,
                                   "{0} state-changing mail(s) with no ledger twin: {1}".format(
                                       len(missing), ", ".join(missing[:3])),
                                   "re-issue the twin through coord-mail.py (append_mail is the only writer) "
                                   "or check that .agents/log/ was not deleted"))
        else:
            results.append(_result("mail twins", PASS, "every delegate/ruling/... has its ledger twin"))
    status_path = os.path.join(agents, "harness-status.json")
    if not os.path.isfile(status_path):
        results.append(_result("doorbells", PASS, "not recorded (run coord-mail.py dispatch to probe a harness)"))
    else:
        try:
            with open(status_path, encoding="utf-8") as handle:
                data = json.load(handle)
        except (OSError, ValueError) as exc:
            data = None
            results.append(_result("doorbells", FAIL, "harness-status.json unreadable ({0})".format(exc),
                                   "delete .agents/harness-status.json and re-run coord-mail.py dispatch"))
        if isinstance(data, dict):
            parts = []
            for harness in sorted(data):
                row = data[harness] if isinstance(data[harness], dict) else {}
                parts.append("{0}: {1} ({2}, {3})".format(harness, row.get("status", "unsupported"),
                                                          row.get("version") or "version not recorded",
                                                          row.get("date") or "undated"))
            results.append(_result("doorbells", PASS, "; ".join(parts) or "no harness recorded"))
    return results


def check_requests(root):
    """Typed seam requests (spec-typed-seam-requests): FAIL on a request past its deadline with
    no recorded outcome; WARN on stale acks and untyped rows; `not recorded` with no store.
    One reader: coord-core.py's request_doctor_lines, loaded beside this file."""
    core_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "coord-core.py")
    if not os.path.isfile(core_path):
        return _result("requests", PASS, "not recorded (coord-core.py is not installed beside pack-doctor.py)")
    import importlib.util
    import time
    spec = importlib.util.spec_from_file_location("coord_core_for_doctor", core_path)
    core = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(core)
    except Exception as exc:  # a broken coord-core is a finding here, not a crash
        return _result("requests", FAIL, "coord-core.py could not be loaded ({0})".format(exc),
                       "re-run pack-apply.py to restore pack/scripts/coord-core.py")
    lines, problems = core.request_doctor_lines(os.path.join(root, ".agents"), root, time.time())
    flagged = [re.sub(r"^requests\s+", "", line.strip()) for line in lines
               if "WARN" in line or "FAIL" in line or "NOT CHECKED" in line]
    detail = "; ".join(flagged) if flagged else lines[0].split(None, 1)[1]
    if problems:
        return _result("requests", FAIL, detail,
                       "run `coord request expire` - every open request past its deadline records its fallback")
    if flagged:
        return _result("requests", WARN, detail,
                       "re-ack against the current blob; re-add untyped requests with --deadline and --fallback")
    return _result("requests", PASS, detail)


def check_heartbeat(root):
    """Progress liveness (spec-liveness-and-track, P3): who beats, how fresh, how many stalled -
    or `not recorded` when no heartbeat row exists (CTX-H: an uninstalled control looks like a
    quiet fleet). One reader: coord-core.py's heartbeat_doctor_line, loaded beside this file."""
    core_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "coord-core.py")
    if not os.path.isfile(core_path):
        return _result("heartbeat", PASS, "not recorded (coord-core.py is not installed beside pack-doctor.py)")
    import importlib.util
    import time
    spec = importlib.util.spec_from_file_location("coord_core_for_heartbeat_doctor", core_path)
    core = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(core)
    except Exception as exc:  # a broken coord-core is a finding here, not a crash
        return _result("heartbeat", FAIL, "coord-core.py could not be loaded ({0})".format(exc),
                       "re-run pack-apply.py to restore pack/scripts/coord-core.py")
    line, problem = core.heartbeat_doctor_line(os.path.join(root, ".agents"), time.time())
    detail = re.sub(r"^heartbeat\s+", "", line.strip())
    if problem:
        return _result("heartbeat", FAIL, detail, "repair the unreadable ledger row named above")
    return _result("heartbeat", PASS, detail)


def _command_head(command):
    """The first word of a registry command: a quoted path as one token, else up to the
    first space. `"C:\\Program Files\\Python\\python.exe" x.py` -> the path; `python3 x.py`
    -> `python3`."""
    text = (command or "").strip()
    if text.startswith('"'):
        end = text.find('"', 1)
        return text[1:end] if end > 0 else text[1:]
    return text.split(None, 1)[0] if text else ""


def _coordination_activation(root):
    """Read existing opt-ins, not the mere presence of shipped scripts/lifecycle hooks."""
    signals = []
    attributes = _read(os.path.join(root, ".gitattributes")) or ""
    for line in attributes.splitlines():
        tokens = line.split()
        if tokens and not tokens[0].startswith("#") and any(
                token.startswith("merge=coord-") for token in tokens[1:]):
            signals.append(".gitattributes coordination merge driver")
            break

    checkout = _git_lines(root, "rev-parse", "--show-toplevel")
    if checkout is not None and checkout[0] == 0:
        drivers = _git_lines(root, "config", "--get-regexp", r"^merge\.coord-.*\.driver$")
        if drivers is not None and drivers[1]:
            signals.append("Git coordination merge driver configuration")
        hooks = _git_lines(root, "rev-parse", "--git-path", "hooks")
        if hooks is not None and hooks[0] == 0 and hooks[1]:
            directory = hooks[1][0]
            if not os.path.isabs(directory):
                directory = os.path.join(root, directory)
            for name in ("pre-commit", "pre-merge-commit"):
                path = os.path.join(directory, name)
                text = _read(path) or ""
                if (os.path.isfile(path) and (os.name == "nt" or os.access(path, os.X_OK))
                        and ("# coord-core pre-commit floor" in text
                             or re.search(r"coord-core\.py[\"']?\s+(?:precommit|staged-markers)\b", text))):
                    signals.append("Git " + name + " coordination hook")

    def ownership_command(value):
        if isinstance(value, dict):
            if value.get("enabled") is False or value.get("disableAllHooks") is True:
                return False
            for key, item in value.items():
                if key in ("command", "bash", "powershell") and isinstance(item, str):
                    if re.search(r"coord-core\.py[\"']?\s+hook\b", item):
                        return True
                elif ownership_command(item):
                    return True
        elif isinstance(value, list):
            return any(ownership_command(item) for item in value)
        return False

    configs = [".claude/settings.json", ".claude/settings.local.json",
               ".agents/hooks.json", ".codex/hooks.json"]
    for directory in (".github/hooks", ".grok/hooks"):
        path = os.path.join(root, directory)
        if os.path.isdir(path):
            configs.extend(directory + "/" + name for name in sorted(os.listdir(path))
                           if name.endswith(".json"))
    for relative in configs:
        text = _read(os.path.join(root, relative))
        if text is None:
            continue
        try:
            config = json.loads(text)
        except ValueError:
            # A broken installed config cannot establish an inactive mode. The launcher
            # check diagnoses its syntax separately; never turn it into a serial exemption.
            signals.append("invalid hook configuration " + relative)
            continue
        if ownership_command(config):
            signals.append("native ownership hook in " + relative)
    return signals


def check_coordination(root):
    """Optional for serial use; validate any registry or explicitly installed coordination.

    Shipping coord-core.py and lifecycle hooks does not activate ownership/merge controls.
    Existing native ownership commands, effective Git hooks/config, and merge attributes
    are opt-ins. A missing enabled contract is a failure; an absent optional one is not.
    Fresh clones with declared but unregistered drivers retain the per-clone warning.
    """
    name = "coordination"
    script = os.path.join(root, "docs", "ai-forward-pack", "scripts", "coord-core.py")
    registry = os.path.join(root, ".agents", "artifacts.yml")
    if not os.path.exists(registry):
        activation = _coordination_activation(root)
        if activation:
            return _result(name, FAIL,
                           "coordination is explicitly activated ({0}) but .agents/artifacts.yml "
                           "is absent; the enabled classification contract is missing".format("; ".join(activation)),
                           "restore the registry or intentionally remove the coordination opt-ins; "
                           "initialize classifications with coord-core.py classify init only if coordination is intended")
        if not os.path.exists(script):
            return _result(name, PASS, "coord-core.py not installed - check does not apply")
        return _result(name, WARN,
                       "NOT APPLICABLE to serial use: optional coordination is not activated; "
                       ".agents/artifacts.yml is absent (not an install failure)",
                       "leave it absent for serial work; classify artifacts only when opting into coordination")

    entries, bad, derived, seen = 0, "", [], {}
    try:
        with open(registry, encoding="utf-8") as handle:
            rows = handle.read().splitlines()
        for lineno, raw in enumerate(rows, start=1):
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            if ":" not in line:
                bad = "line {0}: expected `pattern: class [command]`".format(lineno)
                break
            pattern, rest = line.split(":", 1)
            pattern = pattern.strip().replace("\\", "/")
            if not pattern:
                bad = "line {0}: empty pattern".format(lineno)
                break
            parts = rest.strip().split(None, 1)
            klass = parts[0] if parts else ""
            if klass not in ("authored", "derived", "register"):
                bad = "line {0}: unknown class {1!r}".format(lineno, klass)
                break
            if pattern in seen and seen[pattern] != klass:
                bad = "line {0}: {1!r} has conflicting classes".format(lineno, pattern)
                break
            seen[pattern] = klass
            if klass == "derived":
                if len(parts) < 2 or not parts[1].strip():
                    bad = "line {0}: a `derived` pattern needs a regenerate command".format(lineno)
                    break
                derived.append((lineno, parts[1].strip()))
            entries += 1
    except (OSError, UnicodeError) as e:
        return _result(name, FAIL, f"registry unreadable ({e})",
                       "check .agents/artifacts.yml permissions")
    if bad:
        # `classify` degrades to `authored` on a broken registry - safe, and invisible.
        return _result(name, FAIL, f"registry does not parse - {bad}",
                       "fix .agents/artifacts.yml, or regenerate with `coord classify init --force`")

    # PLAT-B: a registry whose derived command cannot run HERE must FAIL, not parse-and-pass.
    # A Windows `python.exe` path written by `classify init` on one machine passed this check
    # on macOS for a week while `coord regen` failed with "command not found". The token
    # `python3`/`python` is resolved at run time by coord-core, so it always resolves; any
    # other first word must exist as a file or on PATH.
    unresolved = []
    for lineno, command in derived:
        head = _command_head(command)
        if head in ("python3", "python", "py"):
            continue
        if os.path.isabs(head) or os.sep in head or "/" in head:
            ok = os.path.exists(head)
        else:
            ok = shutil.which(head) is not None
        if not ok:
            unresolved.append("line {0}: {1!r}".format(lineno, head))
    if unresolved:
        return _result(name, FAIL,
                       "{0} derived command(s) name an interpreter or tool that does not "
                       "exist on this machine - {1}".format(len(unresolved), "; ".join(unresolved)),
                       "the registry travels with the repo and must carry the portable token "
                       "`python3`, never one machine's path: run `coord classify init --force` "
                       "(it now writes the token) and commit .agents/artifacts.yml")

    declared = set()
    eol_rule = False
    ga = os.path.join(root, ".gitattributes")
    if os.path.exists(ga):
        try:
            with open(ga, encoding="utf-8") as handle:
                attribute_rows = handle.read().splitlines()
            for line in attribute_rows:
                tokens = line.split()
                if not tokens or tokens[0].startswith("#"):
                    continue
                declared.update(token[len("merge="):] for token in tokens[1:]
                                if token.startswith("merge=coord-"))
                if "eol=lf" in tokens[1:]:
                    eol_rule = True
        except OSError:
            pass
    if declared and not eol_rule:
        # PLAT-A (P3): every LF-writer in the pack and the byte-identity of the derived-file
        # merge rest on the working tree being LF on every OS. That is only true when
        # .gitattributes says so; a Windows clone with core.autocrlf=true and no `eol=lf`
        # rule checks the ledgers out CRLF, and a union merge then keeps a CRLF line and
        # its LF twin as two entries. The pack ships the rule via pack-apply; a repo that
        # declares the coord merge drivers without it has half the mechanism.
        return _result(name, FAIL,
                       ".gitattributes declares {0} but no `eol=lf` rule - the ledgers and derived "
                       "files these drivers merge by byte identity would check out CRLF on a Windows "
                       "clone with core.autocrlf=true".format(", ".join(sorted(declared))),
                       "add `* text=auto eol=lf` to .gitattributes (pack-apply appends it), then "
                       "`git add --renormalize .` once")
    if declared:
        registered = set()
        try:
            result = run_bounded(["git", "config", "--get-regexp", r"^merge\..*\.driver"],
                                 cwd=root, timeout_seconds=10)
            for line in (getattr(result, "stdout", "") or "").splitlines():
                parts = line.split(".", 2)
                if len(parts) >= 3:
                    registered.add(parts[1])
        except Exception:
            registered = set()
        missing = sorted(declared - registered)
        if missing:
            return _result(name, WARN,
                           "{0} pattern(s) registered; .gitattributes declares {1} which "
                           "this clone does not register".format(entries, ", ".join(missing)),
                           "python docs/ai-forward-pack/scripts/coord-core.py install  "
                           "(run it once per CLONE, in the primary checkout - worktrees "
                           "share that .git/config and inherit the registration)")

    return _result(name, PASS, "{0} pattern(s) classified{1}".format(
        entries, "; drivers registered" if declared else "; no driver declared yet"))


def check_node_runner():
    """Report whether `npm run …` can actually resolve node on THIS machine.

    FR-055 / registered class PACK-C — *a documented command assumed portable*. The pack and
    its contributor docs say `npm run test:docs-explorer:core`. On a real Windows host that
    command printed `'node' is not recognized` while `node --version` succeeded and the
    identical test invocation run directly passed 31/31. A contributor sees a failure that
    is not a failure and may "fix" a healthy suite; CI is Linux-only so it never surfaces
    there — precisely the blind spot PACK-C describes.

    The probe is the diagnosis: node resolving *for you* proves nothing, because npm runs
    scripts through a **child shell** (cmd.exe on Windows, sh elsewhere) whose PATH can
    differ from your own. So this spawns that same shell and asks it for node — which is the
    only thing that predicts whether the documented command will work.

    Mirrors check_interpreter()'s stance for `python3`: name the working invocation for this
    machine once, here, instead of discovering it one command at a time (CI6).
    """
    def _works(argv):
        try:
            proc = run_bounded(argv, timeout_seconds=20)
        except (OSError, ValueError):
            return None
        out = ((proc.stdout or "") + (proc.stderr or "")).strip()
        return out.splitlines()[0] if proc.returncode == 0 and out.startswith("v") else None

    direct = _works(["node", "--version"])
    if direct is None:
        return _result("node runner", WARN,
                       "node is not on PATH; the Docs Explorer core tests cannot run here",
                       "install Node 22+ (https://nodejs.org) — the Python gates are unaffected")

    # The shell npm spawns for `npm run`, which is the actual failure point.
    if os.name == "nt":
        shell_argv = ["cmd", "/c", "node --version"]
        shell_name = "cmd.exe"
    else:
        shell_argv = ["sh", "-c", "node --version"]
        shell_name = "sh"
    via_shell = _works(shell_argv)
    if via_shell is not None:
        return _result("node runner", PASS,
                       "`npm run` resolves node here (%s); documented commands run as written"
                       % direct)

    return _result(
        "node runner", WARN,
        "node works directly (%s) but %s — the shell `npm run` spawns — cannot find it, so "
        "`npm run …` fails with \"'node' is not recognized\" even though your tests are fine."
        % (direct, shell_name),
        "run the script body directly instead, e.g. "
        "`node --test tests/docs_explorer/*.test.js`. To repair `npm run` itself, ensure the "
        "Node install directory is on the PATH the child shell inherits (on Windows, reopen "
        "the terminal after installing, or add it to the *machine* PATH and start a new "
        "session).")


def check_interpreter():
    """Report the invocation form that actually runs Python 3 on THIS machine.

    The pack documents `python3 …` because that is the POSIX-correct name and matches every
    script's shebang. It is not universally available: python.org's Windows installer ships
    `python.exe` and `py.exe` but **no `python3.exe`**, and Windows additionally provides a
    `python3` App-Execution-Alias that is not Python at all - it prints "Python was not
    found" and exits 9009. So a Windows reader copy-pasting a documented command sees what
    looks like a missing Python installation when Python is installed and working.

    This check exists so that failure is reported once, here, with the right answer, instead
    of being discovered one command at a time (continuous-improvement.md CI6 - convert the
    lesson into a control that fires at the moment of the mistake).
    """
    # uv prepends its selected interpreter directory to the child's PATH. A successful
    # `python3 --version` there cannot prove the caller could run the native docs command.
    # The actual uv marker is observed at runtime; no pack-specific mode flag is needed.
    uv_depth = os.environ.get("UV_RUN_RECURSION_DEPTH", "0")
    if uv_depth.isdigit() and int(uv_depth) > 0:
        executable = sys.executable.replace("\\", "/")
        return _result(
            "python interpreter", WARN,
            "running via uv-managed invocation (Python {0} at {1}); native Python invocation "
            "from the caller PATH is not verified — uv may add Python names only to its child PATH".format(
                sys.version.split()[0], sys.executable),
            'use `uv run --no-config --no-project --python "{0}" '
            'docs/ai-forward-pack/scripts/pack-doctor.py` with this existing interpreter; '
            'test native Python commands separately in the caller shell'.format(executable))

    candidates = [("python3", ["python3", "--version"]),
                  ("python", ["python", "--version"]),
                  ("py -3", ["py", "-3", "--version"])]
    working = []
    for label, argv in candidates:
        try:
            proc = run_bounded(argv, timeout_seconds=15)
        except (OSError, ValueError):
            # Only "this command could not be launched" is an expected miss. A TypeError
            # here would be a bug in this call, and swallowing it would turn a programming
            # error into a plausible-looking FAIL - which is exactly how this check shipped
            # broken the first time.
            continue
        out = ((proc.stdout or "") + (proc.stderr or "")).strip()
        if proc.returncode == 0 and out.startswith("Python 3"):
            working.append((label, out.splitlines()[0]))

    if not working:
        return _result("python interpreter", FAIL,
                       "no working Python 3 found as python3, python, or py -3",
                       "install Python 3 from python.org and re-run")

    forms = [w[0] for w in working]
    version = working[0][1]
    if "python3" in forms:
        return _result("python interpreter", PASS,
                       "`python3` works here (%s); documented commands run as written" % version)

    # Python 3 exists but not under the documented name - the Windows case.
    return _result(
        "python interpreter", WARN,
        "`python3` does not work here; use `%s` instead (%s). The pack documents `python3` "
        "because it is the POSIX name and matches the script shebangs." % (forms[0], version),
        "substitute `%s` for `python3` in documented commands. On Windows, python.org ships "
        "no python3.exe, and the `python3` you may see is a Microsoft Store alias that is not "
        "Python. Optionally disable it: Settings > Apps > Advanced app settings > App "
        "execution aliases." % forms[0])


def run(root):
    checks = [
        check_installed(root)[0],
        check_interpreter(),
        check_node_runner(),
        check_surface(root, ".claude", [".claude/knowledge", ".claude/skills", ".claude/agents"]),
        check_surface(root, ".github", [".github/instructions", ".github/prompts", ".github/agents"]),
        check_surface(root, ".grok", [".grok/skills", ".grok/agents", ".grok/hooks", ".grok/rules"]),
        check_surface(root, ".agents", [".agents/skills", ".agents/rules", ".agents/hooks.json"]),
        check_codex(root),
        check_muse(root),
        check_block(root, "CLAUDE.md"),
        check_block(root, "AGENTS.md"),
        check_claude_md_import(root),
        check_hooks(root),
        check_hook_launchers(root),
        check_claude_settings(root),
        check_copilot_settings(),
        check_graph(root),
        check_coordination(root),
    ]
    checks.extend(check_mail(root))
    checks.append(check_requests(root))
    checks.append(check_heartbeat(root))
    return checks


def main():
    ap = argparse.ArgumentParser(description="AI-Forward install-health doctor")
    ap.add_argument("--root", default=os.getcwd())
    ap.add_argument("--json", action="store_true")
    ap.add_argument(
        "--strict",
        action="store_true",
        help="treat warnings and inconclusive checks as release-blocking failures",
    )
    args = ap.parse_args()
    root = os.path.abspath(args.root)
    checks = run(root)
    summary = {s: sum(1 for c in checks if c["status"] == s) for s in (PASS, WARN, FAIL)}
    if args.json:
        print(json.dumps({"checks": checks, "summary": summary}, indent=2))
    else:
        print("AI-Forward doctor — install health\n")
        for c in checks:
            print(f"  {c['status']:5} {c['name']:24} {c['detail']}")
            if c["fix"] and c["status"] != PASS:
                print(f"        -> fix: {c['fix']}")
        print(f"\n  {summary[FAIL]} FAIL · {summary[WARN]} WARN · {summary[PASS]} PASS")
    return 1 if summary[FAIL] or (args.strict and summary[WARN]) else 0


if __name__ == "__main__":
    sys.exit(main())
