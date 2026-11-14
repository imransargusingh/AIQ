"""Read-only, counts only: cross-tabs of the keyword groups from kinds_words.py against task length, rung,
tool, what the two analysis runs said, and position in the session. Prints no text from the store.
Run in the same folder as kinds_home.py and kinds_words.py.
Usage: python3 kinds_cross.py <path to aiq.db>
"""
import collections
import contextlib
import io
import runpy
import time

buf = io.StringIO()
with contextlib.redirect_stdout(buf):
    w = runpy.run_path("kinds_words.py", run_name="kinds_words")
g = w["g"]
H, NOW, DAY, group, pct, bucket, RX = w["H"], w["NOW"], w["DAY"], w["group"], w["pct"], w["bucket"], w["RX"]
cause, cand_type, cand_job, SPAN = g["cause"], g["cand_type"], g["cand_job"], g["SPAN"]
HOUR = 3_600_000

print(f"snapshot: Home tasks with an analysis {len(H)}; newest first prompt {time.strftime('%Y-%m-%dT%H:%MZ', time.gmtime(max(t['first'] for t in H) / 1000))}; "
      f"newest analysis {time.strftime('%Y-%m-%dT%H:%MZ', time.gmtime(max(t['judged'] for t in H) / 1000))}")

for t in H:
    t["b"] = bucket(t)

# position in the session: previous task of the same session, and the quiet time before this task
sess = collections.defaultdict(list)
for t in H:
    sess[(t["tool"] if t["tool"] not in SPAN else "vscode", t["session"], t["computer"])].append(t)
for L in sess.values():
    L.sort(key=lambda t: t["first"])
    for i, t in enumerate(L):
        t["pos"] = i
        t["prev"] = L[i - 1] if i else None


def plen(t):
    p = t["prompts"]
    return "1" if p == 1 else ("2-3" if p <= 3 else "4+")


def quiet(t):
    if t["prev"] is None:
        return "first in session"
    d = t["first"] - t["prev"]["last"]
    return "under 1 h" if d < HOUR else ("1-4 h" if d < 4 * HOUR else ("4-24 h" if d < 24 * HOUR else "over 24 h"))


def cnt(it):
    return dict(sorted(collections.Counter(it).items(), key=lambda kv: (-kv[1], str(kv[0]))))


def pair(runs, field):
    if not runs or len(runs) < 2:
        return "not recorded"
    return "+".join(sorted([str(runs[0].get(field)), str(runs[1].get(field))]))


names = [n for n, _ in RX] + ["none of these words"]
for label, lo in (("all time", 0), ("last 30 days", NOW - 30 * DAY), ("last 7 days", NOW - 7 * DAY)):
    S = [t for t in H if t["first"] >= lo]
    U = [t for t in S if group(t) in ("job_other", "no_job")]
    print(f"\n== {label}: {len(S)} tasks, {len(U)} with no named kind ==")
    for n in names:
        B = [t for t in S if t["b"] == n]
        if not B:
            continue
        BU = [t for t in B if group(t) in ("job_other", "no_job")]
        gg = collections.Counter(group(t) for t in B)
        print(f"-- {n}: {len(B)} tasks ({pct(len(B), len(S))} of all); named {gg['named']}, Other kind {gg['job_other']}, no job {gg['no_job']}")
        print(f"   prompts {cnt(plen(t) for t in B)}; tool {cnt(t['tool'] for t in B)}; origin {cnt(t['origin'] for t in B)}; other computer {sum(1 for t in B if t['computer'] is not None)}")
        print(f"   rung (sure) {dict(sorted(collections.Counter(t['rung'] for t in B if t['state'] != 'uncertain').items()))}; rung unsure {sum(1 for t in B if t['state'] == 'uncertain')}")
        print(f"   jobs given {cnt(t['job'] for t in B)}")
        if BU:
            print(f"   unnamed, why {cnt(cause(t) for t in BU)}")
            nj = [t for t in BU if group(t) == "no_job"]
            jo = [t for t in BU if group(t) == "job_other"]
            if nj:
                print(f"   no job, the two runs' jobs {cnt(pair(t['jr'], 'job') for t in nj)}; R1 would name {sum(1 for t in nj if t['jr'] and cand_job('R1', t['jr']))}, R2 {cnt(cand_job('R2', t['jr']) for t in nj if t['jr'] and cand_job('R2', t['jr']))}")
                print(f"   no job, the two runs' kinds {cnt(pair(t['tr'], 'task_type') for t in nj)}")
            if jo:
                print(f"   Other kind, job and the two runs' kinds {cnt((t['job'], pair(t['tr'], 'task_type')) for t in jo)}")
                print(f"   Other kind: K1 would name {cnt(cand_type('K1', t['job'], t['tr']) for t in jo if t['tr'] and cand_type('K1', t['job'], t['tr']))}, K2 {cnt(cand_type('K2', t['job'], t['tr']) for t in jo if t['tr'] and cand_type('K2', t['job'], t['tr']))}")
            print(f"   unnamed, quiet time before the task {cnt(quiet(t) for t in BU)}")
            fol = [t for t in BU if t['prev'] is not None]
            print(f"   unnamed and later in a session {len(fol)}: task before it had a named kind {sum(1 for t in fol if group(t['prev']) == 'named')}, of those under 4 h earlier {sum(1 for t in fol if group(t['prev']) == 'named' and t['first'] - t['prev']['last'] < 4 * HOUR)}; kinds of the task before {cnt((t['prev']['job'], t['prev']['kind']) for t in fol if group(t['prev']) == 'named')}")
    print(f"-- all unnamed tasks, {label}: quiet time before the task {cnt(quiet(t) for t in U)}")
    fol = [t for t in U if t["prev"] is not None]
    print(f"   later in a session {len(fol)}; task before had a named kind {sum(1 for t in fol if group(t['prev']) == 'named')}; "
          f"by quiet time {cnt(quiet(t) for t in fol if group(t['prev']) == 'named')}")
    print(f"   one-prompt tasks: {sum(1 for t in S if t['prompts'] == 1)} of {len(S)}; of the unnamed {sum(1 for t in U if t['prompts'] == 1)} of {len(U)}")
    N = [t for t in S if group(t) == "named"]
    print(f"   named tasks, prompts {cnt(plen(t) for t in N)}; unnamed tasks, prompts {cnt(plen(t) for t in U)}")
    print(f"   rung (sure) of unnamed {dict(sorted(collections.Counter(t['rung'] for t in U if t['state'] != 'uncertain').items()))}, unsure {sum(1 for t in U if t['state'] == 'uncertain')}; "
          f"of named {dict(sorted(collections.Counter(t['rung'] for t in N if t['state'] != 'uncertain').items()))}, unsure {sum(1 for t in N if t['state'] == 'uncertain')}")

# what the laptop would analyze again, by provider (this computer's tasks only)
own = [t for t in H if t["computer"] is None]
print(f"\n== this computer's tasks: {len(own)} ==")
for label, pick in (("no job", lambda t: group(t) == "no_job"), ("Other kind", lambda t: group(t) == "job_other"), ("named", lambda t: group(t) == "named")):
    X = [t for t in own if pick(t)]
    print(f"  {label}: {len(X)}; by analysis provider {cnt(t['provider'] for t in X)}; by tool {cnt(t['tool'] for t in X)}; first prompt in the last 30 days {sum(1 for t in X if t['first'] >= NOW - 30 * DAY)}")
print(f"  analysis versions of this computer's tasks {cnt(t['jv'] for t in own)}")
