#!/usr/bin/env python3
"""pack-apply.py — apply the AI-Forward deployment map to a repo, mechanically and reversibly.

`/updatepack` and `/addpacktorepo` used to hand-apply INSTALL.md's deployment map, so every
step a person could forget - re-pasting a managed block, deleting the wrapped copy of a doc
whose load scope moved, converting CLAUDE.md to the `@AGENTS.md` import, retiring a parity
control that encoded the old invariant - was remembered or it was not. This script IS the
deployment map (INSTALL.md 1), run from the pack source against a target repo:

  pack-apply.py plan  --source <ai-forward clone> --target <repo>     # every action, no writes
  pack-apply.py apply --source <ai-forward clone> --target <repo>     # do it, idempotently

What it does, per artifact family (pack-owned names only - repo-local files are never touched):

  knowledge   -> .claude/knowledge/<name>.md verbatim; .github/instructions/<name>.instructions.md
                 (applyTo-wrapped) for load: always|glob; .github/knowledge/<name>.md for
                 load: skill|reference; the STALE copy in the other Copilot location is removed
                 (CTX-E: a doc re-scoped to on-demand must stop attaching).
  skills      -> .claude/skills/<name>/ (the whole directory: SKILL.md + reference/*.md);
                 .github/prompts/<name>.prompt.md;
                 .grok/skills/<name>/ (same files; Grok Build native, wins over Claude-compat scan)
  agents      -> .claude/agents/ (both sets); .github/agents/<name>.agent.md (renamed, `tools:` stripped);
                 .grok/agents/<name>.md (`_agent` suffix stripped, `tools:` stripped)
  bundle      -> docs/ai-forward-pack/{templates,scripts,hooks,README,OVERVIEW,research-synthesis,
                 INSTALL,context-budget.json}; .github/hooks/ai-forward.json; .claude/settings.json
                 (hooks merged, showThinkingSummaries set); .grok/hooks/ai-forward.json;
                 .grok/rules/grok-surface.md (path map only — not knowledge docs); .gitignore lines;
                 docs/index.html and docs/docs-index.js NEVER created or overwritten by
                 install — a content-creating skill instantiates the Explorer shell, as its
                 last action, once it has real content to show (V10; AL0.2, PK-03)
  front doors -> AGENTS.md: the managed block replaced wholesale between markers (appended if absent).
                 CLAUDE.md: converted to `@AGENTS.md` + the addendum block (CTX-B); the old file is
                 backed up under docs/ai-forward-pack/retired/, and every paragraph that is NOT in
                 AGENTS.md (after toolchain-path normalisation) is kept above the addendum.
  controls    -> a repo-local parity test that asserts CLAUDE.md carries the standing-method block
                 (the OLD invariant) is rewritten into a shim asserting the NEW invariant through
                 pack-doctor, its other assertions carried over where they can be read; the original
                 is backed up beside the CLAUDE.md backup.

Repo-local deviations are honoured, not reverted: a destination that differs from the version the
repo received at its installed revision is three-way merged (`git merge-file`) against the pack's
old and new text; a clean merge lands as MERGE, a conflicting one is left untouched with the new
pack text written under docs/ai-forward-pack/conflicts/ and reported as CONFLICT for the skill to
reconcile. The installed revision advances only in `apply`. Re-running is a no-op.

Python 3.8+, stdlib only. Exit 0 = applied/clean, 1 = conflicts or errors reported, 2 = usage.
"""
import argparse
import datetime as _dt
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

MANIFEST = "FOUNDATION.md"


class UnsafeDestination(ValueError):
    """FR-092: a destination that is a link, or that resolves outside the target repository."""


def read_hook_settings(path):
    """Decode policy-bearing configuration strictly; absence alone is an empty install."""
    if os.path.islink(path):
        raise ValueError("symlink hook target")
    try:
        with open(path, encoding="utf-8", newline="") as handle:
            return handle.read()
    except FileNotFoundError:
        return None


# B15 (hook cost): revision 100 shipped one launcher command per pack hook; one dispatcher
# command per host event replaces them. These are the source-managed names and commands it
# retires. A refresh that left them would run every hook twice.
RETIRED_HOOK_SCRIPTS = ("reread-guard", "mail-doorbell", "heartbeat", "git-identity-guard",
                        "owner-review-gate", "session-start")
RETIRED_AGY_SECTIONS = ("reread-guard", "session-start", "mail-doorbell", "heartbeat", "owner-review-gate")


def merge_named_hook_bundles(source_text, current_text=None):
    """Refresh source-owned names and retain project-owned names; reject invalid JSON."""
    source = json.loads(source_text)
    current = json.loads(current_text) if current_text is not None else {}
    for value in (source, current):
        if not isinstance(value, dict) or any(not isinstance(section, dict) for section in value.values()):
            raise ValueError("Named hook bundles must be objects of objects")
    _refresh_legacy_startup(current)
    _refresh_legacy_ownership(current)
    for name in RETIRED_AGY_SECTIONS:
        if name not in source:
            current.pop(name, None)
    current.update(source)
    return json.dumps(current, indent=2) + "\n"


STARTUP_LAUNCHER = ("git -c alias.aif-hook=!sh aif-hook -c '"
    'r=.;n=0;while [ ! -f "$r/docs/ai-forward-pack/hooks/run-hook.sh" ];do '
    '[ -e "$r/.git" ] && exit 2;n=$((n+1));[ "$n" -le 64 ] || exit 2;r=$r/..;done;'
    'cd "$r" || exit 2;exec sh docs/ai-forward-pack/hooks/run-hook.sh "$@"'
    "' aif-hook ")


STARTUP_EVENTS = ("SessionStart", "SubagentStart", "sessionStart", "subagentStart")


def _refresh_legacy_startup(value, event=None):
    """Only exact shipped startup commands; wrappers and argv extensions are local.

    Both earlier shipped forms (the plain launcher and the revision 106/107 bounded search
    running session-start.py) become the bounded search running the event dispatcher (B15),
    in place, so a handler keeps its position and timeout and startup never runs twice."""
    if isinstance(value, dict):
        if event and not value.get("args"):
            old = "git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh "
            for field in ("command", "bash", "powershell"):
                for host in ("claude", "grok", "copilot"):
                    suffix = "session-start.py --host " + host
                    if value.get(field) in (old + suffix, STARTUP_LAUNCHER + suffix):
                        value[field] = STARTUP_LAUNCHER + "dispatch.py --host " + host + " --event " + event
        for key, item in value.items():
            _refresh_legacy_startup(item, key if key in STARTUP_EVENTS else event)
    elif isinstance(value, list):
        for item in value:
            _refresh_legacy_startup(item, event)


def _refresh_legacy_ownership(value):
    """Migrate only the complete previously emitted ownership command, not wrappers."""
    prefix = ("py=$(python3 -c 'import sys;print(sys.executable)' 2>/dev/null); "
              "[ -x \"$py\" ] || py=$(python -c 'import sys;print(sys.executable)'); "
              'root=$(git rev-parse --show-toplevel) || exit 2; '
              'exec "$py" "$root/docs/ai-forward-pack/scripts/coord-core.py" hook --host ')
    launcher = "git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh "
    if isinstance(value, dict):
        command = value.get("command")
        for host in ("claude", "copilot", "codex", "grok", "agy"):
            if command == prefix + host and not value.get("args"):
                caller = "" if host == "agy" else "--caller-cwd "
                value["command"] = launcher + caller + "../scripts/coord-core.py hook --host " + host
                break
        stop = prefix.replace('scripts/coord-core.py" hook --host ',
                              'hooks/owner-review-gate.py" --host ') + "codex --event Stop"
        if command == stop and not value.get("args"):
            value["command"] = launcher + "owner-review-gate.py --host codex --event Stop"
        for item in value.values():
            _refresh_legacy_ownership(item)
    elif isinstance(value, list):
        for item in value:
            _refresh_legacy_ownership(item)


def merge_hook_ownership(current_text):
    """Refresh existing native opt-ins only; never create or enable an ownership hook."""
    config = json.loads(merge_claude_settings('{"hooks":{}}', current_text))
    _refresh_legacy_ownership(config)
    return json.dumps(config, indent=2) + "\n"


def merge_claude_settings(source_text, current_text=None):
    """Replace complete shipped commands, never custom wrappers or permission settings."""
    source = json.loads(source_text)
    current = json.loads(current_text.lstrip("\ufeff")) if current_text is not None else {}
    for config in (source, current):
        if not isinstance(config, dict) or not isinstance(config.get("hooks", {}), dict):
            raise ValueError("Settings and hooks must be objects")
        for entries in config.get("hooks", {}).values():
            if not isinstance(entries, list):
                raise ValueError("Hook events must be arrays")
            for entry in entries:
                if not isinstance(entry, dict) or not isinstance(entry.get("hooks"), list):
                    raise ValueError("Hook entries must contain handler arrays")
                if "matcher" in entry and not isinstance(entry["matcher"], str):
                    raise ValueError("Hook matcher must be a string")
                for handler in entry["hooks"]:
                    if not isinstance(handler, dict):
                        raise ValueError("Hook handlers must be objects")
                    if handler.get("type") == "command" and not isinstance(handler.get("command"), str):
                        raise ValueError("Command handlers require a command string")
                    if "command" in handler and not isinstance(handler["command"], str):
                        raise ValueError("Hook commands must be strings")
    _refresh_legacy_startup(current)
    prefix = ("py=$(python3 -c 'import sys;print(sys.executable)' 2>/dev/null); "
              "[ -x \"$py\" ] || py=$(python -c 'import sys;print(sys.executable)'); ")
    launcher = "git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh "
    ownership = launcher + "--caller-cwd ../scripts/coord-core.py hook --host claude"
    ownership_old = {
        prefix + '"$py" "' + path + '" hook'
        for path in ("pack/scripts/coord-core.py", "docs/ai-forward-pack/scripts/coord-core.py")
    }
    ownership_old.add(prefix + 'root=$(git rev-parse --show-toplevel) || exit 2; '
                      'exec "$py" "$root/docs/ai-forward-pack/scripts/coord-core.py" hook --host claude')
    hooks = current.setdefault("hooks", {})
    ships_dispatcher = any("run-hook.sh dispatch.py --host claude" in handler.get("command", "")
                           for entries in source.get("hooks", {}).values()
                           for entry in entries for handler in entry["hooks"])
    if ships_dispatcher:
        # B15: the exact per-hook commands revision 100 (and its older spellings) shipped are
        # retired; a custom wrapper never matches exactly and survives.
        retired = re.compile("(?:" + re.escape(launcher) + "|(?:" + re.escape(prefix + '"$py" ') +
                             "|python )docs/ai-forward-pack/hooks/)(?:" + "|".join(RETIRED_HOOK_SCRIPTS) +
                             r")\.py --host claude(?: --event \w+)?")
        for event in list(hooks):
            kept = []
            for entry in hooks[event]:
                handlers = [h for h in entry["hooks"]
                            if h.get("args") or not retired.fullmatch(str(h.get("command", "")))]
                if handlers or not entry["hooks"]:
                    kept.append(dict(entry, hooks=handlers))
            hooks[event] = kept
    for event, entries in source.get("hooks", {}).items():
        replacements = {}
        for entry in entries:
            for handler in entry["hooks"]:
                command = handler.get("command", "")
                match = re.fullmatch(re.escape(launcher) + r"([a-z-]+\.py) (--host claude(?: --event \w+)?)", command)
                if not match:
                    match = re.fullmatch(re.escape(STARTUP_LAUNCHER) + r"(session-start\.py) (--host claude)", command)
                if match:
                    script, args = match.groups()
                    target = "docs/ai-forward-pack/hooks/" + script
                    for old in (prefix + '"$py" ' + target + " " + args,
                                "python " + target + " " + args):
                        replacements[old] = command
        if event == "PreToolUse":
            replacements.update({old: ownership for old in ownership_old})
        have = hooks.setdefault(event, [])
        for entry in have:
            for handler in entry["hooks"]:
                old = handler.get("command")
                if old in replacements and not handler.get("args"):
                    handler["command"] = replacements[old]
        existing = {handler.get("command") for entry in have for handler in entry["hooks"]}
        for entry in entries:
            if not any(handler.get("command") in existing for handler in entry["hooks"]):
                have.append(entry)
                existing.update(handler.get("command") for handler in entry["hooks"])
    for key, value in source.items():
        if key != "hooks":
            current.setdefault(key, value)
    return json.dumps(current, indent=2) + "\n"

BEGIN = "<!-- AI-FORWARD-PACK:BEGIN"
END = "<!-- AI-FORWARD-PACK:END -->"
IMPORT_LINE = "@AGENTS.md"
# THE INVARIANT IS "the registry travels with the repo", not any particular pattern.
# `.agents/*` then `!.agents/artifacts.yml` is ONE shape that satisfies it, and the shape
# this script writes by default: a bare `.agents/` excludes the DIRECTORY, git never
# descends into it, the negation never fires, and the registry lands where git cannot see
# it. Other shapes satisfy the invariant too -- a repo that commits its per-run records
# under `.agents/` (a coordination log, an episode-capture log) satisfies it by the wider
# route, and for THAT repo this blanket is wrong: tracked files stay tracked and look fine
# while every new record becomes invisible. So the pattern is CONDITIONAL below, and the
# check is `git check-ignore --quiet <representative path>` -- exit 0 means ignored.
# Never `-v`: with -v the exit code means "some pattern matched", and a `!` negation
# counts, so it exits 0 for a re-included path and inverts the answer (measured).
GITIGNORE_LINES = ["*.jsonl.lock", "spikes/", "docs/audit/.run-starts.json",
                   "docs/audit/.run-starts.json.tmp",
                   ".agents/*", "!.agents/artifacts.yml", "!.agents/log/",
                   "!.agents/skills*", "!.agents/hooks.json", "!.agents/rules*",
                   "!.agents/session-checks.json",
                   ".agents/mail/", ".agents/log/audit/.run-starts.json",
                   ".agents/log/audit/.run-starts.json.tmp",
                   "**/docs/audit/.run-starts.json", "**/docs/audit/.run-starts.json.tmp",
                   "**/.agents/log/audit/.run-starts.json", "**/.agents/log/audit/.run-starts.json.tmp"]
# D10 (ratified 2026-09-19): the coord ledgers `.agents/log/` are TRACKED by default - git is the
# durable and cross-machine path for state-changing mail (their body-less twins). The mail
# inboxes `.agents/mail/` are machine-local and carry bodies, so they are ignored by an EXPLICIT
# line that holds even when the `.agents/*` blanket is withheld below.

# A .gitignore is LAST-MATCH-WINS, so a blanket appended below an existing rule silently
# reverses it. Measured 2026-09-09 in a consuming repo: line 495 recorded "spikes/ is NOT
# ignored in this repo (pack default overridden) ... (Test Architect gate, 2026-08-26)";
# this block re-appended a blanket `spikes/` twenty-nine lines later and won. 117 tracked
# spike files became reachable only with `git add -f`, and a forgotten `-f` loses evidence
# with NO signature -- `git add -A` drops a NEW ignored file silently, `git status` never
# lists it, and the commit succeeds. The default is not the defect; appending it over a
# recorded decision is.
#
# Three deterministic guards, no heuristics and no comment-parsing:
#   1. a CONDITIONAL line is withheld when the repo's own state answers its condition;
#   2. a line the repo already re-includes is REPORTED, never appended;
#   3. `# pack-apply: decline <pattern>` makes a declination survive the next refresh --
#      without it, deleting the line is undone by the following `/updatepack`.
# Every withholding produces a KEEP row. Silence was the whole problem.

# INSTALL.md 2 has always carried this condition and this script applied it unconditionally:
# "add `spikes/` to `.gitignore` UNLESS a probe is worth keeping as evidence." A repo that
# TRACKS files under spikes/ has already answered that, in the only way git records an answer.
# pattern -> (probe path, paths the pack's own negation already re-includes, why)
CONDITIONAL_GITIGNORE = {
    "spikes/": ("spikes", (),
                "the repo tracks {n} file(s) under spikes/ - INSTALL 2 adds this line"
                " *unless a probe is worth keeping as evidence*, and tracking one is how"
                " that is recorded"),
    ".agents/*": (".agents", (".agents/artifacts.yml",),
                  "the repo tracks {n} file(s) under .agents/ beyond the registry, so it"
                  " commits its per-run records; this blanket would leave those tracked"
                  " and hide every NEW one, with no signature"),
}

# A `!` line exists only to punch through its blanket. Withhold the blanket and the
# exception is a negation with nothing to negate -- inert, and it tells a reader the
# opposite of the KEEP row that withheld the blanket.
GITIGNORE_DEPENDENTS = {".agents/*": ("!.agents/artifacts.yml", "!.agents/log/")}

# PLAT-A (P3): the pack writes LF everywhere and merges derived files by byte identity; both
# rest on the working tree being LF on every OS, which only .gitattributes can promise.
GITATTRIBUTES_LINES = ["* text=auto eol=lf"]
EDITORCONFIG_TEXT = (
    "# AI-Forward Pack (INSTALL 2, revision 75): the editor-level twin of .gitattributes eol=lf,\n"
    "# so a Windows editor does not re-introduce CRLF between commits. Yours to extend.\n"
    "root = true\n"
    "\n"
    "[*]\n"
    "charset = utf-8\n"
    "end_of_line = lf\n"
    "insert_final_newline = true\n"
    "trim_trailing_whitespace = false\n"
)

DECLINE_MARKER = "# pack-apply: decline "


def gitignore_negations(pattern):
    """The literal forms a repo writes to re-include a pattern this block would ignore.

    Note what git does NOT allow: `spikes/` followed by `!spikes/**` still ignores the
    tree, because git stops descending at an excluded directory and never consults the
    deeper negation (measured). So a negation cannot repair a blanket that was already
    appended -- which is why this is checked BEFORE appending, not after.
    """
    base = pattern.rstrip("/")
    return {"!" + base, "!" + base + "/", "!" + base + "/**", "!/" + base,
            "!/" + base + "/", "!/" + base + "/**"}
PROTECTED = {"docs/docs-index.js"}
PATH_NORMALISERS = [
    (re.compile(r"\.github/instructions/([\w.-]+?)\.instructions\.md"), r"<doc:\1>"),
    (re.compile(r"\.claude/knowledge/([\w.-]+?)\.md"), r"<doc:\1>"),
    (re.compile(r"\.github/knowledge/([\w.-]+?)\.md"), r"<doc:\1>"),
    (re.compile(r"\.github/prompts/([\w-]+?)\.prompt\.md"), r"<skill:\1>"),
    (re.compile(r"\.claude/skills/([\w-]+?)/SKILL\.md"), r"<skill:\1>"),
    (re.compile(r"\.github/agents/"), "<agents>/"),
    (re.compile(r"\.claude/agents/"), "<agents>/"),
    (re.compile(r"\.github/instructions/"), "<docs>/"),
    (re.compile(r"\.claude/knowledge/"), "<docs>/"),
]


# --------------------------------------------------------------------------- io helpers
def read(path):
    try:
        with open(path, encoding="utf-8", errors="replace", newline="") as fh:
            return fh.read()
    except OSError:
        return None


def norm_nl(text):
    return text.replace("\r\n", "\n").replace("\r", "\n") if text is not None else None


def same(a, b):
    """Equal after newline normalisation and a stripped BOM - a CRLF checkout is not a drift."""
    if a is None or b is None:
        return a is b
    return norm_nl(a).lstrip("﻿").rstrip() == norm_nl(b).lstrip("﻿").rstrip()


def frontmatter(text):
    m = re.match(r"^﻿?---\r?\n(.*?)\r?\n---\r?\n", text, re.S)
    if not m:
        return {}, text
    meta = {}
    for line in m.group(1).splitlines():
        kv = re.match(r"^([A-Za-z_][\w-]*):\s*(.*)$", line)
        if kv:
            val = kv.group(2).strip()
            meta[kv.group(1)] = val.strip('"').strip("'") if not val.startswith("[") else val
    return meta, text[m.end():]


def git(args, cwd):
    try:
        p = subprocess.run(["git"] + args, cwd=cwd, capture_output=True, text=True, timeout=60,
                           encoding="utf-8", errors="replace")
        return p.returncode, p.stdout
    except (OSError, subprocess.SubprocessError):
        return 1, ""


# --------------------------------------------------------------------------- the applier
RUNNING_SCRIPT = os.path.abspath(__file__)


def default_source():
    """The ai-forward clone that ships the RUNNING script, when there is one.

    This is what makes the inverted invocation safe to document: the source and the
    program that reads it cannot be mismatched, because the program derives the source
    from where it lives. An INSTALLED copy sits at <repo>/docs/ai-forward-pack/scripts/,
    which is not a clone, so it gets None and must still be told - never a guess (NG1).
    """
    clone = os.path.dirname(os.path.dirname(os.path.dirname(RUNNING_SCRIPT)))
    if os.path.isfile(os.path.join(clone, "pack", "adapters", "INSTALL.md")):
        return clone
    return None


class Applier(object):
    def __init__(self, source, target, dry, project=None, install=False, force=False,
                 baselines=True, allow_stale=False):
        self.source = os.path.abspath(source)
        self.pack = os.path.join(self.source, "pack")
        self.target = os.path.abspath(target)
        self.dry = dry
        self.project = project
        self.install = install
        self.force = force
        self.baselines = baselines
        self.allow_stale = allow_stale
        self.rows = []
        self.old_pack_shas = None
        self.source_rev, self.source_meta = self._source_revision()
        self.target_rev = self._target_revision()
        # Snapshot provenance before apply advances the installed INSTALL.md.
        self.target_install_text = read(os.path.join(self.target, "docs", "ai-forward-pack", "INSTALL.md"))

    def _project_name(self):
        """The TARGET repo's canonical name -- never `basename(target)` (class PACK-P).

        `--project` still wins. Without it this used the target directory's basename, which
        stamps a WORKTREE folder into the target's committed `docs/index.html` and `AGENTS.md`
        whenever the install is run from a worktree -- which WT1 often makes it.
        """
        here = os.path.dirname(os.path.abspath(__file__))
        if here not in sys.path:
            sys.path.insert(0, here)
        try:
            from repo_identity import canonical_project
        except ImportError:
            return self.project or os.path.basename(self.target)
        return canonical_project(self.target, self.project)

    # ---- bookkeeping
    def _stale_applier(self):
        """Is the pack-apply.py RUNNING the one this source ships? (the bootstrap defect)

        The deployment map IS this program, and `/updatepack` runs the TARGET's installed
        copy. So a revision whose point is a change to pack-apply.py cannot protect the
        very refresh that installs it: the old copy computes the plan, and the new copy is
        merely one of the files it copies.

        Measured 2026-09-09 against a consuming repo at revision 63, planning against
        revision-64 source: `.gitignore | UPDATE | added spikes/, .agents/*` -- both lines
        that repo had explicitly declined, and exactly what revision 64 exists to withhold.
        The identical plan run with the rev-64 script withheld both, with reasons.

        Rev 65 asked the wrong question. It compared the target's INSTALLED copy with the
        source's, which is not the same as asking which program is computing this plan --
        so it accused the source's own script of being the target's stale one whenever the
        target happened to be behind. That false positive also hid the cure: running the
        SOURCE's copy is exactly what makes staleness impossible, and the check said it was
        the disease. Ask about `RUNNING_SCRIPT` and both readings come out right.

        Returns the remedy note, or "" when there is nothing to say. An unreadable copy is
        NOT reported as stale: absent or unreadable is not evidence of staleness (R4).
        """
        mine = read(RUNNING_SCRIPT)
        theirs = read(os.path.join(self.pack, "scripts", "pack-apply.py"))
        if mine is None or theirs is None or norm_nl(mine) == norm_nl(theirs):
            return ""
        return ("this plan was computed by {0}, which differs from the pack-apply.py this "
                "source ships - the deployment map IS this program, so a revision that "
                "changes it cannot apply itself. Run the SOURCE's copy instead, which "
                "cannot be stale: python <source>/pack/scripts/pack-apply.py plan --target "
                ". (--allow-stale overrides, and applies the OLD map)"
                .format(os.path.basename(os.path.dirname(RUNNING_SCRIPT)) + "/"
                        + os.path.basename(RUNNING_SCRIPT)))

    def row(self, area, path, action, status="ok", note=""):
        self.rows.append({"area": area, "path": path.replace("\\", "/"), "action": action, "status": status, "note": note})

    def rel(self, path):
        return os.path.relpath(path, self.target).replace("\\", "/")

    def _source_revision(self):
        text = read(os.path.join(self.pack, "adapters", "INSTALL.md")) or ""
        meta, _ = frontmatter(text)
        rev = re.search(r"^revision:\s*(\d+)", text, re.M)
        return (int(rev.group(1)) if rev else None), meta

    def _target_revision(self):
        text = read(os.path.join(self.target, "docs", "ai-forward-pack", "INSTALL.md"))
        if text is None:
            return None
        rev = re.search(r"^revision:\s*(\d+)", text, re.M)
        return int(rev.group(1)) if rev else None

    def _old_pack_text(self, rel):
        """Resolve installed bytes by full INSTALL provenance, not a revision number.

        Independent lineages can publish the same revision; even one lineage can
        correct it without renumbering. Compare the immutable installed guidance
        with reachable source snapshots (including merge-parent comparisons).
        Missing or ambiguous provenance is not a merge base.
        """
        if self.target_rev is None or self.target_rev == self.source_rev:
            return None
        if self.old_pack_shas is None:
            self.old_pack_shas = []
            # Include pack-only edits too: identical INSTALL text does not prove
            # identical pack bytes. Never choose the oldest/newest matching tree.
            rc, out = git(["log", "--full-history", "-m", "--format=%H", "--", "pack"], self.source)
            if rc != 0 or self.target_install_text is None:
                return None
            installed = self.target_install_text.lstrip("\ufeff").replace("\r\n", "\n").replace("\r", "\n")
            for sha in dict.fromkeys(out.split()):
                rc, guidance = git(["show", sha + ":pack/adapters/INSTALL.md"], self.source)
                revision = re.search(r"^revision:\s*(\d+)\s*$", guidance, re.M)
                if (rc == 0 and revision is not None and int(revision.group(1)) == self.target_rev
                        and guidance.lstrip("\ufeff").replace("\r\n", "\n").replace("\r", "\n") == installed):
                    self.old_pack_shas.append(sha)
        if not self.old_pack_shas:
            return None
        old = None
        for sha in self.old_pack_shas:
            rc, text = git(["show", "{0}:pack/{1}".format(sha, rel.replace("\\", "/"))], self.source)
            if rc != 0 or (old is not None and text != old):
                return None
            old = text
        return old

    # ---- primitive writes
    def _contain(self, dest):
        """Refuse a destination that is a link or that resolves outside the target (FR-092).

        Checked on the deepest directory that already exists, before any is created, so a planted
        junction or symlink in a parent cannot redirect the write either.
        """
        root = os.path.realpath(self.target)
        if os.path.islink(dest):
            raise UnsafeDestination("{0} is a link".format(self.rel(dest)))
        probe = os.path.dirname(os.path.abspath(dest))
        while not os.path.exists(probe) and os.path.dirname(probe) != probe:
            probe = os.path.dirname(probe)
        real = os.path.realpath(probe)
        if os.path.normcase(os.path.commonpath([real, root])) != os.path.normcase(root):
            raise UnsafeDestination("{0} resolves outside the target ({1})".format(self.rel(dest), real))

    def _write(self, dest, text):
        if self.dry:
            return
        self._contain(dest)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        self._contain(dest)   # nothing raced a link into the directories just made
        with open(dest, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(norm_nl(text))

    def _remove(self, dest):
        if self.dry:
            return
        try:
            os.remove(dest)
        except OSError:
            pass

    def place(self, area, rel_src, dest, text, transform_note=""):
        """Land `text` at dest, honouring a repo-local deviation with a three-way merge."""
        drel = self.rel(dest)
        if drel in PROTECTED:
            self.row(area, drel, "SKIP", "ok", "protected (V10)")
            return
        current = read(dest)
        if current is None:
            self._write(dest, text)
            self.row(area, drel, "ADD", "ok", transform_note)
            return
        if same(current, text):
            self.row(area, drel, "UNCHANGED", "ok")
            return
        old = self._old_pack_text(rel_src) if rel_src else None
        if old is not None and same(current, self._transform_like(rel_src, old, dest)):
            self._write(dest, text)  # the repo had the pack's old text verbatim: a plain update
            self.row(area, drel, "UPDATE", "ok", transform_note)
            return
        if old is None and self.target_rev == self.source_rev:
            # same revision, different text: a repo-local deviation over an unchanged pack file - keep it
            self.row(area, drel, "KEEP", "ok", "repo-local deviation over an unchanged pack file")
            return
        base = self._transform_like(rel_src, old, dest) if old is not None else None
        merged = self._merge(current, base, text) if base is not None else None
        if merged is not None:
            self._write(dest, merged)
            self.row(area, drel, "MERGE", "ok", "repo-local deviation carried over (three-way merge)")
            return
        # conflict or no merge base: leave the file, park the new text, report
        park = os.path.join(self.target, "docs", "ai-forward-pack", "conflicts", drel)
        self._write(park, text)
        self.row(area, drel, "CONFLICT", "fail",
                 "repo-local changes could not be merged; new pack text at " + self.rel(park) + " - reconcile, then delete it")

    def _transform_like(self, rel_src, old_text, dest):
        """Apply the same deploy transform to the old pack text that `text` received (wrap / strip)."""
        drel = self.rel(dest)
        if drel.startswith(".github/instructions/") and not drel.endswith(MANIFEST):
            meta, body = frontmatter(old_text)
            pattern = "**" if meta.get("load", "always") == "always" else meta.get("applyTo", "**")
            return '---\napplyTo: "{0}"\n---\n'.format(pattern) + body
        if drel.startswith(".github/agents/"):
            return strip_tools(old_text)
        return old_text

    @staticmethod
    def _merge(current, base, new):
        """git merge-file current base new; returns merged text or None on conflict."""
        tmp = tempfile.mkdtemp()
        try:
            paths = []
            for name, text in (("current", current), ("base", base), ("new", new)):
                p = os.path.join(tmp, name)
                with open(p, "w", encoding="utf-8", newline="\n") as fh:
                    fh.write(norm_nl(text))
                paths.append(p)
            rc, out = git(["merge-file", "-p", "-L", "repo", "-L", "pack@installed", "-L", "pack@source"] + paths, tmp)
            if rc == 0:
                return out
            return None
        finally:
            shutil.rmtree(tmp, ignore_errors=True)

    # ---- families
    def knowledge(self):
        kdir = os.path.join(self.pack, "knowledge")
        names = sorted(n for n in os.listdir(kdir) if n.endswith(".md"))
        inst = os.path.join(self.target, ".github", "instructions")
        know = os.path.join(self.target, ".github", "knowledge")
        cc = os.path.join(self.target, ".claude", "knowledge")
        for name in names:
            src = os.path.join(kdir, name)
            text = read(src)
            stem = name[:-3]
            self.place("knowledge", "knowledge/" + name, os.path.join(cc, name), text)
            if name == MANIFEST:
                self.place("knowledge", "knowledge/" + name, os.path.join(inst, name), text)
                continue
            meta, body = frontmatter(text)
            load = meta.get("load", "")
            if load in ("always", "glob"):
                pattern = "**" if load == "always" else meta.get("applyTo", "")
                wrapped = '---\napplyTo: "{0}"\n---\n'.format(pattern) + body
                self.place("knowledge", "knowledge/" + name, os.path.join(inst, stem + ".instructions.md"), wrapped, "wrapped applyTo")
                stale = os.path.join(know, name)
            elif load in ("skill", "reference"):
                self.place("knowledge", "knowledge/" + name, os.path.join(know, name), text)
                stale = os.path.join(inst, stem + ".instructions.md")
            else:
                self.row("knowledge", "pack/knowledge/" + name, "ERROR", "fail", "no load: scope declared")
                continue
            if os.path.isfile(stale):
                self._remove(stale)
                self.row("knowledge", self.rel(stale), "REMOVE", "ok", "stale copy: the doc's load scope moved (CTX-E)")

    def skills(self):
        cdir = os.path.join(self.pack, "commands")
        pdir = os.path.join(self.pack, "adapters", "copilot", "prompts")
        inventory = {}
        for name in sorted(os.listdir(cdir)):
            sdir = os.path.join(cdir, name)
            if not os.path.isfile(os.path.join(sdir, "SKILL.md")):
                continue
            inventory[name] = []
            for base, _dirs, files in os.walk(sdir):
                for f in sorted(files):
                    src = os.path.join(base, f)
                    rel_in_skill = os.path.relpath(src, sdir)
                    inventory[name].append(rel_in_skill.replace("\\", "/"))
                    rel_skill = "commands/{0}/{1}".format(name, rel_in_skill.replace("\\", "/"))
                    text = read(src)
                    self.place("skills", rel_skill,
                               os.path.join(self.target, ".claude", "skills", name, rel_in_skill), text)
                    self.place("skills", rel_skill,
                               os.path.join(self.target, ".grok", "skills", name, rel_in_skill), text)
                    self.place("skills", rel_skill,
                               os.path.join(self.target, ".agents", "skills", name, rel_in_skill), text)
            prompt = os.path.join(pdir, name + ".prompt.md")
            if os.path.isfile(prompt):
                self.place("skills", "adapters/copilot/prompts/{0}.prompt.md".format(name),
                           os.path.join(self.target, ".github", "prompts", name + ".prompt.md"), read(prompt))
        inventory = {name: sorted(files) for name, files in inventory.items()}
        # This is derived data, not an authored pack file with a git merge base.
        # Always regenerate stale inventory; preserve local skills by leaving
        # their directories alone, not by merging them into the pack manifest.
        manifest = os.path.join(self.target, "docs", "ai-forward-pack", "codex-skills.json")
        try:
            current_inventory = json.loads(read(manifest) or "null")
        except ValueError:
            current_inventory = None
        if current_inventory == inventory:
            self.row("bundle", self.rel(manifest), "UNCHANGED", "ok")
        else:
            action = "UPDATE" if os.path.exists(manifest) else "ADD"
            self._write(manifest, json.dumps(inventory, indent=2) + "\n")
            self.row("bundle", self.rel(manifest), action,
                     "ok", "derived pack skill inventory regenerated from commands/")
        self.place("bundle", "adapters/codex/codex.md",
                   os.path.join(self.target, "docs", "ai-forward-pack", "codex.md"),
                   read(os.path.join(self.pack, "adapters", "codex", "codex.md")))
        self.place("bundle", "adapters/muse/muse-surface.md",
                   os.path.join(self.target, "docs", "ai-forward-pack", "muse-surface.md"),
                   read(os.path.join(self.pack, "adapters", "muse", "muse-surface.md")))

    def agents(self):
        cc = os.path.join(self.pack, "adapters", "claude-code", "agents")
        cop = os.path.join(self.pack, "adapters", "copilot", "agents")
        for name in sorted(os.listdir(cc)):
            if name.endswith(".md"):
                text = read(os.path.join(cc, name))
                self.place("agents", "adapters/claude-code/agents/" + name, os.path.join(self.target, ".claude", "agents", name), text)
                self.place("agents", "adapters/claude-code/agents/" + name,
                           os.path.join(self.target, ".github", "agents", name[:-3] + ".agent.md"), strip_tools(text), "tools: stripped")
                self.place("agents", "adapters/claude-code/agents/" + name,
                           os.path.join(self.target, ".grok", "agents", grok_agent_filename(name)),
                           strip_tools(text), "tools: stripped; Grok type name")
        for name in sorted(os.listdir(cop)):
            if name.endswith("_agent.md"):
                text = read(os.path.join(cop, name))
                self.place("agents", "adapters/copilot/agents/" + name, os.path.join(self.target, ".claude", "agents", name), text)
                self.place("agents", "adapters/copilot/agents/" + name,
                           os.path.join(self.target, ".github", "agents", name[:-len("_agent.md")] + ".agent.md"), text, "renamed .agent.md")
                self.place("agents", "adapters/copilot/agents/" + name,
                           os.path.join(self.target, ".grok", "agents", grok_agent_filename(name)),
                           strip_tools(text), "tools: stripped; Grok type name")

    def bundle(self):
        dp = os.path.join(self.target, "docs", "ai-forward-pack")
        for sub in ("templates", "scripts"):
            src_dir = os.path.join(self.pack, sub)
            for base, _dirs, files in os.walk(src_dir):
                if "__pycache__" in base:
                    continue
                for f in sorted(files):
                    if f.endswith(".pyc"):
                        continue
                    src = os.path.join(base, f)
                    rel = os.path.relpath(src, src_dir)
                    self.place("bundle", "{0}/{1}".format(sub, rel.replace("\\", "/")), os.path.join(dp, sub, rel), read(src))
        for f in ("README.md", "OVERVIEW.md", "research-synthesis.md", "context-budget.json"):
            self.place("bundle", f, os.path.join(dp, f), read(os.path.join(self.pack, f)))
        hooks = os.path.join(self.pack, "adapters", "hooks")
        for f in ("dispatch.py", "reread-guard.py", "session-start.py", "mail-doorbell.py", "heartbeat.py", "coord_identity.py",
                  "owner-review-gate.py", "git-identity-guard.py", "run-hook.sh", "copilot.ai-forward-hooks.json",
                  "README.md"):
            self.place("hooks", "adapters/hooks/" + f, os.path.join(dp, "hooks", f), read(os.path.join(hooks, f)))
        self.place("hooks", "adapters/hooks/copilot.ai-forward-hooks.json",
                   os.path.join(self.target, ".github", "hooks", "ai-forward.json"), read(os.path.join(hooks, "copilot.ai-forward-hooks.json")))
        self.place("hooks", "adapters/hooks/grok.ai-forward-hooks.json",
                   os.path.join(self.target, ".grok", "hooks", "ai-forward.json"),
                   read(os.path.join(hooks, "grok.ai-forward-hooks.json")))
        # A customized whole native bundle is normally kept as a local deviation.
        # Startup migration is narrower: change only exact formerly shipped commands
        # inside it, preserving extra handlers, native metadata and all settings.
        for relative in (".github/hooks/ai-forward.json", ".grok/hooks/ai-forward.json"):
            target = os.path.join(self.target, *relative.split("/"))
            try:
                current = read_hook_settings(target)
                if current is None:
                    continue
                config = json.loads(current)
                if not isinstance(config, dict) or not isinstance(config.get("hooks"), dict):
                    raise ValueError("native hooks must be an object")
                before = json.dumps(config, sort_keys=True)
                _refresh_legacy_startup(config)
                if json.dumps(config, sort_keys=True) != before:
                    self._write(target, json.dumps(config, indent=2) + "\n")
                    self.row("hooks", relative, "MERGE", "ok", "exact startup commands refreshed; custom policy retained")
            except (OSError, ValueError, TypeError):
                self.row("hooks", relative, "CONFLICT", "fail", "invalid native hooks; existing file retained")
        agy_target = os.path.join(self.target, ".agents", "hooks.json")
        try:
            current_hooks = read_hook_settings(agy_target)
            if os.path.islink(agy_target):
                raise ValueError("symlink hook target")
            merged_hooks = merge_named_hook_bundles(read(os.path.join(hooks, "agy.ai-forward-hooks.json")), current_hooks)
            if merged_hooks != current_hooks:
                self._write(agy_target, merged_hooks)
            self.row("hooks", ".agents/hooks.json", "UNCHANGED" if merged_hooks == current_hooks else "MERGE", "ok")
        except (OSError, ValueError):
            self.row("hooks", ".agents/hooks.json", "CONFLICT", "fail", "invalid named bundles; existing file retained")
        self.place("hooks", "adapters/grok/grok-surface.md",
                   os.path.join(self.target, ".grok", "rules", "grok-surface.md"),
                   read(os.path.join(self.pack, "adapters", "grok", "grok-surface.md")))
        self.place("hooks", "adapters/antigravity/agy-surface.md",
                   os.path.join(self.target, ".agents", "rules", "agy-surface.md"),
                   read(os.path.join(self.pack, "adapters", "antigravity", "agy-surface.md")))
        skills_json = json.dumps({"entries": [{"path": ".agents/skills"}, {"path": ".claude/skills"}]}, indent=2) + "\n"
        self.place("bundle", ".agents/skills.json",
                   os.path.join(self.target, ".agents", "skills.json"), skills_json)
        self._settings(read(os.path.join(hooks, "claude-code.settings.hooks.json")))
        optional = [os.path.join(self.target, ".codex", "hooks.json")]
        grok_hooks = os.path.join(self.target, ".grok", "hooks")
        if os.path.isdir(grok_hooks):
            optional.extend(os.path.join(grok_hooks, name) for name in sorted(os.listdir(grok_hooks))
                            if name.endswith(".json"))
        for target in optional:
            relative = os.path.relpath(target, self.target).replace("\\", "/")
            try:
                current = read_hook_settings(target)
                if current is None:
                    continue
                merged = merge_hook_ownership(current)
            except (OSError, ValueError, TypeError):
                self.row("hooks", relative, "CONFLICT", "fail", "invalid native hooks; existing file retained")
                continue
            if merged != current:
                self._write(target, merged)
            self.row("hooks", relative, "UNCHANGED" if merged == current else "MERGE", "ok",
                     "existing opt-ins only; native review remains required")
        self._gitignore()
        self._gitattributes()
        self._editorconfig()
        # docs/index.html is the SAME signal docs-graph.py's `_opted_into_docs_explorer`
        # reads as "this repo wants the Docs Explorer" before `derive` will write
        # docs/docs-index.js (AL0.2/V10). Instantiating it here, at bare install, would
        # satisfy that signal for every repo the installer ever touches -- with zero
        # content under docs/ -- so the next bare `derive` (every shipped skill calls it
        # bare) would seed docs-index.js unasked (PK-03, measured in grid-4: the shell
        # present pre-task, docs-index.js added outside the task's declared scope). The
        # shell is instantiated only by a content-creating skill, as ITS last action
        # (V10 step 3), once it has real content to show -- never by install.
        explorer = os.path.join(self.target, "docs", "index.html")
        if os.path.isfile(explorer):
            self.row("bundle", "docs/index.html", "SKIP", "ok", "exists - never overwritten")
        else:
            self.row("bundle", "docs/index.html", "SKIP", "ok",
                     "never created by install - a content-creating skill instantiates it "
                     "once it has real content to show (V10)")
        self.row("bundle", "docs/docs-index.js", "SKIP", "ok", "never created or overwritten (V10)")

    def _settings(self, snippet_text):
        dest = os.path.join(self.target, ".claude", "settings.json")
        try:
            current_text = read_hook_settings(dest)
            merged = merge_claude_settings(snippet_text, current_text)
        except (OSError, ValueError, TypeError):
            self.row("hooks", ".claude/settings.json", "CONFLICT", "fail",
                     "HOOK-SETTINGS-INVALID: existing settings retained; inspect JSON and nested hook shapes")
            return
        if merged == current_text:
            self.row("hooks", ".claude/settings.json", "UNCHANGED", "ok")
            return
        self._write(dest, merged)
        self.row("hooks", ".claude/settings.json", "ADD" if current_text is None else "MERGE", "ok",
                 "exact shipped hooks refreshed; custom hooks and settings retained; BOM-free UTF-8")

    def _tracked_under(self, path):
        """Which files does the TARGET repo track under this path? None = cannot tell.

        R4: `git ls-files` failing (not a repo, no git, a fresh target) is not evidence
        that nothing is tracked. None keeps the pack default, which is right where the
        question cannot be asked; it never invents an answer.
        """
        try:
            proc = subprocess.run(["git", "ls-files", "--", path], cwd=self.target,
                                  capture_output=True, text=True, encoding="utf-8",
                                  errors="replace", timeout=20)
        except (OSError, subprocess.SubprocessError):
            return None
        if proc.returncode != 0:
            return None
        return [l.strip() for l in proc.stdout.splitlines() if l.strip()]

    def _gitignore_withhold(self, line, have):
        """Why this pack line must NOT be appended to this repo, or "" to append it."""
        # `# pack-apply: decline <pattern> [why]` - the pattern is the first token, so the
        # rest of the line is the human reason, recorded where the next reader will see it.
        declined = set()
        for entry in have:
            if entry.startswith(DECLINE_MARKER):
                rest = entry[len(DECLINE_MARKER):].split()
                if rest:
                    declined.add(rest[0])
        if line in declined:
            return "the repo records `{0}{1}`".format(DECLINE_MARKER, line)
        if line.startswith("**/"):
            # These suffix patterns include the root as well as descendants.
            # Do not reintroduce a root default explicitly withheld above.
            root_pattern = line[3:]
            root_reason = self._gitignore_withhold(root_pattern, have)
            if root_reason:
                return "its root pattern was withheld: " + root_reason
            suffix = "/" + root_pattern
            if any(entry.startswith("!") and entry[1:].endswith(suffix) for entry in have):
                return "the repo already re-includes a descendant duration marker"
        clash = sorted(gitignore_negations(line) & have)
        if clash:
            return ("the repo already re-includes it with {0} - .gitignore is"
                    " last-match-wins, so appending would silently reverse that"
                    .format(", ".join(clash)))
        condition = CONDITIONAL_GITIGNORE.get(line)
        if condition:
            probe, already_kept, template = condition
            tracked = self._tracked_under(probe)
            if tracked is not None:
                # The pack's own re-includes do not count as the repo contradicting it.
                answered = [f for f in tracked if f not in already_kept]
                if answered:
                    return template.format(n=len(answered))
        return ""

    def _gitignore(self):
        dest = os.path.join(self.target, ".gitignore")
        current = read(dest) or ""
        have = {l.strip() for l in norm_nl(current).splitlines()}
        missing, withheld = [], []
        for line in GITIGNORE_LINES:
            if line in have:
                continue
            why = self._gitignore_withhold(line, have)
            if not why:
                owner = next((b for b, deps in GITIGNORE_DEPENDENTS.items()
                              if line in deps and b in dict(withheld)), None)
                if owner:
                    why = "its blanket `{0}` was withheld, so it would negate nothing".format(
                        owner)
            (withheld if why else missing).append((line, why) if why else line)
        for line, why in withheld:
            # REPORTED, never silent: a reversal nobody is told about is the defect.
            self.row("bundle", ".gitignore", "KEEP", "ok",
                     "withheld `{0}`: {1}".format(line, why))
        if not missing:
            self.row("bundle", ".gitignore", "UNCHANGED", "ok")
            return
        text = norm_nl(current)
        if text and not text.endswith("\n"):
            text += "\n"
        text += "\n# AI-Forward Pack (INSTALL 2): local coordination and per-run state, never committed\n" + "\n".join(missing) + "\n"
        self._write(dest, text)
        self.row("bundle", ".gitignore", "UPDATE", "ok", "added " + ", ".join(missing))

    def _gitattributes(self):
        """Ship the line-ending policy every LF-writer in the pack rests on (PLAT-A, P3).

        The coord merge drivers merge derived files and append-only ledgers by byte identity,
        and every script writes LF on purpose - none of which holds on a Windows clone with
        core.autocrlf=true unless .gitattributes says `eol=lf`. This repo happened to carry
        the rule; a consuming repo need not, and the pack installed nothing. Lines are
        APPENDED when missing and never rewritten or removed: a repo's own attributes are
        its own (the same stance as `_gitignore`).
        """
        dest = os.path.join(self.target, ".gitattributes")
        current = read(dest) or ""
        have = {" ".join(l.split()) for l in norm_nl(current).splitlines()}
        missing = [line for line in GITATTRIBUTES_LINES if " ".join(line.split()) not in have]
        if not missing:
            self.row("bundle", ".gitattributes", "UNCHANGED", "ok")
            return
        text = norm_nl(current)
        if text and not text.endswith("\n"):
            text += "\n"
        text += ("\n# AI-Forward Pack (INSTALL 2, revision 75): text is LF on every OS, so the ledgers and\n"
                 "# derived files the coord merge drivers compare by byte identity mean the same thing\n"
                 "# on Windows and macOS. Run `git add --renormalize .` once after adding this.\n"
                 + "\n".join(missing) + "\n")
        self._write(dest, text)
        self.row("bundle", ".gitattributes", "UPDATE", "ok", "added " + ", ".join(missing))

    def _editorconfig(self):
        """Create .editorconfig once (LF, UTF-8, final newline) and never overwrite it - an
        editor-level twin of the git filter, so a Windows editor does not re-introduce CRLF
        into the working tree between commits (XT-10)."""
        dest = os.path.join(self.target, ".editorconfig")
        if os.path.exists(dest):
            self.row("bundle", ".editorconfig", "SKIP", "ok", "exists - never overwritten")
            return
        self._write(dest, EDITORCONFIG_TEXT)
        self.row("bundle", ".editorconfig", "ADD", "ok", "LF, UTF-8, final newline")

    # ---- front doors
    def front_doors(self):
        agents_block = read(os.path.join(self.pack, "adapters", "managed-blocks", "AGENTS.block.md"))
        claude_block = read(os.path.join(self.pack, "adapters", "managed-blocks", "CLAUDE.block.md"))
        agents_path = os.path.join(self.target, "AGENTS.md")
        claude_path = os.path.join(self.target, "CLAUDE.md")
        agents = read(agents_path)
        claude = read(claude_path)
        # AGENTS.md: replace the block wholesale (append if absent); create the file if missing.
        if agents is None:
            starter = "# {0}\n\nProject conventions live here. The AI-Forward Pack's reasoning stack is wired in below.\n\n".format(
                self._project_name())
            self._write(agents_path, starter + agents_block)
            self.row("front-doors", "AGENTS.md", "ADD", "ok", "created with the managed block")
            agents = starter + agents_block
        else:
            new_agents = replace_block(agents, agents_block)
            if same(new_agents, agents):
                self.row("front-doors", "AGENTS.md", "UNCHANGED", "ok")
            else:
                self._write(agents_path, new_agents)
                self.row("front-doors", "AGENTS.md", "UPDATE", "ok", "managed block re-pasted wholesale")
            agents = new_agents
        # CLAUDE.md: the import form.
        if claude is None:
            self._write(claude_path, "# CLAUDE.md\n\n" + IMPORT_LINE + "\n\n" + claude_block)
            self.row("front-doors", "CLAUDE.md", "ADD", "ok", "@AGENTS.md import + addendum")
            return
        if re.search(r"^\s*@AGENTS\.md\s*$", claude, re.M):
            new_claude = replace_block(claude, claude_block)
            if same(new_claude, claude):
                self.row("front-doors", "CLAUDE.md", "UNCHANGED", "ok", "import form")
            else:
                self._write(claude_path, new_claude)
                self.row("front-doors", "CLAUDE.md", "UPDATE", "ok", "addendum block re-pasted")
            return
        # A copy-style CLAUDE.md: back it up, keep what is unique, convert.
        backup = os.path.join(self.target, "docs", "ai-forward-pack", "retired",
                              "CLAUDE.md.rev{0}.md".format(self.target_rev if self.target_rev is not None else "pre"))
        self._write(backup, claude)
        unique = unique_paragraphs(claude, agents)
        kept = ""
        if unique:
            kept = ("<!-- retained from the previous CLAUDE.md on {0}: these paragraphs were not in AGENTS.md. Move them into AGENTS.md above its managed block (both hosts read it) and delete them here. -->\n\n".format(_dt.date.today().isoformat())
                    + "\n\n".join(unique) + "\n\n")
        new_claude = "# CLAUDE.md\n\n" + IMPORT_LINE + "\n\n" + kept + claude_block
        self._write(claude_path, new_claude)
        self.row("front-doors", "CLAUDE.md", "CONVERT", "ok",
                 "now @AGENTS.md + addendum (CTX-B); previous file backed up at {0}; {1} repo-local paragraph(s) retained above the addendum".format(
                     self.rel(backup), len(unique)))
        self._retire_parity_controls(backup)

    def _retire_parity_controls(self, backup_dir_hint):
        """A repo-local control that asserts CLAUDE.md carries the standing-method block encodes
        the invariant this conversion retires. It is rewritten into a shim that asserts the NEW
        invariant via pack-doctor and carries over the assertions that do not depend on CLAUDE.md;
        the original is backed up. Only PowerShell/Python files outside the pack's own directories
        are considered, and only when their text names CLAUDE.md together with a parity marker."""
        skip = (".git", "node_modules", "docs/ai-forward-pack", ".claude", ".github/instructions", ".github/knowledge")
        for base, dirs, files in os.walk(self.target):
            rel_base = self.rel(base)
            if any(rel_base == s or rel_base.startswith(s + "/") for s in skip):
                dirs[:] = []
                continue
            for f in files:
                if not f.endswith((".ps1", ".py", ".sh")):
                    continue
                path = os.path.join(base, f)
                text = read(path) or ""
                if "CLAUDE.md" not in text or not re.search(r"standing-method|parity", text, re.I):
                    continue
                if "pack-apply shim" in text:
                    self.row("controls", self.rel(path), "UNCHANGED", "ok", "already the new-invariant shim")
                    continue
                if not f.endswith(".ps1"):
                    self.row("controls", self.rel(path), "REVIEW", "fail",
                             "asserts the old CLAUDE.md/AGENTS.md parity; not rewritten automatically (not PowerShell) - update it to the import invariant")
                    continue
                bak = os.path.join(self.target, "docs", "ai-forward-pack", "retired", f)
                self._write(bak, text)
                self._write(path, parity_shim(text, self.rel(bak)))
                self.row("controls", self.rel(path), "REWRITE", "ok",
                         "old parity invariant retired: now asserts CLAUDE.md = @AGENTS.md + addendum via pack-doctor; original at " + self.rel(bak))

    # ---- finish
    def advance(self):
        dest = os.path.join(self.target, "docs", "ai-forward-pack", "INSTALL.md")
        text = read(os.path.join(self.pack, "adapters", "INSTALL.md"))
        if same(read(dest), text):
            self.row("meta", "docs/ai-forward-pack/INSTALL.md", "UNCHANGED", "ok", "revision {0}".format(self.source_rev))
        else:
            self._write(dest, text)
            self.row("meta", "docs/ai-forward-pack/INSTALL.md", "UPDATE", "ok",
                     "revision {0} -> {1}".format(self.target_rev, self.source_rev))

    def run_baselines(self):
        if self.dry or not self.baselines:
            self.row("meta", "context-budget baselines", "SKIP", "ok", "plan mode" if self.dry else "--no-baselines")
            return
        script = os.path.join(self.target, "docs", "ai-forward-pack", "scripts", "context-budget.py")
        for args in (["gate", "--update-baseline"], ["prefix", "--update-baseline"], ["skills", "--update-baseline"]):
            try:
                p = subprocess.run([sys.executable, script] + args, cwd=self.target,
                                   capture_output=True, text=True, encoding="utf-8",
                                   errors="replace", timeout=120)
                self.row("meta", "context-budget {0}".format(args[0]), "BASELINE", "ok" if p.returncode == 0 else "fail",
                         (p.stdout.strip().splitlines() or [""])[-1][:120])
            except (OSError, subprocess.SubprocessError) as exc:
                self.row("meta", "context-budget {0}".format(args[0]), "BASELINE", "fail", str(exc)[:120])

    def run(self):
        if self.source_rev is None:
            self.row("meta", "pack/adapters/INSTALL.md", "ERROR", "fail", "source revision unreadable - is --source an ai-forward clone?")
            return self.rows
        if self.target_rev is None and not self.install:
            self.row("meta", "docs/ai-forward-pack/INSTALL.md", "ERROR", "fail",
                     "no installed pack found; pass --install for a fresh install (/addpacktorepo)")
            return self.rows
        stale = self._stale_applier()
        if stale:
            # Reported on `plan` (the table above was computed by the OLD map) and FATAL on
            # `apply`, because applying a stale deployment map is precisely how a repo
            # silently misses the fix the revision exists to deliver.
            fatal = not self.dry and not self.allow_stale
            self.row("meta", "docs/ai-forward-pack/scripts/pack-apply.py", "STALE-APPLIER",
                     "fail" if fatal else "ok", stale)
            if fatal:
                return self.rows
        if self.target_rev is not None and self.target_rev > self.source_rev:
            self.row("meta", "revision", "ERROR", "fail",
                     "target is at revision {0}, ahead of the source {1} - refusing".format(self.target_rev, self.source_rev))
            return self.rows
        if self.target_rev == self.source_rev and not self.force:
            self.row("meta", "revision", "UNCHANGED", "ok", "already at revision {0} (use --force to re-apply)".format(self.source_rev))
        try:
            self.knowledge()
            self.skills()
            self.agents()
            self.bundle()
            self.front_doors()
            self.advance()
        except UnsafeDestination as exc:
            # A planted link makes the target suspect: stop before the revision advances.
            self.row("meta", "target", "REFUSED", "fail", "{0}; nothing written there - remove the link".format(exc))
            return self.rows
        self.run_baselines()
        return self.rows


# --------------------------------------------------------------------------- pure helpers
def grok_agent_filename(name):
    """Grok spawn_subagent types are the persona `name`, not the pack's `_agent` source suffix."""
    if name.endswith("_agent.md"):
        return name[:-len("_agent.md")] + ".md"
    return name


def strip_tools(text):
    """Drop the frontmatter `tools:` line and its indented continuation (INSTALL 1.2)."""
    out, in_tools = [], False
    for line in norm_nl(text).split("\n"):
        if re.match(r"^tools:", line):
            in_tools = True
            continue
        if in_tools and re.match(r"^\s+\S", line):
            continue
        in_tools = False
        out.append(line)
    return "\n".join(out)


def replace_block(text, block):
    """Replace the AI-FORWARD-PACK region wholesale (markers included); append if absent. The
    blank line that separated the block from what follows is preserved."""
    text = norm_nl(text)
    block = norm_nl(block).rstrip(chr(10)) + chr(10)
    b = text.find(BEGIN)
    e = text.find(END)
    if b >= 0 and e > b:
        rest = text[e + len(END):]
        rest = (chr(10) + rest.lstrip(chr(10))) if rest.strip() else ''
        return text[:b] + block + rest
    return text.rstrip(chr(10)) + chr(10) + chr(10) + block

def normalise(text):
    for rx, rep in PATH_NORMALISERS:
        text = rx.sub(rep, text)
    return re.sub(r"\s+", " ", text).strip()


def _outside_block(text):
    text = norm_nl(text)
    b = text.find(BEGIN)
    e = text.find(END)
    if b >= 0 and e > b:
        return text[:b] + "\n\n" + text[e + len(END):]
    return text


def unique_paragraphs(claude, agents):
    """Paragraphs of CLAUDE.md (outside the managed block) with no counterpart in AGENTS.md after
    toolchain-path normalisation; the title line and the import line are never 'unique'."""
    have = {normalise(p) for p in re.split(r"\n\s*\n", _outside_block(agents)) if p.strip()}
    out = []
    for p in re.split(r"\n\s*\n", _outside_block(claude)):
        if not p.strip():
            continue
        n = normalise(p)
        if n in have or re.match(r"^#\s*CLAUDE\.md\s*$", p.strip()) or p.strip() == IMPORT_LINE:
            continue
        out.append(p.strip("\n"))
    return out


def parity_shim(original, backup_rel):
    """A PowerShell shim asserting the import invariant through pack-doctor, carrying over the
    original's skill-surface needles and required-phrase checks where they can be read."""
    needles = re.findall(r"Path\s*=\s*'([^']+)'\s*;\s*Needle\s*=\s*'([^']+)'", original)
    phrases = []
    m = re.search(r"foreach\s*\(\$required\s+in\s+([^)]+)\)", original)
    if m:
        phrases = [p.replace("''", "'") for p in re.findall(r"'((?:[^']|'')*)'", m.group(1))]
    lines = [
        "#requires -Version 7.0",
        "<#",
        ".SYNOPSIS",
        "    pack-apply shim: the front doors are checked for the IMPORT invariant (CLAUDE.md = `@AGENTS.md` + the",
        "    Claude Code addendum), not for a duplicated standing-method block.",
        ".DESCRIPTION",
        "    Rewritten by pack-apply.py when CLAUDE.md was converted (AI-Forward revision 60+, class CTX-B): Copilot",
        "    CLI loads AGENTS.md and CLAUDE.md both, so a byte-identical copy paid the block twice per request;",
        "    Claude Code expands the @AGENTS.md import in place. Parity is now by construction. The original",
        "    control is preserved at " + backup_rel + ". Assertions that did not depend on CLAUDE.md carrying",
        "    the block were carried over below.",
        "#>",
        "[CmdletBinding()]",
        "param([string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot))",
        "Set-StrictMode -Version Latest",
        "$ErrorActionPreference = 'Stop'",
        "$script:failures = 0",
        "function Assert-True { param([Parameter(Mandatory)][bool]$Condition, [Parameter(Mandatory)][string]$Message)",
        "    if ($Condition) { Write-Host \"  PASS $Message\" } else { Write-Host \"  FAIL $Message\" -ForegroundColor Red; $script:failures++ } }",
        "",
        "$claude = Get-Content -LiteralPath (Join-Path $RepositoryRoot 'CLAUDE.md') -Raw",
        "$agents = Get-Content -LiteralPath (Join-Path $RepositoryRoot 'AGENTS.md') -Raw",
        "Assert-True ($claude -match '(?m)^\\s*@AGENTS\\.md\\s*$') 'CLAUDE.md imports AGENTS.md (the one instruction set for every harness).'",
        "Assert-True (($agents -split 'AI-FORWARD-PACK:BEGIN').Count -eq 2) 'AGENTS.md carries exactly one managed block.'",
        "Assert-True (($claude -split 'STANDING-METHOD:BEGIN').Count -le 1) 'CLAUDE.md no longer duplicates the standing-method block.'",
        "if ($agents -match 'STANDING-METHOD:BEGIN') {",
        "    Assert-True (($agents -split 'STANDING-METHOD:BEGIN').Count -eq 2) 'AGENTS.md carries exactly one standing-method block.'",
        "}",
        "$py = if (Get-Command python -ErrorAction SilentlyContinue) { 'python' } else { 'python3' }",
        "$doctor = Join-Path $RepositoryRoot 'docs/ai-forward-pack/scripts/pack-doctor.py'",
        "if (Test-Path -LiteralPath $doctor) {",
        "    $report = & $py $doctor --root $RepositoryRoot --json | ConvertFrom-Json",
        "    $imp = $report.checks | Where-Object { $_.name -eq 'claude-md import' }",
        "    Assert-True ($null -ne $imp -and $imp.status -eq 'PASS') \"pack-doctor: claude-md import ($($imp.detail))\"",
        "}",
    ]
    if phrases:
        lines.append("$block = $agents")
        lines.append("foreach ($required in " + ", ".join("'" + p.replace("'", "''") + "'" for p in phrases) + ") {")
        lines.append("    Assert-True ($block.Contains($required)) \"AGENTS.md states '$required'.\"")
        lines.append("}")
    if needles:
        lines.append("$surfaces = @(")
        for path, needle in needles:
            lines.append("    @{ Path = '" + path + "'; Needle = '" + needle.replace("'", "''") + "' }")
        lines.append(")")
        lines.append("foreach ($surface in $surfaces) {")
        lines.append("    $full = Join-Path $RepositoryRoot $surface.Path")
        lines.append("    $present = (Test-Path -LiteralPath $full) -and ((Get-Content -LiteralPath $full -Raw).Contains($surface.Needle))")
        lines.append("    Assert-True $present \"$($surface.Path) carries '$($surface.Needle)'.\"")
        lines.append("}")
    lines += [
        "if ($script:failures -gt 0) { Write-Host \"Front-door invariant FAILED: $script:failures assertion(s).\" -ForegroundColor Red; exit 1 }",
        "Write-Host 'Front-door invariant passed: CLAUDE.md imports AGENTS.md; the standing method lives once.'",
        "exit 0",
        "",
    ]
    return "\n".join(lines)


# --------------------------------------------------------------------------- report
def render_table(rows):
    out = ["| Area | Artifact | Action | Status | Note |", "|---|---|---|---|---|"]
    for r in rows:
        out.append("| {0} | `{1}` | {2} | {3} | {4} |".format(r["area"], r["path"], r["action"],
                                                             "OK" if r["status"] == "ok" else "FAIL", r["note"].replace("|", "/")))
    return "\n".join(out)


def summarize(rows):
    counts = {}
    for r in rows:
        counts[r["action"]] = counts.get(r["action"], 0) + 1
    return ", ".join("{0} {1}".format(v, k) for k, v in sorted(counts.items(), key=lambda kv: -kv[1]))


def main(argv=None):
    ap = argparse.ArgumentParser(prog="pack-apply.py", description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd")
    for name in ("plan", "apply"):
        p = sub.add_parser(name, help="every action, no writes" if name == "plan" else "apply the map idempotently")
        p.add_argument("--source", default=default_source(),
                       help="an ai-forward clone (holds pack/). Defaults to the clone "
                            "shipping this script, so running the source's own copy needs "
                            "no --source at all")
        p.add_argument("--target", default=os.getcwd(), help="the repo to update (default: cwd)")
        p.add_argument("--project", help="project name for docs/index.html on a fresh install")
        p.add_argument("--install", action="store_true", help="fresh install: allow a target with no installed pack")
        p.add_argument("--force", action="store_true", help="re-apply even when the revisions match")
        p.add_argument("--no-baselines", action="store_true", help="do not run context-budget --update-baseline after applying")
        p.add_argument("--json", action="store_true", help="emit the action rows as JSON")
        p.add_argument("--quiet", action="store_true", help="only the UNCHANGED rows are hidden")
        p.add_argument("--allow-stale", action="store_true",
                       help="apply even though the running pack-apply.py is older than the "
                            "source's. It applies the OLD deployment map - the recorded "
                            "exception, never the default")
    args = ap.parse_args(argv)
    if not args.cmd:
        ap.print_help()
        return 2
    if not args.source:
        # An installed copy cannot derive its clone. Say what to pass, and name the
        # invocation that needs nothing - never guess a source (NG1).
        print("pack-apply: --source is required here.\n"
              "  because   this copy is not inside an ai-forward clone, so the"
              " source cannot be derived from where it lives.\n"
              "  better    run the SOURCE's own copy, which needs no --source"
              " and can never be a stale deployment map:\n"
              "              python <ai-forward-clone>/pack/scripts/pack-apply.py"
              " plan --target .")
        return 2
    if not os.path.isfile(os.path.join(args.source, "pack", "adapters", "INSTALL.md")):
        print("pack-apply: --source must be an ai-forward clone containing pack/adapters/INSTALL.md", file=sys.stderr)
        return 2
    app = Applier(args.source, args.target, dry=(args.cmd == "plan"), project=args.project, install=args.install, allow_stale=args.allow_stale,
                  force=args.force, baselines=not args.no_baselines)
    rows = app.run()
    if args.json:
        print(json.dumps({"mode": args.cmd, "source_revision": app.source_rev, "target_revision": app.target_rev, "rows": rows}, indent=2))
    else:
        shown = [r for r in rows if not (args.quiet and r["action"] == "UNCHANGED")]
        print("pack-apply {0}: source revision {1}, target revision {2}{3}".format(
            args.cmd, app.source_rev, app.target_rev, "" if args.cmd == "apply" else " (no writes)"))
        print(render_table(shown))
        print("\n" + summarize(rows))
    return 1 if any(r["status"] == "fail" for r in rows) else 0


if __name__ == "__main__":
    sys.exit(main())
