#!/usr/bin/env python3
"""Phase 0 feasibility spike: generate the synthetic corpus, group, judge, and measure.

  python3 prototypes/phase0-spike/run_spike.py --out /tmp/aiq-spike \
      --endpoint claude-cli:claude-haiku-5-5 --endpoint claude-cli:claude-sonnet-5-5 --reps 2

Writes <out>/corpus/ (synthetic session stores + labels.json) and results.json; prints a summary.
Judging runs on the GOLD task grouping so judge accuracy is measured apart from grouping errors.
Only synthetic data is read: the corpus lives under <out>, not under ~/.claude or ~/.copilot.
"""
import argparse
import collections
import concurrent.futures as cf
import json
import os
import statistics
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from aiq_spike import generate, group, heldout, judge, llm  # noqa: E402
from aiq_spike.normalize import load_turns  # noqa: E402

ALPHA = ("code_change", "write_doc")


# --------------------------------------------------------------------------- grouping metrics
def grouping_metrics(pred, gold):
    """B-cubed precision/recall over turns, exact-task recovery, splits and merges."""
    keys = list(gold)
    pc, gc = collections.defaultdict(set), collections.defaultdict(set)
    for k in keys:
        pc[pred[k]].add(k)
        gc[gold[k]].add(k)
    p = statistics.mean(len(pc[pred[k]] & gc[gold[k]]) / len(pc[pred[k]]) for k in keys)
    r = statistics.mean(len(pc[pred[k]] & gc[gold[k]]) / len(gc[gold[k]]) for k in keys)
    exact = sum(1 for g in gc.values() if any(g == c for c in pc.values()))
    split = sorted(t for t, g in gc.items() if len({pred[k] for k in g}) > 1)
    merged = sorted({gold[k] for c in pc.values() if len({gold[x] for x in c}) > 1 for k in c})
    return {"bcubed_precision": round(p, 3), "bcubed_recall": round(r, 3),
            "bcubed_f1": round(2 * p * r / (p + r), 3), "tasks_recovered_exactly": exact,
            "tasks_total": len(gc), "gold_tasks_split": split, "gold_tasks_merged": merged}


# --------------------------------------------------------------------------- judge metrics
def judge_metrics(rows, tasks, thresholds=(0.0, 0.6, 0.7, 0.8, 0.9)):
    ok = [r for r in rows if r["out"]]
    m = {"calls": len(rows), "failed_calls": len(rows) - len(ok)}
    if not ok:
        return m
    gold = lambda r, k: tasks[r["task"]][k]  # noqa: E731
    m["job_accuracy"] = round(sum(r["out"]["job"] == gold(r, "job") for r in ok) / len(ok), 3)
    unamb = [r for r in ok if not tasks[r["task"]]["ambiguous"]]
    m["job_accuracy_unambiguous"] = round(sum(r["out"]["job"] == gold(r, "job") for r in unamb) / len(unamb), 3)
    # product rule: Unclassified when job is 'other' or confidence below the pack threshold
    th = {}
    for t in thresholds:
        lab = [(r, r["out"]["job"] if (r["out"]["job"] in ALPHA and r["out"]["job_confidence"] >= t) else "unclassified") for r in ok]
        alpha_gold = [x for x in lab if gold(x[0], "job") in ALPHA]
        classified = [x for x in lab if x[1] != "unclassified"]
        th[str(t)] = {
            "coverage_of_alpha_tasks": round(sum(x[1] == gold(x[0], "job") for x in alpha_gold) / max(1, len(alpha_gold)), 3),
            "precision_when_classified": round(sum(x[1] == gold(x[0], "job") for x in classified) / max(1, len(classified)), 3),
            "other_jobs_leaking_into_alpha_packs": sum(1 for x in lab if gold(x[0], "job") == "other" and x[1] != "unclassified"),
        }
    m["unclassified_rule"] = th
    cur = [r["out"]["current_rung"] - gold(r, "rung") for r in ok]
    pot = [r["out"]["potential_rung"] - gold(r, "potential") for r in ok]
    m["rung_exact"] = round(sum(d == 0 for d in cur) / len(ok), 3)
    m["rung_within_1"] = round(sum(abs(d) <= 1 for d in cur) / len(ok), 3)
    m["rung_mean_signed_error"] = round(statistics.mean(cur), 3)
    m["potential_exact"] = round(sum(d == 0 for d in pot) / len(ok), 3)
    m["potential_within_1"] = round(sum(abs(d) <= 1 for d in pot) / len(ok), 3)
    # what coaching consumes: is there a next rung to coach towards?
    tp = sum(1 for r in ok if r["out"]["potential_rung"] > r["out"]["current_rung"] and gold(r, "potential") > gold(r, "rung"))
    fp = sum(1 for r in ok if r["out"]["potential_rung"] > r["out"]["current_rung"] and not gold(r, "potential") > gold(r, "rung"))
    fn = sum(1 for r in ok if not r["out"]["potential_rung"] > r["out"]["current_rung"] and gold(r, "potential") > gold(r, "rung"))
    m["opportunity_precision"] = round(tp / max(1, tp + fp), 3)
    m["opportunity_recall"] = round(tp / max(1, tp + fn), 3)
    conf = collections.Counter((gold(r, "rung"), r["out"]["current_rung"]) for r in ok)
    m["rung_confusion_gold_to_pred"] = {"{0}->{1}".format(*k): v for k, v in sorted(conf.items())}
    lat = sorted(r["meta"]["latency_ms"] for r in rows if r["meta"].get("latency_ms") is not None)
    m["latency_ms_p50"] = lat[len(lat) // 2] if lat else None
    m["latency_ms_p95"] = lat[min(len(lat) - 1, int(0.95 * len(lat)))] if lat else None
    toks = [r["meta"]["input_tokens"] for r in rows if r["meta"].get("input_tokens")]
    m["input_tokens_mean"] = int(statistics.mean(toks)) if toks else None
    cost = [r["meta"]["cost_usd"] for r in rows if r["meta"].get("cost_usd") is not None]
    m["cost_usd_per_task_mean"] = round(statistics.mean(cost), 5) if cost else None
    return m


def consistency(rows):
    by = collections.defaultdict(list)
    for r in rows:
        if r["out"]:
            by[r["task"]].append(r["out"])
    pairs = [v[:2] for v in by.values() if len(v) >= 2]
    if not pairs:
        return None
    same = lambda k: round(sum(a[k] == b[k] for a, b in pairs) / len(pairs), 3)  # noqa: E731
    return {"tasks": len(pairs), "job_same": same("job"), "rung_same": same("current_rung"),
            "potential_same": same("potential_rung")}


def evaluate_corpus(name, a, endpoints, scenarios=None, plan=None):
    corpus = os.path.join(a.out, name)
    generate.main(corpus, scenarios=scenarios, plan=plan)
    turns = load_turns(generate.REPO, os.path.join(corpus, "claude_home"), os.path.join(corpus, "copilot_home"))
    labels = json.load(open(os.path.join(corpus, "labels.json"), encoding="utf-8"))
    gold = {k: labels["turns"][k] for k in (t["key"] for t in turns)}
    tasks = labels["tasks"]
    if len(gold) != len(labels["turns"]):
        raise SystemExit("reader lost turns: {0} of {1}".format(len(gold), len(labels["turns"])))
    results = {"corpus": {"sessions": len({t["session"] for t in turns}), "tasks": len(tasks), "turns": len(turns),
                          "jobs": dict(collections.Counter(t["job"] for t in tasks.values())),
                          "rungs": dict(collections.Counter(t["rung"] for t in tasks.values()))},
               "grouping": {}, "judge": {}}

    # time-gap-only baseline: the naive rule the heuristic must beat
    base, last, n = {}, None, 0
    for t in turns:
        if last is None or t["session"] != last["session"] or (group._ts(t["start"]) - group._ts(last["end"])).total_seconds() > 300:
            n += 1
        base[t["key"]], last = "b{0}".format(n), t
    results["grouping"]["baseline_5min_gap"] = grouping_metrics(base, gold)
    results["grouping"]["heuristic"] = grouping_metrics(group.heuristic(turns), gold)
    if not a.skip_model_grouping:
        for ep in endpoints:
            for rep in range(a.reps):
                pred, meta = group.model(turns, llm.endpoint(ep))
                gm = grouping_metrics(pred, gold)
                gm["latency_ms"], gm["error"] = meta.get("latency_ms"), meta.get("error")
                results["grouping"]["model {0} rep{1}".format(ep.split(":")[-1], rep + 1)] = gm

    by_task = collections.defaultdict(list)
    for t in turns:
        by_task[gold[t["key"]]].append(t)
    modes = a.modes.split(",")
    jobs = [(ep, mode, tid, rep) for ep in endpoints for mode in modes for tid in sorted(by_task) for rep in range(a.reps)]
    calls = {ep: llm.endpoint(ep) for ep in endpoints}

    def run(job):
        ep, mode, tid, rep = job
        out, meta = judge.judge(by_task[tid], mode, calls[ep])
        return {"corpus": name, "endpoint": ep, "mode": mode, "task": tid, "rep": rep, "out": out, "meta": meta}

    with cf.ThreadPoolExecutor(a.workers) as ex:
        rows = list(ex.map(run, jobs))
    with open(os.path.join(a.out, name + "-judgments.jsonl"), "w", encoding="utf-8") as fh:
        for r in rows:
            fh.write(json.dumps(r) + "\n")
    for ep in endpoints:
        for mode in modes:
            sel = [r for r in rows if r["endpoint"] == ep and r["mode"] == mode]
            m = judge_metrics(sel, tasks)
            m["repeat_consistency"] = consistency(sel)
            results["judge"]["{0} / {1}".format(ep.split(":")[-1], mode)] = m
    return results


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--out", default="/tmp/aiq-spike")
    ap.add_argument("--endpoint", action="append", default=[])
    ap.add_argument("--modes", default="signals,excerpt")
    ap.add_argument("--reps", type=int, default=2)
    ap.add_argument("--workers", type=int, default=8)
    ap.add_argument("--corpora", default="dev,heldout")
    ap.add_argument("--skip-model-grouping", action="store_true")
    a = ap.parse_args()
    endpoints = a.endpoint or ["claude-cli:claude-haiku-5-5"]
    results = {}
    for name in a.corpora.split(","):
        if name == "dev":
            results[name] = evaluate_corpus(name, a, endpoints)
        else:
            lib = heldout.to_library(os.path.join(HERE, "data", "heldout_scenarios.json"))
            results[name] = evaluate_corpus(name, a, endpoints, scenarios=lib, plan=heldout.plan_for(lib))
    with open(os.path.join(a.out, "results.json"), "w", encoding="utf-8") as fh:
        json.dump(results, fh, indent=1)
    print(json.dumps(results, indent=1))


if __name__ == "__main__":
    main()
