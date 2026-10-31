#!/usr/bin/env python3
"""verify-skill-contracts.py - every skill declares its seat and cites the shared stage its shape needs.

THE CLASS (coordination proposal §7b.5, D15; spec-compile-readers US-4..US-8). The pack has a
doctrine of delegation - three shared stages in knowledge/agent-coordination.md (CO-S0 compile,
CO-S1 seat, CO-S2 stop = message) - and, measured on 2026-09-19, 25 of 28 skills declared no
seat, 11 of 14 prose-input skills did not cite CO-S0, and none of the four hard stops cited
CO-S2. Doctrine no skill cites is prose, and prose is a memoir (CI6). This lint is the control.

WHAT IT CHECKS. Every `<skills-root>/<name>/SKILL.md` (plus that skill's `reference/*.md`):
  1. seat            frontmatter carries `runs_as: Coordinator|Sub-Agent|either` (CO-S1)
  2. fan-out         a skill naming a fan-out cap above zero (`fan-out cap 2`, `: 3`, `of 4`,
                     `<= 4`) cites CO-S0 and the five-part contract with a termination condition
  3. hard stop       a hard-stop stage (`**STOP`, `stop for human`, `never merges`) cites CO-S2
  4. dispatch order  a dispatcher (a Dispatch heading or stage label, `coord dispatch`, or a
                     sentence-initial `spawn`) cites CO-S0 in SKILL.md before that instruction
  5. compile         every skill cites CO-S0 in SKILL.md - inline, or the one-line pointer to
                     `reference/co-s0.md` (spec-skill-evolution US-1; measured 10 of 28 without)
  6. pointer         a SKILL.md that names `reference/co-s0.md` has a reference text carrying CO-S0
  7. deadline        a dispatcher, or a skill naming a fan-out cap above zero, names a deadline
                     and a fallback for the dispatch (CO8, CO9; measured: execute-with-coordination
                     said "fallback" once and "deadline" nowhere)

Refusals, one per line on stdout, in the pack's grammar:  <code>: <skill> — fix: <text>
Codes are stable (O7): seat missing · seat invalid · fan-out without compile · fan-out without
contract · hard stop without message · dispatch before compile · compile missing · pointer without
reference · dispatch without deadline · skills root missing.

USAGE
  python3 verify-skill-contracts.py                 check pack/commands (or .claude/skills)
  python3 verify-skill-contracts.py --root <repo>   check that repository's skills
  python3 verify-skill-contracts.py --self-test     prove every direction can fail (DC-104)

EXIT  0 clean  ·  1 refusals  ·  2 usage (no skills root)
"""
from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile

# PLAT-A: a cp1252 console cannot encode the em-dash in the refusal grammar.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

SKILL_ROOTS = ("pack/commands", ".claude/skills")
SEATS = ("Coordinator", "Sub-Agent", "either")
FRONTMATTER = re.compile(r"^---\r?\n(.*?)\r?\n---\r?\n", re.S)
RUNS_AS = re.compile(r"^runs_as:[ \t]*(\S+)[ \t]*$", re.M)
FAN_OUT_ABOVE_ZERO = re.compile(r"fan-out cap(?: of|:|\s*(?:≤|<=|<))?\s*([1-9]\d*)", re.I)
HARD_STOP = re.compile(r"(\*\*STOP\b|(?i:stop for human)|(?i:never merges))")
# An INSTRUCTION to dispatch, not prose about it: a Dispatch heading or stage label, the
# `coord dispatch` verb, or a sentence-initial "spawn". ("which spawns one sub-agent per
# track" in an intro paragraph is a description; measured on prepare-for-coordination.)
DISPATCH = re.compile(r"(?m)^#+ .*\bDispatch\b|\*\*Stage \d+ — Dispatch|coord dispatch|(?:^|[.:;]\s+)[Ss]pawn\b")
POINTER = "reference/co-s0.md"
DEADLINE = re.compile(r"deadline", re.I)
FALLBACK = re.compile(r"fallback", re.I)


def _read(path):
    with open(path, "r", encoding="utf-8") as fh:
        return fh.read()


def _split(text):
    """(frontmatter, body). A file with no fence is all body (the seat rule then refuses it)."""
    m = FRONTMATTER.match(text)
    return (m.group(1), text[m.end():]) if m else ("", text)


def check_skill(name, skill_md, reference_texts):
    """Returns the refusal lines for one skill. Pure over the texts (the self-test and the tests
    call it through the CLI; the CLI is the only place that touches the filesystem)."""
    out = []
    fm, body = _split(skill_md)
    union = body + "\n" + "\n".join(reference_texts)
    m = RUNS_AS.search(fm)
    if not m:
        out.append("seat missing: {0} — fix: add `runs_as: Coordinator|Sub-Agent|either` to the frontmatter (CO-S1)".format(name))
    elif m.group(1) not in SEATS:
        out.append("seat invalid: {0} — fix: runs_as is {1!r}; use one of Coordinator|Sub-Agent|either (CO-S1)".format(name, m.group(1)))
    if FAN_OUT_ABOVE_ZERO.search(union):
        if "CO-S0" not in union:
            out.append("fan-out without compile: {0} — fix: a skill that fans out cites CO-S0 (knowledge/agent-coordination.md) before it plans".format(name))
        if "five-part contract" not in union or "termination" not in union:
            out.append("fan-out without contract: {0} — fix: name the five-part contract with a termination condition for every fan-out (GO7)".format(name))
    if HARD_STOP.search(body) and "CO-S2" not in union:
        out.append("hard stop without message: {0} — fix: cite CO-S2 (knowledge/agent-coordination.md) at the stop — the stop is a message, not a paragraph".format(name))
    d = DISPATCH.search(body)
    if d:
        c = body.find("CO-S0")
        if c < 0 or c > d.start():
            out.append("dispatch before compile: {0} — fix: cite CO-S0 in SKILL.md before the first dispatch/spawn instruction (a dispatcher never spawns before the compile stage)".format(name))
    if "CO-S0" not in body:
        out.append("compile missing: {0} — fix: cite CO-S0 in SKILL.md — inline in Grounding, or the one-line pointer to reference/co-s0.md (CO-S0, knowledge/agent-coordination.md)".format(name))
    if POINTER in body and not any("CO-S0" in ref for ref in reference_texts):
        out.append("pointer without reference: {0} — fix: SKILL.md points at reference/co-s0.md but no reference/*.md carries CO-S0 — add the file with the CO-S0 sentence".format(name))
    if (d or FAN_OUT_ABOVE_ZERO.search(union)) and not (DEADLINE.search(union) and FALLBACK.search(union)):
        out.append("dispatch without deadline: {0} — fix: a skill that dispatches names a deadline and a fallback for every dispatch (CO8, CO9)".format(name))
    return out


def find_skills_root(root):
    for rel in SKILL_ROOTS:
        p = os.path.join(root, *rel.split("/"))
        if os.path.isdir(p):
            return p
    return None


def default_root():
    here = os.path.dirname(os.path.abspath(__file__))
    for start in (here, os.getcwd()):
        d = start
        while True:
            if find_skills_root(d):
                return d
            parent = os.path.dirname(d)
            if parent == d:
                break
            d = parent
    return os.getcwd()


def scan(skills_root):
    refusals, checked = [], 0
    for name in sorted(os.listdir(skills_root)):
        d = os.path.join(skills_root, name)
        if not os.path.isdir(d):
            continue
        skill_md = os.path.join(d, "SKILL.md")
        if not os.path.isfile(skill_md):
            print("skip: {0} — no SKILL.md".format(name))
            continue
        refs = []
        ref_dir = os.path.join(d, "reference")
        if os.path.isdir(ref_dir):
            for fn in sorted(os.listdir(ref_dir)):
                if fn.endswith(".md"):
                    refs.append(_read(os.path.join(ref_dir, fn)))
        refusals += check_skill(name, _read(skill_md), refs)
        checked += 1
    return refusals, checked


def self_test():
    """Every direction must FAIL on its fixture and the good skill must pass (DC-104)."""
    seat = "runs_as: either\n"
    fixtures = [
        ("seat missing", "---\nname: a\n---\n## Flow\nplain.\n", []),
        ("seat invalid", "---\nname: a\nruns_as: Owner\n---\n## Flow\nplain.\n", []),
        ("fan-out without compile", "---\nname: a\n" + seat + "---\n## Flow\nfan-out cap 2.\n", []),
        ("fan-out without contract", "---\nname: a\n" + seat + "---\n## Flow\nCO-S0; fan-out cap of 3.\n", []),
        ("hard stop without message", "---\nname: a\n" + seat + "---\n## Flow\n**STOP for human triage**.\n", []),
        ("hard stop without message", "---\nname: a\n" + seat + "---\n## Flow\nit **never merges**.\n", []),
        ("dispatch before compile", "---\nname: a\n" + seat + "---\n## Stage 3 — Dispatch\nspawn.\n## Later\nCO-S0.\n", []),
        ("dispatch before compile", "---\nname: a\n" + seat + "---\n## Flow\nspawn the tracks.\n", []),
        ("compile missing", "---\nname: a\n" + seat + "---\n## Grounding\nplain, no stage cited.\n", []),
        ("pointer without reference", "---\nname: a\n" + seat + "---\n## Grounding\nCO-S0 applies first — the sentence is `reference/co-s0.md`.\n", ["stage detail without the citation"]),
        ("dispatch without deadline", "---\nname: a\nruns_as: Coordinator\n---\n## Stage 0\nCO-S0.\n## Stage 3 — Dispatch\nspawn under the five-part contract with a termination condition; fan-out cap: 4.\n", []),
    ]
    for code, text, refs in fixtures:
        got = check_skill("a", text, refs)
        if not any(line.startswith(code + ": a — fix: ") for line in got):
            print("self-test FAILED: expected `{0}` on {1!r}; got {2}".format(code, text, got))
            return 1
    ok = [
        "---\nname: a\n" + seat + "---\n## Grounding\nCO-S0 first (five-part contract, termination, deadline, fallback). fan-out cap 2; **STOP for human review** (CO-S2).\n",
        "---\nname: a\nruns_as: Coordinator\n---\n## Stage 0\nCO-S0.\n## Stage 3 — Dispatch\nspawn under the five-part contract with a termination condition, a deadline and a fallback; fan-out cap: 4.\n",
        "---\nname: a\n" + seat + "---\n## Flow\nCO-S0. fan-out cap 0 → 2 is a raise; tier · fan-out cap · budget.\n",
    ]
    for text in ok:
        got = check_skill("a", text, [])
        if got:
            print("self-test FAILED: the good skill was refused: {0}".format(got))
            return 1
    # the reference route: the citation may live in reference/*.md
    got = check_skill("a", "---\nname: a\n" + seat + "---\n## Flow\nCO-S0. **STOP for human approval** (`reference/stop.md`).\n", ["CO-S2 applies."])
    if got:
        print("self-test FAILED: a citation in reference/ was not honoured: {0}".format(got))
        return 1
    # the pointer route: SKILL.md points, reference/co-s0.md carries the sentence
    got = check_skill("a", "---\nname: a\n" + seat + "---\n## Grounding\nCO-S0 applies first — the sentence is `reference/co-s0.md`.\n", ["Consume the compiled prompt when one is in hand (CO-S0)."])
    if got:
        print("self-test FAILED: the pointer route with its reference was refused: {0}".format(got))
        return 1
    # the CLI end: a temp tree with no skills root exits 2, one with a bad skill exits 1
    tmp = tempfile.mkdtemp()
    if main(["--root", tmp]) != 2:
        print("self-test FAILED: a root with no skills directory must exit 2")
        return 1
    d = os.path.join(tmp, "pack", "commands", "x")
    os.makedirs(d)
    with open(os.path.join(d, "SKILL.md"), "w", encoding="utf-8", newline="\n") as fh:
        fh.write("---\nname: x\n---\n## Flow\nplain.\n")
    if main(["--root", tmp]) != 1:
        print("self-test FAILED: a seat-less skill must exit 1 through the CLI")
        return 1
    print("self-test ok: 11 refusal directions fail, 3 good skills, the reference route and the pointer route pass, exit codes 1 and 2 observed")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(prog="verify-skill-contracts.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--root", help="repository root (default: found by walking up from this script or the cwd)")
    ap.add_argument("--self-test", action="store_true", help="prove the gate can fail")
    args = ap.parse_args(argv)
    if args.self_test:
        return self_test()
    root = os.path.abspath(args.root) if args.root else default_root()
    skills_root = find_skills_root(root)
    if not skills_root:
        print("skills root missing: {0} — fix: run from a repo with pack/commands or .claude/skills, or pass --root".format(root))
        return 2
    refusals, checked = scan(skills_root)
    for line in refusals:
        print(line)
    if refusals:
        print("\n{0} refusal(s) across {1} skill(s) in {2}".format(len(refusals), checked, skills_root))
        return 1
    print("clean - {0} skill(s) checked in {1}".format(checked, skills_root))
    return 0


if __name__ == "__main__":
    sys.exit(main())
