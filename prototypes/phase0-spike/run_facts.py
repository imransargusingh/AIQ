#!/usr/bin/env python3
"""Facts-first judge experiment (rung-model spec, PR #3): model reads facts, rules place the rung.

  python3 prototypes/phase0-spike/run_facts.py --out /tmp/aiq-facts \
      --endpoint claude-cli:claude-sonnet-5-5 --endpoint claude-cli:claude-haiku-5-5 --reps 2

Fact calls are made once per task and run; the rubric variants below are then applied to the same
facts, so differences between variants come from the rules alone.
"""
import argparse
import collections
import concurrent.futures as cf
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from aiq_spike import facts, generate, heldout, llm  # noqa: E402
from aiq_spike.normalize import load_turns  # noqa: E402

# simulated repo metadata (F9): in the product this is git history + CI, never the session
REPO_META = {
    "dev": {".claude/commands/dependency-bump.md": {"authors": 4, "runs_by_others_30d": 9},
            ".claude/commands/release-notes.md": {"authors": 6, "runs_by_others_30d": 14}},
    "heldout": {".claude/commands/gen-changelog.md": {"authors": 3, "runs_by_others_30d": 5},
                ".claude/commands/fix-flaky.md": {"authors": 5, "runs_by_others_30d": 11}},
}

VARIANTS = {
    "A spec as written (F2 from logs only)": dict(f2_source="log"),
    "B F2 estimated by the model": dict(f2_source="model"),
    "C B + Factory needs Delegated, not Orchestrated": dict(f2_source="model", factory_needs_orchestrated=False),
    "D C + one sub-agent with checks counts as coordinated": dict(f2_source="model", factory_needs_orchestrated=False, min_subagents=1),
    "E D with fact threshold 0.5": dict(f2_source="model", factory_needs_orchestrated=False, min_subagents=1, th=0.5),
}
FACT_KEYS = ["F2_person_share_pct", "F4_goal_context_constraints", "F5_reprompts_added_information",
             "F6_output_checked", "F7_written_brief", "F12_step_by_step"]


def corpus(name, out):
    d = os.path.join(out, name)
    if name == "dev":
        generate.main(d)
    else:
        lib = heldout.to_library(os.path.join(HERE, "data", "heldout_scenarios.json"))
        generate.main(d, scenarios=lib, plan=heldout.plan_for(lib))
    turns = load_turns(generate.REPO, os.path.join(d, "claude_home"), os.path.join(d, "copilot_home"))
    labels = json.load(open(os.path.join(d, "labels.json"), encoding="utf-8"))
    by_task = collections.defaultdict(list)
    for t in turns:
        by_task[labels["turns"][t["key"]]].append(t)
    return by_task, labels["tasks"]


def score(rows, tasks):
    ok = [r for r in rows if r["rung"] is not None]
    d = [r["rung"] - tasks[r["task"]]["rung"] for r in ok]
    by_gold = collections.defaultdict(lambda: [0, 0])
    for r in ok:
        g = tasks[r["task"]]["rung"]
        by_gold[g][1] += 1
        by_gold[g][0] += r["rung"] == g
    return {"n": len(ok), "exact": round(sum(x == 0 for x in d) / len(ok), 3),
            "within_1": round(sum(abs(x) <= 1 for x in d) / len(ok), 3),
            "mean_signed_error": round(sum(d) / len(ok), 3),
            "uncertain_share": round(sum(r["uncertain"] for r in ok) / len(ok), 3),
            "exact_by_gold_rung": {str(k): "{0}/{1}".format(*v) for k, v in sorted(by_gold.items())}}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="/tmp/aiq-facts")
    ap.add_argument("--endpoint", action="append", default=[])
    ap.add_argument("--reps", type=int, default=2)
    ap.add_argument("--workers", type=int, default=10)
    ap.add_argument("--from-jsonl", help="re-score saved fact calls (no model calls)")
    a = ap.parse_args()
    endpoints = list(a.endpoint)
    saved = collections.defaultdict(list)
    if a.from_jsonl:
        for line in open(a.from_jsonl, encoding="utf-8"):
            r = json.loads(line)
            saved[r["corpus"]].append(r)
            if not a.endpoint and r["endpoint"] not in endpoints:
                endpoints.append(r["endpoint"])
    endpoints = endpoints or ["claude-cli:claude-sonnet-5-5"]
    results, raw = {}, []
    for name in ("dev", "heldout"):
        by_task, tasks = corpus(name, a.out)
        logs = {tid: facts.log_facts(ts, REPO_META[name]) for tid, ts in by_task.items()}
        jobs = [(ep, tid, rep) for ep in endpoints for tid in sorted(by_task) for rep in range(a.reps)]
        calls = {ep: llm.endpoint(ep) for ep in endpoints}

        def run(j):
            ep, tid, rep = j
            mf, meta = facts.model_facts(by_task[tid], calls[ep])
            return {"corpus": name, "endpoint": ep, "task": tid, "rep": rep, "log": logs[tid], "model": mf, "meta": meta}

        if a.from_jsonl:
            rows = saved[name]
        else:
            with cf.ThreadPoolExecutor(a.workers) as ex:
                rows = list(ex.map(run, jobs))
        raw += rows
        results[name] = {}
        for ep in endpoints:
            sel = [r for r in rows if r["endpoint"] == ep and r["model"]]
            res = {"calls": len([r for r in rows if r["endpoint"] == ep]), "failed": len([r for r in rows if r["endpoint"] == ep and not r["model"]])}
            res["job_accuracy"] = round(sum(r["model"]["job"] == tasks[r["task"]]["job"] for r in sel) / max(1, len(sel)), 3)
            pairs = collections.defaultdict(list)
            for r in sel:
                pairs[r["task"]].append(r["model"])
            two = [v for v in pairs.values() if len(v) >= 2]
            res["fact_repeat_agreement"] = {k: round(sum(v[0][k]["value"] == v[1][k]["value"] for v in two) / max(1, len(two)), 3) for k in FACT_KEYS}
            for vname, kw in VARIANTS.items():
                placed = []
                for r in sel:
                    rung, unc, _ = facts.place(r["log"], r["model"], **kw)
                    placed.append({"task": r["task"], "rung": rung, "uncertain": unc, "rep": r["rep"]})
                s = score(placed, tasks)
                by = collections.defaultdict(list)
                for p in placed:
                    by[p["task"]].append(p["rung"])
                s["rung_repeat_agreement"] = round(sum(v[0] == v[1] for v in by.values() if len(v) >= 2) / max(1, len(by)), 3)
                res[vname] = s
            lat = sorted(r["meta"]["latency_ms"] for r in rows if r["endpoint"] == ep)
            res["latency_ms_p50"] = lat[len(lat) // 2]
            results[name][ep.split(":")[-1]] = res
    with open(os.path.join(a.out, "facts-results.json"), "w", encoding="utf-8") as fh:
        json.dump(results, fh, indent=1)
    if not a.from_jsonl:
        with open(os.path.join(a.out, "facts-judgments.jsonl"), "w", encoding="utf-8") as fh:
            for r in raw:
                fh.write(json.dumps(r) + "\n")
    print(json.dumps(results, indent=1))


if __name__ == "__main__":
    main()
