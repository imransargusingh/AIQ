"""Read-only, counts only: what would change if AIQ stopped counting Copilot's autopilot continuations as typed?

Copilot CLI's autopilot mode adds follow-up turns by itself. Each one is a main-line `user.message` with
`data.isAutopilotContinuation: true` (copilot_launch_signs.py, 2026-11-15: 30 such messages in a program's 29
sessions, 55 in sessions the person drove). AIQ's importer counts them as typed prompts, so a program's
one-prompt sessions pass as multi-prompt work and escape the set-aside rule. This measures, before any code
changes:
  1. the continuation messages: how many, in which group, their delivery, length and how many distinct texts;
  2. the live path: is there a live Copilot prompt within 3 s of each continuation (did the live hook count it?),
     with typed messages as the baseline;
  3. the fix, replayed on the store: sessions whose typed count drops, how many fall to one typed prompt, and how
     many of those the import rule would then set aside (T template, B burst, F folder prior; as import.rs);
  4. the cost: analyzed tasks whose id would change (task ids carry the prompt's position in the session).

Groups: app (first prompt starts with the opening given on the command line), typed (other imported), aside
(already set aside). Nothing else is printed: no prompt, folder, id or path.

Usage:
    python3 -I copilot_continuations.py <aiq.db> <copilot home, e.g. %USERPROFILE%\\.copilot> "<opening of the app's prompt>"
"""
import collections
import hashlib
import json
import os
import sys
from datetime import datetime

DB, HOME, OPENING = sys.argv[1], sys.argv[2], sys.argv[3].strip().lower()
import sqlite3

con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")
TEMPLATE_SESSIONS, BURST_SESSIONS, BURST_MS, FOLDER_PRIOR = 11, 50, 30 * 60_000, 0.8


def folder_key(cwd):
    t = (cwd or "").strip()
    windows = "\\" in t or t[1:2] == ":"
    k = t.replace("/", "\\").lower() if windows else t
    while len(k) > 1 and k[-1] in "\\/" and not k.endswith(":\\"):
        k = k[:-1]
    return k


def ts_ms(s):
    try:
        return int(datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp() * 1000)
    except (AttributeError, ValueError):
        return None


include = {folder_key(f) for f, a in con.execute(
    "SELECT folder, action FROM folder_rule r WHERE rule_id = (SELECT MAX(rule_id) FROM folder_rule WHERE folder = r.folder)")
    if a == "include"}
sess = {}
for sid, folder, started, turns, tkey, outcome in con.execute(
        "SELECT source_session_id, folder, started_utc_ms, human_turns, template_key, outcome FROM import_session "
        "WHERE tool = 'copilot-cli'"):
    if outcome != "imported":
        g = "aside"
    else:
        first = con.execute("SELECT text FROM import_event WHERE tool = 'copilot-cli' AND source_session_id = ? "
                            "AND kind = 'prompt' ORDER BY seq LIMIT 1", (sid,)).fetchone()
        g = "app" if first and first[0] and first[0].strip().lower().startswith(OPENING) else "typed"
    sess[sid.lower()] = dict(sid=sid, folder=folder or "", started=started, turns=turns or 0, tkey=tkey, g=g)
print(f"imported Copilot sessions by group {dict(sorted(collections.Counter(s['g'] for s in sess.values()).items()))}")

live = collections.defaultdict(list)
for sid, ts in con.execute("SELECT surface_session_id, ts_utc_ms FROM event WHERE surface = 'copilot-cli' AND kind = 'prompt'"):
    live[sid.lower()].append(ts)

msgs = collections.Counter()  # (group, what) -> n
delivery = collections.defaultdict(collections.Counter)
length = collections.defaultdict(collections.Counter)
texts = collections.defaultdict(set)
live_hit = collections.Counter()  # (kind, hit/miss) only in sessions that have live prompts
cont_at = {}  # sid -> typed positions (0-based among typed messages) that are continuations
typed_n = {}
state = os.path.join(HOME, "session-state")
for d in os.listdir(state) if os.path.isdir(state) else []:
    s = sess.get(d.lower())
    path = os.path.join(state, d, "events.jsonl")
    if s is None or not os.path.isfile(path):
        continue
    g, pos, conts = s["g"], 0, []
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                try:
                    e = json.loads(line)
                except ValueError:
                    continue
                if e.get("type") != "user.message" or e.get("agentId"):
                    continue
                data = e.get("data") or {}
                if data.get("delivery") not in ("idle", "steering"):
                    continue  # what AIQ's importer counts as typed today
                cont = data.get("isAutopilotContinuation") is True
                kind = "continuation" if cont else "typed"
                msgs[(g, kind)] += 1
                if cont:
                    conts.append(pos)
                    delivery[g][str(data.get("delivery"))] += 1
                    n = len(data.get("content") or "")
                    length[g]["<150" if n < 150 else ">=150"] += 1
                    texts[g].add(hashlib.sha256((data.get("content") or "").encode()).hexdigest())
                t = ts_ms(e.get("timestamp"))
                lv = live.get(d.lower())
                if lv and t is not None:
                    live_hit[(kind, "live prompt within 3 s" if any(abs(x - t) <= 3000 for x in lv) else "none")] += 1
                pos += 1
    except OSError:
        continue
    typed_n[d.lower()] = pos
    if conts:
        cont_at[d.lower()] = conts

print("\n== 1. messages AIQ counts as typed today, by group ==")
for g in ("app", "typed", "aside"):
    print(f"  {g}: typed {msgs[(g, 'typed')]}, continuations {msgs[(g, 'continuation')]}, "
          f"in {sum(1 for k in cont_at if sess[k]['g'] == g)} sessions; delivery {dict(delivery[g])}; "
          f"length {dict(length[g])}; distinct texts {len(texts[g])}")

print("\n== 2. live path (sessions that also have live Copilot prompts) ==")
for kind in ("typed", "continuation"):
    print(f"  {kind}: {dict((h, n) for (k, h), n in live_hit.items() if k == kind)}")

# 3. Replay the import rule with continuations left out.
groups = collections.Counter((folder_key(s["folder"]), s["tkey"]) for s in sess.values() if s["tkey"] is not None)
starts = collections.defaultdict(list)
for s in sess.values():
    if s["started"] is not None:
        starts[folder_key(s["folder"])].append(s["started"])
for v in starts.values():
    v.sort()


def new_turns(k, s):
    return max(0, typed_n.get(k, s["turns"]) - len(cont_at.get(k, [])))


def signals(k, s, turns):
    if folder_key(s["folder"]) in include or turns > 1:
        return []
    w = []
    if s["tkey"] is not None and groups[(folder_key(s["folder"]), s["tkey"])] >= TEMPLATE_SESSIONS:
        w.append("T")
    v = starts.get(folder_key(s["folder"]), [])
    if s["started"] is not None and sum(1 for x in v if abs(x - s["started"]) <= BURST_MS) >= BURST_SESSIONS:
        w.append("B")
    return w


after = {k: (new_turns(k, s), signals(k, s, new_turns(k, s))) for k, s in sess.items() if s["g"] != "aside"}
share = collections.defaultdict(lambda: [0, 0])
for k, s in sess.items():
    f = folder_key(s["folder"])
    share[f][0] += 1
    share[f][1] += 1 if s["g"] == "aside" or after[k][1] else 0
print("\n== 3. the fix replayed (imported sessions only; aside stays aside) ==")
out = collections.Counter()
for k, (turns, w) in after.items():
    s = sess[k]
    if k not in cont_at:
        continue
    f = folder_key(s["folder"])
    n, a = share[f]
    if not w and turns <= 1 and f not in include and a / n > FOLDER_PRIOR:
        w = ["F"]
    out[(s["g"], "sessions with continuations")] += 1
    out[(s["g"], "fall to one typed prompt")] += 1 if turns <= 1 else 0
    out[(s["g"], "fall to no typed prompt")] += 1 if turns == 0 else 0
    out[(s["g"], "would be set aside " + ",".join(w))] += 1 if w else 0
for g in ("app", "typed"):
    print(f"  {g}: {dict((w, n) for (gg, w), n in sorted(out.items()) if gg == g)}")
apps = [s for s in sess.values() if s["g"] == "app"]
print(f"  app sessions: {len(apps)} in {len({folder_key(s['folder']) for s in apps})} folder(s); "
      f"largest same-template group in one folder {max((groups[(folder_key(s['folder']), s['tkey'])] for s in apps if s['tkey'] is not None), default=0)}"
      f" (T needs {TEMPLATE_SESSIONS})")

print("\n== 4. analyzed tasks whose id would change (they are analyzed again) ==")
moved = collections.Counter()
for k, conts in cont_at.items():
    s = sess[k]
    if s["g"] == "aside":
        continue
    first = conts[0] * 2  # import_event seq: prompt and stop rows alternate
    for (tid,) in con.execute("SELECT DISTINCT task_id FROM judgment WHERE task_id LIKE ?",
                              (f"import:copilot-cli:{s['sid']}:%",)):
        try:
            seq = int(tid.rsplit(":", 1)[1])
        except ValueError:
            continue
        moved[(s["g"], "changes" if seq >= first else "same id")] += 1
print(f"  {dict(sorted(moved.items()))}")
