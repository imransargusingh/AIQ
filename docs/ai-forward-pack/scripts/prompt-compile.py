#!/usr/bin/env python3
"""prompt-compile.py - the compile stage: a logged raw prompt -> a gated, harness-rendered prompt.

THE SHAPE (design docs/design/compile-stage.md): a deterministic SKELETON (references resolved
under the repo root and hashed, pass-through detected by a fixed grammar, the harness template
loaded from the registry) -> the running agent FILLS the model-only fields in the JSON ->
FINISH runs the gate (verify-compiled-prompt.py, imported by path), renders the harness idiom,
appends ONE `kind: compilation` audit entry and copies the text to the clipboard. Nothing is
logged on a refusal; `finish` is the only writer and writes last.

VERBS
  skeleton  --text "<raw>" | --text-file <path> | --from-audit <al-id>  --harness <name> --out <path.json>
            [--no-model] [--audit-root <docs dir>] [--templates-dir <dir>]
  finish    <compiled.json> [--harness <name>] --session <id> [--compiler-model <name>]
            [--compile-tokens <n>] [--no-clipboard] [--audit-root <docs dir>] [--templates-dir <dir>]
  render    <compiled.json> --harness <name> [--templates-dir <dir>]  |  render --self-test
  distance  --compiled <al-id> --received-file <path> [--audit-root <docs dir>]

REFUSALS (stderr, `<code>: <target> - fix: <text>`, exit 1): empty prompt · raw not found ·
template missing · template ambiguous · forbidden construct · and every gate code. Exit 2 is usage
(a malformed compiled JSON names the offending key). Stdlib only; UTF-8 on every seam.
"""
from __future__ import annotations

import argparse
import difflib
import hashlib
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
import time

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

SCHEMA = "compiled-prompt/1"
MODES = ("compiled", "pass-through", "not-compiled")
NOT_COMPILED = "NOT COMPILED"
GOAL_FIELDS = ("goal", "done_when", "not_in_scope", "tier", "fan_out_cap", "context_ceiling", "main_line_budget")
GOAL_LABELS = ("Goal:", "Done when:", "Not in scope:", "Tier:", "Fan-out cap:", "Context ceiling:", "Main-line budget:")
CONTRACT_KEYS = ("width_cap", "transient_retry", "per_branch_exit", "join_rule", "containment",
                 "termination", "deadline", "fallback")
PLACEHOLDERS = ("goal_state", "trace", "references", "assumptions", "decision_requests", "contract_slot", "provenance")
PATH_EXT = (".py", ".md", ".json", ".yml", ".yaml", ".ps1", ".js", ".html", ".txt")
SKIP_DIRS = {".git", "node_modules", "__pycache__", ".venv", "venv"}
_BACKTICK = re.compile(r"`([^`\n]+)`")
_KEBAB = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)+$")
_STRIP = ".,;:!?)(\"'<>[]"


class Refusal(Exception):
    def __init__(self, code: str, target: str, fix: str):
        super().__init__(f"{code}: {target} — fix: {fix}")
        self.code, self.target, self.fix = code, target, fix


# ----------------------------------------------------------------------------- locations

def _here() -> str:
    return os.path.dirname(os.path.abspath(__file__))


def _sibling(name: str) -> str:
    return os.path.join(_here(), name)


def _repo_root(start: str | None = None) -> str:
    d = os.path.abspath(start or os.getcwd())
    while True:
        if os.path.exists(os.path.join(d, ".git")):   # a dir, or a linked worktree's pointer FILE (WT-A)
            return d
        parent = os.path.dirname(d)
        if parent == d:
            return os.path.abspath(start or os.getcwd())
        d = parent


def default_templates_dir() -> str:
    # One relative path resolves in both layouts: pack/scripts -> pack/templates and
    # docs/ai-forward-pack/scripts -> docs/ai-forward-pack/templates (Coordinator seam).
    return os.path.normpath(os.path.join(_here(), "..", "templates", "prompt-templates"))


def _load_by_path(name: str, filename: str):
    spec = importlib.util.spec_from_file_location(name, _sibling(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _gate():
    return _load_by_path("verify_compiled_prompt", "verify-compiled-prompt.py")


def copy_to_clipboard(text: str):
    """prompt-log.py's ladder, reused by import (pbcopy · xclip · wl-copy · clip.exe UTF-16LE)."""
    return _load_by_path("prompt_log", "prompt-log.py").copy_to_clipboard(text)


# ----------------------------------------------------------------------------- audit log

def _audit_path(audit_root: str) -> str:
    return os.path.join(audit_root, "audit", "audit-log.jsonl")


def read_entries(audit_root: str) -> list[dict]:
    path = _audit_path(audit_root)
    if not os.path.exists(path):
        return []
    out = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return out


def find_entry(audit_root: str, entry_id: str, kind: str) -> dict | None:
    for entry in read_entries(audit_root):
        if entry.get("id") == entry_id and entry.get("kind") == kind:
            return entry
    return None


def log_raw_prompt(audit_root: str, text: str) -> dict:
    """Log the raw text as a kind:prompt entry through prompt-log.py add and return the entry.

    prompt-log.py prints the label, not the id, so the id is read back from the log: the newest
    kind:prompt entry whose text is this text."""
    repo_root = os.path.dirname(os.path.abspath(audit_root))
    # The seam is UTF-8 by construction on both ends (prompt-log.py's own idiom): the text goes
    # through a UTF-8 file, never argv, and the child's console pipe is told the same encoding.
    env = dict(os.environ)
    env["PYTHONIOENCODING"] = "utf-8"
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", newline="\n", suffix=".txt", delete=False) as handle:
        handle.write(text)
        tmp = handle.name
    try:
        cmd = [sys.executable, _sibling("prompt-log.py"), "add", "--file", tmp, "--quiet",
               "--session", "prompt-compile", "--summary", "raw prompt logged for compilation"]
        r = subprocess.run(cmd, cwd=repo_root, capture_output=True, text=True, encoding="utf-8", env=env, check=False)
    finally:
        os.unlink(tmp)
    if r.returncode != 0:
        raise RuntimeError(f"prompt-log.py add failed (exit {r.returncode}): {r.stderr.strip()[:200]}")
    for entry in reversed(read_entries(audit_root)):
        if entry.get("kind") == "prompt" and (entry.get("prompt") or "").strip() == text.strip():
            return entry
    raise Refusal("raw not found", "(unlogged)", "prompt-log.py add wrote no matching kind:prompt entry")


def append_compilation_entry(audit_root: str, entry: dict) -> str:
    """The single write: audit-log.py append --kind compilation --from-json <tmp>. Returns the id."""
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", newline="\n", suffix=".json", delete=False) as handle:
        json.dump(entry, handle, ensure_ascii=False)
        tmp = handle.name
    try:
        r = subprocess.run([sys.executable, _sibling("audit-log.py"), "--root", audit_root, "append",
                            "--kind", "compilation", "--from-json", tmp],
                           capture_output=True, text=True, encoding="utf-8", check=False)
    finally:
        os.unlink(tmp)
    if r.returncode != 0:
        # Not a refusal code: the compile passed and the store refused the write. No plausible
        # code, no partial entry - the failure propagates as what it is.
        raise RuntimeError(f"audit-log.py append failed (exit {r.returncode}): {r.stderr.strip()[:200]}")
    return r.stdout.strip().splitlines()[-1] if r.stdout.strip() else ""


# ----------------------------------------------------------------------------- pass-through

def _split_items(value: str) -> list[str]:
    parts = [p.strip() for p in value.split(";")] if ";" in value else [value.strip()]
    return [p for p in parts if p]


def _int_or_text(value: str):
    cleaned = value.strip().replace(",", "").replace("_", "")
    return int(cleaned) if cleaned.isdigit() else value.strip()


def detect_pass_through(raw: str) -> dict | None:
    """The fixed grammar: the seven labels line-initial, in order, each once; else None."""
    lines = raw.replace("\r\n", "\n").replace("\r", "\n").split("\n")
    found: list[tuple[int, int]] = []   # (label index, line index)
    for i, line in enumerate(lines):
        stripped = line.lstrip()
        for li, label in enumerate(GOAL_LABELS):
            if stripped.startswith(label):
                found.append((li, i))
                break
    if [li for li, _ in found] != list(range(len(GOAL_LABELS))):
        return None
    values: dict[str, str] = {}
    for n, (li, i) in enumerate(found):
        end = found[n + 1][1] if n + 1 < len(found) else len(lines)
        head = lines[i].lstrip()[len(GOAL_LABELS[li]):]
        body = [head.strip()] + [l.strip().lstrip("-*").strip() for l in lines[i + 1:end] if l.strip()]
        values[GOAL_FIELDS[li]] = "\n".join(b for b in body if b)
    if any(not v.strip() for v in values.values()):
        return None
    goal_state = {
        "goal": " ".join(values["goal"].split()),
        "done_when": [x for line in values["done_when"].split("\n") for x in _split_items(line)],
        "not_in_scope": [x for line in values["not_in_scope"].split("\n") for x in _split_items(line)],
        "tier": values["tier"].strip(),
        "fan_out_cap": _int_or_text(values["fan_out_cap"]),
        "context_ceiling": _int_or_text(values["context_ceiling"]),
        "main_line_budget": _int_or_text(values["main_line_budget"]),
    }
    return goal_state


# ----------------------------------------------------------------------------- references

def _walk_files(root: str) -> list[str]:
    out = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if d not in SKIP_DIRS)
        for name in sorted(filenames):
            out.append(os.path.relpath(os.path.join(dirpath, name), root).replace(os.sep, "/"))
    return out


def _frontmatter(path: str) -> dict:
    """`id:` and `links[].to` from a docs artifact's YAML frontmatter (the pack's V2 shape)."""
    try:
        with open(path, encoding="utf-8", errors="replace") as handle:
            head = handle.read(8192)
    except OSError:
        return {}
    if not head.startswith("---"):
        return {}
    block = head.split("\n---", 1)[0]
    ident = None
    links = []
    for line in block.split("\n"):
        m = re.match(r"^id:\s*['\"]?([A-Za-z0-9._-]+)['\"]?\s*$", line)
        if m:
            ident = m.group(1)
        m = re.search(r"\{\s*to:\s*['\"]?([A-Za-z0-9._-]+)['\"]?\s*,", line)
        if m:
            links.append(m.group(1))
    return {"id": ident, "links": links} if ident else {}


def docs_graph(root: str, files: list[str]) -> dict[str, dict]:
    graph: dict[str, dict] = {}
    for rel in files:
        if rel.startswith("docs/") and rel.endswith(".md"):
            fm = _frontmatter(os.path.join(root, rel))
            if fm and fm["id"] not in graph:
                graph[fm["id"]] = {"path": rel, "links": fm["links"]}
    return graph


def _is_pathlike(token: str) -> bool:
    return "/" in token or token.lower().endswith(PATH_EXT)


def extract_tokens(raw: str, graph_ids: set[str]) -> list[str]:
    """Backticked names, path-like tokens and docs-graph ids, in first-appearance order."""
    seen: list[str] = []

    def add(tok: str) -> None:
        # trailing punctuation is prose; a leading "../" is the token (and is refused later)
        tok = tok.strip().rstrip(_STRIP).lstrip("(\"'<[`")
        if tok and tok not in seen:
            seen.append(tok)

    for m in _BACKTICK.finditer(raw):
        add(m.group(1))
    for word in raw.split():
        w = word.rstrip(_STRIP + "`").lstrip("(\"'<[`")
        if not w:
            continue
        if _is_pathlike(w) or (_KEBAB.match(w) and w in graph_ids):
            add(w)
    return seen


def _levenshtein(a: str, b: str) -> int:
    if abs(len(a) - len(b)) > 2:
        return 3
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]


def _sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def resolve_reference(token: str, root: str, files: list[str], graph: dict[str, dict]) -> dict:
    """Exactly one match -> resolved {path, sha256}; else unresolved with a reason; never a read outside root."""
    ref = {"token": token, "status": "unresolved", "path": None, "sha256": None, "reason": None, "nearest": None}
    real_root = os.path.realpath(root)

    def inside(rel_or_abs: str) -> bool:
        real = os.path.realpath(rel_or_abs if os.path.isabs(rel_or_abs) else os.path.join(root, rel_or_abs))
        return real == real_root or real.startswith(real_root + os.sep)

    if token in graph:
        candidates = [graph[token]["path"]]
    elif os.path.isabs(token) or token.startswith(("../", "..\\")) or "/../" in token:
        ref["reason"] = "outside repo"
        return ref
    else:
        norm = token.replace("\\", "/").lstrip("./")
        candidates = [f for f in files if f == norm or f.endswith("/" + norm)]
    if len(candidates) > 1:
        ref["reason"] = f"ambiguous: {len(candidates)} matches"
        return ref
    if not candidates:
        base = os.path.basename(token)
        near = sorted({f for f in files if _levenshtein(os.path.basename(f).lower(), base.lower()) <= 2})
        ref["reason"] = "not found"
        if len(near) == 1:
            ref["nearest"] = near[0]
        return ref
    path = candidates[0]
    if not inside(path):
        ref["reason"] = "outside repo"
        return ref
    ref.update(status="resolved", path=path, sha256=_sha256_file(os.path.join(root, path)))
    return ref


# ----------------------------------------------------------------------------- templates

def _parse_frontmatter_value(value: str):
    value = value.strip()
    if value.startswith("["):
        try:
            return json.loads(value)
        except json.JSONDecodeError:
            return [v.strip().strip("'\"") for v in value.strip("[]").split(",") if v.strip()]
    if value.lower() in ("true", "false"):
        return value.lower() == "true"
    if value.isdigit():
        return int(value)
    return value.strip("'\"")


def read_template(path: str) -> dict | None:
    with open(path, encoding="utf-8") as handle:
        text = handle.read()
    if not text.startswith("---\n"):
        return None
    head, _, body = text[4:].partition("\n---\n")
    meta = {}
    for line in head.split("\n"):
        if ":" in line:
            key, _, value = line.partition(":")
            meta[key.strip()] = _parse_frontmatter_value(value)
    if not meta.get("harness"):
        return None
    return {"harness": meta["harness"], "version": meta.get("version"), "current": meta.get("current") is True,
            "forbids": meta.get("forbids") or [], "body": body, "path": path}


def load_template(templates_dir: str, harness: str) -> dict:
    installed: list[dict] = []
    if os.path.isdir(templates_dir):
        for name in sorted(os.listdir(templates_dir)):
            if name.endswith(".md"):
                tmpl = read_template(os.path.join(templates_dir, name))
                if tmpl:
                    installed.append(tmpl)
    current = [t for t in installed if t["harness"] == harness and t["current"]]
    if len(current) > 1:
        raise Refusal("template ambiguous", harness,
                      "exactly one template may carry current: true (" + ", ".join(os.path.basename(t["path"]) for t in current) + ")")
    if not current:
        names = sorted({t["harness"] for t in installed if t["current"]}) or ["none"]
        raise Refusal("template missing", harness, f"installed current templates: {', '.join(names)}")
    return current[0]


# ----------------------------------------------------------------------------- skeleton

def build_skeleton(raw_text: str, raw_id: str, harness: str, root: str, templates_dir: str, no_model: bool) -> dict:
    template = load_template(templates_dir, harness)
    files = _walk_files(root)
    graph = docs_graph(root, files)
    references = [resolve_reference(tok, root, files, graph) for tok in extract_tokens(raw_text, set(graph))]
    neighbours = sorted({n for r in references if r["status"] == "resolved" and r["token"] in graph
                         for n in graph[r["token"]]["links"]})
    goal_state = detect_pass_through(raw_text)
    if goal_state is not None:
        mode = "pass-through"
        clauses = [{"section": section, "text": text, "trace": {"kind": "phrase", "ref": text}}
                   for section in ("done_when", "not_in_scope") for text in goal_state[section]]
        assumptions, decision_requests = [], []
    elif no_model:
        mode = "not-compiled"
        goal_state = {k: NOT_COMPILED for k in GOAL_FIELDS}
        clauses, assumptions, decision_requests = [], [], []
    else:
        mode = "compiled"
        goal_state = {k: None for k in GOAL_FIELDS}
        clauses = assumptions = decision_requests = None
    return {
        "schema": SCHEMA, "raw_id": raw_id, "raw_sha256": hashlib.sha256(raw_text.encode("utf-8")).hexdigest(),
        "raw_text_normalised": False, "harness": harness, "template": template["harness"],
        "template_version": template["version"], "mode": mode, "goal_state": goal_state, "clauses": clauses,
        "references": references, "graph_neighbours": neighbours, "assumptions": assumptions,
        "decision_requests": decision_requests, "contract_slot": {k: None for k in CONTRACT_KEYS},
        "dispatchable": None if mode == "compiled" else True,
        # engine_seconds is printed, not stored: the skeleton is a golden (byte-identical) artifact
        # and `finish` records the measured engine time of the run it writes.
        "provenance": {"engine_seconds": None, "compiler_model": None, "compile_tokens": None,
                       "refusals": [], "retries": 0},
    }


# ----------------------------------------------------------------------------- schema

def check_schema(doc: dict) -> str | None:
    """The offending key of a malformed compiled document, else None."""
    if not isinstance(doc, dict):
        return "document"
    if doc.get("schema") != SCHEMA:
        return "schema"
    for key in ("raw_id", "raw_sha256", "harness", "template"):
        if not isinstance(doc.get(key), str) or not doc[key]:
            return key
    if not isinstance(doc.get("template_version"), int):
        return "template_version"
    if doc.get("mode") not in MODES:
        return "mode"
    gs = doc.get("goal_state")
    if not isinstance(gs, dict) or any(k not in gs for k in GOAL_FIELDS):
        return "goal_state." + next((k for k in GOAL_FIELDS if not isinstance(gs, dict) or k not in gs), "")
    for key in ("clauses", "references", "graph_neighbours", "assumptions", "decision_requests"):
        if not isinstance(doc.get(key), list):
            return key
    for n, clause in enumerate(doc["clauses"]):
        if not isinstance(clause, dict) or clause.get("section") not in ("done_when", "not_in_scope"):
            return f"clauses[{n}].section"
        trace = clause.get("trace")
        if trace is not None and (not isinstance(trace, dict) or trace.get("kind") not in ("phrase", "assume")):
            return f"clauses[{n}].trace.kind"
    if not isinstance(doc.get("contract_slot"), dict) or not isinstance(doc.get("provenance"), dict):
        return "contract_slot" if not isinstance(doc.get("contract_slot"), dict) else "provenance"
    return None


# ----------------------------------------------------------------------------- render

def _list(value) -> str:
    if isinstance(value, list):
        return "; ".join(str(v) for v in value)
    return str(value)


def render_sections(doc: dict) -> dict[str, str]:
    gs = doc["goal_state"]
    goal_state = "Goal state\n" + "\n".join(f"{label} {_list(gs.get(field))}" for label, field in zip(GOAL_LABELS, GOAL_FIELDS))
    trace_rows = ["Trace", "| clause | trace |", "|---|---|"]
    for clause in doc.get("clauses") or []:
        trace = clause.get("trace") or {}
        shown = (f"#{trace['ref'].lstrip('#')}" if trace.get("kind") == "assume" else f"phrase: {trace.get('ref', '')}") if trace else "-"
        trace_rows.append(f"| {clause.get('section')}: {clause.get('text')} | {shown} |")
    references = ["References"]
    for ref in doc.get("references") or []:
        if ref.get("status") == "resolved":
            references.append(f"- {ref['token']}: {ref['path']} sha256 {ref['sha256']}")
        else:
            near = f"; nearest: {ref['nearest']}" if ref.get("nearest") else ""
            references.append(f"- {ref['token']}: unresolved ({ref.get('reason')}{near})")
    if doc.get("graph_neighbours"):
        references.append("- graph neighbours: " + ", ".join(doc["graph_neighbours"]))
    if len(references) == 1:
        references.append("- none")
    assumptions = ["Assumptions"] + [
        f"- {a.get('id')} belief: {a.get('belief')} · confirm: {a.get('confirm')} · breaks: {a.get('breaks')} · consequential: {str(bool(a.get('consequential'))).lower()}"
        for a in doc.get("assumptions") or []] or ["Assumptions"]
    if len(assumptions) == 1:
        assumptions.append("- none")
    drs = ["Decision requests"] + [
        f"- {d.get('id')} ({d.get('assumption')}): {d.get('question')} · default: {d.get('default')} · answer: {d.get('answer') if d.get('answer') else 'unanswered'}"
        for d in doc.get("decision_requests") or []]
    if len(drs) == 1:
        drs.append("- none")
    slot = ["Contract slot"] + [f"{k}: {doc.get('contract_slot', {}).get(k) if doc.get('contract_slot', {}).get(k) is not None else 'unset'}"
                                for k in CONTRACT_KEYS]
    prov = doc.get("provenance") or {}
    tokens = prov.get("compile_tokens")
    provenance = ["Provenance",
                  f"raw id: {doc.get('raw_id')}", f"raw sha256: {doc.get('raw_sha256')}",
                  f"compiler model: {prov.get('compiler_model') or 'not recorded'}",
                  f"engine seconds: {prov.get('engine_seconds') if prov.get('engine_seconds') is not None else 'not recorded'}",
                  f"tokens: {tokens if tokens is not None else 'not recorded'}",
                  "gate: pass", f"dispatchable: {str(bool(doc.get('dispatchable'))).lower()}"]
    return {"goal_state": goal_state, "trace": "\n".join(trace_rows), "references": "\n".join(references),
            "assumptions": "\n".join(assumptions), "decision_requests": "\n".join(drs),
            "contract_slot": "\n".join(slot), "provenance": "\n".join(provenance)}


def render_document(doc: dict, template: dict, session: str = "<session>", skill: str = "<skill>") -> str:
    text = template["body"]
    # simplify: str.replace over seven named placeholders (+ session/skill); ceiling: seven
    # placeholders, no conditionals; upgrade trigger: a template needs a loop or a conditional.
    for name, value in render_sections(doc).items():
        text = text.replace("{{" + name + "}}", value)
    text = text.replace("{{session}}", session).replace("{{skill}}", skill)
    for token in template.get("forbids") or []:
        if token and token in text:
            raise Refusal("forbidden construct", token, f"the template {os.path.basename(template['path'])} forbids it; remove it from the template or the fill")
    return text


def edit_distance(compiled: str, received: str) -> float:
    def norm(t: str) -> str:
        return t.replace("\r\n", "\n").replace("\r", "\n")
    return round(1.0 - difflib.SequenceMatcher(None, norm(compiled), norm(received)).ratio(), 4)


# ----------------------------------------------------------------------------- verbs

def _audit_root(args) -> str:
    return os.path.abspath(args.audit_root) if getattr(args, "audit_root", None) else os.path.join(_repo_root(), "docs")


def _templates_dir(args) -> str:
    return os.path.abspath(args.templates_dir) if getattr(args, "templates_dir", None) else default_templates_dir()


def _read_doc(path: str) -> dict:
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def cmd_skeleton(args) -> int:
    started = time.perf_counter()
    audit_root = _audit_root(args)
    root = os.path.dirname(audit_root)
    if args.from_audit:
        entry = find_entry(audit_root, args.from_audit, "prompt")
        if entry is None:
            raise Refusal("raw not found", args.from_audit, "pass the id of a kind:prompt entry in docs/audit/audit-log.jsonl")
        raw_text = str(entry.get("prompt") or "")
        raw_id = args.from_audit
    else:
        if args.text_file:
            with open(args.text_file, encoding="utf-8") as handle:
                raw_text = handle.read()
        else:
            raw_text = args.text or ""
        if not raw_text.strip():
            raise Refusal("empty prompt", "(no text)", "pass the operator's prose; nothing was logged")
        load_template(_templates_dir(args), args.harness)   # refuse before logging
        raw_id = str(log_raw_prompt(audit_root, raw_text)["id"])
        raw_text = str(find_entry(audit_root, raw_id, "prompt").get("prompt") or "")
    if not raw_text.strip():
        raise Refusal("empty prompt", raw_id, "the raw entry carries no text")
    doc = build_skeleton(raw_text, raw_id, args.harness, root, _templates_dir(args), args.no_model)
    out = os.path.abspath(args.out)
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    with open(out, "w", encoding="utf-8", newline="\n") as handle:
        json.dump(doc, handle, indent=2, ensure_ascii=False, sort_keys=True)
        handle.write("\n")
    print(out)
    print(f"engine_seconds: {time.perf_counter() - started:.3f}")
    return 0


def _usage(message: str) -> int:
    sys.stderr.write(f"usage: {message}\n")
    return 2


def cmd_finish(args) -> int:
    started = time.perf_counter()
    audit_root = _audit_root(args)
    try:
        doc = _read_doc(args.compiled)
    except (OSError, json.JSONDecodeError) as exc:
        return _usage(f"{args.compiled} — fix: not readable JSON ({exc})")
    bad = check_schema(doc)
    if bad:
        return _usage(f"{args.compiled} malformed at key {bad} — fix: conform to {SCHEMA}")
    harness = args.harness or doc["harness"]
    template = load_template(_templates_dir(args), harness)
    gate = _gate()
    raw = gate.load_raw(audit_root, doc["raw_id"])
    if raw is None:
        raise Refusal("raw not found", doc["raw_id"], "the id must be a kind:prompt entry in the audit log")
    refusals = gate.verify_document(doc, str(raw.get("prompt") or ""))
    if refusals:
        for r in refusals:
            sys.stderr.write(r + "\n")
        return 1
    doc["dispatchable"] = not bool(doc.get("decision_requests"))
    prov = doc.setdefault("provenance", {})
    prov["compiler_model"] = args.compiler_model or prov.get("compiler_model")
    prov["compile_tokens"] = args.compile_tokens if args.compile_tokens is not None else prov.get("compile_tokens")
    prov["engine_seconds"] = round(time.perf_counter() - started, 3)
    rendered = render_document(doc, template, session=args.session, skill=args.skill or "<skill>")
    compiled = {k: v for k, v in doc.items() if k != "raw_text"}
    entry = {
        "kind": "compilation", "shortname": f"compile-{raw.get('shortname') or doc['raw_id']}",
        "session": args.session, "prompt": rendered,
        "summary": (f"compiled {doc['raw_id']} for {harness} v{template['version']}: "
                    f"{len(doc.get('clauses') or [])} clauses, {len(doc.get('assumptions') or [])} assumptions, "
                    f"{len(doc.get('decision_requests') or [])} decision requests"),
        "artifacts": [], "mode": doc["mode"], "dispatchable": doc["dispatchable"], "compiled_from": None,
    }
    if doc["mode"] == "not-compiled":
        entry["compiled"] = False          # the degrade path is a stored flag, the skeleton stays readable
        entry["skeleton"] = compiled
    else:
        entry["compiled"] = compiled
    entry_id = append_compilation_entry(audit_root, entry)
    print(rendered)
    if entry_id:
        print(f"logged: {entry_id}")
    if args.no_clipboard:
        print("clipboard: skipped")
    else:
        tool = copy_to_clipboard(rendered)
        print(f"clipboard: {tool}" if tool else "clipboard: skipped")
    return 0


def render_self_test() -> int:
    doc = None
    body = "---\nharness: t\nversion: 1\ncurrent: true\nforbids: []\n---\n" + "\n".join("{{" + p + "}}" for p in PLACEHOLDERS) + "\n"
    ok = True
    with tempfile.TemporaryDirectory() as tmp:
        try:
            load_template(tmp, "t")
            print("none: FAILED (no refusal)"); ok = False
        except Refusal as exc:
            print(f"none: {exc.code} ok" if exc.code == "template missing" else f"none: FAILED {exc}"); ok = ok and exc.code == "template missing"
        with open(os.path.join(tmp, "t.v1.md"), "w", encoding="utf-8", newline="\n") as handle:
            handle.write(body)
        try:
            tmpl = load_template(tmp, "t")
            sample = doc or {"goal_state": {k: "x" for k in GOAL_FIELDS}, "clauses": [], "references": [], "assumptions": [],
                             "decision_requests": [], "contract_slot": {}, "provenance": {}, "raw_id": "al-0", "raw_sha256": "0"}
            text = render_document(sample, tmpl)
            rendered_ok = "Goal state" in text and "{{" not in text
            print("one: rendered ok" if rendered_ok else "one: FAILED"); ok = ok and rendered_ok
        except Refusal as exc:
            print(f"one: FAILED {exc}"); ok = False
        with open(os.path.join(tmp, "t.v2.md"), "w", encoding="utf-8", newline="\n") as handle:
            handle.write(body.replace("version: 1", "version: 2"))
        try:
            load_template(tmp, "t")
            print("two: FAILED (no refusal)"); ok = False
        except Refusal as exc:
            print(f"two: {exc.code} ok" if exc.code == "template ambiguous" else f"two: FAILED {exc}"); ok = ok and exc.code == "template ambiguous"
    print("self-test OK: template missing / rendered / template ambiguous" if ok else "self-test FAILED")
    return 0 if ok else 1


def cmd_render(args) -> int:
    if args.self_test:
        return render_self_test()
    if not args.compiled or not args.harness:
        return _usage("render <compiled.json> --harness <name> | render --self-test")
    try:
        doc = _read_doc(args.compiled)
    except (OSError, json.JSONDecodeError) as exc:
        return _usage(f"{args.compiled} — fix: not readable JSON ({exc})")
    bad = check_schema(doc)
    if bad:
        return _usage(f"{args.compiled} malformed at key {bad} — fix: conform to {SCHEMA}")
    print(render_document(doc, load_template(_templates_dir(args), args.harness)))
    return 0


def cmd_distance(args) -> int:
    entry = find_entry(_audit_root(args), args.compiled, "compilation")
    if entry is None:
        raise Refusal("raw not found", args.compiled, "pass the id of a kind:compilation entry")
    with open(args.received_file, encoding="utf-8") as handle:
        received = handle.read()
    print(f"{edit_distance(str(entry.get('prompt') or ''), received):.4f}")
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="verb")

    s = sub.add_parser("skeleton")
    src = s.add_mutually_exclusive_group(required=True)
    src.add_argument("--text"); src.add_argument("--text-file"); src.add_argument("--from-audit")
    s.add_argument("--harness", required=True); s.add_argument("--out", required=True)
    s.add_argument("--no-model", action="store_true")
    s.add_argument("--audit-root"); s.add_argument("--templates-dir")

    f = sub.add_parser("finish")
    f.add_argument("compiled"); f.add_argument("--harness"); f.add_argument("--session", required=True)
    f.add_argument("--skill"); f.add_argument("--compiler-model"); f.add_argument("--compile-tokens", type=int)
    f.add_argument("--no-clipboard", action="store_true")
    f.add_argument("--audit-root"); f.add_argument("--templates-dir")

    r = sub.add_parser("render")
    r.add_argument("compiled", nargs="?"); r.add_argument("--harness"); r.add_argument("--self-test", action="store_true")
    r.add_argument("--templates-dir")

    d = sub.add_parser("distance")
    d.add_argument("--compiled", required=True); d.add_argument("--received-file", required=True)
    d.add_argument("--audit-root")

    args = parser.parse_args(argv)
    handlers = {"skeleton": cmd_skeleton, "finish": cmd_finish, "render": cmd_render, "distance": cmd_distance}
    if args.verb not in handlers:
        parser.print_usage(sys.stderr)
        return 2
    try:
        return handlers[args.verb](args)
    except Refusal as exc:
        sys.stderr.write(str(exc) + "\n")
        return 1


if __name__ == "__main__":
    sys.exit(main())
