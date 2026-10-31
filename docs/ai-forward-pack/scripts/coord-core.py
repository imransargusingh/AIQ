#!/usr/bin/env python3
"""coord-core.py - agent coordination, Phase 1 walking skeleton.

Holds the record of intent and answers "may this session touch this artifact?" from it.
Append-only JSONL, one file per session; every piece of state is a fold over it. No daemon,
no database, no dependency beyond the standard library (ADR-0007).

Four controls here were observed failing on the un-fixed shape before they were trusted:
  LOG-A     an append onto a file not ending in a newline fuses two records and loses BOTH
  R4        a check that scanned nothing must not report "free"
  CTRL-PORT os.open without O_BINARY translates newlines on Windows -- which also MASKED
            the LOG-A control, because a stray CR still terminates a line
  F8        a claim over the coordination record itself would lock the substrate

Design: docs/design/coord-core-phase1.md
"""
import argparse
import fnmatch
import hashlib
import json
import math
import os
import re
import stat
import statistics
import subprocess
import sys
import time
from pathlib import Path

# Windows consoles default to cp1252, which cannot encode the glyphs this tool prints
# (DC-211/PLAT-A). Without this the script dies with UnicodeEncodeError on output alone.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

TTL_DEFAULT = 300
# DC-163: a lease sized to a node's lifetime turns a shared control into a serial resource
# (measured: the defect register held for an hour while edited never; two joins queued ~50
# min). The cap is the ceiling a claim may ask for without saying why.
TTL_CAP = 900
SESSION_STALE_SECONDS = 8 * 3600
COORD_DIRNAME = ".agents"
SESSION_CONTRACT = "docs/collaboration/session-contracts.md"
REQUESTS_FILE = "requests.jsonl"

# --- leader designation (spec-leader-designation; D13, ratified 2026-09-19) ---------------
# ONE block. Tune these from `coord metrics` (leader_loss, contested_pins, reclaim latency),
# never from prose: the doctrine (agent-coordination.md CO-L) cites the NAMES, not the numbers.
LEADER_REF = "refs/coord/leader"
LEADER_TTL = 300          # s - a designation lapses without a renew
LEADER_RENEW = 100        # s - the holder renews every TTL/3
LEADER_RETRY = 20         # s - a lost compare-and-swap re-reads and retries after this
LEADER_QUIET = 30         # s - after an EXPIRY nobody may reclaim (Consul lock-delay x2)
ZERO_OID = "0" * 40       # `update-ref <ref> <new> <zeros>` creates, and refuses if present

# --- seam requests (spec-typed-seam-requests; D5, D13) ------------------------------------
# ONE block, beside the leader's. Tune from `coord metrics` (unresolved by deadline, fallback
# taken, stale acks), never from prose: the doctrine cites the NAMES, not the numbers.
REQUEST_DEADLINE = 900    # s - the long lease; a request is terminal by then, by resolution or fallback
REQUEST_RETRY = 20        # s - a waiting requester re-reads at this cadence (D13's retry)
# simplify: REQUEST_RETRY is named here so the block is complete; P3's kick ladder consumes it.
#   ceiling: nothing in P1 reads it.  upgrade trigger: `session heartbeat` (P3) lands.
REQUEST_TERMINAL = ("resolved", "expired")
REQUEST_OPEN = ("sent", "received", "acked", "untyped")
# P3 - progress liveness (spec-liveness-and-track; D7: heartbeats carry progress, the rule is a
# passed deadline OR three missed beats, no phi; CO17: two kicks per work item, counted).
HEARTBEAT_SAMPLE = LEADER_RENEW      # s - a beat reaches the ledger at most this often (TTL/3)
STALL_AFTER = 3 * HEARTBEAT_SAMPLE   # s - three missed beats with no progress -> stalled
KICK_CAP = 2                         # rung-1 kicks per work item; the third is refused
TRACK_STATES = ("live", "stalled", "blocked", "done")


class CoordError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


# --- paths & identity -------------------------------------------------------

def repo_root(cwd):
    """The PRIMARY checkout of this repository, from any worktree.

    The record is per REPOSITORY, not per checkout. `--git-common-dir` is the primitive
    that says so: from a linked worktree it returns the primary .git (absolute), and from
    the primary checkout it returns a relative ".git". Its parent is the primary checkout
    in both cases.

    Found by running the Phase-1 demo: with the root defaulting to cwd/.agents, every
    worktree got its own private record and two sessions could never see each other -
    which is the exact criterion this phase exists to satisfy.

    Read from the filesystem, NOT by shelling out to `git rev-parse --git-common-dir`.
    The first implementation did shell out and cost ~35 ms of the check's budget - measured
    at 82 ms p95, which met NFR-P1 but blew straight through ADR-0007's own 60 ms
    compaction trigger. On the hot path of every edit, a subprocess is not free.

    The layout this reads is git's own:
      primary checkout -> .git is a DIRECTORY; the repo root is its parent
      linked worktree  -> .git is a FILE holding "gitdir: <primary>/.git/worktrees/<name>"
    """
    here = Path(cwd).resolve()
    for candidate in [here, *here.parents]:
        dot_git = candidate / ".git"
        if dot_git.is_dir():
            return candidate
        if dot_git.is_file():
            try:
                text = dot_git.read_text(encoding="utf-8").strip()
            except OSError:
                break
            if text.startswith("gitdir:"):
                gitdir = Path(text.split(":", 1)[1].strip())
                if not gitdir.is_absolute():
                    gitdir = candidate / gitdir
                parts = gitdir.resolve().parts
                if "worktrees" in parts:
                    common = Path(*parts[: parts.index("worktrees")])
                    return common.parent
            break
    return here     # not a git repo: degrade to the directory, and say nothing false


def checkout_top(cwd):
    """The top of the CURRENT checkout - primary or linked worktree - i.e. the first ancestor
    holding a `.git` entry (a directory or a worktree's pointer file).

    `repo_root` answers "which repository" and is right for the `.agents` stores and shared
    refs. Three questions in main() are "which tree": the base a hook's absolute path is made
    relative to, the index the pre-commit floor reads, and the file whose blob a request's ack
    is compared with. Asked of the primary from a worktree they answered about the wrong
    tree - the hook could not match a worktree path to its lease (a false grant), `coord
    precommit` run by hand read the primary's index, and a stale-ack check read the primary's
    file (class WT-A). Filesystem only, like repo_root."""
    here = Path(cwd).resolve()
    for candidate in (here, *here.parents):
        if (candidate / ".git").exists():
            return candidate
    return here


def resolve_root(cwd, raw):
    """Resolve COORD_ROOT, refusing anything outside the repository.

    COORD_ROOT is attacker-controllable input that selects which file becomes trusted
    state (STRIDE B1, elevation of privilege). Found at the design gate, not in the draft.
    """
    base = repo_root(cwd)
    root = Path(raw).resolve() if raw else (base / COORD_DIRNAME).resolve()
    try:
        root.relative_to(base)
    except ValueError:
        return None, {"code": "COORD-NOT-CHECKED-ROOT",
                      "reason": "COORD_ROOT resolves outside the repository: {}".format(root)}
    return root, None


def coordination_active(root):
    """Is coordination ON for this repository? Only when its record holds something: a session
    ledger (`log/<session>.jsonl`, written by `session start`, mail twins, heartbeats), an inbox
    (`mail/*.jsonl`) or the request store. A session identity in the environment is not enough,
    and neither is the `log/` directory alone - the audit log's local fallback (AL0.2) creates
    `log/audit/` in repositories that never coordinate.

    The coordination hooks (doorbell, heartbeat, owner-review gate) return at once when this is
    False, so a repository that never coordinates pays nothing for them and gains no ledger
    (bench decision B15: they ran on every tool call in cells that never coordinate).
    Filesystem only: this runs on the hook hot path."""
    root = Path(root)
    if (root / REQUESTS_FILE).is_file():
        return True
    for sub in ("log", "mail"):
        try:
            with os.scandir(str(root / sub)) as entries:
                if any(entry.name.endswith(".jsonl") and entry.is_file() for entry in entries):
                    return True
        except OSError:
            continue
    return False


def _norm(p):
    return str(p).replace("\\", "/").replace("**", "*")


def _literal_segments(pattern):
    """The leading path segments of a pattern that contain no wildcard."""
    segs = []
    for seg in _norm(pattern).split("/"):
        if any(ch in seg for ch in "*?["):
            break
        segs.append(seg)
    return segs


def overlaps(a, b):
    """Do two path patterns intersect? Prefer a false positive: a false refusal costs a
    message, a false grant costs a merge.

    Compared by SEGMENT, not by string prefix, so src/Foo/** and src/FooBar/** are
    correctly disjoint.

    simplify: fnmatch both ways plus a segment-prefix test.
      ceiling: a wildcard in the middle of a pattern, and character classes.
      upgrade trigger: the first refusal a human calls wrong, or Phase 3's artifact-class
      registry introducing nested patterns.
    """
    na, nb = _norm(a), _norm(b)
    if na == nb:
        return True
    if fnmatch.fnmatch(nb, na) or fnmatch.fnmatch(na, nb):
        return True
    sa, sb = _literal_segments(na), _literal_segments(nb)
    n = min(len(sa), len(sb))
    return sa[:n] == sb[:n]


def excepted(lease, path):
    """Is `path` carved out of this lease by its `except` list (claim --except, class CTX-R)?"""
    return any(overlaps(e, path) for e in lease.get("except", ()))


def lease_covers(lease, path):
    """Does a live lease cover this path? A directory lease minus the peer's named files."""
    return overlaps(lease["path"], path) and not excepted(lease, path)


# --- the record -------------------------------------------------------------

def make_event(kind, session, agent, wi, path, at, ttl=TTL_DEFAULT, seq=None, excepts=None):
    first = next((seg for seg in _norm(path).split("/") if seg not in (".", "")), "")
    if first == COORD_DIRNAME:
        raise CoordError("COORD-CLAIM-SELF",
                         "a claim over the coordination record itself is refused")
    event = {"kind": kind, "session": session, "agent": agent, "wi": wi,
             "path": _norm(path), "at": float(at)}
    if kind == "claim":
        event["ttl"] = float(ttl)
        if excepts:
            event["except"] = [_norm(e) for e in excepts]
    if seq is not None:
        event["seq"] = int(seq)
    return event


def _next_seq(logfile):
    if not logfile.exists():
        return 1
    n = 0
    with open(logfile, "r", encoding="utf-8") as fh:
        for line in fh:
            if line.strip():
                n += 1
    return n + 1


def _portable_event(event):
    """Copy only diagnostic paths; runtime inputs and free text keep their exact bytes."""
    def home_path(value):
        if not isinstance(value, str):
            return value
        normalized = value.replace("\\", "/")
        home = str(Path.home()).replace("\\", "/").rstrip("/")
        if home and (normalized == home or normalized.startswith(home + "/")):
            return "~" + normalized[len(home):]
        # Legacy ledgers can originate on another machine or platform.
        return re.sub(r"^(?:[A-Za-z]:)?/(?:Users|home)/[^/]+(?=/|$)", "~", normalized) if re.match(
            r"^(?:[A-Za-z]:)?/(?:Users|home)/", normalized) else value

    row = dict(event)
    if isinstance(row.get("worktree"), str):
        row["worktree"] = _worktree_label(row["worktree"])
    for name in ("manual_brief", "hook_cwd", "detail_path"):
        if name in row:
            row[name] = home_path(row[name])
    if isinstance(row.get("result"), dict):
        result = row["result"] = dict(row["result"])
        if "manual_brief" in result:
            result["manual_brief"] = home_path(result["manual_brief"])
        if isinstance(result.get("workers"), list):
            result["workers"] = [dict(worker, manual_brief=home_path(worker["manual_brief"]))
                if isinstance(worker, dict) and "manual_brief" in worker else worker
                for worker in result["workers"]]
    return row


def append_event(root, event):
    """Append one event as exactly one write() - atomic under O_APPEND (spike S3)."""
    logdir = Path(root) / "log"
    logdir.mkdir(parents=True, exist_ok=True)
    logfile = logdir / "{}.jsonl".format(event["session"])
    if "seq" not in event:
        event["seq"] = _next_seq(logfile)
    payload = json.dumps(_portable_event(event), sort_keys=True) + "\n"

    # LOG-A: emit a LEADING newline when the file does not already end in one, so a fused
    # record is impossible to express rather than merely detectable (control ladder rung 1).
    # The file may have been left unterminated by a merge resolution or a hand edit -- the
    # writer owns this seam because no single actor otherwise does.
    if logfile.exists() and logfile.stat().st_size:
        with open(logfile, "rb") as fh:
            fh.seek(-1, os.SEEK_END)
            last = fh.read(1)
        if last not in (b"\n", b"\r"):
            payload = "\n" + payload

    # CTRL-PORT: O_BINARY (Windows only; 0 elsewhere) stops newline translation, so the
    # committed bytes are LF on every platform as .gitattributes requires. It also stops a
    # stray CR from masking the LOG-A control above -- which is how that masking was found.
    flags = os.O_WRONLY | os.O_APPEND | os.O_CREAT | getattr(os, "O_BINARY", 0)
    fd = os.open(str(logfile), flags, 0o644)
    try:
        os.write(fd, payload.encode("utf-8"))   # exactly one write() -- atomic (spike S3)
    finally:
        os.close(fd)
    return event


def read_events(root):
    """Return (events, errors, files_scanned).

    Errors are collected, never raised - but a single error makes the whole check
    not_checked. Fail safe, never open (NFR-R2).
    """
    logdir = Path(root) / "log"
    events, errors, files = [], [], 0
    if not logdir.is_dir():
        return events, errors, files
    for logfile in sorted(logdir.glob("*.jsonl")):
        files += 1
        with open(logfile, "r", encoding="utf-8") as fh:
            for lineno, line in enumerate(fh, start=1):
                if not line.strip():
                    continue
                try:
                    events.append(json.loads(line))
                except json.JSONDecodeError as exc:
                    errors.append("{}:{}: {}".format(logfile.name, lineno, exc.msg))
    events.sort(key=lambda e: (e.get("at", 0.0), e.get("session", ""), e.get("seq", 0)))
    return events, errors, files


def fold(events, now):
    """Pure fold: events -> live leases. Replaying is idempotent (NFR-R1).

    derive-don't-store (DM7): `expires` is computed here (at + ttl) and never persisted.
    Two stored definitions of one quantity is the defect signature.
    """
    leases, seen = {}, set()
    for event in events:
        ident = (event.get("session"), event.get("seq"))
        if ident in seen:            # F9: a retried tool call must not take a second lease
            continue
        seen.add(ident)
        key = (event.get("path"), event.get("session"))
        if event.get("kind") == "claim":
            leases[key] = {"path": event["path"], "session": event["session"],
                           "agent": event.get("agent", event["session"]),
                           "wi": event.get("wi", ""),
                           "except": list(event.get("except", [])),
                           "expires": event["at"] + event.get("ttl", TTL_DEFAULT)}
        elif event.get("kind") == "release":
            leases.pop(key, None)
    return {k: v for k, v in leases.items() if v["expires"] > now}


# --- the decision -----------------------------------------------------------

def check(root, path, me, now, covers=None):
    if not me:
        return {"decision": "not_checked", "path": path, "files_scanned": 0,
                "events_scanned": 0, "code": "COORD-NOT-CHECKED-IDENTITY",
                "reason": "AGENT_SESSION is unset, so this session has no identity to check"}

    events, errors, files = read_events(root)

    if errors:
        return {"decision": "not_checked", "path": path, "files_scanned": files,
                "events_scanned": len(events), "code": "COORD-NOT-CHECKED-RECORD",
                "reason": "the record could not be read: " + "; ".join(errors[:3])}

    # R4: a control that scanned nothing has not reported clean. An empty corpus and a
    # clean corpus must never render the same. Written because this architecture's own
    # allocator spike printed "COLLISION-FREE" over zero identifiers.
    if files == 0:
        return {"decision": "not_checked", "path": path, "files_scanned": 0,
                "events_scanned": 0, "code": "COORD-NOT-CHECKED-RECORD",
                "reason": "0 files scanned - there is no record here, so nothing was checked"}

    for lease in fold(events, now).values():
        if lease["session"] != me and (covers or lease_covers)(lease, path):
            return {"decision": "deny", "path": path, "files_scanned": files,
                    "events_scanned": len(events), "code": "COORD-REFUSED",
                    "holder": lease["agent"], "session": lease["session"],
                    "wi": lease["wi"], "expires_in": int(lease["expires"] - now),
                    "reason": "an unexpired lease overlaps your pattern"}

    return {"decision": "allow", "path": path, "files_scanned": files,
            "events_scanned": len(events), "code": None, "reason": ""}


def _safe(value, limit=200):
    """Strip control characters and cap length before interpolating into the refusal.

    STRIDE B4, elevation: the refusal is rendered into ANOTHER MODEL'S context. The
    template is fixed and nothing is interpolated into prose, but an interpolated VALUE
    carrying a newline could still add a line that reads as an instruction. This is the
    Phase-4 trust boundary arriving three phases early, so it is closed here.
    """
    text = "".join(ch for ch in str(value) if ch.isprintable())
    return text[:limit]


def render(decision):
    """Four labelled lines, fixed order: what happened - who - why - what to do.

    No colour is load-bearing: every state is distinguishable from the text and the exit
    code alone. Accessibility and machine-readability are the same requirement here.
    "refused" is never softened to "denied" or "unavailable" - the reader is a model that
    must not read the outcome as a transient failure worth retrying.
    """
    decision = {k: (_safe(v) if isinstance(v, str) else v) for k, v in decision.items()}
    verdict = decision["decision"]
    if verdict == "allow":
        return ""
    if verdict == "deny":
        return ("REFUSED  {path}\n"
                "  held by   {holder} - {wi} - expires in {expires_in}s\n"
                "  because   {reason}\n"
                "  remedy    wait, claim a disjoint subset, or record a block on {wi}"
                ).format(**decision)
    return ("NOT CHECKED  {path}\n"
            "  held by   unknown - this check did not run\n"
            "  because   {reason}\n"
            "  remedy    fix the condition above, then re-run; this is not a pass"
            ).format(**decision)


EXIT = {"allow": 0, "deny": 3, "not_checked": 4}


# --- the decisions store (Phase 2) ------------------------------------------
#
# TWO STORES, TWO GRAINS, ONE READER EACH.
#   log/       one row is one INTENT event  (claim/release/session)  -> FOLDED
#   decisions/ one row is one ENFORCEMENT decision (allow/deny/ask)  -> NEVER folded
#
# Phase 1 appended refusals into the folded log. That is now wrong, and the reason is
# Phase 1's own measurement: the check was 63 ms p95 at 10,000 events, already at
# ADR-0007's 60 ms compaction trigger. Phase 2 records a decision PER EDIT - orders of
# magnitude more traffic than one per claim - so folding those would blow the hot path
# within a day. Keeping them out means the fold stays proportional to CLAIMS, not EDITS,
# and the metric that decides whether this phase worked costs nothing to collect.

def append_decision(root, session, agent, path, decision, hook_context=None):
    """Record one enforcement decision. Never folded; read by `tail` and `metrics`.

    G14: the verdict is computed BEFORE this is attempted and cannot be changed by it.
    A refusal that cannot be recorded is still a refusal.
    """
    logdir = Path(root) / "decisions"
    # The stored kind is the UBIQUITOUS LANGUAGE word, not the internal one: "refused",
    # never "denied". The store is read by humans in `tail`, and the vocabulary is the
    # same one the refusal itself uses.
    kind = {"allow": "allowed", "deny": "refused",
            "not_checked": "not_checked"}.get(decision.get("decision"), "unknown")
    record = {"kind": kind,
              "session": session or "anon", "agent": agent or "anon",
              "wi": decision.get("wi", ""), "path": _norm(path),
              "at": time.time(), "code": decision.get("code")}
    if hook_context:
        record.update(hook_context)
    try:
        logdir.mkdir(parents=True, exist_ok=True)
        logfile = logdir / "{}.jsonl".format(record["session"])
        payload = json.dumps(record, sort_keys=True) + "\n"
        if logfile.exists() and logfile.stat().st_size:
            with open(logfile, "rb") as fh:
                fh.seek(-1, os.SEEK_END)
                if fh.read(1) not in (b"\n", b"\r"):    # LOG-A, same seam
                    payload = "\n" + payload
        flags = os.O_WRONLY | os.O_APPEND | os.O_CREAT | getattr(os, "O_BINARY", 0)
        fd = os.open(str(logfile), flags, 0o644)
        try:
            os.write(fd, payload.encode("utf-8"))
        finally:
            os.close(fd)
    except Exception:
        pass    # never let a bookkeeping failure change a verdict


def read_decisions(root):
    logdir = Path(root) / "decisions"
    out = []
    if not logdir.is_dir():
        return out
    for logfile in sorted(logdir.glob("*.jsonl")):
        with open(logfile, "r", encoding="utf-8") as fh:
            for line in fh:
                if not line.strip():
                    continue
                try:
                    out.append(json.loads(line))
                except json.JSONDecodeError:
                    continue    # a damaged decision is lost telemetry, not lost state
    out.sort(key=lambda d: d.get("at", 0.0))
    return out


def append_record(path, record):
    """Append one JSONL row to a small operator ledger."""
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(record, sort_keys=True) + "\n"
    if Path(path).exists() and Path(path).stat().st_size:
        with open(path, "rb") as fh:
            fh.seek(-1, os.SEEK_END)
            if fh.read(1) not in (b"\n", b"\r"):
                payload = "\n" + payload
    flags = os.O_WRONLY | os.O_APPEND | os.O_CREAT | getattr(os, "O_BINARY", 0)
    fd = os.open(str(path), flags, 0o644)
    try:
        os.write(fd, payload.encode("utf-8"))
    finally:
        os.close(fd)


def request_log_path(root):
    return Path(root) / REQUESTS_FILE


def read_request_events(root):
    path = request_log_path(root)
    if not path.is_file():
        return [], []
    events, errors = [], []
    with open(path, "r", encoding="utf-8") as fh:
        for lineno, line in enumerate(fh, 1):
            if not line.strip():
                continue
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError as exc:
                errors.append("{}:{}: {}".format(path.name, lineno, exc.msg))
    events.sort(key=lambda e: e.get("at", 0.0))
    return events, errors


def decision_request_state(root, session):
    """Bounded, fail-closed acceptance projection; the historical writer/fold is unchanged.

    One checked observation, not a ruling or new store. Missing requests are empty only
    inside an initialized coordination root. Native Stop may remain bounded/fail-open;
    the runner must refuse readiness on checked=False.
    """
    failed = {"checked": False, "open_ids": [], "open_count": None, "truncated": False}
    try:
        root = Path(root)
        if not session or not root.is_dir() or not (root / "log").is_dir():
            return failed
        path = request_log_path(root)
        try:
            before = path.lstat()
        except FileNotFoundError:
            return {"checked": True, "open_ids": [], "open_count": 0, "truncated": False}
        if not stat.S_ISREG(before.st_mode):
            return failed
        fd = os.open(str(path), os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
                     | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_BINARY", 0))
        with os.fdopen(fd, "rb") as handle:
            info = os.fstat(handle.fileno())
            after = path.lstat()
            # Identity observations also reject detected replacement where O_NOFOLLOW
            # is unavailable; they are not an atomic Windows no-follow primitive.
            if (not stat.S_ISREG(info.st_mode) or not stat.S_ISREG(after.st_mode)
                    or (before.st_dev, before.st_ino) != (info.st_dev, info.st_ino)
                    or (after.st_dev, after.st_ino) != (info.st_dev, info.st_ino)
                    or info.st_size > 8 * 1024 * 1024):
                return failed
            raw = handle.read(8 * 1024 * 1024 + 1)
        if len(raw) > 8 * 1024 * 1024:
            return failed
        events = [json.loads(line) for line in raw.decode("utf-8").splitlines() if line.strip()]
        kinds = {"request-add", "request-receive", "request-ack", "request-resolve", "request-expire"}
        for row in events:
            if (not isinstance(row, dict) or row.get("kind") not in kinds
                    or not isinstance(row.get("id"), str)
                    or not re.fullmatch(r"[A-Za-z0-9_.:-]{1,160}", row["id"])
                    or isinstance(row.get("at"), bool) or not isinstance(row.get("at"), (float, int))
                    or not math.isfinite(row["at"])
                    or not isinstance(row.get("session"), str) or not row["session"]):
                return failed
            if row["kind"] == "request-add":
                if (not isinstance(row.get("reason", ""), str)
                        or not isinstance(row.get("from", row["session"]), str)
                        or not (row.get("from") or row["session"])):
                    return failed
                deadline = row.get("deadline_at")
                if deadline is not None and (isinstance(deadline, bool)
                        or not isinstance(deadline, (int, float)) or not math.isfinite(deadline)):
                    return failed
            field = {"request-resolve": "resolution", "request-ack": "blob", "request-expire": "fallback"}.get(row["kind"])
            if field and (not isinstance(row.get(field), str) or not row[field].strip()):
                return failed
        events.sort(key=lambda row: row["at"])
        added = {}
        for row in events:
            if row["kind"] == "request-add":
                if row["id"] in added:
                    return failed
                added[row["id"]] = row
            elif row["id"] not in added:
                return failed
            elif row["kind"] == "request-resolve":
                original = added[row["id"]]
                if (original.get("reason") == "decision-request"
                        and row["session"] == (original.get("from") or original.get("session"))):
                    return failed  # A worker's generic resolve is not an independent ruling.
        ids = [row["id"] for row in fold_requests(events)
               if row.get("reason") == "decision-request"
               and (row.get("from") or row.get("session")) == session
               and row.get("status") in REQUEST_OPEN]
        return {"checked": True, "open_ids": ids[:32], "open_count": len(ids), "truncated": len(ids) > 32}
    except (OSError, ValueError, TypeError, UnicodeError, RecursionError):
        return failed


def fold_requests(events):
    """Pure fold: request-* rows -> one state per request id (spec-typed-seam-requests).

    sent -> received -> acked -> resolved | expired. Terminal wins: a row of a later kind after
    a terminal state is ignored (the CLI refuses to write one; the fold does not rely on that).
    An add with no deadline_at predates the typed shape and folds to `untyped` - listed, never
    expired, never failed (US-10).
    """
    requests = {}
    for event in events:
        rid = event.get("id")
        if not rid:
            continue
        kind = event.get("kind")
        if kind == "request-add":
            row = dict(event)
            row["status"] = "sent" if row.get("deadline_at") is not None else "untyped"
            row.setdefault("text", row.get("contract", ""))
            requests[rid] = row
            continue
        if rid not in requests or requests[rid]["status"] in REQUEST_TERMINAL:
            continue
        row = dict(requests[rid])
        who, at = event.get("session", ""), event.get("at")
        if kind == "request-receive":
            row.update(status="received", received_at=at, received_by=who)
        elif kind == "request-ack":
            row.update(status="acked", ack_blob=event.get("blob", ""), acked_at=at, acked_by=who)
        elif kind == "request-resolve":
            row.update(status="resolved", outcome="resolution",
                       resolution=event.get("resolution", ""), resolved_at=at, resolved_by=who)
        elif kind == "request-expire":
            row.update(status="expired", outcome="fallback", expired_at=at, expired_by=who,
                       fallback=event.get("fallback", row.get("fallback", "")))
        else:
            continue
        requests[rid] = row
    return sorted(requests.values(), key=lambda r: r.get("at", 0.0))


def blob_sha(data):
    """git's blob id: sha1("blob <len>\0" + bytes). Spiked against `git hash-object`."""
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def current_blob(repo, path):
    """The blob id of repo/path now, in-process; None (rendered `not recorded`) when there is
    no path, the path escapes the repository (STRIDE: a crafted --path reads nothing outside
    it), or the file cannot be read."""
    if not path or repo is None:
        return None
    base = Path(repo).resolve()
    target = (base / _norm(path)).resolve()
    try:
        target.relative_to(base)
    except ValueError:
        return None
    try:
        with open(target, "rb") as fh:
            return blob_sha(fh.read())
    except OSError:
        return None


def annotate_requests(requests, repo, now):
    """Derived fields, never stored (DM7): overdue, deadline_in, stale.

    stale is True/False only when an ack pinned a blob AND the cited path can be hashed now;
    otherwise the string "not recorded" - an absent comparison never renders as "fresh".
    """
    out = []
    for request in requests:
        row = dict(request)
        deadline = row.get("deadline_at")
        typed = deadline is not None
        row["overdue"] = bool(typed and row["status"] in REQUEST_OPEN and now >= deadline)
        row["deadline_in"] = round(deadline - now, 1) if typed else "not recorded"
        if row.get("ack_blob") and row["status"] == "acked":
            current = current_blob(repo, row.get("path"))
            row["stale"] = "not recorded" if current is None else (current != row["ack_blob"])
        else:
            row["stale"] = "not recorded"
        out.append(row)
    return out


def request_doctor_lines(root, repo, now):
    """(lines, problems) for `coord doctor` and pack-doctor's `requests` check.

    FAIL  a typed request past its deadline with no recorded outcome (silence - the 28%)
    WARN  an ack pinned to a blob that has since changed; untyped rows (counted, never failed)
    An absent store is `not recorded`, never "0 problems" (R4).
    """
    if not request_log_path(root).is_file():
        return ["requests         not recorded (no {}/{} here)".format(
            COORD_DIRNAME, REQUESTS_FILE)], 0
    events, errors = read_request_events(root)
    if errors:
        return ["requests         NOT CHECKED  [COORD-REQUEST-NOT-CHECKED]",
                "  because     " + _safe("; ".join(errors[:2]), 200)], 1
    rows = annotate_requests(fold_requests(events), repo, now)
    silent = [r["id"] for r in rows if r["overdue"]]
    stale = [r["id"] for r in rows if r["stale"] is True]
    untyped = sum(1 for r in rows if r["status"] == "untyped")
    lines, problems = [], 0
    if silent:
        lines.append("requests         FAIL  [COORD-REQUEST-SILENT-EXPIRY] {} past the deadline with"
                     " no recorded outcome: {}".format(
                         len(silent), ", ".join(_safe(i, 40) for i in silent[:5])))
        lines.append("  remedy      `coord request expire` records each one's fallback as the"
                     " outcome - a request never ends in silence")
        problems += 1
    else:
        terminal = sum(1 for r in rows if r["status"] in REQUEST_TERMINAL)
        lines.append("requests         ok - {} request(s), {} open, {} terminal".format(
            len(rows), len(rows) - terminal, terminal))
    if stale:
        lines.append("  WARN  [COORD-REQUEST-STALE-ACK] {} ack(s) pinned to a blob that has since"
                     " changed: {}".format(len(stale), ", ".join(_safe(i, 40) for i in stale[:5])))
    if untyped:
        lines.append("  WARN  [COORD-REQUEST-UNTYPED {}] request(s) predate deadline/fallback;"
                     " listed as untyped, never expired, never failed".format(untyped))
    return lines, problems


def request_metrics(root, repo, now):
    """Three counts, or `not recorded` over nothing - a rate over an empty corpus is not a
    measurement (R4/PACK-P)."""
    absent = {"requests_unresolved_by_deadline": "not recorded",
              "requests_fallback_taken": "not recorded",
              "requests_stale_acks": "not recorded", "requests_untyped": 0}
    if not request_log_path(root).is_file():
        return dict(absent, requests_reason="no requests recorded - nothing to count")
    events, errors = read_request_events(root)
    if errors:
        return dict(absent, requests_reason="the requests store could not be read")
    rows = annotate_requests(fold_requests(events), repo, now)
    typed = [r for r in rows if r.get("deadline_at") is not None]
    if not typed:
        return dict(absent, requests_untyped=len(rows),
                    requests_reason="no typed requests recorded - nothing to count")
    return {"requests_unresolved_by_deadline": sum(1 for r in typed if r["overdue"]),
            "requests_fallback_taken": sum(1 for r in typed if r.get("outcome") == "fallback"),
            "requests_stale_acks": sum(1 for r in typed if r["stale"] is True),
            "requests_untyped": len(rows) - len(typed), "requests_reason": ""}


def lease_overlap_lines(root, now):
    """(lines, warns): two live leases from two sessions that cover each other's path and
    neither excepts the other (class CTX-R's detector). A WARN never changes doctor's exit."""
    events, errors, _files = read_events(root)
    if errors:
        return ["lease overlap    NOT CHECKED  [COORD-NOT-CHECKED-RECORD] "
                + _safe("; ".join(errors[:2]), 200)], 0
    leases = list(fold(events, now).values())
    pairs = [(a, b) for i, a in enumerate(leases) for b in leases[i + 1:]
             if a["session"] != b["session"] and overlaps(a["path"], b["path"])
             and not excepted(a, b["path"]) and not excepted(b, a["path"])]
    if not pairs:
        return ["lease overlap    none ({} live lease(s))".format(len(leases))], 0
    lines = []
    for a, b in pairs:
        lines.append("lease overlap    WARN  [COORD-LEASE-OVERLAP] {} holds {} and {} holds {}".format(
            _safe(a["session"], 40), _safe(a["path"], 120),
            _safe(b["session"], 40), _safe(b["path"], 120)))
        lines.append("  remedy      the wider lease re-claims with --except <the peer's path>"
                     " (class CTX-R); a WARN does not change this exit")
    return lines, len(pairs)


# --- git plumbing -----------------------------------------------------------

def _git(repo, *args):
    """Run git and READ THE RESULT BACK. An exit code is not a result (CTRL-E)."""
    try:
        proc = subprocess.run(["git", *args], cwd=str(repo), capture_output=True,
                              text=True, encoding="utf-8", errors="replace", timeout=30)
    except (OSError, subprocess.SubprocessError) as exc:
        return None, "{}: {}".format(exc.__class__.__name__, exc)
    if proc.returncode != 0:
        return None, (proc.stderr or proc.stdout or "git {} failed".format(args[0])).strip()
    return proc.stdout, None


def _git_status(repo, *args, stdin=None):
    """Run git and keep the RETURN CODE. `rev-parse -q --verify` says absent with 1 and broken
    with 128; `_git` above folds both into one error, which would render broken as absent.
    (None, "", reason) when git could not run at all."""
    try:
        proc = subprocess.run(["git", *args], cwd=str(repo), capture_output=True, text=True,
                              encoding="utf-8", errors="replace", timeout=30, input=stdin)
    except (OSError, subprocess.SubprocessError) as exc:
        return None, "", "{}: {}".format(exc.__class__.__name__, exc)
    return proc.returncode, proc.stdout, proc.stderr


# --- leader designation -----------------------------------------------------
# Pattern: compare-and-swap cell + fencing token (Kleppmann; etcd creation revision). The ref
# DECIDES (`update-ref <ref> <new> <old>` is git's own CAS), the ledger RECORDS, the join
# FENCES on the epoch. A union-merged ledger cannot refuse a competing claim (SPK-3), so no
# leader fact is ever read back from the ledger to decide anything.

_LEADER_NOT_CHECKED = "COORD-LEADER-NOT-CHECKED"


def leader_validate(record):
    """The blob's contract; anything else is NOT CHECKED, never a leader and never absent."""
    if not isinstance(record, dict):
        return "not a JSON object"
    leader = record.get("leader")
    if leader is not None and not isinstance(leader, str):
        return "leader is not a string"
    epoch = record.get("epoch")
    if isinstance(epoch, bool) or not isinstance(epoch, int) or epoch < 1:
        return "epoch is not a positive integer"
    for key in ("pinned_at", "expires_at"):
        value = record.get(key)
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            return "{} is not a number".format(key)
    return None


def leader_read(repo):
    """(record, oid, err). (None, None, None) is ABSENT - a read that succeeded and found no
    ref. Every failure is err - rendered NOT CHECKED, never "absent" (R4)."""
    code, out, stderr = _git_status(repo, "rev-parse", "-q", "--verify", LEADER_REF)
    if code == 1 and not out.strip():
        return None, None, None
    if code != 0:
        return None, None, {"code": _LEADER_NOT_CHECKED, "reason": _safe(
            stderr.strip() or "git rev-parse exited {}".format(code), 200)}
    oid = out.strip()
    code, out, stderr = _git_status(repo, "cat-file", "-p", oid)
    if code != 0:
        return None, oid, {"code": _LEADER_NOT_CHECKED, "reason": _safe(
            stderr.strip() or "git cat-file exited {}".format(code), 200)}
    try:
        record = json.loads(out)
    except json.JSONDecodeError as exc:
        return None, oid, {"code": _LEADER_NOT_CHECKED,
                           "reason": "the leader blob is not JSON: {}".format(exc.msg)}
    problem = leader_validate(record)
    if problem:
        return None, oid, {"code": _LEADER_NOT_CHECKED,
                           "reason": "the leader blob is not the contract: " + problem}
    return record, oid, None


def leader_state(record, now):
    """absent | live | expired | released - derived on every read, never stored (DM7)."""
    if record is None:
        return "absent"
    if record.get("leader") is None:
        return "released"
    return "live" if now < float(record["expires_at"]) else "expired"


def leader_decide(action, record, now, me, target, ttl, host=None, tree=None):
    """Pure: (new_record, None) or (None, refusal). Touches neither git nor the clock.

    The invariant it holds (with the CAS in leader_write): at most one live designation, and
    the epoch advances by exactly one on every change of holder - never on a renew.
    """
    state = leader_state(record, now)
    epoch = int(record["epoch"]) if record else 0
    holder = record.get("leader") if record else None

    def refuse(code, because, remedy):
        return None, {"code": code, "because": because, "remedy": remedy,
                      "state": state, "epoch": epoch or None, "holder": holder}

    if action in ("pin", "reclaim"):
        if state == "live":
            return refuse("COORD-LEADER-HELD",
                          "{} leads (epoch {}) for {} s more".format(
                              holder, epoch, int(record["expires_at"] - now)),
                          "wait for a release or the expiry; this contested {} is recorded"
                          .format(action))
        if state == "expired":
            left = float(record["expires_at"]) + LEADER_QUIET - now
            if action == "pin":
                return refuse("COORD-LEADER-EXPIRED",
                              "{}'s designation (epoch {}) expired {} s ago".format(
                                  holder, epoch, int(now - record["expires_at"])),
                              "reclaim after the quiet period ({} s): `coord leader reclaim "
                              "<session>` or `pin --reclaim`".format(LEADER_QUIET))
            if left > 0:
                return refuse("COORD-LEADER-QUIET",
                              "quiet period after {}'s expiry (epoch {}): {} s left".format(
                                  holder, epoch, int(math.ceil(left))),
                              "retry after the quiet period; an in-flight join of the old "
                              "leader may still be finishing")
        elif action == "reclaim" and state == "absent":
            return refuse("COORD-LEADER-ABSENT", "no designation exists - nothing to reclaim",
                          "`coord leader pin <session>`")
        new = {"leader": target, "epoch": epoch + 1, "pinned_at": now,
               "expires_at": now + float(ttl), "ttl": float(ttl), "host": host, "tree": tree}
        return new, None

    if state == "absent":
        return refuse("COORD-LEADER-ABSENT", "no designation exists", "`coord leader pin <session>`")
    if not me or me != holder:
        return refuse("COORD-LEADER-NOT-HOLDER",
                      "{} is held by {}, not {}".format(
                          LEADER_REF, holder or "nobody (released)", me or "an unset AGENT_SESSION"),
                      "only the holder may {}; export AGENT_SESSION=<holder>".format(action))
    if action == "renew":
        if state == "expired":
            return refuse("COORD-LEADER-EXPIRED",
                          "the designation (epoch {}) expired {} s ago".format(
                              epoch, int(now - record["expires_at"])),
                          "a lapsed designation is not renewed; reclaim after the quiet period")
        new = dict(record)
        new["expires_at"] = now + float(record.get("ttl") or ttl)
        return new, None
    if action == "release":
        new = dict(record)
        new["leader"] = None            # the epoch SURVIVES (note-20260919-leader-release-keeps-the-epoch)
        new["released_at"] = now
        return new, None
    return refuse("COORD-LEADER-USAGE", "unknown action {}".format(_safe(action, 40)),
                  "pin | who | renew | release | reclaim")


def leader_write(repo, record, old_oid):
    """hash-object then `update-ref <ref> <new> <old>`: the ONLY writer, and the CAS.

    No `-d`, no `--force`, no `--force-with-lease` anywhere in this file (SPK-2: `--force`
    silently overrides the lease); a test walks every git argv here to keep it so.
    """
    payload = json.dumps(record, sort_keys=True) + "\n"
    code, out, stderr = _git_status(repo, "hash-object", "-w", "--stdin", stdin=payload)
    if code != 0:
        return None, {"code": _LEADER_NOT_CHECKED, "reason": _safe(
            stderr.strip() or "git hash-object exited {}".format(code), 200)}
    new_oid = out.strip()
    code, out, stderr = _git_status(repo, "update-ref", LEADER_REF, new_oid, old_oid or ZERO_OID)
    if code == 0:
        return new_oid, None
    text = (stderr or "").lower()
    if code == 128 and ("cannot lock ref" in text or "already exists" in text or "expected" in text):
        last = stderr.strip().splitlines()[-1] if stderr.strip() else "the ref changed under us"
        return None, {"code": "COORD-LEADER-STALE", "reason": _safe(last, 200)}
    return None, {"code": _LEADER_NOT_CHECKED, "reason": _safe(
        stderr.strip() or "git update-ref exited {}".format(code), 200)}


def leader_metrics(events):
    """The three measures P2 exists to move (proposal §7, P2). R4: an empty corpus is a
    reason, never a zero."""
    rows = [e for e in events if e.get("kind") == "leader"]
    if not rows:
        return {"leader_loss": None, "reclaims": None, "contested_pins": None,
                "reclaim_latency_median_seconds": None,
                "leader_reason": "no leader events recorded"}
    reclaims = [e for e in rows if e.get("action") == "reclaim" and e.get("outcome") == "ok"]
    losses = [e for e in reclaims if e.get("expired_at") is not None]
    latencies = [float(e["at"]) - float(e["expired_at"]) for e in losses]
    contested = sum(1 for e in rows if e.get("action") in ("pin", "reclaim")
                    and e.get("outcome") == "refused" and e.get("code") == "COORD-LEADER-HELD")
    return {"leader_loss": len(losses), "reclaims": len(reclaims), "contested_pins": contested,
            "reclaim_latency_median_seconds": (round(statistics.median(latencies), 1)
                                               if latencies else None),
            "leader_reason": ""}


def _leader_lines(record, state, now):
    lines = ["leader      {}".format(record.get("leader") or "-"),
             "epoch       {}".format(record["epoch"]),
             "state       {}".format(state)]
    if state == "live":
        lines.append("expires in  {} s".format(int(record["expires_at"] - now)))
    elif state == "expired":
        lines.append("expired     {} s ago (reclaimable {} s after expiry)".format(
            int(now - record["expires_at"]), LEADER_QUIET))
    else:
        lines.append("released    {} s ago".format(int(now - record.get("released_at", now))))
    lines.append("host        {}".format(record.get("host") or "-"))
    lines.append("tree        {}".format(record.get("tree") or "-"))
    return [_safe(line, 300) for line in lines]


def leader_doctor_line(repo, now):
    """(line, is_problem) for `coord doctor`: the holder, the epoch, the time left - or
    NOT CHECKED, which counts as a problem because a fence cannot run over it."""
    record, _oid, err = leader_read(repo)
    if err:
        return ("leader           NOT CHECKED  [{}]\n  because     {}\n  remedy      fix the "
                "ref (`git update-ref -d {}` by hand is the rollback), then re-run".format(
                    err["code"], err["reason"], LEADER_REF), True)
    state = leader_state(record, now)
    if state == "absent":
        return "leader           none designated", False
    if state == "live":
        return ("leader           {} epoch {} expires in {} s".format(
            _safe(record["leader"], 80), record["epoch"], int(record["expires_at"] - now)), False)
    if state == "expired":
        return ("leader           EXPIRED {} s ago (epoch {}, was {}) - reclaimable {} s after "
                "expiry".format(int(now - record["expires_at"]), record["epoch"],
                                _safe(record["leader"], 80), LEADER_QUIET), False)
    return "leader           released (epoch {})".format(record["epoch"]), False


def cmd_leader(root, repo, action, args, session, agent, cwd, now):
    as_json = bool(getattr(args, "json", False))
    record, oid, err = leader_read(repo)
    if err:
        if as_json:
            print(json.dumps({"state": "not_checked", "code": err["code"], "reason": err["reason"]}))
        else:
            print("leader NOT CHECKED  [{}]\n  because   {}\n  remedy    fix the condition "
                  "above, then re-run; this is not a pass and it is not \"no leader\"".format(
                      err["code"], err["reason"]))
        return 4
    state = leader_state(record, now)

    if action == "who":
        if as_json:
            payload = dict(record or {})
            payload.update({"state": state, "oid": oid,
                            "expires_in": (float(record["expires_at"]) - now) if record else None})
            print(json.dumps(payload, sort_keys=True))
        elif record is None:
            print("leader      -\nstate       absent (no designation; `coord leader pin <session>`)")
        else:
            print("\n".join(_leader_lines(record, state, now)))
        return 0 if state == "live" else 3

    if action == "pin" and getattr(args, "reclaim", False):
        action = "reclaim"
    if action in ("pin", "reclaim"):
        target = args.leader_session
        me = session or target            # a human pinning from a shell has no AGENT_SESSION
        ttl = float(getattr(args, "ttl", LEADER_TTL) or LEADER_TTL)
        if ttl > TTL_CAP:
            print("COORD-LEADER-TTL-CAP  --ttl {:g} exceeds the cap of {} s\n  because   a "
                  "designation is renewed every {} s, not sized to a session\n  remedy    "
                  "use the default ({} s) and renew".format(ttl, TTL_CAP, LEADER_RENEW, LEADER_TTL))
            return 3
    else:
        if not session:
            print(render({"decision": "not_checked", "path": LEADER_REF,
                          "code": "COORD-NOT-CHECKED-IDENTITY",
                          "reason": "AGENT_SESSION is unset"}), file=sys.stderr)
            return 4
        me, target = session, (record.get("leader") if record else None)
        ttl = float(record.get("ttl") or LEADER_TTL) if record else float(LEADER_TTL)
    host = getattr(args, "host", None) or os.environ.get("AGENT_HOST") or "unknown"
    tree = session_tree_kind(repo, cwd)

    event = {"kind": "leader", "type": "leader", "action": action, "session": me,
             "agent": agent or me, "wi": "WI-0", "path": "-", "at": now,
             "leader": target, "host": host, "tree": tree, "ref_old": oid or ZERO_OID,
             "previous_epoch": (record["epoch"] if record else None)}

    def refused(refusal):
        event.update({"outcome": "refused", "code": refusal["code"],
                      "epoch": record["epoch"] if record else None})
        try:
            append_event(root, event)
        except OSError as exc:
            print("COORD-NOT-CHECKED-RECORD  the refusal was not recorded: {}".format(
                _safe(exc, 200)), file=sys.stderr)
        held = ("\n  held by   {} - epoch {}".format(_safe(refusal["holder"], 80), refusal["epoch"])
                if refusal.get("holder") else "")
        print("{}  {}{}\n  because   {}\n  remedy    {}".format(
            refusal["code"], _safe(target or "-", 80), held, _safe(refusal["because"], 300),
            _safe(refusal["remedy"], 300)))
        return 3

    new, refusal = leader_decide(action, record, now, me, target, ttl, host=host, tree=tree)
    if refusal:
        return refused(refusal)
    new_oid, err = leader_write(repo, new, oid)
    if err and err["code"] == "COORD-LEADER-STALE":
        return refused({"code": err["code"], "holder": None, "epoch": None,
                        "because": err["reason"],
                        "remedy": "another writer won the compare-and-swap; re-read "
                                  "`coord leader who` and retry after {} s".format(LEADER_RETRY)})
    if err:
        print("leader NOT CHECKED  [{}]\n  because   {}\n  remedy    the write did not run; "
              "this is not a pass".format(err["code"], err["reason"]))
        return 4
    event.update({"outcome": "ok", "epoch": new["epoch"], "ref_new": new_oid,
                  "expires_at": new["expires_at"]})
    if action == "reclaim" and state == "expired":
        event["expired_at"] = record["expires_at"]          # metrics: reclaim latency, leader loss
    try:
        append_event(root, event)
    except OSError as exc:
        # F9 (accepted): the ref is the truth and it changed; the missing record is reported.
        print("COORD-NOT-CHECKED-RECORD  {} {} epoch {} took effect but was NOT recorded: {}"
              .format(action, _safe(target or "-", 80), new["epoch"], _safe(exc, 200)),
              file=sys.stderr)
        return 4
    if action == "release":
        print("released  {} epoch {} kept (the next pin advances it)".format(
            _safe(target, 80), new["epoch"]))
    else:
        print("{}  {} epoch {} until +{} s (renew every {} s)".format(
            action, _safe(target, 80), new["epoch"], int(new["expires_at"] - now), LEADER_RENEW))
    return 0


def unique_commits(repo):
    """Commits reachable from HEAD and from NO other ref. Returns (count, reason_code).

    `--all` is FORBIDDEN in this expression. Spike S9 reproduced the recorded bug:
    `git rev-list HEAD --not --all` returns 0 for a branch holding exactly one commit
    that exists nowhere else, because --all implicitly includes HEAD -- so the expression
    reduces to `HEAD --not HEAD` and reports SAFE for the one case the guard exists to
    catch. `--exclude=<branch> --all` fails identically, because it does not exclude HEAD.
    """
    out, err = _git(repo, "rev-parse", "--is-inside-work-tree")
    if err:
        return None, "COORD-NOT-CHECKED-GIT"

    out, err = _git(repo, "symbolic-ref", "-q", "--short", "HEAD")
    if err or not (out or "").strip():
        # Detached: every PR gate runs here, and "does my work exist anywhere else?" has
        # no meaning. Decline. A control that cannot see is not licensed to accuse.
        return None, "COORD-DETACHED"
    current = out.strip()

    out, err = _git(repo, "for-each-ref", "--format=%(refname)", "refs/heads", "refs/remotes")
    if err:
        return None, "COORD-NOT-CHECKED-GIT"
    peers = [r for r in out.split() if r != "refs/heads/" + current]
    if not peers:
        # Nothing to compare against: a fresh repo genuinely has one copy of everything.
        # Reported distinctly so it does not train people to switch the guard off.
        return None, "COORD-NO-PEER-REFS"

    out, err = _git(repo, "rev-list", "HEAD", "--not", *peers)
    if err:
        return None, "COORD-NOT-CHECKED-GIT"
    return len([line for line in out.split() if line]), None


def default_branch(repo):
    """The repository's DEFAULT branch, resolved rather than assumed. Returns (name, None)
    or (None, reason_code).

    The ladder: `refs/remotes/origin/HEAD` (what a clone records) -> a local branch of that
    name -> the remote-tracking ref of that name -> `main` -> `master`. Nothing is guessed:
    a repository that resolves none of these reports COORD-NO-DEFAULT-BRANCH and the caller
    HOLDS, because "merged" cannot be established against a branch nobody named (WT7).
    """
    names = []
    out, err = _git(repo, "symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD")
    if not err and (out or "").strip():
        remote = out.strip()                       # e.g. origin/main
        names.append(remote.split("/", 1)[1] if "/" in remote else remote)
    for candidate in names + ["main", "master"]:
        if not candidate:
            continue
        out, err = _git(repo, "rev-parse", "--verify", "--quiet", "refs/heads/" + candidate)
        if not err and (out or "").strip():
            return candidate, None
        out, err = _git(repo, "rev-parse", "--verify", "--quiet",
                        "refs/remotes/origin/" + candidate)
        if not err and (out or "").strip():
            return "origin/" + candidate, None
    return None, "COORD-NO-DEFAULT-BRANCH"


def commits_ahead_of_default(repo, branch):
    """`git rev-list --count <default>..<branch>` -- the ONLY meaning of "merged" this tool
    uses. Returns (count, default_name, None) or (None, default_name, reason_code).

    DC-142 (recurrence 2, measured 2026-09-13): the old label was derived from
    unique_commits(), whose question is "does every commit exist SOMEWHERE else?" A pushed
    branch answers yes -- every commit is on its remote-tracking ref -- so a frozen tree 21
    commits ahead of main was printed as `clean, merged, unheld` and would have been deleted
    by --remove. "Merged" here means merged into the DEFAULT branch, and the count is printed
    so a reader never has to take the word on trust (IO2).
    """
    default, code = default_branch(repo)
    if code:
        return None, None, code
    if not branch:
        return None, default, "COORD-DETACHED"
    out, err = _git(repo, "rev-list", "--count", "{}..{}".format(default, branch))
    if err or not (out or "").strip().isdigit():
        return None, default, "COORD-NOT-CHECKED-GIT"
    return int(out.strip()), default, None


def staged_paths(repo):
    """Staged paths, NUL-separated. Returns (paths, error).

    S8: `--cached` works before the first commit; appending HEAD is FATAL there, so HEAD
    is never passed. The -z form is required - a path containing a space is otherwise
    split, and one containing a quote is otherwise escaped.
    """
    out, err = _git(repo, "diff", "--cached", "--name-only", "-z")
    if err:
        return None, err
    return [p for p in out.split("\0") if p], None


# --- CLI --------------------------------------------------------------------

def _identity():
    session = os.environ.get("AGENT_SESSION")
    return session, os.environ.get("AGENT_NAME") or session


_SESSION_ID = re.compile(r"[A-Za-z0-9._-]+")   # used with fullmatch: `$` would admit a trailing newline


def session_id_error(session):
    """Why `session` may not become a file name, or None when it may (seam XP -> P3, PLAT-A).

    The id is interpolated into `.agents/log/<session>.jsonl` by append_event and
    append_decision, so `:` is a name NTFS refuses, `/` and `\\` change the directory, `..`
    escapes it, and a character outside `[A-Za-z0-9._-]` is a portability bet. The rule
    REFUSES; it never rewrites, because two ids that differ only in a stripped character
    would silently share one log file.
    """
    if session and _SESSION_ID.fullmatch(session) and session not in (".", ".."):
        return None
    return ("COORD-BAD-SESSION-ID  {}\n  because   the session id becomes the file name"
            " .agents/log/<id>.jsonl; only [A-Za-z0-9._-] is portable across NTFS, APFS and ext4"
            "\n  remedy    export AGENT_SESSION=<letters, digits, '.', '_' or '-'>"
            .format(_safe(repr(session), 120)))


def _build_parser():
    parser = argparse.ArgumentParser(prog="coord", description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="cmd", required=True)

    claim = sub.add_parser("claim", help="declare intent over an artifact set")
    claim.add_argument("--wi", required=True)
    claim.add_argument("--path", required=True)
    claim.add_argument("--ttl", type=float, default=TTL_DEFAULT,
                       help="seconds the lease lasts (default {}; capped at {} unless "
                            "--long-edit names why - a lease is for the MINUTES of the edit, "
                            "DC-163)".format(TTL_DEFAULT, TTL_CAP))
    claim.add_argument("--long-edit", dest="long_edit", metavar="REASON",
                       help="the recorded reason for a --ttl above the cap; it is written "
                            "into the claim event so a queued peer can read why it waits")
    claim.add_argument("--except", dest="excepts", action="append", default=[], metavar="PATH",
                       help="carve this path out of the lease (repeatable): a directory lease "
                            "that excludes a peer's owned files (class CTX-R)")

    chk = sub.add_parser("check", help="may this session touch this path?")
    chk.add_argument("path")
    chk.add_argument("--json", action="store_true")

    rel = sub.add_parser("release", help="drop a lease")
    rel.add_argument("--path", required=True)
    rel.add_argument("--wi", default="WI-0")

    tail = sub.add_parser("tail", help="the merged chronological stream")
    tail.add_argument("-n", type=int, default=20)

    # --- Phase 2: enforcement ---
    hook = sub.add_parser("hook", help="PreToolUse adapter: stdin JSON in, decision JSON out")
    hook.add_argument("--host", choices=["claude", "codex", "copilot", "grok", "agy"],
                      help="native response contract (each host's own decision shape)")
    hook.add_argument("--config", action="store_true",
                      help="print a project hook entry as JSON; never install or trust it")
    sub.add_parser("precommit", help="the universal floor: refuse unclaimed staged paths")
    sub.add_parser("staged-markers", help="refuse staged conflict markers (the pre-merge-commit hook)")
    guard = sub.add_parser("guard", help="refuse to move HEAD over work held in one place")
    guard.add_argument("--fix", action="store_true", help="push, the cheapest second copy")
    ses = sub.add_parser("session", help="one session per working tree; `heartbeat` samples progress")
    ses.add_argument("action", choices=["start", "end", "list", "heartbeat"])
    ses.add_argument("--json", action="store_true")
    # P3: `session heartbeat` is what heartbeat.py calls; a human may call it too.
    ses.add_argument("--flush", action="store_true",
                     help="heartbeat: write the row now (a stop-class event) even inside the "
                          "{} s sample window".format(HEARTBEAT_SAMPLE))
    ses.add_argument("--calls", type=int, default=1, help="heartbeat: tool calls this tick adds")
    ses.add_argument("--file", dest="files", action="append", default=[], metavar="PATH",
                     help="heartbeat: a file touched (counted, never stored)")
    ses.add_argument("--tokens", type=int, default=None, help="heartbeat: tokens, when the host knows")
    ses.add_argument("--host", default=None, help="heartbeat: harness name (default $AGENT_HOST)")
    ses.add_argument("--event", default="", help="heartbeat: the host event that fired")
    ses.add_argument("--wi", default=None, help="heartbeat: work item (default $AGENT_WI or WI-0)")
    trk = sub.add_parser("track", help="the running track: one state per (session, work item) "
                                       "from heartbeats and worktree mtimes - live | stalled | "
                                       "blocked | done; empty corpus is NOT CHECKED")
    trk.add_argument("--json", action="store_true")
    kick = sub.add_parser("kick", help="the kick ladder: 0 notify (note) -> 1 kick (cap {}) -> "
                                       "2 decision request; nothing automatic".format(KICK_CAP))
    kick.add_argument("kick_target", metavar="session")
    kick.add_argument("--wi", default=None, help="work item (default: the track row's)")
    kick.add_argument("--rung", type=int, choices=[0, 1, 2], default=None,
                      help="default: 0 for a blocked track not yet notified, else 1")
    kick.add_argument("--reason", default="", help="appended to the mail body")
    kick.add_argument("--owner", default=None,
                      help="rung 2: the Owner session (default: the live leader)")
    kick.add_argument("--deadline", default=None, metavar="SECONDS",
                      help="rung 2: the decision request's deadline (default {} s)".format(REQUEST_DEADLINE))
    kick.add_argument("--fallback", default=None, metavar="TEXT",
                      help="rung 2: what the kicker does at the deadline; required")
    kick.add_argument("--deadline-at", dest="deadline_at", type=float, default=None, metavar="EPOCH",
                      help="the work item's deadline from the plan row; when passed, a kick is due "
                           "even on a live track")
    lg = sub.add_parser("log", help="ledger maintenance: `portable <file>...` normalizes "
                                    "diagnostic paths in existing rows (F-3)")
    lg.add_argument("action", choices=["portable"])
    lg.add_argument("files", nargs="+")
    collab = sub.add_parser("collaborate", help="cross-session collaboration checks")
    collab.add_argument("action", choices=["check", "summary"])
    collab.add_argument("--json", action="store_true")
    req = sub.add_parser("request", help="a typed seam request: add | receive | ack | resolve | "
                                         "expire | list (sent -> received -> acked -> resolved | expired)")
    req_sub = req.add_subparsers(dest="request_action", required=True)
    req_add = req_sub.add_parser("add", help="send a seam request; refused without a deadline "
                                            "and a fallback (its termination variant)")
    req_add.add_argument("text", nargs="?", default="", help="what is asked (or --contract)")
    req_add.add_argument("--to", required=True)
    req_add.add_argument("--deadline", default=None, metavar="SECONDS",
                         help="seconds until the request must be terminal, or `default` "
                              "({} s); omitting it is refused".format(REQUEST_DEADLINE))
    req_add.add_argument("--fallback", default=None, metavar="TEXT",
                         help="what the requester does at the deadline; omitting it is refused")
    req_add.add_argument("--blob", default="", help="the blob sha the request was written against")
    req_add.add_argument("--ref", default="", help="a mail id (coord mail) this request answers")
    req_add.add_argument("--contract", default="")
    req_add.add_argument("--reason", default="")
    req_add.add_argument("--from-role", default="")
    req_add.add_argument("--path", default="")
    req_receive = req_sub.add_parser("receive", help="the addressee has seen it")
    req_receive.add_argument("id")
    req_ack = req_sub.add_parser("ack", help="acknowledge, pinned to the blob you read")
    req_ack.add_argument("id")
    req_ack.add_argument("--blob", default=None, help="the blob sha you read; required")
    req_list = req_sub.add_parser("list", help="list seam requests")
    req_list.add_argument("--json", action="store_true")
    req_list.add_argument("--status", default="open",
                          choices=["open", "all", "sent", "received", "acked", "resolved",
                                   "expired", "untyped"],
                          help="open = every non-terminal state (default)")
    req_resolve = req_sub.add_parser("resolve", help="resolve a seam request")
    req_resolve.add_argument("id")
    req_resolve.add_argument("--resolution", required=True)
    req_expire = req_sub.add_parser("expire", help="past the deadline: record the fallback as "
                                                   "the outcome - every open one, or <id>")
    req_expire.add_argument("id", nargs="?", default=None)
    # WT1-WT12: a new session starts in a new worktree, and nothing is left behind.
    wt = sub.add_parser("worktree", help="session worktree lifecycle: new | list | cleanup")
    wt.add_argument("action", choices=["new", "list", "cleanup"])
    wt.add_argument("--branch", help="branch to create; name it for the WORK, not the session")
    wt.add_argument("--base", help="commit/branch to branch from (default: the INVOKING tree's HEAD)")
    # WT8 + GO14a: without this, `cleanup --remove` adjudicates EVERY worktree in the
    # repository, which is a wider enforced scope than "clean up my tree" ever states.
    wt.add_argument("--path", dest="wt_path",
                    help="cleanup: consider ONLY this worktree. Without it, cleanup "
                         "adjudicates every worktree in the repository.")
    # `worktree new` is the FIRST command of a session, before AGENT_SESSION is necessarily
    # exported, so the id may be passed directly. Everywhere else the env var remains the
    # convention and this flag simply overrides it.
    wt.add_argument("--session", dest="wt_session",
                    help="session id to register (default: $AGENT_SESSION)")
    wt.add_argument("--remove", action="store_true",
                    help="cleanup: actually delete. Off by default - deletion is irreversible")
    wt.add_argument("--include-unmerged", dest="include_unmerged", action="store_true",
                    help="cleanup: also remove a clean tree whose branch has commits NOT on "
                         "the default branch (a pushed but unmerged branch is HELD by "
                         "default - DC-142). The count is printed either way.")
    met = sub.add_parser("metrics", help="the four measures this layer exists to move")
    met.add_argument("--json", action="store_true")
    # spec-leader-designation: the ref decides, the ledger records, the join fences.
    ld = sub.add_parser("leader", help="designation in {} by compare-and-swap: "
                                        "pin | who | renew | release | reclaim".format(LEADER_REF))
    ld_sub = ld.add_subparsers(dest="leader_action", required=True)
    for verb, text in (("pin", "designate a session (refused while a live leader exists)"),
                       ("reclaim", "take a lapsed designation after the quiet period; epoch + 1")):
        ld_verb = ld_sub.add_parser(verb, help=text)
        ld_verb.add_argument("leader_session", metavar="session")
        ld_verb.add_argument("--ttl", type=float, default=LEADER_TTL,
                             help="seconds until the designation lapses (default {}; renew "
                                  "every {})".format(LEADER_TTL, LEADER_RENEW))
        ld_verb.add_argument("--host", default=None,
                             help="harness name recorded in the blob (default $AGENT_HOST)")
        if verb == "pin":
            ld_verb.add_argument("--reclaim", action="store_true",
                                 help="the same path as `reclaim`: over an EXPIRED "
                                      "designation, after the quiet period")
    ld_who = ld_sub.add_parser("who", help="who leads, as of which epoch, until when")
    ld_who.add_argument("--json", action="store_true")
    ld_sub.add_parser("renew", help="extend the holder's designation (holder only)")
    ld_sub.add_parser("release", help="clear the holder; the epoch survives (holder only)")
    inst = sub.add_parser("install",
                          help="write the pre-commit hook; print the settings entry")
    inst.add_argument("--force", action="store_true",
                      help="install from a linked worktree anyway. It overwrites the "
                           "repository's shared registration with a path that dies with "
                           "this tree - the recorded exception, never the default")

    # --- Phase 3 ---
    cls = sub.add_parser("class", help="what class is this artifact?")
    cls.add_argument("path"); cls.add_argument("--json", action="store_true")
    ci = sub.add_parser("classify", help="write the artifact registry from what this repo has")
    ci.add_argument("action", choices=["init"])
    ci.add_argument("--force", action="store_true",
                    help="replace an existing registry (it is repo configuration)")
    ci.add_argument("--timeout", type=float, default=180)
    md = sub.add_parser("merge-derived", help="the .gitattributes merge driver (0 resolved; 1 with conflict markers)")
    md.add_argument("result"); md.add_argument("base")
    md.add_argument("theirs"); md.add_argument("realpath")
    # P4 / P6: the message layer and the board live in sibling scripts; `coord mail …` and
    # `coord board …` pass every remaining argument through unchanged (one front door).
    ml = sub.add_parser("mail", help="send | read | ack | dispatch (delegates to coord-mail.py)")
    ml.add_argument("mail_args", nargs=argparse.REMAINDER)
    bd = sub.add_parser("board", help="board [--follow] | board post (delegates to coord-board.py)")
    bd.add_argument("board_args", nargs=argparse.REMAINDER)
    dc = sub.add_parser("decide", help="request | rule <n|next> | list (delegates to coord-decide.py)")
    dc.add_argument("decide_args", nargs=argparse.REMAINDER)
    rg = sub.add_parser("regen", help="run the regenerations the driver deferred")
    rg.add_argument("--timeout", type=float, default=120)
    sub.add_parser("doctor", help="is the driver effective? is the registry sane?")
    alloc = sub.add_parser("allocate", help="one collision-proof identifier")
    alloc.add_argument("--scheme", required=True)
    res = sub.add_parser("resolve", help="resolve an id prefix; never picks a first match")
    res.add_argument("prefix"); res.add_argument("--register", required=True)
    mr = sub.add_parser("merge-register", help="union two append-only registers (0 merged; 1 with conflict markers)")
    mr.add_argument("result"); mr.add_argument("base")
    mr.add_argument("theirs"); mr.add_argument("realpath")
    pl = sub.add_parser("plugin", help="emit the bundle both harnesses read; never installs")
    pl.add_argument("--emit", required=True, metavar="DIR")
    pl.add_argument("--host", choices=["copilot"], default=None,
                    help="emit the explicit Copilot lifecycle plugin; default keeps the shared edit guard")
    return parser


MERGE_DRIVER_NAME = "coord-regen"
REGISTER_DRIVER_NAME = "coord-register"


MAX_PATH = 4096
HOOK_MARKER = "# coord-core pre-commit floor"
HOOK_BODY = """#!/bin/sh
{marker}
# The universal floor: every harness has a commit boundary, and no settings key removes it.
exec "{python}" "{script}" precommit
"""


# --- Phase 3: the collision-proof allocator ---------------------------------
#
# KG-B has NINE recorded occurrences of client-minted sequential ids colliding across
# branches, twice reaching main, once silently DESTROYING an entry. The prevention built for
# it scans every remote branch, works, takes about a second over 22 branches -- and collided
# again within the hour, because two sessions that mint before either has pushed are
# invisible to each other by construction. So the only rung that holds is rung 1: make the
# collision impossible to express.
#
# NOT uuid.uuid7: absent on the installed 3.12, present on the "3.x"-pinned CI runner. A
# stdlib call that exists on the runner and not on the developer's machine is PACK-J by
# construction (spike S1).

# ONE implementation, in coord_ids.py, imported by this script AND by audit-log.py. Six
# duplicated lines across two scripts is ONE-A -- the copies are identical at birth and only
# diverge later, when one is edited. The sys.path line is what makes the sibling import work
# both when this file is RUN and when a test loads it via importlib.
_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)
from coord_ids import new_id, resolve_prefix          # noqa: E402  (path set above)
from repo_identity import canonical_project             # noqa: E402  (path set above)


# --- register merges: the half that unique ids do NOT solve ------------------

def entry_fingerprint(row):
    """A stable identity for a register entry, EXCLUDING its id.

    The id is deliberately excluded. In the recorded KG-B instance the two entries had the
    SAME id and different content, and the register's own write-up names them by
    `shortname` rather than by id because rebases had renumbered them three times. A
    fingerprint keyed on the id would both miss the real loss and cry wolf on every
    legitimate renumber.

    `renumbered_from` is excluded for the same reason, and the conservation check found
    that itself: it is provenance ABOUT a merge, not part of the entry's identity, and
    including it made a renumbered entry look destroyed.
    """
    body = {k: v for k, v in row.items() if k not in ("id", "renumbered_from")}
    return json.dumps(body, sort_keys=True, ensure_ascii=False)


def conservation_lost(ours, theirs, merged):
    """Entries present on either side and absent from the merge. Empty means conserved.

    Unique ids stop the COLLISION; only this stops the RESOLUTION from destroying an entry,
    which is what actually happened. The recorded resolution reported "203 ours + 203 theirs
    -> 203 unique" and was caught only because that arithmetic is impossible.
    """
    after = {entry_fingerprint(r) for r in merged}
    lost = []
    for row in list(ours) + list(theirs):
        fp = entry_fingerprint(row)
        if fp not in after and fp not in {entry_fingerprint(x) for x in lost}:
            lost.append(row)
    return lost


def merge_register(ours, theirs, base=None):
    """Union two append-only registers by fingerprint. Returns (merged, lost).

    Append-only means the correct resolution is a union, never a pick. Order is preserved:
    ours first, then whatever theirs adds.

    When `base` is supplied, KG-B's own prescribed resolution also applies: *the id is a
    sequence, not an identity.* The side that already published an id keeps it, and an
    entry this merge INTRODUCES on a colliding id is renumbered from the allocator rather
    than deduped away. NFR-C2 still holds -- nothing already in the base is ever rewritten,
    and with no base the driver cannot tell who published first, so it conserves and does
    not guess.
    """
    merged, seen, taken = [], set(), set()
    published = {str(r.get("id")) for r in (base or [])}
    for source_is_ours, row in ([(True, r) for r in ours] + [(False, r) for r in theirs]):
        fp = entry_fingerprint(row)
        if fp in seen:
            continue
        seen.add(fp)
        eid = str(row.get("id", ""))
        if base is not None and eid in taken and eid not in published:
            row = dict(row)
            scheme = eid.split("-", 1)[0] if "-" in eid else "id"
            row["id"] = new_id(scheme)
            row["renumbered_from"] = eid    # provenance: a renumber must leave a trace
            eid = row["id"]
        taken.add(eid)
        merged.append(row)
    return merged, conservation_lost(ours, theirs, merged)


def _read_jsonl(path):
    rows = []
    for lineno, line in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        rows.append(json.loads(line))       # a parse error propagates: never guess a register
    return rows


def cmd_merge_register(result_path, base_path, theirs_path, real_path):
    """The merge driver for `register`-class artifacts.

    Exit 0 only for a clean union. A merge it cannot make writes conflict markers into the
    result AND exits 1 (REG-C, measured 2026-10-05 in a consuming repo): with exit 0, `git
    merge` read the marker file as a clean merge and auto-committed it. With exit 1 git stops
    and leaves the path unmerged, and the markers in the file keep it from looking clean (the
    S12b hazard: an unmerged file holding only OURS invites `git add .`), where the staged-
    markers scan refuses to commit it.
    """
    try:
        ours = _read_jsonl(result_path)
        theirs = _read_jsonl(theirs_path)
        try:
            base = _read_jsonl(base_path)   # %O: which ids were already published
        except (OSError, json.JSONDecodeError):
            base = None                     # no base -> conserve, but never guess a renumber
    except (OSError, json.JSONDecodeError) as exc:
        # A register we cannot read must not be "merged" -- guessing here is exactly how an
        # entry disappears. Make the failure visible in the file AND to git.
        _write_conflict(result_path, result_path, theirs_path,
                        "{} is unreadable as JSONL ({}); not merging".format(
                            real_path, exc.__class__.__name__))
        return 1
    merged, lost = merge_register(ours, theirs, base=base)
    if lost:
        _write_conflict(result_path, result_path, theirs_path,
                        "{} entry/entries would be lost by this merge".format(len(lost)))
        return 1
    Path(result_path).write_text(
        "".join(json.dumps(r, sort_keys=True) + "\n" for r in merged),
        encoding="utf-8", newline="\n")
    return 0


# --- Phase 3: the artifact-class registry -----------------------------------
#
# The class decides the MECHANISM entirely (ADR-0009). Measured: the six busiest files in
# the reference repo are all generated, so a uniform lease aims at 13/60 and misses 58/60.

# Pattern: Strategy, keyed by artifact class. The class decides the MECHANISM entirely
# (ADR-0009) -- derived artifacts are resolved and regenerated afterwards; registers are
# unioned under a conservation assertion. ONE SOURCE OF TRUTH: `_install_merge_driver`
# builds its driver table from this map, so a class cannot be declared without one.
#
# CTX-H: `hotspot` was declared here for two revisions with no mechanism anywhere in the
# file. The parser accepted `x: hotspot`, `classify()` returned it, and the file then merged
# exactly like `authored` while the tool reported it was handled -- a success-shaped
# classification. It is removed until the commit that implements its merge behaviour puts it
# back, in MERGE_MECHANISMS, where the control can see it.
MERGE_MECHANISMS = {"derived": MERGE_DRIVER_NAME, "register": REGISTER_DRIVER_NAME}
# `authored` is the one legitimate mechanism-free class: the Null Object, the safe default,
# whose mechanism IS conventional conflict markers resolved by a human.
CLASSES = ("authored",) + tuple(sorted(MERGE_MECHANISMS))
REGISTRY_NAME = "artifacts.yml"
REGEN_OWED = "regen-owed.txt"


def load_registry(root):
    """Parse `.agents/artifacts.yml` into [(pattern, class, command)].

    simplify: a line-oriented parser for `pattern: class [command...]` plus `#` comments,
      NOT general YAML.
      ceiling: anchors, nesting, multi-line values.
      upgrade trigger: the first registry a human writes that this rejects.
    Thirty lines against a dependency the pack does not have (NFR-P2) -- the
    Gratuitous-Dependency gate holds at rung 5.

    Raises CoordError; never returns a partly-parsed registry, because a half-read registry
    would silently reclassify whatever it failed to read.
    """
    path = Path(root) / REGISTRY_NAME
    if not path.is_file():
        return None                      # unregistered != empty. The caller says "advisory".
    entries, seen = [], {}
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if ":" not in line:
            raise CoordError("COORD-CLASS-CONFLICT",
                             "{}:{}: expected `pattern: class [command]`".format(
                                 REGISTRY_NAME, lineno))
        pattern, rest = line.split(":", 1)
        pattern = _norm(pattern.strip())
        parts = rest.strip().split(None, 1)
        klass = parts[0] if parts else ""
        command = parts[1].strip() if len(parts) > 1 else ""
        if klass not in CLASSES:
            raise CoordError("COORD-CLASS-CONFLICT",
                             "{}:{}: unknown class {!r}; expected one of {}".format(
                                 REGISTRY_NAME, lineno, klass, ", ".join(CLASSES)))
        # STRIDE B7: a pattern escaping the repository would let the registry point the
        # driver at something outside it.
        bad = _reject_path(pattern)
        if bad:
            raise CoordError("COORD-CLASS-CONFLICT",
                             "{}:{}: {}".format(REGISTRY_NAME, lineno, bad))
        # A derived class with no regenerate command cannot do its job: it would resolve
        # every merge and leave the artifact permanently stale while claiming to be handled.
        if klass == "derived" and not command:
            raise CoordError("COORD-CLASS-CONFLICT",
                             "{}:{}: a `derived` pattern needs a regenerate command".format(
                                 REGISTRY_NAME, lineno))
        if pattern in seen and seen[pattern] != klass:
            # H7: overlapping patterns of DIFFERENT class are a registry error, never a
            # precedence rule -- first-match-wins would make a path's class depend on
            # file ordering.
            raise CoordError("COORD-CLASS-CONFLICT",
                             "{}:{}: {!r} is classified both {!r} and {!r}".format(
                                 REGISTRY_NAME, lineno, pattern, seen[pattern], klass))
        seen[pattern] = klass
        entries.append((pattern, klass, command))
    return entries


def classify(root, path):
    """(class, reason_code). Longest matching pattern wins; the default is `authored`.

    Pattern: Null Object -- an unclassified path yields the SAFE class, so no call site
    needs a branch for "unknown".
    """
    try:
        entries = load_registry(root)
    except CoordError as exc:
        return "authored", exc.code
    if entries is None:
        return "authored", "COORD-CLASS-UNREGISTERED"
    target = _norm(path)
    best = None
    for pattern, klass, _cmd in entries:
        if fnmatch.fnmatch(target, pattern) and (best is None or len(pattern) > len(best[0])):
            best = (pattern, klass)
    return (best[1] if best else "authored"), None


def regen_command(root, path):
    try:
        entries = load_registry(root) or []
    except CoordError:
        return None
    target = _norm(path)
    best = None
    for pattern, klass, cmd in entries:
        if klass == "derived" and fnmatch.fnmatch(target, pattern):
            if best is None or len(pattern) > len(best[0]):
                best = (pattern, cmd)
    return best[1] if best else None



# --- the registry is derivable, not authored (CTX-H, proposal P1) -------------------
#
# These artifacts are the SAME obligation in every repo the pack is installed into: the
# pack generates them, so the pack knows how they merge. Asking each repo to hand-author
# them is asking each repo to repeat the same near-miss -- the cfd-bench coordination plan
# first wrote `audit-log.py regen` from inference, and there is no such subcommand.
#
# A WRONG regenerate command is worse than a missing entry. `load_registry` already refuses
# a `derived` entry with NO command; it cannot refuse one with the wrong command, and the
# failure is silent: the driver resolves the merge, records a regeneration owed, and the
# artifact is permanently stale while every tool reports it handled. So `classify init`
# RUNS each command before it writes it, and refuses the entry if the command fails or
# touches anything but its own target.

INTERPRETER_TOKEN = "python3"
_INTERPRETER_WORDS = ("python3", "python")


def resolve_interpreter(command):
    """Map a registry command's leading interpreter TOKEN to this machine's interpreter.

    `python3` (the documented POSIX name) and `python` (the python.org Windows name) are
    resolved to `sys.executable`, quoted, so the same tracked registry line runs on both
    operating systems. Anything else -- another tool, or an explicit interpreter path -- is
    returned unchanged ON PURPOSE: a stale absolute path must fail loudly where it runs,
    not be silently repaired here while `pack-doctor` reports it (class PLAT-B). Mirrors
    `conductor-join._interp`, which does the same for argv lists; this one takes the shell
    string the registry stores.
    """
    text = (command or "").strip()
    if not text:
        return command
    head, sep, rest = text.partition(" ")
    if head in _INTERPRETER_WORDS:
        return '"{0}"{1}{2}'.format(sys.executable, sep, rest)
    return command


def _canonical_project(repo):
    """The project name, derived from git -- never `basename(cwd)` (PACK-P).

    Delegates to `repo_identity.canonical_project`. This carried its own copy of the same
    resolution ladder; two correct copies of one quantity is DM7/ONE-A, and copies only
    diverge later, when one of them is edited. One implementation, five callers.
    """
    return canonical_project(str(repo))


def pack_defaults(repo):
    """The pack's own artifacts, as classify-init candidates.

    `requires` keeps the registry honest about THIS repo: a pattern naming a path that does
    not exist is a claim nothing checks, and it would start matching the day someone creates
    the file. Everything not listed stays `authored` -- the safe default. Do not enumerate it.
    """
    scripts = "docs/ai-forward-pack/scripts"
    # The registry is TRACKED, so it carries the portable token, never `sys.executable`:
    # a Windows `python.exe` path written here broke `coord regen` on every macOS clone
    # while `pack-doctor` passed (class PLAT-B). `resolve_interpreter` maps the token to
    # THIS machine's interpreter at run time, in every consumer (classify init's
    # verification run, `coord regen`, the merge driver's deferred regeneration).
    py = INTERPRETER_TOKEN
    project = _canonical_project(repo)
    return [
        {"patterns": ["docs/docs-index.js"], "class": "derived",
         "command": "{0} {1}/docs-graph.py derive".format(py, scripts),
         "requires": ["docs/docs-index.js", scripts + "/docs-graph.py"]},
        {"patterns": ["docs/audit/audit-data.js", "docs/audit/index.html"],
         "class": "derived",
         # ONE generator, TWO artifacts: `render` rebuilds the data projection AND ensures
         # the viewer exists. Found by verify_regen_command refusing the single-path form,
         # which is the check doing its job -- a generator owns a SET, and classifying only
         # half of it leaves the other half to conflict by hand forever.
         #
         # --root and --project are NOT optional: the default project name is the repo
         # DIRECTORY name, which stamps a worktree folder into a committed file (PACK-P).
         "command": "{0} {1}/audit-log.py --root docs --project {2} render".format(
             py, scripts, project),
         "requires": ["docs/audit/audit-data.js", scripts + "/audit-log.py"]},
        {"patterns": ["docs/audit/audit-log.jsonl"], "class": "register",
         "command": "", "requires": ["docs/audit/audit-log.jsonl"]},
        {"patterns": ["docs/audit/change-log.jsonl"], "class": "register",
         "command": "", "requires": ["docs/audit/change-log.jsonl"]},
        {"patterns": ["docs/health-history.jsonl"], "class": "register",
         "command": "", "requires": ["docs/health-history.jsonl"]},
    ]


def _dirty_paths(repo):
    """The set of paths git currently reports as changed. Compared as a DELTA.

    Absolute state would never pass: the tree is usually already dirty when someone runs
    this. What must be empty is what the command ADDED.
    """
    out, err = _git(repo, "status", "--porcelain", "--untracked-files=all")
    if err and not out:
        return None                       # R4: unreadable is not the same as clean
    paths = set()
    for line in (out or "").splitlines():
        raw = line[3:].strip() if len(line) > 3 else ""
        if " -> " in raw:                 # a rename reports both sides
            raw = raw.split(" -> ", 1)[1]
        if raw:
            paths.add(_norm(raw.strip('"')))
    return paths


def verify_regen_command(repo, patterns, command, timeout=180):
    """Run it. Return (ok, reason). The near-miss control.

    `patterns` is the set the generator OWNS, not one path: `audit-log.py render` rebuilds
    the data projection and ensures the viewer exists, and both are derived. Declaring half
    a generator's output leaves the other half conflicting by hand forever.

    Two ways to fail, and the second is the subtle one: a command that exits 0 while
    rewriting something outside that set is not a regenerate command, it is a side effect,
    and classifying its target `derived` would licence the driver to resolve a file that
    command will then clobber.
    """
    before = _dirty_paths(repo)
    if before is None:
        return False, "git status is unreadable, so nothing was established"
    try:
        # DEVIATION (Rules of the Road 4): shell=True mirrors cmd_regen, and for the same
        # reason -- a regenerate command may use shell operators and must run identically on
        # POSIX and Windows. The string is pack-derived or repo-local config, never input.
        # The interpreter token is resolved to THIS machine's Python first (PLAT-B).
        proc = subprocess.run(resolve_interpreter(command), cwd=str(repo), shell=True,
                              capture_output=True, text=True, encoding="utf-8",
                              errors="replace", timeout=timeout)
    except subprocess.TimeoutExpired:
        return False, "exceeded {0}s".format(timeout)
    except OSError as exc:
        return False, "{0}: {1}".format(exc.__class__.__name__, exc)
    if proc.returncode != 0:
        detail = _safe(((proc.stderr or "") + (proc.stdout or "")).strip(), 160)
        return False, "exited {0}{1}".format(
            proc.returncode, " - " + detail if detail else "")
    after = _dirty_paths(repo)
    if after is None:
        return False, "git status is unreadable after the run"
    # fnmatch, not set membership: a generator may own a GLOB (`docs/api/*.md`), and that is
    # how `classify` matches too - a stray check stricter than the classifier would refuse
    # every directory-emitting generator.
    owned = [_norm(p) for p in patterns]
    stray = sorted(c for c in (after - before)
                   if not any(fnmatch.fnmatch(c, o) for o in owned))
    if stray:
        return False, "it also changed {0}".format(", ".join(stray[:4]))
    return True, ""


# The registry is TWO things in one file: the pack's own generated artifacts, which are
# derivable and identical in every install, and this repo's own, which only a human knows.
# `--force` regenerates the first half ONLY, between these markers - the same managed-block
# idiom the pack uses for AGENTS.md. Without them, the header's own advice ("re-verify with
# --force after changing a generator") destroyed every hand-added entry, silently, on a
# command the file itself recommends. Same shape as CTX-K: a documented remedy that undoes
# something.
MANAGED_BEGIN = "# >>> coord classify init - managed block. --force regenerates BETWEEN these"
MANAGED_END = "# <<< coord classify init - end managed block. Add your own entries BELOW."

REGISTRY_HEADER = """# .agents/{name} - what each artifact IS decides how it merges.
# Format: pattern: class [regenerate command]. Longest matching pattern wins.
# Everything not listed stays `authored` - the safe default, resolved by a human through
# conventional conflict markers. Do not enumerate it.
#
# Every `derived` command in the managed block was RUN before it was written: a wrong
# command resolves the merge silently and leaves the artifact permanently stale while
# reporting as handled. Re-verify with `coord classify init --force` after changing a
# generator - it rewrites the managed block and leaves everything else alone.
#
# Add THIS repo's own generated and append-only artifacts below the end marker, under the
# same rule: run the command first.
"""


def cmd_classify_init(root, repo, candidates=None, force=False, timeout=180):
    """Write `.agents/artifacts.yml` from what this repo actually has. Verified, not guessed."""
    target = Path(root) / REGISTRY_NAME
    existing = target.read_text(encoding="utf-8") if target.exists() else None
    if existing is not None and not force:
        print("COORD-REGISTRY-EXISTS  {0} already exists - not overwritten.".format(target))
        print("  because     a hand-tuned registry is repo configuration, like .gitignore")
        print("  remedy      re-run with --force to regenerate the managed block, or edit"
              " it by hand")
        return 2
    if existing is not None and (MANAGED_BEGIN not in existing or MANAGED_END not in existing):
        # G11/B6, the same stance `coord install` takes on a foreign pre-commit hook: this
        # file predates the markers or was written by hand, and guessing which lines are ours
        # is how you delete the half nobody can regenerate.
        print("COORD-REGISTRY-UNMANAGED  {0} carries no managed block - not touched.".format(
            target))
        print("  because     without the markers there is no way to tell the pack's entries")
        print("              from yours, and --force would rewrite the whole file")
        print("  remedy      move it aside, run `classify init`, then paste your own entries")
        print("              below the end marker")
        return 2

    candidates = pack_defaults(repo) if candidates is None else candidates
    lines, skipped, absent = [], [], []
    for cand in candidates:
        missing = [r for r in cand.get("requires", []) if not (Path(repo) / r).exists()]
        if missing:
            absent.append((", ".join(cand.get("patterns") or [cand["pattern"]]), missing[0]))
            continue
        patterns = cand.get("patterns") or [cand["pattern"]]
        if cand["class"] == "derived":
            ok, reason = verify_regen_command(repo, patterns, cand["command"], timeout)
            if not ok:
                skipped.append((", ".join(patterns), reason))
                continue
            for pattern in patterns:
                lines.append("{0}: derived {1}".format(pattern, cand["command"]))
        else:
            for pattern in patterns:
                lines.append("{0}: {1}".format(pattern, cand["class"]))

    Path(root).mkdir(parents=True, exist_ok=True)
    block = "\n".join([MANAGED_BEGIN] + lines + [MANAGED_END])
    if existing is None:
        body = REGISTRY_HEADER.format(name=REGISTRY_NAME) + "\n" + block + "\n"
    else:
        head, _old, tail = existing.partition(MANAGED_BEGIN)
        _managed, _end, tail = tail.partition(MANAGED_END)
        body = head + block + tail
        if not body.endswith("\n"):
            body += "\n"
    target.write_text(body, encoding="utf-8", newline="\n")

    try:
        entries = load_registry(root)
    except CoordError as exc:
        # A registry this tool writes that its own parser rejects is the worst outcome.
        print("COORD-REGISTRY-UNPARSEABLE  wrote {0} and could not read it back: {1}".format(
            target, exc.code))
        return 2

    print("{0} {1} - {2} pattern(s) total, {3} in the managed block.".format(
        "Rewrote the managed block of" if existing is not None else "Wrote",
        target, len(entries or []), len(lines)))
    for line in lines:
        print("  {0}".format(line if len(line) <= 110 else line[:107] + "..."))
    for pattern, why in absent:
        print("  not present   {0}  ({1} does not exist here)".format(pattern, why))
    for pattern, why in skipped:
        print("  REFUSED       {0}  its regenerate command {1}".format(pattern, why))
    if skipped:
        print("")
        print("A refused entry is NOT a missing feature - it is the control working. A wrong")
        print("regenerate command resolves every merge and leaves the artifact permanently")
        print("stale while reporting as handled. Fix the command, then re-run with --force.")
        return 3
    print("")
    print("Next: `coord install` declares the drivers in .gitattributes and writes the")
    print("pre-commit floor. Run it ONCE PER CLONE, in the primary checkout: .git/config")
    print("and .git/hooks are shared by every worktree, so a worktree inherits the")
    print("registration and an install inside one overwrites it. `coord doctor` reads the")
    print("state back - including whether the registered path outlives the tree.")
    return 0


# --- the deferred-regeneration debt -----------------------------------------
#
# The driver RESOLVES during the merge and regenerates AFTERWARDS. It cannot regenerate in
# place: git runs merge drivers per file in arbitrary order, so a derived artifact's own
# sources may still be unmerged when its driver runs, and regenerating then produces output
# from a half-merged tree. This is the shape the prior art already uses (sync-generated.ps1
# rebases, then regenerates) and the reason the design was amended during implementation.

def record_regen_owed(root, path):
    owed = set(regen_owed(root))
    owed.add(_norm(path))
    target = Path(root) / REGEN_OWED
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text("".join(p + "\n" for p in sorted(owed)), encoding="utf-8", newline="\n")


def regen_owed(root):
    target = Path(root) / REGEN_OWED
    if not target.is_file():
        return []
    return [l.strip() for l in target.read_text(encoding="utf-8").splitlines() if l.strip()]


def clear_regen_owed(root, paths):
    remaining = [p for p in regen_owed(root) if p not in set(paths)]
    target = Path(root) / REGEN_OWED
    if remaining:
        target.write_text("".join(p + "\n" for p in remaining), encoding="utf-8", newline="\n")
    elif target.exists():
        target.unlink()


def _reject_path(path):
    """Reasons a path may not be checked at all. Returns a reason, or None if it is fine.

    STRIDE B4/B5, tampering: the path is never opened, never globbed against the
    filesystem, and never passed to a shell - but a path that escapes the repository has
    no meaning in a repo-scoped lease, and answering "free" for it would be a false grant.
    """
    if len(path) > MAX_PATH:
        return "the path exceeds the length bound"
    normalised = _norm(path)
    if "\0" in path or not path.strip():
        return "the path contains a NUL or is empty"
    segments = [s for s in normalised.split("/") if s not in ("", ".")]
    depth = 0
    for segment in segments:
        depth += -1 if segment == ".." else 1
        if depth < 0:
            return "the path escapes the repository"
    return None


# --- Phase 3: harness adapters ----------------------------------------------
#
# THE TWO ENVELOPES ARE NOT SIMILAR. Established by execution, not by documentation:
#
#   Claude   {"tool_name": "Edit", "tool_input": {"file_path": "src/a.cs"}}
#   Copilot  {"hookType": "preToolUse",
#             "input": {"cwd": "C:\\repo",
#                       "toolCalls": [{"name": "edit", "args": "{\"path\": \"C:\\repo\\src\\a.cs\"}"}]}}
#
# Copilot batches N tool calls into ONE invocation, `args` is a JSON STRING rather than an
# object, the path field is `path` not `file_path`, and the path is ABSOLUTE. A hook that
# reads tool_input.file_path finds nothing in a Copilot payload and returns "allow" for
# every edit -- a silent no-op wearing the shape of enforcement. The conformance suite
# caught exactly that before any of it shipped.
#
# Shape recorded from ~/.copilot/session-state/*/events.jsonl: 55,541 preToolUse
# invocations, of which `powershell` is the commonest single tool (26,210) -- the
# shell-bypass path named as G4 in the Phase-2 design, here measured rather than supposed.

# Tools that WRITE. Everything else carries no path we care about, and one that carries no
# path must never have one invented for it.
_WRITE_TOOLS = {"edit", "create", "write", "apply_patch", "str_replace", "search_replace", "multiedit",
                "notebookedit", "edit_file", "write_file", "write_to_file",
                "replace_file_content", "multi_replace_file_content"}
_PATH_KEYS = ("file_path", "path", "filePath", "notebook_path", "target_file", "TargetFile")

# CAPABILITY, NOT MEASUREMENT (class CTX-H, proposal P3).
#
# These are spike results about what a HARNESS can do. They are identical in every repo, for
# ever, and they are not a statement about the repo `coord doctor` is running in. Printed
# under the same heading as the six measured lines, a reader takes them as measured state --
# IO5 pointed at the pack's own instrument. `render_harness_capability` gives them their own
# heading, and is the ONLY place either surface formats them.
#
# `established` and `harness_version` are load-bearing: a capability claim with no version has
# no expiry. Copilot's deny was proven against CLI 1.0.80 and the runtime has moved since.
# Where a spike did not pin a version the field says so -- "not recorded" is the honest
# degradation, never a plausible number (IO8). Both dates recovered from git history
# (50b849a, e1ec9d0), not recalled.
HARNESS_STATUS = {
    "claude": {
        "edit_boundary": "enforcing",
        "established": "2026-08-24",
        "harness_version": "not recorded (spike S5 pinned no version)",
        "why": "PreToolUse contract established by execution and the deny response is "
               "honoured (spike S5, five cases incl. both fail-safe paths).",
    },
    "copilot": {
        # Historical proof retained honestly rather than promoted to a current qualification.
        "edit_boundary": "historical",
        "established": "2026-08-24",
        "harness_version": "Copilot CLI 1.0.80",
        "why": "Historical proof only: on Copilot CLI 1.0.80 a live session honoured a "
               "deny (read of an unleased file succeeded; write to a leased file was "
               "refused with our reason rendered verbatim; held bytes were unchanged). The "
               "runtime has moved since, so current enforcement requires a fresh version-"
               "bound qualification. RESIDUAL from the historical proof: Copilot fails OPEN "
               "on a 30s hook timeout, so a hung hook allows; the measured historical check "
               "was 63ms p95 and the commit floor backs it.",
    },
}


def render_harness_capability():
    """The harness capability block, as lines. ONE renderer, both surfaces.

    The two surfaces disagreed for two revisions because each carried its own literal:
    `plugin emit` called Copilot's edit boundary advisory-pending-proof, beside the constant
    recording that the proof had arrived, and the comment above the doctor loop said the same
    superseded thing a third time. Prose restating a verdict is REC-A; a single renderer makes
    the disagreement structurally impossible. The superseded sentences are deliberately not
    reproduced here -- a file that still contains them cannot be grepped clean, and the next
    reader could copy one back out.

    The commit-floor sentence is UNCONDITIONAL. It used to sit behind `if edit_boundary !=
    "enforcing"`, which became unreachable the moment both entries said enforcing -- so the
    one sentence that is true in every state printed in none of them. It is not a consolation
    for a weak harness; it is the floor that holds regardless of what the hook does.
    """
    lines = ["harness capability (from spikes - NOT measured here; re-qualify per version)"]
    for name, status in sorted(HARNESS_STATUS.items()):
        lines.append("  {0:<8} edit boundary: {1}   established {2}, against {3}".format(
            name, status["edit_boundary"], status["established"], status["harness_version"]))
        lines.append("    because   {0}".format(_safe(status["why"], 400)))
        lines.append("    floor     the commit floor enforces regardless of the hook")
    return lines


def _relativise(path, repo, cwd=None):
    """An absolute harness path made repo-relative, or left alone if already relative.

    Both the literal and the RESOLVED form of the path and of each base are compared: on macOS
    a harness hands `/var/folders/...` while the checkout resolves to `/private/var/...`, and a
    prefix miss left the path absolute, matched no lease, and allowed the edit (WT-A test)."""
    if not path:
        return None
    text = _norm(path)
    forms = [text]
    try:
        resolved = _norm(str(Path(path).resolve()))
        if resolved not in forms:
            forms.append(resolved)
    except (OSError, ValueError):
        pass
    for base in (cwd, repo):
        if not base:
            continue
        bases = [_norm(base)]
        try:
            resolved_base = _norm(str(Path(base).resolve()))
            if resolved_base not in bases:
                bases.append(resolved_base)
        except (OSError, ValueError):
            pass
        for candidate in forms:
            for b in bases:
                prefix = b.rstrip("/") + "/"
                if candidate.lower().startswith(prefix.lower()):
                    return candidate[len(prefix):]
    return text


def _patch_paths(command):
    """Extract every native apply_patch target, refusing unfamiliar syntax as a unit.

    This is a bounded target recognizer, not a patch applier. Body text never becomes a
    target. A future native grammar extension needs a contract test before it is accepted.
    """
    if not isinstance(command, str):
        raise ValueError("missing patch")          # no target at all: no decision, as on Claude
    if len(command.encode("utf-8")) > 1048576:
        raise UnreadableWrite("oversized patch")
    # splitlines also splits Unicode filename characters such as U+2028, inventing a
    # different target. Native patches use LF/CRLF records; preserve every other byte.
    lines = [line[:-1] if line.endswith("\r") else line for line in command.strip().split("\n")]
    if len(lines) < 3 or lines[0] != "*** Begin Patch" or lines[-1] != "*** End Patch":
        raise UnreadableWrite("invalid patch envelope")
    paths, operation, moved, body = [], None, False, False
    headers = {"*** Add File: ": "add", "*** Delete File: ": "delete",
               "*** Update File: ": "update"}
    for line in lines[1:-1]:
        header = next((prefix for prefix in headers if line.startswith(prefix)), None)
        if header:
            path = line[len(header):]
            if not path or path != path.strip():
                raise UnreadableWrite("invalid patch target")
            paths.append(path)
            operation, moved, body = headers[header], False, False
        elif line.startswith("*** Move to: ") and operation == "update" and not moved and not body:
            path = line[len("*** Move to: "):]
            if not path or path != path.strip():
                raise UnreadableWrite("invalid move target")
            paths.append(path)
            moved = True
        elif operation == "add" and line.startswith("+"):
            body = True
        elif operation == "update" and (line.startswith(("+", "-", " ", "@@"))
                                        or line == "*** End of File"):
            body = True
        else:
            raise UnreadableWrite("unrecognized patch syntax")
    if not paths or len(paths) > 256:
        raise UnreadableWrite("patch must have 1..256 targets")
    return paths


def _physical_spelling(path):
    """Recover existing component case only when samefile proves a physical alias.

    Path.resolve does not fix case on a case-insensitive volume. Never lowercase lease
    keys globally: two names differing by case may be distinct on another filesystem.
    """
    result = Path(path.anchor)
    for part in path.parts[1:]:
        candidate = result / part
        if candidate.exists():
            for entry in result.iterdir():
                if entry.name.casefold() == part.casefold() and os.path.samefile(entry, candidate):
                    part = entry.name
                    break
        result = result / part
    return result


class UnreadableWrite(ValueError):
    """A native patch whose targets cannot be read. Refused as a unit on every host: the
    host may apply what this recognizer cannot see, so no decision would half-allow it."""


class PrimaryCheckoutWrite(ValueError):
    """A target outside this linked worktree and inside the repository's primary checkout."""


class SiblingCheckoutWrite(ValueError):
    """WT-D: a target inside another LINKED worktree of this repository; `top` is that tree."""

    def __init__(self, top):
        super().__init__(str(top))
        self.top = top


class OutsideCheckout(ValueError):
    """A target in no checkout of this repository. `via_alias` is True when the spelled path
    was inside the checkout and only its resolved target escapes -- that one fails closed."""

    def __init__(self, target, via_alias=False):
        super().__init__(target)
        self.via_alias = via_alias


def _linked_sibling_top(target, base):
    """WT-D: the top of the linked worktree of THIS repository that holds `target`, else None.

    A submodule's `.git` file points into `.git/modules`, not `.git/worktrees`, so repo_root
    does not map it to this repository and it stays outside."""
    probe = target
    while not probe.exists() and probe != probe.parent:
        probe = probe.parent
    top = checkout_top(probe)
    if top == base or not (top / ".git").is_file():
        return None
    try:
        same = os.path.samefile(repo_root(top), repo_root(base))
    except OSError:
        return None
    return top if same else None


def _in_primary_from_linked_tree(target, base):
    """PRIM-A: True when `base` is a LINKED worktree and `target` lies in the primary checkout
    (the main worktree, parent of the common .git dir) but not in `base`. A session working in
    place in the primary is not a linked tree, so the Leader's own work is never this case."""
    out, err = _git(base, "rev-parse", "--git-common-dir")
    if err or not out or not out.strip():
        return False
    common = Path(out.strip())
    common = (common if common.is_absolute() else base / common).resolve()
    if common.name != ".git":
        return False                      # a bare or separate-git-dir layout: no primary tree here
    primary = common.parent
    if primary == base:
        return False
    try:
        target.relative_to(primary)
    except ValueError:
        return False
    return True


def _escapes_from_checkout(lexical, base):
    """WT-E2: does some component of `lexical` sit inside `base` and resolve out of it?

    Found by resolving each prefix, never by comparing spellings: the lexical path need not
    sit under the resolved root (/var -> /private/var on macOS, 8.3 short names on Windows).
    """
    def inside(path):
        try:
            path.resolve().relative_to(base)
            return True
        except ValueError:
            return False
    return any(inside(prefix.parent) and not inside(prefix)
               for prefix in [*reversed(lexical.parents), lexical] if prefix.parent != prefix)


def _native_paths(path, repo, cwd):
    """Check lexical and symlink-resolved targets, relative to the actual process cwd.

    Payload cwd/session fields confer no authority. Outside-checkout paths fail closed,
    including an in-tree symlink to an outside target. Keep both aliases for lease checks.
    """
    if not isinstance(path, str) or not path or len(path) > 4096 or any(ord(c) < 32 for c in path):
        raise ValueError("invalid native path")
    base = Path(repo).resolve()
    candidate = Path(path)
    if not candidate.is_absolute():
        candidate = Path(cwd or repo) / candidate
    lexical = Path(os.path.abspath(candidate))
    paths = []
    canonical = _physical_spelling(candidate.resolve())
    # A resolved outside target is always refused, even if its lexical symlink is inside.
    try:
        canonical.relative_to(base)
    except ValueError:
        if _in_primary_from_linked_tree(canonical, base):
            raise PrimaryCheckoutWrite(str(canonical)) from None
        sibling = _linked_sibling_top(canonical, base)
        if sibling is not None:
            raise SiblingCheckoutWrite(sibling) from None
        raise OutsideCheckout(str(canonical), via_alias=_escapes_from_checkout(lexical, base)) from None
    for target in (lexical, canonical):
        try:
            relative = target.relative_to(base).as_posix()
        except ValueError:
            # macOS /var -> /private/var and a differently-cased checkout root are real
            # aliases. Accept only an ancestor proven to be this exact checkout root.
            alias = next((p for p in target.parents if p.is_dir() and os.path.samefile(p, base)), None)
            if alias is None:
                raise
            relative = target.relative_to(alias).as_posix()
        if relative == ".":
            raise ValueError("native target is a directory root")
        if relative not in paths:
            paths.append(relative)
    return paths


def _native_lease_covers(lease, path, repo):
    """Compare both sides of the native boundary in the checkout's physical namespace.

    Existing lease prefixes may themselves use a symlink or a case alias. For future
    names, use an observed case probe in the target directory, never a platform guess.
    An empty directory cannot answer: only a case-ambiguous collision is then refused.
    """
    base = Path(repo).resolve()
    def physical(pattern):
        return _physical_spelling((base / pattern).resolve()).relative_to(base).as_posix()
    canonical = dict(lease, path=physical(lease["path"]),
                     **{"except": [physical(p) for p in lease.get("except", ())]})
    exact = lease_covers(canonical, path)
    folded = dict(canonical, path=canonical["path"].casefold(),
                  **{"except": [p.casefold() for p in canonical["except"]]})
    insensitive = lease_covers(folded, path.casefold())
    if exact == insensitive:
        return exact
    parent = (base / path).parent
    while not parent.is_dir() and parent != base:
        parent = parent.parent
    entries = list(parent.iterdir())
    names = {entry.name for entry in entries}
    for entry in entries:
        alternate = entry.with_name(entry.name.swapcase())
        if alternate.name != entry.name:
            case_sensitive = (alternate.name in names or not alternate.exists()
                              or not os.path.samefile(entry, alternate))
            return exact if case_sensitive else insensitive
    return exact or insensitive


def parse_hook_request(event, repo, host=None, cwd=None):
    """Normalise any harness's PreToolUse envelope to [(tool_name, repo_relative_path)].

    A path of None means "this tool call carries no path" -- a shell command, a search, a
    read. That is not the same as "no path found", and the difference decides whether the
    layer has an opinion at all.
    """
    if not isinstance(event, dict):
        return []

    if host == "grok":
        event = {"tool_name": event.get("toolName"), "tool_input": event.get("toolInput")}
    elif host == "agy":
        call = event.get("toolCall")
        if not isinstance(call, dict):
            raise ValueError("missing native tool call")
        args = call.get("args")
        if not isinstance(args, dict):
            raise ValueError("invalid native arguments")
        if call.get("name") in ("write_to_file", "replace_file_content", "multi_replace_file_content"):
            if not isinstance(args.get("TargetFile"), str) or not args["TargetFile"]:
                raise ValueError("native write has no target")   # nothing to judge: no decision
            if not Path(args["TargetFile"]).is_absolute():
                raise UnreadableWrite("native TargetFile must be absolute; hook cwd is not the tool resolver")
        # Native tool schemas, not generic aliases, choose the authoritative path.
        event = {"tool_name": call.get("name"), "tool_input": {"file_path": args.get("TargetFile")}}
    if host in ("grok", "agy") and (not isinstance(event.get("tool_name"), str)
            or not event["tool_name"] or not isinstance(event.get("tool_input"), dict)):
        raise ValueError("invalid native tool call")
    if host in ("grok", "agy") and event["tool_name"].lower() not in _WRITE_TOOLS:
        raise ValueError("unsupported tool at native ownership write seam")
    if host == "grok" and event["tool_name"].lower() != "apply_patch":
        args = event["tool_input"]
        paths = [target for key in _PATH_KEYS if args.get(key)
                 for target in _native_paths(args[key], repo, cwd)]
        if not paths:
            raise ValueError("native write has no recognized target")
        return [(event["tool_name"], target) for target in paths]
    if host == "copilot" and any(k in event for k in ("toolName", "tool_name", "toolArgs", "tool_input")):
        tool = str(event.get("toolName") or event.get("tool_name") or "")
        args = event.get("toolArgs") or event.get("tool_input") or {}
        patch_tool = tool.lower() == "apply_patch" or (
            tool == "Edit" and isinstance(args, str)
            and event.get("hook_event_name") in ("PreToolUse", "PostToolUse"))
        raw_patch = args if isinstance(args, str) and patch_tool else None
        if isinstance(args, str):
            try:
                args = json.loads(args)
            except (ValueError, TypeError):
                if raw_patch is not None:
                    args = raw_patch
                else:
                    args = None
        if patch_tool:
            command = args if isinstance(args, str) else (args.get("command") if isinstance(args, dict) else None)
            return [(tool, target) for path in _patch_paths(command)
                    for target in _native_paths(path, repo, cwd)]
        path = None
        if isinstance(args, dict):
            for key in _PATH_KEYS:
                if args.get(key):
                    path = args[key]
                    break
        if tool.lower() in _WRITE_TOOLS:
            if path is None:
                raise ValueError("write call has no recognized path")
            return [(tool, target) for target in _native_paths(path, repo, cwd)]
        return [(tool, _relativise(path, repo, cwd) if path else None)]

    # Copilot: a batch, under input.toolCalls, with args as a JSON string.
    payload = event.get("input")
    if host == "copilot" and not isinstance(payload, dict) and isinstance(event.get("toolCalls"), list):
        payload = event
    if isinstance(payload, dict) and isinstance(payload.get("toolCalls"), list):
        cwd = payload.get("cwd")
        calls = []
        for call in payload["toolCalls"]:
            if not isinstance(call, dict):
                continue
            name = str(call.get("name", ""))
            tool = name.lower()
            args = call.get("args")
            raw_patch = args if isinstance(args, str) and tool == "apply_patch" else None
            if isinstance(args, str):
                try:
                    args = json.loads(args)
                except (ValueError, TypeError):
                    if raw_patch is not None:
                        args = raw_patch
                    elif tool in _WRITE_TOOLS:
                        raise ValueError("malformed write args")
                    else:
                        args = None
            if tool == "apply_patch":
                command = args if isinstance(args, str) else (args.get("command") if isinstance(args, dict) else None)
                if not isinstance(command, str):
                    raise ValueError("write call has no recognized path")
                for patch_path in _patch_paths(command):
                    if host == "copilot":
                        for target in _native_paths(patch_path, repo, cwd):
                            calls.append((name, target))
                    else:
                        calls.append((name, _relativise(patch_path, repo, cwd)))
                continue
            path = None
            if isinstance(args, dict):
                for key in _PATH_KEYS:
                    if args.get(key):
                        if tool in _WRITE_TOOLS and host == "copilot":
                            targets = _native_paths(args[key], repo, cwd)
                            path = targets[0] if targets else None
                        else:
                            path = _relativise(args[key], repo, cwd)
                        break
            if tool in _WRITE_TOOLS and path is None:
                raise ValueError("write call has no recognized path")
            calls.append((name, path))
        return calls

    # Claude and Codex: one call, flat. Codex supplies the entire native patch in command.
    if "tool_name" in event or "tool_input" in event:
        name = str(event.get("tool_name", ""))
        tool_input = event.get("tool_input")
        path = None
        if name.lower() == "apply_patch":
            command = tool_input.get("command") if isinstance(tool_input, dict) else None
            return [(name, target) for path in _patch_paths(command)
                    for target in _native_paths(path, repo, cwd)]
        if isinstance(tool_input, dict):
            for key in _PATH_KEYS:
                if tool_input.get(key):
                    if host:
                        return [(name, target) for target in
                                _native_paths(tool_input[key], repo, cwd)]
                    path = _relativise(tool_input[key], repo)
                    break
        if name.lower() in _WRITE_TOOLS and path is None:
            raise ValueError("write call has no recognized path")
        return [(name, path)]

    return []


def detect_harness(event):
    if isinstance(event, dict) and isinstance(event.get("input"), dict) \
            and "toolCalls" in event["input"]:
        return "copilot"
    return "claude"


def hook_decision_of(response):
    """Read a decision back out of any harness's response envelope.

    Used by the conformance suite so the assertion does not have to know which shape it is
    looking at -- adding a harness means adding a fixture and a branch here, not rewriting
    the tests.
    """
    if not isinstance(response, dict):
        return None
    if not response:
        return "allow"  # No ownership refusal; native permission policy still decides.
    block = response.get("hookSpecificOutput")
    if isinstance(block, dict) and block.get("permissionDecision"):
        return block["permissionDecision"]
    return response.get("permissionDecision") or response.get("decision")


def hook_response_is_valid(response, harness):
    """Does this response match the envelope that harness actually reads?

    Copilot consumes the Claude plugin format, and the recorded corpus does not show the
    response shape -- so both adapters emit the Claude envelope and this returns True for
    both. That is a DELIBERATE, RECORDED assumption, not a verified fact: it is exactly
    what a live Copilot deny would confirm or refute (H13).
    """
    if not isinstance(response, dict):
        return False
    if harness == "copilot" and not response:
        return True
    block = response.get("hookSpecificOutput")
    if not isinstance(block, dict):
        return False
    return (block.get("hookEventName") == "PreToolUse"
            and block.get("permissionDecision") in
            (("allow", "deny") if harness == "codex" else ("allow", "deny", "ask"))
            and isinstance(block.get("permissionDecisionReason"), str))


def hook_response(decision, reason, host=None):
    """The PreToolUse envelope. ALWAYS printed, and the caller ALWAYS exits 0 - the
    harness reads the decision in the JSON, not the exit code. Conflating them would make
    a crashed hook indistinguishable from a refusal.
    """
    if host == "agy" and decision == "allow":
        return ""  # Neutral success: ownership is not permission to autoapprove a tool.
    if host == "copilot" and decision == "allow":
        return "{}"
    if host in ("grok", "agy"):
        return json.dumps({"decision": decision, "reason": reason})
    return json.dumps({"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": decision,
        "permissionDecisionReason": reason}})


def no_decision(host=None):
    """The hook has no opinion: the host's own permission mode decides, as with no hook at all.

    Each host's documented neutral answer. Codex and Grok read exit 0 with empty stdout as
    "continue"; Copilot reads `{}`; agy reads empty success; Claude reads `{}`.
    """
    return "" if host in ("codex", "grok", "agy") else "{}"


def _not_checked(reason, host=None, refuse=False):
    """The answer when the ownership check could not run.

    EDIT-ASK (owner ruling, 2026-10-19): an edit never asks. Claude Code reads `ask` as
    "prompt now" even under bypass permissions, so a path that is itself suspect (`refuse`:
    traversal, an alias out of the checkout) is refused, and anything else gets no decision --
    the session's own permission mode decides, as it would with no hook at all.
    HOST-P (owner ruling, 2026-10-19, FR-100): every host answers as Claude does. Worktrees,
    user memory and scratch directories sit outside the repository, so a native host that
    refused them could not do ordinary work.
    """
    if not refuse:
        return no_decision(host)
    return hook_response("deny", "NOT CHECKED  -\n  held by   unknown - this check did"
                         " not run\n  because   {}\n"
                         "  remedy    fix the condition above; this is not a pass"
                         .format(reason), host)


def _emit(output):
    """Print a hook answer. An empty answer prints nothing: a blank line is not empty stdout."""
    if output:
        print(output)


def cmd_hook(root, session, agent, now, stdin_text, repo=None, host=None, cwd=None):
    """G1: this must never raise. A hook that crashes on a bad payload blocks every edit.

    Envelope-agnostic: `parse_hook_request` normalises whichever harness is calling. Copilot
    BATCHES tool calls, so one invocation can carry several paths -- and if any of them is
    refused the whole batch is refused. A false refusal costs a message; a false grant costs
    a merge.
    """
    try:
        event = json.loads(stdin_text or "")
        if not isinstance(event, dict):
            raise ValueError("payload is not an object")
        if host == "copilot":
            payload = event.get("input") if isinstance(event.get("input"), dict) else event
            tool_cwd = payload.get("cwd")
            if tool_cwd is not None:
                origin = repo_root(cwd or repo or root)
                target = repo_root(tool_cwd) if isinstance(tool_cwd, str) else None
                if not origin or not target or Path(origin).resolve() != Path(target).resolve():
                    raise ValueError("tool cwd is not a checkout of the bound repository")
                cwd = tool_cwd
                repo = checkout_top(tool_cwd)
        try:
            calls = parse_hook_request(event, repo or root, host=host, cwd=cwd)
        except SiblingCheckoutWrite as exc:
            # WT-D: the same check, made in the linked tree that holds the path. The ledger
            # (root) is per repository, so the leases are the same leases.
            repo = str(exc.top)
            calls = parse_hook_request(event, repo, host=host, cwd=cwd)
    except PrimaryCheckoutWrite as exc:
        # PRIM-A: refused by name on every host (Claude's generic not-checked answer is `{}`).
        return hook_response("deny", "COORD-PRIMARY-WRITE  refused  {}\n"
                             "  because   this session works in a linked worktree and the path is in"
                             " the primary checkout, the Leader's fast-forward target\n"
                             "  remedy    write it in your own tree; the primary receives work by merge"
                             .format(_safe(str(exc), 300)), host)
    except OutsideCheckout as exc:
        reason = "the write target is outside this repository's checkouts ({})".format(_safe(str(exc), 300))
        if exc.via_alias:
            return _not_checked(reason, host, refuse=True)
        # WT-E: no lease can cover a path outside every checkout, so this hook has no opinion.
        # `ask` overrode the session's own permission mode on every write to user memory or a
        # scratchpad; `allow` would grant a write anywhere on disk. No decision defers to the
        # mode -- on every host (HOST-P).
        return no_decision(host)
    except UnreadableWrite as exc:
        return _not_checked("unreadable patch ({})".format(_safe(str(exc), 300)), host, refuse=True)
    except Exception as exc:
        return _not_checked("unreadable hook payload ({})".format(exc.__class__.__name__), host)

    if not calls:
        return _not_checked("the payload matched no known harness envelope", host)

    # The PARSER normalises the envelope; the POLICY lives here. Reads are parallel and
    # writes serialize, so a `view` of a leased artifact is allowed -- refusing reads would
    # be both wrong and the fastest way to get the hook switched off.
    paths = [p for name, p in calls if p and str(name).lower() in _WRITE_TOOLS]
    if not paths:
        # G2: powershell, view, grep -- 26,210 of the recorded Copilot invocations are
        # `powershell` alone. A call that carries no path, or only reads one, is allowed.
        return hook_response("allow", "coordination: no write to a coordinated path", host)

    worst = None
    native = host is not None or any(str(name).lower() == "apply_patch" for name, _ in calls)
    covers = (lambda lease, path: _native_lease_covers(lease, path, repo or root)) if native else None
    for path in paths:
        bad = _reject_path(str(path))                   # B4 tampering
        if bad:
            return _not_checked(bad, host, refuse=True)
        # B4 spoofing: identity is the ENVIRONMENT's, never the payload's sessionId.
        decision = check(root, str(path), session, now, covers=covers)
        append_decision(root, session, agent, path, decision,
                        {"hook_host": host, "hook_cwd": str(Path(cwd or repo or root).resolve())}
                        if host else None)
        if decision["decision"] == "deny":
            worst = decision
            break                                       # the batch is already refused
        if decision["decision"] == "not_checked" and worst is None:
            worst = decision

    if worst is None:
        return hook_response("allow", "coordination: {} path(s) free or mine"
                             .format(len(paths)), host)
    if worst["decision"] == "not_checked":
        return _not_checked(render(worst), host)
    return hook_response("deny", render(worst), host)


def cmd_precommit(root, repo, session, agent, now):
    paths, err = staged_paths(repo)
    if err is not None:
        print("COORD-NOT-CHECKED-GIT: {}".format(_safe(err, 300)))
        return 4
    if not paths:
        print("0 staged paths - nothing to check")
        return 0

    # US-8: a repository that has not adopted the layer runs in ADVISORY mode and SAYS SO,
    # rather than implying enforcement it cannot deliver. Blocking every commit in every
    # unconfigured repo is how a floor gets deleted instead of adopted.
    _, _, files = read_events(root)
    if files == 0:
        print("advisory: no coordination record in this repository, so nothing was checked."
              "\n  {} staged path(s) allowed. Run `coord claim` to make this enforcing."
              .format(len(paths)))
        return 0

    refused = []
    for path in paths:
        decision = check(root, path, session, now)
        append_decision(root, session, agent, path, decision)
        if decision["decision"] == "deny":
            refused.append(decision)
        elif decision["decision"] == "not_checked":
            print(render(decision))
            return 4
    if refused:
        for decision in refused:
            print(render(decision))
        print("\n{} of {} staged path(s) are held by another session".format(
            len(refused), len(paths)))
        return 3
    print("{} staged path(s) checked - all free or mine".format(len(paths)))
    return 0


def cmd_guard(repo, fix):
    count, reason = unique_commits(repo)
    if reason == "COORD-DETACHED":
        print("COORD-DETACHED  HEAD is detached, so 'does this exist anywhere else' has no"
              " meaning here. NOT CHECKED - this is not a pass.")
        return 4
    if reason == "COORD-NOT-CHECKED-GIT":
        print("COORD-NOT-CHECKED-GIT  git could not answer; nothing was checked.")
        return 4
    if reason == "COORD-NO-PEER-REFS":
        print("COORD-NO-PEER-REFS  there are no other refs in this repository, so nothing"
              " here exists in a second place.\n  remedy    push, or accept that this is a"
              " fresh repository")
        return 3
    if count:
        out, _ = _git(repo, "rev-list", "--oneline", "-n", "20", "HEAD", "--not",
                      *[r for r in (_git(repo, "for-each-ref", "--format=%(refname)",
                                         "refs/heads", "refs/remotes")[0] or "").split()
                        if r != "refs/heads/" + (_git(repo, "symbolic-ref", "-q",
                                                      "--short", "HEAD")[0] or "").strip()])
        print("COORD-UNIQUE-WORK  {} commit(s) exist here and nowhere else:".format(count))
        for line in (out or "").splitlines():
            print("  {}".format(_safe(line, 120)))
        print("  remedy    push - it is the cheapest way to make the work exist twice")
        if fix:
            _, err = _git(repo, "push")
            if err:
                print("  push failed: {}".format(_safe(err, 200)))
                return 3
            recount, _ = unique_commits(repo)
            if recount:
                print("  push reported success but {} commit(s) are still unique"
                      .format(recount))
                return 3
            print("  pushed - now safe")
            return 0
        return 3
    print("safe to move HEAD - nothing here exists in only one place")
    return 0


def _worktree_key(cwd):
    return str(Path(cwd).resolve()).replace("\\", "/")


def _worktree_label(value):
    """The basename of a worktree path: the value the ledger carries in `worktree` (PLAT-B).

    A linked worktree is a sibling of the primary checkout, so no repo-relative form exists;
    the basename is what the plans and `coord worktree list` already call the tree. A legacy
    absolute value reduces to the same label, so nothing recorded before F-3 is orphaned.
    """
    text = str(value or "").replace("\\", "/").rstrip("/")
    return text.rsplit("/", 1)[-1] if text else ""


def active_sessions(root, now, stale_seconds=SESSION_STALE_SECONDS):
    """Fold the append-only record into active collaboration sessions.

    The session ledger is evidence that someone announced themselves, not proof that nobody
    else exists (DC-024). This fold therefore reports only positive liveness; callers that
    need absence-of-use proof must also inspect the filesystem/worktree state.
    """
    events, errors, files = read_events(root)
    leases = fold(events, now)
    claims_by_session = {}
    for lease in leases.values():
        claims_by_session.setdefault(lease["session"], []).append({
            "path": lease["path"],
            "wi": lease.get("wi", ""),
            "agent": lease.get("agent", lease["session"]),
            "expires": lease.get("expires"),
        })

    live = {}
    for event in events:
        session = event.get("session")
        if not session:
            continue
        if event.get("kind") == "session-start":
            live[session] = {
                "session": session,
                "agent": event.get("agent", session),
                "worktree": event.get("worktree", ""),
                "started_at": event.get("at", 0.0),
                "last_at": event.get("at", 0.0),
                "claims": [],
            }
            continue
        if event.get("kind") == "session-end":
            live.pop(session, None)
            continue
        if session in live:
            live[session]["last_at"] = max(live[session]["last_at"], event.get("at", 0.0))

    sessions = []
    for session, state in live.items():
        if now - state.get("last_at", 0.0) >= stale_seconds:
            continue
        state = dict(state)
        state["claims"] = sorted(claims_by_session.get(session, []), key=lambda c: c["path"])
        sessions.append(state)
    sessions.sort(key=lambda s: (s.get("worktree", ""), s.get("session", "")))
    return sessions, errors, files


def session_contract_path(repo):
    return Path(repo) / SESSION_CONTRACT


def _role_token(value):
    return re.sub(r"[^a-z0-9]+", "", (value or "").lower())


def _strip_cell(value):
    value = value.strip()
    if value.startswith("`") and value.endswith("`") and len(value) >= 2:
        value = value[1:-1]
    return value.strip()


def contract_ownership(repo):
    """Parse the simple ownership tables from the session contract template.

    This is intentionally Markdown-shaped rather than a general Markdown parser: the pack
    owns the template and the rows are `| Path | Why |` under `### <Role> owns`.
    Unknown shapes simply yield no ownership facts; the contract remains human-readable.
    """
    path = session_contract_path(repo)
    if not path.is_file():
        return {}
    ownership, current = {}, None
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        heading = re.match(r"^###\s+(.+?)\s+owns\s*$", raw.strip(), flags=re.IGNORECASE)
        if heading:
            current = heading.group(1).strip()
            ownership.setdefault(current, [])
            continue
        if raw.startswith("### "):
            current = None
            continue
        if not current or not raw.strip().startswith("|"):
            continue
        cells = [_strip_cell(c) for c in raw.strip().strip("|").split("|")]
        if len(cells) < 2 or cells[0].lower() in ("path", "---") or set(cells[0]) <= {"-"}:
            continue
        ownership.setdefault(current, []).append({"path": _norm(cells[0]), "why": cells[1]})
    return {role: rows for role, rows in ownership.items() if rows}


def infer_session_roles(session, agent, ownership):
    """Infer contract role membership from session/agent labels.

    This stays advisory. A false warning costs a message; a false grant is what creates
    cross-owned edits. A later slice can replace this with explicit `COORD_ROLE`.
    """
    tokens = {t for t in re.split(r"[^a-z0-9]+", "{} {}".format(session, agent).lower()) if t}
    roles = []
    for role in ownership:
        role_tokens = [t for t in re.split(r"[^a-z0-9]+", role.lower()) if t]
        if role_tokens and all(token in tokens for token in role_tokens):
            roles.append(role)
    return roles


def owner_rows_for_path(ownership, path):
    owners = []
    for role, rows in ownership.items():
        for row in rows:
            if overlaps(row["path"], path):
                owners.append({"role": role, "path": row["path"], "why": row.get("why", "")})
    return owners


def collaboration_findings(root, repo, now, snapshot=None):
    """Return collaboration health findings.

    This is a small operator gate over the live session fold. It is deliberately advisory:
    it catches the AI-DE class where two sessions had to publish a contract and claim files
    to avoid merge/rebase damage, but it does not pretend a claim is a distributed lock.
    """
    sessions, errors, files = snapshot or active_sessions(root, now)
    findings = []
    if errors:
        findings.append({
            "code": "COORD-COLLAB-NOT-CHECKED-RECORD",
            "severity": "blocker",
            "reason": "coordination record is unreadable: {}".format(_safe("; ".join(errors[:2]), 200)),
        })
        return findings
    if files == 0:
        findings.append({
            "code": "COORD-COLLAB-NOT-CHECKED-EMPTY",
            "severity": "blocker",
            "reason": "0 coordination files scanned; no collaboration state established",
        })
        return findings
    if len(sessions) > 1 and not session_contract_path(repo).is_file():
        findings.append({
            "code": "COORD-COLLAB-NO-CONTRACT",
            "severity": "blocker",
            "reason": "{} active sessions but {} is missing".format(
                len(sessions), SESSION_CONTRACT),
        })
    ownership = contract_ownership(repo)
    if ownership:
        for item in sessions:
            roles = infer_session_roles(item.get("session", ""), item.get("agent", ""), ownership)
            for claim in item.get("claims", []):
                owners = owner_rows_for_path(ownership, claim.get("path", ""))
                if owners and not any(o["role"] in roles for o in owners):
                    findings.append({
                        "code": "COORD-COLLAB-CROSS-OWNED-CLAIM",
                        "severity": "warning",
                        "reason": "session {} claims {} owned by {}".format(
                            item["session"], claim.get("path", ""),
                            ", ".join(sorted({o["role"] for o in owners}))),
                    })
    if len(sessions) > 1:
        no_claims = [s["session"] for s in sessions if not s.get("claims")]
        if no_claims:
            findings.append({
                "code": "COORD-COLLAB-NO-CLAIMS",
                "severity": "warning",
                "reason": "active session(s) with no claims: {}".format(", ".join(sorted(no_claims))),
            })
    return findings


def cmd_session_list(root, now, as_json=False):
    sessions, errors, files = active_sessions(root, now)
    payload = {"files_scanned": files, "sessions": sessions, "errors": errors}
    if as_json:
        print(json.dumps(payload, indent=2, sort_keys=True))
        return 0 if not errors else 4
    if errors:
        print("COORD-NOT-CHECKED-RECORD: {}".format(_safe("; ".join(errors[:2]), 200)))
        return 4
    print("{} active session(s); {} file(s) scanned".format(len(sessions), files))
    for item in sessions:
        print("  {:<24} {:<18} {}  {} claim(s)".format(
            _safe(item["session"], 24),
            _safe(item.get("agent", ""), 18),
            _safe(item.get("worktree", ""), 90),
            len(item.get("claims", []))))
        for claim in item.get("claims", []):
            print("    - {:<30} {}".format(_safe(claim.get("wi", ""), 30),
                                           _safe(claim.get("path", ""), 120)))
    return 0


def cmd_collaborate(root, repo, action, now, as_json=False):
    if action not in ("check", "summary"):
        return 2
    sessions, errors, files = active_sessions(root, now)
    findings = collaboration_findings(root, repo, now, snapshot=(sessions, errors, files))
    request_events, request_errors = read_request_events(root)
    requests = fold_requests(request_events)
    open_requests = [r for r in requests if r.get("status") in REQUEST_OPEN]
    payload = {"files_scanned": files, "active_sessions": sessions, "findings": findings,
               "contract": SESSION_CONTRACT, "contract_exists": session_contract_path(repo).is_file()}
    if action == "summary":
        payload["requests"] = open_requests
        payload["request_errors"] = request_errors
    if as_json:
        print(json.dumps(payload, indent=2, sort_keys=True))
        if action == "summary":
            return 0
        return 3 if any(f.get("severity") == "blocker" for f in findings) else 0
    print("collaboration: {} active session(s); contract {}".format(
        len(sessions), "present" if payload["contract_exists"] else "missing"))
    for item in sessions:
        print("  session {:<24} agent {:<18} claims {}".format(
            _safe(item["session"], 24), _safe(item.get("agent", ""), 18),
            len(item.get("claims", []))))
    if action == "summary":
        print("requests: {} open".format(len(open_requests)))
        for request in open_requests:
            print("  {:<22} -> {:<12} {}".format(
                _safe(request.get("id", ""), 22),
                _safe(request.get("to", ""), 12),
                _safe(request.get("contract", ""), 90)))
    if not findings:
        print("collaboration check: OK")
        return 0
    for finding in findings:
        print("{}  {}  {}".format(finding["severity"].upper(), finding["code"],
                                  finding["reason"]))
    if action == "summary":
        return 0
    return 3 if any(f.get("severity") == "blocker" for f in findings) else 0


def _parse_deadline(raw):
    """Seconds, or `default` -> REQUEST_DEADLINE; None when absent or not a positive number."""
    if raw is None:
        return None
    if str(raw).strip().lower() == "default":
        return float(REQUEST_DEADLINE)
    try:
        value = float(raw)
    except ValueError:
        return None
    return value if value > 0 else None


def _request_twin(root, session, agent, action, rid, now, **extra):
    """The ledger row per transition (`type: request`). The store is the state; this is the
    audit trail - a twin that cannot be written is reported, and never changes the verdict."""
    event = {"kind": "request", "type": "request", "action": action, "id": rid,
             "session": session or "anon", "agent": agent or "anon",
             "wi": "WI-0", "path": "-", "at": now}
    event.update(extra)
    try:
        append_event(root, event)
    except OSError as exc:
        print("COORD-NOT-CHECKED-RECORD  the ledger twin was not recorded: {}".format(
            _safe(exc, 200)), file=sys.stderr)


def _deadline_text(row):
    if row.get("deadline_at") is None:
        return "untyped"
    remaining = row.get("deadline_in", 0)
    return "in {:g}s".format(remaining) if remaining >= 0 else "overdue {:g}s".format(-remaining)


def cmd_request(root, action, now, session, agent, args, repo=None):
    events, errors = read_request_events(root)
    if errors:
        print("COORD-REQUEST-NOT-CHECKED  {}".format(_safe("; ".join(errors[:2]), 200)),
              file=sys.stderr)
        return 4
    store = request_log_path(root)
    who = {"session": session or "anon", "agent": agent or "anon"}

    if action == "add":
        text = args.text or args.contract
        fallback = (args.fallback or "").strip()
        seconds = _parse_deadline(args.deadline)
        missing = [flag for flag, value in (("--deadline", seconds), ("--fallback", fallback),
                                            ("<text>", text)) if not value]
        if missing:
            print("COORD-REQUEST-INCOMPLETE  a seam request without {} has no termination"
                  " variant\n  because   a request nobody answers must still end - by its"
                  " deadline, through its fallback\n  remedy    pass --deadline <seconds|default>"
                  " ({} s) and --fallback <what you do at the deadline>".format(
                      " and ".join(missing), REQUEST_DEADLINE), file=sys.stderr)
            return 2
        rid = new_id("req")
        deadline_at = now + seconds
        record = {"kind": "request-add", "id": rid, "at": now, **who,
                  "from": args.from_role or agent or session or "unknown", "to": args.to,
                  "text": text, "path": _norm(args.path or ""), "deadline_at": deadline_at,
                  "fallback": fallback}
        for key in ("contract", "reason", "blob", "ref"):
            if getattr(args, key, ""):
                record[key] = getattr(args, key)
        append_record(store, record)
        _request_twin(root, session, agent, "add", rid, now, to=args.to, deadline_at=deadline_at)
        print(json.dumps({"id": rid, "status": "sent", "deadline_at": deadline_at}))
        return 0

    if action == "list":
        rows = annotate_requests(fold_requests(events), repo, now)
        if args.status == "open":
            rows = [r for r in rows if r["status"] in REQUEST_OPEN]
        elif args.status != "all":
            rows = [r for r in rows if r["status"] == args.status]
        payload = {"requests": rows, "events_scanned": len(events), "errors": []}
        if args.json:
            print(json.dumps(payload, indent=2, sort_keys=True))
            return 0
        print("{} request(s)".format(len(rows)))
        for row in rows:
            print("  {:<30} {:<9} -> {:<12} {:<14} stale={:<13} {}".format(
                _safe(row.get("id", ""), 30), _safe(row.get("status", ""), 9),
                _safe(row.get("to", ""), 12), _deadline_text(row), str(row["stale"]),
                _safe(row.get("text", ""), 80)))
        return 0

    folded = {r["id"]: r for r in fold_requests(events)}

    if action == "expire":
        if args.id:
            row = folded.get(args.id)
            if row is None:
                print("COORD-REQUEST-NOT-FOUND  {}".format(_safe(args.id, 80)))
                return 4
            if row.get("deadline_at") is None:
                print("COORD-REQUEST-UNTYPED  {} predates deadline/fallback and is never expired;"
                      " resolve it, or re-add it typed".format(_safe(args.id, 80)))
                return 3
            if row["status"] in REQUEST_TERMINAL:
                print("COORD-REQUEST-TERMINAL  {} is already {}".format(
                    _safe(args.id, 80), row["status"]))
                return 3
            if now < row["deadline_at"]:
                print("COORD-REQUEST-NOT-DUE  {} has {:g}s left; the fallback is for the"
                      " deadline, not before it".format(_safe(args.id, 80), row["deadline_at"] - now))
                return 3
            due = [row]
        else:
            due = [r for r in folded.values() if r.get("deadline_at") is not None
                   and r["status"] in REQUEST_OPEN and now >= r["deadline_at"]]
        for row in due:
            append_record(store, {"kind": "request-expire", "id": row["id"], "at": now, **who,
                                  "outcome": "fallback", "fallback": row.get("fallback", "")})
            _request_twin(root, session, agent, "expire", row["id"], now, outcome="fallback",
                          deadline_at=row.get("deadline_at"))
        print("{} expired".format(len(due)))
        for row in due:
            print("  {}  fallback: {}".format(_safe(row["id"], 40),
                                              _safe(row.get("fallback", ""), 200)))
        return 0

    row = folded.get(args.id)
    if row is None:
        print("COORD-REQUEST-NOT-FOUND  {}".format(_safe(args.id, 80)))
        return 4
    if row["status"] in REQUEST_TERMINAL:
        print("COORD-REQUEST-TERMINAL  {} is already {} ({})".format(
            _safe(args.id, 80), row["status"], _safe(row.get("outcome", ""), 20)))
        return 3

    if action == "receive":
        append_record(store, {"kind": "request-receive", "id": args.id, "at": now, **who})
        _request_twin(root, session, agent, "receive", args.id, now)
        print(json.dumps({"id": args.id, "status": "received"}))
        return 0

    if action == "ack":
        if not args.blob:
            print("COORD-REQUEST-ACK-NO-BLOB  {}\n  because   an ack says what you READ; without"
                  " the blob sha a later change is invisible\n  remedy    ack <id> --blob"
                  " $(git hash-object <the artifact>)".format(_safe(args.id, 80)), file=sys.stderr)
            return 2
        append_record(store, {"kind": "request-ack", "id": args.id, "at": now, **who,
                              "blob": args.blob})
        _request_twin(root, session, agent, "ack", args.id, now, blob=args.blob)
        print(json.dumps({"id": args.id, "status": "acked", "blob": args.blob}))
        return 0

    # resolve (the pre-P1 shape, kept)
    append_record(store, {"kind": "request-resolve", "id": args.id, "at": now, **who,
                          "resolution": args.resolution})
    _request_twin(root, session, agent, "resolve", args.id, now, outcome="resolution")
    print(json.dumps({"id": args.id, "status": "resolved", "resolution": args.resolution}))
    return 0


# --- worktree lifecycle (session-worktree-discipline.md WT1-WT12) ------------
# This file already resolves the primary checkout from any tree and keys occupancy by
# worktree, so the lifecycle belongs here rather than in a parallel tool. WT1 makes a fresh
# worktree the default response to a concurrent-writer risk; WT6-WT12 close the half that actually rots:
# an isolation mechanism nobody cleans up becomes a disk of half-finished trees, one of which
# is eventually the only copy of some real work.

def _slug(text):
    return re.sub(r"[^a-z0-9]+", "-", (text or "").lower()).strip("-")[:48] or "session"


def worktree_inventory(repo):
    """Parse `git worktree list --porcelain`. Returns (records, error).

    Porcelain is used rather than the human format because a path containing a space is
    otherwise unparseable - the same reasoning as staged_paths()'s -z form.
    """
    out, err = _git(repo, "worktree", "list", "--porcelain")
    if err:
        return None, err
    records, current = [], {}
    for line in (out or "").splitlines():
        if not line.strip():
            if current:
                records.append(current)
                current = {}
            continue
        key, _, value = line.partition(" ")
        if key == "worktree":
            current["path"] = value
        elif key == "branch":
            current["branch"] = value.replace("refs/heads/", "")
        elif key == "HEAD":
            current["head"] = value
        elif key in ("bare", "detached", "locked", "prunable"):
            current[key] = value or True
    if current:
        records.append(current)
    return records, None


def worktree_is_clean(path):
    """True when there is nothing modified, staged OR UNTRACKED.

    Untracked is the condition that matters most: a new file nobody has committed exists
    nowhere else, so deleting its tree destroys the only copy. `git status --porcelain`
    includes untracked by default and the -z form survives paths with spaces or quotes.
    """
    out, err = _git(path, "status", "--porcelain", "-z")
    if err:
        return None, err
    return not [p for p in (out or "").split("\0") if p.strip()], None


def base_commit(cwd, repo, base):
    """Resolve `--base` against the INVOKING worktree, never the primary (class PACK-P).

    `repo` is deliberately the PRIMARY checkout: the coordination record is per repository,
    which is exactly what `repo_root` exists to answer. But HEAD, @ and every relative ref
    are per WORKTREE, so `git -C <primary> worktree add ... HEAD` run from a linked worktree
    silently bases the new tree on the PRIMARY's commit. That is PACK-P one level along --
    the right primitive for "which repository", used for a question that is "which tree".
    Branch and tag names resolve identically from either tree, so for those this is only ever
    a confirmation, never a change.

    MEASURED HARM, and why this is worse than a wrong directory name: a node that had just
    committed a fix created a tree with `--base HEAD`, silently got the primary's OLDER
    commit, ran the pre-fix script, saw the pre-fix result, and nearly reported a correct fix
    as broken. A tool that silently bases work on the wrong commit will be believed.

    Returns (sha, None) or (None, message).
    """
    ref = "{}^{{commit}}".format(base)
    out, err = _git(cwd, "rev-parse", "--verify", "--quiet", ref)
    if err or not (out or "").strip():
        # cwd need not be inside the repo (a caller may pass an unrelated directory); the
        # primary is then the only tree there is, and falling back to it beats guessing.
        out, err = _git(repo, "rev-parse", "--verify", "--quiet", ref)
    if err or not (out or "").strip():
        return None, "not a commit in this repository: {}".format(_safe(base, 80))
    return out.strip(), None


def classify_removals(attempts, after, exists=os.path.isdir):
    """What ACTUALLY happened to each attempted removal -- read back, never inferred (E14).

    The old summary printed `len(removable) - failed`: a count derived from INTENT, where a
    git call that returned quietly counted as a success. An operator then reads "removed 4 of
    4" while a tree is still there, and an over-reporting cleanup is worse than an
    under-reporting one -- the recovery nobody takes is the one nobody knows is needed.

    REGISTRATION is the discriminator, not the error text, because git de-registers BEFORE it
    deletes. A delete that fails can therefore leave the tree unregistered AND on disk, and
    calling that "not removed" tells the operator to retry something git can no longer see.
    Observed on Windows with a file held open: exit 255, "failed to delete", entry already
    gone. That state is ORPHANED and must be named, because no worktree command will ever
    mention it again.

    `attempts` is [(record, err_text_or_None)]; `after` is the post-prune inventory.
    Returns (removed_paths, refused_pairs, orphaned_pairs).
    """
    still_registered = {_worktree_key(r.get("path", "")) for r in (after or [])}
    removed, refused, orphaned = [], [], []
    for record, err_text in attempts:
        path = record.get("path", "")
        if _worktree_key(path) in still_registered:
            refused.append((path, err_text or "still registered after the attempt"))
        elif exists(path):
            orphaned.append((path, err_text or "directory survived the delete"))
        else:
            removed.append(path)
    return removed, refused, orphaned


def worktree_safety(record, primary, cwd, live_keys, index, include_unmerged=False):
    """WT7, in order, fail-safe. Returns (safe, reason).

    Every condition is a HARD STOP that reports rather than removes. A cleanup that deletes
    on a heuristic will eventually delete the tree that mattered, and that single event ends
    the adoption of the whole practice.
    """
    path = record.get("path", "")
    resolved = _worktree_key(path)
    if resolved == _worktree_key(primary):
        return False, "primary checkout - the reference tree is never cleanup"
    if resolved == _worktree_key(cwd):
        return False, "current working directory - deleting the floor you stand on"
    if record.get("locked"):
        return False, "locked by git"
    # live_keys holds LABELS since F-3; a caller still passing resolved paths is honoured too.
    if resolved in live_keys or _worktree_label(resolved) in live_keys:
        return False, "a live session holds it (coord session start, not ended)"
    if not os.path.isdir(path):
        return True, "directory is gone; only the git metadata remains (prunable)"
    clean, err = worktree_is_clean(path)
    if err:
        return False, "could not read status: {}".format(_safe(err, 120))
    if not clean:
        return False, "uncommitted or untracked changes - the only copy of that work"
    count, code = unique_commits(path)
    if count is None:
        return False, "could not compute unique commits ({})".format(code)
    if count > 0:
        return False, "{} commit(s) exist nowhere else - unmerged work".format(count)
    branch = record.get("branch")
    if branch and index.get(branch, 0) > 1:
        return False, "branch {} is checked out in another tree".format(_safe(branch, 60))
    # DC-142: "merged" is `rev-list --count <default>..<branch> == 0`, never "exists
    # somewhere else". A pushed, open branch passed every hold above; this one catches it.
    ahead, default, code = commits_ahead_of_default(path, branch)
    if code:
        return False, "cannot establish merged ({}): no default branch resolved".format(code)
    if ahead > 0:
        if include_unmerged:
            return True, "clean, {} commit(s) not on {} - removed on --include-unmerged".format(
                ahead, default)
        return False, ("{} commit(s) not on {} - open work; pass --include-unmerged to remove"
                       .format(ahead, default))
    return True, "clean, merged into {} (0 ahead), unheld".format(default)


def cmd_worktree(root, repo, action, cwd, now, session=None, agent=None,
                 branch=None, base=None, remove=False, only=None, include_unmerged=False):
    records, err = worktree_inventory(repo)
    if err:
        print("COORD-NOT-CHECKED-GIT: {}".format(_safe(err, 200)))
        return 4
    primary = records[0]["path"] if records else str(repo)

    if action == "new":
        name = branch or (session and "session/" + _slug(session))
        if not name:
            print("worktree new needs --branch <name> (or --session to derive one).\n"
                  "  Name it for the WORK, not the session (WT5): a tree called\n"
                  "  session-2026-08-22-a is one nobody can ever safely clean up.")
            return 2
        # Sibling of the primary, never inside it: a tree inside the repo would be walked by
        # docs-graph, check-consistency and every test that scans the tree.
        parent = os.path.dirname(os.path.abspath(primary))
        target = os.path.join(parent, "{}-{}".format(os.path.basename(os.path.abspath(primary)),
                                                     _slug(name)))
        if os.path.exists(target):
            print("COORD-WORKTREE-EXISTS  {}\n  remedy    cd there, or pick another --branch"
                  .format(_safe(target, 300)))
            return 3
        # ALWAYS resolve explicitly, including the default. With no --base, `git -C
        # <primary> worktree add` uses the PRIMARY's HEAD while the help promises
        # "current HEAD" -- so the documented contract and the behaviour disagreed in
        # exactly the case WT1 makes common under concurrency. Resolving here makes them agree.
        sha, base_err = base_commit(cwd, repo, base or "HEAD")
        if base_err:
            print("COORD-WORKTREE-BASE-UNRESOLVED: {}\n"
                  "  remedy    pass a --base that exists (a branch, a tag or a commit)"
                  .format(base_err))
            return 2
        out, err = _git(repo, "worktree", "add", "-b", name, target, sha)
        if err:
            print("COORD-WORKTREE-ADD-FAILED: {}".format(_safe(err, 300)))
            return 4
        if session:
            # `worktree new` is the OTHER session-start emitter. Both carry the field or
            # the rate is wrong in the direction that flatters us.
            append_event(root, {"kind": "session-start", "session": session, "tree": "worktree",
                                "agent": agent or session, "wi": "WI-0", "path": "-",
                                "at": now, "worktree": _worktree_label(target)})
        print("worktree ready\n  branch    {}\n  path      {}\n  base      {}  ({})"
              "\n  next      cd {}"
              .format(name, target, sha[:12], _safe(base or "HEAD of this tree", 60),
                      target))
        print("  install   NOT needed here: this tree shares .git/config and .git/hooks"
              "\n            with the primary, so it already carries the drivers and the"
              "\n            pre-commit floor. Installing here would OVERWRITE them.")
        if session:
            print("  session   {} registered there".format(session))
        return 0

    # Shared state for list/cleanup.
    STALE_SECONDS = 8 * 3600
    events, errors, _ = read_events(root)
    if errors:
        print("COORD-NOT-CHECKED-RECORD: {}".format(_safe("; ".join(errors[:2]), 200)))
        return 4
    live = {}
    for event in events:
        key = _worktree_label(event.get("worktree"))
        if event.get("kind") == "session-start":
            live[key] = max(live.get(key, 0.0), event.get("at", 0.0))
        elif event.get("kind") == "session-end":
            live.pop(key, None)
    live_keys = {k for k, t in live.items() if k and now - t < STALE_SECONDS}
    index = {}
    for record in records:
        if record.get("branch"):
            index[record["branch"]] = index.get(record["branch"], 0) + 1

    verdicts = []
    for record in records:
        safe, reason = worktree_safety(record, primary, cwd, live_keys, index,
                                       include_unmerged=include_unmerged)
        verdicts.append((record, safe, reason))

    if action == "list":
        print("{} worktree(s); primary {}".format(len(records), _safe(primary, 200)))
        for record, safe, reason in verdicts:
            print("  {:<10} {:<44} {:<22} {}".format(
                "SAFE" if safe else "HELD",
                _safe(record.get("path", "?"), 140),
                _safe(record.get("branch") or "(detached)", 40),
                reason))
        return 0

    # cleanup: reports by default; --remove is the explicit gate on an irreversible act (WT8).
    #
    # SCOPE, stated rather than assumed (GO14a). Without --path this adjudicates EVERY
    # worktree in the repository -- which is wider than the intent that usually reaches
    # it ("remove the tree I just finished with"). The wide default is kept, because the
    # orphan sweep is the reason the command exists, but it now SAYS so, and --path gives
    # the narrow intent somewhere to be expressed.
    if only:
        wanted = _worktree_key(only)
        scoped = [v for v in verdicts if _worktree_key(v[0].get('path', '')) == wanted]
        if not scoped:
            print('COORD-WORKTREE-NOT-FOUND  {}\n'
                  '  remedy    `coord worktree list` shows the paths git knows about'
                  .format(_safe(only, 300)))
            return 3
        verdicts = scoped
        print('scope   only {}'.format(_safe(only, 200)))
    else:
        print('scope   ALL {} worktree(s) in this repository '
              '(--path <tree> to consider only one)'.format(len(verdicts)))
    removable = [(r, why) for r, safe, why in verdicts if safe]
    held = [(r, why) for r, safe, why in verdicts if not safe]
    # WT12: refusals are printed, because a silent skip is indistinguishable from finding
    # nothing - and the difference is exactly what the human needs.
    for record, why in held:
        print("KEEP    {:<44} {}".format(_safe(record.get("path", "?"), 140), why))
    if not removable:
        print("nothing to remove ({} tree(s) kept)".format(len(held)))
        out, err = _git(repo, "worktree", "prune")   # WT9: metadata still gets tidied
        return 0 if not err else 4
    for record, why in removable:
        print("{} {:<44} {}".format("REMOVE " if remove else "WOULD   ",
                                    _safe(record.get("path", "?"), 140), why))
    if not remove:
        print("\n{} tree(s) are safe to remove. Nothing was deleted - deleting a directory is\n"
              "irreversible and git cannot undo it, so it is opt-in (WT8):\n"
              "  coord worktree cleanup --remove".format(len(removable)))
        return 0
    attempts = []
    for record, _why in removable:
        # No `--force`: cleanup only reaches here for a tree it measured clean INCLUDING
        # untracked files, so git's own refusal of an unclean tree is a second floor under
        # ours, not an obstacle. It also keeps `--force` out of every git argv in this file
        # (spec-leader-designation US-9: `--force` silently overrides `--force-with-lease`).
        out, err = _git(repo, "worktree", "remove", record.get("path", ""))
        attempts.append((record, err))
        if err:
            print("  FAILED  {}: {}".format(_safe(record.get("path", "?"), 140), _safe(err, 160)))
    # WT9: a hand-deleted directory leaves .git/worktrees/<name> behind and git keeps the
    # name reserved, so the next `worktree add` fails describing a state the filesystem does
    # not show. Prune so the administrative record matches reality, then read it back (E14).
    _git(repo, "worktree", "prune")
    after, err = worktree_inventory(repo)
    if err or after is None:
        # PACK-P (the section entry): a verdict with no corpus is not a verdict. The old
        # line printed a count here regardless, which is the failure this whole fix is
        # about -- reporting a number nobody measured.
        print("COORD-NOT-CHECKED-GIT: attempted {} removal(s); the inventory could not be "
              "read back, so what actually happened is UNKNOWN: {}"
              .format(len(attempts), _safe(err or "no inventory", 200)))
        return 4
    removed, refused, orphaned = classify_removals(attempts, after)
    # E14: the count is READ BACK, never derived from intent. `len(removable) - failed`
    # counted attempts that returned quietly as successes, so a tree that survived was
    # reported as gone -- and an over-reporting cleanup is worse than an under-reporting
    # one, because the recovery nobody takes is the one nobody knows is needed.
    for path in refused:
        print("  NOT REMOVED  {:<44} {}".format(_safe(path[0], 140), _safe(path[1], 160)))
    for path, why in orphaned:
        # git de-registers BEFORE deleting, so a directory it could not delete (a live
        # process holding a file, an open handle) is left behind with NOTHING tracking it:
        # `worktree list` will never mention it again. Observed: exit 255, "failed to
        # delete", entry already gone. Say it loudly or it is lost.
        print("  ORPHANED     {:<44} git DE-REGISTERED it but could not delete the "
              "directory ({}); no worktree command will mention it again - delete it "
              "by hand".format(_safe(path, 140), _safe(why, 120)))
    print("removed {}, not removed {}, orphaned {} (of {} attempted); {} tree(s) remain"
          .format(len(removed), len(refused), len(orphaned), len(attempts), len(after)))
    return 4 if (refused or orphaned) else 0



# --- WT4's exception, made countable (class CTX-I, proposal P5) ----------------------
#
# Measured across 48 sessions in three repos: 16 worktrees existed and NOT ONE profiled
# session ran inside one, including three pairs that overlapped in time in a primary
# checkout. WT4 permits the primary as a RECORDED exception -- and an exception with no
# counter becomes the default, which is exactly what that measurement shows happened.
#
# Deliberately not a refusal. There is no baseline for how often the exception is correct,
# and a refusal built on no baseline is tuning from a feeling -- the thing this whole loop
# exists to prevent. Record the fact; argue about enforcement once there is a rate.

def session_tree_kind(repo, cwd):
    """"primary" | "worktree", or None when it cannot be established.

    None is a real answer and must not collapse to either value: a session whose tree could
    not be resolved is not evidence of discipline (IO8).
    """
    if not repo:
        return None
    try:
        records, err = worktree_inventory(repo)
    except Exception:
        return None
    if err or not records:
        return None                       # R4: unresolved is not evidence of discipline
    return ("primary" if _worktree_key(cwd) == _worktree_key(records[0]["path"])
            else "worktree")


def wt4_exception_rate(root):
    """How often did a session start in the primary checkout?

    Sessions recorded before this field existed carry no `tree` and are counted as
    `not_recorded` -- never as `worktree`, which would invent a number in the direction
    that flatters us.
    """
    events, _errors, _files = read_events(root)
    starts = [e for e in events if e.get("kind") == "session-start"]
    recorded = [e for e in starts if e.get("tree") in ("primary", "worktree")]
    in_primary = sum(1 for e in recorded if e.get("tree") == "primary")
    return {"sessions": len(starts),
            "sessions_recorded": len(recorded),
            "not_recorded": len(starts) - len(recorded),
            "in_primary": in_primary,
            # R4 again: a rate over an empty corpus is not a measurement.
            "pct": round(100.0 * in_primary / len(recorded), 1) if recorded else None}


def cmd_session(root, action, session, agent, cwd, now, repo=None):
    # simplify: occupancy is the newest session-start with no matching session-end,
    #   inside a staleness window.
    #   ceiling: a session killed without `session end` holds the tree until it elapses.
    #   upgrade trigger: the first time a human is blocked by a dead session.
    STALE_SECONDS = 8 * 3600
    key = _worktree_key(cwd)
    # F-3 / PLAT-B: the ledger carries the tree's LABEL (basename), never the path; a legacy
    # absolute value is reduced to its label on read (note-20260919-liveness-worktree-field-is-a-label).
    label = _worktree_label(key)
    events, errors, _ = read_events(root)
    if errors:
        print("COORD-NOT-CHECKED-RECORD: {}".format(_safe("; ".join(errors[:2]), 200)))
        return 4
    live = {}
    for event in events:
        if _worktree_label(event.get("worktree")) != label:
            continue
        if event.get("kind") == "session-start":
            live[event.get("session")] = event.get("at", 0.0)
        elif event.get("kind") == "session-end":
            live.pop(event.get("session"), None)
    live = {s: t for s, t in live.items() if now - t < STALE_SECONDS and s != session}

    if action == "start":
        if live:
            holder = sorted(live, key=live.get)[-1]
            print("COORD-WORKTREE-OCCUPIED  {}\n  held by   {}\n  because   one session per"
                  " working tree - two sessions in one tree is how work gets lost\n"
                  "  remedy    use a separate worktree, or run `coord session end` there"
                  .format(_safe(key, 300), _safe(holder)))
            return 3
        append_event(root, {"kind": "session-start", "session": session, "agent": agent,
                            "wi": "WI-0", "path": "-", "at": now, "worktree": label,
                            # WT4's exception, recorded where `coord metrics` can count it.
                            "tree": session_tree_kind(repo, cwd)})
        print("session {} registered in {}".format(session, key))
        return 0

    append_event(root, {"kind": "session-end", "session": session, "agent": agent,
                        "wi": "WI-0", "path": "-", "at": now, "worktree": label})
    print("session {} released {}".format(session, key))
    return 0


def cmd_metrics(root, repo, as_json):
    decisions = read_decisions(root)
    allowed = sum(1 for d in decisions if d.get("kind") == "allowed")
    refused = sum(1 for d in decisions if d.get("kind") == "refused")
    unchecked = sum(1 for d in decisions if d.get("kind") == "not_checked")
    total = allowed + refused
    # G15 / R4: a rate over an empty corpus is not a measurement. Report the absence.
    pct = round(100.0 * allowed / total, 1) if total else None
    unique, unique_reason = unique_commits(repo)
    wt4 = wt4_exception_rate(root)
    leader = leader_metrics(read_events(root)[0])
    requests = request_metrics(root, checkout_top(os.getcwd()), time.time())   # WT-A
    payload = {"decisions": len(decisions), "allowed": allowed, "refused": refused,
               "not_checked": unchecked, "edits_under_lease_pct": pct,
               "unique_commits": unique, "unique_commits_reason": unique_reason,
               "wt4": wt4,
               "reason": "" if total else "no decisions recorded - nothing to rate"}
    payload.update(leader)
    payload.update(requests)
    liveness = liveness_metrics(read_events(root)[0], time.time(), worktree_mtimes(repo))
    payload.update(liveness)
    if as_json:
        print(json.dumps(payload))
        return 0
    print("decisions        {}".format(payload["decisions"]))
    print("  allowed        {}".format(allowed))
    print("  refused        {}".format(refused))
    print("  not checked    {}".format(unchecked))
    print("edits under a held lease   {}".format(
        "{}%".format(pct) if pct is not None else "no decisions recorded - nothing to rate"))
    print("commits existing in one place   {}".format(
        unique if unique is not None else unique_reason))
    if wt4["pct"] is None:
        print("sessions started in the primary   no session carries the tree it started in"
              + (" ({} predate the field)".format(wt4["not_recorded"])
                 if wt4["not_recorded"] else ""))
    else:
        print("sessions started in the primary   {}% ({} of {}){}".format(
            wt4["pct"], wt4["in_primary"], wt4["sessions_recorded"],
            "; {} predate the field".format(wt4["not_recorded"])
            if wt4["not_recorded"] else ""))
        print("  meaning        WT4 allows the primary as a RECORDED exception. A rate that"
              " does not fall is the finding.")
    if leader["leader_reason"]:
        print("leader           {}".format(leader["leader_reason"]))
    else:
        print("leader loss (reclaims after an expiry)   {}".format(leader["leader_loss"]))
        print("  reclaims       {}".format(leader["reclaims"]))
        print("  reclaim latency, median   {}".format(
            "{} s".format(leader["reclaim_latency_median_seconds"])
            if leader["reclaim_latency_median_seconds"] is not None else "no expiry reclaimed"))
        print("  contested pins {}   (a pin or reclaim refused because a live leader existed)"
              .format(leader["contested_pins"]))
    if requests["requests_reason"]:
        print("requests         {}".format(requests["requests_reason"]))
    else:
        print("requests unresolved by deadline   {}   (open past deadline_at with no outcome)"
              .format(requests["requests_unresolved_by_deadline"]))
        print("  fallback taken {}   (expired: the fallback was recorded as the outcome)"
              .format(requests["requests_fallback_taken"]))
        print("  stale acks     {}   (acked blob no longer the artifact's current blob)"
              .format(requests["requests_stale_acks"]))
        if requests["requests_untyped"]:
            print("  untyped        {}   (predate deadline/fallback)".format(
                requests["requests_untyped"]))
    if liveness["heartbeat_reason"]:
        print("heartbeat        {}".format(liveness["heartbeat_reason"]))
    else:
        print("heartbeats       {}   ({} zero-delta = stalls observed)".format(
            liveness["heartbeats"], liveness["stalls_observed"]))
        print("  tracks now     {} live, {} stalled, {} blocked".format(
            liveness["tracks_live"], liveness["tracks_stalled"], liveness["tracks_blocked"]))
        print("kicks            {}   (rung 1, cap {}; {} refused at the cap; {} rung-2 escalations)"
              .format(liveness["kicks"], KICK_CAP, liveness["kicks_refused_cap"],
                      liveness["escalations"]))
        print("  stall detection latency, median   {}".format(
            "{} s".format(liveness["stall_latency_median_s"])
            if liveness["stall_latency_median_s"] is not None else "no kick recorded"))
        print("  false kicks    {}   (a kick followed by the target's progress within {} s)"
              .format(liveness["false_kicks"], STALL_AFTER))
    return 0


# --- P3: progress liveness, the running track, the kick ladder -----------------------------
# spec-liveness-and-track / design-liveness-and-track. Two new row kinds in the SAME ledger
# (`heartbeat` in the beating session's file, `kick-ladder` in the kicker's); the track is a
# pure fold (pattern: event sourcing, as `fold`/`fold_requests`); the ladder refuses, counts
# and records (as `cmd_request`/`cmd_leader`). Nothing here acts on its own (WT11, CO17).

def heartbeat_scratch_path(root, repo, session):
    """The machine-local accumulator between samples. It lives in the git COMMON dir (never
    tracked, shared by every worktree of the clone, no .gitignore line to forget); when there
    is no .git at all it falls back beside the ledgers, under the `.agents/*` ignore."""
    common = Path(repo) / ".git" if repo else None
    base = (common / "coord" / "heartbeat") if (common is not None and common.is_dir()) \
        else (Path(root) / "heartbeat")
    return base / "{}.json".format(session)


def _fresh_scratch(now, since):
    return {"calls": 0, "files": [], "tokens": 0, "tokens_known": False,
            "window_at": float(now), "since": since}


def _read_scratch(path):
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    return data if isinstance(data, dict) else None


def _write_scratch(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(json.dumps(data, sort_keys=True), encoding="utf-8", newline="\n")
    os.replace(str(tmp), str(path))


def _renew_leader_if_holder(repo, session, now):
    """F-1 (note-20260919-liveness-heartbeat-renews-the-leader): a SAMPLED beat from the live
    holder renews; an expired designation is reported, never reclaimed (the epoch is an act)."""
    if not repo or not session:
        return None
    record, oid, err = leader_read(repo)
    if err or record is None or record.get("leader") != session:
        return None
    state = leader_state(record, now)
    if state == "expired":
        return "expired"
    if state != "live":
        return None
    new, refusal = leader_decide("renew", record, now, session, session,
                                 float(record.get("ttl") or LEADER_TTL))
    if refusal:
        return None
    _new_oid, err = leader_write(repo, new, oid)
    return None if err else True


def heartbeat_tick(root, repo, session, agent, now, files=(), calls=1, tokens=None, host="",
                   event="", wi=None, cwd=None, flush=False):
    """Accumulate one host event; write ONE `heartbeat` row when the sample window (100 s) has
    passed or on `flush` (a stop-class event). Returns the row written, else None.

    The row carries COUNTS: calls and distinct files since the previous row, tokens when a host
    exposed them (`not recorded` otherwise - never a plausible number, IO8), `since` = the
    previous row's instant, and `leader_renewed` (F-1). A zero-delta row is legal and renders
    `stalled` (D7). Paths from the host are relativised, counted, never stored or opened.
    """
    scratch = heartbeat_scratch_path(root, repo, session)
    data = _read_scratch(scratch)
    if data is None:                       # first tick, or a corrupt accumulator: start from zero
        data = _fresh_scratch(now, None)
    data["calls"] = int(data.get("calls") or 0) + int(calls)
    seen = [str(f) for f in (data.get("files") or [])]
    for path in files or ():
        rel = _relativise(str(path), repo, cwd) or str(path)
        if rel not in seen and len(seen) < 64:
            seen.append(rel)
    data["files"] = seen
    if tokens is not None:
        data["tokens"] = int(data.get("tokens") or 0) + int(tokens)
        data["tokens_known"] = True
    window_at = float(data.get("window_at") or now)
    if not flush and now - window_at < HEARTBEAT_SAMPLE:
        _write_scratch(scratch, data)
        return None
    row = {"kind": "heartbeat", "session": session, "agent": agent or session,
           "wi": wi or os.environ.get("AGENT_WI") or "WI-0", "path": "-", "at": float(now),
           "worktree": _worktree_label(_worktree_key(cwd or os.getcwd())),
           "host": host or os.environ.get("AGENT_HOST") or "unknown", "event": event or "",
           "calls": int(data["calls"]), "files": len(seen),
           "tokens": int(data["tokens"]) if data.get("tokens_known") else "not recorded",
           "since": data.get("since"),
           "leader_renewed": _renew_leader_if_holder(repo, session, now)}
    append_event(root, row)
    _write_scratch(scratch, _fresh_scratch(now, float(now)))
    return row


def _has_progress(row):
    total = 0
    for key in ("calls", "files", "tokens"):
        value = row.get(key)
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            total += value
    return total > 0


def track_fold(events, now, mtimes=None):
    """Pure fold: ledger rows (+ worktree mtimes by label) -> one row per (session, wi).

    live     the newest beat carries a progress delta and is younger than STALL_AFTER
             (or, with no beat at all, the worktree changed within STALL_AFTER)
    stalled  everything else that is neither blocked nor done - a zero-delta ping is stalled
             however fresh (D7), and unproven liveness (no beat, no worktree) is never live
    blocked  a `blocked` mail twin newer than any `unblocked`; blocked_on = its addressee
    done     a session-end or a `done` twin
    `missed_beats` counts sample windows since the last progress (or the last beat, or the
    start); `kicks` counts rung-1 kicks recorded against (session, wi). Nothing is stored.
    """
    mtimes = mtimes or {}
    tracks, kicks, notified = {}, {}, set()
    for event in events:
        session, kind, at = event.get("session"), event.get("kind"), float(event.get("at") or 0.0)
        if kind == "kick-ladder":
            key = (event.get("to"), event.get("wi"))
            if event.get("outcome") == "ok" and event.get("rung") == 1:
                kicks[key] = kicks.get(key, 0) + 1
            if event.get("outcome") == "ok" and event.get("rung") == 0:
                notified.add(key)
            continue
        if kind == "session-start":
            tracks[session] = {"session": session, "agent": event.get("agent", session),
                               "wi": event.get("wi") or "WI-0",
                               "worktree": _worktree_label(event.get("worktree")),
                               "started_at": at, "last_at": at, "last_beat_at": None,
                               "last_progress_at": None, "calls_last": None, "files_last": None,
                               "beats": 0, "zero_delta_beats": 0, "ended": False, "done": False,
                               "blocked_on": None, "deadline_at": None}
            continue
        row = tracks.get(session)
        if row is None:
            continue
        row["last_at"] = max(row["last_at"], at)
        if kind == "session-end":
            row["ended"] = True
        elif kind == "heartbeat":
            row["beats"] += 1
            row["last_beat_at"] = at
            row["wi"] = event.get("wi") or row["wi"]
            row["calls_last"] = event.get("calls")
            row["files_last"] = event.get("files")
            if _has_progress(event):
                row["last_progress_at"] = at
                row["latest_has_progress"] = True
            else:
                row["zero_delta_beats"] += 1
                row["latest_has_progress"] = False
        elif event.get("type") == "mail":
            if kind == "blocked":
                row["blocked_on"] = event.get("to")
            elif kind == "unblocked":
                row["blocked_on"] = None
            elif kind == "done":
                row["done"] = True

    rows = []
    for row in tracks.values():
        mtime = mtimes.get(row["worktree"])
        if row["beats"]:
            row["source"] = "heartbeat"
        elif mtime is not None:
            row["source"] = "worktree-mtime"
            row["last_progress_at"] = float(mtime)
        else:
            row["source"] = "none"
        evidence = max(v for v in (row["last_at"], row["last_progress_at"], mtime) if v is not None)
        if now - evidence >= SESSION_STALE_SECONDS:
            continue
        anchor = row["last_progress_at"] or row["last_beat_at"] or row["started_at"]
        row["missed_beats"] = max(0, int((now - anchor) // HEARTBEAT_SAMPLE))
        row["stall_age_s"] = round(now - anchor, 1)
        if row["ended"] or row["done"]:
            state = "done"
        elif row["blocked_on"]:
            state = "blocked"
        elif row["source"] == "heartbeat":
            state = ("live" if row.get("latest_has_progress")
                     and now - row["last_progress_at"] < STALL_AFTER else "stalled")
        elif row["source"] == "worktree-mtime":
            state = "live" if now - float(mtime) < STALL_AFTER else "stalled"
        else:
            state = "stalled"
        row["state"] = state
        row["kicks"] = kicks.get((row["session"], row["wi"]), 0)
        row["notified"] = (row["session"], row["wi"]) in notified
        row.pop("latest_has_progress", None)
        rows.append(row)
    order = {s: i for i, s in enumerate(TRACK_STATES)}
    rows.sort(key=lambda r: (order.get(r["state"], 9), r["session"], r["wi"]))
    return rows


def worktree_mtimes(repo):
    """label -> newest change instant per registered worktree: the last commit's time or the
    newest mtime of a modified/untracked file, whichever is later. Read from the world, bounded
    by the changed set (never a walk of the whole tree). A tree git cannot read is absent."""
    mtimes = {}
    if not repo:
        return mtimes
    records, err = worktree_inventory(repo)
    if err:
        return mtimes
    for record in records:
        path = record.get("path") or ""
        if not os.path.isdir(path):
            continue
        code, out, _stderr = _git_status(path, "log", "-1", "--format=%ct")
        newest = float(out.strip()) if code == 0 and out.strip().isdigit() else None
        code, out, _stderr = _git_status(path, "status", "--porcelain", "--untracked-files=all")
        if code == 0:
            for line in out.splitlines():
                name = line[3:].split(" -> ")[-1].strip().strip('"')
                try:
                    stamp = os.path.getmtime(os.path.join(path, name))
                except OSError:
                    continue
                newest = stamp if newest is None else max(newest, stamp)
        if newest is not None:
            mtimes[_worktree_label(path)] = newest
    return mtimes


def _track_render_rows(rows):
    for row in rows:
        row["deadline"] = ("not recorded" if row.get("deadline_at") is None
                           else time.strftime("%H:%M:%S", time.localtime(row["deadline_at"])))
        row["last_progress"] = ("-" if row.get("last_progress_at") is None else
                                time.strftime("%H:%M:%S", time.localtime(row["last_progress_at"])))
    return rows


def cmd_track(root, repo, now, as_json=False):
    events, errors, files = read_events(root)
    if errors:
        print("COORD-NOT-CHECKED-RECORD: {}".format(_safe("; ".join(errors[:2]), 200)))
        return 4
    if not any(e.get("kind") == "session-start" for e in events):
        # R4 / invariant 11: an empty fleet view is NOT CHECKED, never "all quiet".
        print("COORD-TRACK-NOT-CHECKED  {} ledger file(s) scanned, no session-start among them\n"
              "  because   nothing was established about anyone's liveness\n"
              "  remedy    sessions register with `coord session start` (or `coord worktree new`);"
              " hosts beat via heartbeat.py".format(files))
        return 4
    rows = _track_render_rows(track_fold(events, now, worktree_mtimes(repo)))
    if as_json:
        print(json.dumps({"tracks": rows, "files_scanned": files, "now": now,
                          "stall_after_s": STALL_AFTER}, sort_keys=True))
        return 0
    print("{:<8} {:<24} {:<10} {:<15} {:<9} {:>6} {:>5}  {:<16} {}".format(
        "state", "session", "wi", "source", "progress", "missed", "kicks", "blocked-on", "deadline"))
    for row in rows:
        print("{:<8} {:<24} {:<10} {:<15} {:<9} {:>6} {:>5}  {:<16} {}".format(
            row["state"], _safe(row["session"], 24), _safe(row["wi"], 10), row["source"],
            row["last_progress"], row["missed_beats"], row["kicks"],
            _safe(row.get("blocked_on") or "-", 16), row["deadline"]))
    print("{} track(s); {} file(s) scanned; stalled = no progress for {} s or a zero-delta beat"
          " (D7); deadline is not recorded until `coord delegate` lands".format(
              len(rows), files, STALL_AFTER))
    return 0


def _load_mail():
    """coord-mail.py beside this file (the only mail writer); None when not installed."""
    target = os.path.join(_HERE, "coord-mail.py")
    if not os.path.isfile(target):
        return None
    import importlib.util
    spec = importlib.util.spec_from_file_location("coord_mail_for_kick", target)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def cmd_kick(root, repo, target, args, session, agent, now):
    """The kick ladder (CO17): 0 notify (mail `note`) -> 1 kick (mail `kick`, cap KICK_CAP,
    counted) -> 2 decision request (P1 typed request + mail `decision-request`). Every climb -
    ok or refused - is a `kick-ladder` row in the kicker's ledger carrying the target's state,
    stall age and missed beats at that instant (the SRE's measurement)."""
    events, errors, _files = read_events(root)
    if errors:
        print("COORD-KICK-NOT-CHECKED  {}".format(_safe("; ".join(errors[:2]), 200)))
        return 4
    rows = {r["session"]: r for r in track_fold(events, now, worktree_mtimes(repo))}
    row = rows.get(target)
    if row is None:
        print("COORD-KICK-NOT-CHECKED  {}\n  because   no track row exists for that session"
              " (no session-start within {} h)\n  remedy    `coord track` lists who can be kicked"
              .format(_safe(target, 80), SESSION_STALE_SECONDS // 3600))
        return 4
    wi = getattr(args, "wi", None) or row["wi"]
    kicks_before = sum(1 for e in events if e.get("kind") == "kick-ladder" and e.get("rung") == 1
                       and e.get("outcome") == "ok" and e.get("to") == target and e.get("wi") == wi)
    notified = any(e.get("kind") == "kick-ladder" and e.get("rung") == 0 and e.get("outcome") == "ok"
                   and e.get("to") == target and e.get("wi") == wi for e in events)
    deadline_at = getattr(args, "deadline_at", None)
    overdue = deadline_at is not None and now >= float(deadline_at)
    rung = getattr(args, "rung", None)
    if rung is None:
        rung = 0 if (row["state"] == "blocked" and not notified) else 1
    event = {"kind": "kick-ladder", "session": session, "agent": agent or session, "wi": wi,
             "path": "-", "at": now, "to": target, "rung": int(rung), "kicks_before": kicks_before,
             "state": row["state"], "stall_age_s": row.get("stall_age_s"),
             "missed_beats": row.get("missed_beats"), "deadline_at": deadline_at,
             "mail_id": "", "request_id": ""}

    def refused(code, because, remedy, exit_code=3):
        event.update({"outcome": "refused", "code": code})
        append_event(root, event)
        print("{}  {} {}\n  because   {}\n  remedy    {}".format(
            code, _safe(target, 80), _safe(wi, 20), _safe(because, 300), _safe(remedy, 300)))
        return exit_code

    due = overdue or row["state"] == "stalled" or (row["state"] == "blocked"
                                                    and row["missed_beats"] >= 3)
    if rung == 0 and row["state"] != "blocked":
        return refused("COORD-KICK-NOT-DUE", "notify (rung 0) is for a recorded block; the track is {}"
                       .format(row["state"]), "kick a stalled track (rung 1), or wait")
    if rung >= 1 and not due:
        return refused("COORD-KICK-NOT-DUE",
                       "the track is {} (last progress {} s ago, {} missed beat(s)); a kick needs a"
                       " passed deadline or three missed beats (CO17)".format(
                           row["state"], row.get("stall_age_s"), row.get("missed_beats")),
                       "wait, or pass --deadline-at <epoch> when the plan's deadline has passed")
    if rung == 1 and kicks_before >= KICK_CAP:
        return refused("COORD-KICK-CAP", "{} kick(s) already recorded for this work item; the cap is {}"
                       .format(kicks_before, KICK_CAP),
                       "escalate: `coord kick {} --wi {} --rung 2 --fallback <what you do at the"
                       " deadline>` (a decision request to the Owner)".format(target, wi))
    fallback = (getattr(args, "fallback", None) or "").strip()
    if rung == 2 and not fallback:
        print("COORD-KICK-INCOMPLETE  {} {}\n  because   a decision request without a fallback has"
              " no termination variant\n  remedy    pass --fallback <what the coordinator does at"
              " the deadline> (and --deadline <s>, default {} s)".format(
                  _safe(target, 80), _safe(wi, 20), REQUEST_DEADLINE), file=sys.stderr)
        return 2
    owner = None
    if rung == 2:
        owner = getattr(args, "owner", None)
        if not owner:
            record, _oid, err = leader_read(repo)
            if not err and record is not None and leader_state(record, now) == "live":
                owner = record.get("leader")
        if not owner:
            return refused("COORD-KICK-NO-OWNER", "no --owner given and no live leader in {}".format(LEADER_REF),
                           "pass --owner <session>, or `coord leader pin <session>` first")
    mail = _load_mail()
    if mail is None:
        return refused("COORD-KICK-NOT-CHECKED", "coord-mail.py is not beside coord-core.py",
                       "install the message layer (P4)", 4)
    reason = getattr(args, "reason", None) or ""
    body = "{} rung {}: {} {} is {} (last progress {} s ago, {} missed beat(s), {} kick(s) before). {}".format(
        "kick ladder", rung, target, wi, row["state"], row.get("stall_age_s"),
        row.get("missed_beats"), kicks_before, reason).strip()
    try:
        if rung == 0:
            event["mail_id"] = mail.append_mail(root, session, {"to": target, "kind": "note", "body": body,
                                                                "ref": None}, now=now)
        elif rung == 1:
            event["mail_id"] = mail.append_mail(root, session, {"to": target, "kind": "kick", "body": body,
                                                                "ref": None}, now=now)
        else:
            last_kick = [e for e in events if e.get("type") == "mail" and e.get("kind") == "kick"
                         and e.get("to") == target and e.get("session") == session]
            ref = str(last_kick[-1].get("mail_id")) if last_kick else ""
            ns = argparse.Namespace(text=body, to=owner, deadline=getattr(args, "deadline", None) or "default",
                                    fallback=fallback, blob="", ref=ref, contract="", reason="kick-ladder",
                                    from_role="", path="")
            import contextlib
            import io
            captured = io.StringIO()
            with contextlib.redirect_stdout(captured):
                code = cmd_request(root, "add", now, session, agent, ns, repo=repo)
            if code != 0:
                return refused("COORD-KICK-REQUEST-REFUSED", "coord request add exited {}".format(code),
                               "see the request refusal above", code)
            event["request_id"] = json.loads(captured.getvalue().strip().splitlines()[-1])["id"]
            event["mail_id"] = mail.append_mail(root, session, {"to": owner, "kind": "decision-request",
                                                                "body": body, "ref": event["request_id"]},
                                                now=now)
    except mail.MailError as exc:
        return refused(exc.code, str(exc), "fix the mail refusal, then kick again", 3)
    event.update({"outcome": "ok", "code": ""})
    append_event(root, event)
    print("kick rung {} -> {} {}  state {}  stall_age {} s  missed {}  kicks {}/{}  mail {}{}  deadline_at={}".format(
        rung, _safe(target, 80), _safe(wi, 20), row["state"], row.get("stall_age_s"),
        row.get("missed_beats"), kicks_before + (1 if rung == 1 else 0), KICK_CAP, event["mail_id"],
        "  request {}".format(event["request_id"]) if event["request_id"] else "",
        deadline_at if deadline_at is not None else "not-recorded"))
    return 0


def liveness_metrics(events, now, mtimes=None):
    """The P3 measures (proposal §7 row P3): stall-detection latency and the false-kick rate,
    plus the counts they rest on. R4: an empty corpus is a reason, never a zero."""
    beats = [e for e in events if e.get("kind") == "heartbeat"]
    ladder = [e for e in events if e.get("kind") == "kick-ladder"]
    if not beats and not ladder:
        return {"heartbeats": None, "stalls_observed": None, "kicks": None, "kicks_refused_cap": None,
                "escalations": None, "false_kicks": None, "stall_latency_median_s": None,
                "tracks_live": None, "tracks_stalled": None, "tracks_blocked": None,
                "heartbeat_reason": "no heartbeat recorded"}
    kicks = [e for e in ladder if e.get("rung") == 1 and e.get("outcome") == "ok"]
    latencies = [float(e["stall_age_s"]) for e in kicks if e.get("stall_age_s") is not None]
    false_kicks = 0
    for kick in kicks:
        at = float(kick.get("at") or 0.0)
        if any(b.get("session") == kick.get("to") and _has_progress(b)
               and at < float(b.get("at") or 0.0) <= at + STALL_AFTER for b in beats):
            false_kicks += 1
    rows = track_fold(events, now, mtimes)
    return {"heartbeats": len(beats),
            "stalls_observed": sum(1 for b in beats if not _has_progress(b)),
            "kicks": len(kicks),
            "kicks_refused_cap": sum(1 for e in ladder if e.get("code") == "COORD-KICK-CAP"),
            "escalations": sum(1 for e in ladder if e.get("rung") == 2 and e.get("outcome") == "ok"),
            "false_kicks": false_kicks,
            "stall_latency_median_s": round(statistics.median(latencies), 1) if latencies else None,
            "tracks_live": sum(1 for r in rows if r["state"] == "live"),
            "tracks_stalled": sum(1 for r in rows if r["state"] == "stalled"),
            "tracks_blocked": sum(1 for r in rows if r["state"] == "blocked"),
            "heartbeat_reason": ""}


def heartbeat_doctor_line(root, now):
    """(line, is_problem) for `coord doctor` and pack-doctor: who beats, how fresh, how many
    stalled - or `not recorded`, which is not a problem and not a pass (CTX-H)."""
    events, errors, _files = read_events(root)
    if errors:
        return ("heartbeat        NOT CHECKED  the ledger has unreadable rows: {}".format(
            _safe("; ".join(errors[:2]), 200)), True)
    beats = [e for e in events if e.get("kind") == "heartbeat"]
    if not beats:
        return ("heartbeat        not recorded (no heartbeat in .agents/log; wire heartbeat.py at the"
                " host's tool seam)", False)
    rows = track_fold(events, now)
    sessions = {b.get("session") for b in beats}
    newest = max(float(b.get("at") or 0.0) for b in beats)
    counts = {s: sum(1 for r in rows if r["state"] == s) for s in TRACK_STATES}
    return ("heartbeat        {} session(s) beating; newest beat {} s ago; {} stalled, {} live, {} blocked,"
            " {} done".format(len(sessions), int(max(0.0, now - newest)), counts["stalled"],
                              counts["live"], counts["blocked"], counts["done"]), False)


def cmd_log_portable(paths):
    """F-3 migration: normalize only the diagnostic fields handled by the event writer.
    Idempotent (a label maps to itself); every other line is copied byte-for-byte, including
    lines that are not JSON; the writer's own dump (sort_keys) is used for the rewritten rows."""
    for raw in paths:
        path = Path(raw)
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as exc:
            print("{}: NOT CHECKED ({})".format(_safe(raw, 200), exc.__class__.__name__))
            continue
        out, rewritten = [], 0
        for line in text.splitlines(keepends=True):
            body = line.rstrip("\r\n")
            ending = line[len(body):]
            try:
                row = json.loads(body) if body.strip() else None
            except ValueError:
                row = None
            portable = _portable_event(row) if isinstance(row, dict) else row
            if portable != row:
                out.append(json.dumps(portable, sort_keys=True) + ending)
                rewritten += 1
            else:
                out.append(line)
        if rewritten:
            with open(str(path), "wb") as fh:
                fh.write("".join(out).encode("utf-8"))
        print("{}: {} row(s) rewritten".format(_safe(raw, 200), rewritten))
    return 0


INSTALL_IN_WORKTREE = """COORD-INSTALL-IN-WORKTREE  refusing to install from a linked worktree.

A git worktree SHARES `.git/config` and `.git/hooks` with its parent -- there is no
per-worktree config unless `extensions.worktreeConfig` is set, and it is off by default.
"Per-clone" is true; "therefore per-worktree" does not follow. So an install here does not
ADD a registration: it OVERWRITES the repository's one registration, and the pre-commit
floor with it, using a path inside THIS tree.

The tree is temporary and WT8 cleanup deletes it. Every later merge of the paths
.gitattributes declares would then invoke a script that is not there -- and there is no
signature before that moment, because the path resolves and names a byte-identical script.

  remedy    the primary checkout already carries the registration this tree inherits, so
            there is nothing to do here. If it does not, install THERE:
              cd {primary}
              coord install
  measured  2026-09-09: an install inside a worktree repointed both merge.coord-*.driver
            and the shared pre-commit hook at that tree's copy of the script.
  override  --force, for the recorded exception. It writes a path that dies with this tree.
"""


def cmd_install(repo, root, force=False):
    # The instruction this refusal replaces lived in nine pack surfaces and was wrong in
    # every one of them. A control at the "make it impossible" rung, because the harm is
    # invisible until the tree is gone and `doctor` called the drivers effective throughout.
    if not force and session_tree_kind(repo, os.getcwd()) == "worktree":
        # None is NOT "worktree": an unresolved tree is not evidence of a violation (R4).
        print(INSTALL_IN_WORKTREE.format(primary=_safe(str(repo), 300)))
        return 2

    hooks, err = _git(repo, "rev-parse", "--git-path", "hooks")
    if err:
        print("COORD-NOT-CHECKED-GIT  {}".format(_safe(err, 200)))
        return 4
    hooks_dir = Path(hooks.strip())
    if not hooks_dir.is_absolute():
        hooks_dir = Path(repo) / hooks_dir
    hooks_dir.mkdir(parents=True, exist_ok=True)
    target = hooks_dir / "pre-commit"
    # .git/hooks is per-clone, so an absolute interpreter path is correct HERE (never in a
    # tracked file). Both paths are forward-slashed: the hook runs under `sh` (Git Bash on
    # Windows), and only the script path was normalised before (XP-06).
    body = HOOK_BODY.format(marker=HOOK_MARKER, python=sys.executable.replace("\\", "/"),
                            script=str(Path(__file__).resolve()).replace("\\", "/"))
    if target.exists():
        existing = target.read_text(encoding="utf-8", errors="replace")
        if HOOK_MARKER not in existing:
            # G11 / B6: never overwrite somebody else's hook.
            print("COORD-HOOK-EXISTS  {} already exists and is not ours - not overwritten."
                  "\n  remedy    merge the two by hand, or move the existing hook aside"
                  .format(target))
            return 2
        if existing == body:
            # The hook is only ONE of install's two jobs. `.git/hooks` is shared by every
            # worktree of a repository (this command says so when it writes the hook), so in
            # every worktree after the first the hook already exists -- and returning here
            # took the merge-driver declaration with it. The command printed success and
            # never touched .gitattributes, which made it a no-op in exactly the case
            # `pack-doctor`'s WARN sends people to run it. Fall through instead.
            print("pre-commit hook already installed (unchanged)")
            _install_merge_hook(hooks_dir)
            _install_merge_driver(repo, root)
            _print_settings_entry(repo)
            return 0
    target.write_text(body, encoding="utf-8", newline="\n")
    try:
        os.chmod(target, 0o755)
    except OSError:
        pass
    gitignore = Path(root) / ".gitignore"
    try:
        Path(root).mkdir(parents=True, exist_ok=True)
        current = gitignore.read_text(encoding="utf-8") if gitignore.exists() else ""
        if "decisions/" not in current:
            gitignore.write_text(current + "decisions/\n", encoding="utf-8", newline="\n")
    except OSError:
        pass
    print("Wrote {}  (shared by every worktree of this repo)".format(target))
    _install_merge_hook(hooks_dir)
    _install_merge_driver(repo, root)
    _print_settings_entry(repo)
    return 0


def _install_merge_driver(repo, root):
    """Register the driver in .git/config and declare it in .gitattributes.

    .git/config is per-clone and NEVER committed, which is why `coord doctor` exists and
    why the value is READ BACK here rather than assumed -- the recorded CTRL-E instance was
    a `git config` that failed while the script reported success.

    Per-clone is NOT per-worktree. A linked worktree shares this exact file with its parent
    (git writes a per-worktree config only under `extensions.worktreeConfig`, off by
    default), so there is one registration per REPOSITORY and `cmd_install` refuses to
    write it from a tree that will not outlive it.
    """
    try:
        entries = load_registry(root)
    except CoordError as exc:
        print("merge driver not installed: {}".format(exc.code))
        return
    if not entries:
        return
    me = str(Path(__file__).resolve()).replace("\\", "/")
    # Built from MERGE_MECHANISMS, never from a second literal -- a driver table that can
    # drift from CLASSES is how a class comes to exist with no mechanism (CTX-H).
    labels = {
        "derived": "coord: resolve derived artifacts, regenerate after the merge",
        "register": "coord: union append-only registers, conserving every entry",
    }
    drivers = {
        name: (klass, labels[klass],
               '"{}" "{}" merge-{} %A %O %B %P'.format(sys.executable, me, klass))
        for klass, name in MERGE_MECHANISMS.items()
    }
    declared = {}
    for name, (klass, label, command) in drivers.items():
        patterns = [p for p, k, _c in entries if k == klass]
        if not patterns:
            continue
        _git(repo, "config", "merge.{}.name".format(name), label)
        _git(repo, "config", "merge.{}.driver".format(name), command)
        # CTRL-E: read the value back. The recorded instance was a `git config` that failed
        # while the script reported success, leaving the driver unregistered for weeks.
        got, _err = _git(repo, "config", "--get", "merge.{}.driver".format(name))
        if not (got or "").strip():
            print("merge driver {!r} registration FAILED - `git config` reported nothing back"
                  .format(name))
            continue
        declared[name] = patterns

    ga = Path(repo) / ".gitattributes"
    current = ga.read_text(encoding="utf-8") if ga.exists() else ""
    # ATTR-A: install RECONCILES. Adding only left the driver on a file the registry had moved
    # back to `authored` until somebody edited .gitattributes by hand.
    wanted = {name: {p for p, k, _c in entries if k == klass} for klass, name in MERGE_MECHANISMS.items()}
    current, removed = _reconcile_attributes(current, wanted)
    added = 0
    for name, patterns in sorted(declared.items()):
        for pattern in patterns:
            line = "{} merge={}".format(pattern, name)
            if line in current:
                continue
            if current and not current.endswith("\n"):
                current += "\n"                     # LOG-A's sibling seam
            current += line + "\n"
            added += 1
    if added or removed:
        ga.write_text(current, encoding="utf-8", newline="\n")
    if removed:
        print("Removed {} stale merge attribute(s) the registry no longer declares".format(removed))
    if not declared:
        return
    print("Registered {}; .gitattributes declares {} pattern(s)".format(
        ", ".join(repr(n) for n in sorted(declared)),
        sum(len(p) for p in declared.values())))


def _reconcile_attributes(text, wanted):
    """Drop `merge=<coord driver>` from each .gitattributes line whose pattern the registry no
    longer declares for that driver (ATTR-A). The line's other attributes stay, a line left with
    only its pattern goes, and a driver this command does not own is never touched."""
    out, removed = [], 0
    for line in text.splitlines(keepends=True):
        tokens = line.split()
        if not tokens or tokens[0].startswith("#"):
            out.append(line)
            continue
        pattern, attrs = tokens[0], tokens[1:]
        keep = [a for a in attrs if not (a.startswith("merge=") and a[len("merge="):] in wanted
                                         and _norm(pattern) not in wanted[a[len("merge="):]])]
        if len(keep) == len(attrs):
            out.append(line)
            continue
        removed += len(attrs) - len(keep)
        if keep:
            out.append(" ".join([pattern] + keep) + "\n")
    return "".join(out), removed


MERGE_HOOK_BODY = """#!/bin/sh
{marker}
# An automatic merge commit skips pre-commit; pre-merge-commit is its seam (GATE-B / REG-C).
exec "{python}" "{script}" staged-markers
"""


def _install_merge_hook(hooks_dir):
    """Write the pre-merge-commit hook: a merge git concludes on its own runs no pre-commit,
    which is how a driver that exited 0 over conflict markers got them committed (REG-C)."""
    target = hooks_dir / "pre-merge-commit"
    body = MERGE_HOOK_BODY.format(marker=HOOK_MARKER, python=sys.executable.replace("\\", "/"),
                                  script=str(Path(__file__).resolve()).replace("\\", "/"))
    if target.exists():
        existing = target.read_text(encoding="utf-8", errors="replace")
        if HOOK_MARKER not in existing:
            print("COORD-HOOK-EXISTS  {} already exists and is not ours - not overwritten;"
                  " add `coord-core.py staged-markers` to it by hand".format(target))
            return
        if existing == body:
            return
    target.write_text(body, encoding="utf-8", newline="\n")
    try:
        os.chmod(target, 0o755)
    except OSError:
        pass
    print("Wrote {}  (refuses staged conflict markers on a merge commit)".format(target))


# Only the unambiguous markers (verify-no-conflict-markers.py's set): `=======` alone is a
# Markdown setext underline and is never flagged.
CONFLICT_MARKERS = ("<" * 7, ">" * 7, "|" * 7)


def staged_marker_hits(repo):
    """Lines the index ADDS that start with a conflict marker, as `path:line` strings.
    Returns (hits, error). GATE-B, 2026-10-05: a conflict-resolution edit failed while a
    parallel `git add` staged the file, and the commit carried the markers."""
    try:
        proc = subprocess.run(["git", "-C", str(repo), "diff", "--cached", "--no-color", "--no-ext-diff",
                               "-U0", "--diff-filter=ACMR"], capture_output=True, timeout=60, check=False)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return [], exc.__class__.__name__
    if proc.returncode != 0:
        return [], proc.stderr.decode("utf-8", "replace").strip()[:200] or "git diff failed"
    hits, path, lineno = [], None, 0
    for raw in proc.stdout.decode("utf-8", "replace").splitlines():
        if raw.startswith("+++ "):
            path = raw[4:].strip().strip('"')
            path = path[2:] if path.startswith("b/") else path
        elif raw.startswith("@@"):
            match = re.search(r"\+(\d+)", raw)
            lineno = int(match.group(1)) if match else 0
        elif raw.startswith("+"):
            if raw[1:].startswith(CONFLICT_MARKERS):
                hits.append("{}:{}".format(path, lineno))
            lineno += 1
    return hits, None


def cmd_staged_markers(repo):
    hits, err = staged_marker_hits(repo)
    if err is not None:
        print("COORD-NOT-CHECKED-GIT: staged conflict markers not checked ({})".format(_safe(err, 200)))
        return 4
    if hits:
        print("COORD-STAGED-MARKERS  {} staged line(s) start with a conflict marker:".format(len(hits)))
        for hit in hits[:20]:
            print("  " + _safe(hit, 300))
        print("  remedy    resolve the conflict (Read, then Edit), scan for markers, then stage again")
        return 1
    return 0


def _write_conflict(result_path, ours_path, theirs_path, reason):
    """Write conventional conflict markers into the driver's result file.

    S12b, the hazard this exists for: a driver that exits NON-ZERO with no markers leaves the
    file unmerged with OURS content. It looks clean, and `git add .` commits ours and
    silently discards theirs. Writing the markers makes the failure visible in the file --
    where a human, `git diff --check`, and the staged-markers scan all see it. The caller
    decides the exit: merge-register and merge-derived exit 1 after this (REG-C), so git
    stops as well.
    """
    def read(p):
        try:
            return Path(p).read_text(encoding="utf-8", errors="replace")
        except OSError:
            return ""
    body = ("{s} ours\n{ours}{m}\n{theirs}{e} theirs\n"
            .format(s="<" * 7, m="=" * 7, e=">" * 7,
                    ours=read(ours_path), theirs=read(theirs_path)))
    Path(result_path).write_text("# coord: {}\n".format(_safe(reason, 160)) + body,
                                 encoding="utf-8", newline="\n")


def cmd_merge_derived(root, repo, result_path, base_path, theirs_path, real_path):
    """The .gitattributes merge driver for `derived` artifacts.

    Resolves a `derived` artifact to OURS, records that a regeneration is owed, and returns 0.
    Anything it cannot classify as derived gets conventional conflict markers AND returns 1
    (REG-C's sibling, 2026-10-05): with exit 0, `git merge` read the marker file as a clean
    merge and auto-committed it. The markers keep the unmerged path from looking clean (S12b).
    """
    try:
        klass, reason = classify(root, real_path)
        if klass != "derived":
            # H6/B7: the highest-severity path in the phase. If the registry does not say
            # this exact path is derived, the driver must NOT resolve it -- resolving an
            # authored file is how a merge silently overwrites someone's work.
            _write_conflict(result_path, result_path, theirs_path,
                            "{} is classified {}{}; not resolving".format(
                                real_path, klass, " (" + reason + ")" if reason else ""))
            return 1
        # Resolve to ours, byte for byte. `result_path` (%A) already holds ours; touching
        # nothing else is the whole of STRIDE B8's mitigation -- %P is identity, never a
        # write target.
        record_regen_owed(root, real_path)
        return 0
    except Exception as exc:                      # never raise; markers, then stop git
        try:
            _write_conflict(result_path, result_path, theirs_path,
                            "driver error: {}".format(exc.__class__.__name__))
        except Exception:
            pass
        return 1


def cmd_regen(root, repo, timeout=120):
    """Run the regenerations the driver deferred. Returns (exit_code, results).

    A failed regeneration STAYS OWED and reports non-zero: a stale derived artifact looks
    finished, which is worse than a conflict.
    """
    results, done = [], []
    for path in regen_owed(root):
        command = regen_command(root, path)
        if not command:
            results.append({"path": path, "status": "no-command"})
            continue
        try:
            # DEVIATION (Rules of the Road §4, FR-075): shell=True is deliberate. `command` is a
            # regeneration command resolved from the repo-local coordination registry
            # (regen_command -> load_registry) - a trusted config source editable only by someone
            # with repo write access, never untrusted or network input, so this is not an injection
            # surface. It is retained rather than tokenized because registry regen commands may use
            # shell operators (&&, |, >) and must run identically on POSIX and Windows; a shlex
            # arg-list split mishandles Windows path separators and would break the regen path.
            # The interpreter token is resolved to THIS machine's Python first (PLAT-B).
            proc = subprocess.run(resolve_interpreter(command), cwd=str(repo), shell=True,
                                  capture_output=True, text=True, encoding="utf-8",
                                  errors="replace", timeout=timeout)
            ok = proc.returncode == 0
            results.append({"path": path, "status": "ok" if ok else "failed",
                            "detail": _safe((proc.stderr or proc.stdout).strip(), 200)})
            if ok:
                done.append(path)
        except subprocess.TimeoutExpired:
            results.append({"path": path, "status": "failed",
                            "detail": "regeneration exceeded {}s".format(timeout)})
        except OSError as exc:
            results.append({"path": path, "status": "failed",
                            "detail": "{}: {}".format(exc.__class__.__name__, exc)})
    clear_regen_owed(root, done)
    failed = [r for r in results if r["status"] != "ok"]
    return (1 if failed else 0), results


def driver_status(repo):
    """Is the merge driver EFFECTIVE? Requires reading BOTH sources (spike S13).

    `git check-attr` reports the DECLARATION whether or not a driver exists, and
    `git config` reports the registration without knowing what it covers. Only comparing
    the two finds the gap -- and .git/config is per-clone and never committed, so a fresh
    CLONE is exactly where the gap appears. A worktree is not: it shares the parent's
    config and inherits the registration.

    This answers "is a driver registered", which is not the same question as "will its
    path still be there next month" -- see driver_path_status.
    """
    declared, err = _git(repo, "check-attr", "--all", "--", ".")
    names = set()
    out, _ = _git(repo, "config", "--get-regexp", r"^merge\..*\.driver")
    for line in (out or "").splitlines():
        parts = line.split(".", 2)
        if len(parts) >= 3:
            names.add(parts[1])
    attrs, _ = _git(repo, "ls-files")
    declared_names, covered = set(), 0
    for f in (attrs or "").splitlines():
        got, _e = _git(repo, "check-attr", "merge", "--", f)
        if got and ": merge: " in got:
            value = got.rsplit(": merge: ", 1)[1].strip()
            if value not in ("unspecified", "unset", "set"):
                declared_names.add(value)
                covered += 1
    missing = sorted(declared_names - names)
    return {"declared": sorted(declared_names), "registered": sorted(names),
            "missing": missing, "covered_paths": covered,
            # R4: a scan of zero tracked files has not established "none declared".
            "files_scanned": len((attrs or "").splitlines())}


# The driver command is `"<python>" "<script>" merge-<class> %A %O %B %P`. Read the script
# back out of it rather than re-deriving it: what matters is the path git will actually run.
_DRIVER_SCRIPT = re.compile(r'"([^"]*coord-core\.py)"|(\S*coord-core\.py)')


def driver_path_status(repo):
    """Will the registered driver path OUTLIVE the tree that wrote it? (measured defect)

    `driver_status` asks whether a driver is declared and registered. Both were true in the
    consuming repo that found this, throughout -- and the registration pointed inside a
    temporary worktree, because the pack told every agent to run `coord install` in one.
    A worktree SHARES .git/config, so that install did not add a registration, it replaced
    the repository's. WT8 cleanup then deletes the tree, and every declared path merges by
    invoking a script that is not there.

    There is no signature while the tree exists: the path resolves and names a byte-identical
    script. So the question `doctor` has to ask is not "is a driver registered" but "where
    does it point, and does that place outlive this merge". The primary checkout is the
    answer, because it is the only tree the repository cannot lose.

    The hazard is narrow and worth stating precisely: a path inside a LINKED WORKTREE,
    which WT8 cleanup deletes. A path merely outside the repository is a different and
    legitimate shape -- a global or out-of-tree install of the scripts -- and reporting it
    would be a false positive, so this does not.

    Returns one row per registered `merge.coord-*.driver`:
      ok | missing | foreign | unreadable | unchecked
    `unchecked` is a real answer and never collapses to `ok` (R4).
    """
    out, _err = _git(repo, "config", "--get-regexp", r"^merge\.coord-.*\.driver")
    rows = []
    if not (out or "").strip():
        return rows
    records, err = worktree_inventory(repo)
    linked, checked = [], True
    if err or not records:
        checked = False                     # R4: unresolved is not evidence of health
    else:
        for record in records[1:]:          # records[0] is the primary checkout
            try:
                linked.append(Path(record["path"]).resolve())
            except (OSError, KeyError):
                checked = False
    for line in out.splitlines():
        key, _sp, command = line.partition(" ")
        parts = key.split(".", 2)
        name = parts[1] if len(parts) >= 3 else key
        match = _DRIVER_SCRIPT.search(command)
        if not match:
            rows.append({"name": name, "path": command.strip(), "verdict": "unreadable",
                         "detail": "no coord-core.py path in the registered command"})
            continue
        raw = match.group(1) or match.group(2)
        try:
            resolved = Path(raw).resolve()
            exists = resolved.exists()
        except OSError:
            rows.append({"name": name, "path": raw, "verdict": "unchecked",
                         "detail": "the path could not be resolved on this filesystem"})
            continue
        if not exists:
            rows.append({"name": name, "path": raw, "verdict": "missing",
                         "detail": "no file there - every declared path now merges by"
                                   " invoking a script that is not present"})
            continue
        if not checked:
            rows.append({"name": name, "path": raw, "verdict": "unchecked",
                         "detail": "this repository's worktrees could not be enumerated"})
            continue
        doomed = next((tree for tree in linked
                       if resolved == tree or tree in resolved.parents), None)
        rows.append({"name": name, "path": raw,
                     "verdict": "foreign" if doomed else "ok",
                     "detail": "" if not doomed else
                     "inside the linked worktree {} - that tree is temporary, and cleanup"
                     " takes the driver with it".format(doomed)})
    return rows


_DRIVER_PATH_CODES = {"missing": "COORD-DRIVER-PATH-MISSING",
                      "foreign": "COORD-DRIVER-PATH-FOREIGN",
                      "unreadable": "COORD-DRIVER-PATH-UNREADABLE",
                      "unchecked": "COORD-NOT-CHECKED-DRIVER-PATH"}


def cmd_doctor(root, repo):
    problems = 0
    try:
        entries = load_registry(root)
        if entries is None:
            print("registry         NOT PRESENT (advisory)")
            print("  effect      every path is treated as `authored`; nothing is regenerated")
            print("  remedy      create .agents/{} to make classification real".format(
                REGISTRY_NAME))
        else:
            print("registry         ok - {} pattern(s)".format(len(entries)))
            # REG-C: the register driver unions JSONL rows. On any other file it cannot
            # merge, so a markdown "append-only log" classed `register` conflicts on the first
            # concurrent append (and auto-committed markers while the driver exited 0).
            for pattern, klass, _command in entries:
                if klass == "register" and not pattern.endswith(".jsonl"):
                    print("register class   NOT JSONL  [COORD-REGISTER-NOT-JSONL]  {}".format(
                        _safe(pattern, 200)))
                    print("  because     the register driver merges JSONL rows only")
                    print("  remedy      class it `authored` (one file per session if it is a log),"
                          " or make it .jsonl")
                    problems += 1
    except CoordError as exc:
        print("registry         {}".format(exc.code))
        print("  because     {}".format(_safe(str(exc), 200)))
        problems += 1

    status = driver_status(repo)
    if status["missing"]:
        print("merge driver     NOT EFFECTIVE  [COORD-DRIVER-NOT-EFFECTIVE]")
        print("  declared    .gitattributes covers {} path(s) via {}".format(
            status["covered_paths"], ", ".join(status["missing"])))
        print("  registered  no - `git config merge.<name>.driver` is unset in this clone")
        print("  effect      those files will conflict normally instead of regenerating")
        print("  remedy      run `coord install` in the PRIMARY checkout of this clone;"
              " .git/config is per-clone and never committed (a worktree shares it)")
        problems += 1
    elif status["declared"]:
        print("merge driver     effective - {} declared, {} registered".format(
            ", ".join(status["declared"]), ", ".join(status["registered"])))
    elif status["files_scanned"] == 0:
        # R4 again, in code written the same afternoon the rule was cited. A scan of zero
        # tracked files has not established that no driver is declared.
        print("merge driver     NOT CHECKED - 0 tracked files scanned, so nothing was"
              " established")
        problems += 1
    else:
        print("merge driver     none declared ({} tracked file(s) scanned)".format(
            status["files_scanned"]))

    for row in driver_path_status(repo):
        if row["verdict"] == "ok":
            continue
        print("driver path      {}  [{}]".format(row["verdict"].upper(),
                                                 _DRIVER_PATH_CODES[row["verdict"]]))
        print("  driver      merge.{}.driver".format(row["name"]))
        print("  points at   {}".format(_safe(row["path"], 300)))
        print("  because     {}".format(row["detail"]))
        print("  remedy      re-run `coord install` in the PRIMARY checkout, the only"
              " tree this repository cannot lose")
        problems += 1

    owed = regen_owed(root)
    if owed:
        print("regeneration     {} artifact(s) OWED - run `coord regen`".format(len(owed)))
        problems += 1

    line, is_problem = leader_doctor_line(repo, time.time())
    print(line)
    if is_problem:
        problems += 1
    line, is_problem = heartbeat_doctor_line(root, time.time())
    print(line)
    if is_problem:
        problems += 1

    lines, request_problems = request_doctor_lines(root, checkout_top(os.getcwd()), time.time())   # WT-A
    for line in lines:
        print(line)
    problems += request_problems
    for line in lease_overlap_lines(root, time.time())[0]:
        print(line)

    # NFR-S2: state the limit of our own control rather than implying enforcement we have not
    # established. Everything above this point is MEASURED in this repo; everything below it
    # is a spike result about a harness and is the same in every repo (CTX-H / P3). The blank
    # line and the heading are the separation.
    print("")
    for line in render_harness_capability():
        print(line)

    return 1 if problems else 0


PLUGIN_NAME = "coord-agent-coordination"


def cmd_plugin_emit(out_dir, host=None):
    """Write the plugin bundle BOTH harnesses read. It never installs anything.

    S14 established that Copilot CLI consumes the Claude plugin format verbatim --
    `.claude-plugin/plugin.json` plus `hooks/hooks.json` with the same matcher/hooks shape
    and the same ${CLAUDE_PLUGIN_ROOT} placeholder. One bundle therefore serves both, which
    is what made NFR-C1 cheap.

    STRIDE B9: this writes only where it is told and PRINTS what it wrote. It never edits
    ~/.copilot/settings.json or .claude/settings.json, because a layer that grants itself
    tool permissions is the elevation it exists to prevent -- the same rule `install`
    follows by printing the settings entry rather than writing it.
    """
    out = Path(out_dir)
    manifest_path = out / ".claude-plugin" / "plugin.json"
    if manifest_path.exists():
        try:
            existing = json.loads(manifest_path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            existing = {}
        if existing.get("name") != PLUGIN_NAME:
            print("COORD-PLUGIN-FOREIGN  {} already holds a different plugin ({!r}) -"
                  " not overwritten.\n  remedy    emit to another directory"
                  .format(manifest_path, _safe(existing.get("name", "unknown"), 60)))
            return 2

    me = str(Path(__file__).resolve()).replace("\\", "/")
    manifest = {
        "name": PLUGIN_NAME,
        "description": "Refuse an edit to an artifact another session holds a lease on.",
        "version": "0.1.0",
        "author": {"name": "AI-Forward Pack"},
        "license": "MIT",
        "keywords": ["agent-coordination", "leases", "pretooluse"],
    }

    # THE COMMAND SHAPE IS LOAD-BEARING, and it was got wrong first time.
    # A live Copilot run denied EVERY tool call with "(hook errored)" and the hook script
    # never executed at all -- no output, no side effect, nothing. The bundle then emitted
    # `"C:\...\python.exe" "C:/...coord-core.py" hook`: a QUOTED EXECUTABLE.
    # The one plugin known to work on this machine (wt-agent-hooks, 55,541 invocations)
    # quotes its SCRIPT but never its interpreter:
    #     powershell -NoProfile ... -File "${CLAUDE_PLUGIN_ROOT}/hooks/send-event.ps1" ...
    # So: a bare interpreter resolved from PATH, a quoted script path, and the script
    # shipped INSIDE the bundle and addressed via ${CLAUDE_PLUGIN_ROOT} -- which also
    # means the bundle is relocatable rather than pinned to an absolute path.
    launcher = ("#!/usr/bin/env python3\n"
                '"""Launcher for the coord PreToolUse hook. Emitted by `coord plugin`.\n\n'
                "Kept minimal on purpose: the harness runs THIS, and it delegates. It exists\n"
                "because a hook command must name a bare interpreter and a quoted script\n"
                "inside the bundle -- a quoted absolute interpreter path does not execute.\n"
                '"""\n'
                "import runpy, sys\n"
                'COORD = r"{}"\n'
                'sys.argv = [COORD, "hook"]\n'
                'runpy.run_path(COORD, run_name="__main__")\n').format(me)

    hooks = {"hooks": {"PreToolUse": [{
        "matcher": ".*",
        "hooks": [{"type": "command",
                   # The bundle is emitted ON the machine that loads it, so the bare
                   # interpreter is chosen here: `python` is the python.org Windows name
                   # (the live Copilot run that proved this shape ran there), `python3`
                   # is the name macOS and Linux actually have (XP-04).
                   "command": '{0} "${{CLAUDE_PLUGIN_ROOT}}/hooks/hook.py"'.format(
                       "python" if os.name == "nt" else "python3"),
                   "timeout": 10}]}]}}

    lifecycle = None
    if host == "copilot":
        launcher = launcher.replace('[COORD, "hook"]', '[COORD, "hook", "--host", "copilot"]')
        here = Path(__file__).resolve().parent
        hook_dir = next((p for p in (here.parent / "adapters" / "hooks", here.parent / "hooks")
                         if (p / "copilot.ai-forward-hooks.json").is_file()), None)
        if hook_dir is None:
            print("COORD-PLUGIN-HOOKS  install the native Copilot hook bundle before emitting this profile")
            return 2
        source = json.loads((hook_dir / "copilot.ai-forward-hooks.json").read_text(encoding="utf-8"))
        # Reuse the applier's exact managed startup syntax in source and installed layouts.
        import importlib.util
        spec = importlib.util.spec_from_file_location("coord_plugin_pack_apply", here / "pack-apply.py")
        if spec is None or spec.loader is None:
            raise ValueError("Unsupported source-managed Copilot hook bundle")
        applier = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(applier)
        events = {"preToolUse": "PreToolUse", "postToolUse": "PostToolUse",
                  "sessionStart": "SessionStart", "subagentStart": "SubagentStart",
                  "agentStop": "Stop", "subagentStop": "SubagentStop",
                  "userPromptSubmitted": "UserPromptSubmit"}
        scripts = {}
        for event, entries in source["hooks"].items():
            for entry in entries:
                # Only the complete canonical startup command; its shell body is not argv.
                match = re.fullmatch(re.escape(applier.STARTUP_LAUNCHER) +
                                     r"(session-start\.py|dispatch\.py)( --host copilot(?: --event [A-Za-z]+)?)", entry["bash"])
                if not match:
                    # Existing PLAT-A commands retain their script, host and event argv.
                    match = re.fullmatch(re.escape(
                        "git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh ") +
                        r"([A-Za-z_-]+\.py)( --host copilot(?: --event [A-Za-z]+)?)", entry["bash"])
                if not match or not (hook_dir / match.group(1)).is_file():
                    raise ValueError("Unsupported source-managed Copilot hook command")
                name, arguments = match.groups()
                scripts[name] = str(hook_dir / name)
                command = '{} "${{CLAUDE_PLUGIN_ROOT}}/hooks/lifecycle.py" {}{}'.format(
                    "python" if os.name == "nt" else "python3", name, arguments)
                native = {"hooks": [{"type": "command", "command": command,
                                      "timeout": entry.get("timeoutSec", 10)}]}
                if event == "preToolUse" and "matcher" in entry:
                    # Copilot's native view tool is named Read in the PascalCase envelope.
                    native["matcher"] = "Read" if entry["matcher"] == "^view$" else entry["matcher"]
                hooks["hooks"].setdefault(events[event], []).append(native)
        lifecycle = ("import runpy, sys\n"
                     "SCRIPTS = " + repr(scripts) + "\n"
                     "script = SCRIPTS[sys.argv.pop(1)]\n"
                     "sys.argv[0] = script\n"
                     "runpy.run_path(script, run_name='__main__')\n")

    (out / ".claude-plugin").mkdir(parents=True, exist_ok=True)
    (out / "hooks").mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n",
                             encoding="utf-8", newline="\n")
    (out / "hooks" / "hook.py").write_text(launcher, encoding="utf-8", newline="\n")
    if lifecycle is not None:
        (out / "hooks" / "lifecycle.py").write_text(lifecycle, encoding="utf-8", newline="\n")
    (out / "hooks" / "hooks.json").write_text(json.dumps(hooks, indent=2) + "\n",
                                              encoding="utf-8", newline="\n")

    print("Wrote the plugin bundle to {}".format(out))
    print("  {}".format(manifest_path))
    print("  {}".format(out / "hooks" / "hooks.json"))
    print("")
    print("This tool does NOT install it. Load it yourself, per harness:")
    print("  Copilot CLI   copilot --plugin-dir \"{}\"".format(out))
    print("  Claude Code   add the entry `coord install` prints, or install as a plugin")
    print("")
    for line in render_harness_capability():
        print(line)
    return 0


def _print_settings_entry(repo):
    """Print the hook entry for a human to paste.

    Built with json.dumps, NOT by hand. The hand-formatted version shipped literal `{{`
    braces (from .format escaping on lines that never called .format) and an unescaped
    Windows path - output that reads correctly and is invalid the moment it is pasted.
    A serializer cannot make either mistake.
    """
    entry = native_hook_config("claude")
    print("")
    print("Add this to .claude/settings.json yourself - this tool does not edit it:")
    for line in json.dumps(entry, indent=2).splitlines():
        print("  " + line)
    print("")
    print("Enforcement can be switched off by disableAllHooks, allowManagedHooksOnly, or")
    print("strictPluginOnlyCustomization. The pre-commit floor cannot.")


def native_hook_config(host):
    """A reviewable project-local entry; no settings, trust, or permission mutation.

    Native hooks use a shell command string. Only fixed syntax and the selected enum enter
    it; runtime paths stay in quoted expansions, never eval or interpolated source code.
    """
    if host == "copilot":
        # PLAT-A: Copilot on Windows runs hook commands through PowerShell (measured 2026-09-24, Copilot CLI
        # 1.0.89-1). Both arms are the quote-free launcher, which pwsh, cmd.exe and sh parse alike and which
        # resolves the interpreter; --caller-cwd keeps the caller's directory, as the old relative form needed.
        command = ("git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh "
                   "--caller-cwd ../scripts/coord-core.py hook --host copilot")
        return {"version": 1, "hooks": {"preToolUse": [{
            "type": "command",
            "bash": command,
            "powershell": command,
            "timeoutSec": 10,
            "matcher": "^(edit|create|write|apply_patch|str_replace|search_replace|multiedit|notebookedit|edit_file|write_file|write_to_file|replace_file_content|multi_replace_file_content)$",
        }]}}
    if host == "claude":
        # PLAT-A: this entry is merged into .claude/settings.json, which Copilot CLI 1.0.89-1 also reads and runs
        # through PowerShell on Windows (Inferred from its runtime.node). --caller-cwd keeps the caller's directory.
        command = ("git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh "
                   "--caller-cwd ../scripts/coord-core.py hook --host claude")
    elif host == "codex":
        # PLAT-C: Codex on Windows runs hook commands through `pwsh -Command` (measured 2026-09-24,
        # codex 0.156), which cannot parse the sh form below. One quote-free git invocation parses
        # alike under pwsh, cmd.exe and sh; run-hook.sh resolves the interpreter and, with
        # --caller-cwd, keeps the caller's directory, against which Codex patch paths resolve.
        command = ("git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh "
                   "--caller-cwd ../scripts/coord-core.py hook --host codex")
    elif host == "agy":
        # PLAT-A: Agy on Windows runs hook commands through cmd.exe from <repo>/.agents (measured 2026-09-24,
        # agy 1.2.10), as its six bundle commands already do. Its TargetFile is absolute, so the launcher
        # runs the hook from the top of the tree with no --caller-cwd.
        command = "git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh ../scripts/coord-core.py hook --host agy"
    else:
        command = ("git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh "
                   "--caller-cwd ../scripts/coord-core.py hook --host " + host)
    matcher = {"codex": "apply_patch", "claude": "Write|Edit|MultiEdit|NotebookEdit",
               "grok": "Write|Edit|MultiEdit|NotebookEdit|write_file|edit_file|search_replace",
               "agy": "write_to_file|replace_file_content|multi_replace_file_content"}[host]
    entries = [{"matcher": matcher, "hooks": [{"type": "command", "command": command, "timeout": 5}]}]
    if host == "agy":
        return {"ownership-guard": {"enabled": True, "PreToolUse": entries}}
    return {"hooks": {"PreToolUse": entries}}


def main(argv=None):
    args = _build_parser().parse_args(argv)

    if args.cmd == "hook" and args.config:
        if not args.host:
            print("hook --config requires --host claude|codex|copilot|grok|agy", file=sys.stderr)
            return 2
        print(json.dumps(native_hook_config(args.host), indent=2))
        return 0

    try:
        root, err = resolve_root(os.getcwd(), os.environ.get("COORD_ROOT"))
    except Exception as exc:
        if args.cmd != "hook":
            raise
        _emit(_not_checked("hook root unavailable ({})".format(type(exc).__name__), args.host))
        return 0
    if err:
        payload = {"decision": "not_checked", "path": "-"}
        payload.update(err)
        if args.cmd == "hook":
            _emit(_not_checked(render(payload), args.host))
            return 0
        print(render(payload), file=sys.stderr)
        return 4

    session, agent = _identity()
    # BEFORE any command, including `hook`: an id that cannot be a file name never reaches a
    # writer. An UNSET (or empty) id is not this refusal - the identity gate below renders that.
    if session:
        bad = session_id_error(session)
        if bad:
            if args.cmd == "hook":
                _emit(_not_checked(bad, args.host))
                return 0
            print(bad, file=sys.stderr)
            return 2
    now = time.time()

    repo = repo_root(os.getcwd())
    tree = checkout_top(os.getcwd())   # WT-A: paths, index and blobs are per checkout

    if args.cmd == "hook":
        # ALWAYS exit 0: the harness reads the decision in the JSON, not the exit code.
        try:
            output = cmd_hook(root, session, agent, now, sys.stdin.read(), repo=tree,
                              host=args.host, cwd=os.getcwd())
        except Exception as exc:
            output = _not_checked("hook state unavailable ({})".format(type(exc).__name__), args.host)
        if output:
            print(output)
        return 0

    if args.cmd == "guard":
        return cmd_guard(repo, args.fix)

    if args.cmd == "install":
        return cmd_install(repo, root, force=args.force)

    if args.cmd == "classify":
        return cmd_classify_init(root, repo, force=args.force, timeout=args.timeout)

    if args.cmd == "class":
        klass, reason = classify(root, args.path)
        payload = {"path": args.path, "class": klass, "code": reason,
                   "reason": ("no registry at .agents/{} - advisory, everything is"
                              " `authored`".format(REGISTRY_NAME)
                              if reason == "COORD-CLASS-UNREGISTERED"
                              else ("the registry could not be read"
                                    if reason else ""))}
        print(json.dumps(payload) if args.json else
              "{}  {}{}".format(klass, args.path,
                                "  [" + reason + "]" if reason else ""))
        return 0

    if args.cmd == "merge-derived":
        return cmd_merge_derived(root, repo, args.result, args.base,
                                 args.theirs, args.realpath)

    if args.cmd == "regen":
        code, results = cmd_regen(root, repo, args.timeout)
        for r in results:
            print("{:<10} {}  {}".format(r["status"], r["path"], r.get("detail", "")))
        if not results:
            print("nothing owed")
        return code

    if args.cmd == "doctor":
        return cmd_doctor(root, repo)

    if args.cmd == "allocate":
        print(new_id(args.scheme))
        return 0

    if args.cmd == "plugin":
        return cmd_plugin_emit(args.emit, args.host)

    if args.cmd == "merge-register":
        return cmd_merge_register(args.result, args.base, args.theirs, args.realpath)

    if args.cmd == "resolve":
        try:
            rows = _read_jsonl(args.register)
        except (OSError, json.JSONDecodeError) as exc:
            print("COORD-NOT-CHECKED-RECORD  {} is unreadable: {}".format(
                _safe(args.register, 200), exc.__class__.__name__))
            return 4
        status, result, corpus = resolve_prefix(rows, args.prefix)
        if status == "unique":
            print(result["id"])
            return 0
        if status == "ambiguous":
            print("COORD-PREFIX-AMBIGUOUS  {!r} matches {} entries in a corpus of {}:"
                  .format(args.prefix, len(result), corpus))
            for row in result:
                print("  {}  {}".format(row.get("id"), _safe(row.get("shortname", ""), 80)))
            print("  remedy    lengthen the prefix; this never picks a first match")
            return 3
        # R4: "not found" carries the corpus size, so an empty register cannot render as a
        # searched one.
        print("COORD-PREFIX-NOMATCH  {!r} matches nothing in a corpus of {} entries"
              .format(args.prefix, corpus))
        return 4

    if args.cmd == "metrics":
        return cmd_metrics(root, repo, args.json)

    # BEFORE the identity gate: `who` is a read (the join script and a human both ask it with
    # no AGENT_SESSION), and `pin`/`reclaim` name their target; the holder check for
    # `renew`/`release` is inside cmd_leader.
    if args.cmd == "leader":
        return cmd_leader(root, repo, args.leader_action, args, session, agent, os.getcwd(), now)

    # BEFORE the identity gate for the same reason: the delegate scripts own their identity
    # rules (read/board are reads; send/post read AGENT_SESSION themselves). The exit code is
    # the child's, never folded (an exit code is a result only when it is read).
    if args.cmd in ("mail", "board", "decide"):
        script = {"mail": "coord-mail.py", "board": "coord-board.py", "decide": "coord-decide.py"}[args.cmd]
        target = os.path.join(_HERE, script)
        passthrough = getattr(args, args.cmd + "_args")
        if not os.path.isfile(target):
            print("COORD-NOT-CHECKED  {} is not beside coord-core.py; the message layer is not installed here".format(os.path.basename(target)))
            return 4
        completed = subprocess.run([sys.executable, target, *passthrough], encoding="utf-8", errors="replace")
        return completed.returncode

    # Dispatched BEFORE the identity gate: `worktree list` and `cleanup` are read/maintenance
    # commands, and refusing to tell someone what trees exist because AGENT_SESSION is unset
    # would make the orphan check unreachable exactly when it is most needed (WT10).
    if args.cmd == "worktree":
        chosen = getattr(args, "wt_session", None) or session
        # `--session` bypasses the env var, so it meets the same file-name rule here.
        bad = session_id_error(chosen) if chosen else None
        if bad:
            print(bad, file=sys.stderr)
            return 2
        return cmd_worktree(root, repo, args.action, os.getcwd(), now,
                            session=chosen, agent=agent or chosen, branch=args.branch,
                            base=args.base, remove=args.remove,
                            only=getattr(args, 'wt_path', None),
                            include_unmerged=getattr(args, 'include_unmerged', False))

    if args.cmd == "session" and args.action == "list":
        return cmd_session_list(root, now, args.json)

    # P3 reads and maintenance, before the identity gate: `track` is the coordinator's and a
    # human's view; `log portable` runs at a landing with no session of its own.
    if args.cmd == "track":
        return cmd_track(root, repo, now, args.json)

    if args.cmd == "log":
        return cmd_log_portable(args.files)

    if args.cmd == "collaborate":
        return cmd_collaborate(root, repo, args.action, now, args.json)

    if args.cmd == "request":
        return cmd_request(root, args.request_action, now, session, agent, args, repo=tree)

    if args.cmd == "check":
        decision = check(root, args.path, session, now)
        append_decision(root, session, agent, args.path, decision)
        print(json.dumps(decision) if args.json else render(decision))
        return EXIT[decision["decision"]]

    if args.cmd in ("precommit", "staged-markers"):
        # GATE-B: staged conflict markers are refused before identity or the record matter,
        # so the floor holds in advisory mode and for a commit made by hand.
        rc = cmd_staged_markers(tree)
        if rc or args.cmd == "staged-markers":
            return rc

    if args.cmd == "precommit":
        if not session:
            # ADVISORY, not blocking. `check` returns 4 here and should -- you asked a
            # question and there is no identity to answer it with. But this hook runs on
            # EVERY commit in the repository, including a human's by hand and any tool's,
            # and a floor that refuses all of them is a floor that gets deleted rather
            # than adopted. Same trade as the missing-registry case (US-8), and NFR-S2
            # already concedes this is an integrity control, not a security one.
            print("advisory: AGENT_SESSION is unset, so nothing was checked."
                  "\n  set AGENT_SESSION to make this commit boundary enforcing.")
            return 0
        return cmd_precommit(root, tree, session, agent, now)

    if not session:
        print(render({"decision": "not_checked", "path": "-",
                      "code": "COORD-NOT-CHECKED-IDENTITY",
                      "reason": "AGENT_SESSION is unset"}), file=sys.stderr)
        return 4

    if args.cmd == "claim":
        # DC-163, two refusals BEFORE the check: neither is a contention verdict.
        # (1) A `register`-class artifact merges by UNION (its driver is the mechanism), so
        # a lease on it protects nothing and blocks every join that must append to it.
        try:
            klass, _why = classify(root, args.path)
        except CoordError as exc:
            print("{}: {}".format(exc.code, exc), file=sys.stderr)
            return 2
        if klass == "register":
            print("COORD-CLAIM-REGISTER-CLASS  {}\n"
                  "  a register-class artifact merges by union; no lease is needed and one\n"
                  "  only queues the joins behind it - append (a placeholder id if the\n"
                  "  allocator is the join's) and commit".format(_safe(args.path, 200)),
                  file=sys.stderr)
            return 3
        # (2) A TTL past the cap needs a recorded reason: the lease is for the minutes of
        # the edit, and a queued peer reads why it waits from the claim event itself.
        if args.ttl > TTL_CAP and not args.long_edit:
            print("COORD-CLAIM-TTL-CAP  --ttl {:g} exceeds the cap of {} s\n"
                  "  claim for the minutes of the edit (default {} s) and release at once;\n"
                  "  a genuinely long edit passes --long-edit <reason>".format(
                      args.ttl, TTL_CAP, TTL_DEFAULT), file=sys.stderr)
            return 3
        decision = check(root, args.path, session, now)
        if decision["decision"] == "deny":
            append_decision(root, session, agent, args.path, decision)
            print(render(decision))
            return 3
        try:
            event = make_event("claim", session, agent, args.wi, args.path, now, args.ttl,
                               excepts=args.excepts)
            if args.long_edit:
                event["long_edit"] = args.long_edit
            append_event(root, event)
        except CoordError as exc:
            print("{}: {}".format(exc.code, exc), file=sys.stderr)
            return 2
        print("granted  {}  {}".format(args.path, args.wi))
        return 0

    if args.cmd == "release":
        try:
            append_event(root, make_event("release", session, agent, args.wi,
                                          args.path, now))
        except CoordError as exc:
            print("{}: {}".format(exc.code, exc), file=sys.stderr)
            return 2
        print("released {}".format(args.path))
        return 0

    if args.cmd == "session" and args.action == "heartbeat":
        row = heartbeat_tick(root, repo, session, agent, now, files=args.files, calls=args.calls,
                             tokens=args.tokens, host=args.host, event=args.event, wi=args.wi,
                             cwd=os.getcwd(), flush=args.flush)
        print("heartbeat {}  calls {} files {} tokens {}  leader_renewed {}".format(
            "written", row["calls"], row["files"], row["tokens"], row["leader_renewed"])
              if row else "heartbeat accumulated (sample window {} s open)".format(HEARTBEAT_SAMPLE))
        return 0

    if args.cmd == "kick":
        return cmd_kick(root, repo, args.kick_target, args, session, agent, now)

    if args.cmd == "session":
        return cmd_session(root, args.action, session, agent, os.getcwd(), now, repo=repo)

    if args.cmd == "tail":
        # tail is the HUMAN stream, so it reads BOTH stores. `check` is the machine
        # verdict, so it reads only log/. Two grains, two stores, one reader each.
        events, errors, files = read_events(root)
        events = sorted(events + read_decisions(root), key=lambda e: e.get("at", 0.0))
        if files == 0 and not events:
            print("0 files scanned - there is no record here", file=sys.stderr)
            return 4
        for event in events[-args.n:]:
            stamp = time.strftime("%H:%M:%S", time.localtime(event.get("at", 0)))
            print("{}  {:<10} {:<9} {}  {}".format(
                stamp, event.get("agent", "?"), event.get("kind", "?"),
                event.get("path", ""), event.get("wi", "")))
        for line in errors:
            print("  ! unreadable: {}".format(line), file=sys.stderr)
        return 0

    return 2


if __name__ == "__main__":
    sys.exit(main())
