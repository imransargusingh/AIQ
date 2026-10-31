#!/usr/bin/env python3
"""marker-lint.py - completeness check for the pack's inline decision markers.

Tier-1 of the prose->structure review (docs/proposals/prose-to-structure-review.html).
The `simplify:` (L5) and `assume:` (NG4) markers each carry required fields that were,
until now, specified only in prose and therefore unenforced:

  simplify: <shortcut> ... <TRIGGER>                 (the "revisit when" condition; L5/L6)
  assume:   <belief> ... <CONSEQUENCE> ... <CONFIRM>  (what breaks + how to verify; NG4)

This lints for the *semantic components* the directives already require, using the existing
free-prose marker style (a trigger keyword / an em-dash clause / a "Confirm:" cue) rather than
mandating a new label syntax - so every existing marker and the dream.py harvest regex keep
working. It is the first-class harvest command L6 names as the natural follow-up.

Posture (V16a / the docs-graph.py --gate pattern): default reports and exits 0 (warn,
grandfathers legacy free-form); --gate exits 1 on any finding. --json for machine use. A clean
scan of a non-empty corpus says so, so an empty run is distinguishable from a clean one (E14).

Stdlib only; Python 3.8+.
"""
import argparse
import json
import os
import re
import sys

# Windows consoles default to cp1252, which cannot encode the box/arrow glyphs this tool
# prints - `prompt-log.py --help` crashed outright with UnicodeEncodeError (FR-047). The guard
# is applied uniformly (class PLAT-A): a script that survives only because its glyphs happen
# to exist in cp1252 is luck, not an invariant.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

# A marker is a comment: its leader opens the line or follows whitespace. A leader glued to a
# quote or an escape (`"# simplify:`, `\n# assume:`) is string data - a test fixture - and was
# 100% of this linter's findings until it was excluded (FR-077, class LINT-A). dream.py's
# harvest carries the same rule.
MARKER_RX = re.compile(r"(?:^|(?<=\s))(?:#|//)\s?(simplify|assume|ponytail):\s*(.*)")
COMMENT_RX = re.compile(r"^\s*(?:#|//)\s?(.*)")

# Cue sets - word-boundary matched so "if" inside "verify" never counts (the key false-positive).
TRIGGER_RX = re.compile(
    r"\b(when|if|once|upgrade|revisit|beyond|above|exceeds|exceed|grows|grow|"
    r"becomes|become|reaches|reach|past|until|unless|drops|hits)\b", re.I)
CONFIRM_RX = re.compile(
    r"\b(confirm|verify|check|inspect|test|measure|read|query|run|validate)\b", re.I)
CONSEQUENCE_RX = re.compile(
    r"\b(if|breaks|break|false|otherwise|wrong|shifts|shift|fails|fail|corrupts|corrupt|"
    r"silently|would|collide|collides|mismatch|drift|lost|overflow)\b", re.I)

CODE_EXT = (".py", ".ps1", ".ts", ".js", ".cs", ".go", ".rs")
SKIP_DIRS = {".git", "node_modules", "dist", "_site", ".claude", ".github"}


def _iter_files(root, include_md):
    exts = CODE_EXT + ((".md",) if include_md else ())
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        # never lint the generated pack mirror or the dream output. SKIP-P: judged on the path
        # BELOW the root - matching the whole path skipped every file of a repo checked out
        # under a directory whose name contained `ai-forward-pack`, so --gate passed on 0 files.
        parts = os.path.relpath(dirpath, root).split(os.sep)
        if "ai-forward-pack" in parts or "dreams" in parts:
            dirnames[:] = []
            continue
        for fn in filenames:
            if fn.endswith(exts):
                yield os.path.join(dirpath, fn)


def _block(lines, i):
    """Assemble the marker block starting at line i: the marker text plus any immediately
    following continuation comment lines, stopping at the first non-comment line or next marker.
    Returns (marker_keyword, joined_text, next_index)."""
    m = MARKER_RX.search(lines[i])
    marker = m.group(1)
    parts = [m.group(2).strip()]
    j = i + 1
    while j < len(lines):
        if MARKER_RX.search(lines[j]):
            break  # a new marker ends this block
        c = COMMENT_RX.match(lines[j])
        if not c:
            break  # a code (non-comment) line ends this block
        parts.append(c.group(1).strip())
        j += 1
    return marker, " ".join(p for p in parts if p), j


def _classify(marker, text):
    findings = []
    if marker in ("simplify", "ponytail"):
        if not TRIGGER_RX.search(text):
            findings.append("simplify-no-trigger")
    elif marker == "assume":
        if not CONFIRM_RX.search(text):
            findings.append("assume-no-confirm")
        if not CONSEQUENCE_RX.search(text):
            findings.append("assume-no-consequence")
    return findings


def scan(root, include_md):
    total = 0
    files = 0
    findings = []
    for fp in _iter_files(root, include_md):
        files += 1
        try:
            with open(fp, "r", encoding="utf-8", errors="ignore") as f:
                lines = f.read().splitlines()
        except OSError:
            continue
        i = 0
        while i < len(lines):
            if MARKER_RX.search(lines[i]):
                marker, text, nxt = _block(lines, i)
                total += 1
                rel = os.path.relpath(fp, root).replace(os.sep, "/")
                for code in _classify(marker, text):
                    findings.append({
                        "code": code, "marker": marker,
                        "src": "{0}#L{1}".format(rel, i + 1),
                        "text": text[:200],
                    })
                i = nxt
            else:
                i += 1
    return {"markers": total, "scanned_files": files, "findings": findings}


def main(argv=None):
    ap = argparse.ArgumentParser(description="Completeness lint for simplify:/assume: markers.")
    ap.add_argument("--root", default=".", help="directory to scan (default: cwd)")
    ap.add_argument("--gate", action="store_true", help="exit nonzero on any finding (default: warn)")
    ap.add_argument("--json", action="store_true", help="emit JSON")
    ap.add_argument("--include-md", action="store_true", help="also scan .md files")
    args = ap.parse_args(argv)

    result = scan(args.root, args.include_md)

    if args.json:
        print(json.dumps(result, ensure_ascii=False))
    else:
        for f in result["findings"]:
            print("{0}  {1}: {2}".format(f["src"], f["code"], f["text"]))
        if result["markers"] == 0:
            print("marker-lint: 0 markers found in {0} file(s)".format(result["scanned_files"]))
        elif not result["findings"]:
            print("marker-lint: all {0} marker(s) complete".format(result["markers"]))
        else:
            print("marker-lint: {0} finding(s) across {1} marker(s){2}".format(
                len(result["findings"]), result["markers"],
                "" if args.gate else " (warn; use --gate to fail)"))

    if args.gate and result["findings"]:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
