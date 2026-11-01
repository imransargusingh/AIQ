#!/usr/bin/env python3
"""Phase 0 fact check on the person's own imported sessions (history-import spec, PR #13).

Replaces "collect a week of new sessions": the sessions already on the device are read, automated
sessions are set aside with the history-import rule, a small sample of hand-run sessions gets its
facts extracted, and the person marks each fact right or wrong on a local page.

Privacy: everything runs on the device. Prompt text appears only in the local review page under
--out (default ~/.aiq/fact-check). Model facts go only to the provider that already saw the session
(Copilot sessions -> the person's `copilot -p`; Claude Code sessions -> their `claude -p`). The
answers file holds item ids and yes/no/can't-tell only, and `score` prints counts only.

  python fact_check.py sample [--sessions 6] [--seed 7] [--copilot-model claude-haiku-5.5]
                              [--claude-model claude-haiku-5-5] [--no-model] [--out DIR]
  python fact_check.py score [--out DIR] [--answers PATH]

Stdlib only. Uses the spike's readers (aiq_spike.normalize) and fact code (aiq_spike.facts).
"""
import argparse
import collections
import datetime as dt
import hashlib
import html
import json
import os
import random
import re
import sqlite3
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from aiq_spike import normalize as N  # noqa: E402
from aiq_spike.facts import FACT_SCHEMA, FACT_SYSTEM, log_facts  # noqa: E402
from aiq_spike.judge import excerpt, signals  # noqa: E402
from aiq_spike import llm  # noqa: E402

SP = N.SP
for _s in (sys.stdout, sys.stderr):
    try:
        _s.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, ValueError, OSError):
        pass

FACT_TYPES = ["job", "F1", "F3", "F7", "F8", "F4", "F5", "F6", "F12", "F13"]
JOB_LABEL = {"code_change": "Code a change", "write_doc": "Write a spec or doc", "other": "Other work"}


# ---------------------------------------------------------------- discovery + automated rule

def _ts(s):
    try:
        return dt.datetime.fromisoformat(str(s).replace("Z", "+00:00"))
    except ValueError:
        return None


def _claude_entrypoint(path):
    with open(path, encoding="utf-8", errors="replace") as fh:
        for k, line in enumerate(fh):
            if k > 200:
                break
            if '"entrypoint"' in line:
                try:
                    return json.loads(line).get("entrypoint")
                except ValueError:
                    continue
    return None


def discover():
    """All sessions on the device as {tool, id, folder, start, ni, load}. No content is read here
    except the Claude Code entrypoint field."""
    out = []
    projects = os.path.join(SP.claude_home(), "projects")
    if os.path.isdir(projects):
        for slug in sorted(os.listdir(projects)):
            d = os.path.join(projects, slug)
            if not os.path.isdir(d):
                continue
            for f in sorted(os.listdir(d)):
                if not f.endswith(".jsonl"):
                    continue
                p = os.path.join(d, f)
                ep = _claude_entrypoint(p)
                out.append({"tool": "claude", "id": f[:-6], "folder": slug.lower(), "path": p,
                            "start": dt.datetime.fromtimestamp(os.path.getctime(p), dt.timezone.utc),
                            "ni": ep == "sdk-cli" if ep else None})
    home = SP.copilot_home()
    db = os.path.join(home, "session-store.db")
    if os.path.isfile(db):
        con = sqlite3.connect("file:{0}?mode=ro".format(db.replace("\\", "/")), uri=True)
        for sid, cwd, created in con.execute("select id, cwd, created_at from sessions"):
            ev = os.path.join(home, "session-state", sid, "events.jsonl")
            if os.path.isfile(ev):
                out.append({"tool": "copilot", "id": sid, "folder": (cwd or "(none)").lower().rstrip("\\/"),
                            "path": ev, "start": _ts(created), "ni": None})
        con.close()
    return out


NORM = [(re.compile(r"https?://\S+"), "<url>"),
        (re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", re.I), "<guid>"),
        (re.compile(r"([A-Za-z]:)?[\\/][^\s\"']+"), "<path>"),
        (re.compile(r"\"[^\"]*\"|'[^']*'"), "<q>"),
        (re.compile(r"\d+"), "0"),
        (re.compile(r"\s+"), " ")]


def template_hash(prompt):
    s = (prompt or "").lower()
    for rx, rep in NORM:
        s = rx.sub(rep, s)
    s = s.strip()[:200]
    return hashlib.sha1(s.encode("utf-8")).hexdigest() if s else None


def load(sess):
    turns = (N.claude_turns if sess["tool"] == "claude" else N.copilot_turns)(sess["path"], sess["id"])
    for t in turns:
        t["mentioned"] = sorted(set(N.PATH_RX.findall(t["prompt"])))
        t["files"] = sorted(set(t["files"]))
    return turns


def classify(sessions):
    """Apply the history-import automated rule (spec: Automated rule). Mutates sessions with h,
    tmpl, auto, why."""
    for s in sessions:
        try:
            s["turns"] = load(s)
        except OSError:
            s["turns"] = []
        s["h"] = len(s["turns"])
        s["tmpl"] = template_hash(s["turns"][0]["prompt"]) if s["turns"] else None
    groups = collections.Counter((s["folder"], s["tmpl"]) for s in sessions if s["tmpl"])
    by_folder = collections.defaultdict(list)
    for s in sessions:
        by_folder[s["folder"]].append(s)
    for fs in by_folder.values():
        starts = sorted(x["start"] for x in fs if x["start"])
        for s in fs:
            why = []
            if s["ni"]:
                why.append("NI")
            if s["tmpl"] and groups[(s["folder"], s["tmpl"])] >= 11:
                why.append("T")
            if s["start"]:
                lo, hi = s["start"] - dt.timedelta(minutes=30), s["start"] + dt.timedelta(minutes=30)
                if sum(1 for x in starts if lo <= x <= hi) >= 50:
                    why.append("B")
            s["why"] = why
            s["auto"] = s["h"] <= 1 and bool(why)
    for fs in by_folder.values():  # folder prior (F)
        if fs and sum(1 for s in fs if s["auto"]) / float(len(fs)) > 0.8:
            for s in fs:
                if not s["auto"] and s["h"] <= 1:
                    s["auto"], s["why"] = True, s["why"] + ["F"]
    return sessions


# ---------------------------------------------------------------- model facts per provider

def _mcp_names(home):
    names = set()
    for f in ("mcp-config.json", "config.json", "settings.json"):
        p = os.path.join(home, f)
        try:
            with open(p, encoding="utf-8") as fh:
                d = json.loads(re.sub(r"^\s*//.*$", "", fh.read(), flags=re.M))
        except (OSError, ValueError):
            continue
        names |= set((d.get("mcpServers") or {}).keys())
    return sorted(names)


def copilot_cli(model):
    """The person's own signed-in Copilot CLI (history-import H5), sealed as far as its flags allow:
    no tools, no custom instructions, built-in MCP off, every configured MCP server off, own session
    id recorded so the judge session is never imported (US-11)."""
    home = SP.copilot_home()
    mcp = _mcp_names(home)
    judge_ids = os.path.join(os.path.expanduser("~"), ".aiq", "judge-sessions.txt")

    def call(system, prompt, schema):
        import uuid
        sid = str(uuid.uuid4())
        os.makedirs(os.path.dirname(judge_ids), exist_ok=True)
        with open(judge_ids, "a", encoding="utf-8") as fh:
            fh.write(sid + "\n")
        full = (system + "\n\nReturn ONLY one JSON object, no prose, matching this JSON schema:\n"
                + json.dumps(schema) + "\n\n" + prompt)
        cmd = ["copilot", "-p", full, "-s", "--model", model, "--available-tools=", "--no-custom-instructions",
               "--no-ask-user", "--disable-builtin-mcps", "--sync-session=off", "--no-auto-update",
               "--disable-client-telemetry", "--stream", "off", "--session-id", sid]
        for n in mcp:
            cmd += ["--disable-mcp-server", n]
        t0 = time.time()
        meta = {"latency_ms": None, "error": None, "session_id": sid}
        with tempfile.TemporaryDirectory() as cwd:
            try:
                p = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace",
                                   timeout=300, cwd=cwd)
                out = p.stdout
            except (OSError, subprocess.TimeoutExpired) as exc:
                meta["error"] = type(exc).__name__
                out = ""
        meta["latency_ms"] = int((time.time() - t0) * 1000)
        a, b = out.find("{"), out.rfind("}")
        try:
            return json.loads(out[a:b + 1], strict=False), meta  # copilot -s hard-wraps long lines inside strings
        except ValueError:
            meta["error"] = meta["error"] or "no JSON in reply"
            return None, meta
    return call


# ---------------------------------------------------------------- items

def _model_item(mf, key, ftype, label):
    f = (mf or {}).get(key) or {}
    if f.get("value") is None:
        return None
    return {"type": ftype, "value": f["value"], "confidence": f.get("confidence"),
            "statement": label if f["value"] else "Not true: " + label[0].lower() + label[1:]}


def build_items(sess, lf, mf):
    sig = signals(sess["turns"])
    items = {
        "job": ({"type": "job", "value": mf.get("job"), "confidence": mf.get("job_confidence"),
                 "statement": "This task's job is: " + JOB_LABEL.get(mf.get("job"), str(mf.get("job")))}
                if mf and mf.get("job") else None),
        "F1": {"type": "F1", "value": lf["F1_agent_made_edits"], "confidence": None,
               "statement": ("The agent made the edits ({0} files edited by the agent)".format(sig["files_edited_by_agent"])
                             if lf["F1_agent_made_edits"] else "The agent made no file edits in this task")},
        "F3": {"type": "F3", "value": lf["F3_checks_run_by"], "confidence": None,
               "statement": {"agent": "The agent ran checks (tests, build or lint)",
                             "person": "You ran the checks yourself; the agent did not",
                             "nobody": "Nobody ran tests, build or lint in this task"}[lf["F3_checks_run_by"]]},
        "F7": None, "F8": {"type": "F8", "value": lf["F8_subagents"], "confidence": None,
                           "statement": ("The agent used {0} sub-agent(s)".format(lf["F8_subagents"])
                                         if lf["F8_subagents"] else "No sub-agents were used")},
        "F4": _model_item(mf, "F4_goal_context_constraints", "F4", "Your request stated a goal, context and constraints"),
        "F5": _model_item(mf, "F5_reprompts_added_information", "F5", "Every follow-up prompt added new information"),
        "F6": _model_item(mf, "F6_output_checked", "F6", "The output was reviewed or checked before use"),
        "F12": _model_item(mf, "F12_step_by_step", "F12", "You drove the agent step by step, one small instruction per turn"),
        "F13": _model_item(mf, "F13_whole_outcome_handed_over", "F13", "You handed over the whole outcome with done criteria"),
    }
    m7 = (mf or {}).get("F7_written_brief") or {}
    v7 = bool(lf["F7_spec_file_given"] or m7.get("value") is True)
    items["F7"] = {"type": "F7", "value": v7, "confidence": m7.get("confidence"),
                   "statement": "A written spec, brief or instructions file was given to the agent" if v7
                   else "No written spec, brief or instructions file was given to the agent"}
    return items


def sample_sessions(sessions, n, seed, since=None):
    rng = random.Random(seed)
    ok = [s for s in sessions if not s["auto"] and s["h"] >= 1
          and (since is None or (s["start"] and s["start"] >= since))
          and sum(len(t["tools"]) for t in s["turns"]) > 0]
    by_folder = collections.defaultdict(list)
    for s in ok:
        by_folder[(s["tool"], s["folder"])].append(s)
    keys = list(by_folder)
    rng.shuffle(keys)
    for k in keys:
        rng.shuffle(by_folder[k])
    pick = []
    while len(pick) < n and any(by_folder.values()):
        for k in keys:
            if by_folder[k] and len(pick) < n:
                pick.append(by_folder[k].pop())
    return pick


# ---------------------------------------------------------------- page

PAGE = """<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>AIQ fact check</title>
<style>
:root{--bg:#fff;--fg:#1b1b1b;--muted:#5c5c5c;--line:#d8d8d8;--card:#f6f7f7;--accent:#0f6e6e;--yes:#1e7a3c;--no:#b3261e}
@media (prefers-color-scheme:dark){:root{--bg:#161818;--fg:#ececec;--muted:#a5a5a5;--line:#333;--card:#1f2222;--accent:#5cc2c2;--yes:#6fcf8a;--no:#ff8a80}}
body{background:var(--bg);color:var(--fg);font:15px/1.5 "Segoe UI Variable","Segoe UI",system-ui,sans-serif;margin:0;padding:16px;max-width:980px;margin-inline:auto}
h1{font-size:22px;margin:4px 0}h2{font-size:17px;margin:0 0 8px}.muted{color:var(--muted)}
.task{border:1px solid var(--line);border-radius:8px;padding:14px;margin:18px 0;background:var(--card)}
details{margin:8px 0}pre{white-space:pre-wrap;word-break:break-word;font:13px/1.45 Consolas,monospace;margin:4px 0 10px}
.fact{display:grid;grid-template-columns:1fr auto;gap:8px;align-items:center;padding:8px 0;border-top:1px solid var(--line)}
.opts label{margin-left:10px;cursor:pointer;white-space:nowrap}.conf{font-size:12px;color:var(--muted)}
.bar{position:sticky;bottom:0;background:var(--bg);border-top:1px solid var(--line);padding:10px 0;display:flex;gap:12px;align-items:center}
button{font:inherit;padding:6px 14px;border-radius:6px;border:1px solid var(--accent);background:var(--accent);color:var(--bg);cursor:pointer}
button:focus-visible,input:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
@media (max-width:600px){.fact{grid-template-columns:1fr}.opts label{margin:0 12px 0 0}}
</style></head><body>
<h1>AIQ fact check</h1>
<p class="muted">This page lives only on your computer. For each statement, mark whether it is right about that task.
Your answers save in this browser as you go. When done, press <b>Save answers</b>: it downloads <code>answers.json</code>,
which holds only item numbers and yes/no, no prompt text. Put it in __OUT__.</p>
__TASKS__
<div class="bar"><button id="save" type="button">Save answers</button><span id="count" class="muted"></span></div>
<script>
const KEY="aiq-fact-check-__RUN__";let a={};try{a=JSON.parse(localStorage.getItem(KEY)||"{}")}catch(e){}
const inputs=[...document.querySelectorAll("input[type=radio]")];
function upd(){const n=new Set(inputs.map(i=>i.name)).size;document.getElementById("count").textContent=Object.keys(a).length+" of "+n+" marked"}
inputs.forEach(i=>{if(a[i.name]===i.value)i.checked=true;i.addEventListener("change",()=>{a[i.name]=i.value;try{localStorage.setItem(KEY,JSON.stringify(a))}catch(e){}upd()})});upd();
document.getElementById("save").onclick=()=>{const b=new Blob([JSON.stringify({run:"__RUN__",answers:a},null,1)],{type:"application/json"});
const l=document.createElement("a");l.href=URL.createObjectURL(b);l.download="answers.json";l.click()};
</script></body></html>"""


def render_task(i, s, items):
    e = html.escape
    parts = ['<section class="task"><h2>Task {0} · {1} · {2}</h2>'.format(
        i, "Copilot CLI" if s["tool"] == "copilot" else "Claude Code", e(s["folder"].split("\\")[-1].split("/")[-1]))]
    parts.append('<p class="muted">{0} · {1} prompt(s)</p>'.format(e(str(s["start"])[:16]), s["h"]))
    parts.append("<details open><summary>What happened</summary>")
    for k, t in enumerate(s["turns"][:10], 1):
        parts.append("<pre><b>Prompt {0}:</b> {1}</pre>".format(k, e(t["prompt"][:1500])))
        if t["files"]:
            parts.append('<pre class="muted">files: {0}</pre>'.format(e(", ".join(t["files"][:10]))))
        cmds = [x.get("command", "")[:100] for x in t["tools"] if x["tool"] == "Bash"][:8]
        if cmds:
            parts.append('<pre class="muted">ran: {0}</pre>'.format(e(" | ".join(cmds))))
        for sa in t["subagents"][:4]:
            parts.append('<pre class="muted">sub-agent: {0}</pre>'.format(e(sa["name"])))
    parts.append("</details>")
    for it in items:
        name = it["id"]
        conf = "" if it.get("confidence") is None else '<span class="conf"> (model, {0:.0%} sure)</span>'.format(it["confidence"])
        parts.append('<div class="fact"><div>{0}{1}</div><div class="opts" role="radiogroup" aria-label="{2}">'.format(
            e(it["statement"]), conf, e(it["statement"])))
        for v, lab in (("yes", "Right"), ("no", "Wrong"), ("cant", "Can't tell")):
            parts.append('<label><input type="radio" name="{0}" value="{1}"> {2}</label>'.format(name, v, lab))
        parts.append("</div></div>")
    parts.append("</section>")
    return "\n".join(parts)


# ---------------------------------------------------------------- commands

def cmd_sample(a):
    t0 = time.time()
    os.makedirs(a.out, exist_ok=True)
    sessions = classify(discover())
    skip = set()
    try:
        with open(os.path.join(os.path.expanduser("~"), ".aiq", "judge-sessions.txt"), encoding="utf-8") as fh:
            skip = {x.strip() for x in fh if x.strip()}
    except OSError:
        pass
    sessions = [s for s in sessions if s["id"] not in skip]
    for s in sessions:  # Claude Code: ctime can be copy time; use the first turn's timestamp when present
        if s["turns"] and s["turns"][0].get("start"):
            s["start"] = _ts(s["turns"][0]["start"]) or s["start"]
    since = dt.datetime.now(dt.timezone.utc) - dt.timedelta(days=a.since_days) if a.since_days else None
    pick = sample_sessions(sessions, a.sessions, a.seed, since)
    calls = {"copilot": None if a.no_model else copilot_cli(a.copilot_model),
             "claude": None if a.no_model else llm.claude_cli(a.claude_model)}
    tasks_html, items_out, model_meta = [], [], []
    for i, s in enumerate(pick, 1):
        lf = log_facts(s["turns"], {})
        mf, meta = (None, {"error": "model off"})
        if calls[s["tool"]]:
            prompt = "Task signals (extracted on device):\n" + json.dumps(signals(s["turns"]), indent=1) \
                     + "\n\nMinimised excerpt:\n" + excerpt(s["turns"])
            try:
                mf, meta = calls[s["tool"]](FACT_SYSTEM, prompt, FACT_SCHEMA)
            except (OSError, subprocess.SubprocessError) as exc:  # CLI missing or failed: log facts only
                mf, meta = None, {"error": type(exc).__name__}
        model_meta.append({"task": i, "tool": s["tool"], "latency_ms": meta.get("latency_ms"),
                           "error": meta.get("error") or (None if mf else "no facts returned")})
        all_items = build_items(s, lf, mf)
        want = [FACT_TYPES[(5 * (i - 1) + k) % len(FACT_TYPES)] for k in range(5)]
        chosen = [all_items[t] for t in want if all_items.get(t)]
        for t in FACT_TYPES:  # fill a gap (model fact missing) with another available fact
            if len(chosen) >= 5:
                break
            if all_items.get(t) and all_items[t] not in chosen:
                chosen.append(all_items[t])
        for k, it in enumerate(chosen, 1):
            it["id"] = "t{0}f{1}".format(i, k)
            it["task"] = i
            it["tool"] = s["tool"]
            items_out.append({x: it[x] for x in ("id", "task", "tool", "type", "value", "confidence")})
        tasks_html.append(render_task(i, s, chosen))
    run = dt.datetime.now().strftime("%Y%m%d-%H%M")
    page = (PAGE.replace("__TASKS__", "\n".join(tasks_html)).replace("__RUN__", run)
            .replace("__OUT__", html.escape(a.out)))
    with open(os.path.join(a.out, "review.html"), "w", encoding="utf-8") as fh:
        fh.write(page)
    by_tool = collections.Counter((s["tool"], s["auto"]) for s in sessions)
    summary = {"run": run, "wall_s": round(time.time() - t0, 1),
               "sessions": {t: {"total": by_tool[(t, True)] + by_tool[(t, False)], "automated": by_tool[(t, True)]}
                            for t in ("copilot", "claude")},
               "since_days": a.since_days,
               "eligible": len(sample_sessions(sessions, 10 ** 6, a.seed, since)),
               "sampled": len(pick), "items": len(items_out),
               "items_by_type": dict(collections.Counter(x["type"] for x in items_out)),
               "model_calls": model_meta}
    with open(os.path.join(a.out, "items.json"), "w", encoding="utf-8") as fh:
        json.dump({"run": run, "items": items_out, "summary": summary}, fh, indent=1)
    print(json.dumps(summary, indent=1))
    print("review page: " + os.path.join(a.out, "review.html"))


def cmd_score(a):
    with open(os.path.join(a.out, "items.json"), encoding="utf-8") as fh:
        items = json.load(fh)["items"]
    with open(a.answers or os.path.join(a.out, "answers.json"), encoding="utf-8") as fh:
        ans = json.load(fh)["answers"]
    rows = collections.defaultdict(collections.Counter)
    for it in items:
        rows[it["type"]][ans.get(it["id"], "unmarked")] += 1
    print("| fact | right | wrong | can't tell | unmarked | precision |")
    print("|---|---|---|---|---|---|")
    tot = collections.Counter()
    for t in FACT_TYPES:
        c = rows.get(t)
        if not c:
            continue
        tot.update(c)
        judged = c["yes"] + c["no"]
        prec = "{0:.0%}".format(c["yes"] / float(judged)) if judged else "n/a"
        print("| {0} | {1} | {2} | {3} | {4} | {5} |".format(t, c["yes"], c["no"], c["cant"], c["unmarked"], prec))
    judged = tot["yes"] + tot["no"]
    print("| all | {0} | {1} | {2} | {3} | {4} |".format(tot["yes"], tot["no"], tot["cant"], tot["unmarked"],
                                                        "{0:.0%}".format(tot["yes"] / float(judged)) if judged else "n/a"))


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = p.add_subparsers(dest="cmd", required=True)
    default_out = os.path.join(os.path.expanduser("~"), ".aiq", "fact-check")
    s = sub.add_parser("sample")
    s.add_argument("--sessions", type=int, default=6)
    s.add_argument("--seed", type=int, default=7)
    s.add_argument("--since-days", type=float, default=3,
                   help="sample only sessions started in the last N days, so the person still remembers them (0 = all)")
    s.add_argument("--copilot-model", default="claude-haiku-5.5")
    s.add_argument("--claude-model", default="claude-haiku-5-5")
    s.add_argument("--no-model", action="store_true")
    s.add_argument("--out", default=default_out)
    c = sub.add_parser("score")
    c.add_argument("--out", default=default_out)
    c.add_argument("--answers")
    a = p.parse_args(argv)
    (cmd_sample if a.cmd == "sample" else cmd_score)(a)


if __name__ == "__main__":
    main()
