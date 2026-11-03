#!/usr/bin/env python3
"""Read-only follow-up: copies the 10 s / same-text / same-id rule might miss. Counts only, no prompt text.
Usage: python dupes2.py <path to aiq.db>
"""
import sqlite3
import sys
from collections import defaultdict

GAP = 30 * 60_000
FAMILY = {"vscode": "copilot-cli"}
DAY_START = 1793606400000  # 2026-11-02 00:00 PST (08:00Z)
DAY_END = DAY_START + 86_400_000


def norm(t):
    return " ".join(t.lower().split()) if t else None


def split(prompts, spans):
    open_, out = {}, []
    for pid, surface, session, ts in sorted(prompts, key=lambda p: (p[3], p[0])):
        key = (surface, "" if surface in spans else session)
        i = open_.get(key)
        if i is not None and not ts - out[i]["last"] > GAP:
            out[i]["last"] = ts
            out[i]["n"] += 1
        else:
            open_[key] = len(out)
            out.append({"id": pid, "surface": surface, "session": session, "first": ts, "last": ts, "n": 1})
    return out


def bucket(d):
    for lim, name in ((1_000, "<=1s"), (3_000, "<=3s"), (10_000, "<=10s"), (30_000, "<=30s"), (120_000, "<=2min")):
        if d <= lim:
            return name
    return ">2min"


def main(db):
    c = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    live = c.execute(
        "SELECT e.event_id, e.surface, e.surface_session_id, e.ts_utc_ms, t.text FROM event e"
        " LEFT JOIN content t ON t.event_id = e.event_id WHERE e.kind = 'prompt'"
        " AND (t.text IS NULL OR trim(t.text) != '')"
    ).fetchall()
    imp = c.execute(
        "SELECT s.tool, s.source_session_id, e.seq, e.ts_utc_ms, e.text, s.outcome FROM import_event e"
        " JOIN import_session s ON s.tool = e.tool AND s.source_session_id = e.source_session_id"
        " WHERE e.kind = 'prompt' AND e.ts_utc_ms IS NOT NULL AND (e.text IS NULL OR trim(e.text) != '')"
    ).fetchall()
    imp_kept = [r for r in imp if r[5] == "imported"]
    print(f"live prompts {len(live)} (text missing {sum(1 for r in live if r[4] is None)}); "
          f"imported prompts kept {len(imp_kept)}, in set-aside imported sessions {len(imp) - len(imp_kept)}")
    live_ids = {(FAMILY.get(s, s), sid) for _, s, sid, _, _ in live}

    # A. Every live prompt: nearest imported prompt (any session, kept or set aside) within 2 min.
    by_time = sorted(imp, key=lambda r: r[3])
    times = [r[3] for r in by_time]
    import bisect
    a = defaultdict(int)
    other_sessions = set()
    for eid, s, sid, ts, text in live:
        lo, hi = bisect.bisect_left(times, ts - 120_000), bisect.bisect_right(times, ts + 120_000)
        cands = by_time[lo:hi]
        if not cands:
            a["no imported prompt within 2 min"] += 1
            continue
        best = min(cands, key=lambda r: abs(r[3] - ts))
        same_id = (best[0], best[1]) == (FAMILY.get(s, s), sid)
        same_text = text is None or best[4] is None or norm(text) == norm(best[4])
        key = f"{'same id' if same_id else 'OTHER id'} | {'same text' if same_text else 'text differs'} | {bucket(abs(best[3] - ts))} | {best[5]}"
        a[key] += 1
        if not same_id and same_text:
            other_sessions.add((s, sid, best[0], best[1]))
    print("A. live prompt -> nearest imported prompt:")
    for k in sorted(a):
        print(f"   {a[k]:5d}  {k}")
    print(f"   live/imported session pairs linked only by time+text, different ids: {len(other_sessions)}")

    # B. Day 2026-11-02 PST, tasks starting that day, three ways.
    lp = [(e, s, sid, ts) for e, s, sid, ts, _ in live]
    ip = [(f"import:{t}:{sid}:{seq}", t, sid, ts) for t, sid, seq, ts, _, o in imp_kept]
    day = lambda ts: [t for t in ts if DAY_START <= t["first"] < DAY_END]
    l_span, l_nospan, i_all = day(split(lp, {"vscode"})), day(split(lp, set())), day(split(ip, set()))
    i_shared = [t for t in i_all if (t["surface"], t["session"]) in live_ids]
    print(f"B. 2026-11-02 PST tasks: live {len(l_span)} (vscode spanning, as AIQ) / {len(l_nospan)} (split per session); "
          f"imported {len(i_all)}, of them in a session also held live {len(i_shared)}")
    print(f"   live prompts that day {sum(1 for p in lp if DAY_START <= p[3] < DAY_END)}, "
          f"imported prompts that day {sum(1 for p in ip if DAY_START <= p[3] < DAY_END)}")
    per_tool = defaultdict(int)
    for t in l_nospan:
        per_tool["live " + t["surface"]] += 1
    for t in i_all:
        per_tool["imported " + t["surface"]] += 1
    print("   by tool:", dict(per_tool))


if __name__ == "__main__":
    main(sys.argv[1])
