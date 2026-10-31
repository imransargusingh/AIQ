#!/usr/bin/env python3
"""dream.py - the AI-Forward dreaming / continuous-improvement consolidation harness.

Offline, reviewable consolidation over the committed corpus (audit-log.jsonl, change-log.jsonl,
the defect-class register, captured mitigations, and triggered simplify:/assume: markers). It runs
light -> (REM) -> deep and emits a *dream*: a set of reviewable proposals + an HTML review view + a
Dream Diary entry. It writes NO durable store on `run` - only `apply-decisions` (after a human
approves in the HTML view) touches a durable store, and it re-validates + re-scrubs first.

Design authority: docs/architecture-dreaming.md + ADR-0002..0005 + spec-dreaming.
- Deterministic at the floor (LOA P2): staging, taint gate, scrub, dedup, scoring, thresholds,
  rendering, promotion and reconciliation are all stdlib. The one model step (REM abstraction) is an
  INJECTED boundary (ADR-0005): `run` produces deterministic proposals; a runner may enrich them.
- Human gate, no auto-merge (BoK D3): `run` proposes; `apply-decisions` writes only approved items.
- Append-only inputs, new-artifact output: the source logs are never mutated.

Python 3.8+, stdlib only. Subcommands:
  run                 Consolidate the corpus -> a dream (dream.json + dream-data.js + review HTML + diary).
  capture-mitigation  Append a MitigationRecord (the promotion oracle) - red-green or human-validated.
  apply-decisions     Validate a decisions file from the review view, then promote approved learnings.
  list                Show recent dreams / mitigations.
"""
import argparse, datetime, hashlib, json, os, re, sys
from collections import defaultdict

# Windows consoles default to cp1252, which cannot encode the box/arrow glyphs this
# tool prints - `prompt-log.py --help` crashed outright with UnicodeEncodeError (FR-047).
# The other scripts survived only because their glyphs happen to exist in cp1252, which is
# luck rather than an invariant, so the guard is applied uniformly.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass


# ----------------------------------------------------------------------------- paths & io
def find_root(start):
    p = os.path.abspath(start)
    while True:
        if os.path.isdir(os.path.join(p, "docs", "audit")) or os.path.isdir(os.path.join(p, ".git")):
            return p
        parent = os.path.dirname(p)
        if parent == p:
            return os.path.abspath(start)
        p = parent

def now_iso():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

def read_jsonl(path):
    out = []
    if not os.path.isfile(path):
        return out
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                pass  # a malformed line is skipped, not fatal (reliability NFR)
    return out

def append_jsonl(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "a", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps(obj, ensure_ascii=False) + "\n")

# ----------------------------------------------------------------------------- scrub & taint (defence in depth)
SECRET_RE = [
    re.compile(r"[A-Za-z0-9_\-]*(?:secret|token|api[_-]?key|password|passwd|pwd|bearer)[A-Za-z0-9_\-]*\s*[:=]\s*\S+", re.I),
    re.compile(r"\b(?:sk|pk|ghp|gho|xox[baprs])[-_][A-Za-z0-9]{16,}\b"),
    re.compile(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b"),  # email (PII)
    re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
    re.compile(r"\bgithub_pat_[A-Za-z0-9_]{20,}"),           # FR-103: fine-grained GitHub token
    re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),        # FR-103: a pasted private key
]
UNTRUSTED_ORIGINS = {"untrusted", "system", "tool", "subagent", "cron", "heartbeat"}

def scrub(text):
    if not isinstance(text, str):
        return text, False
    hit = False
    for rx in SECRET_RE:
        if rx.search(text):
            hit = True
            text = rx.sub("[REDACTED]", text)
    return text, hit

def control_text(record):
    """The control on a learning may be a {rung, text} object (what a dream writes) or a bare
    string (a hand-edited store, or an older record — this JSONL is a plain committed file
    anyone can edit). Chaining `.get("control", {}).get("text")` crashed with an unhandled
    AttributeError on the string form. A genuinely absent control still returns "" and is
    rejected by the caller, so the CI6 guard is unchanged; only the crash is gone."""
    control = record.get("control")
    if isinstance(control, dict):
        return str(control.get("text") or "").strip()
    if isinstance(control, str):
        return control.strip()
    return ""


def is_tainted(sig):
    origin = str(sig.get("origin", "")).lower()
    if origin in UNTRUSTED_ORIGINS:
        return True
    blob = json.dumps(sig, ensure_ascii=False)
    _, hit = scrub(blob)
    return hit

# ----------------------------------------------------------------------------- corpus reader (light phase input)
def parse_defect_classes(path):
    """Very small parser for docs/lessons/defect-classes.md: one entry per '### <ID> - <shape>'."""
    classes = []
    if not os.path.isfile(path):
        return classes
    with open(path, "r", encoding="utf-8") as f:
        txt = f.read()
    for m in re.finditer(r"^###\s+([A-Z0-9\-]+)\s+[—\-]\s+(.+?)\s*$(.*?)(?=^###\s|\Z)", txt, re.M | re.S):
        cid, shape, body = m.group(1).strip(), m.group(2).strip(), m.group(3)
        status = "unknown"
        sm = re.search(r"\*\*Status:\*\*\s*`?([a-z\-]+)`?", body)
        if sm:
            status = sm.group(1)
        signature = ""
        sg = re.search(r"\*\*Signature:\*\*\s*(.+)", body)
        if sg:
            signature = sg.group(1).strip()
        classes.append({"id": cid, "shape": shape, "status": status, "signature": signature})
    return classes

def grep_markers(root):
    """Harvest simplify:/assume: markers (bounded; skip generated/vendored trees)."""
    skip = {".git", "node_modules", "dist", "_site", ".claude", ".github"}
    # A comment leader glued to a quote or escape is a string literal, not a marker (LINT-A).
    rx = re.compile(r"(?:^|(?<=\s))(?:#|//)\s?(simplify|assume|ponytail):\s*(.+)")
    out = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in skip]
        # do not walk the dreams output or the pack mirror -- judged BELOW the root (SKIP-P):
        # a root under a directory named `ai-forward-pack...` must still be harvested.
        parts = os.path.relpath(dirpath, root).split(os.sep)
        if "dreams" in parts or "ai-forward-pack" in parts:
            dirnames[:] = []
            continue
        for fn in filenames:
            if not fn.endswith((".py", ".ps1", ".ts", ".js", ".cs", ".go", ".rs", ".md")):
                continue
            fp = os.path.join(dirpath, fn)
            try:
                with open(fp, "r", encoding="utf-8", errors="ignore") as f:
                    for i, line in enumerate(f, 1):
                        mm = rx.search(line)
                        if mm:
                            rel = os.path.relpath(fp, root).replace(os.sep, "/")
                            out.append({"marker": mm.group(1), "text": mm.group(2).strip()[:200],
                                        "src": "{0}#L{1}".format(rel, i), "origin": "owner"})
            except OSError:
                pass
    return out

def load_corpus(root, days):
    audit_dir = os.path.join(root, "docs", "audit")
    cutoff = (datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(days=days))
    def recent(entries, field="datetime"):
        keep = []
        for e in entries:
            ts = e.get(field) or e.get("created_at") or ""
            try:
                dt = datetime.datetime.fromisoformat(ts.replace("Z", "+00:00"))
                if dt.tzinfo is None:
                    dt = dt.replace(tzinfo=datetime.timezone.utc)
            except (ValueError, AttributeError):
                dt = cutoff  # undated -> treated as in-window
            if dt >= cutoff:
                keep.append(e)
        return keep
    audit = recent(read_jsonl(os.path.join(audit_dir, "audit-log.jsonl")))
    change = recent(read_jsonl(os.path.join(audit_dir, "change-log.jsonl")))
    mitig = recent(read_jsonl(os.path.join(root, "docs", "lessons", "mitigations.jsonl")))
    classes = parse_defect_classes(os.path.join(root, "docs", "lessons", "defect-classes.md"))
    markers = grep_markers(root)
    profiles = load_profiles(root, cutoff)
    return {"audit": audit, "change": change, "mitigations": mitig, "classes": classes, "markers": markers,
            "profiles": profiles,
            "counts": {"audit": len(audit), "change": len(change), "mitigations": len(mitig),
                       "classes": len(classes), "markers": len(markers), "profiles": len(profiles)}}


def load_profiles(root, cutoff):
    """docs/profiles/<sp-id>/profile.json in the window - session-profile.py output (data, not graph)."""
    out = []
    base = os.path.join(root, "docs", "profiles")
    if not os.path.isdir(base):
        return out
    for name in sorted(os.listdir(base)):
        path = os.path.join(base, name, "profile.json")
        if not os.path.isfile(path):
            continue
        try:
            with open(path, encoding="utf-8") as fh:
                pr = json.load(fh)
        except (OSError, ValueError):
            continue
        ts = pr.get("generated") or ""
        try:
            dt = datetime.datetime.fromisoformat(ts.replace("Z", "+00:00"))
        except (ValueError, AttributeError):
            dt = cutoff
        if dt >= cutoff:
            out.append(pr)
    return out

# ----------------------------------------------------------------------------- scoring (deep phase)
def score(freq, distinct_days, has_control, recency=1.0):
    # descends from Generative Agents importance x recency x relevance; weights per OpenClaw deep-ranking.
    f = min(freq / 5.0, 1.0)
    d = min(distinct_days / 3.0, 1.0)
    c = 0.0 if has_control else 1.0  # an uncontrolled recurring shape is higher-leverage
    return round(0.30 * f + 0.24 * c + 0.15 * d + 0.15 * recency + 0.16 * 0.5, 2)

# ----------------------------------------------------------------------------- proposal builders (REM->deep, deterministic floor)
def build_proposals(corpus):
    proposals, diary = [], {"added": 0, "merged": 0, "superseded": 0, "excluded": 0}
    seen = set()

    # 1. Confirmed mitigation -> learning (the promotion oracle; strongest signal)
    for m in corpus["mitigations"]:
        if is_tainted(m):
            diary["excluded"] += 1
            continue
        oracle = m.get("oracle", "unverified")
        if oracle == "unverified":
            continue  # an optimistic self-report is not an oracle (ADR-0003)
        title, _ = scrub(m.get("summary") or m.get("title") or "Successful mitigation")
        sig, _ = scrub(m.get("class") or m.get("signature") or title)
        ev = [{"eid": m.get("id", "mit-?"),
               "note": "oracle={0}; {1}".format(oracle, "red-observed then green" if oracle == "red-green" else "human-validated")}]
        for t in (m.get("tests") or [])[:4]:
            ev.append({"eid": str(t), "note": "verification test"})
        proposals.append({
            "kind": "Confirmed mitigation", "group": "Confirmed mitigation \u2192 learning",
            "title": "Successful mitigation: {0}".format(title[:120]),
            "sig": sig[:200], "scope": "general" if oracle == "red-green" else "repo-local",
            "confidence": "v" if oracle == "red-green" else "i", "source": "deterministic",
            "evidence": ev,
            "control": {"rung": "automated control",
                        "text": (m.get("control") or "Keep the verifying test as the control; it fails red on recurrence."),
                        "loc": (m.get("control_loc") or ", ".join(m.get("tests", [])) or "tests/")},
            "boundary": (m.get("boundary") or "Applies wherever this shape recurs; the red-observed test is the control (CI6)."),
            "_freq": 1, "_days": 1, "_has_control": bool(m.get("tests"))})
        diary["added"] += 1

    # 2. Uncontrolled recurring classes -> control-upgrade proposals
    for c in corpus["classes"]:
        if c["status"] in ("uncontrolled", "partially-controlled"):
            # recency: is this class referenced by any recent audit/change entry?
            refs = [e for e in corpus["audit"] + corpus["change"]
                    if c["id"].lower() in json.dumps(e, ensure_ascii=False).lower()]
            freq = 1 + len(refs)
            s = score(freq, min(freq, 3), has_control=(c["status"] == "partially-controlled"))
            proposals.append({
                "kind": "Control upgrade", "group": "Control upgrade",
                "title": "Build a control for {0} ({1})".format(c["id"], c["status"]),
                "sig": "{0} \u00b7 {1}".format(c["id"], c["shape"])[:200],
                "scope": "general", "confidence": "i", "source": "deterministic",
                "evidence": ([{"eid": "defect-classes#" + c["id"], "note": "status: " + c["status"]}] +
                             [{"eid": e.get("id", "?"), "note": "recent reference"} for e in refs[:4]]),
                "control": {"rung": "automated control",
                            "text": "Derive a falsifiable control for this class and observe it failing on the un-fixed shape (CI6); move status -> controlled.",
                            "loc": "docs/lessons/defect-classes.md#" + c["id"]},
                "boundary": "Applies wherever the class's signature recurs; a control is not a control until observed failing.",
                "_freq": freq, "_days": min(freq, 3), "_has_control": c["status"] == "partially-controlled",
                "_score": s})

    # 3. Register dedup (token-overlap on signatures; flagged, human-resolved - never auto-merged)
    cl = corpus["classes"]
    for i in range(len(cl)):
        for j in range(i + 1, len(cl)):
            a, b = cl[i], cl[j]
            ta = set(re.findall(r"[a-z]{4,}", (a["shape"] + " " + a["signature"]).lower()))
            tb = set(re.findall(r"[a-z]{4,}", (b["shape"] + " " + b["signature"]).lower()))
            if not ta or not tb:
                continue
            overlap = len(ta & tb) / max(1, len(ta | tb))
            if overlap >= 0.5:
                key = tuple(sorted((a["id"], b["id"])))
                if key in seen:
                    continue
                seen.add(key)
                proposals.append({
                    "kind": "Register dedup", "group": "Register dedup",
                    "title": "Possible duplicate: {0} and {1}".format(a["id"], b["id"]),
                    "sig": "Reconcile {0} with {1} (signature overlap {2:.0%})".format(a["id"], b["id"], overlap),
                    "scope": "repo-local", "confidence": "f", "source": "deterministic",
                    "evidence": [{"eid": "defect-classes#" + a["id"], "note": a["shape"][:120]},
                                 {"eid": "defect-classes#" + b["id"], "note": b["shape"][:120]}],
                    "control": {"rung": "register entry only",
                                "text": "FLAGGED for human decision: merge, cross-reference, or keep separate. Reconciliation is slug-exact + human-flagged, never auto-merged (ADR-0004).",
                                "loc": "docs/lessons/defect-classes.md"},
                    "boundary": "Possible duplicate surfaced for human resolution; the two may share a signature but differ in cause.",
                    "_freq": 1, "_days": 1, "_has_control": False, "_score": round(0.4 + overlap * 0.3, 2)})

    # 4. Marker harvest (triggered assume:/simplify: markers - a lesson already written, unread)
    if corpus["markers"]:
        by_kind = defaultdict(list)
        for mk in corpus["markers"]:
            by_kind[mk["marker"]].append(mk)
        for kind, items in by_kind.items():
            if kind == "assume":
                title = "Harvest {0} assume: marker(s) - each is an unverified belief with a stated trigger".format(len(items))
                text = "Review each assume: marker; a triggered one is a bug already written down (NG9). Verify or convert to a control."
            else:
                title = "Harvest {0} simplify: marker(s) - each is a bounded shortcut with an upgrade trigger".format(len(items))
                text = "Review each simplify: marker against its upgrade trigger; a triggered one is debt due (L6)."
            proposals.append({
                "kind": "Doc update", "group": "Doc / knowledge update",
                "title": title, "sig": "{0} marker harvest".format(kind), "scope": "repo-local",
                "confidence": "v", "source": "deterministic",
                "evidence": [{"eid": mk["src"], "note": mk["text"]} for mk in items[:8]],
                "control": {"rung": "knowledge doc", "text": text, "loc": "solution-selection-ladder.md L6 / no-guessing NG9"},
                "boundary": "Markers in this repo only; harvested at consolidation time.",
                "_freq": len(items), "_days": 1, "_has_control": False, "_score": round(0.35 + min(len(items) / 10.0, 0.3), 2)})

    # 5. PACK-O: front-matter presence + scope-drift review (the rung-2 control for PACK-O).
    #    Presence is mechanical (a substantive turn either recorded done_when or it did not);
    #    "summary exceeds goal" is surfaced as review material, never auto-judged (dream proposes,
    #    the human decides). This is what promotes PACK-O from rung-3 instruction to a rung-2 control.
    PACKO_SUBSTANTIVE = {"skill", "manual", "prompt", "command"}
    subst = [e for e in corpus["audit"] if e.get("kind") in PACKO_SUBSTANTIVE]
    if subst:
        missing = [e for e in subst if not e.get("done_when")]
        have = [e for e in subst if e.get("done_when")]
        ev = [{"eid": e.get("id", "al-?"),
               "note": "{0} - no done_when recorded (front matter skipped)".format(e.get("shortname", "?"))}
              for e in missing[:8]]
        # CTX-C: a goal-state with no tier is a turn with no ceremony budget; a fan-out past the
        # declared cap is a council above tier unless a hard gate was named.
        no_tier = [e for e in have if not e.get("tier")]
        over_cap = [e for e in subst if e.get("fan_out") is not None
                    and len(e.get("agent_runs") or []) > int(e.get("fan_out") or 0)]
        ev += [{"eid": e.get("id", "al-?"),
                "note": "{0} - goal-state without a tier (CT19 tier/fan-out cap, CTX-C)".format(e.get("shortname", "?"))}
               for e in no_tier[:4]]
        ev += [{"eid": e.get("id", "al-?"),
                "note": "{0} - {1} agent runs vs declared fan-out cap {2} (council above tier?)".format(
                    e.get("shortname", "?"), len(e.get("agent_runs") or []), e.get("fan_out"))}
               for e in over_cap[:4]]
        # drift-review material: goal -> summary pairs a human can scan for scope drift
        for e in have[:4]:
            ev.append({"eid": e.get("id", "al-?"),
                       "note": "done_when='{0}' -> summary='{1}'".format(
                           (e.get("done_when") or "")[:60], (e.get("summary") or "")[:80])})
        if ev:
            pct = (len(missing) * 100) // max(1, len(subst))
            proposals.append({
                "kind": "Control upgrade", "group": "Control upgrade",
                "title": "PACK-O: {0}/{1} substantive turns ({2}%) recorded no goal-state (done_when)".format(
                    len(missing), len(subst), pct),
                "sig": "PACK-O front-matter presence + scope-drift review",
                "scope": "general", "confidence": "v", "source": "deterministic",
                "evidence": ev,
                "control": {"rung": "automated control",
                            "text": ("Presence (mechanical): every substantive turn records done_when (CT19); a missing "
                                     "one skipped the front matter. Satisfaction: review each done_when->summary pair "
                                     "where the summary exceeds the goal (scope drift, PACK-O). The audit done_when "
                                     "field + this miner ARE the rung-2 control (CI6)."),
                            "loc": "docs/lessons/defect-classes.md#PACK-O"},
                "boundary": ("Presence is mechanical; 'summary exceeds goal' is surfaced for human review, not "
                             "auto-judged. Trivial/conversational turns are exempt from logging (AL5b)."),
                "_freq": max(1, len(missing)), "_days": min(max(len(missing), 1), 3), "_has_control": True})

    # 7. CO-S0 compile fields (spec-compile-readers US-3): the compile stage records, per workflow
    #    run, `compiled: false` or `compiled_from`; per compilation, `decision_requests[].answer`,
    #    `dispatchable` and `provenance.refusals`. Three deterministic candidates, proposals only:
    #    presence gaps, decision requests still unanswered in the text a workflow received, and the
    #    gate's refusals by code. Evidence carries ids, shortnames, DR ids and codes - never text.
    comp_runs = [e for e in corpus["audit"] if e.get("kind") == "skill" and ("compiled" in e or e.get("compiled_from"))]
    comps = [e for e in corpus["audit"] if e.get("kind") == "compilation" and isinstance(e.get("compiled"), dict)]
    if comp_runs:
        gaps = [e for e in comp_runs if e.get("compiled") is False]
        pct = (len(gaps) * 100) // max(1, len(comp_runs))
        ev = [{"eid": e.get("id", "al-?"), "note": "{0} - closed with compiled: false (tier {1})".format(
            e.get("shortname", "?"), e.get("tier") or "unset")} for e in gaps[:8]]
        proposals.append({
            "kind": "Control upgrade", "group": "Control upgrade",
            "title": "CO-S0: {0}/{1} substantive turns ({2}%) recorded compiled: false".format(len(gaps), len(comp_runs), pct),
            "sig": "CO-S0 compiled:false presence", "scope": "general", "confidence": "v", "source": "deterministic",
            "evidence": ev or [{"eid": comp_runs[0].get("id", "al-?"), "note": "every recorded run started compiled"}],
            "control": {"rung": "automated control",
                        "text": ("Presence (mechanical): a substantive turn closes with compiled_from or compiled: false; "
                                 "the consuming skill cites CO-S0 (pack fix F-26) and session-profile.py SP-27 flags "
                                 "a gap above T0. A T0 closed question needs no compile - review each gap."),
                        "loc": "knowledge/agent-coordination.md#CO-S0"},
            "boundary": "Presence is mechanical; whether a gap was a closed question is human review, never auto-judged.",
            "_freq": max(1, len(gaps)), "_days": min(max(len(gaps), 1), 3), "_has_control": True})
    if comps:
        latest = {}
        for c in sorted(comps, key=lambda e: e.get("datetime") or ""):
            latest[(c["compiled"].get("raw_id") or c.get("id"))] = c  # the newest compilation of a raw prompt decides
        dr_ev = []
        for c in latest.values():
            text = c.get("prompt") or ""
            for dr in c["compiled"].get("decision_requests") or []:
                did = dr.get("id", "DR-?")
                if dr.get("answer") is None and re.search(re.escape(did) + r"\b.*answer: unanswered", text):
                    dr_ev.append({"eid": c.get("id", "al-?"), "note": "{0} unanswered in the compiled text (dispatchable: {1})".format(
                        did, c["compiled"].get("dispatchable"))})
        if dr_ev:
            proposals.append({
                "kind": "Control upgrade", "group": "Control upgrade",
                "title": "CO-S0: {0} decision request(s) never answered before the workflow ran".format(len(dr_ev)),
                "sig": "CO-S0 unanswered decision requests", "scope": "general", "confidence": "v", "source": "deterministic",
                "evidence": dr_ev[:8],
                "control": {"rung": "automated control",
                            "text": ("A consuming skill refuses a compiled prompt whose dispatchable is false or whose text still "
                                     "carries an unanswered DR-n (stop: decision request unanswered) - verify-skill-contracts.py "
                                     "and the optimize-graph dispatch stop."),
                            "loc": "knowledge/agent-coordination.md#CO-S0"},
                "boundary": "Only the newest compilation of a raw prompt is read; an answer in a later recompile clears the request.",
                "_freq": len(dr_ev), "_days": min(len(dr_ev), 3), "_has_control": True})
        codes = defaultdict(int)
        first_by_code = {}
        for c in comps:
            for code in (c["compiled"].get("provenance") or {}).get("refusals") or []:
                codes[code] += 1
                first_by_code.setdefault(code, c.get("id", "al-?"))
        if codes:
            proposals.append({
                "kind": "Control upgrade", "group": "Control upgrade",
                "title": "CO-S0: the compile gate refused {0} fill(s) across {1} code(s)".format(sum(codes.values()), len(codes)),
                "sig": "CO-S0 gate refusals", "scope": "general", "confidence": "v", "source": "deterministic",
                "evidence": [{"eid": first_by_code[code], "note": "{0}: {1} refusal(s)".format(code, n)}
                             for code, n in sorted(codes.items(), key=lambda kv: -kv[1])][:8],
                "control": {"rung": "automated control",
                            "text": "A refusal code that recurs is a template or fill-instruction class: revise the harness template version (pack fix F-27), not the fill.",
                            "loc": "knowledge/agent-coordination.md#CO-S0"},
                "boundary": "Counts by stable refusal code; the clause text stays in the compilation entry.",
                "_freq": sum(codes.values()), "_days": min(len(codes), 3), "_has_control": True})

    # 6. Session profiles (docs/profiles/*/profile.json, written by session-profile.py): each
    #    finding id that recurs across profiles becomes a control-upgrade proposal carrying the
    #    fix catalog's control. Deterministic; the profiles are data the profiler measured, and
    #    every note passes the scrub so a prompt fragment cannot carry a secret into the dream.
    prof = corpus.get("profiles") or []
    by_id = {}
    for pr in prof:
        for f in pr.get("findings") or []:
            fid = f.get("id")
            if not fid:
                continue
            slot = by_id.setdefault(fid, {"title": f.get("title", fid), "severity": f.get("severity", "Minor"),
                                          "fixes": f.get("fixes") or [], "evidence": [], "profiles": set()})
            slot["profiles"].add(pr.get("id", "?"))
            for e in (f.get("evidence") or [])[:2]:
                note = scrub(str(e.get("note", "")))
                if isinstance(note, tuple):  # (text, tainted) - a tainted note is EXCLUDED, not down-scored (AL4)
                    if note[1]:
                        continue
                    note = note[0]
                slot["evidence"].append({"eid": "{0}/{1}".format(pr.get("id", "?"), fid), "note": str(note)[:160]})
    for fid, slot in sorted(by_id.items()):
        fixes = ", ".join(slot["fixes"]) or "see the fix catalog (session-profile.py fixes)"
        proposals.append({
            "kind": "Control upgrade", "group": "Control upgrade",
            "title": "{0}: {1} (seen in {2} profile(s))".format(fid, slot["title"], len(slot["profiles"])),
            "sig": "session-profile {0}".format(fid), "scope": "general", "confidence": "v", "source": "deterministic",
            "evidence": slot["evidence"][:8],
            "control": {"rung": "automated control",
                        "text": "Apply pack fix(es) {0}; the profiler re-flags {1} on recurrence.".format(fixes, fid),
                        "loc": "docs/profiles/ (session-profile.py)"},
            "boundary": "Measured from harness telemetry; heuristics (regex over text) are labelled Inferred in the profile.",
            "_freq": len(slot["profiles"]), "_days": min(len(slot["profiles"]), 3), "_has_control": True})

    # finalize scores + ids + threshold gate
    out = []
    for k, p in enumerate(proposals, 1):
        p["id"] = "p{0}".format(k)
        if "_score" not in p:
            p["_score"] = score(p.get("_freq", 1), p.get("_days", 1), p.get("_has_control", False))
        p["score"] = p.pop("_score")
        for junk in ("_freq", "_days", "_has_control"):
            p.pop(junk, None)
        # threshold gate: keep anything with evidence; a proposal with no evidence is dropped
        if p["evidence"]:
            out.append(p)
        else:
            diary["excluded"] += 1
    out.sort(key=lambda x: x["score"], reverse=True)
    return out, diary

# ----------------------------------------------------------------------------- render
def dream_id(root):
    d = os.path.join(root, "docs", "dreams")
    n = 0
    if os.path.isdir(d):
        for name in os.listdir(d):
            m = re.match(r"drm-(\d+)", name)
            if m:
                n = max(n, int(m.group(1)))
    return "drm-{0:04d}".format(n + 1)

def render_data_js(dream):
    return "window.DREAM_DATA = " + json.dumps(dream, ensure_ascii=False, indent=2) + ";\n"

def render_html(root, out_dir):
    tpl = os.path.join(root, "docs", "ai-forward-pack", "templates", "dream-review.template.html")
    if not os.path.isfile(tpl):
        tpl = os.path.join(root, "pack", "templates", "dream-review.template.html")
    if os.path.isfile(tpl):
        return open(tpl, "r", encoding="utf-8").read()
    # minimal self-bootstrap fallback (kept dependency-free) if the template is missing
    return ("<!doctype html><html><head><meta charset='utf-8'><title>Dream review</title></head>"
            "<body><h1>Dream review</h1><p>Load dream-data.js:</p><script src='./dream-data.js'></script>"
            "<pre id='o'></pre><script>document.getElementById('o').textContent="
            "JSON.stringify(window.DREAM_DATA,null,2);</script></body></html>")

def append_diary(root, dream):
    path = os.path.join(root, "docs", "dreams", "DREAMS.md")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if not os.path.isfile(path):
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write("---\n"
                    "id: dream-diary\n"
                    "title: \"Dream Diary\"\n"
                    "type: doc\n"
                    "status: accepted\n"
                    "owner: \"@timianmalloo\"\n"
                    "phase: \"dreaming\"\n"
                    "tags: [dreaming, dream-diary, continuous-improvement]\n"
                    "links:\n"
                    "  - { to: spec-dreaming, rel: relates-to }\n"
                    "review-by: \"\"\n"
                    "summary: >-\n"
                    "  Human-readable narrative of each dream pass (what it added/merged/superseded). NOT a\n"
                    "  promotion source - excluded from re-ingestion (no self-poisoning). Generated by dream.py.\n"
                    "---\n\n"
                    "# Dream Diary\n\n*A human-readable narrative of each dream pass. NOT a promotion "
                    "source - excluded from re-ingestion (no self-poisoning). Generated by dream.py.*\n\n")
    d = dream["diary"]
    with open(path, "a", encoding="utf-8", newline="\n") as f:
        f.write("## {0} - {1}\n".format(dream["id"], dream["date"]))
        f.write("- window: {0}\n".format(dream["window"]))
        f.write("- proposals: {0} (added {1} - merged {2} - superseded {3} - excluded/tainted {4})\n".format(
            len(dream["proposals"]), d["added"], d["merged"], d["superseded"], d["excluded"]))
        top = dream["proposals"][0]["title"] if dream["proposals"] else "(none)"
        f.write("- highest-leverage: {0}\n\n".format(top))

# ----------------------------------------------------------------------------- commands
def cmd_run(args):
    root = find_root(args.root)
    corpus = load_corpus(root, args.days)
    proposals, diary = build_proposals(corpus)
    did = dream_id(root)
    window = "last {0} days \u00b7 {1} audit \u00b7 {2} change \u00b7 {3} mitigations \u00b7 {4} markers".format(
        args.days, corpus["counts"]["audit"], corpus["counts"]["change"],
        corpus["counts"]["mitigations"], corpus["counts"]["markers"])
    dream = {"id": did, "date": datetime.date.today().isoformat(), "generated": now_iso(),
             "window": window, "counts": corpus["counts"], "proposals": proposals, "diary": diary}
    out_dir = os.path.join(root, "docs", "dreams", did)
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "dream.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(dream, f, ensure_ascii=False, indent=2)
    with open(os.path.join(out_dir, "dream-data.js"), "w", encoding="utf-8", newline="\n") as f:
        f.write(render_data_js(dream))
    with open(os.path.join(out_dir, "index.html"), "w", encoding="utf-8", newline="\n") as f:
        f.write(render_html(root, out_dir))
    append_diary(root, dream)
    # audit trail (Audit Mandate) - best effort via audit-log.py, else inline
    _audit(root, "dream-run", "Dream {0}: {1} proposals over {2}".format(did, len(proposals), window),
           os.path.relpath(os.path.join(out_dir, "index.html"), root).replace(os.sep, "/"), args.session)
    print("dream {0}: {1} proposals (excluded/tainted {2})".format(did, len(proposals), diary["excluded"]))
    print("  review: {0}".format(os.path.join(out_dir, "index.html")))
    print("  diary:  {0}".format(os.path.join(root, "docs", "dreams", "DREAMS.md")))
    return 0

def cmd_capture_mitigation(args):
    root = find_root(args.root)
    if args.oracle not in ("red-green", "human-validated"):
        print("error: --oracle must be red-green or human-validated (an unverified fix is not a mitigation).", file=sys.stderr)
        return 2
    if args.oracle == "red-green" and not args.tests:
        print("error: red-green oracle requires --test (the red-observed->green verification).", file=sys.stderr)
        return 2
    path = os.path.join(root, "docs", "lessons", "mitigations.jsonl")
    n = len(read_jsonl(path))
    summary, _ = scrub(args.summary)
    rec = {"id": "mit-{0:04d}".format(n + 1), "datetime": now_iso(), "oracle": args.oracle,
           "class": args.klass or "", "summary": summary, "tests": args.tests or [],
           "control": args.control or "", "boundary": args.boundary or "", "origin": "owner",
           "session": args.session or ""}
    append_jsonl(path, rec)
    _audit(root, "capture-mitigation", "Captured {0} ({1}): {2}".format(rec["id"], args.oracle, summary[:100]),
           "docs/lessons/mitigations.jsonl", args.session)
    print("captured {0} ({1})".format(rec["id"], args.oracle))
    return 0

def cmd_apply_decisions(args):
    root = find_root(args.root)
    if not os.path.isfile(args.file):
        print("error: decisions file not found: {0}".format(args.file), file=sys.stderr)
        return 2
    try:
        with open(args.file, "r", encoding="utf-8") as f:
            decisions = json.load(f)
    except json.JSONDecodeError as e:
        print("error: decisions file is malformed ({0}); nothing written.".format(e), file=sys.stderr)
        return 2
    if not isinstance(decisions, dict) or "decisions" not in decisions:
        print("error: decisions file missing 'decisions' array; nothing written.", file=sys.stderr)
        return 2
    did = decisions.get("dream", "")
    dpath = os.path.join(root, "docs", "dreams", did, "dream.json")
    if not os.path.isfile(dpath):
        print("error: dream {0} not found; cannot apply.".format(did), file=sys.stderr)
        return 2
    with open(dpath, "r", encoding="utf-8") as f:
        dream = json.load(f)
    by_id = {p["id"]: p for p in dream["proposals"]}
    # idempotency ledger: which (dream,proposal) were already promoted
    ledger_path = os.path.join(root, "learnings", "promoted.jsonl")
    promoted = {(r.get("dream"), r.get("proposal")) for r in read_jsonl(ledger_path)}
    applied, skipped, conflicts, repo_local = 0, 0, 0, 0
    for dec in decisions["decisions"]:
        if dec.get("decision") != "approve":
            continue
        pid = dec.get("id")
        p = by_id.get(pid)
        if not p:
            continue
        if (did, pid) in promoted:
            skipped += 1
            continue  # idempotent: never double-promote (LOA P8)
        # re-run the taint/scrub pass at the boundary (defence in depth)
        blob = json.dumps(p, ensure_ascii=False)
        _, tainted = scrub(blob)
        if tainted:
            conflicts += 1
            print("  ! {0}: excluded at apply-time (taint/scrub hit) - not promoted".format(pid), file=sys.stderr)
            continue
        scope = dec.get("scope", p.get("scope", "repo-local"))
        # G2 guard: a promotable learning must carry a falsifiable control.
        # control_text() tolerates both the {rung,text} object and a bare string; chaining
        # .get().get() here crashed with AttributeError on the string form (swept as a class
        # alongside the identical line in apply-learnings.py).
        if not control_text(p):
            conflicts += 1
            print("  ! {0}: no control - rejected (a lesson without a control is a memoir, CI6)".format(pid), file=sys.stderr)
            continue
        rec = {"dream": did, "proposal": pid, "datetime": now_iso(), "scope": scope,
               "kind": p["kind"], "sig": p["sig"], "control": p["control"], "boundary": p.get("boundary", ""),
               "confidence": p.get("confidence", "i"), "evidence": [e.get("eid") for e in p.get("evidence", [])]}
        if scope == "general":
            _promote_fleet(root, rec)
            applied += 1
        else:
            repo_local += 1
            _promote_repo_local(root, rec)
        append_jsonl(ledger_path, {"dream": did, "proposal": pid, "datetime": now_iso(), "scope": scope})
    _audit(root, "apply-decisions", "Applied {0} general + {1} repo-local (skipped {2}, rejected {3}) from {4}".format(
        applied, repo_local, skipped, conflicts, did), "learnings/fleet-classes.jsonl", args.session)
    print("applied: {0} general, {1} repo-local; skipped (already promoted): {2}; rejected (taint/no-control): {3}".format(
        applied, repo_local, skipped, conflicts))
    return 0

def _promote_fleet(root, rec):
    append_jsonl(os.path.join(root, "learnings", "fleet-classes.jsonl"), rec)
    md = os.path.join(root, "learnings", "fleet-classes.md")
    if not os.path.isfile(md):
        os.makedirs(os.path.dirname(md), exist_ok=True)
        with open(md, "w", encoding="utf-8", newline="\n") as handle:
            handle.write("# Fleet learnings (general, control-bearing classes)\n\n")
    # rec["control"] may be a bare string in a hand-edited store; normalise before .get().
    control = rec.get("control")
    control_body = control if isinstance(control, dict) else {"text": control_text(rec), "rung": ""}
    with open(md, "a", encoding="utf-8", newline="\n") as f:
        f.write("\n### {0}\n- **Signature:** {1}\n- **Control:** {2} ({3})\n- **Boundary:** {4}\n- **Confidence:** {5}\n- **From:** {6} / {7}\n".format(
            rec["sig"][:100], rec["sig"], control_body.get("text", ""), control_body.get("rung", ""),
            rec.get("boundary", ""), rec.get("confidence", ""), rec["dream"], rec["proposal"]))

def _promote_repo_local(root, rec):
    append_jsonl(os.path.join(root, "learnings", "repo-local.jsonl"), rec)

def cmd_list(args):
    root = find_root(args.root)
    d = os.path.join(root, "docs", "dreams")
    if os.path.isdir(d):
        print("dreams:")
        for name in sorted(os.listdir(d)):
            jp = os.path.join(d, name, "dream.json")
            if os.path.isfile(jp):
                with open(jp, encoding="utf-8") as f:
                    dj = json.load(f)
                print("  {0}  {1}  {2} proposals".format(dj["id"], dj["date"], len(dj["proposals"])))
    mits = read_jsonl(os.path.join(root, "docs", "lessons", "mitigations.jsonl"))
    print("mitigations captured: {0}".format(len(mits)))
    return 0

# ----------------------------------------------------------------------------- audit helper
def _audit(root, shortname, summary, artifact, session):
    """Best-effort audit entry via audit-log.py; degrade silently if unavailable."""
    import subprocess
    script = os.path.join(root, "docs", "ai-forward-pack", "scripts", "audit-log.py")
    if not os.path.isfile(script):
        script = os.path.join(root, "pack", "scripts", "audit-log.py")
    if not os.path.isfile(script):
        return
    try:
        subprocess.run([sys.executable, script, "append", "--shortname", shortname, "--kind", "script",
                        "--skill", "dream", "--session", session or "dream-job",
                        "--prompt", "dream.py " + shortname, "--summary", summary, "--artifact", artifact],
                       cwd=root, capture_output=True, timeout=30)
    except (OSError, subprocess.SubprocessError):
        pass

# ----------------------------------------------------------------------------- cli
def main(argv=None):
    ap = argparse.ArgumentParser(description="AI-Forward dreaming consolidation harness (stdlib only).")
    ap.add_argument("--root", default=".", help="repo root (default: discover from cwd)")
    ap.add_argument("--session", default="", help="session id for the audit trail")
    sub = ap.add_subparsers(dest="cmd")

    r = sub.add_parser("run", help="consolidate the corpus into a dream")
    r.add_argument("--days", type=int, default=30, help="corpus window in days (default 30)")
    r.set_defaults(fn=cmd_run)

    c = sub.add_parser("capture-mitigation", help="append a MitigationRecord (the promotion oracle)")
    c.add_argument("--oracle", required=True, choices=["red-green", "human-validated"])
    c.add_argument("--summary", required=True, help="what was mitigated")
    c.add_argument("--class", dest="klass", default="", help="the defect-class id/signature this addressed")
    c.add_argument("--test", dest="tests", action="append", help="a verifying test id (repeatable; required for red-green)")
    c.add_argument("--control", default="", help="the control the fix leaves behind")
    c.add_argument("--boundary", default="", help="where the class applies / does not")
    c.set_defaults(fn=cmd_capture_mitigation)

    a = sub.add_parser("apply-decisions", help="promote approved learnings from a decisions file")
    a.add_argument("file", help="the decisions JSON exported from the review view")
    a.set_defaults(fn=cmd_apply_decisions)

    l = sub.add_parser("list", help="show recent dreams and mitigations")
    l.set_defaults(fn=cmd_list)

    args = ap.parse_args(argv)
    if not getattr(args, "fn", None):
        ap.print_help()
        return 1
    return args.fn(args)

if __name__ == "__main__":
    sys.exit(main())
