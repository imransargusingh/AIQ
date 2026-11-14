"""Read-only, counts only: sort Home's tasks into rough groups by words in AIQ's own one-line analysis note,
and show which Kind each group got. Prints no text from the store.
Run after kinds_home.py in the same folder (it reuses that script's rebuild of Home's task list).
Usage: python3 kinds_words.py <path to aiq.db>
"""
import collections
import contextlib
import io
import json
import re
import runpy
import sys

buf = io.StringIO()
with contextlib.redirect_stdout(buf):
    g = runpy.run_path("kinds_home.py", run_name="kinds_home")
H, NOW, DAY, group, pct = g["H"], g["NOW"], g["DAY"], g["group"], g["pct"]

RULES = [
    ("news that a release is out", r"\b(published|released)\b"),
    ("newsletter, deck or presentation", r"newsletter|\bdeck\b|presentation|slides|leadership update"),
    ("carrying out a plan or continuing a build", r"continu\w* (the |with )?implementation|implementation plan|architecture plan|publication plan|written plan|named plan|existing plan|completion plan|earlier plan|against (a|an|the) [\w\- ]*plan|remaining implementation|keep going|resume work|\bdo it\b|autonomous\w* (implementation|continuation|progress)|handed over [\w\- ]*implementation|execute (an|the) [\w\- ]*(plan|instructions)|implement TASK"),
    ("checking on progress or status", r"progress (update|summary)|\bstatus\b|for progress|whether [\w\- ]*(finished|working|addressing)|why the agent stopped|actively working|still working"),
    ("source control chore", r"\bcommit(s|ted)?\b|\bpull(ed)?\b|\borigin\b|\bpush(ed)?\b|\bbranch\b|\bmerge"),
    ("starting or stopping something", r"stop the app|stop [\w\- ]*process|launch it|run the app|\brestart"),
    ("backlog or task list", r"backlog|task list"),
    ("checking claims or premises", r"premise"),
    ("greeting or test probe", r"greeting|say hi|single word|the word (ok|done)|single-word|reply with"),
    ("audit or clean-up", r"\baudit\b|safe to delete|\bstorage\b|clean\w* up"),
    ("watching a run", r"monitor"),
    ("asking what to do next", r"what to (do|work on) next|recommend the next"),
]
RX = [(n, re.compile(p, re.I)) for n, p in RULES]


def note(t):
    try:
        ev = json.loads(t["ev"])
    except Exception:
        return ""
    return " ".join((ev[0] if ev else "").split())


def bucket(t):
    s = note(t)
    for n, rx in RX:
        if rx.search(s):
            return n
    return "none of these words"


for label, lo in (("all time", 0), ("last 30 days", NOW - 30 * DAY), ("last 7 days", NOW - 7 * DAY)):
    S = [t for t in H if t["first"] >= lo]
    U = [t for t in S if group(t) in ("job_other", "no_job")]
    print(f"== {label}: {len(S)} tasks, {len(U)} with no named kind ==")
    c = collections.Counter(bucket(t) for t in U)
    allc = collections.Counter(bucket(t) for t in S)
    for n, v in c.most_common():
        oj = sum(1 for t in U if bucket(t) == n and group(t) == "job_other")
        jobs = collections.Counter(t["job"] for t in U if bucket(t) == n)
        print(f"  {n}: {v} of the unnamed ({pct(v, len(U))}; Other kind {oj}, no job {v - oj}); in all tasks {allc[n]}; unnamed by job {dict(jobs)}")
    named = collections.defaultdict(collections.Counter)
    for t in S:
        if group(t) == "named":
            named[bucket(t)][(t["job"], t["kind"])] += 1
    print("  -- where tasks with these words did get a named kind:")
    for n, _ in RX:
        if named[n]:
            print(f"    {n}: " + ", ".join(f"{k[0]}/{k[1]} {v}" for k, v in named[n].most_common(6)))
    tools = collections.Counter((bucket(t), t["tool"]) for t in U)
    print("  -- unnamed, by tool: " + ", ".join(f"{k[0]} [{k[1]}] {v}" for k, v in tools.most_common(14)))
    rungs = collections.Counter((bucket(t), t["rung"]) for t in S if t["state"] != "uncertain" and bucket(t) == "carrying out a plan or continuing a build")
    print(f"  -- rung of 'carrying out a plan' tasks (sure rungs): {dict(sorted((k[1], v) for k, v in rungs.items()))}")
