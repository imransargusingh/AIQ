#!/usr/bin/env python3
"""Read-only: how often one session is stored twice (live `event` + imported `import_event`).

Prints counts only, never prompt text. Usage: python dupes.py <path to aiq.db>
"""
import json
import sqlite3
import sys
from collections import defaultdict

GAP = 30 * 60_000
MATCH_MS = 10_000
FAMILY = {"claude-code": "claude-code", "copilot-cli": "copilot-cli", "vscode": "copilot-cli"}


def norm(t):
    return " ".join(t.split()) if t else None


def split(prompts, spans):
    """prompts: (id, surface, session, ts) sorted by ts. Same rule as split_tasks."""
    open_, out = {}, []
    for pid, surface, session, ts in prompts:
        key = (surface, "" if surface in spans else session)
        i = open_.get(key)
        if i is not None and not ts - out[i]["last"] > GAP:
            out[i]["last"] = max(out[i]["last"], ts)
            out[i]["members"].append(pid)
        else:
            open_[key] = len(out)
            out.append({"id": pid, "surface": surface, "session": session, "first": ts, "last": ts, "members": [pid]})
    return out


def main(db):
    c = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    live = c.execute(
        "SELECT e.event_id, e.surface, e.surface_session_id, e.ts_utc_ms, t.text FROM event e"
        " LEFT JOIN content t ON t.event_id = e.event_id"
        " WHERE e.kind = 'prompt' AND (t.text IS NULL OR trim(t.text) != '') ORDER BY e.ts_utc_ms, e.event_id"
    ).fetchall()
    imp = c.execute(
        "SELECT s.tool, s.source_session_id, e.seq, e.ts_utc_ms, e.text FROM import_event e"
        " JOIN import_session s ON s.tool = e.tool AND s.source_session_id = e.source_session_id"
        " WHERE s.outcome = 'imported' AND e.kind = 'prompt' AND e.ts_utc_ms IS NOT NULL"
        " AND (e.text IS NULL OR trim(e.text) != '') ORDER BY e.ts_utc_ms, s.source_session_id, e.seq"
    ).fetchall()
    print(f"live prompts {len(live)}, imported prompts {len(imp)}")

    live_by = defaultdict(list)  # (family, sid) -> [(id, surface, ts, text)]
    for eid, surface, sid, ts, text in live:
        live_by[(FAMILY.get(surface, surface), sid)].append((eid, surface, ts, norm(text)))
    imp_by = defaultdict(list)
    for tool, sid, seq, ts, text in imp:
        imp_by[(tool, sid)].append((f"import:{tool}:{sid}:{seq}", ts, norm(text)))
    both = set(live_by) & set(imp_by)
    print(f"sessions: live {len(live_by)}, imported {len(imp_by)}, in both (same id) {len(both)}")
    pairs = defaultdict(int)
    for fam, sid in both:
        pairs[(tuple(sorted({s for _, s, _, _ in live_by[(fam, sid)]})), fam)] += 1
    print("  live surface(s) -> import tool:", {f"{'+'.join(k[0])}->{k[1]}": v for k, v in pairs.items()})

    # Prompt matching inside sessions held both ways: nearest unused live prompt within MATCH_MS.
    dt_hist = defaultdict(int)
    stat = defaultdict(int)
    extra = []  # imported prompts with no live twin, moved into the live stream
    for key in both:
        lv = live_by[key]
        used = set()
        for iid, ts, text in imp_by[key]:
            best = None
            for j, (eid, surface, lts, ltext) in enumerate(lv):
                if j in used or abs(lts - ts) > MATCH_MS:
                    continue
                if text and ltext and text != ltext:
                    stat["near_but_text_differs"] += 1
                    continue
                if best is None or abs(lts - ts) < abs(lv[best][2] - ts):
                    best = j
            if best is None:
                stat["imported_only"] += 1
                surface = lv[0][1]
                extra.append((iid, surface, key[1], ts))
            else:
                used.add(best)
                stat["matched"] += 1
                d = abs(lv[best][2] - ts)
                dt_hist["<=1s" if d <= 1000 else "<=3s" if d <= 3000 else "<=5s" if d <= 5000 else "<=10s"] += 1
                both_text = text is not None and lv[best][3] is not None
                stat["matched_text_equal" if both_text else "matched_one_text_missing"] += 1
        stat["live_only"] += len(lv) - len(used)
    print("prompts in shared sessions:", dict(stat))
    print("matched twin time gap:", dict(dt_hist))

    spans = {"vscode"}
    old_live = split([(e, s, sid, ts) for e, s, sid, ts, _ in live], spans)
    old_imp = split([(f"import:{t}:{sid}:{seq}", t, sid, ts) for t, sid, seq, ts, _ in imp], set())
    print(f"OLD tasks (no set-aside filter): {len(old_live) + len(old_imp)} = live {len(old_live)} + imported {len(old_imp)}")

    live_stream = sorted([(e, s, sid, ts) for e, s, sid, ts, _ in live] + extra, key=lambda p: (p[3], p[0]))
    new_live = split(live_stream, spans)
    rest = [(f"import:{t}:{sid}:{seq}", t, sid, ts) for t, sid, seq, ts, _ in imp if (t, sid) not in both]
    new_imp = split(rest, set())
    print(f"NEW tasks (no set-aside filter): {len(new_live) + len(new_imp)} = live/merged {len(new_live)} + imported only {len(new_imp)}")

    # Judgments: tasks judged twice (once per copy), and whether the two copies agree.
    j = {r[0]: r[1:] for r in c.execute(
        "SELECT task_id, job, task_type, current_rung, potential_rung FROM judgment j WHERE j.rowid ="
        " (SELECT x.rowid FROM judgment x WHERE x.task_id = j.task_id ORDER BY x.judged_utc_ms DESC, x.rowid DESC LIMIT 1)"
    )}
    old_ids = {t["id"] for t in old_live + old_imp}
    new_ids = {t["id"] for t in new_live + new_imp}
    # Map each old imported task in a shared session to the merged task that now holds its first prompt's twin.
    twin_of = {}
    for t in new_live:
        for m in t["members"]:
            twin_of[m] = t["id"]
    judged_old = sum(1 for i in old_ids if i in j)
    judged_new = sum(1 for i in new_ids if i in j)
    print(f"judged tasks: old {judged_old}, new {judged_new}, judgments no longer shown {judged_old - judged_new}")
    kinds = defaultdict(int)
    for t in old_imp:
        if (t["surface"], t["session"]) not in both or t["id"] not in j:
            continue
        # The live task overlapping this imported task in time, same session family.
        lt = [x for x in old_live if FAMILY.get(x["surface"], x["surface"]) == t["surface"] and x["session"] == t["session"]
              and x["first"] <= t["last"] + MATCH_MS and t["first"] <= x["last"] + MATCH_MS]
        lj = [j[x["id"]] for x in lt if x["id"] in j]
        if not lj:
            kinds["imported copy judged, live copy not"] += 1
            continue
        a, b = j[t["id"]], lj[0]
        kinds["both judged"] += 1
        kinds["same job"] += a[0] == b[0]
        kinds["same kind"] += a[1] == b[1]
        kinds["same current rung"] += a[2] == b[2]
    print("double judging:", dict(kinds))


if __name__ == "__main__":
    main(sys.argv[1])
