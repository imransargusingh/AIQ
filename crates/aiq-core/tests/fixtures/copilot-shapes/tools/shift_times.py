#!/usr/bin/env python3
"""Move every timestamp in a shape fixture so the file's first one is 2026-01-01T00:00:00Z, keeping the gaps.

usage: shift_times.py <fixture.jsonl>...   (rewrites each file in place)
"""
import datetime as dt
import json
import re
import sys

TS = re.compile(r"^\d{4}-\d\d-\d\dT[\d:.]+Z$")
EPOCH_KEY = re.compile(r"Time$")  # epoch-millisecond numbers, moved with the timestamps
BASE = dt.datetime(2026, 1, 1, tzinfo=dt.timezone.utc)


def parse(s):
    return dt.datetime.fromisoformat(s.replace("Z", "+00:00"))


def times(v, out):
    if isinstance(v, dict):
        for x in v.values():
            times(x, out)
    elif isinstance(v, list):
        for x in v:
            times(x, out)
    elif isinstance(v, str) and TS.match(v):
        out.append(parse(v))


def shift(v, delta, key=None):
    if isinstance(v, dict):
        return {k: shift(x, delta, k) for k, x in v.items()}
    if isinstance(v, int) and not isinstance(v, bool) and key and EPOCH_KEY.search(key) and v > 10**12:
        return v - int(delta.total_seconds() * 1000)
    if isinstance(v, list):
        return [shift(x, delta, key) for x in v]
    if isinstance(v, str) and TS.match(v):
        return (parse(v) - delta).isoformat(timespec="milliseconds").replace("+00:00", "Z")
    return v


for path in sys.argv[1:]:
    rows = [json.loads(l) for l in open(path, encoding="utf-8")]
    found = []
    for r in rows:
        times(r, found)
    delta = min(found) - BASE
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(json.dumps(shift(r, delta), separators=(",", ":")) for r in rows) + "\n")
    print(path, len(found), "timestamps moved")
