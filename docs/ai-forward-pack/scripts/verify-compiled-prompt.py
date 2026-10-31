#!/usr/bin/env python3
"""verify-compiled-prompt.py - the compile-stage gate: a compiled prompt never adds scope.

THE CLASS (spec US-4, design docs/design/compile-stage.md). A model step turns the operator's
prose into a goal state. The measured failure is that it ADDS scope: a *Done when* clause the
operator never asked for, a phrase "quoted" from the prompt that is not in it, or a belief
laundered into a clause through an assumption nobody was asked about. This gate refuses each of
those shapes deterministically, against the raw text the audit log holds, before anything is
logged or rendered.

WHAT IT CHECKS, IN ORDER (each refusal `<code>: <target> - fix: <text>` on stderr, exit 1)
  field missing: <name>          the seven goal_state fields are non-empty ("NOT COMPILED" counts)
  raw mismatch: <raw_id>         sha256(raw text) != raw_sha256
  assumption incomplete: #<n>    belief / confirm / breaks all non-empty
  added scope: <clause>          a done_when / not_in_scope clause with no trace
  invalid trace: <clause>        a phrase trace that is not a verbatim substring of the raw text
                                 (whitespace runs collapsed, line endings normalised, CASE-SENSITIVE),
                                 or an assume trace naming an id that is not in assumptions
  decision request missing: #<n> a clause whose only trace is an assumption needs that assumption
                                 consequential: true AND a decision request referencing it
  pass-through refused: <x>      pass-through mode: the same minus the trace checks
  raw not found: <raw_id>        (CLI) the raw id is not a kind:prompt entry in the audit log
In "not-compiled" mode the trace checks are skipped; field and hash checks still run.

USAGE
  python3 verify-compiled-prompt.py verify <compiled.json> [--audit-root <docs dir>]
  python3 verify-compiled-prompt.py --self-test      the nine directions in a temp dir
  python3 verify-compiled-prompt.py                  the same (run-verify-gates.py's argument-free form)

EXIT  0 pass  ·  1 refused  ·  2 usage.  Stdlib only.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile

# Windows consoles default to cp1252, which cannot encode the em dash in every refusal line
# (class PLAT-A): the guard is applied uniformly so a refusal is never itself a crash.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

GOAL_FIELDS = ("goal", "done_when", "not_in_scope", "tier", "fan_out_cap",
               "context_ceiling", "main_line_budget")
SECTIONS = ("done_when", "not_in_scope")
MODES = ("compiled", "pass-through", "not-compiled")
NOT_COMPILED = "NOT COMPILED"
REFUSAL_RE = re.compile(r"^[a-z][a-z -]*: .+ — fix: .+$")
_WS = re.compile(r"\s+")


def refusal(code: str, target: str, fix: str) -> str:
    """The one refusal grammar: `<code>: <target> — fix: <text>` on a single line."""
    target = _WS.sub(" ", str(target)).strip() or "?"
    return f"{code}: {target} — fix: {fix}"


def collapse(text: str) -> str:
    """Line endings normalised, every whitespace run one space; case untouched."""
    return _WS.sub(" ", text.replace("\r\n", "\n").replace("\r", "\n")).strip()


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def _empty(value) -> bool:
    if value is None:
        return True
    if isinstance(value, str):
        return not value.strip()
    if isinstance(value, (list, dict)):
        return len(value) == 0
    return False


def _repo_root(start: str | None = None) -> str:
    d = os.path.abspath(start or os.getcwd())
    while True:
        if os.path.exists(os.path.join(d, ".git")):   # a dir, or a linked worktree's pointer FILE (WT-A)
            return d
        parent = os.path.dirname(d)
        if parent == d:
            return os.path.abspath(start or os.getcwd())
        d = parent


def load_raw(audit_root: str, raw_id: str) -> dict | None:
    """The kind:prompt entry with this id from <audit_root>/audit/audit-log.jsonl, else None."""
    path = os.path.join(audit_root, "audit", "audit-log.jsonl")
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                continue
            if entry.get("id") == raw_id and entry.get("kind") == "prompt":
                return entry
    return None


def verify_document(doc: dict, raw_text: str) -> list[str]:
    """Every refusal for this compiled document against the raw text, in the fixed order."""
    mode = doc.get("mode")
    pass_through = mode == "pass-through"
    trace_checks = mode == "compiled"
    out: list[str] = []

    def add(code: str, target: str, fix: str) -> None:
        if pass_through:
            out.append(refusal("pass-through refused", target, f"{code}: {fix}"))
        else:
            out.append(refusal(code, target, fix))

    goal_state = doc.get("goal_state") or {}
    for name in GOAL_FIELDS:
        if _empty(goal_state.get(name)):
            add("field missing", name, "fill the field, or write NOT COMPILED in every field with --no-model")
    if sha256_text(raw_text) != doc.get("raw_sha256"):
        add("raw mismatch", str(doc.get("raw_id")), "re-run skeleton against the raw prompt; never edit the raw text")

    assumptions = {a.get("id"): a for a in (doc.get("assumptions") or []) if isinstance(a, dict)}
    for aid, a in assumptions.items():
        if any(_empty(a.get(k)) for k in ("belief", "confirm", "breaks")):
            add("assumption incomplete", str(aid), "state belief, what would confirm it, and what breaks if it is false")
    if not trace_checks:
        return out

    raw_norm = collapse(raw_text)
    dr_refs = {d.get("assumption") for d in (doc.get("decision_requests") or []) if isinstance(d, dict)}
    for clause in doc.get("clauses") or []:
        if clause.get("section") not in SECTIONS:
            continue
        text = str(clause.get("text") or "")
        trace = clause.get("trace")
        if not isinstance(trace, dict) or _empty(trace.get("ref")):
            out.append(refusal("added scope", text, "trace the clause to a verbatim phrase of the raw prompt or drop it"))
            continue
        ref = str(trace.get("ref"))
        if trace.get("kind") == "phrase":
            if collapse(ref) not in raw_norm:
                out.append(refusal("invalid trace", text, "quote the raw prompt verbatim (case-sensitive), or trace to an assumption"))
        elif trace.get("kind") == "assume":
            if ref not in assumptions:
                out.append(refusal("invalid trace", text, f"assumption {ref} is not in assumptions"))
            elif not (assumptions[ref].get("consequential") is True and ref in dr_refs):
                out.append(refusal("decision request missing", ref,
                                   "an assumption that carries a clause is consequential: mark it and add a DR-n referencing it"))
        else:
            out.append(refusal("invalid trace", text, "trace.kind must be phrase or assume"))
    return out


def trace_table(doc: dict) -> str:
    rows = ["| section | clause | trace |", "|---|---|---|"]
    for clause in doc.get("clauses") or []:
        trace = clause.get("trace") or {}
        shown = f"{trace.get('kind', '?')}: {trace.get('ref', '')}" if trace else "-"
        rows.append(f"| {clause.get('section')} | {clause.get('text')} | {shown} |")
    return "\n".join(rows)


def verify_file(path: str, audit_root: str) -> tuple[list[str], dict | None]:
    with open(path, encoding="utf-8") as handle:
        doc = json.load(handle)
    raw = load_raw(audit_root, str(doc.get("raw_id")))
    if raw is None:
        return [refusal("raw not found", str(doc.get("raw_id")),
                        "the id must be a kind:prompt entry in the audit log")], doc
    return verify_document(doc, str(raw.get("prompt") or "")), doc


# ----------------------------------------------------------------------------- self-test

_RAW = ("Add a --dry-run flag to sync-pack.ps1. Don't touch the install counts.\n"
        "Done when the flag is documented in README.\n")


def _base_doc() -> dict:
    return {
        "schema": "compiled-prompt/1", "raw_id": "al-0001", "raw_sha256": sha256_text(_RAW),
        "raw_text_normalised": False, "harness": "claude-code", "template": "claude-code",
        "template_version": 1, "mode": "compiled",
        "goal_state": {"goal": "Add a --dry-run flag to sync-pack.ps1",
                       "done_when": ["the flag is documented in README"],
                       "not_in_scope": ["the install counts"], "tier": "T1", "fan_out_cap": 0,
                       "context_ceiling": 400000, "main_line_budget": 40},
        "clauses": [
            {"section": "done_when", "text": "the flag is documented in README",
             "trace": {"kind": "phrase", "ref": "the flag is documented in README"}},
            {"section": "not_in_scope", "text": "the install counts",
             "trace": {"kind": "phrase", "ref": "Don't touch the install counts"}}],
        "references": [], "graph_neighbours": [],
        "assumptions": [{"id": "#1", "belief": "no dry-run flag exists", "confirm": "grep",
                         "breaks": "added twice", "consequential": False}],
        "decision_requests": [], "contract_slot": {}, "dispatchable": True,
        "provenance": {"engine_seconds": None, "compiler_model": None, "compile_tokens": None,
                       "refusals": [], "retries": 0},
    }


def _directions() -> list[tuple[str, dict, str | None]]:
    """(name, document, expected refusal code or None for accepted)."""
    out = []
    d = _base_doc(); d["clauses"].append({"section": "done_when", "text": "CI is green", "trace": None})
    out.append(("added done_when clause refused", d, "added scope"))
    d = _base_doc(); d["clauses"].append({"section": "not_in_scope", "text": "the README", "trace": None})
    out.append(("added not_in_scope clause refused", d, "added scope"))
    d = _base_doc(); d["clauses"][0]["trace"]["ref"] = "the flag is documented in CHANGELOG"
    out.append(("trace to a phrase absent from raw refused", d, "invalid trace"))
    d = _base_doc(); d["goal_state"]["goal"] = ""
    out.append(("missing field refused", d, "field missing"))
    d = _base_doc(); d["raw_sha256"] = "0" * 64
    out.append(("raw hash mismatch refused", d, "raw mismatch"))
    out.append(("complete marker accepted", _base_doc(), None))
    d = _base_doc(); d["assumptions"][0]["breaks"] = ""
    out.append(("incomplete marker refused", d, "assumption incomplete"))
    d = _base_doc(); d["clauses"][0]["trace"] = {"kind": "assume", "ref": "#9"}
    out.append(("trace to a missing assumption id refused", d, "invalid trace"))
    d = _base_doc(); d["clauses"][0]["trace"] = {"kind": "assume", "ref": "#1"}
    out.append(("assumption-only trace without consequential+DR refused", d, "decision request missing"))
    return out


def self_test_refusals() -> list[str]:
    """Every refusal the nine directions produce (for the grammar assertion)."""
    out: list[str] = []
    for _, doc, _ in _directions():
        out.extend(verify_document(doc, _RAW))
    return out


def self_test() -> int:
    ok = True
    with tempfile.TemporaryDirectory() as tmp:
        audit_root = os.path.join(tmp, "docs")
        os.makedirs(os.path.join(audit_root, "audit"))
        with open(os.path.join(audit_root, "audit", "audit-log.jsonl"), "w", encoding="utf-8", newline="\n") as handle:
            handle.write(json.dumps({"id": "al-0001", "kind": "prompt", "prompt": _RAW}) + "\n")
        for n, (name, doc, expected) in enumerate(_directions(), 1):
            path = os.path.join(tmp, f"d{n}.json")
            with open(path, "w", encoding="utf-8", newline="\n") as handle:
                json.dump(doc, handle)
            refusals, _ = verify_file(path, audit_root)
            codes = [r.split(": ", 1)[0] for r in refusals]
            grammar = all(REFUSAL_RE.match(r) for r in refusals)
            behaved = grammar and ((expected is None and not refusals) or (expected in codes))
            ok = ok and behaved
            print(f"direction {n}: {name} - {'ok' if behaved else 'FAILED ' + repr(refusals)}")
    print("self-test OK: nine directions behave; every refusal matches the grammar" if ok
          else "self-test FAILED")
    return 0 if ok else 1


# ----------------------------------------------------------------------------- CLI

def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--self-test", action="store_true")
    sub = parser.add_subparsers(dest="verb")
    v = sub.add_parser("verify", help="gate one compiled prompt JSON")
    v.add_argument("compiled")
    v.add_argument("--audit-root", help="the docs dir holding audit/audit-log.jsonl (default <repo>/docs)")
    args = parser.parse_args(argv)
    # run-verify-gates.py runs every verify-*.py argument-free and counts a non-zero exit as a
    # failed gate: the bare form IS the self-test (Coordinator seam, 2026-09-19), never usage.
    if args.self_test or args.verb is None:
        return self_test()
    if not os.path.isfile(args.compiled):
        sys.stderr.write(f"usage: {args.compiled} — fix: pass an existing compiled JSON file\n")
        return 2
    audit_root = os.path.abspath(args.audit_root) if args.audit_root else os.path.join(_repo_root(), "docs")
    try:
        refusals, doc = verify_file(args.compiled, audit_root)
    except (json.JSONDecodeError, OSError) as exc:
        sys.stderr.write(f"usage: {args.compiled} — fix: not readable JSON ({exc})\n")
        return 2
    if refusals:
        for r in refusals:
            sys.stderr.write(r + "\n")
        return 1
    print(trace_table(doc or {}))
    print("gate: pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
