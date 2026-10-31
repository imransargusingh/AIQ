#!/usr/bin/env python3
"""verify-documented-commands.py - every documented command under pack/ runs in any shell.

THE CLASS (PLAT-A, the shell half; cross-platform readiness P4, XS-01..25, 2026-09-19).
The pack targets bash, zsh, PowerShell 5.1/7 and cmd, and an agent types a documented
command as written. Three shapes were written once for bash and left in their siblings:

  1. A trailing ` \\` continuation - bash/zsh only. cmd runs the next line as a SEPARATE
     command; PowerShell continues with a backtick. 10 multi-line commands (16 lines)
     carried it at a01ed77, among them the AL5 audit step, the WT1 worktree step and the
     only join line.
  2. ` && ` between two commands - not a statement separator in Windows PowerShell 5.1
     (it parses as a token error, so `git add -A && git commit ... && git push` runs
     nothing). 2 fenced and 4 inline documented commands carried it.
  3. A line-initial bare `python` - stock macOS and Linux ship `python3` only; the pack's
     own convention is `python3` (the Windows substitution is `pack-doctor`'s job to name).
     7 fenced skill/INSTALL commands and 6 inline ones carried it; INSTALL's labelled
     Windows twin is the one allowlisted line.

SCAN CONTRACT (GO14a - root, recursion, token set, allowlist).
  root       <repo>/pack (absent in a consuming repo: nothing to scan, exit 0 and say so)
  recursion  commands/**/*.md, knowledge/*.md, adapters/INSTALL.md, templates/*.md
  blocks     fenced code blocks (``` or ~~~) whose info string is empty or one of
             bash, sh, shell, zsh. A block whose FIRST line starts with `#!` is a file
             (DC-207: a multi-line program is a file, then a run), not a typed command,
             and is skipped whole. Blank lines and `#` comment lines are skipped.
  tokens     continuation  - the line ends in whitespace + `\\`
             and-chain     - the line contains ` && `
             bare-python   - the first token is `python` (optionally after a `$ ` prompt)
  allowlist  the inline marker `portable-ok: <reason>` anywhere on the line, like
             `machine-path-ok` in verify-no-machine-paths.py. A marker with no reason is
             a finding of its own (`marker-without-reason`).

USAGE
  python3 verify-documented-commands.py               scan this repository
  python3 verify-documented-commands.py --root <repo>  scan that repository
  python3 verify-documented-commands.py --self-test    prove the gate can fail (DC-104)

EXIT  0 clean  ·  1 findings  ·  2 usage
"""
from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path
from typing import Iterator, NamedTuple

# PLAT-A console guard: cp1252 consoles cannot encode the glyphs the findings may quote.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

PACK_DIR = "pack"
ROOT_GLOBS = ("commands/**/*.md", "knowledge/*.md", "adapters/INSTALL.md", "templates/*.md")
COMMAND_LANGS = frozenset({"", "bash", "sh", "shell", "zsh"})
MARKER = "portable-ok:"

_FENCE = re.compile(r"^\s*(`{3,}|~{3,})\s*([A-Za-z0-9_+.-]*)")
_CONTINUATION = re.compile(r"\s\\$")
_BARE_PYTHON = re.compile(r"^\s*(?:\$\s+)?python(?=\s)")
_MARKER_WITH_REASON = re.compile(re.escape(MARKER) + r"\s*\S")


class Finding(NamedTuple):
    path: str
    lineno: int
    token: str
    line: str


def documented_files(pack: Path) -> list[Path]:
    """The files the contract names, in a stable order."""
    files: list[Path] = []
    for pattern in ROOT_GLOBS:
        files.extend(sorted(p for p in pack.glob(pattern) if p.is_file()))
    return files


def command_lines(text: str) -> Iterator[tuple[int, str]]:
    """Yield (lineno, line) for every command line inside a command fence.

    A fence opens on ``` or ~~~ with a command info string and closes on the same
    marker; a block whose first line is a shebang is a file and is skipped whole.
    """
    fence: str | None = None
    in_command = False
    first_line = False
    for lineno, raw in enumerate(text.splitlines(), 1):
        stripped = raw.strip()
        if fence is None:
            opened = _FENCE.match(raw)
            if opened:
                fence = opened.group(1)
                in_command = opened.group(2).lower() in COMMAND_LANGS
                first_line = True
            continue
        if stripped.startswith(fence) and not stripped.strip(fence[0]):
            fence = None
            in_command = False
            continue
        if not in_command:
            continue
        if first_line:
            first_line = False
            if stripped.startswith("#!"):
                in_command = False
                continue
        if not stripped or stripped.startswith("#"):
            continue
        yield lineno, raw


def tokens_in(line: str) -> list[str]:
    """The portability tokens one command line carries (empty when allowlisted)."""
    if MARKER in line:
        return [] if _MARKER_WITH_REASON.search(line) else ["marker-without-reason"]
    found: list[str] = []
    if _CONTINUATION.search(line.rstrip("\r\n")):
        found.append("continuation")
    if " && " in line:
        found.append("and-chain")
    if _BARE_PYTHON.match(line):
        found.append("bare-python")
    return found


def scan(root: Path) -> list[Finding]:
    pack = root / PACK_DIR
    findings: list[Finding] = []
    for path in documented_files(pack):
        text = path.read_text(encoding="utf-8", errors="replace")
        rel = path.relative_to(root).as_posix()
        for lineno, line in command_lines(text):
            for token in tokens_in(line):
                findings.append(Finding(rel, lineno, token, line.strip()))
    return findings


def report(findings: list[Finding], root: Path, pack_present: bool) -> int:
    name = "verify-documented-commands"
    if not pack_present:
        print("{0}: no {1}/ under {2} - nothing to scan (a consuming repo).".format(name, PACK_DIR, root))
        return 0
    if not findings:
        print("{0}: OK - every documented command under {1}/ is single-line, unchained and python3.".format(name, PACK_DIR))
        return 0
    by_token: dict[str, int] = {}
    for f in findings:
        by_token[f.token] = by_token.get(f.token, 0) + 1
        print("{0}:{1}  {2:<22}{3}".format(f.path, f.lineno, f.token, f.line))
    summary = ", ".join("{0} {1}".format(v, k) for k, v in sorted(by_token.items()))
    print("{0}: {1} finding(s) on {2} line(s) - {3}.".format(
        name, len(findings), len({(f.path, f.lineno) for f in findings}), summary))
    print("  remedy  one command per line; `python3`; or mark the line `{0} <reason>`".format(MARKER))
    return 1


_FIXTURE = """# fixture

```bash
python3 x.py derive && \\
python3 x.py validate
python x.py run
git add -A && git commit -m x
python3 ok.py --flag   # portable-ok: single line, kept for the test
python x.py            # portable-ok:
# a comment ending in a backslash is not a command \\
```

```bash
#!/usr/bin/env bash
[ -n "$a" ] && exit 1
```

```yaml
run: a && b
```

prose with `a && b` outside a fence
"""


def self_test() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        skill = root / PACK_DIR / "commands" / "x"
        skill.mkdir(parents=True)
        (skill / "SKILL.md").write_text(_FIXTURE, encoding="utf-8", newline="\n")
        got = sorted((f.lineno, f.token) for f in scan(root))
    want = [(4, "and-chain"), (4, "continuation"), (6, "bare-python"), (7, "and-chain"),
            (9, "marker-without-reason")]
    if got != want:
        print("verify-documented-commands --self-test: FAILED - got {0}, want {1}".format(got, want))
        return 1
    print("verify-documented-commands --self-test: OK - the three shapes are found, the marker "
          "with a reason is honoured, a shebang block and a non-command fence are skipped.")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", default=None, help="repository root (default: this repo)")
    parser.add_argument("--self-test", action="store_true", help="prove the gate can fail")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parents[2]
    if not root.is_dir():
        print("verify-documented-commands: {0} is not a directory".format(root), file=sys.stderr)
        return 2
    pack_present = (root / PACK_DIR).is_dir()
    return report(scan(root) if pack_present else [], root, pack_present)


if __name__ == "__main__":
    sys.exit(main())
