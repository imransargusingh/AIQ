#!/usr/bin/env python3
"""verify-no-new-console-launches.py - no code launches a child with CREATE_NEW_CONSOLE.

The control for defect class DC-170, measured in a consuming repo (2026-09-12): on a
machine whose default terminal application is Windows Terminal, a child started with
`CREATE_NEW_CONSOLE` is handed to Windows Terminal as a TAB, and Windows Terminal's agent
host attaches an agent session (an assistant child and its MCP servers, one node.exe each)
to that tab and keeps it for Windows Terminal's lifetime after the tab closes. Nine test
classes launched a helper that way: two tests, two node.exe born; 25 during one whole-suite
run; 257 accumulated over two days. The launcher now uses `CREATE_NO_WINDOW` (a headless
console, never a tab): the same tests, zero born. The mechanism is the platform's, not the
project's, so the gate ships with the pack.

What it reads. Every source file under the scan roots (default `src/` and `tests/`,
excluding `bin/` and `obj/`), in the languages where the flag is spelled by name (C#, C,
C++, Rust, Go, Python, PowerShell, JavaScript/TypeScript); the token `CREATE_NEW_CONSOLE`
on a line that is CODE - one whose first non-blank characters are not a comment leader
(`//`, `///`, `#`, `*`, `--`), because a doc comment may name the flag while explaining why
it is not used. Allowlist: none. A `const` declaration counts: it exists to be used.

Usage
  python3 verify-no-new-console-launches.py                 scan src/ and tests/
  python3 verify-no-new-console-launches.py --root-dir app   scan these roots (repeatable)
  python3 verify-no-new-console-launches.py --self-test      prove the gate can fail (DC-104)

Exit 0 when clean, 1 on a finding, 2 on a usage error. Stdlib only.
"""
from __future__ import annotations

import argparse
import re
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

DEFAULT_ROOTS = ("src", "tests")
EXTENSIONS = (".cs", ".c", ".cc", ".cpp", ".h", ".hpp", ".rs", ".go", ".py", ".ps1", ".psm1",
              ".js", ".ts")
SKIP_DIRS = {"bin", "obj", "node_modules", ".git", "target", "__pycache__"}
TOKEN = re.compile(r"CREATE_NEW_CONSOLE")
COMMENT = re.compile(r"^\s*(//|#|\*|--)")
SELF = Path(__file__).resolve()   # this file names the token it hunts; skipped by identity


def repo_root(start: Path | None = None) -> Path:
    done = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=str(start or Path.cwd()),
                          capture_output=True, text=True, encoding="utf-8", errors="replace")
    if done.returncode == 0 and done.stdout.strip():
        return Path(done.stdout.strip())
    return start or Path.cwd()


def findings(root: Path, roots=DEFAULT_ROOTS) -> tuple[list[str], int]:
    """(findings, files read). Reads code lines only; a comment naming the flag is prose."""
    out: list[str] = []
    read = 0
    for sub in roots:
        base = root / sub
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*")):
            if not path.is_file() or path.suffix.lower() not in EXTENSIONS:
                continue
            if any(part in SKIP_DIRS for part in path.relative_to(root).parts):
                continue
            if path.resolve() == SELF:
                continue
            try:
                lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
            except OSError as error:
                out.append("{0}: unreadable ({1})".format(path.relative_to(root).as_posix(), error))
                continue
            read += 1
            for number, line in enumerate(lines, 1):
                if TOKEN.search(line) and not COMMENT.match(line):
                    out.append("{0}:{1}: {2}".format(path.relative_to(root).as_posix(), number,
                                                   line.strip()[:100]))
    return out, read


def self_test() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        fake = Path(tmp)
        (fake / "tests").mkdir()
        (fake / "tests" / "Launch.cs").write_text(
            "/// <summary>Explains CREATE_NEW_CONSOLE in prose.</summary>\n"
            "const uint CREATE_NEW_CONSOLE = 0x00000010;\n", encoding="utf-8", newline="\n")
        (fake / "tests" / "launch.py").write_text(
            "# a comment naming CREATE_NEW_CONSOLE\n"
            "flags = subprocess.CREATE_NEW_CONSOLE\n", encoding="utf-8", newline="\n")
        red, _ = findings(fake)
        (fake / "tests" / "Launch.cs").write_text(
            "/// <summary>Explains CREATE_NEW_CONSOLE in prose.</summary>\n"
            "const uint CREATE_NO_WINDOW = 0x08000000;\n", encoding="utf-8", newline="\n")
        (fake / "tests" / "launch.py").write_text(
            "# a comment naming CREATE_NEW_CONSOLE\nflags = 0\n", encoding="utf-8", newline="\n")
        green, read = findings(fake)
    if len(red) != 2 or green or read != 2:
        print("verify-no-new-console-launches --self-test: FAILED (red={0}, green={1}, read={2})".format(
            red, green, read))
        return 1
    print("verify-no-new-console-launches --self-test: OK - a code-line launch is a finding "
          "(C# and Python), a comment is not")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0],
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--root", help="repository root (default: git's answer from the cwd)")
    parser.add_argument("--root-dir", action="append", metavar="DIR",
                        help="a directory to scan, relative to the root (repeatable; default: src, tests)")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    root = Path(args.root).resolve() if args.root else repo_root()
    roots = tuple(args.root_dir) if args.root_dir else DEFAULT_ROOTS
    found, read = findings(root, roots)
    if found:
        print("verify-no-new-console-launches: FAILED - a child is launched with CREATE_NEW_CONSOLE (DC-170):")
        for f in found:
            print("  - " + f)
        print("  remedy: CREATE_NO_WINDOW (a headless console) - on a Windows Terminal host a new "
              "console is a tab, and the tab's agent outlives the test")
        return 1
    if read == 0:
        print("verify-no-new-console-launches: OK (vacuous) - no source file under {0} in {1}; "
              "pass --root-dir to name the code roots".format(", ".join(roots), root))
        return 0
    print("verify-no-new-console-launches: OK - {0} file(s) read under {1}, no CREATE_NEW_CONSOLE launch.".format(
        read, ", ".join(roots)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
