#!/usr/bin/env python3
"""verify-subprocess-utf8.py - a text-mode subprocess states its encoding; the locale never decides.

THE CLASS (PLAT-A; ai-de's DC-211, absorbed). `subprocess.run(..., text=True)` with no `encoding=`
decodes the child's bytes with the interpreter's locale codec - cp1252 on Windows - while every
file this pack writes and every path git prints is UTF-8. A non-ASCII branch name or file path
comes back as mojibake, `_dirty_paths` mis-compares, and a byte-identity oracle reports its own
committed bytes as CHANGED. The pack fixed this once, in `prompt-log.py`, with a comment naming
the defect, and left the same shape in 21 sibling call sites (the audit of 2026-09-19). A lesson
applied where it was learned and never swept is the class; this gate is the sweep that stays.

WHAT IT CHECKS. Every `.py` under pack/scripts, pack/adapters/hooks and tools (the sources;
docs/ai-forward-pack/scripts is a generated copy and is skipped). Each call to
subprocess.run / check_output / check_call / call / Popen that passes `text=True` or
`universal_newlines=True` must also pass `encoding=`. Read with the `ast` module - a call split
over lines or spelled through an alias still counts; prose in comments and docstrings does not.

USAGE
  python3 verify-subprocess-utf8.py               scan this repository
  python3 verify-subprocess-utf8.py --root <repo>  scan that repository
  python3 verify-subprocess-utf8.py --self-test    prove the gate can fail (DC-104)

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
CALLS = {"run", "check_output", "check_call", "call", "Popen"}


def _is_true(node):
    return isinstance(node, ast.Constant) and node.value is True


def scan_source(source, rel):
    """Findings for one module's source text: (line, head) per undecoded text-mode call."""
    try:
        tree = ast.parse(source)
    except SyntaxError as exc:
        return [(exc.lineno or 0, "does not parse: {0}".format(exc.msg))]
    findings = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        func = node.func
        name = func.attr if isinstance(func, ast.Attribute) else (func.id if isinstance(func, ast.Name) else None)
        if name not in CALLS:
            continue
        keywords = {kw.arg: kw.value for kw in node.keywords if kw.arg}
        textual = any(_is_true(keywords[k]) for k in ("text", "universal_newlines") if k in keywords)
        if textual and "encoding" not in keywords:
            findings.append((node.lineno, "{0}(... text=True) without encoding=".format(name)))
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
                for lineno, head in scan_source(source, rel):
                    out.append("{0}:{1}: {2}".format(rel, lineno, head))
    return out


def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        os.makedirs(os.path.join(tmp, "tools"))
        probe = os.path.join(tmp, "tools", "probe.py")
        with open(probe, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(
                "import subprocess\n"
                "# a comment saying text=True is not a call\n"
                'out = subprocess.run(["git", "show", "HEAD:x"],\n'
                "                     capture_output=True, text=True)\n"
                'ok = subprocess.run(["git", "x"], capture_output=True, text=True, encoding="utf-8")\n'
                'legacy = subprocess.check_output(["git", "x"], universal_newlines=True)\n'
                'raw = subprocess.run(["git", "x"], capture_output=True)\n')
        findings = scan(tmp)
        expected = {"tools/probe.py:3", "tools/probe.py:6"}
        got = {f.rsplit(":", 1)[0] for f in findings}
        if got != expected:
            print("self-test FAILED: expected findings at {0}, got {1}".format(sorted(expected), findings))
            return 1
    print("self-test OK: text=True and universal_newlines=True without encoding= are findings; "
          "a stated encoding, a bytes call and a comment are not")
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
        print("text-mode subprocess calls that let the locale choose the codec ({0}):".format(len(findings)))
        for finding in findings:
            print("  " + finding)
        print('fix: add encoding="utf-8", errors="replace" to the call (prompt-log.py has the worked example)')
        return 1
    print("clean - every text-mode subprocess under {0} states its encoding".format(", ".join(ROOTS)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
