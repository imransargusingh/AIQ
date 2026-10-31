#!/usr/bin/env python3
"""prompt-log.py — the fast prompt-reuse lens over the project's audit log.

A tiny, stdlib-only front-end for browsing, searching, and **reusing** the prompts already
recorded in the committed **audit log** (docs/audit/audit-log.jsonl). Unified with the Audit &
Change Log Standard (audit-and-change-log.md): there is **one store of prompts** — the audit
log — and this is the reuse lens over it (its arrow-navigable stack + clipboard reuse), the
companion to the broader /auditlog timeline/search/change-log/viewer.

  add      log a prompt (writes a kind:prompt entry to the audit log)  -> via audit-log.py
  list     show the stack, newest first (label · time)
  search   freeform search; matches contain ALL terms
  show     print one entry in full (label, time, text)
  get      print one entry's RAW text only (for piping/copying)
  browse   interactive stack: Up/Down move, Right expand, Left collapse, Enter reuse
  pick     like browse, pre-filtered by a search query (powers /searchprompts)

REUSE MODEL (honest about the medium). A script cannot type into the Copilot CLI's input
line, so "reuse" copies the chosen prompt to the clipboard (pbcopy, when present) and prints
it — you paste it into your next prompt (Cmd+V) and edit before sending.

ONE STORE. The default store is the committed audit log (docs/audit/audit-log.jsonl), so every
prompt the audit mandate records — skill runs, scripts, and prompts you `add` — is reusable
here, and there is no second parallel prompt store. `add` writes through audit-log.py (the
single writer of record, AL0.1) as a kind:prompt entry. Override the store with --store or
$AIFORWARD_PROMPT_LOG (e.g. a legacy <repo>/.aiforward/prompts.jsonl); the reader adapts to
either schema. Stdlib only; no third-party import.
"""
import argparse
import json
import os
import sys
import shutil
import subprocess
import uuid
from datetime import datetime, timezone

# Windows consoles default to cp1252, which cannot encode the box/arrow glyphs this
# tool prints - `prompt-log.py --help` crashed outright with UnicodeEncodeError (FR-047).
# The other scripts survived only because their glyphs happen to exist in cp1252, which is
# luck rather than an invariant, so the guard is applied uniformly.
for _stream in (sys.stdin, sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass


ENV_STORE = "AIFORWARD_PROMPT_LOG"
AUDIT_REL = os.path.join("docs", "audit", "audit-log.jsonl")   # the unified store of record
LEGACY_DIRNAME = ".aiforward"                                   # pre-unification personal store
LEGACY_FILENAME = "prompts.jsonl"


# ----------------------------------------------------------------------------- store

def _repo_root(start=None):
    """Nearest ancestor containing .git (the repo root), else the start dir."""
    d = os.path.abspath(start or os.getcwd())
    while True:
        if os.path.exists(os.path.join(d, ".git")):   # a dir, or a linked worktree's pointer FILE (WT-A)
            return d
        parent = os.path.dirname(d)
        if parent == d:
            return os.path.abspath(start or os.getcwd())
        d = parent


def resolve_store(explicit=None):
    if explicit:
        return os.path.abspath(explicit)
    env = os.environ.get(ENV_STORE)
    if env:
        return os.path.abspath(env)
    return os.path.join(_repo_root(), AUDIT_REL)  # the unified audit log


def _ensure_store_dir(store):
    """Create the store dir (used only for a legacy/explicit --store; the audit log dir
    is owned by audit-log.py)."""
    d = os.path.dirname(store)
    if d and not os.path.isdir(d):
        os.makedirs(d, exist_ok=True)
    # A legacy .aiforward store stays git-ignored so a personal scratchpad is never committed.
    if d and os.path.basename(d) == LEGACY_DIRNAME:
        gi = os.path.join(d, ".gitignore")
        if not os.path.exists(gi):
            with open(gi, "w", encoding="utf-8", newline="\n") as f:
                f.write("# AI-Forward legacy prompt log — local history, never committed.\n*\n")


def _adapt(e):
    """Map a stored entry to the reuse-stack shape {id, ts, label, text, tags}.

    Handles BOTH the audit-log schema (prompt/shortname/datetime, the unified store) and the
    legacy prompt-log schema (text/label/ts), so the lens reads either store transparently.
    """
    text = e.get("prompt")
    if text is None:
        text = e.get("text", "")
    label = e.get("shortname") or e.get("label") or _derive_label(text or "")
    ts = e.get("datetime") or e.get("ts") or ""
    compiled = e.get("compiled")
    raw_id = compiled.get("raw_id") if isinstance(compiled, dict) else None
    return {"id": e.get("id", ""), "ts": ts, "label": label, "text": text or "",
            "tags": e.get("tags") or [], "kind": e.get("kind"), "skill": e.get("skill"),
            "raw_id": raw_id}


def display_label(e):
    """The stack row's label. A `kind:compilation` row (P7) is the rendered compiled twin of a
    logged prompt, so it is suffixed with the raw prompt it was compiled from - the operator
    sees both and can reuse either (spec-compile-stage US-6)."""
    label = e.get("label") or "(untitled)"
    if e.get("kind") == "compilation":
        return f"{label} \u27f2 compiled from {e.get('raw_id') or '?'}"
    return label


def filter_raw(entries, raw_id):
    """--raw <al-id>: the raw prompt and its compilations only (any kind whose id is the raw id,
    plus every compilation naming it); preserves order."""
    if not raw_id:
        return entries
    return [e for e in entries if e.get("id") == raw_id or e.get("raw_id") == raw_id]


def load_entries(store):
    """Return entries oldest-first in the stack shape; callers reverse for newest-first views.

    Reads the unified audit log (or a legacy store) and adapts each row; rows with no prompt
    text are skipped (you cannot reuse an empty prompt)."""
    if not os.path.exists(store):
        return []
    out = []
    with open(store, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                e = _adapt(json.loads(line))
            except json.JSONDecodeError:
                continue  # skip a corrupt line rather than crash the whole log
            if e["text"].strip():
                out.append(e)
    return out


def append_entry(store, entry):
    _ensure_store_dir(store)
    with open(store, "a", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps(entry, ensure_ascii=False) + "\n")


# ----------------------------------------------------------------------------- helpers

def _now_iso():
    return datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds")


def _derive_label(text, explicit=None, limit=72):
    if explicit:
        return explicit.strip()[:limit]
    for line in text.splitlines():
        line = line.strip().lstrip("#").strip()
        if line:
            return (line[:limit] + "…") if len(line) > limit else line
    return "(untitled prompt)"


def _fmt_time(iso):
    try:
        dt = datetime.fromisoformat(iso)
        return dt.strftime("%Y-%m-%d %H:%M")
    except (ValueError, TypeError):
        return iso or "?"


def newest_first(entries):
    return list(reversed(entries))


def filter_entries(entries, query):
    """Case-insensitive AND match over label+text; preserves order."""
    terms = [t.lower() for t in query.split() if t.strip()]
    if not terms:
        return entries
    res = []
    for e in entries:
        hay = (e.get("label", "") + "\n" + e.get("text", "")).lower()
        if all(t in hay for t in terms):
            res.append(e)
    return res


def resolve_one(entries_newest, ref):
    """Resolve a 1-based newest-first index OR an id (full/unique-prefix) to an entry."""
    if ref.isdigit():
        i = int(ref)
        if 1 <= i <= len(entries_newest):
            return entries_newest[i - 1]
        return None
    matches = [e for e in entries_newest if e.get("id", "").startswith(ref)]
    return matches[0] if len(matches) == 1 else None


def copy_to_clipboard(text):
    """pbcopy (macOS) / xclip / wl-copy (Wayland) / clip.exe; returns the tool name or None.

    Two cross-platform rules (DC-211). (1) `clip.exe` decodes its stdin with the console
    code page, so UTF-8 bytes land as mojibake - it is fed UTF-16LE with a BOM, the one
    encoding it reads unambiguously whatever the code page is. (2) The ladder falls
    THROUGH: a tool that is on PATH but fails to launch (an xclip with no DISPLAY, a WSL
    shim) hands its turn to the next rung instead of ending the ladder at the first
    failure, which previously returned None with Wayland/clip.exe still untried."""
    for tool, cmd in (("pbcopy", ["pbcopy"]),
                      ("xclip", ["xclip", "-selection", "clipboard"]),
                      ("wl-copy", ["wl-copy"]),
                      ("clip", ["clip.exe"])):
        if not shutil.which(cmd[0]):
            continue
        if cmd[0] == "clip.exe":
            payload = b"\xff\xfe" + text.encode("utf-16-le", "replace")
        else:
            payload = text.encode("utf-8")
        try:
            subprocess.run(cmd, input=payload, check=True)
            return tool
        except (subprocess.SubprocessError, OSError):
            continue
    return None


# ----------------------------------------------------------------------------- commands

def _sibling(name):
    return os.path.join(os.path.dirname(os.path.abspath(__file__)), name)


def cmd_add(args):
    if getattr(args, "file", None):
        # Pack finding #1: a prompt carrying an arrow or an em dash died in argv under a
        # cp1252 console. A file is the one path with no console in it.
        with open(args.file, encoding="utf-8") as handle:
            text = handle.read()
    elif args.text:
        text = args.text
    elif args.words:
        text = " ".join(args.words)
    elif not sys.stdin.isatty():
        text = sys.stdin.read()
    else:
        sys.stderr.write("prompt-log add: provide --text, positional words, or pipe via stdin.\n")
        return 2
    text = text.strip()
    if not text:
        sys.stderr.write("prompt-log add: empty prompt, nothing logged.\n")
        return 0  # not an error — just nothing to do
    label = _derive_label(text, args.label)

    # Default: log a kind:prompt entry to the unified audit log via audit-log.py (the single
    # writer of record, AL0.1) — so a logged prompt is reusable here AND visible in /auditlog.
    # An explicit --store keeps the legacy direct-append behaviour (a personal/.aiforward store).
    if not args.store:
        audit = _sibling("audit-log.py")
        if os.path.exists(audit):
            cmd = [sys.executable, audit, "--root", os.path.join(_repo_root(), "docs"),
                   "append", "--kind", "prompt", "--shortname", label,
                   "--session", args.session or "prompt-log",
                   "--summary", args.summary or "prompt logged for reuse",
                   "--prompt-file", "-"]
            for t in (args.tag or []):
                cmd += ["--tag", t]
            # The seam this script owns is UTF-8 by construction: the pipe is encoded here
            # and the child is told to decode it the same way (its own stdin reconfigure
            # is the second half). Before this, `text=True` used the console code page and
            # an arrow arrived as "â†’" with no error (pack finding #1).
            env = dict(os.environ)
            env["PYTHONIOENCODING"] = "utf-8"
            r = subprocess.run(cmd, input=text, text=True, encoding="utf-8", env=env)
            if r.returncode == 0 and not args.quiet:
                print(f"logged to the audit log: {label}")
            return r.returncode
        # audit-log.py absent (unexpected): fall through to a direct write so `add` still works.

    store = resolve_store(args.store)
    entry = {
        "id": datetime.now().strftime("%Y%m%dT%H%M%S") + "-" + uuid.uuid4().hex[:6],
        "ts": _now_iso(),
        "label": label,
        "text": text,
        "tags": list(args.tag or []),
    }
    append_entry(store, entry)
    if not args.quiet:
        print(f"logged: {entry['label']}  ·  {_fmt_time(entry['ts'])}  ({entry['id']})")
    return 0


def _print_list(entries_newest, as_json=False):
    if as_json:
        print(json.dumps(entries_newest, ensure_ascii=False, indent=2))
        return
    if not entries_newest:
        print("no prompts logged yet — `prompt-log add \"<your prompt>\"` to start.")
        return
    width = len(str(len(entries_newest)))
    for i, e in enumerate(entries_newest, 1):
        print(f"  {str(i).rjust(width)}. {display_label(e)}   ·   {_fmt_time(e.get('ts'))}")


def cmd_list(args):
    entries = newest_first(load_entries(resolve_store(args.store)))
    entries = filter_raw(entries, getattr(args, "raw", None))
    if args.limit and args.limit > 0:
        entries = entries[: args.limit]
    _print_list(entries, args.json)
    return 0


def cmd_search(args):
    entries = newest_first(load_entries(resolve_store(args.store)))
    entries = filter_raw(entries, getattr(args, "raw", None))
    matches = filter_entries(entries, " ".join(args.query))
    if not args.json:
        print(f"{len(matches)} match(es) for: {' '.join(args.query)!r}")
    _print_list(matches, args.json)
    return 0


def cmd_show(args):
    entries = newest_first(load_entries(resolve_store(args.store)))
    e = resolve_one(entries, args.ref)
    if not e:
        sys.stderr.write(f"prompt-log show: no entry for {args.ref!r}\n")
        return 1
    print(f"label : {e.get('label')}")
    print(f"time  : {_fmt_time(e.get('ts'))}")
    print(f"id    : {e.get('id')}")
    if e.get("tags"):
        print(f"tags  : {', '.join(e['tags'])}")
    print("-" * 60)
    print(e.get("text", ""))
    return 0


def cmd_get(args):
    entries = newest_first(load_entries(resolve_store(args.store)))
    e = resolve_one(entries, args.ref)
    if not e:
        sys.stderr.write(f"prompt-log get: no entry for {args.ref!r}\n")
        return 1
    sys.stdout.write(e.get("text", ""))
    if sys.stdout.isatty():
        sys.stdout.write("\n")
    if args.copy:
        tool = copy_to_clipboard(e.get("text", ""))
        if tool:
            sys.stderr.write(f"\n(copied to clipboard via {tool})\n")
    return 0


def _reuse(entry, no_copy=False):
    """Print a chosen prompt and copy it to the clipboard for paste-and-edit."""
    text = entry.get("text", "")
    print("\n" + "=" * 60)
    print(f"reuse: {entry.get('label')}  ·  {_fmt_time(entry.get('ts'))}")
    print("=" * 60)
    print(text)
    print("=" * 60)
    if not no_copy:
        tool = copy_to_clipboard(text)
        if tool:
            print(f"(copied to clipboard via {tool} — paste with Cmd/Ctrl+V into your next prompt and edit)")
        else:
            print("(no clipboard tool found — copy the text above to reuse it)")


# ----------------------------------------------------------------------------- curses TUI

def _run_curses(entries_newest):
    """Interactive stack. Returns the selected entry, or None. Requires a real terminal."""
    import curses
    import textwrap

    def app(stdscr):
        curses.curs_set(0)
        stdscr.keypad(True)
        cursor = 0
        top = 0
        expanded = set()
        filt = ""
        view = entries_newest

        def visible():
            return filter_entries(entries_newest, filt) if filt else entries_newest

        while True:
            view = visible()
            if cursor >= len(view):
                cursor = max(0, len(view) - 1)
            h, w = stdscr.getmaxyx()
            stdscr.erase()
            header = " prompt-log — ↑/↓ move · → expand · ← collapse · / search · Enter reuse · q quit "
            stdscr.addnstr(0, 0, header.ljust(w)[: w - 1], w - 1, curses.A_REVERSE)
            if filt:
                stdscr.addnstr(1, 0, f" filter: {filt}".ljust(w)[: w - 1], w - 1, curses.A_BOLD)
            row = 2
            # Build a flat list of render lines tagged with their entry index.
            lines = []
            for idx, e in enumerate(view):
                marker = "▸" if idx not in expanded else "▾"
                label = display_label(e)
                lines.append((idx, f"{marker} {label}   ·   {_fmt_time(e.get('ts'))}", True))
                if idx in expanded:
                    for seg in e.get("text", "").splitlines() or [""]:
                        for wrapped in (textwrap.wrap(seg, max(10, w - 6)) or [""]):
                            lines.append((idx, "    " + wrapped, False))
            # Keep the cursor's header line in view.
            header_positions = [li for li, (idx, _, is_head) in enumerate(lines) if is_head and idx == cursor]
            cpos = header_positions[0] if header_positions else 0
            avail = h - row - 1
            if cpos < top:
                top = cpos
            elif cpos >= top + avail:
                top = cpos - avail + 1
            for li in range(top, min(len(lines), top + avail)):
                idx, text, is_head = lines[li]
                attr = curses.A_NORMAL
                if is_head and idx == cursor:
                    attr = curses.A_REVERSE
                elif not is_head:
                    attr = curses.A_DIM
                try:
                    stdscr.addnstr(row, 0, text[: w - 1], w - 1, attr)
                except curses.error:
                    pass
                row += 1
            stdscr.refresh()

            ch = stdscr.getch()
            if ch in (ord("q"), 27):  # q or ESC
                return None
            elif ch in (curses.KEY_UP, ord("k")):
                cursor = max(0, cursor - 1)
            elif ch in (curses.KEY_DOWN, ord("j")):
                cursor = min(len(view) - 1, cursor + 1) if view else 0
            elif ch in (curses.KEY_RIGHT, ord("l")):
                if view:
                    expanded.add(cursor)
            elif ch in (curses.KEY_LEFT, ord("h")):
                expanded.discard(cursor)
            elif ch in (curses.KEY_ENTER, 10, 13):
                return view[cursor] if view else None
            elif ch == ord("/"):
                filt = _prompt_filter(stdscr, filt)
                cursor, top, expanded = 0, 0, set()

    def _prompt_filter(stdscr, current):
        curses.curs_set(1)
        curses.echo()
        h, w = stdscr.getmaxyx()
        stdscr.addnstr(h - 1, 0, " search: ".ljust(w)[: w - 1], w - 1, curses.A_REVERSE)
        stdscr.move(h - 1, 9)
        try:
            s = stdscr.getstr(h - 1, 9, max(1, w - 11)).decode("utf-8", "ignore")
        except Exception:
            s = ""
        curses.noecho()
        curses.curs_set(0)
        return s.strip()

    return curses.wrapper(app)


def _interactive_ok():
    return sys.stdin.isatty() and sys.stdout.isatty()


def cmd_browse(args):
    entries = newest_first(load_entries(resolve_store(args.store)))
    if not entries:
        print("no prompts logged yet — `prompt-log add \"<your prompt>\"` to start.")
        return 0
    if not _interactive_ok():
        # Graceful fallback when there is no TTY (piped, or an agent's non-interactive shell).
        print("interactive browse needs a real terminal; here is the stack (newest first):\n")
        _print_list(entries)
        print("\nreuse one with:  prompt-log get <number> --copy")
        return 0
    chosen = _run_curses(entries)
    if chosen:
        _reuse(chosen, args.no_copy)
    return 0


def cmd_pick(args):
    entries = newest_first(load_entries(resolve_store(args.store)))
    matches = filter_entries(entries, " ".join(args.query))
    if not matches:
        print(f"no prompts match: {' '.join(args.query)!r}")
        return 0
    if not _interactive_ok():
        print(f"{len(matches)} match(es) for {' '.join(args.query)!r} (newest first):\n")
        _print_list(matches)
        print("\nreuse one with:  prompt-log get <number> --copy")
        return 0
    chosen = _run_curses(matches)
    if chosen:
        _reuse(chosen, args.no_copy)
    return 0


# ----------------------------------------------------------------------------- self-test

def cmd_selftest(args):
    """Exercise the data layer end-to-end in a temp store (no TTY needed)."""
    import tempfile
    failures = []

    def check(cond, msg):
        if not cond:
            failures.append(msg)

    with tempfile.TemporaryDirectory() as d:
        store = os.path.join(d, "t.jsonl")
        # add (positional + stdin-like via --text) + label derivation
        for txt, lbl in [("Refactor the auth module\nwith TDD", None),
                         ("Investigate the flaky test", "flaky-investigation"),
                         ("Refactor the billing pipeline", None)]:
            append_entry(store, {
                "id": "p" + uuid.uuid4().hex, "ts": _now_iso(),
                "label": _derive_label(txt, lbl), "text": txt, "tags": [],
            })
        entries = newest_first(load_entries(store))
        check(len(entries) == 3, f"expected 3 entries, got {len(entries)}")
        check(entries[0]["text"].startswith("Refactor the billing"), "newest-first order wrong")
        check(_derive_label("# Heading\nbody") == "Heading", "label should strip markdown heading")
        check(entries[1]["label"] == "flaky-investigation", "explicit label not honored")
        # search AND semantics
        m = filter_entries(entries, "refactor")
        check(len(m) == 2, f"search 'refactor' expected 2, got {len(m)}")
        m2 = filter_entries(entries, "refactor billing")
        check(len(m2) == 1, f"search 'refactor billing' expected 1, got {len(m2)}")
        check(filter_entries(entries, "nonexistentxyz") == [], "no-match search should be empty")
        # resolve by index and id-prefix
        check(resolve_one(entries, "1") is entries[0], "index resolve failed")
        idp = entries[2]["id"][:8]
        check(resolve_one(entries, idp) is entries[2], "id-prefix resolve failed")
        check(resolve_one(entries, "999") is None, "out-of-range index should be None")
        # legacy-store privacy guard: a .aiforward store still gets a `*` .gitignore
        _ensure_store_dir(os.path.join(d, ".aiforward", "x.jsonl"))
        check(os.path.exists(os.path.join(d, ".aiforward", ".gitignore")), ".gitignore not created")
        # adapter: audit-log schema (prompt/shortname/datetime) maps to the stack shape; empty prompt skipped
        astore = os.path.join(d, "audit.jsonl")
        append_entry(astore, {"id": "al-0001", "shortname": "design-gw", "datetime": "2026-06-27T10:00:00Z",
                              "prompt": "Design the payment gateway", "summary": "x", "kind": "skill"})
        append_entry(astore, {"id": "al-0002", "shortname": "noop", "datetime": "2026-06-27T10:01:00Z",
                              "prompt": "", "summary": "y", "kind": "manual"})
        ad = newest_first(load_entries(astore))
        check(len(ad) == 1, f"adapter: expected 1 non-empty audit entry, got {len(ad)}")
        check(bool(ad) and ad[0]["text"] == "Design the payment gateway", "adapter: prompt should map to text")
        check(bool(ad) and ad[0]["label"] == "design-gw", "adapter: shortname should map to label")

    if failures:
        print("SELF-TEST FAILED:")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("prompt-log self-test: OK (add, newest-first, label, search AND, resolve, gitignore, audit-adapter)")
    return 0


# ----------------------------------------------------------------------------- cli

def build_parser():
    p = argparse.ArgumentParser(
        prog="prompt-log",
        description="A project-local log of prompts you can browse, search, and reuse.")
    p.add_argument("--store", help=f"path to the JSONL store (default: the unified audit log "
                                   f"<repo>/{AUDIT_REL}, or ${ENV_STORE}; pass a legacy "
                                   f"<repo>/{LEGACY_DIRNAME}/{LEGACY_FILENAME} to read/append that)")
    sub = p.add_subparsers(dest="cmd")

    a = sub.add_parser("add", help="log a prompt (to the audit log by default)")
    a.add_argument("words", nargs="*", help="the prompt text (or use --text, or pipe via stdin)")
    a.add_argument("--text", help="the prompt text")
    a.add_argument("--file", help="read the prompt text from a UTF-8 file (the safe path for "
                                  "non-ASCII under a Windows console)")
    a.add_argument("--label", help="a short label / shortname (default: derived from the first line)")
    a.add_argument("--session", help="the session id to record on the audit entry (default: prompt-log)")
    a.add_argument("--summary", help="the audit summary (default: 'prompt logged for reuse')")
    a.add_argument("--tag", action="append", help="a tag (repeatable)")
    a.add_argument("--quiet", action="store_true", help="don't echo the logged line")
    a.set_defaults(func=cmd_add)

    l = sub.add_parser("list", help="show the stack, newest first")
    l.add_argument("--limit", type=int, default=30, help="max entries to show (0 = all)")
    l.add_argument("--raw", metavar="AL-ID", help="show one raw prompt and its compilations only (P7)")
    l.add_argument("--json", action="store_true", help="emit JSON")
    l.set_defaults(func=cmd_list)

    s = sub.add_parser("search", help="freeform search (matches contain ALL terms)")
    s.add_argument("query", nargs="+", help="search terms")
    s.add_argument("--raw", metavar="AL-ID", help="search within one raw prompt and its compilations only (P7)")
    s.add_argument("--json", action="store_true", help="emit JSON")
    s.set_defaults(func=cmd_search)

    sh = sub.add_parser("show", help="print one entry in full")
    sh.add_argument("ref", help="a 1-based newest-first index, or an id prefix")
    sh.set_defaults(func=cmd_show)

    g = sub.add_parser("get", help="print one entry's RAW text (for piping/copying)")
    g.add_argument("ref", help="a 1-based newest-first index, or an id prefix")
    g.add_argument("--copy", action="store_true", help="also copy to the clipboard")
    g.set_defaults(func=cmd_get)

    b = sub.add_parser("browse", help="interactive stack (↑/↓ move, → expand, ← collapse, Enter reuse)")
    b.add_argument("--no-copy", action="store_true", help="don't copy the chosen prompt to the clipboard")
    b.set_defaults(func=cmd_browse)

    pk = sub.add_parser("pick", help="interactive stack pre-filtered by a query (powers /searchprompts)")
    pk.add_argument("query", nargs="+", help="search terms")
    pk.add_argument("--no-copy", action="store_true", help="don't copy the chosen prompt to the clipboard")
    pk.set_defaults(func=cmd_pick)

    sub.add_parser("self-test", help="exercise the data layer (no TTY needed)").set_defaults(func=cmd_selftest)
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    if not getattr(args, "func", None):
        build_parser().print_help()
        return 0
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
