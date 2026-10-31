#!/usr/bin/env python3
"""context-budget.py — the always-on context budget, measured (AI-Forward Pack).

An instruction set that is attached to every request IS the static prefix of every call.
It is re-read on every turn, it is billed on every turn (cached or not), and it subtracts
from the window before the user has said anything. Left undeclared, it grows silently:
each new knowledge doc looks free at the moment it is written, because nothing reports
what it costs.

This makes that cost a NUMBER, emitted on the normal path (instrumentation-over-inference
IO2/IO4: a feature is not done until its behaviour is measurable by default), and gates it
so the set cannot re-grow UNNOTICED (continuous-improvement CI6: a lesson recorded as
prose is a memoir). The control is a ratchet, not a ceiling: growing the set is fine,
growing it without recording that you did is what fails.

Every knowledge doc declares its own load scope in frontmatter:

    load: always                # attached to every request  -> Tier A, counts against the budget
    load: glob                  # attached to matching files -> Tier B, costs nothing elsewhere
    applyTo: "**/*.cs,**/*.csx"
    load: skill                 # read on demand by a skill  -> Tier C
    skills: [specify, implement]
    load: reference             # consulted, never attached  -> Tier D

FOUNDATION.md is the vendored provenance manifest: always-loaded by definition, kept
verbatim, and carries no frontmatter of its own.

Subcommands
  report      Tier table + the always-on total.
  gate        Fail on unacknowledged growth past the recorded baseline (ratchet),
              and on a derived backstop. CI-able. See pack/context-budget.json.
  agents      Per-agent declared knowledge prefix (the sub-agent lens, P3).
  preflight   Fail when an assembled prefix would not fit a model's window (P5).
  prefix      The WHOLE static prefix as the host assembles it - managed blocks (AGENTS.md /
              CLAUDE.md, counted twice where the host loads both), the always-on docs, plus
              stated tool/host allowances - with its own ratchet (CTX-B).
  skills      Per-skill SKILL.md size with a per-skill ratchet and a ceiling: a skill is
              re-injected whole on every invocation, so its size is a per-invocation tax (CTX-E).

Token figures are ESTIMATES (chars / 4.83) and are labelled as such everywhere. The ratio
is calibrated against a measured system prompt of 184,364 tokens over 890,204 characters of
this doc set. It is accurate enough to gate on and is never presented as a measurement:
where an exact count matters, count with the target model's tokenizer.

Python 3.8+, stdlib only.
"""
import argparse
import datetime
import json
import os
import re
import sys

# Windows consoles default to cp1252, which cannot encode the glyphs this tool prints.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

# Calibrated against the profiled session: 890,204 chars of knowledge docs reported as
# 184,364 system tokens => 4.83 chars/token. An estimate, not a measurement (see module doc).
CHARS_PER_TOKEN = 4.83

# The vendored provenance manifest is always-loaded and deliberately frontmatter-free.
MANIFEST = "FOUNDATION.md"

TIERS = {
    "always": "A", "glob": "B", "skill": "C", "reference": "D",
}
TIER_NOTE = {
    "always": "attached to every request",
    "glob": "attached to matching files only",
    "skill": "read on demand by a skill",
    "reference": "consulted, never attached",
}

HERE = os.path.dirname(os.path.abspath(__file__))


def est_tokens(chars):
    """Estimated tokens for a character count. Always reported as an estimate."""
    return int(round(chars / CHARS_PER_TOKEN))


def find_dir(*candidates, predicate=None):
    """Resolve a pack directory from either the pack layout or an installed repo.

    `predicate` guards against a same-named directory that is not the one meant: walking up
    from docs/ai-forward-pack/scripts, a bare "knowledge" candidate matches docs/knowledge/
    (the evidence dirs), which contains no knowledge docs at all. Matching it produced an
    empty scan that the gate then reported as clean -- defect class PACK-P.
    """
    start = HERE
    for _ in range(6):
        for rel in candidates:
            path = os.path.join(start, rel)
            if os.path.isdir(path) and (predicate is None or predicate(path)):
                return path
        parent = os.path.dirname(start)
        if parent == start:
            break
        start = parent
    return None


def _is_knowledge_dir(path):
    """A pack knowledge directory always carries the vendored provenance manifest."""
    return os.path.isfile(os.path.join(path, MANIFEST))


def _declares_scope(path):
    """At least one doc in the directory declares a `load:` scope. The vendored copy under
    docs/ai-forward-pack/knowledge carries the manifest but no frontmatter, so it satisfied
    _is_knowledge_dir and reported 461 always-on tokens over an unscoped corpus - green (CTX-B).
    A budget over a corpus that declares nothing is not a budget."""
    try:
        for name in os.listdir(path):
            if name.endswith(".md") and name != MANIFEST:
                meta, _ = read_frontmatter(os.path.join(path, name))
                if meta.get("load"):
                    return True
    except OSError:
        return False
    return False


def knowledge_dir(explicit=None):
    if explicit:
        return explicit
    # Walk every level for the declared layouts FIRST; a bare "knowledge" directory wins only
    # when nothing declared exists anywhere above - and only if it declares scope itself.
    declared = find_dir(os.path.join("pack", "knowledge"), os.path.join(".claude", "knowledge"),
                        predicate=lambda p: _is_knowledge_dir(p) and _declares_scope(p))
    if declared:
        return declared
    return find_dir(os.path.join("pack", "knowledge"), os.path.join(".claude", "knowledge"),
                    "knowledge", predicate=lambda p: _is_knowledge_dir(p) and _declares_scope(p))


def repo_root(explicit=None):
    """The repo root the managed blocks live in: the nearest ancestor holding AGENTS.md,
    CLAUDE.md or .git."""
    if explicit:
        return explicit
    start = HERE
    for _ in range(6):
        for marker in ("AGENTS.md", "CLAUDE.md", ".git"):
            if os.path.exists(os.path.join(start, marker)):
                return start
        parent = os.path.dirname(start)
        if parent == start:
            break
        start = parent
    return None


CONFIG_NAME = "context-budget.json"
CONFIG_DEFAULTS = {
    "always_on_tokens": None, "growth_tolerance_pct": 2, "shrink_report_pct": 5,
    "ceiling_tokens": 60000,
    # the whole-prefix ratchet (CTX-B) and the per-skill ratchet (CTX-E)
    "prefix_tokens": None, "prefix_allowances": {"tool_definitions": 24070, "host_prompt": 12000},
    "skills_baseline": {}, "skill_ceiling_tokens": 5000,
}


def config_path(explicit=None):
    """Locate the committed budget config (pack/ in the source repo, docs/ai-forward-pack/ once
    installed). Returns None when absent -- the gate then runs ceiling-only and says so."""
    if explicit:
        return explicit
    start = HERE
    for _ in range(6):
        for rel in (os.path.join("pack", CONFIG_NAME),
                    os.path.join("docs", "ai-forward-pack", CONFIG_NAME),
                    CONFIG_NAME):
            path = os.path.join(start, rel)
            if os.path.isfile(path):
                return path
        parent = os.path.dirname(start)
        if parent == start:
            break
        start = parent
    return None


def load_config(explicit=None):
    path = config_path(explicit)
    cfg = dict(CONFIG_DEFAULTS)
    if not path:
        return cfg, None
    try:
        with open(path, encoding="utf-8") as fh:
            raw = json.load(fh)
    except (OSError, ValueError) as exc:
        # A malformed config must not silently disable the gate: fail loudly at the caller.
        raise SystemExit(f"context-budget: cannot read {path}: {exc}")
    cfg.update({k: v for k, v in raw.items() if not k.startswith("_")})
    return cfg, path


def _eol(text):
    """The file's own line ending. The config is read and written with newline="" so an
    existing CRLF checkout round-trips unchanged; an insertion must therefore carry the
    file's EOL, not a literal "\n", or the rewrite leaves the file mixed (PLAT-A)."""
    return "\r\n" if "\r\n" in text else "\n"


def write_baseline(path, total, key="always_on_tokens", stamp="baseline_set_on"):
    """Rewrite only the named baseline + its stamp, preserving comments, key order and formatting."""
    with open(path, encoding="utf-8", newline="") as fh:
        text = fh.read()
    nl = _eol(text)
    if re.search(r'"%s":\s*(\d+|null)' % re.escape(key), text):
        text = re.sub(r'("%s":\s*)(\d+|null)' % re.escape(key), lambda m: m.group(1) + str(total), text, count=1)
    elif re.search(r'\r?\n\s*"always_on_tokens":', text):
        text = re.sub(r'(\r?\n\s*"always_on_tokens":)', lambda m: '%s  "%s": %d,%s' % (nl, key, total, m.group(1)), text, count=1)
    else:
        text = re.sub(r'^\s*\{', lambda m: m.group(0) + '%s  "%s": %d,' % (nl, key, total), text, count=1)
    if re.search(r'"%s":\s*"[^"]*"' % re.escape(stamp), text):
        text = re.sub(r'("%s":\s*)"[^"]*"' % re.escape(stamp),
                      lambda m: m.group(1) + '"' + datetime.date.today().isoformat() + '"', text, count=1)
    with open(path, "w", encoding="utf-8", newline="") as fh:
        fh.write(text)


def write_json_key(path, key, value):
    """Rewrite one JSON object-valued key (the per-skill baseline map). Comments and the other
    keys are preserved; the map is re-serialised one entry per line."""
    with open(path, encoding="utf-8", newline="") as fh:
        text = fh.read()
    nl = _eol(text)
    body = json.dumps(value, indent=4, sort_keys=True)
    body = nl.join(("  " + line) if i else line for i, line in enumerate(body.splitlines()))
    # a flat map of ints: `{}` and a multi-line map both match; nested braces never occur here
    pattern = re.compile(r'"%s":\s*\{[^{}]*\}' % re.escape(key), re.S)
    if pattern.search(text):
        text = pattern.sub(lambda m: '"%s": %s' % (key, body), text, count=1)
    elif re.search(r'\r?\n\s*"always_on_tokens":', text):
        text = re.sub(r'(\r?\n\s*"always_on_tokens":)', lambda m: '%s  "%s": %s,%s' % (nl, key, body, m.group(1)), text, count=1)
    else:
        # no anchor key: insert as the first member of the top-level object
        text = re.sub(r'^\s*\{', lambda m: m.group(0) + '%s  "%s": %s,' % (nl, key, body), text, count=1)
    with open(path, "w", encoding="utf-8", newline="") as fh:
        fh.write(text)


def agents_dirs(explicit=None):
    if explicit:
        return [explicit]
    found = []
    for rel in (os.path.join("pack", "adapters", "claude-code", "agents"),
                os.path.join("pack", "adapters", "copilot", "agents"),
                os.path.join(".claude", "agents")):
        path = find_dir(rel)
        if path:
            found.append(path)
    # Prefer the pack sources when both are present; they are the source of truth.
    pack_sources = [p for p in found if os.sep + "pack" + os.sep in p]
    return pack_sources or found


def read_frontmatter(path):
    """Return (meta_dict, body). meta values are raw strings; lists are parsed for [a, b]."""
    with open(path, encoding="utf-8", errors="replace", newline="") as fh:
        raw = fh.read()
    match = re.match(r"^---\r?\n(.*?)\r?\n---\r?\n", raw, re.S)
    if not match:
        return {}, raw
    meta = {}
    for line in match.group(1).splitlines():
        kv = re.match(r"^([A-Za-z_][\w-]*):\s*(.*)$", line)
        if not kv:
            continue
        key, val = kv.group(1), kv.group(2).strip()
        if val.startswith("[") and val.endswith("]"):
            meta[key] = [v.strip() for v in val[1:-1].split(",") if v.strip()]
        else:
            meta[key] = val.strip('"').strip("'")
    return meta, raw[match.end():]


class EmptyCorpus(Exception):
    """The scanned directory held no knowledge docs.

    PACK-P: a check that reports a verdict over a corpus it never established was non-empty
    is worse than no check, because it reports success. An empty scan is always a resolution
    bug -- there is no legitimate pack with zero knowledge docs -- so it is raised, never
    quietly counted as zero.
    """


def scan(kdir):
    """Every knowledge doc with its declared scope and estimated size. Sorted, deterministic."""
    docs = []
    for name in sorted(os.listdir(kdir)):
        if not name.endswith(".md"):
            continue
        path = os.path.join(kdir, name)
        meta, _ = read_frontmatter(path)
        chars = os.path.getsize(path)
        if name == MANIFEST:
            load = "always"
        else:
            load = meta.get("load", "")
        docs.append({
            "name": name[:-3], "path": path, "chars": chars,
            "tokens": est_tokens(chars), "load": load,
            "applyTo": meta.get("applyTo", ""), "skills": meta.get("skills", []),
        })
    if not docs:
        raise EmptyCorpus(f"no knowledge docs found in {kdir}")
    return docs


def always_on(docs):
    return [d for d in docs if d["load"] == "always"]


# --------------------------------------------------------------------------- report

def cmd_report(args):
    kdir = knowledge_dir(args.knowledge_dir)
    if not kdir:
        print("context-budget: no knowledge directory found", file=sys.stderr)
        return 1
    docs = scan(kdir)
    undeclared = [d for d in docs if d["load"] not in TIERS]
    by_tier = {}
    for doc in docs:
        by_tier.setdefault(doc["load"], []).append(doc)

    print(f"knowledge dir: {kdir}")
    print(f"docs: {len(docs)}   (token figures are ESTIMATES at {CHARS_PER_TOKEN} chars/token)\n")
    for load in ("always", "glob", "skill", "reference"):
        group = sorted(by_tier.get(load, []), key=lambda d: -d["tokens"])
        if not group:
            continue
        total = sum(d["tokens"] for d in group)
        print(f"Tier {TIERS[load]} — load: {load:<9} {len(group):2d} docs  ~{total:>7,d} tok"
              f"   ({TIER_NOTE[load]})")
        if args.verbose:
            for doc in group:
                extra = doc["applyTo"] or (", ".join(doc["skills"]) if doc["skills"] else "")
                print(f"      ~{doc['tokens']:>6,d}  {doc['name']}"
                      + (f"   [{extra}]" if extra else ""))
        print()

    total_always = sum(d["tokens"] for d in always_on(docs))
    total_all = sum(d["tokens"] for d in docs)
    print(f"ALWAYS-ON (the static prefix): ~{total_always:,} est. tokens"
          f"  of ~{total_all:,} across the whole set")
    if total_all:
        print(f"                               {100.0 * total_always / total_all:.0f}% of the corpus is attached to every request")
    if undeclared:
        print(f"\nUNDECLARED load scope ({len(undeclared)}): " + ", ".join(d["name"] for d in undeclared))
        return 1
    return 0


# ----------------------------------------------------------------------------- gate

def cmd_gate(args):
    """Fail on UNACKNOWLEDGED GROWTH first, and on the derived backstop second.

    The ratchet is the real control. PACK-R is silent accumulation, so the question that
    matters is "did this change grow the always-on set without saying so?", not "is the
    number above X". An absolute ceiling answers the second question, stays quiet through
    the whole accumulation, and then red-lights an ordinary paragraph -- which trains people
    to raise the ceiling reflexively, the exact habit the gate exists to break.
    """
    kdir = knowledge_dir(args.knowledge_dir)
    if not kdir:
        print("context-budget: no knowledge directory found", file=sys.stderr)
        return 1
    cfg, cfgpath = load_config(args.config)
    docs = scan(kdir)
    undeclared = [d for d in docs if d["load"] not in TIERS]
    always = always_on(docs)
    total = sum(d["tokens"] for d in always)

    baseline = cfg.get("always_on_tokens")
    ceiling = args.ceiling if args.ceiling is not None else cfg.get("ceiling_tokens")
    tol_pct = cfg.get("growth_tolerance_pct") or 0
    allowed = int(baseline * (1 + tol_pct / 100.0)) if baseline else None

    print(f"always-on knowledge: ~{total:,} est. tokens across {len(always)} docs")
    if baseline:
        delta = total - baseline
        sign = "+" if delta >= 0 else ""
        print(f"  baseline           ~{baseline:,}  ({sign}{delta:,}, tolerance {tol_pct}% "
              f"= {allowed:,})")
    else:
        print("  baseline            not recorded — ratchet inactive, backstop only")
    print(f"  backstop            {ceiling:,}")

    if args.update_baseline:
        if not cfgpath:
            print("FAIL: --update-baseline needs a config file; none found.")
            return 1
        write_baseline(cfgpath, total)
        print(f"\nbaseline updated to ~{total:,} in {cfgpath}")
        print("Commit it with the change that caused the growth — that diff IS the control.")
        return 0

    failed = False
    if undeclared:
        print(f"\nFAIL: {len(undeclared)} doc(s) declare no `load:` scope — "
              + ", ".join(d["name"] for d in undeclared))
        print("      An undeclared doc is an unbudgeted doc. Add `load:` frontmatter.")
        failed = True

    if allowed is not None and total > allowed:
        print(f"\nFAIL: the always-on set grew ~{total - baseline:,} tokens past the recorded "
              f"baseline.")
        print("      Growing it is allowed. Growing it SILENTLY is not — every always-on doc")
        print("      is re-read on every call, and this is the only place that shows up.")
        print("      If the growth is intended, record it in the same commit:")
        print("        python context-budget.py gate --update-baseline")
        print("      If it is not, move a doc to `load: glob` / `skill` / `reference`.")
        for doc in sorted(always, key=lambda d: -d["tokens"])[:5]:
            print(f"        ~{doc['tokens']:>6,d}  {doc['name']}")
        failed = True

    if ceiling and total > ceiling:
        print(f"\nFAIL: past the derived backstop by ~{total - ceiling:,} tokens.")
        deriv = cfg.get("ceiling_derivation") or {}
        if deriv:
            print(f"      The backstop is where the always-on set stops fitting the smallest")
            print(f"      model tier the roster delegates to: window "
                  f"{deriv.get('smallest_supported_window', 0):,} - tools "
                  f"{deriv.get('tool_definition_tokens', 0):,} - headroom "
                  f"{deriv.get('required_working_headroom', 0):,}.")
            print("      Raising this is a decision about which models can still be used,")
            print("      not a formatting preference. Change the derivation inputs.")
        failed = True

    if failed:
        return 1

    # A ratchet that only travels one way is a ceiling in disguise. Say so when the set has
    # shrunk enough that the baseline is now recording history rather than intent.
    if baseline:
        shrink_pct = cfg.get("shrink_report_pct") or 0
        if shrink_pct and total < baseline * (1 - shrink_pct / 100.0):
            print(f"\nNOTE: the set has shrunk ~{baseline - total:,} tokens below the baseline.")
            print("      Ratchet it down (`gate --update-baseline`) so the budget keeps")
            print("      measuring intent rather than a high-water mark.")
    print(f"\nclean - no unacknowledged growth"
          + (f"; ~{ceiling - total:,} to the backstop" if ceiling else ""))
    return 0


# --------------------------------------------------------------------------- agents

def cmd_agents(args):
    """Per-agent declared knowledge prefix (P3). An agent inherits its LENS, not the world."""
    kdir = knowledge_dir(args.knowledge_dir)
    adirs = agents_dirs(args.agents_dir)
    if not kdir or not adirs:
        print("context-budget: knowledge or agents directory not found", file=sys.stderr)
        return 1
    sizes = {d["name"]: d["tokens"] for d in scan(kdir)}
    base = sum(d["tokens"] for d in always_on(scan(kdir)))

    rows, undeclared, unknown_refs = [], [], []
    for adir in adirs:
        for name in sorted(os.listdir(adir)):
            if not name.endswith(".md"):
                continue
            meta, _ = read_frontmatter(os.path.join(adir, name))
            agent = meta.get("name") or name[:-3]
            if "knowledge" not in meta:
                undeclared.append(agent)
                continue
            docs = meta["knowledge"] if isinstance(meta["knowledge"], list) else []
            missing = [d for d in docs if d not in sizes]
            unknown_refs.extend(f"{agent} -> {d}" for d in missing)
            rows.append((agent, docs, sum(sizes.get(d, 0) for d in docs)))

    rows.sort(key=lambda r: -r[2])
    print(f"per-agent knowledge prefix (ESTIMATES; the main thread's always-on set is ~{base:,})\n")
    for agent, docs, total in rows:
        print(f"  ~{total:>6,d} tok  {agent:<32} {len(docs)} doc(s)")
        if args.verbose:
            for doc in docs:
                print(f"                    - {doc}  (~{sizes.get(doc, 0):,})")
    if rows:
        worst = max(r[2] for r in rows)
        print(f"\n  widest lens: ~{worst:,} est. tokens"
              f"   ({100.0 * worst / base:.0f}% of the main thread's always-on set)" if base else "")
    failed = False
    if unknown_refs:
        print("\nFAIL: agent references a knowledge doc that does not exist:")
        for ref in unknown_refs:
            print(f"        {ref}")
        failed = True
    if undeclared:
        print(f"\nFAIL: {len(undeclared)} agent(s) declare no `knowledge:` lens — "
              + ", ".join(sorted(undeclared)))
        print("      An agent with no declared lens inherits the whole set, which is")
        print("      what put a main-thread-sized prefix on every delegated run.")
        failed = True
    return 1 if failed else 0


# ------------------------------------------------------------------------ preflight

def cmd_preflight(args):
    """Fail BEFORE a fan-out when the assembled prefix cannot fit the target window (P5).

    One failure at the context ceiling predicts every sibling in the wave: the prefix is
    the same for all of them. Probing it once costs a subsecond; discovering it per-run
    cost 27 of 39 delegated runs in the profiled session.
    """
    kdir = knowledge_dir(args.knowledge_dir)
    if not kdir:
        print("context-budget: no knowledge directory found", file=sys.stderr)
        return 1
    docs = scan(kdir)
    sizes = {d["name"]: d["tokens"] for d in docs}

    if args.agent:
        adirs = agents_dirs(args.agents_dir)
        lens, found = None, False
        for adir in adirs or []:
            for name in sorted(os.listdir(adir)):
                if not name.endswith(".md"):
                    continue
                meta, _ = read_frontmatter(os.path.join(adir, name))
                if (meta.get("name") or name[:-3]) == args.agent:
                    lens, found = meta.get("knowledge", []), True
                    break
            if found:
                break
        if not found:
            print(f"preflight: agent '{args.agent}' not found", file=sys.stderr)
            return 1
        if not isinstance(lens, list):
            lens = []
        knowledge = sum(sizes.get(d, 0) for d in lens)
        scope = f"agent '{args.agent}' ({len(lens)} doc lens)"
    else:
        knowledge = sum(d["tokens"] for d in always_on(docs))
        scope = "main thread (always-on set)"

    prefix = knowledge + args.tools + args.overhead
    headroom = args.window - prefix
    print(f"preflight: {scope}")
    print(f"  knowledge   ~{knowledge:>8,d} est. tokens")
    print(f"  tools        {args.tools:>8,d}")
    print(f"  overhead     {args.overhead:>8,d}")
    print(f"  prefix      ~{prefix:>8,d}  of a {args.window:,} window"
          f"  ({100.0 * prefix / args.window:.0f}%)")
    print(f"  headroom    ~{headroom:>8,d}")

    if headroom < args.min_headroom:
        print(f"\nFAIL: less than the required {args.min_headroom:,} tokens of working headroom.")
        print("      Do NOT dispatch this wave — every run in it carries the same prefix,")
        print("      so one failure here is all of them. Narrow the lens (`knowledge:` in")
        print("      the agent), pick a model with a larger window, or scope a doc out of")
        print("      the always-on tier.")
        return 1
    print("\nclean - the wave fits")
    return 0


# ---------------------------------------------------------------------------- prefix

IMPORT_RX = re.compile(r"^\s*@AGENTS\.md\s*$", re.M)


def _file_chars(path):
    try:
        return os.path.getsize(path)
    except OSError:
        return 0


def prefix_components(root, kdir, cfg, host):
    """Every component of the static prefix this tool can see, per host, with a label each.
    'measured' = a file on disk; 'allowance' = a stated figure from config (Inferred)."""
    comps = []
    agents = os.path.join(root, "AGENTS.md")
    claude = os.path.join(root, "CLAUDE.md")
    a_chars, c_chars = _file_chars(agents), _file_chars(claude)
    c_text = ""
    if c_chars:
        with open(claude, encoding="utf-8", errors="replace") as fh:
            c_text = fh.read()
    claude_is_import = bool(IMPORT_RX.search(c_text))
    if host == "copilot":
        # Copilot CLI loads AGENTS.md AND CLAUDE.md as custom instructions (measured: two
        # near-identical <custom_instruction> blocks in a captured prefix, CTX-B).
        if a_chars:
            comps.append(("AGENTS.md", est_tokens(a_chars), "measured"))
        if c_chars:
            comps.append(("CLAUDE.md" + (" (import stub)" if claude_is_import else " (FULL COPY - double-load)"),
                          est_tokens(c_chars), "measured"))
    else:
        # Claude Code loads CLAUDE.md; an @AGENTS.md line expands AGENTS.md in place.
        if c_chars:
            comps.append(("CLAUDE.md", est_tokens(c_chars), "measured"))
            if claude_is_import and a_chars:
                comps.append(("AGENTS.md (via @import)", est_tokens(a_chars), "measured"))
        elif a_chars:
            comps.append(("AGENTS.md (not loaded by Claude Code - no CLAUDE.md import)", 0, "measured"))
    if kdir:
        docs = scan(kdir)
        comps.append(("always-on knowledge docs ({0})".format(len(always_on(docs))),
                      sum(d["tokens"] for d in always_on(docs)), "measured"))
    allow = cfg.get("prefix_allowances") or {}
    for name, key in (("tool definitions", "tool_definitions"), ("host system prompt", "host_prompt")):
        if allow.get(key):
            comps.append((name + " (allowance)", int(allow[key]), "allowance"))
    return comps, {"claude_is_import": claude_is_import, "agents_chars": a_chars, "claude_chars": c_chars}


def cmd_prefix(args):
    root = repo_root(args.root)
    kdir = knowledge_dir(args.knowledge_dir)
    if not root:
        print("context-budget: no repo root (AGENTS.md / CLAUDE.md / .git) found", file=sys.stderr)
        return 1
    cfg, cfgpath = load_config(args.config)
    hosts = [args.host] if args.host != "both" else ["copilot", "claude"]
    worst = 0
    failed = False
    for host in hosts:
        comps, facts = prefix_components(root, kdir, cfg, host)
        total = sum(t for _, t, _ in comps)
        worst = max(worst, total)
        print("static prefix as {0} assembles it   (ESTIMATES at {1} chars/token; allowances are Inferred)".format(host, CHARS_PER_TOKEN))
        for name, tokens, kind in comps:
            print("  ~{0:>8,d}  {1}{2}".format(tokens, name, "" if kind == "measured" else "   [Inferred]"))
        print("  ~{0:>8,d}  TOTAL\n".format(total))
        if host == "copilot" and facts["claude_chars"] and not facts["claude_is_import"] and facts["agents_chars"]:
            print("  NOTE: CLAUDE.md is a full copy beside AGENTS.md; Copilot CLI loads both, so ~{0:,} est. tokens are paid twice on every request. Make CLAUDE.md an `@AGENTS.md` import (INSTALL 1.1).\n".format(est_tokens(facts["claude_chars"])))
            if args.gate:
                failed = True
    baseline = cfg.get("prefix_tokens")
    tol = cfg.get("growth_tolerance_pct") or 0
    if args.update_baseline:
        if not cfgpath:
            print("FAIL: --update-baseline needs a config file; none found.")
            return 1
        write_baseline(cfgpath, worst, key="prefix_tokens", stamp="prefix_set_on")
        print("prefix baseline updated to ~{0:,} in {1}".format(worst, cfgpath))
        return 0
    if args.gate:
        if baseline:
            allowed = int(baseline * (1 + tol / 100.0))
            print("prefix ratchet: ~{0:,} vs baseline ~{1:,} (tolerance {2}% = {3:,})".format(worst, baseline, tol, allowed))
            if worst > allowed:
                print("FAIL: the static prefix grew ~{0:,} tokens past its recorded baseline. Growing it is allowed; growing it silently is not: `context-budget.py prefix --update-baseline` in the same commit, or move a doc out of the always-on tier.".format(worst - baseline))
                failed = True
        else:
            print("prefix ratchet: no baseline recorded (`prefix --update-baseline` to start it)")
        if failed:
            return 1
        print("clean - prefix within its baseline")
    return 0


# ---------------------------------------------------------------------------- skills

def skills_dir(explicit=None):
    if explicit:
        return explicit
    return find_dir(os.path.join("pack", "commands"), os.path.join(".claude", "skills"),
                    predicate=lambda p: any(os.path.isfile(os.path.join(p, d, "SKILL.md")) for d in os.listdir(p)))


def scan_skills(sdir):
    out = []
    for name in sorted(os.listdir(sdir)):
        path = os.path.join(sdir, name, "SKILL.md")
        if os.path.isfile(path):
            chars = os.path.getsize(path)
            refs = [f for f in os.listdir(os.path.join(sdir, name)) if f != "SKILL.md"]
            out.append({"name": name, "chars": chars, "tokens": est_tokens(chars), "reference_files": refs})
    if not out:
        raise EmptyCorpus("no skills found in {0}".format(sdir))
    return out


def cmd_skills(args):
    sdir = skills_dir(args.skills_dir)
    if not sdir:
        print("context-budget: no skills directory found", file=sys.stderr)
        return 1
    cfg, cfgpath = load_config(args.config)
    skills = scan_skills(sdir)
    baseline = cfg.get("skills_baseline") or {}
    ceiling = args.ceiling if args.ceiling is not None else cfg.get("skill_ceiling_tokens")
    tol = cfg.get("growth_tolerance_pct") or 0
    print("skills dir: {0}   ({1} skills; ESTIMATES at {2} chars/token; each SKILL.md is re-injected whole on every invocation)".format(sdir, len(skills), CHARS_PER_TOKEN))
    failed = False
    for sk in sorted(skills, key=lambda d: -d["tokens"]):
        base = baseline.get(sk["name"])
        flag = ""
        if base is not None and sk["tokens"] > int(base * (1 + tol / 100.0)):
            flag = "  GREW ~{0:,} past baseline ~{1:,}".format(sk["tokens"] - base, base)
            failed = True
        elif base is None and ceiling and sk["tokens"] > ceiling:
            flag = "  NEW and above the {0:,} ceiling".format(ceiling)
            failed = True
        print("  ~{0:>6,d}  {1:<22}{2}{3}".format(sk["tokens"], sk["name"],
                                              (" (+{0} reference file(s))".format(len(sk["reference_files"])) if sk["reference_files"] else ""), flag))
    if args.update_baseline:
        if not cfgpath:
            print("FAIL: --update-baseline needs a config file; none found.")
            return 1
        write_json_key(cfgpath, "skills_baseline", {sk["name"]: sk["tokens"] for sk in skills})
        print("\nskills baseline updated in {0}".format(cfgpath))
        return 0
    if args.gate:
        if failed:
            print("\nFAIL: a SKILL.md grew past its recorded baseline (or a new one is above the ceiling). Move stage detail into a reference file the skill reads on demand, or record the growth: `context-budget.py skills --update-baseline`.")
            return 1
        print("\nclean - no unacknowledged skill growth")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(
        prog="context-budget.py",
        description="Measure, gate and preflight the always-on context budget.")
    parser.add_argument("--knowledge-dir", help="override knowledge doc discovery")
    parser.add_argument("--agents-dir", help="override agent definition discovery")
    parser.add_argument("--config", help="override context-budget.json discovery")
    parser.add_argument("--root", help="override repo-root discovery (AGENTS.md / CLAUDE.md)")
    parser.add_argument("--skills-dir", help="override skills discovery (pack/commands or .claude/skills)")
    sub = parser.add_subparsers(dest="cmd")

    p_rep = sub.add_parser("report", help="tier table + always-on total")
    p_rep.add_argument("-v", "--verbose", action="store_true", help="list every doc")
    p_rep.set_defaults(func=cmd_report)

    p_gate = sub.add_parser("gate", help="fail on unacknowledged always-on growth (CI-able)")
    p_gate.add_argument("--ceiling", type=int, default=None,
                        help="override the derived backstop from context-budget.json")
    p_gate.add_argument("--update-baseline", action="store_true",
                        help="record the current total as the new baseline; commit the diff "
                             "alongside the change that caused the growth")
    p_gate.set_defaults(func=cmd_gate)

    p_ag = sub.add_parser("agents", help="per-agent declared knowledge prefix")
    p_ag.add_argument("-v", "--verbose", action="store_true", help="list each agent's docs")
    p_ag.set_defaults(func=cmd_agents)

    p_pre = sub.add_parser("preflight", help="fail before a fan-out that cannot fit")
    p_pre.add_argument("--window", type=int, required=True, help="target model context window")
    p_pre.add_argument("--agent", help="preflight one agent's lens instead of the main thread")
    p_pre.add_argument("--tools", type=int, default=24070,
                       help="tool-definition tokens (default 24070, the profiled figure)")
    p_pre.add_argument("--overhead", type=int, default=0, help="any further fixed prefix")
    p_pre.add_argument("--min-headroom", type=int, default=32000,
                       help="working headroom the task itself needs (default 32000)")
    p_pre.set_defaults(func=cmd_preflight)

    p_px = sub.add_parser("prefix", help="the whole static prefix per host, with its ratchet")
    p_px.add_argument("--host", choices=["copilot", "claude", "both"], default="both")
    p_px.add_argument("--gate", action="store_true", help="fail on unacknowledged prefix growth or a double-loaded CLAUDE.md")
    p_px.add_argument("--update-baseline", action="store_true")
    p_px.set_defaults(func=cmd_prefix)

    p_sk = sub.add_parser("skills", help="per-skill SKILL.md size with a per-skill ratchet")
    p_sk.add_argument("--gate", action="store_true")
    p_sk.add_argument("--ceiling", type=int, default=None, help="override skill_ceiling_tokens")
    p_sk.add_argument("--update-baseline", action="store_true")
    p_sk.set_defaults(func=cmd_skills)

    args = parser.parse_args(argv)
    if not getattr(args, "func", None):
        parser.print_help()
        return 0
    try:
        return args.func(args)
    except EmptyCorpus as exc:
        # Never degrade to a clean report: an empty corpus means discovery failed, and a
        # green gate over nothing is the failure this guard exists to prevent (PACK-P).
        print(f"FAIL: {exc}", file=sys.stderr)
        print("      Pass --knowledge-dir explicitly, or run from a repo that has one.",
              file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
