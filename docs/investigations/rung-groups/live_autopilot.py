"""Read-only, counts only: do Copilot's autopilot turns reach AIQ as live prompts, and is Copilot's own record of
them in its session log by the time the hook reaches AIQ?

For every live Copilot CLI and VS Code prompt AIQ recorded (event + content), it looks in the session's log
(<copilot home>/session-state/<session>/events.jsonl) for main-line `user.message` records with the same text
(whitespace trimmed) and takes the one nearest in time. It counts: matched or not; how many of the matched carry
`isAutopilotContinuation: true` (autopilot sent them, yet AIQ counted them); and for those, how long after Copilot
wrote the record the core received the hook (negative = the hook came first), so a lookup at hook time can be
judged. It also counts the autopilot records in the same sessions that never reached AIQ live, and how many
distinct texts the autopilot records hold (by length band, never the text).
Nothing else is printed: no prompt, id, folder or path.

Usage:
    python3 -I live_autopilot.py <path to aiq.db> <copilot home, e.g. %USERPROFILE%\\.copilot>
"""
import collections
import datetime
import hashlib
import json
import os
import re
import sqlite3
import sys

DB, HOME = sys.argv[1], sys.argv[2]
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")
SAFE_ID = re.compile(r"^[A-Za-z0-9_-]{1,128}$")


def ms(t):
    try:
        return int(datetime.datetime.fromisoformat(str(t).replace("Z", "+00:00")).timestamp() * 1000)
    except ValueError:
        return None


live = collections.defaultdict(list)  # session -> [(text, ts, received)]
for surface, sid, ts, received, text in con.execute(
        """SELECT e.surface, e.surface_session_id, e.ts_utc_ms, e.received_utc_ms, c.text
           FROM event e JOIN content c ON c.event_id = e.event_id
           WHERE e.kind = 'prompt' AND e.surface IN ('copilot-cli', 'vscode')
             AND (e.computer IS NULL)"""):
    live[sid].append((text.strip(), ts, received, surface))

c = collections.Counter()
lag = collections.Counter()
texts = collections.Counter()
text_sessions = collections.defaultdict(set)
for sid, prompts in live.items():
    path = os.path.join(HOME, "session-state", sid, "events.jsonl")
    if not SAFE_ID.match(sid) or not os.path.isfile(path):
        c["live prompts whose session log is gone"] += len(prompts)
        continue
    records = []
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                try:
                    e = json.loads(line)
                except ValueError:
                    continue
                if e.get("type") != "user.message" or e.get("agentId"):
                    continue
                d = e.get("data") or {}
                records.append(((d.get("content") or "").strip(), ms(e.get("timestamp")),
                                d.get("isAutopilotContinuation") is True))
    except OSError:
        c["live prompts whose session log is unreadable"] += len(prompts)
        continue
    used = set()
    for text, ts, received, surface in prompts:
        best = None
        for i, (rt, rts, auto) in enumerate(records):
            if rt != text or i in used or rts is None:
                continue
            if best is None or abs(rts - ts) < abs(records[best][1] - ts):
                best = i
        if best is None:
            c[f"{surface}: live prompts with no matching log record"] += 1
            continue
        used.add(best)
        if records[best][2]:
            c[f"{surface}: live prompts that autopilot sent (counted as typed)"] += 1
            d = received - records[best][1]
            lag[("hook received before the record's time" if d < 0 else
                 "0-1 s after the record" if d < 1000 else "1-5 s after" if d < 5000 else "over 5 s after")] += 1
        else:
            c[f"{surface}: live prompts the person typed (matched)"] += 1
    for i, (rt, rts, auto) in enumerate(records):
        if auto:
            h = hashlib.sha256(rt.encode()).hexdigest()[:12]
            texts[(h, len(rt))] += 1
            text_sessions[(h, len(rt))].add(sid)
            if i not in used:
                c["autopilot records in these sessions that never reached AIQ live"] += 1
print(f"live Copilot sessions: {len(live)}")
for k, v in sorted(c.items()):
    print(f"  {k}: {v}")
print("autopilot prompts AIQ counted, by when the hook reached the core:")
for k, v in sorted(lag.items()):
    print(f"  {k}: {v}")
print(f"distinct autopilot texts in these sessions: {len(texts)}")
for (h, n), k in sorted(texts.items(), key=lambda x: -x[1])[:5]:
    print(f"  text {h} ({n} chars): {k} records in {len(text_sessions[(h, n)])} sessions")
