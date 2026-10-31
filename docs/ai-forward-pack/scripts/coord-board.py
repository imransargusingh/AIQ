#!/usr/bin/env python3
"""coord-board.py — the board: human transparency over agent messages (spec-board, D12).

A READ MODEL over the message store, never a store. It folds every session inbox
(`.agents/mail/<session>.jsonl`) and every ledger twin (`type: mail` records in
`.agents/log/<session>.jsonl`) into one timeline, one row per mail id:

    ts · from → to · kind · ref · age · acked?

Reading writes nothing (the read-rate is measured from the shell history / the session
profiler, not by the board). An empty corpus prints NOT CHECKED, never "all quiet" (R4).
A human speaks into the same inboxes with `board post`, which goes ONLY through the message
layer's single writer — P4's `append_mail(root, session, entry)` in coord-mail.py, imported by
path — so a post that bypasses the inbox writer is impossible by construction. Until that file
lands, `--writer <path>` names a module with the same signature (the tests ship a fixture).

Subcommands
  board            print the rows (or NOT CHECKED); --json for the same rows as data
    --follow       poll every --interval seconds (5.0) until --max-polls reads (720 = 1 h at
                   5 s) have happened, printing only rows not yet printed; says why it stopped
    --since <id>   rows ordered after that mail id (by ts, then id)
    --session <s>  rows where from == s, to == s, or to == "*"
  board post --to <session|*> "<text>" [--kind note|ruling] [--ref <ref>] [--from <session>]
             [--writer <path>]

Conventions
  --root is the `.agents` directory (default: <repo root>/.agents, the repo root found by
  walking up from cwd to a .git file or directory — the layout coord-core.repo_root reads).
  A root outside the repository is refused (COORD-NOT-CHECKED-ROOT, as coord-core does).
  Python 3.8+, stdlib only. Exit 0 for every read, 2 for a refused argument or post.
"""
import argparse
import datetime
import importlib.util
import json
import os
import secrets
import sys
import time
from pathlib import Path

# Windows consoles default to cp1252; the arrow and the tick below would crash a bare print
# (same seam audit-log.py owns for itself — the script that prints guards its own console).
for _stream in (sys.stdin, sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

COORD_DIRNAME = ".agents"
ISO = "%Y-%m-%dT%H:%M:%SZ"
POST_KINDS = ("note", "ruling")
BODY_LIMIT = 4096          # bytes, utf-8 — the store contract's "text ≤ 4 KiB"
NOT_ON_THIS_MACHINE = "(not on this machine)"
DEFAULT_INTERVAL = 5.0
DEFAULT_MAX_POLLS = 720    # 1 h at 5 s — the loop's termination variant (GO: every loop has one)
_ULID_ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"


# --- root ------------------------------------------------------------------------------

def repo_root(cwd):
    """The PRIMARY checkout, from any worktree - the `.agents` stores are per repository.

    Walk up from cwd to the first `.git` entry. A directory is the primary itself. A FILE is a
    linked worktree's pointer (`gitdir: <primary>/.git/worktrees/<name>`), so the primary is
    that path's third parent. Filesystem only, no subprocess (coord-core's reasoning). Before
    this the board resolved the NEAREST checkout and, from a worktree, read the committed ledger
    copies and no inbox at all (class WT-A)."""
    here = Path(cwd).resolve()
    for candidate in (here, *here.parents):
        marker = candidate / ".git"
        if marker.is_dir():
            return candidate
        if marker.is_file():
            try:
                first = marker.read_text(encoding="utf-8").splitlines()[0]
            except (OSError, IndexError):
                return candidate
            if first.startswith("gitdir:"):
                gitdir = Path(first.split(":", 1)[1].strip())
                if not gitdir.is_absolute():
                    gitdir = (candidate / gitdir).resolve()
                if gitdir.parent.name == "worktrees" and gitdir.parent.parent.name == ".git":
                    return gitdir.parent.parent.parent
            return candidate
    return here


def resolve_root(cwd, raw):
    """The .agents root, refused when it resolves outside the repository (coord-core's rule)."""
    base = repo_root(cwd)
    root = Path(raw).resolve() if raw else (base / COORD_DIRNAME).resolve()
    try:
        root.relative_to(base)
    except ValueError:
        return None, "COORD-NOT-CHECKED-ROOT  --root resolves outside the repository: {}".format(root)
    return root, None


# --- the fold: files -> rows ----------------------------------------------------------------

def _iter_jsonl(path, errors):
    """Yield (lineno, obj) for every readable line; an unreadable one is COUNTED, never hidden."""
    with open(path, "r", encoding="utf-8") as fh:
        for lineno, line in enumerate(fh, start=1):
            if not line.strip():
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError as exc:
                errors.append("{}:{} unreadable ({})".format(path.name, lineno, exc.msg))
                continue
            if isinstance(obj, dict):
                yield lineno, obj


def _epoch_to_iso(value):
    try:
        return datetime.datetime.fromtimestamp(float(value), datetime.timezone.utc).strftime(ISO)
    except (TypeError, ValueError, OverflowError, OSError):
        return None


def _twin_ts(twin):
    return twin.get("ts") or _epoch_to_iso(twin.get("at"))


def read_sources(root):
    """Return (inbox_lines, twins, errors, files_scanned) — every line of every source, once."""
    inbox, twins, errors, files = [], [], [], 0
    mail_dir, log_dir = Path(root) / "mail", Path(root) / "log"
    if mail_dir.is_dir():
        for path in sorted(mail_dir.glob("*.jsonl")):
            files += 1
            for _, obj in _iter_jsonl(path, errors):
                if obj.get("id"):
                    obj.setdefault("_inbox", path.stem)
                    inbox.append(obj)
    if log_dir.is_dir():
        for path in sorted(log_dir.glob("*.jsonl")):
            files += 1
            for _, obj in _iter_jsonl(path, errors):
                if obj.get("type") == "mail" and obj.get("mail_id"):
                    twins.append(obj)
    return inbox, twins, errors, files


def fold(inbox, twins, now=None):
    """Pure fold: store lines -> rows keyed by mail id. Rendering twice yields equal rows."""
    now = now if now is not None else time.time()
    rows = {}
    for line in inbox:
        mid = line["id"]
        row = rows.get(mid)
        if row is None:
            row = {"id": mid, "ts": line.get("ts") or "", "from": line.get("from") or "",
                   "to": line.get("to") or "", "kind": line.get("kind") or "",
                   "ref": line.get("ref"), "body": line.get("body") if line.get("body") is not None else "",
                   "acked": False, "source": "inbox", "sessions": []}
            rows[mid] = row
        if line.get("_inbox") and line["_inbox"] not in row["sessions"]:
            row["sessions"].append(line["_inbox"])
    for twin in twins:
        mid = twin["mail_id"]
        row = rows.get(mid)
        if row is None:
            rows[mid] = {"id": mid, "ts": _twin_ts(twin) or "", "from": twin.get("from") or "",
                         "to": twin.get("to") or "", "kind": twin.get("kind") or "",
                         "ref": twin.get("ref"), "body": NOT_ON_THIS_MACHINE, "acked": False,
                         "source": "ledger", "sessions": [twin["session"]] if twin.get("session") else []}
        else:
            row["source"] = "both"
            if not row["ts"]:
                row["ts"] = _twin_ts(twin) or ""
    # acked is DERIVED: an inbox line of kind ack whose ref names the row (ack is a new line).
    for line in inbox:
        if line.get("kind") == "ack" and line.get("ref") in rows:
            rows[line["ref"]]["acked"] = True
    ordered = sorted(rows.values(), key=lambda r: (r["ts"], r["id"]))
    for row in ordered:
        row["age_seconds"] = _age(row["ts"], now)
    return ordered


def _age(ts, now):
    try:
        then = datetime.datetime.strptime(ts, ISO).replace(tzinfo=datetime.timezone.utc).timestamp()
    except (TypeError, ValueError):
        return None
    return max(0, int(now - then))


def apply_filters(rows, session=None, since=None, errors=None):
    if session:
        rows = [r for r in rows if r["from"] == session or r["to"] in (session, "*")]
    if since:
        index = next((i for i, r in enumerate(rows) if r["id"] == since), None)
        if index is None:
            if errors is not None:
                errors.append("--since {}: no such mail id in the corpus".format(since))
            return []
        rows = rows[index + 1:]
    return rows


# --- rendering ------------------------------------------------------------------------------

def _age_text(seconds):
    if seconds is None:
        return "age not recorded"
    if seconds < 60:
        return "{}s".format(seconds)
    if seconds < 3600:
        return "{}m".format(seconds // 60)
    if seconds < 86400:
        return "{}h{:02d}m".format(seconds // 3600, (seconds % 3600) // 60)
    return "{}d".format(seconds // 86400)


def format_row(row):
    """One row, plain text, no colour-only meaning: the ack state is a WORD beside its glyph."""
    acked = "✓ acked" if row["acked"] else "- unacked"
    head = "{ts}  {frm} → {to}  {kind}  ref={ref}  {age}  {acked}  {mid}".format(
        ts=row["ts"] or "ts not recorded", frm=row["from"] or "?", to=row["to"] or "?",
        kind=row["kind"] or "?", ref=row["ref"] if row["ref"] is not None else "-",
        age=_age_text(row["age_seconds"]), acked=acked, mid=row["id"])
    body = row.get("body") or ""
    return head if not body else head + "\n    " + body.replace("\n", "\n    ")


def not_checked_line(root):
    return "NOT CHECKED — no inbox or ledger mail found under {}".format(root)


def read_board(root, session=None, since=None):
    inbox, twins, errors, files = read_sources(root)
    rows = apply_filters(fold(inbox, twins), session=session, since=since, errors=errors)
    status = "ok" if (inbox or twins) else "not-checked"
    return {"status": status, "root": str(root), "files_scanned": files, "rows": rows,
            "errors": errors}


def _report_errors(errors):
    for err in errors:
        print("NOT CHECKED — {}".format(err), file=sys.stderr)


def cmd_board(args, root):
    if not args.follow:
        payload = read_board(root, session=args.session, since=args.since)
        if args.json:
            print(json.dumps(payload, ensure_ascii=False))
            return 0
        _report_errors(payload["errors"])
        if payload["status"] == "not-checked":
            print(not_checked_line(root))
            return 0
        for row in payload["rows"]:
            print(format_row(row))
        return 0
    # --follow: a bounded loop. The cap is the termination variant and its firing is said aloud.
    seen, polls = set(), 0
    try:
        while True:
            payload = read_board(root, session=args.session, since=args.since)
            polls += 1
            _report_errors(payload["errors"])
            fresh = [r for r in payload["rows"] if r["id"] not in seen]
            if polls == 1 and payload["status"] == "not-checked":
                print(not_checked_line(root))
            for row in fresh:
                seen.add(row["id"])
                print(json.dumps(row, ensure_ascii=False) if args.json else format_row(row))
            sys.stdout.flush()
            if polls >= args.max_polls:
                print("stopped: --max-polls {} reached".format(args.max_polls))
                return 0
            time.sleep(args.interval)
    except KeyboardInterrupt:
        print("stopped: interrupted")
        return 0


# --- post: through the single writer, never around it -------------------------------------

def new_ulid(now=None):
    """A 26-char Crockford-base32 ULID: 48-bit ms timestamp + 80 random bits (stdlib only)."""
    ms = int((now if now is not None else time.time()) * 1000)
    value = (ms << 80) | int.from_bytes(secrets.token_bytes(10), "big")
    out = []
    for _ in range(26):
        out.append(_ULID_ALPHABET[value & 31])
        value >>= 5
    return "".join(reversed(out))


def load_writer(path):
    """Import the message layer's writer module BY PATH (the file lands with P4 at the join)."""
    spec = importlib.util.spec_from_file_location("coord_mail_writer", str(path))
    if spec is None or spec.loader is None:
        return None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build_entry(to, body, kind, ref, sender, now=None):
    return {"id": new_ulid(now), "ts": time.strftime(ISO, time.gmtime(now)), "from": sender,
            "to": to, "kind": kind, "body": body, "ref": ref, "ack": None}


def cmd_post(args, root):
    def refuse(reason):
        print("board post refused: {}".format(reason), file=sys.stderr)
        return 2

    if not args.to:
        return refuse("--to must name a session or '*'")
    if args.kind not in POST_KINDS:
        return refuse("--kind must be one of {}".format("|".join(POST_KINDS)))
    if args.kind == "ruling" and not args.ref:
        return refuse("a ruling needs --ref DR-n (the decision request it answers)")
    body = args.text
    if not body.strip():
        return refuse("the text is empty")
    if len(body.encode("utf-8")) > BODY_LIMIT:
        return refuse("the text is {} bytes; the store allows at most {}".format(
            len(body.encode("utf-8")), BODY_LIMIT))
    writer_path = Path(args.writer) if args.writer else Path(__file__).resolve().parent / "coord-mail.py"
    if not writer_path.is_file():
        return refuse("no writer at {} — the message layer (P4, coord-mail.py) is not installed"
                      " here; pass --writer <path> to a module exposing"
                      " append_mail(root, session, entry)".format(writer_path))
    try:
        writer = load_writer(writer_path)
    except Exception as exc:  # a foreign module: any failure is reported, never masked
        return refuse("writer {} could not be imported: {}".format(writer_path, exc))
    if writer is None or not callable(getattr(writer, "append_mail", None)):
        return refuse("writer {} exposes no append_mail(root, session, entry)".format(writer_path))
    sender = args.sender or os.environ.get("AGENT_SESSION") or "human"
    entry = build_entry(args.to, body, args.kind, args.ref, sender, now=time.time())
    try:
        # The writer's second argument is the SENDER (P4: append_mail(root, session, entry)); the
        # recipient travels inside the entry. Passing the recipient here wrote a file named after
        # `*` on POSIX and raised EINVAL on Windows (CI, 2026-09-19).
        written = writer.append_mail(str(root), sender, entry)
    except Exception as exc:
        return refuse("writer raised {}: {}".format(type(exc).__name__, exc))
    print("posted {} → {}  {}".format(written or entry["id"], args.to, args.kind))
    return 0


# --- CLI -------------------------------------------------------------------------------------

def build_parser():
    ap = argparse.ArgumentParser(prog="coord-board.py", description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--root", help="the .agents directory (default: <repo root>/.agents)")
    sub = ap.add_subparsers(dest="cmd")
    board = sub.add_parser("board", help="print the board (a read model; reading writes nothing)")
    board.add_argument("--follow", action="store_true", help="poll until --max-polls reads")
    board.add_argument("--interval", type=float, default=DEFAULT_INTERVAL, help="seconds between polls")
    board.add_argument("--max-polls", type=int, default=DEFAULT_MAX_POLLS,
                       help="stop after this many reads (default 720 = 1 h at 5 s)")
    board.add_argument("--since", help="rows ordered after this mail id")
    board.add_argument("--session", help="rows from or to this session (or to '*')")
    board.add_argument("--json", action="store_true", help="the rows as JSON")
    bsub = board.add_subparsers(dest="board_cmd")
    post = bsub.add_parser("post", help="write a human note or ruling through the single writer")
    post.add_argument("--to", required=True, help="recipient session, or '*'")
    post.add_argument("text", help="the message body (≤ 4 KiB utf-8)")
    post.add_argument("--kind", default="note", help="note (default) or ruling")
    post.add_argument("--ref", help="the decision request a ruling answers (DR-n), or a path/id")
    post.add_argument("--from", dest="sender", help="sender (default: $AGENT_SESSION or 'human')")
    post.add_argument("--writer", help="path to the module exposing append_mail (default: sibling coord-mail.py)")
    return ap


def main(argv=None):
    args = build_parser().parse_args(argv)
    if args.cmd != "board":
        build_parser().print_help()
        return 2
    root, err = resolve_root(os.getcwd(), args.root)
    if err:
        print(err, file=sys.stderr)
        return 2
    if getattr(args, "board_cmd", None) == "post":
        return cmd_post(args, root)
    if args.max_polls < 1:
        print("board refused: --max-polls must be at least 1", file=sys.stderr)
        return 2
    return cmd_board(args, root)


if __name__ == "__main__":
    sys.exit(main())
