"""Read-only: which Kinds the second reading picks instead of "Looking up a fact" (ask_research/find).

Part 1 crosses the person's review marks (aiq-review-answers.json) with the Kind the second reading gave each task
in the check the review page showed. Part 2 counts, for the newest check run of one Kind only (scope kind:<job>/<kind>),
the Kind the second reading picked and whether it listed the analysis's Kind as also fitting. Part 3 counts, for the
same run, how many of the person's prompts show each kind of cue (asks for a status, names a work item, asks who or
where, asks for many things, names a source such as mail or meetings), split by the Kind the second reading picked:
the evidence for sharper Kind wording. The prompts are read on this computer and only the counts are printed.

It prints counts and Kind ids only (no prompt, note, folder or id). The store is opened mode=ro with query_only,
not immutable, so rows still in the write-ahead log are read too (DC-090).

Usage:
    python3 -I find_overlap.py <path to aiq.db> <path to aiq-review-answers.json> [<job>/<kind>]
"""
import collections
import json
import re
import sqlite3
import sys

DB, ANSWERS = sys.argv[1], sys.argv[2]
SCOPE = sys.argv[3] if len(sys.argv) > 3 else "ask_research/find"
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")



def has_table(t):
    return con.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (t,)).fetchone() is not None


# AIQ's text rule and prompt lookup, as in paused_review.py
ENVELOPES = ("<wake ", "<relay ", "<coordinator-relay", "<task-notification", "<scheduled-task", "<project_claude_message")


def unescape(s):
    for a, b in (("&#34;", '"'), ("&quot;", '"'), ("&#39;", "'"), ("&apos;", "'"), ("&lt;", "<"), ("&gt;", ">"), ("&amp;", "&")):
        s = s.replace(a, b)
    return s


def person_words(p):
    t = p.lstrip()
    if not any(t.startswith(o) for o in ENVELOPES):
        return p
    if not t.startswith("<wake"):
        return None
    trigger, bodies = None, []
    for part in t.split("<message ")[1:]:
        if ">" not in part:
            continue
        attrs, rest = part.split(">", 1)
        words = attrs.split()
        if 'from="human"' not in words or "</message>" not in rest:
            continue
        body = unescape(rest.split("</message>", 1)[0].strip())
        if 'trigger="true"' in words and trigger is None:
            trigger = body
        bodies.append(body)
    return trigger if trigger is not None else ("\n".join(bodies) if bodies else None)


def prompt_text(key):
    if key.startswith("import:"):
        try:
            tool, rest = key[len("import:"):].split(":", 1)
            sid, seq = rest.rsplit(":", 1)
            r = con.execute("SELECT text FROM import_event WHERE tool = ? AND source_session_id = ? AND seq = ?",
                            (tool, sid, int(seq))).fetchone()
        except (ValueError, sqlite3.Error):
            return None
    else:
        r = con.execute("SELECT text FROM content WHERE event_id = ?", (key,)).fetchone() if has_table("content") else None
    if not r or not r[0]:
        return None
    w = person_words(r[0])
    return w if w and w.strip() else None


with open(ANSWERS, encoding="utf-8") as f:
    answers = json.load(f)["answers"]


def answer_of(row):
    try:
        return json.loads(row)
    except (TypeError, ValueError):
        return {}


print("Part 1: your marks by the Kind the newest second reading gave the analysis your review showed")
part1 = collections.Counter()
for a in answers:
    if not a.get("mark") or a["mark"] == "agent":
        continue
    r = con.execute(
        "SELECT answer FROM judge_check WHERE judgment_id = ? AND outcome = 'ok' ORDER BY checked_utc_ms DESC LIMIT 1",
        (a.get("task"),)).fetchone()
    ans = answer_of(r[0]) if r else {}
    part1[(a.get("scope") or "?", a["mark"], f"{ans.get('job')}/{ans.get('task_type')}" if r else "no reading")] += 1
for (scope, mark, kind), n in sorted(part1.items()):
    print(f"  {scope}  mark {mark}  second reading {kind}: {n}")

run = con.execute("SELECT run_id FROM judge_check_run WHERE scope = ? ORDER BY started_utc_ms DESC LIMIT 1",
                  (f"kind:{SCOPE}",)).fetchone()
print(f"\nPart 2: newest check of {SCOPE} only: the Kind the second reading picked")
if not run:
    print("  no such run")
    sys.exit(0)
job, kind = SCOPE.split("/", 1)
part2 = collections.Counter()
groups = []
for ans_text, task_id in con.execute("SELECT answer, task_id FROM judge_check WHERE run_id = ? AND outcome = 'ok'", run):
    ans = answer_of(ans_text)
    groups.append((kind if ans.get("job") == job and ans.get("task_type") == kind
                   else f"{ans.get('job')}/{ans.get('task_type')}", prompt_text(task_id)))
    also = ans.get("task_types_also_fit") or []
    if ans.get("job") != job:
        part2[("other job", "")] += 1
    elif ans.get("task_type") == kind:
        part2[(kind, "")] += 1
    else:
        part2[(ans.get("task_type"), f"lists {kind} as also fitting" if kind in also else f"does not list {kind}")] += 1
for (k, note), n in sorted(part2.items(), key=lambda x: -x[1]):
    print(f"  {k}{' (' + note + ')' if note else ''}: {n}")

CUES = [
    ("asks for a status or whether something finished",
     r"\b(status|progress|eta|done yet|finished|complete[d]?|still running|any update|update on|how is|how's|where are we)\b"),
    ("refers to the agent's own earlier work (you / your)", r"\b(did you|have you|are you|you were|your (pr|branch|run|build|change|fix))\b"),
    ("names a work item (PR, ticket, incident, build, pipeline, deployment)",
     r"\b(pr|prs|pull request|work item|ticket|incident|build|pipeline|deployment|release|rollout)\b"),
    ("asks who (owner, contact, team)", r"\b(who|owner|owns|contact|manager)\b"),
    ("asks where (path, link, which file or repo)", r"\b(where|path|link|url|which (file|folder|repo|branch))\b"),
    ("asks whether something exists", r"\b(is there|are there|do we have|does .{0,40} exist)\b"),
    ("asks for many things (all, list, summary, latest, what's new)",
     r"\b(all|every|list|summari[sz]e|summary|overview|everything|across|latest|recent|news|what's new|whats new)\b"),
    ("names a source (mail, meetings, chats, calendar, docs, wiki)",
     r"\b(e-?mails?|mail|meetings?|teams|chats?|calendar|docs?|documents?|sharepoint|wiki|onenote|transcripts?)\b"),
    ("asks to research, explain or compare", r"\b(research|learn about|explain|compare|best practices?|options|pros and cons)\b"),
]
print(f"\nPart 3: cues in the person's words, by the Kind the second reading picked (same run)")
by = collections.defaultdict(list)
for g, text in groups:
    by[g].append(text)
order = sorted(by, key=lambda g: (g != kind, -len(by[g])))
print("  " + " | ".join(f"{g}: {len(by[g])} tasks ({sum(1 for t in by[g] if t is None)} with no words found)" for g in order))
for name, rx in CUES:
    r = re.compile(rx, re.I)
    print(f"  {name}: " + " | ".join(f"{g} {sum(1 for t in by[g] if t and r.search(t))}" for g in order))
for name, lo, hi in (("under 80 characters", 0, 80), ("80 to 300 characters", 80, 300), ("over 300 characters", 300, 1 << 30)):
    print(f"  {name}: " + " | ".join(f"{g} {sum(1 for t in by[g] if t and lo <= len(t) < hi)}" for g in order))
print("  holds a question mark: " + " | ".join(f"{g} {sum(1 for t in by[g] if t and '?' in t)}" for g in order))
