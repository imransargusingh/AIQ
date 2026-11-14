"""Read-only, counts only: how well a fixed word list finds progress checks in the person's first prompt of a task,
and which earlier tasks they would join. Prints no text from the store.
Usage (same folder as the other scripts): python3 -I cue_match.py <path to aiq.db>
"""
import collections
import contextlib
import io
import re
import runpy

buf = io.StringIO()
with contextlib.redirect_stdout(buf):
    g = runpy.run_path("kinds_cross.py", run_name="kinds_cross")
H = g["H"]
home = g["g"]
text_of = {r[0]: r[5] for r in home["kept"]}
text_of.update({p[0]: p[5] for p in home["imp"]})

PLAN = "carrying out a plan or continuing a build"
PROG = "checking on progress or status"

# A progress check asks about work already running; it gives no new goal.
CUES = re.compile(
    r"\b(status|progress|update me|any update|how('?s| is) it going|how far|where are (we|you)|are you (done|finished|still)|"
    r"is it (done|finished|working)|did (it|you) (finish|complete|work)|what('?s| is) left|still (working|running)|"
    r"what happened|why did (you|it) stop|eta|how long)\b",
    re.I,
)


def words(s):
    return len(re.findall(r"\w+", s or ""))


def cue(t):
    s = text_of.get(t["task_id"])
    if s is None:
        return "no text"
    if words(s) > 25:
        return "long"
    return "cue" if CUES.search(s) else "short, no cue"


def cnt(it):
    return dict(sorted(collections.Counter(it).items(), key=lambda kv: (-kv[1], str(kv[0]))))


H2 = [t for t in H if t["task_id"] in text_of]
print(f"tasks with first-prompt text: {len(H2)} of {len(H)}")
for name, sel in (("progress checks", lambda t: t["b"] == PROG), ("all other tasks", lambda t: t["b"] != PROG)):
    S = [t for t in H2 if sel(t)]
    print(f"== {name}: {len(S)} ==")
    print(f"  word list says: {cnt(cue(t) for t in S)}")
    C = [t for t in S if cue(t) == "cue"]
    print(f"  of the cue hits: has an earlier task in the same session {sum(1 for t in C if t.get('prev'))}; "
          f"rung {cnt(t['rung'] for t in C if t['state'] != 'uncertain')}; job {cnt(t['job'] for t in C)}")
    if name == "all other tasks":
        print(f"  cue hits by keyword group: {cnt(t['b'] for t in C)}")

J = [t for t in H2 if cue(t) == "cue" and t.get("prev")]
parents = {id(t["prev"]): t["prev"] for t in J}
print(f"== joining every cue hit to the task before it in the session: {len(J)} tasks join {len(parents)} earlier tasks ==")
print(f"  earlier tasks by keyword group: {cnt(p['b'] for p in parents.values())}")
print(f"  earlier tasks' rung (sure): {cnt(p['rung'] for p in parents.values() if p['state'] != 'uncertain')}")
pl = [p for p in parents.values() if p["b"] == PLAN]
print(f"  plan follow-ons that would gain a later check: {len(pl)}; their rung {cnt(p['rung'] for p in pl)}")
print(f"  by tool: {cnt(t['tool'] for t in J)}")

# Which word in the list each hit used: progress checks vs other tasks (names of the list's own words only).
def which(t):
    m = CUES.search(text_of.get(t["task_id"]) or "")
    return re.sub(r"\W+", " ", m.group(1).lower()).strip() if m else None


C = [t for t in H2 if cue(t) == "cue"]
print("== hits by list word: (progress checks, other tasks) ==")
per = collections.defaultdict(lambda: [0, 0])
for t in C:
    per[which(t)][0 if t["b"] == PROG else 1] += 1
for k, v in sorted(per.items(), key=lambda kv: -sum(kv[1])):
    print(f"  {k}: {tuple(v)}")


# Joining is transitive: a check after a check continues the same task. Follow each hit back to the first
# task in the session that is not itself a hit (the root).
def root(t):
    while t.get("prev") and cue(t) == "cue":
        t = t["prev"]
    return t


R = collections.Counter(id(root(t)) for t in J)
roots = {id(root(t)): root(t) for t in J}
print(f"== transitive join: {len(J)} hits fold into {len(roots)} root tasks ==")
print(f"  roots by keyword group: {cnt(r['b'] for r in roots.values())}")
print(f"  roots' rung (sure): {cnt(r['rung'] for r in roots.values() if r['state'] != 'uncertain')}")
print(f"  roots that are themselves hits (session starts with a check): {sum(1 for r in roots.values() if cue(r) == 'cue')}")
print(f"  hits per root: {cnt(min(n, 4) for n in R.values())} (4 = 4 or more)")
