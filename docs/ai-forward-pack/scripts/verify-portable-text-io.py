#!/usr/bin/env python3
"""verify-portable-text-io.py - text the pack writes is LF and UTF-8 on every OS, and every CLI survives a legacy console.

THE CLASS (PLAT-A, the newline and console halves; cross-platform readiness P2/P3, 2026-09-19).
Three shapes, each fixed once somewhere in the pack and left in its siblings:

  1. A text-mode WRITE without `newline="\\n"` emits CRLF on Windows. `audit-log.py` writes its
     JSONL with `newline="\\n"`; 26 sibling writers used the platform default, so a ledger line
     appended on Windows and its LF twin from macOS are two distinct strings that a union merge
     conserves both of.
  2. A script with a `__main__` entry that prints has no stdio guard: `--help` raises
     UnicodeEncodeError on a cp1252 console the moment a help string carries an em-dash.
     `pack-doctor.py` carries the guard (reconfigure stdout/stderr to utf-8, errors=replace);
     nine siblings did not.
  3. `tempfile.mkstemp(..., text=True)` opens the descriptor with `_O_TEXT` on Windows - a flag
     that says CRLF while the wrapper says LF. Measured harmless on windows-latest, kept out of
     the source so the file says one thing.

WHAT IT CHECKS. Every `.py` under pack/scripts, pack/adapters/hooks and tools (sources only).
With the `ast` module: (1) `open(..., mode)` where the mode contains w, a or + and no `newline`
keyword, and `Path.write_text(...)` with no `newline` keyword, in TEXT mode (a `b` in the mode
exempts the call); (2) a module containing `if __name__ == "__main__"` and a `print(` must
contain `.reconfigure(` on a stream (the guard) - modules that never print are exempt;
(3) `mkstemp(... text=True)`.

USAGE
  python3 verify-portable-text-io.py               scan this repository
  python3 verify-portable-text-io.py --root <repo>  scan that repository
  python3 verify-portable-text-io.py --self-test    prove the gate can fail (DC-104)

EXIT  0 clean  ·  1 findings  ·  2 usage
"""
from __future__ import annotations

import argparse
import ast
import os
import sys
import tempfile

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

ROOTS = ("pack/scripts", "pack/adapters/hooks", "tools")


def _const_str(node):
    return node.value if isinstance(node, ast.Constant) and isinstance(node.value, str) else None


def _name(func):
    if isinstance(func, ast.Attribute):
        return func.attr
    if isinstance(func, ast.Name):
        return func.id
    return None


def _dotted(func):
    if isinstance(func, ast.Attribute) and isinstance(func.value, ast.Name):
        return func.value.id + "." + func.attr
    return None


def scan_source(source):
    try:
        tree = ast.parse(source)
    except SyntaxError as exc:
        return [(exc.lineno or 0, "does not parse: {0}".format(exc.msg))]
    findings = []
    has_main = False
    prints = False
    has_guard = ".reconfigure(" in source
    for node in ast.walk(tree):
        if isinstance(node, ast.If):
            test = node.test
            if (isinstance(test, ast.Compare) and isinstance(test.left, ast.Name)
                    and test.left.id == "__name__"):
                has_main = True
        if not isinstance(node, ast.Call):
            continue
        name = _name(node.func)
        dotted = _dotted(node.func)
        keywords = {kw.arg: kw.value for kw in node.keywords if kw.arg}
        if name == "print":
            prints = True
        if name == "open" and not (dotted and dotted.startswith("os.")):
            mode = "r"
            if len(node.args) > 1:
                mode = _const_str(node.args[1]) or "?"
            elif "mode" in keywords:
                mode = _const_str(keywords["mode"]) or "?"
            if "b" not in mode and any(ch in mode for ch in "wa+") and "newline" not in keywords:
                findings.append((node.lineno, 'open(..., "{0}") without newline="\\n"'.format(mode)))
        elif (name == "write_text" and isinstance(node.func, ast.Attribute)
              and "newline" not in keywords):
            # Attribute form only (`path.write_text(...)`): a module-local helper that happens
            # to be called `write_text(...)` is a bare Name, owns its own open(), and is checked
            # there - the first sweep hit that false positive in graphify-setup and obsidian-setup.
            findings.append((node.lineno, 'write_text(...) without newline="\\n"'))
        elif name == "mkstemp" and "text" in keywords and isinstance(keywords["text"], ast.Constant) and keywords["text"].value is True:
            findings.append((node.lineno, "mkstemp(... text=True) - the flag says CRLF on Windows"))
    if has_main and prints and not has_guard:
        findings.append((1, "prints from a __main__ entry with no stdio reconfigure guard (cp1252 console)"))
    return findings


def scan(root):
    out = []
    for base in ROOTS:
        top = os.path.join(root, base)
        if not os.path.isdir(top):
            continue
        for dirpath, dirnames, filenames in os.walk(top):
            dirnames[:] = [d for d in dirnames if d not in ("__pycache__", "node_modules")]
            for filename in sorted(filenames):
                if not filename.endswith(".py"):
                    continue
                path = os.path.join(dirpath, filename)
                rel = os.path.relpath(path, root).replace(os.sep, "/")
                try:
                    with open(path, encoding="utf-8", errors="replace") as handle:
                        source = handle.read()
                except OSError:
                    continue
                for lineno, head in scan_source(source):
                    out.append("{0}:{1}: {2}".format(rel, lineno, head))
    return out


def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        os.makedirs(os.path.join(tmp, "tools"))
        probe = os.path.join(tmp, "tools", "probe.py")
        with open(probe, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(
                "import sys, tempfile\n"
                "from pathlib import Path\n"
                'with open("a.txt", "w", encoding="utf-8") as f: f.write("x")\n'          # 3 finding
                'with open("b.txt", "w", encoding="utf-8", newline="\\n") as f: f.write("x")\n'  # 4 ok
                'with open("c.bin", "wb") as f: f.write(b"x")\n'                          # 5 ok
                'with open("d.txt", encoding="utf-8") as f: f.read()\n'                   # 6 ok
                'Path("e.txt").write_text("x", encoding="utf-8")\n'                       # 7 finding
                'fd, p = tempfile.mkstemp(text=True)\n'                                    # 8 finding
                'def write_text(p, t): pass\n'                                             # 9 ok (local helper)
                'write_text("f.txt", "x")\n'                                               # 10 ok (bare name)
                'if __name__ == "__main__":\n'
                '    print("hi")\n')                                                       # guard finding at 1
        findings = scan(tmp)
        got = sorted(int(f.split(":")[1]) for f in findings)
        if got != [1, 3, 7, 8]:
            print("self-test FAILED: expected findings at lines [1, 3, 7, 8], got {0}".format(findings))
            return 1
        with open(probe, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(
                "import sys\n"
                "for s in (sys.stdout, sys.stderr):\n"
                "    s.reconfigure(encoding='utf-8', errors='replace')\n"
                'with open("b.txt", "w", encoding="utf-8", newline="\\n") as f: f.write("x")\n'
                'if __name__ == "__main__":\n'
                '    print("hi")\n')
        if scan(tmp):
            print("self-test FAILED: clean fixture reported findings: {0}".format(scan(tmp)))
            return 1
    print("self-test OK: an LF-less text write, a write_text without newline, mkstemp(text=True) and an "
          "unguarded __main__ are findings; binary, read-only and guarded shapes are not")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", default=os.getcwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    findings = scan(os.path.abspath(args.root))
    if findings:
        print("text I/O that is not portable across Windows and macOS ({0}):".format(len(findings)))
        for finding in findings:
            print("  " + finding)
        print('fix: newline="\\n" on every text write (audit-log.py:465 is the example); the stdio guard from')
        print("     pack-doctor.py at the top of every printing script; drop text=True from mkstemp")
        return 1
    print("clean - every text write under {0} is LF, every printing CLI guards its console".format(", ".join(ROOTS)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
