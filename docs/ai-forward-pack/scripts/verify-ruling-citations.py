#!/usr/bin/env python3
"""verify-ruling-citations.py - every ruling cited as authority resolves to exactly one heading that says what it decided.

THE CLASS (absorbed from ai-de's tools/verify-ruling-citations.py, measured 2026-09-11; class ID-A).
Programme decisions were cited by number across that repository and enforced as binding. Eight of
them defined nothing: numbers cited with no note anywhere recording what they said, one of them
cited six times as governing a dependency decision. Two ends of that met on the same day: a number
was cited by a mockup, a review and a mid-task correction BEFORE any ruling of that number had been
made, and the Owner, unable to see it because nothing recorded it, allocated the same number again
for a different decision. THE ABSENCE OF THE REGISTER IS WHAT CAUSED THE COLLISION IN THE REGISTER.
A decision that cannot be read is not a decision; it is a number with a reputation.

WHAT COUNTS. A DEFINITION is a heading `### Ruling NN — …` (or `## Ruling NN — …`) in
docs/notes/rulings.md - the only definition site; `coord decide rule` writes it. A CITATION is
`Ruling NN` (or `Rulings NN`) in PROSE - `.md`, `.html`, `.txt` - anywhere under docs/, pack/,
.agents/log/, .github/, .claude/. A mention inside prose is a citation, never a definition: that
distinction is the whole point, and collapsing it would make the gate agree with any file that talks
about a ruling often enough. Records (`.json`, `.jsonl`) are not scanned: the audit log and the
dreams quote other repositories' prose verbatim, and a quote is not a citation (decision note
note-20260919-owner-review-register-and-scan-scope). docs/ai-forward-pack/ is skipped as the
generated copy of pack/.

THE SHORT FORM. A register may number its rulings `## R-n · date · seat · title` (measured in
x-harness-x-model-bench, 2026-09-24): there the gate read no heading and no citation, and passed
while checking nothing. So `## R-n` / `### R-n` also define number n, and `R-n` in prose (e.g.
"R-7", "ruling R-4") also cites it: `R-n` and `Ruling n` name one number. The short form is
strict because short ids are common: `R-` must not follow a letter, digit or hyphen (DR-1, PR-12,
AR-R-7) and the number must not continue as `.d`, `-d` or a letter (spike R-2.3, R-3-4, R-12a).

THE TWO DEFECTS. (1) A number cited with no heading. (2) A number defined by two headings. There is
no frozen list: nothing predates this control, so the list that "may only shrink" starts empty and
therefore does not exist. And one refusal to report clean (class PACK-P): a register with level-2/3
headings of which none parses as a ruling is NOT CHECKED - the R-n register passed as "0 defined"
because an unread register and an empty one printed the same.

USAGE
  python3 verify-ruling-citations.py                 scan the repository at the cwd
  python3 verify-ruling-citations.py --root <repo>   scan that repository
  python3 verify-ruling-citations.py --self-test     prove both defects fire and a clean tree is quiet (DC-104)

EXIT  0 ok  ·  1 refused (defects listed, one per line)  ·  2 usage (--root is not a directory)
"""
from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile
from pathlib import Path
from typing import Dict, List, Set, Tuple

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

REGISTER = Path("docs") / "notes" / "rulings.md"
ROOTS = ("docs", "pack", os.path.join(".agents", "log"), ".github", ".claude")
SKIP_PARTS = {"ai-forward-pack", "node_modules", ".git", "_site"}
PROSE_SUFFIXES = {".md", ".html", ".txt"}
SHORT = r"R-(\d{1,3})(?!\w|[.-]\d)"   # R-n: the number ends the id (not R-2.3, R-3-4, R-12a)
CITATION = re.compile(r"\bRulings?\s+(\d{1,3})\b|(?<![\w-])" + SHORT)
DEFINITION = re.compile(r"^#{2,3}[ \t]+(?:Ruling[ \t]+(\d{1,3})\b|" + SHORT + ")", re.M)
HEADING = re.compile(r"^#{2,3}[ \t]+\S", re.M)


def _number(match: "re.Match[str]") -> Tuple[int, str]:
    """The number a match names, and its spelling as written (`Ruling n` or `R-n`)."""
    if match.group(1) is not None:
        return int(match.group(1)), "Ruling {}".format(int(match.group(1)))
    return int(match.group(2)), "R-{}".format(int(match.group(2)))


def _name(number: int, spellings: Dict[int, Set[str]]) -> str:
    """How a defect names a number: the spelling(s) the repository used for it."""
    return " / ".join(sorted(spellings.get(number) or {"Ruling {}".format(number)}, reverse=True))


def definitions(root: Path, spellings: Dict[int, Set[str]] = None) -> Dict[int, List[int]]:
    """number -> the register line numbers that define it (two lines = the collision)."""
    path = root / REGISTER
    found: Dict[int, List[int]] = {}
    if not path.is_file():
        return found
    text = path.read_text(encoding="utf-8", errors="replace")
    for match in DEFINITION.finditer(text):
        line = text.count("\n", 0, match.start()) + 1
        number, spelled = _number(match)
        found.setdefault(number, []).append(line)
        if spellings is not None:
            spellings.setdefault(number, set()).add(spelled)
    return found


def citations(root: Path, spellings: Dict[int, Set[str]] = None) -> Dict[int, Set[str]]:
    """number -> the prose files (repo-relative, posix) that cite it. The register is scanned
    too: prose in it that names a number is a citation (a heading is the only definition)."""
    found: Dict[int, Set[str]] = {}
    for top in ROOTS:
        base = root / top
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*")):
            if not path.is_file() or path.suffix.lower() not in PROSE_SUFFIXES:
                continue
            if SKIP_PARTS.intersection(path.relative_to(root).parts):
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            for match in CITATION.finditer(text):
                number, spelled = _number(match)
                found.setdefault(number, set()).add(path.relative_to(root).as_posix())
                if spellings is not None:
                    spellings.setdefault(number, set()).add(spelled)
    return found


def unread_headings(root: Path) -> int:
    """How many level-2/3 register headings exist when NONE of them parsed as a definition (PACK-P):
    a register of headings in an unknown numbering is not an empty register, and must not read as one."""
    path = root / REGISTER
    if not path.is_file():
        return 0
    text = path.read_text(encoding="utf-8", errors="replace")
    if DEFINITION.search(text):
        return 0
    return len(HEADING.findall(text))


def check(root: Path) -> Tuple[List[str], Dict[int, List[int]], Dict[int, Set[str]]]:
    defined_as: Dict[int, Set[str]] = {}
    cited_as: Dict[int, Set[str]] = {}
    defined = definitions(root, defined_as)
    cited = citations(root, cited_as)
    defects: List[str] = []
    unread = unread_headings(root)
    if unread:
        defects.append("NOT CHECKED: {} has {} heading(s) and none is a ruling in a form this gate reads"
                       " (`## Ruling n` or `## R-n`). A register the gate cannot read is not an empty register"
                       " (class PACK-P).".format(REGISTER.as_posix(), unread))
    for number in sorted(defined):
        lines = defined[number]
        if len(lines) > 1:
            defects.append("{} is defined twice in {} (lines {}). One number, one decision (class ID-A):"
                           " the register is the allocator, so the second heading is a collision."
                           .format(_name(number, defined_as), REGISTER.as_posix(), ", ".join(str(n) for n in lines)))
    for number in sorted(cited):
        if number in defined:
            continue
        where = sorted(cited[number])
        defects.append("{} is cited as authority in {} file(s) and no heading in {} defines it. Cited by: {}."
                       " Anyone can assert what it said and nobody can check."
                       .format(_name(number, cited_as), len(where), REGISTER.as_posix(), ", ".join(where)))
    return defects, defined, cited


def self_test() -> int:
    """Break a synthetic tree in each direction and require the gate to notice (DC-104): a
    control's first green is evidence about the control, and the control is the part nobody
    re-examines because it is what they just reasoned about."""
    nl = "\n"
    head = "---" + nl + "id: rulings" + nl + "type: doc" + nl + "---" + nl + "# Rulings" + nl
    cases = [
        ("a ruling cited with no heading defining it",
         {"docs/plans/p.md": "We applied Ruling 91 here." + nl}, "no heading"),
        ("the same ruling, defined by a heading in the register",
         {"docs/plans/p.md": "We applied Ruling 91 here." + nl,
          REGISTER.as_posix(): head + "### Ruling 91 — the thing" + nl + "It decided the thing." + nl}, None),
        ("a number defined by two headings",
         {REGISTER.as_posix(): head + "### Ruling 91 — one" + nl + "## Ruling 91 — again" + nl}, "defined twice"),
        ("a definition that is only prose, not a heading",
         {REGISTER.as_posix(): head + "Ruling 91 decided the thing, at length, repeatedly." + nl}, "no heading"),
        ("a heading outside the register does not define",
         {"docs/notes/other.md": "### Ruling 91 — elsewhere" + nl}, "no heading"),
        ("a record is a quote, not a citation",
         {"docs/audit/audit-log.jsonl": '{"prompt": "ai-de cited Ruling 91"}' + nl}, None),
        ("a short-form R-n cited with no heading defining it",
         {"docs/plans/p.md": "As ruling R-99 says." + nl,
          REGISTER.as_posix(): head + "## R-1 · 2026-09-24 · Owner · seats" + nl}, "R-99 is cited"),
        ("a short-form R-n cited and defined by a short-form heading",
         {"docs/plans/p.md": "Per R-7." + nl, REGISTER.as_posix(): head + "## R-7 · 2026-09-24 · Owner · x" + nl}, None),
        ("a register whose headings are all in an unread numbering",
         {REGISTER.as_posix(): head + "## RUL-1 · 2026-09-24 · Owner · x" + nl}, "NOT CHECKED"),
        ("look-alike ids are not citations",
         {"docs/plans/p.md": "Spike R-2.3, US-13, HB-PRE-002, DR-1, R-12a." + nl}, None),
    ]
    failures = []
    for name, files, expected in cases:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for rel, body in files.items():
                target = root / rel
                target.parent.mkdir(parents=True, exist_ok=True)
                with open(target, "w", encoding="utf-8", newline="\n") as fh:
                    fh.write(body)
            found, _, _ = check(root)
            if expected is None:
                if found:
                    failures.append("{}: expected clean, got {}".format(name, found))
            elif not any(expected in d for d in found):
                failures.append("{}: expected a defect containing {!r}, got {}".format(name, expected, found))
    if failures:
        print("self-test FAILED: {} case(s).".format(len(failures)))
        for failure in failures:
            print("  - " + failure)
        return 1
    print("self-test: {} cases, the gate fires on each break and stays quiet when clean.".format(len(cases)))
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", default=os.getcwd(), help="the repository to scan (default: cwd)")
    parser.add_argument("--self-test", action="store_true", help="prove this gate can fail, against a synthetic tree")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    root = Path(args.root)
    if not root.is_dir():
        print("verify-ruling-citations: usage — --root is not a directory: {}".format(root), file=sys.stderr)
        return 2
    defects, defined, cited = check(root)
    if defects:
        print("verify-ruling-citations: FAILED — {} defect(s) under {}.".format(len(defects), root))
        for defect in defects:
            print("  - " + defect)
        return 1
    print("verify-ruling-citations: OK — {} ruling(s) cited, {} defined by a heading in {} (root {}).".format(
        len(cited), len(defined), REGISTER.as_posix(), root))
    return 0


if __name__ == "__main__":
    sys.exit(main())
