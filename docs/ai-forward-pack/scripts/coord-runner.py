#!/usr/bin/env python3
"""Prepare and run an opt-in, qualified multi-harness coordination contract.

Run `prepare --contract FILE`, qualify the resulting checkout-specific fingerprints,
then `run --run ID --qualification FILE`. `status --run ID` never replays work.
The existing coordinator remains responsible for decisions and semantic review.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, wait, FIRST_COMPLETED
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import threading
import time
import uuid

from bounded_process import run_bounded
from coord_transport import file_root_identities

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")

HERE = Path(__file__).resolve().parent
CAPABILITIES = {"worktree_isolation", "instructions", "hooks", "permissions"}
RANK = {"unsupported": 0, "observed-only": 1, "enforced": 2}
MAX_DOCUMENT = 2 * 1024 * 1024
# `coord-core.py leader who --json` measured 144-246 ms idle, up to 1352 ms under one
# parallel test run on a 28-core host (x-harness-x-model-bench, 2026-09-29, brief
# al-01M3Q4K73M91FWPR48PACHH04W). The old fixed 2 s per-check bound left under 650 ms of
# headroom there and, under heavier load, turned a live lease into a false RUN-LEADER
# (RUN-C). LEADER_CHECK_TIMEOUT gives a single check >3.5x that measured worst case.
LEADER_CHECK_TIMEOUT = 5
# assume: a housekeeping leader check (no operation deadline of its own -- the periodic
# in-run check, the renewal, and the post-completion re-check) tolerates slow checks until
# the last observed lease expiry is one check window away, never for a fixed budget. Basis:
# the fixed 2x LEADER_CHECK_TIMEOUT budget it replaces cancelled run w2-lgc-e1e4
# (x-harness-x-model-bench, 2026-10-05) at 1,298 s with 833 s of lease left, under one full
# pytest suite plus two worker gates; and before the observed expiry no other session can
# take the designation (leader_decide refuses pin/reclaim of a live lease with
# COORD-LEADER-HELD), so slowness alone cannot hide a new holder. Only the holder's own
# release can, and that is seen at the next check that answers. Wrong if a designation can
# change holder before its expires_at by any path other than the holder's release -- then a
# slow check could keep an in-flight turn running for up to one lease without authority.
# Prompts and approvals still need a positive answer inside their own deadline (worker_fence).


def housekeeping_check(check, lease, epoch):
    """One bounded leader check for a caller with no operation deadline (RUN-C).

    True while authority stands. A definitive refusal or a changed epoch ends it at once. A
    check that only ran slow (LeaderCheckSlow) is not lost authority while the last observed
    lease expiry is more than one check window away; the caller checks again on its next
    tick, so the dispatch loop never blocks for longer than one check. `check(window)` returns
    a leader row, or None for a renewal, which carries no row."""
    try:
        row = check(min(LEADER_CHECK_TIMEOUT, lease["expires_at"] - time.time()))
    except LeaderCheckSlow:
        return time.time() + LEADER_CHECK_TIMEOUT < lease["expires_at"]
    except (Refused, OSError, ValueError):
        return False
    if row is not None:
        if row["epoch"] != epoch:
            return False
        lease["expires_at"] = row["expires_at"]
    return True


def load_module(name, filename):
    spec = importlib.util.spec_from_file_location(name, HERE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


core = load_module("runner_core", "coord-core.py")
compiler = load_module("runner_compiler", "prompt-compile.py")
gate = load_module("runner_compile_gate", "verify-compiled-prompt.py")


class Refused(Exception):
    def __init__(self, code, remedy):
        self.code, self.remedy = code, remedy
        super().__init__(code)


class LeaderCheckSlow(Refused):
    """A bounded leader-check subprocess did not finish inside its timeout. Carries the
    same RUN-LEADER code as any other refusal, so an unaware caller behaves exactly as
    before; only Runner.leader_retrying tells this apart, to retry while a check's
    deadline still allows it instead of treating load as lost authority (RUN-C)."""

    def __init__(self, remedy):
        super().__init__("RUN-LEADER", remedy)


def require(condition, code, remedy):
    if not condition:
        raise Refused(code, remedy)


def encoded(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=True, allow_nan=False).encode("utf-8")


def digest(value):
    return hashlib.sha256(encoded(value)).hexdigest()


@contextlib.contextmanager
def interruption():
    """An owned child gets a cleanup opportunity when the attachment CLI is stopped."""
    stop = threading.Event()
    previous = {}
    for signum in (signal.SIGINT, signal.SIGTERM):
        previous[signum] = signal.signal(signum, lambda _signum, _frame: stop.set())
    try:
        yield stop
    finally:
        for signum, handler in previous.items():
            signal.signal(signum, handler)


def read_json(path):
    if os.name == "nt":
        from coord_files import read_regular
        data = read_regular(path, MAX_DOCUMENT)
    else:
        with open(path, "rb") as handle:
            data = handle.read(MAX_DOCUMENT + 1)
    require(len(data) <= MAX_DOCUMENT, "RUN-INPUT", "Use a JSON document below 2 MiB.")
    value = json.loads(data)
    require(isinstance(value, dict), "RUN-INPUT", "Use a JSON object.")
    return value


def private_write(path, value):
    with contextlib.ExitStack() as stack:
        if os.name == "nt":
            from coord_files import pinned_directory, protect_private_directory
            stack.enter_context(pinned_directory(path.parent))
            protect_private_directory(path.parent)
        with open(os.open(str(path), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600),
                  "w", encoding="utf-8", newline="\n") as handle:
            handle.write(encoded(value).decode("utf-8") + "\n")
            handle.flush()
            os.fsync(handle.fileno())


def git(cwd, *args):
    result = subprocess.run(["git", *args], cwd=str(cwd), capture_output=True,
                            text=True, encoding="utf-8", errors="replace", timeout=10)
    require(result.returncode == 0, "RUN-GIT", "Resolve the repository/index condition and retry with a new run id.")
    return result.stdout.strip()


def dispatch_base(cwd, contract):
    """The commit every worker tree starts from: the contract's `base` (a branch, tag or
    commit) when it names one, else the invoking checkout's HEAD.

    BASE-A (x-harness-x-model-bench, 2026-10-04): with HEAD as the only base, a dirty primary
    froze every dispatch, because the integration head was a branch the primary could not
    fast-forward to. The base is resolved once, here, and pinned in the manifest as a sha."""
    if "base" not in contract:
        return git(cwd, "rev-parse", "HEAD")
    base = contract["base"]
    require(text(base) and len(base) <= 256 and not base.startswith("-")
            and not any(ord(c) < 32 for c in base), "RUN-BASE",
            "Name the dispatch base as a branch, tag or commit (not an option).")
    result = subprocess.run(["git", "rev-parse", "--verify", "--quiet", base + "^{commit}"], cwd=str(cwd),
                            capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=10)
    require(result.returncode == 0 and result.stdout.strip(), "RUN-BASE",
            "Name a dispatch base that resolves to a commit in this repository.")
    return result.stdout.strip()


def identity(value):
    require(isinstance(value, str) and len(value) <= 80 and not core.session_id_error(value),
            "RUN-IDENTITY", "Use a unique portable session/run id of at most 80 characters.")


def text(value):
    return isinstance(value, str) and bool(value.strip())


def integer(value, minimum, maximum):
    return type(value) is int and minimum <= value <= maximum


def copilot_model(argv):
    forbidden = {"--allow-all", "--allow-all-tools", "--allow-all-paths", "--allow-all-urls",
                 "--yolo", "--assisted-approval", "--resume", "--continue", "--connect"}
    require("--acp" in argv and not any(arg.split("=", 1)[0] in forbidden for arg in argv),
            "RUN-COPILOT-PROFILE", "Use a fresh native --acp profile without approval bypass or implicit resume.")
    models = [arg.split("=", 1)[1] for arg in argv if arg.startswith("--model=")]
    for index, arg in enumerate(argv):
        if arg == "--model":
            require(index + 1 < len(argv), "RUN-COPILOT-MODEL", "Pin one explicit model.")
            models.append(argv[index + 1])
    require(len(models) == 1 and text(models[0]) and models[0] != "auto"
            and not models[0].startswith("-"), "RUN-COPILOT-MODEL",
            "Pin one explicit model; defaults and automatic routing are not qualified.")
    return models[0]


def expected_model(worker):
    """The model the transport must select and confirm before any prompt, or None.

    SERVE-A (x-harness-x-model-bench run w2-g3-e1e4): a Grok argv pin (-m / --model) was never
    sent as session/set_model, so Grok's ACP default answered instead. An unpinned Grok worker
    keeps the earlier behaviour; an ambiguous pin is refused."""
    if worker["harness"] == "copilot":
        return copilot_model(worker["argv"])
    if worker["harness"] != "grok":
        return None
    argv = worker["argv"]
    models = [arg.split("=", 1)[1] for arg in argv if arg.startswith("--model=")]
    for index, arg in enumerate(argv):
        if arg in ("-m", "--model"):
            models.append(argv[index + 1] if index + 1 < len(argv) else "")
    if not models:
        return None
    require(len(models) == 1 and text(models[0]) and not models[0].startswith("-"),
            "RUN-GROK-MODEL", "Pin one explicit Grok model with -m or --model.")
    return models[0]


def copilot_model_evidence(session_id, env, expected):
    """Check actual native inference events, never the advertised ACP model list."""
    from coord_files import read_regular
    try:
        require(str(uuid.UUID(session_id)) == session_id, "RUN-COPILOT-MODEL",
                "Native model evidence requires the exact Copilot session UUID.")
        home = Path(env.get("COPILOT_HOME") or Path.home() / ".copilot")
        require(home.is_absolute(), "RUN-COPILOT-MODEL", "Use an absolute native state directory.")
        data = read_regular(home / "session-state" / session_id / "events.jsonl", 16 * 1024 * 1024)
        rows = [json.loads(line) for line in data.splitlines() if line.strip()]
        require(all(isinstance(row, dict) and isinstance(row.get("data"), dict) for row in rows),
                "RUN-COPILOT-MODEL", "Native inference records are malformed.")
        messages = [row["data"].get("model") for row in rows if row.get("type") == "assistant.message"]
        usage = set()
        for row in rows:
            if row.get("type") == "session.usage_checkpoint":
                for checkpoint in row["data"].get("promptCacheBreakState", []):
                    usage.update(checkpoint.get("models", {}))
        require(bool(messages) and set(messages) == {expected} and usage == {expected},
                "RUN-COPILOT-MODEL",
                "Actual assistant and usage models must both match the admitted model; metadata/cache state is insufficient.")
        return {"actual_models": sorted(usage), "source": "native-assistant-and-usage-events",
                "messages": len(messages), "sha256": hashlib.sha256(data).hexdigest()}
    except (OSError, ValueError, TypeError, AttributeError):
        raise Refused("RUN-COPILOT-MODEL", "Actual native model evidence is missing or invalid; do not qualify this attempt.") from None


def copilot_policy(cwd, model):
    """Require the native exact-ID policy before preparation and fingerprinting."""
    from coord_files import read_regular
    policy = Path(cwd) / ".github" / "allowed_models.txt"
    try:
        policy.resolve().relative_to(Path(cwd).resolve())
        lines = [line.strip() for line in read_regular(policy, 8192).decode("utf-8").splitlines()
                 if line.strip()]
    except (OSError, ValueError):
        raise Refused("RUN-COPILOT-POLICY",
                      "Commit a local .github/allowed_models.txt exact-ID policy with a fallback before preparing Copilot.") from None
    fallback = [line.partition(":")[2].strip() for line in lines if line.startswith("fallback:")]
    models = {line for line in lines if not line.startswith("fallback:")}
    require(len(fallback) == 1 and fallback[0] in models and model in models and "auto" not in models
            and all(re.fullmatch(r"[A-Za-z0-9_.-]+", item) for item in models),
            "RUN-COPILOT-POLICY", "Use exact native model IDs plus one allowed fallback; provider overrides are not this profile.")


def relative_path(value):
    return (text(value) and not Path(value).is_absolute() and "\\" not in value
            and not any(part in ("..", ".git") for part in Path(value).parts)
            and str(Path(value)) != ".")


def child_env(worker):
    env = dict(os.environ, AGENT_SESSION=worker["session"], AGENT_HOST=worker["harness"],
               AGENT_WI=worker["session"], PWD=worker["worktree"])
    # The external Claude adapter is its own session, not a nested native CLI turn.
    env.pop("CLAUDECODE", None)
    env.pop("AGENT_NAME", None)
    if worker["harness"] == "copilot":
        env["COPILOT_MODEL"] = copilot_model(worker["argv"])
    return env


def file_hash(path):
    if not path.exists():
        return "missing"
    require(path.is_file(), "RUN-BINDING", "Bind regular executable/configuration files, not directories.")
    sha = hashlib.sha256()
    total = 0
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            total += len(chunk)
            require(total <= 512 * 1024 * 1024, "RUN-BINDING", "Bind files below 512 MiB.")
            sha.update(chunk)
    return sha.hexdigest()


class Runner:
    def __init__(self, cwd):
        self.cwd = Path(git(cwd, "rev-parse", "--show-toplevel")).resolve()
        self.repo = Path(core.repo_root(self.cwd)).resolve()
        self.root, error = core.resolve_root(self.cwd, os.environ.get("COORD_ROOT"))
        require(not error, "RUN-ROOT", "Use the repository's coordination root.")
        common = Path(git(self.cwd, "rev-parse", "--git-common-dir"))
        self.common = (self.cwd / common).resolve()
        self.lock = threading.Lock()

    def directory(self, run_id):
        identity(run_id)
        parent = self.common / "coord-runs"
        require(not parent.is_symlink(), "RUN-STORE", "Use an ordinary common-git run directory.")
        result = parent / run_id
        require(not result.is_symlink(), "RUN-STORE", "Inspect the existing run directory; do not follow a symlink.")
        return result

    def event(self, manifest, state, worker=None, **fields):
        row = {"kind": "runner", "session": manifest["owner"], "agent": manifest["owner"],
               "wi": manifest["run_id"], "path": "-", "at": time.time(),
               "run_id": manifest["run_id"], "state": state, "worker": worker,
               "usage": "not recorded", "cost": "not recorded", **fields}
        with self.lock:
            core.append_event(self.root, row)
            print(json.dumps(row, sort_keys=True), flush=True)

    def compiled_prompts(self, ids, session):
        path = self.cwd / "docs/audit/audit-log.jsonl"
        entries = {}
        with path.open(encoding="utf-8") as handle:
            for line in handle:
                if line.strip():
                    row = json.loads(line)
                    entries[row.get("id")] = row
        prompts = []
        for entry_id in ids:
            entry = entries.get(entry_id, {})
            doc = entry.get("compiled")
            require(entry.get("kind") == "compilation" and entry.get("dispatchable") is True
                    and isinstance(doc, dict) and not compiler.check_schema(doc)
                    and doc.get("dispatchable") is True and doc.get("mode") != "not-compiled"
                    and not doc.get("decision_requests"), "RUN-COMPILE",
                    "Finish a verified, dispatchable compilation with no unanswered decision requests.")
            raw = entries.get(doc.get("raw_id"), {})
            require(raw.get("kind") == "prompt" and not gate.verify_document(doc, raw.get("prompt", "")),
                    "RUN-COMPILE", "Repair the compilation against its original audit prompt.")
            # A compilation's rendered `prompt` may include native launcher instructions
            # (Codex's template does). Dispatch only deterministically rendered verified
            # sections, never a separately mutable rendition or a nested launch command.
            prompt = (("python" if os.name == "nt" else "python3")
                      + " docs/ai-forward-pack/scripts/audit-log.py start --session "
                      + session + " --skill coordination-worker\n"
                      + "\n\n".join(compiler.render_sections(doc).values())
                      + "\nWork only in your assigned cwd. Follow the plan's owned paths and return evidence to the Owner.\n")
            require(text(prompt) and len(prompt.encode("utf-8")) <= 256 * 1024, "RUN-COMPILE",
                    "Use a nonempty compiled prompt below 256 KiB.")
            prompts.append(prompt)
        return prompts

    def access_roots(self, worker, owner, preparing=False):
        if "additional_roots" not in worker:
            return []
        require(worker.get("harness") == "codex" and worker.get("transport") == "acp", "RUN-ROOTS",
                "Explicit operational file roots are supported only by the qualified Codex ACP profile.")
        paths = worker["additional_roots"]
        expected_log = str(self.root / "log" / (worker["session"] + ".jsonl"))
        allowed = {str(self.root / "requests.jsonl"), expected_log,
                   str(self.root / "mail" / (owner + ".jsonl"))}
        try:
            identities = file_root_identities(paths, expected_log if preparing else None)
        except (OSError, ValueError):
            raise Refused("RUN-ROOTS", "Use up to three unique canonical existing regular operational files; no symlinks.") from None
        require(set(paths) <= allowed, "RUN-ROOTS",
                "Allow only the primary request ledger, this worker log and this Owner inbox.")
        return identities

    def validate(self, contract):
        require(contract.get("schema") == "coord-run/1", "RUN-CONTRACT", "Use schema coord-run/1.")
        identity(contract.get("run_id"))
        identity(contract.get("owner"))
        require(os.environ.get("AGENT_SESSION") == contract["owner"], "RUN-OWNER",
                "Invoke from the declared Owner session (AGENT_SESSION).")
        workers = contract.get("workers")
        require(isinstance(workers, list) and 1 <= len(workers) <= 8
                and integer(contract.get("parallelism"), 1, 4), "RUN-CONTRACT",
                "Declare 1–8 workers and parallelism 1–4.")
        seen, branches = {contract["owner"]}, set()
        events, errors, _ = core.read_events(self.root)
        require(not errors, "RUN-STORE", "Repair unreadable coordination state before preparing new workers.")
        used_sessions = {e.get("session", "").casefold() for e in events if isinstance(e.get("session"), str)}
        prepared = []
        for original in workers:
            require(isinstance(original, dict), "RUN-CONTRACT", "Each worker must be an object.")
            worker = dict(original)
            if "runtime" in worker:
                policy = worker["runtime"]
                require(isinstance(policy, dict) and set(policy) - {"mode_id"} == {
                    "unattended", "mailbox", "max_turns", "max_retries", "permissions"}
                    and policy["unattended"] is True and type(policy["mailbox"]) is bool
                    and integer(policy["max_turns"], 1, 8) and integer(policy["max_retries"], 0, 2)
                    and policy["permissions"] in ("deny", "ask"), "RUN-RUNTIME",
                    "Explicitly enable unattended:true, declare mailbox, 1–8 total turns, 0–2 startup retries and deny/ask permissions.")
                require(not (original.get("harness") == "agy" and policy["permissions"] == "ask"),
                        "RUN-PERMISSION-UNSUPPORTED", "Agy headless has no interactive approval response; use native interactive approval or deny mode.")
                require("mode_id" not in policy or (original.get("transport") == "acp"
                        and text(policy["mode_id"]) and len(policy["mode_id"]) <= 256),
                        "RUN-RUNTIME", "Select a literal advertised ACP fresh-session mode.")
            require("additional_root_identities" not in worker, "RUN-ROOTS",
                    "Access identities are derived during preparation, never supplied by the caller.")
            identity(worker.get("session"))
            require(worker["session"].casefold() not in used_sessions, "RUN-IDENTITY",
                    "Each new worker attempt needs a new session identity, including after a prior run stopped.")
            require(worker["session"].casefold() not in {s.casefold() for s in seen}, "RUN-IDENTITY",
                    "Use distinct worker identities, different from the Owner.")
            seen.add(worker["session"])
            branch = worker.get("branch")
            require(text(branch) and not branch.startswith("-") and branch.casefold() not in branches,
                    "RUN-BRANCH", "Use distinct new work-named git branches.")
            git(self.cwd, "check-ref-format", "--branch", branch)
            branches.add(branch.casefold())
            require(not git(self.cwd, "for-each-ref", "--format=%(refname)", "refs/heads/" + branch),
                    "RUN-BRANCH", "Select a new branch; existing worker branches are never reused.")
            require(worker.get("harness") in ("claude", "codex", "grok", "agy", "copilot") and
                    worker.get("transport") == ("agy" if worker.get("harness") == "agy" else "acp"),
                    "RUN-TRANSPORT", "Select ACP for Claude/Codex/Grok/Copilot or the explicit Agy native transport.")
            self.access_roots(worker, contract["owner"], preparing=True)
            argv = worker.get("argv")
            require(isinstance(argv, list) and 1 <= len(argv) <= 32
                    and all(text(v) and "\x00" not in v and ("{worktree}" not in v or v == "{worktree}") for v in argv)
                    and argv.count("{worktree}") <= 1 and len(encoded(argv)) <= 65536,
                    "RUN-ARGV", "Supply an installed executable and an explicit argument array, without a shell.")
            if worker["harness"] == "copilot":
                copilot_policy(self.cwd, copilot_model(argv))
            if worker["transport"] == "agy":
                require(all(flag in argv for flag in ("--add-dir", "--input-format", "--output-format"))
                        and argv[argv.index("--add-dir") + 1:][:1] == ["{worktree}"]
                        and all(argv[argv.index(flag) + 1:][:1] == ["stream-json"]
                                for flag in ("--input-format", "--output-format")), "RUN-ARGV",
                        "Agy requires --add-dir {worktree} --input-format stream-json --output-format stream-json.")
            require(integer(worker.get("deadline_seconds"), 1, 3600)
                    and integer(worker.get("output_limit"), 1024, 16 * 1024 * 1024)
                    and text(worker.get("fallback")), "RUN-BOUNDS", "Declare finite time/byte bounds and a fallback.")
            caps = worker.get("required_capabilities")
            require(isinstance(caps, dict) and CAPABILITIES <= caps.keys()
                    and all(text(k) and v in ("observed-only", "enforced") for k, v in caps.items()),
                    "RUN-CAPABILITY", "Explicitly require worktree_isolation, instructions, hooks and permissions.")
            bindings = worker.get("binding_files")
            require(isinstance(bindings, list) and bindings and all(text(p) for p in bindings),
                    "RUN-BINDING", "List all effective instruction, hook, trust, permission and adapter configuration files.")
            if worker["harness"] == "copilot" and ".github/allowed_models.txt" not in bindings:
                worker["binding_files"] = [*bindings, ".github/allowed_models.txt"]
            evidence = worker.get("evidence")
            require(isinstance(evidence, list) and 1 <= len(evidence) <= 32,
                    "RUN-EVIDENCE", "Declare 1–32 file and/or commit evidence items.")
            for item in evidence:
                require(isinstance(item, dict) and (item.get("kind") == "commit" or
                        (item.get("kind") == "file" and relative_path(item.get("path"))
                         and integer(item.get("max_bytes"), 1, 16 * 1024 * 1024))),
                        "RUN-EVIDENCE", "Use commit or bounded file evidence within the worker checkout.")
            ids = worker.get("prompts")
            require(isinstance(ids, list) and 1 <= len(ids) <= 8 and all(text(i) for i in ids),
                    "RUN-COMPILE", "Supply 1–8 completed compilation audit IDs.")
            worker["prompt_texts"] = self.compiled_prompts(ids, worker["session"])
            require("runtime" not in worker or worker["runtime"]["max_turns"] >= len(ids),
                    "RUN-RUNTIME", "The total turn bound must include all initial prompts.")
            prepared.append(worker)
        # Headroom accommodates actual checkout/executable paths and immutable metadata.
        require(len(encoded(prepared)) <= 512 * 1024, "RUN-INPUT",
                "Keep the aggregate admitted worker/brief data below 512 KiB; split a larger plan into explicit runs.")
        return prepared

    def prepare(self, contract):
        workers = self.validate(contract)
        require(not git(self.cwd, "ls-files", "-u"), "RUN-INDEX", "Resolve the invoking checkout's unmerged index first.")
        manifest = {"schema": "coord-prepared/1", "run_id": contract["run_id"], "owner": contract["owner"],
                    "parallelism": contract["parallelism"], "base": dispatch_base(self.cwd, contract),
                    "contract_sha256": digest(contract), "workers": workers, "created_at": time.time()}
        directory = self.directory(manifest["run_id"])
        directory.parent.mkdir(mode=0o700, exist_ok=True)
        require(not directory.exists(), "RUN-EXISTS", "Inspect status and retained worktrees; use a new run id for a new attempt.")
        directory.mkdir(mode=0o700)
        private_write(directory / "contract.json", contract)
        self.event(manifest, "preparing", contract_sha256=digest(contract))
        try:
            reservations = self.common / "coord-run-sessions"
            require(not reservations.is_symlink(), "RUN-STORE", "Inspect the session reservation directory.")
            reservations.mkdir(mode=0o700, exist_ok=True)
            for worker in workers:
                try:
                    private_write(reservations / worker["session"].casefold(), {"run_id": manifest["run_id"]})
                except FileExistsError:
                    raise Refused("RUN-IDENTITY", "A prepared attempt already reserved this worker identity; use a new identity.")
            for worker in workers:
                with contextlib.redirect_stdout(io.StringIO()):
                    result = core.cmd_worktree(self.root, self.repo, "new", self.cwd, time.time(),
                        session=worker["session"], agent=worker["session"], branch=worker["branch"], base=manifest["base"])
                require(result == 0, "RUN-WORKTREE", "Inspect the actual git worktree inventory; retain partial work and use a new run id.")
                if "additional_roots" in worker:
                    worker["additional_root_identities"] = self.access_roots(worker, manifest["owner"])
                inventory, error = core.worktree_inventory(self.repo)
                rows = [r for r in inventory or [] if r.get("branch") == worker["branch"]]
                require(not error and len(rows) == 1, "RUN-WORKTREE", "Resolve the worker's unique branch/worktree mapping.")
                worker["worktree"] = str(Path(rows[0]["path"]).resolve())
                require(Path(worker["worktree"]) != self.repo and rows[0].get("head") == manifest["base"],
                        "RUN-WORKTREE", "A worker must have a separate checkout at the invoking base commit.")
                worker["argv"] = [arg.replace("{worktree}", worker["worktree"]) for arg in worker["argv"]]
                private_write(directory / (worker["session"] + ".brief.json"), {"prompts": worker["prompt_texts"], "fallback": worker["fallback"]})
                self.event(manifest, "worker_prepared", worker=worker["session"], branch=worker["branch"],
                           worktree=worker["worktree"], manual_brief=str(directory / (worker["session"] + ".brief.json")))
                worker["argv"][0] = self.resolve_executable(worker)
            private_write(directory / "manifest.json", manifest)
            self.event(manifest, "prepared", manifest_sha256=digest(manifest), base=manifest["base"])
        except Exception:
            self.event(manifest, "prepare_failed", code="RUN-PREPARE", remedy="Inspect retained trees and contract; no automatic retry.")
            raise
        return self.public_manifest(manifest)

    def public_manifest(self, manifest):
        return {"run_id": manifest["run_id"], "state": "prepared", "base": manifest["base"],
                "workers": [{"session": w["session"], "worktree": w["worktree"], "branch": w["branch"],
                             "manual_brief": str(self.directory(manifest["run_id"]) / (w["session"] + ".brief.json"))}
                            for w in manifest["workers"]]}

    def load(self, run_id, allow_partial=False):
        if allow_partial and not (self.directory(run_id) / "manifest.json").exists():
            contract = read_json(self.directory(run_id) / "contract.json")
            require(contract.get("run_id") == run_id and text(contract.get("owner")),
                    "RUN-STORE", "Inspect the retained partial preparation contract.")
            return {"schema": "coord-partial/1", "run_id": run_id, "owner": contract["owner"],
                    "workers": contract.get("workers", []), "contract_sha256": digest(contract)}
        manifest = read_json(self.directory(run_id) / "manifest.json")
        require(manifest.get("schema") == "coord-prepared/1" and manifest.get("run_id") == run_id,
                "RUN-STORE", "Inspect the immutable prepared manifest; do not reconstruct an attempt by guessing.")
        rows, errors, _ = core.read_events(self.root)
        admissions = [r for r in rows if r.get("kind") == "runner" and r.get("run_id") == run_id
                      and r.get("state") == "prepared" and r.get("session") == manifest["owner"]]
        require(not errors and len(admissions) == 1 and admissions[0].get("manifest_sha256") == digest(manifest),
                "RUN-STORE", "Prepared manifest must match its recorded admission hash.")
        return manifest

    def controls(self, manifest, session):
        from coord_runtime import Controls
        require(any(w["session"] == session for w in manifest["workers"]),
                "RUN-IDENTITY", "Select an admitted worker identity.")
        return Controls(self.directory(manifest["run_id"]) / (session + ".controls"))

    def control(self, manifest, command, session, compilation=None, request=None, option=None):
        owner = manifest["owner"]
        require(os.environ.get("AGENT_SESSION") == owner, "RUN-OWNER", "Invoke from the admitted Owner session.")
        worker = next((w for w in manifest["workers"] if w["session"] == session), None)
        require(worker is not None and "runtime" in worker, "RUN-RUNTIME", "Select a worker with an explicit runtime policy.")
        box = self.controls(manifest, session)
        if command == "permissions":
            return {"run_id": manifest["run_id"], "worker": session, "requests": [
                {"id": r["id"], "state": r["state"], "expires_at": r["expires_at"],
                 "detail_path": str(box.directory / (r["id"] + ".json"))} for r in box.pending()]}
        if command == "permission-show":
            rows = [r for r in box.records() if r["kind"] == "permission" and r["id"] == request]
            require(len(rows) == 1, "RUN-PERMISSION", "Select an exact pending permission request ID.")
            return rows[0]
        leader = self.leader(owner)
        started = self.directory(manifest["run_id"]) / "started.json"
        if started.exists():
            require(read_json(started).get("epoch") == leader["epoch"], "RUN-LEADER", "The original leader epoch no longer owns this run.")
        require(self.status(manifest)["state"] in ("prepared", "interrupted_or_running"),
                "RUN-FINISHED", "The run has finished; retain its controls as history.")
        events, errors, _ = core.read_events(self.root)
        require(not errors and not any(e.get("kind") == "runner" and e.get("run_id") == manifest["run_id"]
                and e.get("session") == owner and e.get("worker") == session and e.get("state") == "worker_finished"
                for e in events), "RUN-FINISHED", "This worker has finished; its controls are retained as history.")
        if command == "enqueue":
            require(worker["runtime"]["mailbox"], "RUN-MAILBOX", "Mailbox input was not admitted in this manifest.")
            prompt = self.compiled_prompts([compilation], session)[0]
            require(compilation not in worker["prompts"], "RUN-COMPILE", "A prompt already present in the initial contract cannot be replayed.")
            record = box.enqueue(compilation, digest(prompt), worker["runtime"]["max_turns"] - len(worker["prompts"]))
        elif command == "finish":
            require(worker["runtime"]["mailbox"], "RUN-MAILBOX", "Mailbox input was not admitted in this manifest.")
            record = box.finish()
        else:
            require(command == "permission-decide" and worker["runtime"]["permissions"] == "ask"
                    and started.exists(), "RUN-PERMISSION", "Interactive approval requires an active ask-mode worker.")
            record = box.decide(request, option)
        self.event(manifest, "control_admitted", worker=session, control_id=record["id"],
                   control_kind=record["kind"], control_sha256=record["sha256"])
        return {"state": "queued", "control_id": record["id"], "kind": record["kind"]}

    def attach(self, args):
        """Send one compiled input through a native, already addressable live backend."""
        with interruption() as stop:
            return self.attach_owned(args, stop)

    def attach_owned(self, args, stop):
        require(os.name == "posix", "RUN-PLATFORM", "Native attachment is qualified only for the POSIX pilot.")
        owner = os.environ.get("AGENT_SESSION", "")
        identity(owner)
        identity(args.worker)
        identity(args.delivery_id)
        require(args.harness in ("codex", "grok"), "RUN-ATTACH-UNSUPPORTED",
                "Live input is qualified for addressable Codex app-server and Grok leader sessions; use native interactive control for this harness.")
        require(str(uuid.UUID(args.native_session)) == args.native_session, "RUN-ATTACH-IDENTITY",
                "Supply the exact canonical native session UUID, never a display name.")
        admission = self.leader(owner)
        cwd = Path(args.cwd).resolve()
        inventory, error = core.worktree_inventory(self.repo)
        sessions, errors, _ = core.active_sessions(self.root, time.time())
        require(not error and not errors and cwd != self.repo and any(Path(r["path"]).resolve() == cwd for r in inventory or [])
                and any(s["session"] == args.worker and s["worktree"] == cwd.name for s in sessions),
                "RUN-ATTACH-WORKTREE", "Register the existing worker in its own repository worktree before attachment.")
        matches = [r for r in inventory if Path(r["path"]).resolve() == cwd]
        require(len(matches) == 1 and text(matches[0].get("branch")), "RUN-ATTACH-WORKTREE", "The registered checkout must have a branch.")
        worker = {"session": args.worker, "harness": args.harness, "worktree": str(cwd),
                  "branch": matches[0]["branch"], "argv": [args.executable]}
        def checkout_identity(remaining):
            result = run_bounded([sys.executable, str(HERE / "coord-runner.py"), "_attach_identity",
                "--worker", args.worker, "--cwd", str(cwd), "--branch", worker["branch"]],
                cwd=self.cwd, env=dict(os.environ), timeout_seconds=min(2, max(.001, remaining)),
                stdout_limit=8192, stderr_limit=2048, cancelled=stop.is_set)
            require(result.returncode == 0 and result.contained and not result.timed_out
                    and not result.limit_exceeded and not result.cancelled, "RUN-ATTACH-WORKTREE",
                    "The actual checkout no longer matches its registered repository and branch.")
        checkout_identity(2)
        socket = Path(args.socket)
        require(socket.is_absolute() and str(socket.resolve()) == str(socket), "RUN-ATTACH-SOCKET",
                "Supply the canonical path of an already-running local native socket.")
        info = socket.lstat()
        require(stat.S_ISSOCK(info.st_mode) and info.st_uid == os.getuid(), "RUN-ATTACH-SOCKET",
                "Use an existing native socket owned by the current OS user.")
        socket_identity = (info.st_dev, info.st_ino)
        executable = self.resolve_executable(worker)
        prompt = self.compiled_prompts([args.compilation], args.worker)[0]
        manifest = {"owner": owner, "run_id": "attach-" + args.delivery_id}
        directory = self.directory(manifest["run_id"])
        directory.parent.mkdir(mode=0o700, exist_ok=True)
        require(not directory.exists(), "RUN-ATTACH-REPLAY", "This delivery ID was already attempted; inspect native state, never replay blindly.")
        directory.mkdir(mode=0o700)
        private_write(directory / "attachment.json", {"owner": owner, "epoch": admission["epoch"],
            "worker": args.worker, "native_session": args.native_session, "cwd": str(cwd),
            "harness": args.harness, "socket": str(socket), "socket_identity": socket_identity,
            "compilation": args.compilation, "prompt_sha256": digest(prompt)})

        def admitted(remaining):
            if stop.is_set():
                return False
            started_check = time.monotonic()
            checkout_identity(remaining)
            row = self.leader(owner, timeout=min(LEADER_CHECK_TIMEOUT, remaining - (time.monotonic() - started_check)))
            current = socket.lstat()
            return row["epoch"] == admission["epoch"] and time.time() < row["expires_at"] and (
                current.st_dev, current.st_ino) == socket_identity and stat.S_ISSOCK(current.st_mode)

        require(admitted(LEADER_CHECK_TIMEOUT), "RUN-LEADER", "Live authority and socket identity must remain unchanged.")
        self.event(manifest, "attachment_started", worker=args.worker, harness=args.harness)
        try:
            if args.harness == "codex":
                from coord_native import thread_metadata
                native = thread_metadata(socket, args.native_session, timeout=5, cancelled=stop.is_set)
                require(Path(native["cwd"]).resolve() == cwd, "RUN-ATTACH-CWD",
                        "The native thread belongs to a different checkout; do not queue this delegation.")
                require(admitted(5), "RUN-LEADER", "Authority, checkout or endpoint changed before native queue admission.")
                result = run_bounded([executable, "queue", "--remote", "unix://" + str(socket),
                    "--thread", args.native_session, "--message", prompt], cwd=cwd, env=child_env(worker),
                    timeout_seconds=30, stdout_limit=8192, stderr_limit=2048, cancelled=stop.is_set)
                accepted = result.returncode == 0 and not result.timed_out and not result.limit_exceeded and not result.cleanup_error and result.contained and not stop.is_set()
                public = {"state": "queued" if accepted else "indeterminate", "code": "RUN-ATTACH-QUEUED" if accepted else "RUN-ATTACH-FAILED"}
            else:
                from coord_transport import run_session
                result = run_session("acp", [executable, "agent", "--leader", "--leader-socket", str(socket), "stdio"],
                    str(cwd), child_env(worker), [prompt], 120, 4 * 1024 * 1024,
                    lambda event: self.event(manifest, "progress", worker=args.worker, observation=event),
                    stop.is_set, admitted, session_id=args.native_session, max_turns=1, require_loaded_cwd=True)
                public = {"state": "turn_complete" if result["outcome"] == "complete" else "indeterminate",
                          "code": result["code"], "transport": result}
        except (OSError, ValueError, Refused) as exc:
            public = {"state": "indeterminate", "code": exc.code if isinstance(exc, Refused) else "RUN-ATTACH-METADATA"}
        public.update(delivery_id=args.delivery_id, worker=args.worker, completion="Native lifecycle only; inspect work evidence separately",
                      qualification="Attachment does not qualify or transfer ownership of the existing session")
        private_write(directory / "result.json", public)
        self.event(manifest, "attachment_finished", worker=args.worker, result=public)
        return public

    def resolve_executable(self, worker):
        """Resolve using the child's cwd, including relative executable/PATH entries."""
        cwd = Path(worker["worktree"])
        name = worker["argv"][0]
        env = child_env(worker)
        if os.sep in name:
            candidate = Path(name)
            path = candidate if candidate.is_absolute() else cwd / candidate
            executable = str(path) if path.is_file() and os.access(path, os.X_OK) else None
        else:
            search_path = os.pathsep.join(str(Path(p) if Path(p).is_absolute() else cwd / p)
                                          for p in env.get("PATH", os.defpath).split(os.pathsep))
            executable = shutil.which(name, path=search_path)
        require(executable is not None, "RUN-EXECUTABLE", "Install the selected adapter or continue with the compiled manual brief; no model launched.")
        return str(Path(executable).resolve())

    def fingerprint(self, manifest, worker):
        if worker["harness"] == "copilot":
            copilot_policy(worker["worktree"], copilot_model(worker["argv"]))
        require(self.access_roots(worker, manifest["owner"]) == worker.get("additional_root_identities", []),
                "RUN-ROOTS", "An admitted operational file was replaced; prepare and qualify a new attempt.")
        env = child_env(worker)
        self.worker_identity(manifest, worker)
        executable = self.resolve_executable(worker)
        files = {}
        for value in worker["binding_files"]:
            path = Path(value)
            if not path.is_absolute():
                path = Path(worker["worktree"]) / path
            files[str(path.resolve())] = file_hash(path)
        return digest({"manifest_sha256": digest(manifest), "worker": worker,
                       "executable": str(Path(executable).resolve()), "executable_sha256": file_hash(Path(executable)),
                       "binding_files": files, "environment_sha256": digest(env)})

    def fingerprints(self, manifest):
        return {"run_id": manifest["run_id"], "qualification": "not performed",
                "fingerprints": {w["session"]: self.fingerprint(manifest, w) for w in manifest["workers"]}}

    def bounded_profile(self, manifest, worker, remaining, retry=False):
        require(remaining > 0, "RUN-BOUNDS", "The original attempt deadline expired.")
        command = [sys.executable, str(HERE / "coord-runner.py"), "_profile", "--run", manifest["run_id"],
                   "--worker", worker["session"]]
        if retry:
            command.append("--clean")
        result = run_bounded(command, cwd=self.cwd, env=dict(os.environ),
                             timeout_seconds=min(2, remaining), stdout_limit=8192, stderr_limit=2048)
        require(result.returncode == 0 and not result.timed_out and not result.limit_exceeded
                and not result.cleanup_error and result.contained, "RUN-QUALIFICATION",
                "Profile or clean-retry check failed within its bound; inspect the retained attempt.")
        return json.loads(result.stdout)["fingerprint"]

    def bounded_prompt(self, manifest, worker, compilation, remaining):
        require(remaining > 0, "RUN-BOUNDS", "The original attempt deadline expired.")
        result = run_bounded([sys.executable, str(HERE / "coord-runner.py"), "_prompt", "--run", manifest["run_id"],
                              "--worker", worker["session"], "--compilation", compilation],
                             cwd=self.cwd, env=dict(os.environ), timeout_seconds=min(2, remaining),
                             stdout_limit=MAX_DOCUMENT, stderr_limit=2048)
        require(result.returncode == 0 and not result.timed_out and not result.limit_exceeded
                and not result.cleanup_error and result.contained, "RUN-COMPILE",
                "Queued compilation could not be verified within its budget.")
        return json.loads(result.stdout)["prompt"]

    def leader(self, owner, timeout=2, renew_run=None):
        require(timeout > 0, "RUN-LEADER", "Lease or attempt deadline expired; retain the worker evidence.")
        command = ([sys.executable, str(HERE / "coord-runner.py"), "_renew", "--run", renew_run]
                   if renew_run else [sys.executable, str(HERE / "coord-core.py"), "leader", "who", "--json"])
        result = run_bounded(command,
                             cwd=self.cwd, env=dict(os.environ, AGENT_SESSION=owner),
                             timeout_seconds=timeout, stdout_limit=8192, stderr_limit=2048)
        if result.timed_out:
            raise LeaderCheckSlow("The bounded leader check did not finish inside its "
                                   "timeout; retry while the check's deadline still allows it.")
        require(result.returncode == 0 and not result.limit_exceeded
                and not result.cleanup_error and result.contained, "RUN-LEADER",
                "Establish a live designation for this Owner or inspect the bounded leader command failure.")
        if renew_run:
            return None
        row = json.loads(result.stdout)
        require(row.get("state") == "live" and row.get("leader") == owner,
                "RUN-LEADER", "Use the live designated Owner; the runner never pins or steals leadership.")
        return row

    def leader_retrying(self, owner, deadline, renew_run=None, stopped=lambda: False):
        """Retry a leader check while its deadline allows -- never a fixed per-attempt
        timeout. Only a definitive epoch/liveness answer (raised by leader() as a plain
        Refused), the deadline itself, or a stopped run ends the retry; a bounded-subprocess
        timeout (LeaderCheckSlow) is retried instead of read as lost authority (RUN-C)."""
        while True:
            require(not stopped(), "RUN-LEADER", "The run was stopped; no further check is admitted.")
            window = min(LEADER_CHECK_TIMEOUT, deadline - time.time())
            require(window > 0, "RUN-LEADER", "Lease or attempt deadline expired; retain the worker evidence.")
            try:
                return self.leader(owner, timeout=window, renew_run=renew_run)
            except LeaderCheckSlow:
                time.sleep(min(0.25, max(0, deadline - time.time())))

    def renew_admitted(self, manifest):
        """Bounded helper: expected epoch is checked in the SAME CAS attempt as renewal."""
        admission = read_json(self.directory(manifest["run_id"]) / "started.json")
        owner = manifest["owner"]
        require(os.environ.get("AGENT_SESSION") == owner, "RUN-LEADER", "Only the admitted Owner may renew.")
        row, oid, error = core.leader_read(self.repo)
        now = time.time()
        require(not error and row and row.get("leader") == owner
                and row.get("epoch") == admission.get("epoch") and core.leader_state(row, now) == "live",
                "RUN-LEADER", "The admitted designation changed or expired; do not renew its successor.")
        updated, error = core.leader_decide("renew", row, now, owner, owner, row["ttl"],
                                          host=row.get("host"), tree=row.get("tree"))
        require(not error, "RUN-LEADER", "Renewal was refused by the existing designation protocol.")
        new_oid, error = core.leader_write(self.repo, updated, oid)
        require(not error, "RUN-LEADER", "The designation changed during renewal; stop this attempt.")
        core.append_event(self.root, {"kind": "leader", "type": "leader", "action": "renew",
            "session": owner, "agent": owner, "wi": manifest["run_id"], "path": "-", "at": now,
            "leader": owner, "epoch": row["epoch"], "outcome": "ok", "ref_old": oid, "ref_new": new_oid,
            "expires_at": updated["expires_at"]})
        return {"state": "renewed", "epoch": row["epoch"], "expires_at": updated["expires_at"]}

    def worker_identity(self, manifest, worker, initial=False):
        cwd = Path(worker["worktree"])
        common = Path(git(cwd, "rev-parse", "--git-common-dir"))
        inventory, error = core.worktree_inventory(self.repo)
        matches = [r for r in inventory or [] if Path(r["path"]).resolve() == cwd]
        require((cwd / common).resolve() == self.common and not error and len(matches) == 1
                and matches[0].get("branch") == worker["branch"], "RUN-WORKTREE",
                "Use the registered worktree of this repository; a substituted clone is not the assigned checkout.")
        require(cwd.resolve() != self.repo and Path(git(cwd, "rev-parse", "--show-toplevel")).resolve() == cwd
                and git(cwd, "symbolic-ref", "--short", "HEAD") == worker["branch"],
                "RUN-WORKTREE", "Restore the assigned checkout and branch before reviewing evidence.")
        if initial:
            require(git(cwd, "rev-parse", "HEAD") == manifest["base"], "RUN-WORKTREE",
                    "Prepared worker HEAD changed; inspect work and prepare an explicit new attempt.")

    def verify(self, manifest, worker):
        self.worker_identity(manifest, worker)
        receipts = []
        cwd = Path(worker["worktree"])
        for expected in worker["evidence"]:
            if expected["kind"] == "commit":
                head = git(cwd, "rev-parse", "HEAD")
                require(head != manifest["base"], "RUN-EVIDENCE", "Return a new descendant commit.")
                git(cwd, "merge-base", "--is-ancestor", manifest["base"], head)
                receipts.append({"kind": "commit", "commit": head})
            else:
                if os.name == "nt":
                    from coord_files import read_regular
                    data = read_regular(cwd / expected["path"], expected["max_bytes"])
                    require(bool(data), "RUN-EVIDENCE", "Return a stable nonempty artifact within its byte bound.")
                    receipts.append({"kind": "file", "path": expected["path"], "bytes": len(data),
                                     "sha256": hashlib.sha256(data).hexdigest()})
                    continue
                # Each component is opened relative to an already-open directory, so a
                # renamed parent cannot substitute an outside artifact after a path check.
                parts = Path(expected["path"]).parts
                directory_fd = os.open(str(cwd), os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
                try:
                    for part in parts[:-1]:
                        next_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory_fd)
                        os.close(directory_fd)
                        directory_fd = next_fd
                    fd = os.open(parts[-1], os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW, dir_fd=directory_fd)
                finally:
                    os.close(directory_fd)
                with os.fdopen(fd, "rb") as handle:
                    info = os.fstat(handle.fileno())
                    require(stat.S_ISREG(info.st_mode), "RUN-EVIDENCE", "Return a regular file.")
                    data = handle.read(expected["max_bytes"] + 1)
                    after = os.fstat(handle.fileno())
                    require(0 < len(data) <= expected["max_bytes"] and info.st_mtime_ns == after.st_mtime_ns
                            and info.st_size == after.st_size, "RUN-EVIDENCE", "Return a stable nonempty artifact within its byte bound.")
                receipts.append({"kind": "file", "path": expected["path"], "bytes": len(data),
                                 "sha256": hashlib.sha256(data).hexdigest()})
        return receipts

    def status(self, manifest):
        rows, errors, _ = core.read_events(self.root)
        require(not errors, "RUN-STORE", "Repair unreadable coordination events before deriving status.")
        events = [r for r in rows if r.get("kind") == "runner" and r.get("run_id") == manifest["run_id"]
                  and r.get("session") == manifest["owner"]]
        if manifest["schema"] == "coord-partial/1":
            retained = {e["worker"]: e for e in events if e.get("state") == "worker_prepared"}
            inventory, error = core.worktree_inventory(self.repo)
            require(not error, "RUN-WORKTREE", "Read the actual inventory before reporting retained partial worktrees.")
            def retained_tree(worker):
                event = retained.get(worker["session"], {})
                matches = [r["path"] for r in inventory if r.get("branch") == worker["branch"]
                           and core._worktree_label(r["path"]) == core._worktree_label(event.get("worktree", ""))]
                return matches[0] if len(matches) == 1 else None
            return {"run_id": manifest["run_id"], "state": "prepare_failed" if any(
                    e.get("state") == "prepare_failed" for e in events) else "preparation_interrupted",
                    "workers": [{"session": w["session"], "branch": w["branch"],
                                 "worktree": retained_tree(w),
                                 "manual_brief": str(self.directory(manifest["run_id"]) / (w["session"] + ".brief.json"))
                                     if w["session"] in retained else None}
                                for w in manifest["workers"]],
                    "remedy": "Retain the reported trees and contract; prepare a new explicit attempt, never resume partially by guessing."}
        terminal = next((e for e in reversed(events) if e.get("state") == "finished"), None)
        if terminal:
            started = read_json(self.directory(manifest["run_id"]) / "started.json")
            require(started.get("owner") == manifest["owner"] and any(e.get("state") == "started"
                    and e.get("epoch") == started.get("epoch")
                    and e.get("qualification_sha256") == started.get("qualification_sha256") for e in events)
                    and terminal.get("result", {}).get("epoch") == started.get("epoch"),
                    "RUN-STORE", "Completion requires matching Owner admission and terminal evidence.")
            return terminal["result"]
        return {"run_id": manifest["run_id"], "state": "interrupted_or_running" if
                (self.directory(manifest["run_id"]) / "started.json").exists() else "prepared",
                "workers": self.public_manifest(manifest)["workers"], "events": len(events),
                "remedy": "Inspect retained evidence; never replay an interrupted run automatically."}

    def run(self, manifest, qualification):
        require(os.name in ("posix", "nt"), "RUN-PLATFORM", "Use manual briefs on unsupported process platforms.")
        owner = manifest["owner"]
        require(os.environ.get("AGENT_SESSION") == owner, "RUN-OWNER", "Invoke from the admitted Owner session.")
        require(qualification.get("schema") == "coord-qualification/1" and isinstance(qualification.get("workers"), dict),
                "RUN-QUALIFICATION", "Supply measured checkout-specific qualification, or use the retained manual briefs.")
        for worker in manifest["workers"]:
            q = qualification["workers"].get(worker["session"], {})
            require(isinstance(q, dict) and all(text(q.get(k)) for k in
                    ("version", "evidence", "effective_policy", "trust")) and q.get("fingerprint") == self.fingerprint(manifest, worker),
                    "RUN-QUALIFICATION", "Reobserve effective policy, trust and required hooks for this fingerprint; mode labels are not evidence.")
            if worker["harness"] == "copilot":
                require(q.get("effective_model") == copilot_model(worker["argv"]),
                        "RUN-COPILOT-MODEL", "Observe and attest the effective native model for this exact profile.")
            caps = q.get("capabilities", {})
            require(isinstance(caps, dict) and all(RANK.get(caps.get(k), 0) >= RANK[v]
                    for k, v in worker["required_capabilities"].items()), "RUN-QUALIFICATION",
                    "Qualify every declared capability at the required strength or use the manual brief.")
            require(worker.get("runtime", {}).get("permissions") != "ask"
                    or RANK.get(caps.get("interactive_permissions"), 0) >= 1,
                    "RUN-QUALIFICATION", "Ask mode requires a non-vacuous native callback/once-approval observation for this fingerprint.")
            self.worker_identity(manifest, worker, initial=True)
        admission = self.leader(owner)
        directory = self.directory(manifest["run_id"])
        require(not (directory / "started.json").exists(), "RUN-STARTED", "Read status; use a new run id and worker identities for a new attempt.")
        private_write(directory / "started.json", {"owner": owner, "epoch": admission["epoch"],
                      "at": time.time(), "qualification_sha256": digest(qualification)})
        self.event(manifest, "started", epoch=admission["epoch"], qualification_sha256=digest(qualification),
                   qualification_source="Owner attestation; not runner enforcement")
        stop = threading.Event()
        lease = {"expires_at": admission["expires_at"], "reason": None}
        old_handlers = {}

        def cancel_signal(signum, _frame):
            lease["reason"] = "RUN-CANCELLED"
            stop.set()

        for signum in (signal.SIGINT, signal.SIGTERM):
            old_handlers[signum] = signal.signal(signum, cancel_signal)

        def cancelled():
            if time.time() >= lease["expires_at"]:
                lease["reason"] = "RUN-LEADER"
                stop.set()
            return stop.is_set()

        def fence(remaining=None):
            """Positive confirmation of authority. `remaining` is the caller's operation
            deadline; None (a housekeeping check) retries slow checks until the observed
            lease could expire before the next answer (RUN-C)."""
            if cancelled():
                return False
            try:
                horizon = lease["expires_at"] - (LEADER_CHECK_TIMEOUT if remaining is None else 0)
                deadline = horizon if remaining is None else min(time.time() + remaining, horizon)
                row = self.leader_retrying(owner, deadline, stopped=stop.is_set)
                require(row["epoch"] == admission["epoch"], "RUN-LEADER", "The leader epoch changed; no further dispatch is admitted.")
                lease["expires_at"] = row["expires_at"]
                return not cancelled()
            except (Refused, OSError, ValueError):
                lease["reason"] = "RUN-LEADER"
                stop.set()
                return False

        def execute(worker):
            from coord_transport import run_session
            if cancelled():
                result = {"session": worker["session"], "state": "cancelled", "code": lease["reason"]}
            else:
                try:
                    # A queued worker may wait behind another turn. Bind the *current*
                    # executable/configuration/checkout again at its actual launch seam.
                    self.worker_identity(manifest, worker, initial=True)
                    require(self.fingerprint(manifest, worker) == qualification["workers"][worker["session"]]["fingerprint"],
                            "RUN-QUALIFICATION", "The queued worker changed before launch; requalify a new explicit attempt.")
                    self.event(manifest, "worker_started", worker=worker["session"], harness=worker["harness"])
                    def worker_fence(remaining, retry=False):
                        if cancelled():
                            return False
                        started_check = time.monotonic()
                        require(self.access_roots(worker, owner) == worker.get("additional_root_identities", []),
                                "RUN-ROOTS", "An admitted operational file changed before the next prompt.")
                        require(self.bounded_profile(manifest, worker, remaining, retry) == qualification["workers"][worker["session"]]["fingerprint"],
                                "RUN-QUALIFICATION", "The admitted profile changed; stop and requalify a new attempt.")
                        return fence(remaining - (time.monotonic() - started_check))
                    policy = worker.get("runtime", {})
                    box = self.controls(manifest, worker["session"]) if policy else None
                    pending_permissions = {}

                    def next_prompt(remaining):
                        started_check = time.monotonic()
                        row = box.next_prompt()
                        if row is None or row is False:
                            return row
                        prompt = self.bounded_prompt(manifest, worker, row["compilation_id"], remaining - (time.monotonic() - started_check))
                        require(digest(prompt) == row["prompt_sha256"], "RUN-COMPILE", "Queued compilation changed after admission.")
                        require(worker_fence(remaining - (time.monotonic() - started_check)), "RUN-LEADER", "The original live leader is required before mailbox dispatch.")
                        box.dispatched(row["id"])
                        self.event(manifest, "prompt_admitted", worker=worker["session"], control_id=row["id"])
                        return prompt

                    def permission_handler(request, remaining):
                        started_check = time.monotonic()
                        key = request["requestSequence"]
                        if key not in pending_permissions:
                            require(worker_fence(remaining), "RUN-LEADER", "The original live leader is required for approval.")
                            row = box.permission(request, expires_at=time.time() + remaining - (time.monotonic() - started_check))
                            pending_permissions[key] = row
                            self.event(manifest, "permission_pending", worker=worker["session"],
                                       request_id=row["id"], detail_path=str(box.directory / (row["id"] + ".json")))
                        row = pending_permissions[key]
                        require(row["request"] == request, "RUN-PERMISSION", "Native permission identity changed during approval.")
                        answer = box.answer(row["id"])
                        if answer is not None:
                            require(worker_fence(remaining - (time.monotonic() - started_check)), "RUN-LEADER", "The original live leader is required to select an approval.")
                            self.event(manifest, "permission_decided", worker=worker["session"], request_id=row["id"])
                        return answer

                    options = {}
                    if policy:
                        options = {"max_turns": policy["max_turns"],
                                   "next_prompt": next_prompt if policy["mailbox"] else None,
                                   "permission_handler": permission_handler if policy["permissions"] == "ask" else None,
                                   "mode_id": policy.get("mode_id")}
                    pin = expected_model(worker)
                    if pin is not None:
                        options["expected_model"] = pin
                    deadline = time.monotonic() + worker["deadline_seconds"]
                    attempts = []
                    bytes_used = 0
                    for attempt in range(policy.get("max_retries", 0) + 1):
                        remaining = deadline - time.monotonic()
                        require(remaining > 0 and worker["output_limit"] - bytes_used >= 1024,
                                "RUN-BOUNDS", "The original attempt budget is exhausted.")
                        transport = run_session(worker["transport"], worker["argv"], worker["worktree"], child_env(worker),
                            worker["prompt_texts"], remaining, worker["output_limit"] - bytes_used,
                            lambda event: self.event(manifest, "progress", worker=worker["session"], observation=event),
                            cancelled, worker_fence, worker.get("additional_roots"), **options)
                        attempts.append({"code": transport["code"], "prompts_started": transport.get("prompts_started", 0),
                                         "duration_seconds": transport["duration_seconds"]})
                        bytes_used += transport["stdout_bytes"] + transport["stderr_bytes"]
                        if not (attempt < policy.get("max_retries", 0) and transport["code"] in
                                ("spawn_failed", "early_eof", "io_error") and transport.get("prompts_started") == 0
                                and transport["cleanup_error"] is None and not cancelled()):
                            break
                        require(worker_fence(deadline - time.monotonic(), retry=True), "RUN-LEADER", "Retry requires unchanged live authority.")
                        self.event(manifest, "retry_started", worker=worker["session"], attempt=attempt + 2,
                                   reason=transport["code"], remaining_seconds=max(0, deadline - time.monotonic()))
                        stop.wait(min(.2 * (attempt + 1), max(0, deadline - time.monotonic())))
                    transport["attempts"] = attempts
                    transport["total_output_bytes"] = bytes_used
                    transport_state = {"blocked": "blocked", "cancelled": "cancelled"}.get(transport["outcome"], "failed")
                    result = {"session": worker["session"], "state": transport_state, "transport": transport,
                              "manual_brief": str(directory / (worker["session"] + ".brief.json"))}
                    if transport["outcome"] == "complete" and not cancelled():
                        if worker["harness"] == "copilot":
                            result["model_evidence"] = copilot_model_evidence(
                                transport.get("session_id"), child_env(worker), copilot_model(worker["argv"]))
                        try:
                            result["receipts"] = self.verify(manifest, worker)
                        except (Refused, OSError, subprocess.SubprocessError):
                            result.update(state="evidence_incomplete", code="RUN-EVIDENCE")
                        else:
                            state = core.decision_request_state(self.root, worker["session"])
                            result["decision_state"] = state
                            if not state["checked"]:
                                result.update(state="blocked", code="RUN-DECISION-NOT-CHECKED")
                            elif state["open_count"]:
                                result.update(state="blocked", code="RUN-DECISION-OPEN")
                            elif not fence():
                                result.update(state="cancelled", code=lease["reason"])
                            else:
                                result["state"] = "ready_for_review"
                except Refused as exc:
                    result = {"session": worker["session"], "state": "blocked", "code": exc.code,
                              "manual_brief": str(directory / (worker["session"] + ".brief.json"))}
                except Exception as exc:
                    result = {"session": worker["session"], "state": "failed", "code": "RUN-WORKER",
                              "error_type": type(exc).__name__}
            self.event(manifest, "worker_finished", worker=worker["session"], result=result)
            return result

        results = []
        try:
            with ThreadPoolExecutor(max_workers=manifest["parallelism"]) as pool:
                pending = {pool.submit(execute, worker) for worker in manifest["workers"]}
                next_check = time.monotonic()
                while pending:
                    done, pending = wait(pending, timeout=0.1, return_when=FIRST_COMPLETED)
                    results.extend(f.result() for f in done)
                    if not stop.is_set() and time.monotonic() >= next_check:
                        # RUN-C: one bounded check per tick, so a slow check never blocks the
                        # loop for longer than LEADER_CHECK_TIMEOUT nor cancels a live lease.
                        def check(window):
                            return self.leader(owner, timeout=window)

                        def renew(window):
                            return self.leader(owner, timeout=window, renew_run=manifest["run_id"])

                        held = not cancelled() and housekeeping_check(check, lease, admission["epoch"])
                        if held and lease["expires_at"] - time.time() <= float(admission["ttl"]) * 2 / 3:
                            held = (housekeeping_check(renew, lease, admission["epoch"])
                                    and housekeeping_check(check, lease, admission["epoch"]))
                        if not held:
                            lease["reason"] = lease["reason"] or "RUN-LEADER"
                            stop.set()
                        next_check = time.monotonic() + 0.5
        finally:
            stop.set()
            for signum, handler in old_handlers.items():
                signal.signal(signum, handler)
        ready = not lease["reason"] and all(r["state"] == "ready_for_review" for r in results)
        result = {"run_id": manifest["run_id"], "state": "ready_for_review" if ready else "incomplete",
                  "workers": sorted(results, key=lambda r: r["session"]), "epoch": admission["epoch"],
                  "semantic_acceptance": "Owner review required", "code": lease["reason"]}
        self.event(manifest, "finished", result=result)
        return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("prepare").add_argument("--contract", required=True)
    attach = sub.add_parser("attach")
    attach.add_argument("--harness", choices=("codex", "grok", "claude", "agy"), required=True)
    for name in ("worker", "delivery-id", "native-session", "socket", "cwd", "compilation", "executable"):
        attach.add_argument("--" + name, required=True)
    attachment_identity = sub.add_parser("_attach_identity")
    for name in ("worker", "cwd", "branch"):
        attachment_identity.add_argument("--" + name, required=True)
    for command in ("run", "status", "fingerprint", "_renew"):
        item = sub.add_parser(command)
        item.add_argument("--run", required=True)
        if command == "run":
            item.add_argument("--qualification", required=True)
    profile = sub.add_parser("_profile")
    profile.add_argument("--run", required=True)
    profile.add_argument("--worker", required=True)
    profile.add_argument("--clean", action="store_true")
    prompt = sub.add_parser("_prompt")
    prompt.add_argument("--run", required=True)
    prompt.add_argument("--worker", required=True)
    prompt.add_argument("--compilation", required=True)
    for command in ("enqueue", "finish", "permissions", "permission-show", "permission-decide"):
        item = sub.add_parser(command)
        item.add_argument("--run", required=True)
        item.add_argument("--worker", required=True)
        if command == "enqueue":
            item.add_argument("--compilation", required=True)
        if command in ("permission-show", "permission-decide"):
            item.add_argument("--request", required=True)
        if command == "permission-decide":
            item.add_argument("--option", required=True)
    args = parser.parse_args(argv)
    try:
        runner = Runner(Path.cwd())
        if args.command == "prepare":
            result = runner.prepare(read_json(args.contract))
        elif args.command == "attach":
            result = runner.attach(args)
        elif args.command == "_attach_identity":
            runner.worker_identity({}, {"worktree": str(Path(args.cwd).resolve()), "session": args.worker,
                                        "branch": args.branch})
            result = {"state": "checked"}
        else:
            manifest = runner.load(args.run, allow_partial=args.command == "status")
            if args.command == "fingerprint":
                result = runner.fingerprints(manifest)
            elif args.command == "status":
                result = runner.status(manifest)
            elif args.command == "_renew":
                result = runner.renew_admitted(manifest)
            elif args.command == "_profile":
                worker = next(w for w in manifest["workers"] if w["session"] == args.worker)
                if args.clean:
                    runner.worker_identity(manifest, worker, initial=True)
                    require(not git(Path(worker["worktree"]), "status", "--porcelain", "--untracked-files=all"),
                            "RUN-RETRY-DIRTY", "Startup changed the checkout; inspect instead of replaying.")
                result = {"fingerprint": runner.fingerprint(manifest, worker)}
            elif args.command == "_prompt":
                require(any(w["session"] == args.worker for w in manifest["workers"]),
                        "RUN-IDENTITY", "Select an admitted worker.")
                result = {"prompt": runner.compiled_prompts([args.compilation], args.worker)[0]}
            elif args.command in ("enqueue", "finish", "permissions", "permission-show", "permission-decide"):
                result = runner.control(manifest, args.command, args.worker,
                                        getattr(args, "compilation", None), getattr(args, "request", None),
                                        getattr(args, "option", None))
            else:
                result = runner.run(manifest, read_json(args.qualification))
        print(json.dumps(result, sort_keys=True))
        return 3 if ((args.command == "run" and result["state"] != "ready_for_review")
                     or (args.command == "attach" and result["state"] == "indeterminate")) else 0
    except Refused as exc:
        print(json.dumps({"state": "blocked", "code": exc.code, "remedy": exc.remedy}))
        return 2
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError) as exc:
        print(json.dumps({"state": "blocked", "code": "RUN-INPUT", "error_type": type(exc).__name__,
                          "remedy": "Inspect the contract, retained run state and local dependencies; do not replay an attempt."}))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
