"""Read-only: rebuild the task list AIQ Home shows, with AIQ's own rules, check the rebuild against what AIQ
itself recorded, then count Kinds over that list.

Prints counts only: no prompt text and no analysis text from the store.
Opens the store read-only with query_only (not immutable=1, which skips rows still in the write-ahead log).
Rules copied from the source (aiq-home task_list / not_typed_out / twins, aiq-domain split_tasks / automated).
Usage: python3 kinds_home.py <path to aiq.db>
"""
import collections
import json
import random
import re
import sqlite3
import sys
import time

DB = sys.argv[1]
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)  # not immutable: that skips rows still in the WAL
con.execute("PRAGMA query_only = ON")
NOW = int(time.time() * 1000)
DAY = 86_400_000
GAP = 30 * 60_000  # pack idle_gap_min = 30
SPAN = {"vscode"}  # pack reprompt.span_surfaces
W0 = 0 - GAP
random.seed(20261114)


def cols(t):
    return [r[1] for r in con.execute(f"PRAGMA table_info({t})")]


def has_table(t):
    return con.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (t,)).fetchone() is not None


def pct(n, d):
    return f"{(100.0 * n / d):.0f}%" if d else "-"


# ---------------------------------------------------------------- AIQ's text rules (aiq-domain automated.rs, text.rs)
TOOL_OPENINGS = ("<system_notification>", "[scheduled prompt #")
ENVELOPES = ("<wake ", "<relay ", "<coordinator-relay", "<task-notification", "<scheduled-task", "<project_claude_message")
GUID = re.compile(rb"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\Z")
HEX = set(b"0123456789abcdefABCDEF")


def written_by_tool(p):
    head = p.lstrip()[:24].lower()
    return any(head.startswith(o) for o in TOOL_OPENINGS)


def unescape(s):
    for a, b in (("&#34;", '"'), ("&quot;", '"'), ("&#39;", "'"), ("&apos;", "'"), ("&lt;", "<"), ("&gt;", ">"), ("&amp;", "&")):
        s = s.replace(a, b)
    return s


def own_words(p):
    """(owned, text): the person's own message inside a <wake ...> envelope."""
    t = p.lstrip()
    if not t.startswith("<wake"):
        return (False, p)
    trigger, bodies = None, []
    for part in t.split("<message ")[1:]:
        if ">" not in part:
            continue
        attrs, rest = part.split(">", 1)
        words = attrs.split()
        if 'from="human"' not in words:
            continue
        if "</message>" not in rest:
            continue
        body = unescape(rest.split("</message>", 1)[0].strip())
        if 'trigger="true"' in words and trigger is None:
            trigger = body
        bodies.append(body)
    if trigger is not None:
        return (True, trigger)
    if bodies:
        return (True, "\n".join(bodies))
    return (False, p)


def person_words(p):
    """None for an envelope with no message from a person; else (unwrapped?, text)."""
    t = p.lstrip()
    if not any(t.startswith(o) for o in ENVELOPES):
        return (False, p)
    owned, w = own_words(p)
    return (True, w) if owned else None


def named_sessions(prompt, own):
    b = prompt.encode("utf-8")
    out, i, n = [], 0, len(b)
    own = own.lower()
    while i + 36 <= n:
        boundary = i == 0 or b[i - 1] not in HEX
        after_ok = i + 36 >= n or (b[i + 36] not in HEX and b[i + 36] != 0x2D)
        if boundary and after_ok and GUID.match(b[i:i + 36]):
            gid = b[i:i + 36].decode("ascii").lower()
            if gid != own and gid not in out:
                out.append(gid)
            i += 36
            continue
        i += 1
    return out


def folder_key(cwd):
    t = cwd.strip()
    windows = "\\" in t or t.encode("utf-8")[1:2] == b":"
    k = t.replace("/", "\\").lower() if windows else t
    while len(k) > 1 and (k.endswith("\\") or k.endswith("/")) and not k.endswith(":\\"):
        k = k[:-1]
    return k


# ---------------------------------------------------------------- live prompts and turn ends
synced = "computer" in cols("event")
ec = "e.computer" if synced else "NULL"
sc = "s.computer" if synced else "NULL"
live_raw = con.execute(
    f"""SELECT e.event_id, e.surface, e.surface_session_id, e.ts_utc_ms, e.payload_meta, t.text, {ec}
        FROM event e LEFT JOIN content t ON t.event_id = e.event_id
        WHERE e.kind = 'prompt' AND e.ts_utc_ms >= ? AND (t.text IS NULL OR trim(t.text) != '')
        ORDER BY e.ts_utc_ms, e.event_id""",
    (W0,),
).fetchall()
known = {(s, i) for s, i in con.execute("SELECT surface, lower(surface_session_id) FROM surface_session")}
first, agent = set(), set()
for eid, surface, session, ts, meta, text, comp in live_raw:
    k = (surface, session)
    if k in first:
        continue
    first.add(k)
    names = named_sessions(text, session) if text is not None else []
    if any((surface, n) in known for n in names):
        agent.add(k)
kept = []
left = collections.Counter()
for eid, surface, session, ts, meta, text, comp in live_raw:
    if text is not None and written_by_tool(text):
        left["written by the tool"] += 1
        continue
    if text is not None and person_words(text) is None:
        left["envelope with no person's message"] += 1
        continue
    if (surface, session) in agent:
        left["a sub-agent's session"] += 1
        continue
    if text is not None:
        pw = person_words(text)
        if pw[0]:
            text = pw[1]
    try:
        folder = json.loads(meta).get("folder")
    except Exception:
        folder = None
    if not isinstance(folder, str):
        folder = None
    kept.append((eid, surface, session, ts, folder, text, comp))
stops = con.execute(
    f"""SELECT e.event_id, e.surface, e.surface_session_id, e.ts_utc_ms, {ec}
        FROM event e WHERE e.kind = 'stop' AND e.ts_utc_ms >= ? ORDER BY e.ts_utc_ms, e.event_id""",
    (W0,),
).fetchall()

# ---------------------------------------------------------------- imported prompts and turn ends
imp, imp_stops = [], []
if has_table("import_session") and has_table("import_event"):
    for tool, sid, seq, ts in con.execute(
        """SELECT e.tool, e.source_session_id, e.seq, e.ts_utc_ms
           FROM import_event e JOIN import_session s ON s.tool = e.tool AND s.source_session_id = e.source_session_id
           WHERE s.outcome = 'imported' AND e.kind = 'stop' AND e.ts_utc_ms IS NOT NULL AND e.ts_utc_ms >= ?
           ORDER BY e.ts_utc_ms, e.source_session_id, e.seq""",
        (W0,),
    ):
        imp_stops.append((f"import:{tool}:{sid}:{seq}:stop", tool, sid, ts))
    for tool, sid, folder, seq, ts, text, comp in con.execute(
        f"""SELECT s.tool, s.source_session_id, s.folder, e.seq, e.ts_utc_ms, e.text, {sc}
            FROM import_event e JOIN import_session s ON s.tool = e.tool AND s.source_session_id = e.source_session_id
            WHERE s.outcome = 'imported' AND e.kind = 'prompt' AND e.ts_utc_ms IS NOT NULL AND e.ts_utc_ms >= ?
              AND (e.text IS NULL OR trim(e.text) != '')
            ORDER BY e.ts_utc_ms, s.source_session_id, e.seq""",
        (W0,),
    ):
        imp.append((f"import:{tool}:{sid}:{seq}", tool, sid, folder_key(folder), ts, text, comp))


# ---------------------------------------------------------------- one session stored twice (twins.rs)
def family(s):
    return "copilot-cli" if s == "vscode" else s


def same_text(a, b):
    if a is None or b is None:
        return True
    return a.split() == b.split()


by_session = collections.defaultdict(list)
for j, r in enumerate(kept):
    by_session[(family(r[1]), r[2])].append(j)
fate = ["alone"] * len(imp)
extra_of = [None] * len(imp)
pairs = []
for i, p in enumerate(imp):
    same = by_session.get((p[1], p[2]))
    if not same:
        continue
    fate[i] = "extra"
    extra_of[i] = same[0]
    for j in same:
        d = abs(kept[j][3] - p[4])
        if d <= 3000 or (d <= 30000 and same_text(kept[j][5], p[5])):
            pairs.append((d, i, j))
pairs.sort()
used = [False] * len(kept)
for d, i, j in pairs:
    if not used[j] and fate[i] != "twin":
        used[j] = True
        fate[i] = "twin"

stream = [(r[0], r[1], r[2], r[3], r[4]) for r in kept]
computer_of = {r[0]: r[6] for r in kept}
computer_of.update({p[0]: p[6] for p in imp})
alone = []
for i, p in enumerate(imp):
    if fate[i] == "alone":
        alone.append(p)
    elif fate[i] == "extra":
        j = extra_of[i]
        f = kept[j][4] if kept[j][4] is not None else (p[3] if p[3] else None)
        stream.append((p[0], kept[j][1], kept[j][2], p[4], f))
stream.sort(key=lambda x: (x[3], x[0]))


# ---------------------------------------------------------------- the Task rule (reprompt.rs split_tasks)
def split_tasks(ps, span):
    open_, out = {}, []
    for ts, stop, eid, surface, session in ps:
        key = (surface, "" if surface in span else session)
        i = open_.get(key)
        if stop:
            if i is not None:
                out[i]["last"] = max(out[i]["last"], ts)
            continue
        if i is not None and not (ts - out[i]["last"] > GAP):
            t = out[i]
            t["last"] = max(t["last"], ts)
            t["prompts"] += 1
            t["members"].append(eid)
        else:
            open_[key] = len(out)
            out.append(dict(task_id=eid, tool=surface, session=session, first=ts, last=ts, prompts=1, members=[eid]))
    return out


folder_of = {x[0]: x[4] for x in stream}
prompts_in = collections.Counter((x[1], x[2]) for x in stream)
session_of = {x[0]: (x[1], x[2]) for x in stream}
by_comp = collections.defaultdict(list)
for eid, surface, session, ts, _f in stream:
    by_comp[computer_of.get(eid)].append((ts, False, eid, surface, session))
for eid, surface, session, ts, who in stops:
    by_comp[who].append((ts, True, eid, surface, session))
aside = set()
if has_table("folder_rule"):
    aside = {
        r[0]
        for r in con.execute(
            "SELECT folder FROM folder_rule r WHERE action = 'set_aside' AND rule_id = "
            "(SELECT MAX(rule_id) FROM folder_rule WHERE folder = r.folder)"
        )
    }
tasks = []
n_aside = 0
for who, ps in by_comp.items():
    ps.sort(key=lambda x: (x[0], x[1], x[2]))
    for t in split_tasks(ps, SPAN):
        f = folder_of.get(t["task_id"])
        if f is not None and f in aside and prompts_in.get(session_of[t["task_id"]], 0) < 2:
            n_aside += 1
            continue
        t.update(origin="live", computer=computer_of.get(t["task_id"]))
        tasks.append(t)
alone_sessions = {(p[1], p[2]) for p in alone}
aps = [(p[4], False, p[0], p[1], p[2]) for p in alone]
aps += [(ts, True, k, tool, sid) for (k, tool, sid, ts) in imp_stops if (tool, sid) in alone_sessions]
aps.sort(key=lambda x: (x[0], x[1], x[2]))
for t in split_tasks(aps, set()):
    t.update(origin="imported", computer=computer_of.get(t["task_id"]))
    tasks.append(t)
tasks = [t for t in tasks if t["first"] >= 0]
by_id = {t["task_id"]: t for t in tasks}

print("== rebuild of Home's task list ==")
print(f"  live prompts read {len(live_raw)}; left out as not typed: {dict(left)}; kept {len(kept)}")
print(f"  imported prompts {len(imp)}: {dict(collections.Counter(fate))}")
print(f"  one-prompt sessions in set-aside folders left out: {n_aside}; set-aside folders: {len(aside)}")
print(f"  tasks: {len(tasks)} (live {sum(1 for t in tasks if t['origin'] == 'live')}, imported {sum(1 for t in tasks if t['origin'] == 'imported')}); "
      f"this computer {sum(1 for t in tasks if t['computer'] is None)}, other computers {sum(1 for t in tasks if t['computer'] is not None)}")

# ---------------------------------------------------------------- current analyses
jcols = cols("judgment")
jcomp = "j.computer" if "computer" in jcols else "NULL"
J = {}
for r in con.execute(
    f"""SELECT j.task_id, j.provider, j.job, j.current_rung, j.state, j.read, j.task_type, j.task_type_confidence,
               j.evidence, j.judged_utc_ms, j.judge_version, {jcomp}, j.job_confidence
        FROM judgment j
        WHERE j.rowid = (SELECT x.rowid FROM judgment x WHERE x.task_id = j.task_id
                         ORDER BY x.judged_utc_ms DESC, x.rowid DESC LIMIT 1)"""
):
    try:
        read = json.loads(r[5])
    except Exception:
        read = {}
    J[r[0]] = dict(task_id=r[0], provider=r[1], job=r[2], rung=r[3], state=r[4], kind=r[6], kconf=r[7], ev=r[8],
                   judged=r[9], jv=r[10], jcomp=r[11], jconf=r[12], tr=read.get("type_runs"), jr=read.get("job_runs"),
                   keys=read.get("keys"), rprompts=read.get("prompts") or 0)

# ---------------------------------------------------------------- does the rebuild agree with AIQ?
print("== check of the rebuild against AIQ's own records ==")
for label, lo in (("analyzed in the last 24 h", NOW - DAY), ("analyzed in the last 3 days", NOW - 3 * DAY), ("analyzed in the last 7 days", NOW - 7 * DAY)):
    S = [j for j in J.values() if j["judged"] >= lo and j["jcomp"] is None]
    hit = sum(1 for j in S if j["task_id"] in by_id)
    print(f"  {label} on this computer: {len(S)} analyses, {hit} are tasks in the rebuild ({pct(hit, len(S))})")
K = [j for j in J.values() if isinstance(j["keys"], list) and j["task_id"] in by_id]
same = sum(1 for j in K if j["keys"] == by_id[j["task_id"]]["members"])
grew = sum(1 for j in K if j["keys"] != by_id[j["task_id"]]["members"] and set(j["keys"]) < set(by_id[j["task_id"]]["members"]))
print(f"  analyses that recorded which prompts they read: {len(K)}; same prompts as the rebuild {same}, rebuild has more (task grew) {grew}, other mismatch {len(K) - same - grew}")
Kall = [j for j in J.values() if isinstance(j["keys"], list)]
print(f"  analyses with recorded prompts whose task is not in the rebuild: {sum(1 for j in Kall if j['task_id'] not in by_id)} of {len(Kall)}")
own_closed = [t for t in tasks if t["computer"] is None and t["last"] + GAP < NOW]
noj = [t for t in own_closed if t["task_id"] not in J]
print(f"  this computer's finished tasks in the rebuild: {len(own_closed)}; with no analysis at all: {len(noj)} "
      f"(first prompt in the last day {sum(1 for t in noj if t['first'] >= NOW - DAY)}, last 7 days {sum(1 for t in noj if t['first'] >= NOW - 7 * DAY)}, by tool {dict(collections.Counter(t['tool'] for t in noj))})")
other_closed = [t for t in tasks if t["computer"] is not None and t["last"] + GAP < NOW]
print(f"  other computers' finished tasks in the rebuild: {len(other_closed)}; with no analysis here: {sum(1 for t in other_closed if t['task_id'] not in J)}")
orph = [j for j in J.values() if j["task_id"] not in by_id]
print(f"  analyses in the store: {len(J)}; for a task in the rebuild: {len(J) - len(orph)}; for a prompt that is no longer the start of a task: {len(orph)}")


# ---------------------------------------------------------------- Kinds over Home's list
def group(j):
    if j["job"] == "unclassified":
        return "no_job"
    if j["kind"] is None:
        return "kind_not_recorded"
    return "job_other" if j["kind"] == "other" else "named"


def cause(j):
    g = group(j)
    runs, field = (j["tr"], "task_type") if g == "job_other" else (j["jr"], "job")
    if g not in ("job_other", "no_job"):
        return g
    if not runs or len(runs) < 2:
        return "not_recorded"
    a, b = runs[0].get(field), runs[1].get(field)
    if a == "other" and b == "other":
        return "both_other"
    return "differ" if a != b else "agree_unsure"


print("== analyses for prompts that are no longer task starts (left out of everything below) ==")
print(f"  n={len(orph)}; {dict(collections.Counter(group(j) for j in orph))}; causes {dict(collections.Counter(cause(j) for j in orph if group(j) in ('job_other', 'no_job')))}")

H = []
for t in tasks:
    j = J.get(t["task_id"])
    if j:
        H.append(dict(t, **{k: v for k, v in j.items() if k != "task_id"}))
print(f"== Home's tasks with an analysis: {len(H)} of {len(tasks)} ==")
ORDER = ("named", "job_other", "no_job", "kind_not_recorded")


def line(S):
    g = collections.Counter(group(t) for t in S)
    return f"n={len(S)}; " + ", ".join(f"{k} {g[k]} ({pct(g[k], len(S))})" for k in ORDER if g[k])


windows = (("all time", 0), ("last 90 days", NOW - 90 * DAY), ("last 30 days", NOW - 30 * DAY), ("last 14 days", NOW - 14 * DAY), ("last 7 days", NOW - 7 * DAY))
for label, lo in windows:
    S = [t for t in H if t["first"] >= lo]
    print(f"  {label}: {line(S)}")
for label, lo in (("all time", 0), ("last 30 days", NOW - 30 * DAY)):
    S = [t for t in H if t["first"] >= lo]
    print(f"  {label} by computer: this one {line([t for t in S if t['computer'] is None])} | others {line([t for t in S if t['computer'] is not None])}")
    for tool in sorted({t["tool"] for t in S}):
        print(f"    {tool}: {line([t for t in S if t['tool'] == tool])}")
    print(f"    live {line([t for t in S if t['origin'] == 'live'])} | imported {line([t for t in S if t['origin'] == 'imported'])}")
    c = collections.Counter((group(t), cause(t)) for t in S if group(t) in ("job_other", "no_job"))
    print(f"    why no named kind: " + "; ".join(f"{k[0]}/{k[1]} {v} ({pct(v, len(S))})" for k, v in sorted(c.items(), key=lambda kv: -kv[1])))
print("== by job and kind (Home's tasks) ==")
for label, lo in (("all time", 0), ("last 30 days", NOW - 30 * DAY)):
    S = [t for t in H if t["first"] >= lo]
    print(f"  -- {label}, n={len(S)}")
    jobs = collections.Counter(t["job"] for t in S)
    for job, n in jobs.most_common():
        kinds = collections.Counter(t["kind"] for t in S if t["job"] == job)
        sure = collections.Counter(t["kind"] for t in S if t["job"] == job and (t["kconf"] or 0) >= 0.8 and (t["jconf"] or 0) >= 0.8)
        print(f"    {job} {n} ({pct(n, len(S))}): " + ", ".join(f"{k} {v} [{sure[k]} at 0.8+]" for k, v in kinds.most_common()))
print("== weekly share with a named kind (Home's tasks, by week of first prompt) ==")
wk = collections.defaultdict(list)
for t in H:
    wk[time.strftime("%G-W%V", time.gmtime(t["first"] / 1000))].append(t)
for w in sorted(wk)[-12:]:
    S = wk[w]
    g = collections.Counter(group(t) for t in S)
    print(f"  {w}: n={len(S)}; named {pct(g['named'], len(S))}, other {pct(g['job_other'], len(S))}, no job {pct(g['no_job'], len(S))}")
print("== rung and task length against no named kind (Home's tasks) ==")
for rung in (1, 2, 3, 4, 5):
    S = [t for t in H if t["rung"] == rung and t["state"] != "uncertain"]
    if S:
        print(f"  rung {rung} (sure): {line(S)}")
S = [t for t in H if t["state"] == "uncertain"]
print(f"  rung unsure: {line(S)}")
for lo, hi in ((1, 1), (2, 3), (4, 9), (10, 10_000)):
    S = [t for t in H if lo <= t["prompts"] <= hi]
    if S:
        print(f"  {lo}-{hi} prompts: {line(S)}")
print("== recurrence, last 30 days (coaching needs 3 tasks of a kind in 30 days) ==")
S30 = [t for t in H if t["first"] >= NOW - 30 * DAY]
named = collections.Counter((t["job"], t["kind"]) for t in S30 if group(t) == "named")
rec = {k for k, v in named.items() if v >= 3}
sure30 = [t for t in S30 if group(t) == "named" and (t["kconf"] or 0) >= 0.8 and (t["jconf"] or 0) >= 0.8]
print(f"  tasks {len(S30)}; named kinds seen {len(named)}, with 3+ tasks {len(rec)}; tasks in a recurring named kind {sum(v for k, v in named.items() if k in rec)} ({pct(sum(v for k, v in named.items() if k in rec), len(S30))})")
print(f"  named with job and kind both at 0.8 or more (sure enough to coach): {len(sure30)} ({pct(len(sure30), len(S30))})")
print("  top named kinds: " + ", ".join(f"{k[0]}/{k[1]} {v}" for k, v in named.most_common(12)))
print("== runs that agreed on a kind but were unsure (kind shown as Other): lower of the two confidences ==")
for label, lo in (("all time", 0), ("last 30 days", NOW - 30 * DAY)):
    S = [t for t in H if t["first"] >= lo and group(t) == "job_other" and cause(t) == "agree_unsure"]
    b = collections.Counter()
    kinds = collections.Counter()
    for t in S:
        m = min((r.get("task_type_confidence") or r.get("confidence") or 0) for r in t["tr"][:2])
        b["0.6 to 0.7" if m >= 0.6 else "0.5 to 0.6" if m >= 0.5 else "under 0.5"] += 1
        kinds[(t["job"], t["tr"][0].get("task_type"))] += 1
    print(f"  {label}: n={len(S)}; {dict(b)}; agreed kind: " + ", ".join(f"{k[0]}/{k[1]} {v}" for k, v in kinds.most_common(8)))
    if S:
        print(f"    keys seen in a run record: {sorted(S[0]['tr'][0].keys())}")
print("== runs that named different kinds (pairs, all time, Home's tasks) ==")
pairs_c = collections.Counter()
for t in H:
    if group(t) == "job_other" and cause(t) == "differ":
        a, b = sorted([str(t["tr"][0].get("task_type")), str(t["tr"][1].get("task_type"))])
        pairs_c[(t["job"], a, b)] += 1
print("  " + ", ".join(f"{k[0]}:{k[1]}/{k[2]} {v}" for k, v in pairs_c.most_common(14)))
pj = collections.Counter()
for t in H:
    if group(t) == "no_job" and t["jr"] and len(t["jr"]) >= 2:
        a, b = sorted([str(t["jr"][0].get("job")), str(t["jr"][1].get("job"))])
        pj[(a, b)] += 1
print("  no job, what the two runs said: " + ", ".join(f"{k[0]}+{k[1]} {v}" for k, v in pj.most_common(12)))
print("== position in the session (Home's tasks) ==")
sess = collections.defaultdict(list)
for t in H:
    sess[(t["tool"] if t["tool"] not in SPAN else "vscode", t["session"])].append(t)
pos1, later, inherit = [], [], collections.Counter()
for k, L in sess.items():
    L.sort(key=lambda t: t["first"])
    for i, t in enumerate(L):
        (pos1 if i == 0 else later).append(t)
        if i > 0 and group(t) in ("job_other", "no_job"):
            prev = L[i - 1]
            inherit["earlier task in the session had a named kind" if group(prev) == "named" else "earlier task had none either"] += 1
print(f"  first task of its session: {line(pos1)}")
print(f"  later task in a session: {line(later)}")
print(f"  later tasks with no named kind: {dict(inherit)}")
print("== kinds never or rarely used (Home's tasks, all time) ==")
LIST = {
    "code_change": ["fix_bug", "feature", "tests", "refactor", "build_fix", "deps", "review", "explain", "scaffold"],
    "write_doc": ["spec", "design_note", "status", "criteria", "research", "howto", "notes"],
    "ask_research": ["how_to", "explain", "troubleshoot", "compare", "find", "research", "analyse"],
    "run_ship": ["restart_fix", "release", "run_job", "setup"],
}
allc = collections.Counter((t["job"], t["kind"]) for t in H)
c30 = collections.Counter((t["job"], t["kind"]) for t in S30)
print("  " + ", ".join(f"{j}/{k} {allc[(j, k)]} ({c30[(j, k)]} in 30 d)" for j, ks in LIST.items() for k in ks if allc[(j, k)] < 10))

# ---------------------------------------------------------------- candidate rules already in the source (not live)
PACK = ["code_change", "write_doc", "ask_research", "run_ship"]


def cand_type(rule, job, runs):
    named = [r for r in runs if r.get("task_type") != "other" and r.get("task_type") in LIST.get(job, [])]
    if rule == "K1":
        if len(runs) < 2 or all(r.get("task_type") == runs[0].get("task_type") for r in runs) or not named:
            return None
        best = max(named, key=lambda r: r.get("confidence") or 0)
        ahead = all((best.get("confidence") or 0) >= (r.get("confidence") or 0) + 0.1 for r in runs if r.get("task_type") != best.get("task_type"))
        return best["task_type"] if (best.get("confidence") or 0) >= 0.7 and ahead else None
    if rule == "K2":
        ok = len(runs) >= 2 and len(named) == len(runs) and all(r["task_type"] == named[0]["task_type"] and (r.get("confidence") or 0) >= 0.5 for r in named)
        return named[0]["task_type"] if ok else None
    return None


def cand_job(rule, runs):
    packs = [r for r in runs if r.get("job") in PACK]
    if rule == "R1":
        if len(packs) == 1 and (packs[0].get("confidence") or 0) >= 0.7 and all(r.get("job") in (packs[0]["job"], "other") for r in runs):
            return packs[0]["job"]
        return None
    if rule == "R2":
        ok = len(runs) >= 2 and len(packs) == len(runs) and all(r["job"] == packs[0]["job"] and (r.get("confidence") or 0) >= 0.5 for r in packs)
        return packs[0]["job"] if ok else None
    return None


print("== what the rules already written (not live) would do on Home's tasks ==")
for label, lo in (("all time", 0), ("last 30 days", NOW - 30 * DAY)):
    S = [t for t in H if t["first"] >= lo]
    fo = [t for t in S if group(t) == "job_other" and t["tr"]]
    nj = [t for t in S if group(t) == "no_job" and t["jr"]]
    k1 = sum(1 for t in fo if cand_type("K1", t["job"], t["tr"]))
    k2 = sum(1 for t in fo if cand_type("K2", t["job"], t["tr"]))
    r1 = sum(1 for t in nj if cand_job("R1", t["jr"]))
    r2 = sum(1 for t in nj if cand_job("R2", t["jr"]))
    g = collections.Counter(group(t) for t in S)
    print(f"  {label} (n={len(S)}): Other-kind tasks with runs kept {len(fo)}: K1 names {k1}, K2 names {k2}; no-job tasks with runs kept {len(nj)}: R1 gives a job to {r1}, R2 to {r2}; named share now {pct(g['named'], len(S))}, with K2 {pct(g['named'] + k2, len(S))}, with K1+K2 {pct(g['named'] + k1 + k2, len(S))}")

# ---------------------------------------------------------------- second readings (aiq judge --check), ids only
if has_table("judge_check"):
    print("== second readings stored by the accuracy check (a blind second model; no task text) ==")
    jb = {}
    for r in con.execute("SELECT judgment_id, task_id, job, task_type, read, judge_version FROM judgment"):
        try:
            rd = json.loads(r[4])
        except Exception:
            rd = {}
        jb[r[0]] = dict(task_id=r[1], job=r[2], kind=r[3], tr=rd.get("type_runs"), jr=rd.get("job_runs"), jv=r[5])
    checks = con.execute("SELECT judgment_id, answer, grader_model, checked_utc_ms FROM judge_check WHERE outcome = 'ok' AND answer IS NOT NULL").fetchall()
    n_runs = con.execute("SELECT COUNT(*) FROM judge_check_run").fetchone()[0]
    C = []
    for jid, ans, model, at in checks:
        j = jb.get(jid)
        try:
            a = json.loads(ans)
        except Exception:
            continue
        if j:
            C.append((j, a, model, at))
    print(f"  check runs {n_runs}; second readings {len(C)}; distinct tasks {len({c[0]['task_id'] for c in C})}; of tasks still in Home's list {sum(1 for c in C if c[0]['task_id'] in by_id)}; "
          f"by reader model {dict(collections.Counter(c[2] for c in C))}; newest {int((NOW - max([c[3] for c in C] or [NOW])) / DAY)} d ago; by analysis version {dict(collections.Counter(c[0]['jv'] for c in C))}")
    jobhit = collections.Counter()
    for j, a, _, _ in C:
        jj = "other" if j["job"] == "unclassified" else j["job"]
        jobhit[(jj == a.get("job"))] += 1
    print(f"  job: same as the second reading {jobhit[True]} of {len(C)} ({pct(jobhit[True], len(C))})")
    rj = collections.Counter(a.get("job") for _, a, _, _ in C)
    print(f"  second reading's job: {dict(rj)}")
    for job in PACK:
        S = [a for _, a, _, _ in C if a.get("job") == job]
        ks = collections.Counter(a.get("task_type") for a in S)
        print(f"    reader kinds in {job} (n={len(S)}): " + ", ".join(f"{k} {v}" for k, v in ks.most_common()))
    M = [(j, a) for j, a, _, _ in C if j["kind"] is not None and j["job"] == a.get("job")]
    named_m = [(j, a) for j, a in M if j["kind"] != "other"]
    other_m = [(j, a) for j, a in M if j["kind"] == "other"]
    print(f"  kind, where the job matched: {sum(1 for j, a in M if j['kind'] == a.get('task_type'))} of {len(M)}; "
          f"analysis named a kind: {sum(1 for j, a in named_m if j['kind'] == a.get('task_type'))} of {len(named_m)} agreed; "
          f"analysis said Other: {len(other_m)}, second reading also other {sum(1 for j, a in other_m if a.get('task_type') == 'other')}")
    miss = collections.Counter((j["job"], j["kind"], a.get("task_type")) for j, a in named_m if j["kind"] != a.get("task_type"))
    print("  named kind read differently (job: analysis -> second reading): " + ", ".join(f"{k[0]}: {k[1]} -> {k[2]} x{v}" for k, v in miss.most_common(20)))
    lost = collections.Counter((j["job"], a.get("task_type")) for j, a in other_m)
    print("  analysis said Other, second reading said: " + ", ".join(f"{k[0]}/{k[1]} {v}" for k, v in lost.most_common(20)))
    for rule in ("K1", "K2"):
        W = [(j, a) for j, a in M if j["tr"]]
        before = sum(1 for j, a in W if j["kind"] == a.get("task_type"))
        given = [(j, a, cand_type(rule, j["job"], j["tr"])) for j, a in W if j["kind"] == "other"]
        given = [(j, a, g) for j, a, g in given if g]
        good = sum(1 for j, a, g in given if g == a.get("task_type"))
        print(f"  rule {rule}: checks with runs kept {len(W)}; agreement before {before}; rule names a kind for {len(given)}, second reading agrees with {good} of them; after {before + good} of {len(W)}")
    NJ = [(j, a) for j, a, _, _ in C if j["job"] == "unclassified"]
    print(f"  no-job analyses checked: {len(NJ)}; second reading's job {dict(collections.Counter(a.get('job') for j, a in NJ))}; its kinds {dict(collections.Counter((a.get('job'), a.get('task_type')) for j, a in NJ).most_common(12))}")
    for rule in ("R1", "R2"):
        given = [(j, a, cand_job(rule, j["jr"])) for j, a in NJ if j["jr"]]
        given = [(j, a, g) for j, a, g in given if g]
        print(f"  rule {rule}: gives a job to {len(given)} checked no-job analyses; second reading agrees {sum(1 for j, a, g in given if g == a.get('job'))}")

