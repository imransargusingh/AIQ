#!/usr/bin/env python3
"""Score the guarded hybrid (rung-model spec Q8) against the plain holistic judge, offline.

  python3 prototypes/phase0-spike/score_hybrid.py \
      --facts prototypes/phase0-spike/results/facts-v2-judgments.jsonl \
      --holistic prototypes/phase0-spike/results/dev-judgments.jsonl \
      --holistic prototypes/phase0-spike/results/heldout-judgments.jsonl \
      --labels-dir /tmp/aiq-facts2

Pairs each holistic judgment (Sonnet, excerpt mode) with the fact reading of the same task and run,
then reports, per gold rung: holistic, rules-only (v2) and guarded hybrid, at two fact thresholds.
"""
import argparse
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from aiq_spike import facts  # noqa: E402


def per_rung(pairs):
    by = collections.defaultdict(lambda: [0, 0, 0])
    for gold, pred in pairs:
        b = by[gold]
        b[0] += pred == gold
        b[1] += abs(pred - gold) <= 1
        b[2] += 1
    tot = [sum(v[i] for v in by.values()) for i in range(3)]
    out = {str(k): "{0}/{2} exact, {1}/{2} within 1".format(*v) for k, v in sorted(by.items())}
    out["all"] = "exact {0:.2f}, within 1 {1:.2f} (n={2})".format(tot[0] / tot[2], tot[1] / tot[2], tot[2])
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--facts", required=True)
    ap.add_argument("--holistic", action="append", required=True)
    ap.add_argument("--labels-dir", required=True, help="folder with dev/ and heldout/ labels.json")
    ap.add_argument("--model", default="claude-sonnet-5-5")
    ap.add_argument("--out")
    a = ap.parse_args()
    gold = {c: json.load(open(os.path.join(a.labels_dir, c, "labels.json"), encoding="utf-8"))["tasks"] for c in ("dev", "heldout")}
    hol = {}
    for p in a.holistic:
        for line in open(p, encoding="utf-8"):
            r = json.loads(line)
            if r["endpoint"].endswith(a.model) and r["mode"] == "excerpt" and r["out"]:
                hol[(r["corpus"], r["task"], r["rep"])] = r["out"]["current_rung"]
    fx = [json.loads(x) for x in open(a.facts, encoding="utf-8")]
    fx = [r for r in fx if r["endpoint"].endswith(a.model) and r["model"]]
    res = {}
    for th in (0.5, 0.7):
        rows = collections.defaultdict(list)
        lowered_correct = fixed_overcall = lowered_total = 0
        for r in fx:
            key = (r["corpus"], r["task"], r["rep"])
            if key not in hol:
                continue
            g = gold[r["corpus"]][r["task"]]["rung"]
            rule_rung, holds = facts.rules_v2(r["log"], r["model"], th=th)
            h = hol[key]
            gr = facts.guarded(h, holds)
            rows["holistic"].append((g, h))
            rows["rules only (v2)"].append((g, rule_rung))
            rows["guarded hybrid"].append((g, gr))
            rows["guarded hybrid + fact floor for 4-5"].append((g, facts.guarded_with_floor(h, holds)))
            if gr < h:
                lowered_total += 1
                lowered_correct += h == g
                fixed_overcall += h > g and abs(gr - g) < abs(h - g)
        res["fact threshold {0}".format(th)] = {
            **{k: per_rung(v) for k, v in rows.items()},
            "guardrail lowered the rung": lowered_total,
            "of which the model was right (guardrail made it wrong)": lowered_correct,
            "of which the model overcalled (guardrail improved it)": fixed_overcall,
        }
    if a.out:
        json.dump(res, open(a.out, "w", encoding="utf-8"), indent=1)
    print(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()
