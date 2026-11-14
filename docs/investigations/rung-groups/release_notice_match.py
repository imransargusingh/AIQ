"""Read-only, counts only: how well a fixed word list tells a one-line "the release is out" notice from a request
to do a release, on the person's first prompt of a task. Prints no text from the store: only counts, and the
words of the list below.

A task the Kinds trial re-read (results file of `aiq analyze --trial --out`, commit 749c451) is counted with the
job, Kind and rung the re-cut list gave it; every other task with what the store holds. The goal-given fact (F4)
is always the stored one: the trial file does not keep facts.

Usage (same folder as the other scripts):
    python3 -I release_notice_match.py <path to aiq.db> [trial results .jsonl ...]
"""
import collections
import contextlib
import io
import json
import re
import runpy
import sqlite3
import sys

buf = io.StringIO()
with contextlib.redirect_stdout(buf):
    g = runpy.run_path("kinds_cross.py", run_name="kinds_cross")
H, NOW, DAY, group = g["H"], g["NOW"], g["DAY"], g["group"]
home = g["g"]
person_words = home["person_words"]

RELEASE_NEWS = "news that a release is out"  # kinds_words.py: the analysis's own note says published or released
KIND = ("run_ship", "release")
MAX_WORDS = 25  # the progress-check rule's length (aiq-domain coach::PROGRESS_CHECK_MAX_WORDS)

# The release is already out: a state or a past form. Each alternative is counted on its own below.
NOTICE = [
    "published", "released", "shipped", "deployed", "is out", "is live", "is up", "is available", "is public",
    "went out", "went live", "now out", "now live", "it's out", "it's live", "its out", "its live",
]
NOTICE_RX = re.compile(r"\b(" + "|".join(re.escape(w) for w in NOTICE) + r")\b", re.I)
# A request to do the release: the prompt opens with the verb.
REQUEST_RX = re.compile(
    r"^\W*(please\s+|can you\s+|could you\s+|go ahead and\s+|now\s+|ok\s+|okay\s+)*"
    r"(publish|release|ship|cut|deploy|tag|draft|rerun|re-run|bump|build|make|create|prepare|start|kick off|run)\b",
    re.I,
)
VERSION_RX = re.compile(r"\b\d+\.\d+(\.\d+)?\b")
# The shipped progress-check words (aiq-domain coach::is_progress_check), to count tasks both rules would take.
PROGRESS_RX = re.compile(r"\b(status|progress|where are you|why did you stop)\b", re.I)


def own_text(s):
    """The person's own words of a stored prompt (an envelope unwrapped), or None."""
    if s is None:
        return None
    pw = person_words(s)
    if pw is None:
        return None
    return pw[1].replace("’", "'")


text_of = {r[0]: own_text(r[5]) for r in home["kept"]}
text_of.update({p[0]: own_text(p[5]) for p in home["imp"]})


def words(s):
    return len(re.findall(r"[^\W_]+(?:'[^\W_]+)?", s or ""))


def cnt(it):
    return dict(sorted(collections.Counter(it).items(), key=lambda kv: (-kv[1], str(kv[0]))))


def size(t):
    s = text_of.get(t["task_id"])
    if s is None:
        return "no text"
    n = words(s)
    return "0" if n == 0 else ("1-5" if n <= 5 else ("6-10" if n <= 10 else ("11-25" if n <= MAX_WORDS else "over 25")))


def plen(t):
    p = t["prompts"]
    return "1" if p == 1 else ("2-3" if p <= 3 else "4+")


# ---------------------------------------------------------------- stored goal fact, and the trial's answers
con = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro&immutable=1", uri=True)
f4 = {}
for tid, facts in con.execute(
    """SELECT j.task_id, j.facts FROM judgment j WHERE j.rowid = (SELECT x.rowid FROM judgment x WHERE x.task_id = j.task_id
       ORDER BY x.judged_utc_ms DESC, x.rowid DESC LIMIT 1)"""
):
    try:
        f4[tid] = (json.loads(facts).get("F4") or {}).get("value") is True
    except Exception:
        f4[tid] = False

trial = {}
for path in sys.argv[2:]:
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                v = json.loads(line)
            except Exception:
                continue
            new = v.get("new") or {}
            if (new.get("answered") or 0) > 0 and isinstance(v.get("task_id"), str):
                trial[v["task_id"]] = new

for t in H:
    n = trial.get(t["task_id"])
    t["trial"] = n is not None
    if n is not None:
        t["vjob"], t["vkind"], t["vstate"], t["vrung"] = n.get("job"), n.get("task_type"), n.get("state"), n.get("rung")
    else:
        t["vjob"], t["vkind"], t["vstate"], t["vrung"] = t["job"], t["kind"], t["state"], t["rung"]
    s = text_of.get(t["task_id"]) or ""
    t["notice"] = NOTICE_RX.search(s) is not None
    t["request"] = REQUEST_RX.search(s) is not None
    t["short"] = 0 < words(s) <= MAX_WORDS
    t["goal"] = f4.get(t["task_id"], False)
    t["sure1"] = t["vstate"] != "uncertain" and t["vrung"] == 1
    t["rel"] = (t["vjob"], t["vkind"]) == KIND
    t["prog"] = t["short"] and t["sure1"] and not t["goal"] and PROGRESS_RX.search(s) is not None

print(f"tasks on Home with an analysis: {len(H)}; with first-prompt text: {sum(1 for t in H if text_of.get(t['task_id']) is not None)}; "
      f"re-read by the trial: {sum(1 for t in H if t['trial'])} (trial answers read: {len(trial)})")
print(f"  by computer: this one {sum(1 for t in H if t['computer'] is None)}, others {sum(1 for t in H if t['computer'] is not None)}")

R = [t for t in H if t["rel"]]
print(f"\n== Kind 'Releasing or publishing' (stored, or given by the trial): {len(R)} tasks ==")
print(f"  from the trial {sum(1 for t in R if t['trial'])}, stored {sum(1 for t in R if not t['trial'])}; tool {cnt(t['tool'] for t in R)}")
print(f"  rung (sure) {dict(sorted(collections.Counter(t['vrung'] for t in R if t['vstate'] != 'uncertain').items()))}; unsure {sum(1 for t in R if t['vstate'] == 'uncertain')}")
print(f"  prompts {cnt(plen(t) for t in R)}; first prompt words {cnt(size(t) for t in R)}")
print(f"  the analysis's note says published or released: {sum(1 for t in R if t['b'] == RELEASE_NEWS)}")

A = [t for t in R if t["sure1"]]
print(f"\n== of those, a sure Assisted reading: {len(A)} ==")
print(f"  first prompt words {cnt(size(t) for t in A)}; prompts {cnt(plen(t) for t in A)}; goal given (stored F4) {sum(1 for t in A if t['goal'])}")
print(f"  (notice word, opens with a release verb): {cnt((t['notice'], t['request']) for t in A)}")
print(f"  names a version number {sum(1 for t in A if VERSION_RX.search(text_of.get(t['task_id']) or ''))}; "
      f"note says published or released {sum(1 for t in A if t['b'] == RELEASE_NEWS)}")
for label, sel in (
    ("notice word", lambda t: t["notice"]),
    ("no notice word", lambda t: not t["notice"]),
):
    S = [t for t in A if sel(t)]
    print(f"  {label}: {len(S)}; words {cnt(size(t) for t in S)}; opens with a release verb {sum(1 for t in S if t['request'])}; "
          f"note says published or released {sum(1 for t in S if t['b'] == RELEASE_NEWS)}; goal given {sum(1 for t in S if t['goal'])}")


def rule(t, kind=True, cue=True):
    return t["short"] and t["sure1"] and not t["goal"] and (t["rel"] or not kind) and (t["notice"] or not cue)


print("\n== candidate rules (each: short first prompt, sure Assisted, no goal given) ==")
for label, kw in (
    ("A: Kind is Releasing or publishing, and a notice word", dict()),
    ("B: Kind is Releasing or publishing, any words", dict(cue=False)),
    ("C: a notice word, any Kind", dict(kind=False)),
):
    M = [t for t in H if rule(t, **kw)]
    print(f"  {label}: {len(M)} tasks; note says published or released {sum(1 for t in M if t['b'] == RELEASE_NEWS)}; "
          f"opens with a release verb {sum(1 for t in M if t['request'])}; also a progress check {sum(1 for t in M if t['prog'])}; "
          f"last 30 days {sum(1 for t in M if t['first'] >= NOW - 30 * DAY)}; tool {cnt(t['tool'] for t in M)}")
    if "kind" in kw:
        print(f"     by job and Kind: {cnt((t['vjob'], t['vkind']) for t in M)}")

print("\n== hits per list word: (sure Assisted release tasks, other release tasks, tasks of any other Kind) ==")
per = collections.defaultdict(lambda: [0, 0, 0])
for t in H:
    for m in NOTICE_RX.finditer(text_of.get(t["task_id"]) or "") if t["short"] else ():
        per[m.group(1).lower()][0 if (t["rel"] and t["sure1"]) else (1 if t["rel"] else 2)] += 1
for k, v in sorted(per.items(), key=lambda kv: -sum(kv[1])):
    print(f"  {k}: {tuple(v)}")

N = [t for t in H if t["b"] == RELEASE_NEWS]
print(f"\n== tasks whose analysis note says published or released: {len(N)} ==")
print(f"  job and Kind now {cnt((t['vjob'], t['vkind']) for t in N)}")
print(f"  rung (sure) {dict(sorted(collections.Counter(t['vrung'] for t in N if t['vstate'] != 'uncertain').items()))}; unsure {sum(1 for t in N if t['vstate'] == 'uncertain')}; "
      f"re-read by the trial {sum(1 for t in N if t['trial'])}")
print(f"  rule A takes {sum(1 for t in N if rule(t))}; short first prompt {sum(1 for t in N if t['short'])}; notice word {sum(1 for t in N if t['notice'])}")

S1 = [t for t in H if t["sure1"]]
M = [t for t in H if rule(t)]
P = [t for t in H if t["prog"]]
print(f"\n== effect on the ladder (trial answers applied) ==")
print(f"  sure rungs {dict(sorted(collections.Counter(t['vrung'] for t in H if t['vstate'] != 'uncertain').items()))}; unsure {sum(1 for t in H if t['vstate'] == 'uncertain')}")
print(f"  sure Assisted {len(S1)}; progress checks (shipped rule) {len(P)}; release notices (rule A) {len(M)}; both {sum(1 for t in M if t['prog'])}; "
      f"Assisted left on the ladder {len(S1) - len(set(id(t) for t in P) | set(id(t) for t in M))}")

# What this computer analyzes again when the re-cut list ships with "every task with no named Kind": by provider.
own = [t for t in H if t["computer"] is None]
print(f"\n== this computer's analyzed tasks: {len(own)} ==")
for label, pick in (("no job", lambda t: group(t) == "no_job"), ("Other kind", lambda t: group(t) == "job_other"),
                    ("Kind not recorded", lambda t: group(t) == "kind_not_recorded"), ("named Kind", lambda t: group(t) == "named")):
    X = [t for t in own if pick(t)]
    print(f"  {label}: {len(X)}; by analysis provider {cnt(t['provider'] for t in X)}; first prompt in the last 30 days {sum(1 for t in X if t['first'] >= NOW - 30 * DAY)}; "
          f"with text {sum(1 for t in X if text_of.get(t['task_id']) is not None)}")
print(f"  analysis versions {cnt(t['jv'] for t in own)}")
retired = {("code_change", "build_fix"), ("code_change", "review"), ("code_change", "explain"), ("write_doc", "criteria"), ("write_doc", "notes")}
print(f"  stored with a Kind the re-cut list retires: {cnt((t['job'], t['kind']) for t in H if (t['job'], t['kind']) in retired)}")
