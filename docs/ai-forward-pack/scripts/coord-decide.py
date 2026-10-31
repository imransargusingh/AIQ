#!/usr/bin/env python3
"""coord-decide.py - the Owner seat's mechanism: decision request -> numbered ruling (D6).

THE CLASS (proposal §2.1 A7, §4, §7 P5; spec-owner-review). The doctrine says the Owner reviews by
decision request -> numbered ruling (CO1) and the kick ladder escalates with a decision request
(CO17). Until now nothing implemented it: the number was typed by hand and enforced by reputation.
ai-de measured where that ends (tools/verify-ruling-citations.py, 2026-09-11): eight numbers cited
as binding defined nothing, and one number was allocated twice BECAUSE nothing recorded the first.

WHAT THIS IS NOT. Not a store and not an allocator (class ID-A):
  - a decision request IS P1's typed seam request. `request` runs `coord-core.py request add`
    with `--reason decision-request` and the five decision fields as a JSON object in
    `--contract`; `rule` runs `coord-core.py request resolve`. This file never opens the
    request store.
  - the two mails (`decision-request`, `ruling`) go through P4's single writer, `append_mail`
    in coord-mail.py, imported by path (the coord-board.py idiom).
  - the ruling number is read from the register's own headings: `### Ruling NN — <title>` or
    `## R-n · ...` in docs/notes/rulings.md, the only file this script writes and the only
    definition site (verify-ruling-citations.py is the gate, and reads the same grammar). A
    register whose headings none parse is NOT CHECKED: nothing is allocated from it (PACK-P).

VERBS
  request  --to <owner-session> --options T --evidence T --recommendation T --reversibility T
           --blast-radius T --deadline <seconds|default> --fallback T [--ref <mail-id>] "<question>"
           refused (exit 2, nothing written) without every one of them; writes the P1 row, then
           the decision-request mail (ref = the request id); prints one JSON line.
  rule     <n|next> --title T --text T --request <req-id>
           n must be the next number (max defined + 1); a defined number, a gap, a self-rule
           (requester == ruler, D6) are refused before anything is written. Appends the heading,
           resolves the request with "Ruling n", mails the requester.
  list     [--json]   open decision requests + the register's rulings; an absent store or
           register renders NOT CHECKED (never quiet).

EXIT  0 ok · 2 refused · 3 request terminal · 4 not checked (store unreadable, request unknown,
      siblings not installed) · otherwise the child's code (coord-core.py's stderr passes through)
"""
from __future__ import annotations

import argparse
import datetime as _dt
import importlib.util
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

HERE = Path(__file__).resolve().parent
REGISTER_REL = Path("docs") / "notes" / "rulings.md"
REASON = "decision-request"
FIELDS: Tuple[Tuple[str, str, str], ...] = (
    ("options", "--options", "options"), ("evidence", "--evidence", "evidence"),
    ("recommendation", "--recommendation", "recommendation"),
    ("reversibility", "--reversibility", "reversibility"),
    ("blast_radius", "--blast-radius", "blast radius"))
# The definition grammar of verify-ruling-citations.py (the gate): `## Ruling n` or `## R-n`, where R-n ends
# the id (not R-2.3, R-3-4, R-12a). test_coord_decide pins the two to the same numbers (PACK-P, ID-A).
SHORT = r"R-(\d{1,3})(?!\w|[.-]\d)"
HEADING = re.compile(r"^#{2,3}[ \t]+(?:Ruling[ \t]+(\d{1,3})\b|" + SHORT + r")(.*)$", re.M)
ANY_HEADING = re.compile(r"^#{2,3}[ \t]+\S", re.M)
PROVENANCE = re.compile(r"^- request: (\S+)", re.M)
EXIT_OK, EXIT_REFUSED, EXIT_TERMINAL, EXIT_NOT_CHECKED = 0, 2, 3, 4
REGISTER_FRONTMATTER = """---
id: rulings
title: "Rulings — the Owner seat's numbered decisions (the only definition site)"
type: doc
status: accepted
owner: "@owner"
phase: "coordination"
tags: [coordination, owner-review, rulings, register]
links:
  - { to: spec-owner-review, rel: relates-to }
review-by: "{review_by}"
summary: >-
  The ruling register. Each `### Ruling NN — <title>` heading defines exactly one numbered
  decision of the Owner seat; prose anywhere cites it as `Ruling NN`. Written only by
  `coord decide rule`; numbering is read from these headings; verify-ruling-citations.py fails a
  cited number with no heading here and a number defined twice.
---

# Rulings

One heading, one decision. A ruling is never edited in place: a later ruling supersedes it in
prose and cites it. Each block carries the decision request it answered and who ruled.
"""


def refuse(code: str, what: str, because: str, remedy: str, exit_code: int = EXIT_REFUSED) -> int:
    print("{}  {}\n  because   {}\n  remedy    {}".format(code, what, because, remedy), file=sys.stderr)
    return exit_code


def _load_by_path(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, str(path))
    if spec is None or spec.loader is None:
        raise ImportError(str(path))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_siblings(scripts: Path) -> Tuple[Optional[Any], Optional[Any]]:
    """coord-core.py (the request reader + resolve_root) and coord-mail.py (append_mail)."""
    core_path, mail_path = scripts / "coord-core.py", scripts / "coord-mail.py"
    if not core_path.is_file() or not mail_path.is_file():
        return None, None
    if str(scripts) not in sys.path:
        sys.path.insert(0, str(scripts))
    return _load_by_path("coord_core_for_decide", core_path), _load_by_path("coord_mail_for_decide", mail_path)


def iso_utc(now: float) -> str:
    return _dt.datetime.fromtimestamp(now, tz=_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


# --- the register -----------------------------------------------------------------------------

def parse_register(path: Path) -> List[Dict[str, Any]]:
    """[{number, title, request}] in file order; an absent file is an empty register."""
    if not path.is_file():
        return []
    text = path.read_text(encoding="utf-8")
    matches = list(HEADING.finditer(text))
    rulings = []
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        block = text[match.end():end]
        prov = PROVENANCE.search(block)
        rulings.append({"number": int(match.group(1) or match.group(2)), "title": match.group(3).strip(" \t—·:-"),
                        "request": prov.group(1) if prov else None})
    return rulings


def unread_headings(path: Path) -> int:
    """Level-2/3 headings in a register where NONE parses as a ruling (PACK-P): an unread register is not
    an empty one, and allocating 1 in it would collide with its own numbering."""
    if not path.is_file():
        return 0
    text = path.read_text(encoding="utf-8")
    return 0 if HEADING.search(text) else len(ANY_HEADING.findall(text))


def next_number(rulings: List[Dict[str, Any]]) -> int:
    return max((r["number"] for r in rulings), default=0) + 1


def append_ruling(path: Path, number: int, title: str, text: str, request: str, session: str, now: float) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.is_file():
        review_by = _dt.datetime.fromtimestamp(now, tz=_dt.timezone.utc) + _dt.timedelta(days=180)
        with open(path, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(REGISTER_FRONTMATTER.replace("{review_by}", review_by.strftime("%Y-%m-%d")))
    block = "\n### Ruling {} — {}\n\n{}\n\n- request: {} · ruled by: {} · at: {}\n".format(
        number, title, text, request, session, iso_utc(now))
    existing = path.read_bytes()
    if existing and not existing.endswith(b"\n"):
        block = "\n" + block
    with open(path, "a", encoding="utf-8", newline="\n") as fh:
        fh.write(block)


# --- shared plumbing --------------------------------------------------------------------------

def _checkout_top(cwd: str, fallback: Path) -> Path:
    """The top of the CURRENT checkout (primary or linked worktree): the first ancestor holding
    a `.git` entry. The register is a tracked document and is written where the session
    commits (WT1); the `.agents` stores stay at the primary (`core.repo_root`). Filesystem
    walk, no subprocess, for the same budget reason as `repo_root` itself."""
    here = Path(cwd).resolve()
    for candidate in (here, *here.parents):
        if (candidate / ".git").exists():
            return candidate
    return fallback


def _root_and_repo(core: Any, raw_root: Optional[str]) -> Tuple[Optional[Path], Optional[Path], Optional[Dict[str, str]]]:
    root, err = core.resolve_root(os.getcwd(), raw_root or os.environ.get("COORD_ROOT"))
    if err:
        return None, None, err
    primary = Path(core.repo_root(os.getcwd()))
    return Path(root), _checkout_top(os.getcwd(), primary), None


def _register_path(repo: Path, explicit: Optional[str]) -> Optional[Path]:
    """The register, inside the repository or refused (a --register outside it writes nothing)."""
    path = Path(explicit) if explicit else repo / REGISTER_REL
    if not path.is_absolute():
        path = Path(os.getcwd()) / path
    path = path.resolve()
    try:
        path.relative_to(repo.resolve())
    except ValueError:
        return None
    return path


def _run_core(core_path: Path, root: Path, *args: str) -> subprocess.CompletedProcess:
    env = dict(os.environ, COORD_ROOT=str(root), PYTHONIOENCODING="utf-8")
    return subprocess.run([sys.executable, str(core_path), *args], cwd=os.getcwd(), env=env,
                          capture_output=True, text=True, encoding="utf-8", errors="replace")


def _send(mail: Any, root: Path, session: str, entry: Dict[str, Any]) -> str:
    """The mail id, or `not sent: <code>` - the store is truth, the mail is push; a failure here
    never changes the verdict of the write that preceded it (a non-zero would invite a retry
    that duplicates the request)."""
    try:
        return str(mail.append_mail(root, session, entry))
    except mail.MailError as exc:
        return "not sent: {} {}".format(exc.code, exc)
    except OSError as exc:
        return "not sent: OSError {}".format(exc)


def _fold(core: Any, root: Path) -> Tuple[Optional[Dict[str, Dict[str, Any]]], List[str]]:
    events, errors = core.read_request_events(root)
    if errors:
        return None, errors
    return {r["id"]: r for r in core.fold_requests(events)}, []


# --- verbs --------------------------------------------------------------------------------------

def decision_body(question: str, contract: Dict[str, str]) -> str:
    lines = [question]
    for key, _flag, label in FIELDS:
        lines.append("{}: {}".format(label, contract[key]))
    return "\n".join(lines)


def cmd_request(args: argparse.Namespace, core: Any, mail: Any, core_path: Path, root: Path, session: str) -> int:
    missing = [flag for key, flag, _label in FIELDS if not (getattr(args, key) or "").strip()]
    if not (args.to or "").strip():
        missing.append("--to")
    if args.deadline is None:
        missing.append("--deadline")
    if not (args.fallback or "").strip():
        missing.append("--fallback")
    if not (args.question or "").strip():
        missing.append("<question>")
    if missing:
        return refuse("COORD-DECIDE-INCOMPLETE",
                      "a decision request without {} cannot be ruled on".format(" and ".join(missing)),
                      "the Owner rules from options, evidence, recommendation, reversibility and blast"
                      " radius, and every request ends by its deadline through its fallback (D5, D6)",
                      "pass --to --options --evidence --recommendation --reversibility --blast-radius"
                      " --deadline --fallback and the question")
    to = args.to.strip()
    if to == mail.BROADCAST or not mail.SESSION_RE.match(to):
        return refuse("COORD-DECIDE-TO", "a decision request is addressed to one Owner session, not {!r}".format(to),
                      "a ruling has one author; a broadcast has none", "--to <the owner or coordinator session>")
    contract = {key: getattr(args, key).strip() for key, _flag, _label in FIELDS}
    body = decision_body(args.question.strip(), contract)
    size = len(body.encode("utf-8"))
    if size > mail.BODY_MAX_BYTES:
        return refuse("COORD-DECIDE-BODY-SIZE", "the question and five fields are {} bytes; the mail limit is {}".format(
            size, mail.BODY_MAX_BYTES), "the decision-request mail must carry what the Owner rules from",
            "shorten the fields, or cite a file path instead of pasting its contents")
    argv = ["request", "add", "--to", to, "--deadline", str(args.deadline), "--fallback", args.fallback.strip(),
            "--contract", json.dumps(contract, sort_keys=True), "--reason", REASON]
    if args.ref:
        argv += ["--ref", args.ref]
    argv += ["--", args.question.strip()]
    child = _run_core(core_path, root, *argv)
    if child.returncode != 0:
        sys.stderr.write(child.stderr)
        sys.stdout.write(child.stdout)
        return child.returncode
    record = json.loads(child.stdout.strip().splitlines()[-1])
    record["mail"] = _send(mail, root, session, {"to": to, "kind": REASON, "body": body, "ref": record["id"]})
    print(json.dumps(record, sort_keys=True))
    return EXIT_OK


def cmd_rule(args: argparse.Namespace, core: Any, mail: Any, core_path: Path, root: Path, repo: Path, session: str) -> int:
    register = _register_path(repo, args.register)
    if register is None:
        return refuse("COORD-RULING-REGISTER", "--register must resolve inside the repository: {}".format(args.register),
                      "the register is a tracked file (class register); a ruling written elsewhere defines nothing",
                      "omit --register (default docs/notes/rulings.md) or point it inside the repo")
    title, text = (args.title or "").strip(), (args.text or "").strip()
    if not title or not text:
        return refuse("COORD-RULING-INCOMPLETE", "a ruling needs --title and --text",
                      "a heading with no text is a number with a reputation", "rule <n> --title T --text T --request <id>")
    unread = unread_headings(register)
    if unread:
        return refuse("COORD-RULING-NOT-CHECKED", "{} has {} heading(s) and none is a ruling (`## Ruling n` or `## R-n`)"
                      .format(register, unread),
                      "an unread register is not an empty one; allocating from it would collide (class PACK-P)",
                      "write the register's headings as `## R-n` or `### Ruling n — <title>`", exit_code=EXIT_NOT_CHECKED)
    rulings = parse_register(register)
    expected = next_number(rulings)
    if args.number == "next":
        number = expected
    else:
        try:
            number = int(args.number)
        except ValueError:
            return refuse("COORD-RULING-NUMBER", "{!r} is not a ruling number".format(args.number),
                          "numbers come from the register's headings", "rule {} (or `next`)".format(expected))
    defined = {r["number"] for r in rulings}
    if number in defined:
        return refuse("COORD-RULING-DEFINED", "Ruling {} is already defined in {}".format(number, register),
                      "one number, one decision (class ID-A); the register is the allocator",
                      "rule {} (or `next`)".format(expected))
    if number != expected:
        return refuse("COORD-RULING-NOT-NEXT", "Ruling {} is not the next number".format(number),
                      "numbering is monotonic from the register itself; a gap is a number nobody defined",
                      "rule {} (or `next`)".format(expected))
    folded, errors = _fold(core, root)
    if folded is None:
        print("COORD-REQUEST-NOT-CHECKED  {}".format("; ".join(errors[:2])), file=sys.stderr)
        return EXIT_NOT_CHECKED
    row = folded.get(args.request)
    if row is None:
        print("COORD-REQUEST-NOT-FOUND  {}".format(args.request))
        return EXIT_NOT_CHECKED
    if row.get("status") in core.REQUEST_TERMINAL:
        print("COORD-REQUEST-TERMINAL  {} is already {}".format(args.request, row.get("status")))
        return EXIT_TERMINAL
    requester = str(row.get("from") or row.get("session") or "")
    if requester == session:
        return refuse("COORD-RULING-SELF", "{} sent {} and may not rule on it".format(session, args.request),
                      "reviewer != author (D6); the Owner never clears its own veto",
                      "rule from the Owner or coordinator session (export AGENT_SESSION=<owner>)")
    now = time.time()
    append_ruling(register, number, title, text, args.request, session, now)
    resolution = "Ruling {}".format(number)
    child = _run_core(core_path, root, "request", "resolve", args.request, "--resolution", resolution)
    if child.returncode != 0:
        sys.stderr.write(child.stderr)
        sys.stdout.write(child.stdout)
        return refuse("COORD-RULING-UNRESOLVED", "{} is defined in {} but {} was not resolved (coord-core exit {})".format(
            resolution, register, args.request, child.returncode),
            "the heading is appended before the resolve so a resolution never points at no heading",
            "coord request resolve {} --resolution \"{}\"".format(args.request, resolution),
            exit_code=child.returncode)
    heading = "{} — {}".format(resolution, title)
    body = heading + "\n" + text
    if len(body.encode("utf-8")) > mail.BODY_MAX_BYTES:
        body = heading + "\n(the text is in {})".format(REGISTER_REL.as_posix())
    sent = _send(mail, root, session, {"to": requester, "kind": "ruling", "body": body, "ref": args.request})
    print(json.dumps({"ruling": number, "title": title, "request": args.request, "resolution": resolution,
                      "mail": sent, "register": str(register)}, sort_keys=True))
    return EXIT_OK


def _deadline_text(row: Dict[str, Any], now: float) -> str:
    if row.get("deadline_at") is None:
        return "untyped"
    remaining = float(row["deadline_at"]) - now
    return "in {:.0f}s".format(remaining) if remaining >= 0 else "overdue {:.0f}s".format(-remaining)


def cmd_list(args: argparse.Namespace, core: Any, root: Path, repo: Path) -> int:
    now = time.time()
    not_checked: List[str] = []
    store = Path(core.request_log_path(root))
    open_rows: Optional[List[Dict[str, Any]]] = None
    if store.is_file():
        folded, errors = _fold(core, root)
        if folded is None:
            not_checked.append("NOT CHECKED — requests store unreadable: {}".format("; ".join(errors[:2])))
        else:
            open_rows = [r for r in folded.values()
                         if r.get("reason") == REASON and r.get("status") in core.REQUEST_OPEN]
    else:
        not_checked.append("NOT CHECKED — no requests store at {}".format(store))
    register = _register_path(repo, args.register) or (repo / REGISTER_REL)
    rulings = parse_register(register) if register.is_file() else None
    if rulings is None:
        not_checked.append("NOT CHECKED — no register at {}".format(register))
    if args.json:
        payload = {"open": None if open_rows is None else [
            {"id": r["id"], "from": r.get("from"), "to": r.get("to"), "status": r.get("status"),
             "deadline": _deadline_text(r, now), "text": r.get("text", ""), "contract": r.get("contract", "")}
            for r in open_rows], "rulings": rulings, "not_checked": not_checked}
        print(json.dumps(payload, indent=2, sort_keys=True))
        return EXIT_OK
    for line in not_checked:
        print(line)
    if open_rows is not None:
        print("{} open decision request(s)".format(len(open_rows)))
        for r in open_rows:
            print("  {:<30} {:<9} {} -> {}  {:<14} {}".format(
                str(r.get("id", ""))[:30], str(r.get("status", ""))[:9], r.get("from", ""), r.get("to", ""),
                _deadline_text(r, now), str(r.get("text", ""))[:80]))
    if rulings is not None:
        print("{} ruling(s) in {}".format(len(rulings), register))
        for r in rulings:
            print("  Ruling {:<4} {:<60} request: {}".format(r["number"], r["title"][:60], r["request"] or "not recorded"))
    return EXIT_OK


# --- CLI ----------------------------------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", help="the .agents directory (default: $COORD_ROOT or <repo>/.agents)")
    parser.add_argument("--register", help="the ruling register (default: <repo>/docs/notes/rulings.md; inside the repo)")
    parser.add_argument("--scripts", help="directory holding coord-core.py and coord-mail.py (default: beside this script)")
    sub = parser.add_subparsers(dest="cmd", required=True)
    rq = sub.add_parser("request", help="raise a decision request (P1 request + decision-request mail)")
    rq.add_argument("question", nargs="?", default="", help="the decision asked, one sentence")
    rq.add_argument("--to", default="", help="the Owner (or coordinator) session that rules")
    for key, flag, label in FIELDS:
        rq.add_argument(flag, dest=key, default="", help="the {} the Owner rules from".format(label))
    rq.add_argument("--deadline", default=None, metavar="SECONDS", help="seconds until terminal, or `default`")
    rq.add_argument("--fallback", default="", metavar="TEXT", help="what the requester does at the deadline")
    rq.add_argument("--ref", default="", help="the kick mail id this request escalates (CO17 rung 2)")
    rl = sub.add_parser("rule", help="answer a decision request with the next numbered ruling")
    rl.add_argument("number", help="the ruling number - must be the next one - or `next`")
    rl.add_argument("--title", required=True)
    rl.add_argument("--text", required=True)
    rl.add_argument("--request", required=True, metavar="REQ-ID")
    ls = sub.add_parser("list", help="open decision requests and the register's rulings")
    ls.add_argument("--json", action="store_true")
    return parser


def main(argv: Optional[List[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    scripts = Path(args.scripts).resolve() if args.scripts else HERE
    core, mail = load_siblings(scripts)
    if core is None or mail is None:
        return refuse("COORD-DECIDE-NOT-INSTALLED", "coord-core.py and coord-mail.py are not in {}".format(scripts),
                      "a decision request is P1's request and its mail is P4's; without them there is no store to write",
                      "install the pack's script bundle beside this script, or pass --scripts <dir>",
                      exit_code=EXIT_NOT_CHECKED)
    root, repo, err = _root_and_repo(core, args.root)
    if err or root is None or repo is None:
        print("{}  {}".format(err.get("code", "COORD-NOT-CHECKED-ROOT"), err.get("reason", "")), file=sys.stderr)
        return EXIT_NOT_CHECKED
    if args.cmd == "list":
        return cmd_list(args, core, root, repo)
    session = os.environ.get("AGENT_SESSION") or ""
    if not session:
        return refuse("COORD-DECIDE-IDENTITY", "AGENT_SESSION is unset", "a request has a sender and a ruling has an author",
                      "export AGENT_SESSION=<your session id>", exit_code=EXIT_NOT_CHECKED)
    core_path = scripts / "coord-core.py"
    if args.cmd == "request":
        return cmd_request(args, core, mail, core_path, root, session)
    return cmd_rule(args, core, mail, core_path, root, repo, session)


if __name__ == "__main__":
    sys.exit(main())
