#!/usr/bin/env python3
"""conductor-join.py - the join, as a script: every step gated by its exit code, none by a
shell line.

The control for defect class DC-113's fourth recurrence (measured 2026-09-13, a consuming
repo's join): the conductor resolved a register conflict, piped the conflict-marker gate
through `| tail -1`, read the gate's remedy text as a pass, and committed a merge carrying
`<<<<<<<` (DC-136's shape) - caught only because the gate runner was re-run bare before the
push. Three earlier recurrences had the same cause: a hand-typed line at the join whose
status was the formatter's, not the gate's. This script IS the join line. It has no pipes;
each step is a subprocess whose return code decides whether the next runs. The
`execute-with-coordination` skill names it as the only join line.

Steps, in order, stop on the first red (the exit status is the failing step's number):
  0. leader fence                     `coord-core.py leader who --json` BEFORE the merge (and
                                      before --continue): refused with exit 11 (EXIT_FENCE - 0
                                      is success) when the epoch this join carries (--epoch,
                                      default: the ref's epoch at the join's start) is lower
                                      than the ref's, or when the ref cannot be read; an absent
                                      ref is "not applicable" (S2 has no leader)
  1. git merge --no-ff <branch>       a conflict stops here with the file list; resolve by hand,
                                      `git add`, `git commit --no-edit`, re-run with --continue
  2. audit marker                     `audit-log.py start --session <s> --skill execute-with-coordination`
                                      so the join's own duration is MEASURED (a resumed or
                                      hand-typed join set none: 14 of 14 join entries carried
                                      no duration, no tier, no fan-out)
  3. verify-no-conflict-markers       ALWAYS, before anything else reads the tree
  4. checks, then the recount         from join.json (`checks`, `recount`); the recount is timed
                                      as recount_seconds - the largest cost centre of a measured
                                      programme (44.8% of the conductor's active main line) and
                                      the one no entry recorded. --docs-only skips the recount.
  5. audit-log append                 tier T1, fan-out 0, the marker's duration, recount_seconds
  6. regenerate                       `regenerate` from join.json, default `coord-core.py regen`
  7. git add -A && git commit         the join commit (the pre-commit boundary runs)
  8. gates                            `gates` from join.json, default `run-verify-gates.py`
  9. git push <remote> <branch>       only if 8 passed; --no-push skips
 10. build                            `build` from join.json, optional; --no-build skips

join.json (default docs/coordination/join.json; --join <path> overrides; absent = defaults):
  { "checks":     [["python3", "tools/verify-register.py", "--fix-counts"]],
    "recount":    [["python3", "tools/verify-test-run.py", "--update"]],
    "regenerate": [["python3", "docs/ai-forward-pack/scripts/coord-core.py", "regen"]],
    "gates":      [["python3", "docs/ai-forward-pack/scripts/run-verify-gates.py"]],
    "build":      [["dotnet", "build", "src/App/App.csproj", "-c", "Release"]],
    "push_remote": "origin",
    "trailer_file": "docs/coordination/commit-trailer.txt" }
  A command whose first word is `python3` or `python` runs under THIS interpreter.

Usage (from the checkout the join lands on):
  python3 conductor-join.py <branch> --title "<merge title>" --audit-shortname <name>
      --audit-summary "<text>" --audit-goal "<text>" --audit-done-when "<text>"
      [--artifact <path> ...] [--docs-only] [--no-push] [--no-build] [--continue]
      [--join <join.json>] [--session <id>] [--self-test]

Exit 0 on a complete join; the failing step's number otherwise; 11 when the leader fence (step
0) refuses; 2 on a usage error. Stdlib only.
"""
from __future__ import annotations

import argparse
import json
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

HERE = Path(__file__).resolve().parent
PY = sys.executable
JOIN_JSON_DEFAULT = os.path.join("docs", "coordination", "join.json")
SKILL = "execute-with-coordination"
# spec-leader-designation US-7: the fence is a named step with its own exit code. Steps are
# 1..10 and 0 is success, so the fence's refusal cannot be "step 0" as a status.
EXIT_FENCE = 11


def _sibling(name: str) -> str:
    return str(HERE / name)


def repo_root(start: Path | None = None) -> Path | None:
    done = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=str(start or Path.cwd()),
                          capture_output=True, text=True, encoding="utf-8", errors="replace")
    if done.returncode != 0 or not done.stdout.strip():
        return None
    return Path(done.stdout.strip())


def _interp(command: list) -> list[str]:
    command = [str(c) for c in command]
    if command and command[0] in ("python3", "python"):
        command[0] = PY
    return command


def load_join(root: Path, path: str | None) -> dict:
    """The join contract for this repository, or the defaults. A malformed file is a usage
    error, never a silent fallback to defaults (a join that skipped the recount because its
    config had a typo would report as complete)."""
    candidate = Path(path) if path else root / JOIN_JSON_DEFAULT
    if not candidate.is_file():
        if path:
            raise SystemExit("conductor-join: --join {0} does not exist".format(candidate))
        return {}
    try:
        data = json.loads(candidate.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise SystemExit("conductor-join: {0} is not valid JSON ({1})".format(candidate, error))
    if not isinstance(data, dict):
        raise SystemExit("conductor-join: {0} must be a JSON object".format(candidate))
    return data


class Join:
    def __init__(self, root: Path, env: dict, log=print):
        self.root = root
        self.env = env
        self.log = log

    def run(self, step: int, what: str, command: list[str], allow=(0,)) -> subprocess.CompletedProcess:
        self.log("\n== step {0}: {1}\n   $ {2}".format(step, what, " ".join(command)))
        completed = subprocess.run(command, cwd=str(self.root), env=self.env, text=True,
                                   encoding="utf-8", errors="replace", capture_output=True)
        tail = (completed.stdout + completed.stderr).strip().splitlines()
        for line in tail[-8:]:
            self.log("   | " + line[:200])
        if completed.returncode not in allow:
            self.log("\nconductor-join: step {0} ({1}) exited {2} - the join stops here.".format(
                step, what, completed.returncode))
            raise SystemExit(step)
        return completed

    def git(self, *args) -> subprocess.CompletedProcess:
        return subprocess.run(["git", *args], cwd=str(self.root), capture_output=True, text=True,
                              encoding="utf-8", errors="replace")


def leader_fence(j: Join, given: "int | None", log) -> None:
    """Step 0 - the join carries an epoch and the ref decides (spec-leader-designation US-7).

    Not a `Join.run` step: `run` knows exit codes only and the fence must read the JSON
    (absent is not-applicable; expired/released still carry the epoch). Refuses with
    EXIT_FENCE; never proceeds on an unread fence (R4: NOT CHECKED is not "no leader").
    """
    command = [PY, _sibling("coord-core.py"), "leader", "who", "--json"]
    log("\n== step 0: leader fence\n   $ {0}".format(" ".join(command)))
    done = subprocess.run(command, cwd=str(j.root), env=j.env, text=True, encoding="utf-8",
                          errors="replace", capture_output=True)
    payload = {}
    text = done.stdout.strip()
    if text:
        try:
            payload = json.loads(text.splitlines()[-1])
        except ValueError:
            payload = {}
    state = payload.get("state")
    if done.returncode == 4 or state in (None, "not_checked"):
        for line in (done.stdout + done.stderr).strip().splitlines()[-4:]:
            log("   | " + line[:200])
        log("\nconductor-join: step 0 (leader fence) refused - the leader ref could not be read "
            "[COORD-JOIN-LEADER-NOT-CHECKED]; a join never proceeds on an unread fence "
            "(exit {0}).".format(EXIT_FENCE))
        raise SystemExit(EXIT_FENCE)
    if state == "absent":
        log("   | no leader designated - fence not applicable (S2 has no leader)")
        return
    epoch = int(payload.get("epoch"))
    effective = epoch if given is None else int(given)
    log("   | ref epoch {0} ({1}, {2}); this join carries epoch {3}{4}".format(
        epoch, payload.get("leader") or "released", state, effective,
        "" if given is not None else " (default: read at the join's start)"))
    if effective < epoch:
        log("\nconductor-join: step 0 (leader fence) refused - epoch {0} < ref epoch {1} "
            "[COORD-JOIN-EPOCH-STALE]; the plan predates the current leader. Re-read "
            "`coord leader who`, re-plan under the current leader, then re-run (exit {2})."
            .format(effective, epoch, EXIT_FENCE))
        raise SystemExit(EXIT_FENCE)


def join(args, root: Path, contract: dict, log=print) -> int:
    env = dict(os.environ)
    env.setdefault("PYTHONIOENCODING", "utf-8")
    session = args.session or env.get("AGENT_SESSION") or contract.get("session") or "conductor"
    env["AGENT_SESSION"] = session
    env.setdefault("AGENT_NAME", session)
    j = Join(root, env, log)

    branch_now = j.git("branch", "--show-current").stdout.strip()
    if not branch_now:
        log("conductor-join: run from a checkout on a branch (HEAD is detached)")
        return 2

    leader_fence(j, getattr(args, "epoch", None), log)

    trailer = ""
    trailer_file = args.trailer_file or contract.get("trailer_file")
    if trailer_file:
        trailer = "\n\n" + (root / trailer_file).read_text(encoding="utf-8").strip() + "\n"

    if not args.cont:
        if not args.branch:
            log("conductor-join: a branch is required unless --continue")
            return 2
        log("\n== step 1: merge --no-ff {0} into {1}".format(args.branch, branch_now))
        merge = j.git("merge", "--no-ff", args.branch, "-m", args.title + trailer)
        for line in (merge.stdout + merge.stderr).strip().splitlines()[-6:]:
            log("   | " + line[:200])
        if merge.returncode != 0:
            conflicts = j.git("diff", "--name-only", "--diff-filter=U").stdout.strip().splitlines()
            log("\nconductor-join: the merge has conflicts - resolve by hand (a DERIVED file is "
                "regenerated, never resolved), `git add` them, `git commit --no-edit`, then "
                "re-run with --continue:")
            for c in conflicts:
                log("   - " + c)
            return 1
    else:
        log("\n== step 1: --continue (the merge was resolved by hand)")

    # The join's own marker (F-22): one marker measures one join (AL4a), keyed to the skill
    # so a prompt logged meanwhile cannot consume it.
    j.run(2, "audit marker", [PY, _sibling("audit-log.py"), "start", "--session", session, "--skill", SKILL])

    j.run(3, "no conflict markers", [PY, _sibling("verify-no-conflict-markers.py")])

    for command in contract.get("checks") or []:
        j.run(4, "check", _interp(command))
    recount_started = time.monotonic()
    recount = contract.get("recount") or []
    if args.docs_only:
        log("\n== step 4: recount skipped (--docs-only)")
    elif not recount:
        log("\n== step 4: no recount configured (join.json `recount` is empty)")
    else:
        for command in recount:
            j.run(4, "recount", _interp(command))
    recount_seconds = int(time.monotonic() - recount_started)

    audit = [PY, _sibling("audit-log.py"), "append",
             "--shortname", args.audit_shortname, "--session", session,
             "--skill", SKILL, "--kind", "skill", "--tier", "T1", "--fan-out", "0",
             "--prompt", "the join of {0} into {1}".format(args.branch or "the resolved merge", branch_now),
             "--summary", "{0} recount_seconds={1} (docs_only={2}).".format(
                 args.audit_summary, recount_seconds, args.docs_only),
             "--goal", args.audit_goal, "--done-when", args.audit_done_when,
             "--signal-verification-path", "true", "--signal-verification-executed", "true",
             "--signal-acceptance-met", "true"]
    for a in args.artifact:
        audit += ["--artifact", a]
    j.run(5, "audit entry", audit)

    regenerate = contract.get("regenerate")
    if regenerate is None:
        regenerate = [[PY, _sibling("coord-core.py"), "regen"]]
    for command in regenerate:
        j.run(6, "regenerate derived views", _interp(command))

    j.run(7, "stage", ["git", "add", "-A"])
    j.run(7, "commit", ["git", "commit", "-q", "-m",
                        "chore(join): {0} - the join's audit entry, derived views, floors{1}".format(
                            args.audit_shortname, trailer)])

    gates = contract.get("gates")
    if gates is None:
        gates = [[PY, _sibling("run-verify-gates.py")]]
    for command in gates:
        j.run(8, "every verify gate", _interp(command))

    if args.no_push:
        log("\n== step 9: push skipped (--no-push)")
    else:
        j.run(9, "push", ["git", "push", contract.get("push_remote") or "origin", branch_now])

    sha = j.git("rev-parse", "--short", "HEAD").stdout.strip()
    build = contract.get("build") or []
    if args.no_build or not build:
        log("\n== step 10: build skipped ({0}); {1} at {2}".format(
            "--no-build" if args.no_build else "none configured", branch_now, sha))
    else:
        for command in build:
            j.run(10, "build", _interp(command))
        log("\nconductor-join: complete - {0} at {1}; build ran".format(branch_now, sha))
    return 0


def self_test() -> int:
    """Two joins in a throwaway repository, both with --no-push --docs-only and no gates:
    (a) a clean branch completes, and the audit entry carries tier T1, fan_out 0 and a
    measured duration; (b) a branch that commits a file with a conflict marker - DC-136's
    shape, a hand-resolved file - stops at step 3 with NO join commit."""
    def git(cwd, *a):
        done = subprocess.run(["git", *a], cwd=str(cwd), capture_output=True, text=True,
                              encoding="utf-8", errors="replace")
        if done.returncode != 0:
            raise AssertionError("git {0}: {1}".format(" ".join(a), done.stderr))
        return done.stdout

    problems = []
    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp) / "repo"
        repo.mkdir()
        git(tmp, "init", "-q", "-b", "main", str(repo))
        git(repo, "config", "user.email", "join@example.invalid")
        git(repo, "config", "user.name", "join")
        (repo / "docs" / "audit").mkdir(parents=True)
        (repo / "README.md").write_text("base\n", encoding="utf-8", newline="\n")
        git(repo, "add", "-A")
        git(repo, "commit", "-qm", "init")
        contract = {"regenerate": [], "gates": [["python3", "-c", "print('gates ok')"]]}
        (repo / "join.json").write_text(json.dumps(contract), encoding="utf-8", newline="\n")
        git(repo, "add", "-A")
        git(repo, "commit", "-qm", "join contract")

        # (a) the clean branch
        git(repo, "checkout", "-q", "-b", "feature/clean")
        (repo / "clean.txt").write_text("done\n", encoding="utf-8", newline="\n")
        git(repo, "add", "-A")
        git(repo, "commit", "-qm", "clean work")
        git(repo, "checkout", "-q", "main")
        lines = []
        args = _parser().parse_args(["feature/clean", "--title", "merge clean", "--audit-shortname",
                                     "join-clean", "--audit-summary", "s", "--audit-goal", "g",
                                     "--audit-done-when", "d", "--no-push", "--docs-only",
                                     "--join", str(repo / "join.json"), "--session", "selftest"])
        try:
            status = join(args, repo, contract, log=lines.append)
        except SystemExit as exc:
            status = exc.code
        if status != 0:
            problems.append("a clean join exited {0}: {1}".format(status, "\n".join(lines[-12:])))
        else:
            head = git(repo, "log", "-1", "--format=%s").strip()
            if not head.startswith("chore(join): join-clean"):
                problems.append("the join commit is not at HEAD ({0!r})".format(head))
            entries = [json.loads(l) for l in (repo / "docs" / "audit" / "audit-log.jsonl")
                       .read_text(encoding="utf-8").splitlines() if l.strip()]
            entry = entries[-1] if entries else {}
            if entry.get("tier") != "T1" or entry.get("fan_out") != 0:
                problems.append("the join entry lacks tier T1 / fan_out 0: {0}".format(entry))
            if "duration_seconds" not in entry:
                problems.append("the join entry carries no measured duration")
            if "recount_seconds=" not in entry.get("summary", ""):
                problems.append("the join entry does not record recount_seconds")

        # (b) the hand-resolved file carrying a marker
        git(repo, "checkout", "-q", "-b", "feature/markers")
        (repo / "resolved-by-hand.md").write_text(
            "before\n" + "<" * 7 + " HEAD\nmine\n=======\ntheirs\n" + ">" * 7 + " theirs\n",
            encoding="utf-8", newline="\n")
        git(repo, "add", "-A")
        git(repo, "commit", "-qm", "a file resolved by hand")
        git(repo, "checkout", "-q", "main")
        before = git(repo, "rev-parse", "HEAD").strip()
        lines = []
        args = _parser().parse_args(["feature/markers", "--title", "merge markers", "--audit-shortname",
                                     "join-markers", "--audit-summary", "s", "--audit-goal", "g",
                                     "--audit-done-when", "d", "--no-push", "--docs-only",
                                     "--join", str(repo / "join.json"), "--session", "selftest"])
        try:
            status = join(args, repo, contract, log=lines.append)
        except SystemExit as exc:
            status = exc.code
        if status != 3:
            problems.append("a merge carrying a conflict marker did not stop at step 3 (exit {0})".format(status))
        after = git(repo, "log", "-1", "--format=%s").strip()
        if after.startswith("chore(join)"):
            problems.append("a join commit was made over a conflict marker")
        if git(repo, "rev-parse", "HEAD").strip() == before:
            problems.append("precondition: the merge itself should have landed before the gate")

        # (c) the leader fence: a ref at epoch 2 and a plan carrying epoch 1 - refused at step 0
        # with EXIT_FENCE and NO merge (spec-leader-designation US-7).
        git(repo, "checkout", "-q", "-b", "feature/fenced")
        (repo / "fenced.txt").write_text("late\n", encoding="utf-8", newline="\n")
        git(repo, "add", "-A")
        git(repo, "commit", "-qm", "work under an old epoch")
        git(repo, "checkout", "-q", "main")
        now = time.time()
        blob = subprocess.run(["git", "hash-object", "-w", "--stdin"], cwd=str(repo),
                              input=json.dumps({"leader": "lead", "epoch": 2, "pinned_at": now,
                                                "expires_at": now + 300, "ttl": 300,
                                                "host": "claude", "tree": "primary"}),
                              capture_output=True, text=True, encoding="utf-8").stdout.strip()
        git(repo, "update-ref", "refs/coord/leader", blob, "0" * 40)
        before = git(repo, "rev-parse", "HEAD").strip()
        lines = []
        args = _parser().parse_args(["feature/fenced", "--title", "merge fenced", "--audit-shortname",
                                     "join-fenced", "--audit-summary", "s", "--audit-goal", "g",
                                     "--audit-done-when", "d", "--no-push", "--docs-only",
                                     "--join", str(repo / "join.json"), "--session", "selftest",
                                     "--epoch", "1"])
        try:
            status = join(args, repo, contract, log=lines.append)
        except SystemExit as exc:
            status = exc.code
        if status != EXIT_FENCE:
            problems.append("a plan carrying epoch 1 against a ref at epoch 2 did not stop at the "
                            "fence (exit {0})".format(status))
        if git(repo, "rev-parse", "HEAD").strip() != before:
            problems.append("the merge ran although the leader fence refused")
        if not any("COORD-JOIN-EPOCH-STALE" in line for line in lines):
            problems.append("the fence's refusal is not named")
    if problems:
        print("conductor-join --self-test: FAILED - " + "; ".join(problems))
        return 1
    print("conductor-join --self-test: OK - a clean join completes with a measured T1 entry; "
          "a merge carrying a conflict marker stops at step 3 with no join commit; a plan "
          "carrying a lower epoch than refs/coord/leader stops at the fence (exit 11) with no merge")
    return 0


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0],
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("branch", nargs="?", help="the branch to merge (omit with --continue)")
    parser.add_argument("--title", default="", help="the merge commit's title (the trailer is appended)")
    parser.add_argument("--audit-shortname", help="the join's audit shortname")
    parser.add_argument("--audit-summary", default="join")
    parser.add_argument("--audit-goal", default="")
    parser.add_argument("--audit-done-when", default="")
    parser.add_argument("--artifact", action="append", default=[], help="proof paths for the audit entry")
    parser.add_argument("--docs-only", action="store_true", help="skip the recount (no test or product change)")
    parser.add_argument("--continue", dest="cont", action="store_true",
                        help="the merge was resolved by hand; start at step 2")
    parser.add_argument("--no-push", action="store_true")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--join", help="the join contract (default docs/coordination/join.json)")
    parser.add_argument("--session", help="audit session id (default $AGENT_SESSION, then join.json, then 'conductor')")
    parser.add_argument("--trailer-file", dest="trailer_file", help="text appended to every commit message")
    parser.add_argument("--epoch", type=int, default=None,
                        help="the leader epoch this join's plan carries (`coord leader who`); "
                             "default: the ref's epoch at the join's start. Lower than the "
                             "ref's -> refused at step 0 with exit {0}".format(EXIT_FENCE))
    parser.add_argument("--self-test", action="store_true", help="prove a red step stops the join")
    return parser


def main(argv: list[str]) -> int:
    args = _parser().parse_args(argv)
    if args.self_test:
        return self_test()
    if not args.audit_shortname or not args.title:
        print("conductor-join: --title and --audit-shortname are required", file=sys.stderr)
        return 2
    root = repo_root()
    if root is None:
        print("conductor-join: not inside a git repository", file=sys.stderr)
        return 2
    contract = load_join(root, args.join)
    try:
        return join(args, root, contract)
    except SystemExit as exc:
        return int(exc.code or 0)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
