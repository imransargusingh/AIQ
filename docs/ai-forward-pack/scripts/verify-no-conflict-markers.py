#!/usr/bin/env python3
"""verify-no-conflict-markers.py - a conflict marker must never reach a commit.

The control for defect class DC-136 (a merge resolved as "regenerate, then stage
everything" leaves markers in a file that is patched in place, not regenerated), measured
in a consuming repo: two PUBLISHED pages carried `<<<<<<< HEAD` / `=======` / `>>>>>>>`
on main. Every existing gate passed - a figure check verified four copies of a right
answer, and a derived-views check did not own the file because it was only PARTIALLY
derived. A content check cannot see structural damage by construction. Then a join
resolved a register conflict, piped this gate's output through `| tail -1`, read the
remedy text as a pass, and sealed a merge carrying markers (DC-113's fourth recurrence).

That is why this gate is FIRST in a join's step list and runs on its own line: a marker is
syntactically legal in almost every text format we commit, survives a skimmed diff of a
large generated file, and is unambiguous evidence that a file which should have been
regenerated was resolved by hand instead. There is no legitimate reason for one in a
tracked file, which makes this the rare check with no judgement in it.

What is checked. Every tracked text file, for `<<<<<<<`, `>>>>>>>` and `|||||||` at the
start of a line. `=======` alone is deliberately NOT flagged: it is a Markdown setext
heading underline and a reStructuredText rule, and a gate that fires on valid prose is a
gate someone switches off.

Usage
  python3 verify-no-conflict-markers.py               scan every tracked file
  python3 verify-no-conflict-markers.py --root <repo>  scan that repository
  python3 verify-no-conflict-markers.py --self-test    prove all three directions

Exit 0 when clean, 1 on any finding (or when nothing could be read - a verdict over an
empty corpus is not a verdict, PACK-P). Stdlib only.
"""
from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

# Only the unambiguous ones. `=======` alone is legitimate Markdown/RST and is never flagged.
MARKERS = ("<<<<<<<", ">>>>>>>", "|||||||")
SELF = Path(__file__).resolve()


def repo_root(start: Path | None = None) -> Path | None:
    done = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=str(start or Path.cwd()),
                          capture_output=True, text=True, encoding="utf-8", errors="replace")
    if done.returncode != 0 or not done.stdout.strip():
        return None
    return Path(done.stdout.strip())


def tracked_files(root: Path) -> tuple[list[str], str | None]:
    """Returns (files, error). A failed `git ls-files` yields an EMPTY list, and an empty
    corpus read as "no markers found" is a fail-open gate (PACK-P): the return code is
    read and handed back, so the caller reports NOT-CHECKED instead of OK."""
    try:
        done = subprocess.run(["git", "ls-files", "-z"], capture_output=True, text=True,
                              encoding="utf-8", errors="replace", cwd=str(root))
    except (OSError, subprocess.SubprocessError) as exc:
        return [], "{0}: {1}".format(exc.__class__.__name__, exc)
    if done.returncode != 0:
        reason = (done.stderr or done.stdout or "").strip()
        return [], reason or "git ls-files exited {0}".format(done.returncode)
    return [p for p in done.stdout.split("\0") if p], None


def scan(root: Path, files: list[str]) -> tuple[list[str], int]:
    """Returns (findings, files actually read). This file describes the markers it hunts,
    so it necessarily contains them in prose and is skipped by identity, not by name."""
    findings: list[str] = []
    read = 0
    for relative in files:
        path = root / relative
        try:
            if path.resolve() == SELF:
                continue
        except OSError:
            pass
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue   # binary, or gone from the worktree: neither carries a text marker
        read += 1
        for number, line in enumerate(text.splitlines(), 1):
            if line.startswith(MARKERS):
                findings.append("{0}:{1} carries a conflict marker: {2!r}".format(
                    relative, number, line.strip()[:60]))
    return findings, read


def self_test() -> int:
    """All three directions: it fires; it does NOT fire on valid prose; an empty corpus is
    refused rather than reported clean."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        dirty = root / "conflicted.md"
        clean = root / "setext.md"
        dirty.write_text("before\n" + "<" * 7 + " HEAD\nmine\n=======\ntheirs\n"
                         + ">" * 7 + " origin/main\nafter\n", encoding="utf-8", newline="\n")
        clean.write_text("A heading\n=========\n\nbody text\n", encoding="utf-8", newline="\n")
        findings, _ = scan(root, ["conflicted.md"])
        if not any("conflicted.md" in f for f in findings):
            print("self-test FAILED: a conflict marker was not reported", file=sys.stderr)
            return 1
        findings, _ = scan(root, ["setext.md"])
        if findings:
            print("self-test FAILED: a Markdown setext underline was reported: {0}".format(findings),
                  file=sys.stderr)
            return 1
        _, read = scan(root, ["NO_SUCH_FILE_AT_ALL.md"])
        if read != 0:
            print("self-test FAILED: a missing file was counted as read", file=sys.stderr)
            return 1
        # A failing `git ls-files` must surface as an error, not as an empty clean scan:
        # a temp dir is not a repository, so git exits non-zero here.
        files, error = tracked_files(root)
        if files or error is None:
            print("self-test FAILED: a failing `git ls-files` was reported as an empty clean "
                  "corpus instead of an error", file=sys.stderr)
            return 1
    print("verify-no-conflict-markers --self-test: OK - a marker is reported, a setext underline "
          "is not, an unreadable corpus counts as nothing read, and a failing `git ls-files` is "
          "an error rather than a clean scan of nothing")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0],
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--root", help="repository to scan (default: git's answer from the cwd)")
    parser.add_argument("--self-test", action="store_true",
                        help="prove the gate fires, does not fire on valid Markdown, and refuses an empty corpus")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    root = repo_root(Path(args.root)) if args.root else repo_root()
    if root is None:
        print("verify-no-conflict-markers: FAILED - not inside a git repository (nothing was scanned)")
        return 1
    files, error = tracked_files(root)
    if error is not None:
        print("verify-no-conflict-markers: NOT-CHECKED")
        print("  - the file list could not be obtained: " + error[:300])
        print("  - nothing was scanned, which is not the same as finding nothing.")
        return 1
    findings, read = scan(root, files)
    if read == 0:
        print("verify-no-conflict-markers: FAILED")
        print("  - no tracked text file could be read at all - this gate examined nothing, which is "
              "not the same as finding nothing")
        return 1
    if findings:
        print("verify-no-conflict-markers: FAILED")
        for finding in findings:
            print("  - " + finding)
        print()
        print("  A marker in a tracked file means a conflict was resolved by hand and left unfinished.")
        print("  If the file is DERIVED, do not edit it - regenerate it:")
        print("      python3 docs/ai-forward-pack/scripts/coord-core.py regen   (or the repo's own regenerator)")
        return 1
    print("verify-no-conflict-markers: OK - {0} tracked text file(s) read, no conflict markers.".format(read))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
