#!/usr/bin/env python3
"""run-verify-gates.py - every verify-*.py gate, one exit status, no pipe.

The control for defect class DC-113 (a gate made advisory by the SHAPE of the shell line)
in its loop form. Measured in a consuming repo over three days: 168 main-line shell lines
piped a gate run into `tail`/`head`/`grep` with no pipefail, 102 of them also committing,
merging or pushing on the same line; four hid a red. A `for ... done` loop's exit status is
its LAST command's, not its worst one's, so "chain with &&" is defeated by construction -
there is no single status to chain on. This runner IS that status.

What it runs. Every `verify-*.py` in the repository's gate directories - by default
`tools/` (the repo's own gates) and `docs/ai-forward-pack/scripts/` (the pack's) - each in
its argument-free form, which every gate is expected to support as "check the repository".
A gate that needs arguments gets them with `--args NAME=ARG ...`. A gate that is missing,
cannot be started, or runs past its budget counts as a FAILURE, never as a skip; a skip is
printed by name (`--skip`), never silently.

Why not bounded_process.run_bounded: it blanks stdout on a non-zero exit (its callers read
stderr diagnostics only), and the one thing this runner must relay is a failing gate's last
printed line - the finding. The budget is enforced here with a process-tree kill instead.

Usage
  python3 run-verify-gates.py                        run every gate found
  python3 run-verify-gates.py --dir tools            only this directory (repeatable)
  python3 run-verify-gates.py --skip verify-slow.py  skip by basename (printed as skipped)
  python3 run-verify-gates.py --args verify-test-run.py=--no-run
  python3 run-verify-gates.py --budget 300           seconds per gate (default 300)
  python3 run-verify-gates.py --self-test            prove a red gate is reported red

Exit 0 when every gate passed, 1 when any failed, 2 on a usage error (no gate found).
Stdlib only. Use it as the ONLY line before a commit at a join:
  python3 run-verify-gates.py && git commit ...
"""
from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

BUDGET_SECONDS = 300
DEFAULT_DIRS = ("tools", os.path.join("docs", "ai-forward-pack", "scripts"))
SELF = Path(__file__).resolve()


def repo_root(start: Path | None = None) -> Path:
    """The repository root: git's answer, else the nearest ancestor carrying `tools/` or
    `docs/ai-forward-pack/`, else the cwd. Printed, never assumed silently."""
    cwd = start or Path.cwd()
    done = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=str(cwd),
                          capture_output=True, text=True, encoding="utf-8", errors="replace")
    if done.returncode == 0 and done.stdout.strip():
        return Path(done.stdout.strip())
    for candidate in [cwd, *cwd.parents]:
        if (candidate / "tools").is_dir() or (candidate / "docs" / "ai-forward-pack").is_dir():
            return candidate
    return cwd


def gates(root: Path, dirs) -> list[Path]:
    found = []
    for d in dirs:
        base = root / d
        if not base.is_dir():
            continue
        for p in sorted(base.glob("verify-*.py")):
            if p.is_file() and p.resolve() != SELF:
                found.append(p)
    return found


def _kill_tree(process: subprocess.Popen) -> None:
    if os.name == "nt":
        subprocess.run(["taskkill", "/T", "/F", "/PID", str(process.pid)],
                       capture_output=True)
    else:
        try:
            os.killpg(os.getpgid(process.pid), 9)
        except (OSError, ProcessLookupError):
            process.kill()


def run_one(gate: Path, root: Path, extra_args=(), budget: int = BUDGET_SECONDS) -> tuple[bool, float, str]:
    """(ok, seconds, last_line). A gate past its budget is killed with its children and is
    a failure - a hung gate that is waited on forever is the same silence as a skipped one."""
    command = [sys.executable, str(gate), *extra_args]
    env = dict(os.environ, PYTHONIOENCODING="utf-8")
    started = time.monotonic()
    try:
        process = subprocess.Popen(command, cwd=str(root), env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT,
                                   start_new_session=(os.name != "nt"))
    except OSError as error:
        return False, time.monotonic() - started, "could not start: {0}".format(error)
    try:
        out, _ = process.communicate(timeout=budget)
    except subprocess.TimeoutExpired:
        _kill_tree(process)
        try:
            process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            pass
        return False, time.monotonic() - started, "ran past {0} s (killed with its children)".format(budget)
    elapsed = time.monotonic() - started
    lines = out.decode("utf-8", errors="replace").strip().splitlines()
    return process.returncode == 0, elapsed, (lines[-1] if lines else "(no output)")


def parse_args_map(specs) -> dict:
    out = {}
    for spec in specs or []:
        name, sep, rest = spec.partition("=")
        if not sep:
            raise SystemExit("--args expects NAME=ARG[ ARG...], got {0!r}".format(spec))
        out.setdefault(name, []).extend(rest.split())
    return out


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0],
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dir", action="append", metavar="DIR",
                        help="gate directory relative to the repo root (repeatable; default: "
                             "tools/ and docs/ai-forward-pack/scripts/)")
    parser.add_argument("--skip", nargs="*", default=[], metavar="NAME",
                        help="gate basenames to skip - each is printed as skipped, never silently")
    parser.add_argument("--args", action="append", metavar="NAME=ARG",
                        help="arguments for one gate, e.g. verify-test-run.py=--no-run (repeatable)")
    parser.add_argument("--budget", type=int, default=BUDGET_SECONDS, metavar="SECONDS",
                        help="per-gate wall budget; past it the gate is killed and counted red")
    parser.add_argument("--root", help="repository root (default: git's answer from the cwd)")
    parser.add_argument("--self-test", action="store_true",
                        help="prove the runner reports a failing gate as a failure")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    root = Path(args.root).resolve() if args.root else repo_root()
    extra = parse_args_map(args.args)
    selected = gates(root, args.dir or DEFAULT_DIRS)
    if not selected:
        print("run-verify-gates: no verify-*.py found under {0} in {1}".format(
            ", ".join(args.dir or DEFAULT_DIRS), root), file=sys.stderr)
        return 2

    print("run-verify-gates: {0} gate(s) in {1}".format(len(selected), root))
    failures, ran = 0, 0
    for gate in selected:
        if gate.name in args.skip:
            print("  skip   {0}".format(gate.name))
            continue
        ran += 1
        ok, elapsed, last = run_one(gate, root, extra.get(gate.name, ()), args.budget)
        failures += 0 if ok else 1
        print("  {0}   {1:44} {2:6.1f} s  {3}".format("ok  " if ok else "FAIL", gate.name,
                                                    elapsed, last[:110]))
    if failures:
        print("run-verify-gates: {0} of {1} gate(s) FAILED - the line stops here.".format(failures, ran))
        return 1
    print("run-verify-gates: OK - {0} gate(s) passed.".format(ran))
    return 0


def self_test() -> int:
    """A gate that exits 1 must be red; one that exits 0 green; one that hangs past its
    budget red - and the runner's own exit status must carry each verdict (DC-104: a control
    that cannot fail is not a control)."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        gate_dir = root / "tools"
        gate_dir.mkdir()
        (gate_dir / "verify-red.py").write_text("import sys; print('the finding'); sys.exit(1)\n", encoding="utf-8", newline="\n")
        (gate_dir / "verify-green.py").write_text("print('green')\n", encoding="utf-8", newline="\n")
        (gate_dir / "verify-hang.py").write_text("import time; time.sleep(30)\n", encoding="utf-8", newline="\n")
        ok_red, _, last_red = run_one(gate_dir / "verify-red.py", root)
        ok_green, _, _ = run_one(gate_dir / "verify-green.py", root)
        ok_hang, _, last_hang = run_one(gate_dir / "verify-hang.py", root, budget=2)
        status = main(["--root", str(root), "--dir", "tools", "--skip", "verify-hang.py"])
        status_green = main(["--root", str(root), "--dir", "tools", "--skip", "verify-hang.py", "verify-red.py"])
    problems = []
    if ok_red:
        problems.append("a red gate was reported green")
    if last_red != "the finding":
        problems.append("the failing gate's last line was not relayed ({0!r})".format(last_red))
    if not ok_green:
        problems.append("a green gate was reported red")
    if ok_hang or "ran past" not in last_hang:
        problems.append("a gate past its budget was not counted red ({0!r})".format(last_hang))
    if status != 1:
        problems.append("the runner exited {0} with a red gate in the set".format(status))
    if status_green != 0:
        problems.append("the runner exited {0} with only green gates".format(status_green))
    if problems:
        print("run-verify-gates --self-test: FAILED - " + "; ".join(problems))
        return 1
    print("run-verify-gates --self-test: OK - a red gate is a failure, a green one is not, "
          "a hung one is killed and red, and the runner's exit status carries the verdict")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
