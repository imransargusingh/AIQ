"""Read-only: a review page for the tip Kinds the accuracy check paused, for the person to read on their own computer.

For each paused Kind (design judge-accuracy-check K6b, the same window as aiq-domain check::verdicts: the newest
whole check runs until at least 5 readings), it lists every checked task AIQ gave that Kind: the person's own
words, what AIQ said (job, Kind, how sure, rung) and what the second reading said. A second, folded list holds
the tasks the second reading gave that Kind and AIQ did not. Buttons let the person mark who was right; the
marks stay in the browser (localStorage) and the page shows only their tally.

The page holds the person's prompts, so it is written only to the path given and never leaves the computer. The
script prints counts only (no prompt, note, folder or id). The store is opened mode=ro with query_only, like AIQ's own
readers (aiq-store open_read_only), so it also reads the newest rows still in the write-ahead log: immutable=1 would
skip them, and a check run minutes ago would be missing.

Usage:
    python3 -I paused_review.py <path to aiq.db> <output .html> [--open]
"""
import collections
import datetime
import html
import json
import os
import re
import sqlite3
import sys

DB, OUT = sys.argv[1], sys.argv[2]
OPEN = "--open" in sys.argv[3:]
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
con.execute("PRAGMA query_only = ON")

PAUSE_BAR, MIN_JOB, MIN_KIND = 0.8, 10, 5  # check::PAUSE_BAR, PAUSE_MIN_JOB, PAUSE_MIN_KIND
MAX_PROMPTS, MAX_CHARS = 6, 1200

JOBS = {"code_change": "Code a change", "write_doc": "Write a spec or doc", "ask_research": "Ask or research",
        "run_ship": "Run or ship it", "unclassified": "No job", "other": "No job"}
# aiq-contracts judge::TASK_TYPES
NAMES = {
    ("code_change", "fix_bug"): "Fixing a bug, test or build error", ("code_change", "feature"): "Building or adding a feature",
    ("code_change", "tests"): "Writing tests", ("code_change", "refactor"): "Refactoring or cleaning up",
    ("code_change", "deps"): "Upgrading or migrating", ("code_change", "scaffold"): "Scaffolding or prototyping",
    ("code_change", "other"): "Other coding", ("write_doc", "spec"): "Writing a spec or PRD",
    ("write_doc", "design_note"): "Designing an architecture or plan", ("write_doc", "status"): "Writing an update or announcement",
    ("write_doc", "research"): "Summarising notes or research", ("write_doc", "howto"): "Writing documentation or a how-to",
    ("write_doc", "deck"): "Preparing a deck or presentation", ("write_doc", "other"): "Other writing",
    ("ask_research", "how_to"): "Asking how to do something", ("ask_research", "explain"): "Understanding code or a concept",
    ("ask_research", "troubleshoot"): "Investigating a bug or problem", ("ask_research", "compare"): "Comparing options",
    ("ask_research", "find"): "Looking up a fact", ("ask_research", "research"): "Researching a topic",
    ("ask_research", "analyse"): "Analyzing data", ("ask_research", "progress"): "Checking on progress",
    ("ask_research", "review"): "Reviewing code or other work", ("ask_research", "plan"): "Planning or brainstorming",
    ("ask_research", "other"): "Other questions", ("run_ship", "restart_fix"): "Getting something running again",
    ("run_ship", "release"): "Releasing or publishing", ("run_ship", "run_job"): "Running an app, script or pipeline",
    ("run_ship", "setup"): "Setting up or connecting a tool", ("run_ship", "source_control"): "Committing, merging or syncing code",
    ("run_ship", "other"): "Other running or shipping",
}
RUNGS = {1: "Assisted", 2: "Directed", 3: "Delegated", 4: "Orchestrated", 5: "Factory"}
# The Kind definitions analysis j7 used (aiq-domain rung::SYSTEM, TASK TYPE section, AIQ 0.1.65).
J7_TYPES = r"""
- code_change: fix_bug (fixing a bug, a failing test, or lint and build errors), feature (building or adding functionality, including carrying out or continuing a build plan; when the work is clearly a fix, tests or a refactor, that type wins), tests (writing tests), refactor (refactoring or cleaning up), deps (upgrading dependencies, or migrating to a new version, framework or platform), scaffold (scaffolding or prototyping something new), other.
- write_doc: spec (a spec, PRD or acceptance criteria), design_note (a design note, an architecture or a plan written down as a document), status (a status update, newsletter or announcement), research (summarising notes, research or a transcript), howto (documentation, a README or a how-to), deck (slides, a talk or a presentation), other.
- ask_research: how_to (asking how to do something), explain (understanding code or how something works), troubleshoot (investigating a bug or finding the cause of a problem), compare (comparing options), find (looking up one fact, such as a path, an owner, a setting or whether something exists; no counting or computing), research (researching a topic), analyse (analyzing data: the result is a number, table, chart, breakdown or trend computed from data such as a spreadsheet, query results, logs, metrics or a set of records; not reviewing or critiquing a plan, doc, deck or code, not auditing a repo or app, not summarising one document or transcript), progress (asking how work that is already running is going: its status, progress, ETA or whether it finished), review (checking or reviewing code, a document, a plan or a set of claims, or auditing a repo or app; the result is findings and no changed files), plan (working out an approach, a breakdown, next steps or ideas before the work starts, with no document written), other.
- run_ship: restart_fix (getting a stopped or broken thing running), release (packaging, publishing or deploying; also telling the agent a release is out so it installs, updates or verifies it), run_job (starting, stopping or running an app, script, pipeline or data refresh that already works), setup (installing, connecting or configuring a tool), source_control (commit, pull, push, branch, merge or sync, when no code change was asked for), other.
"""
DEFS = {}
for line in J7_TYPES.strip().splitlines():
    job = line[2:].split(":", 1)[0]
    for kind, desc in re.findall(r"(\w+) \(([^()]*)\)", line):
        DEFS[(job, kind)] = desc


def has_table(t):
    return con.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (t,)).fetchone() is not None


def kind_name(job, kind):
    if job in ("unclassified", "other") or not job:
        return "No job"
    return NAMES.get((job, kind), f"{JOBS.get(job, job)}: {kind or 'no Kind'}")


# ---------------------------------------------------------------- AIQ's text rule (aiq-domain automated::person_words)
ENVELOPES = ("<wake ", "<relay ", "<coordinator-relay", "<task-notification", "<scheduled-task", "<project_claude_message")


def unescape(s):
    for a, b in (("&#34;", '"'), ("&quot;", '"'), ("&#39;", "'"), ("&apos;", "'"), ("&lt;", "<"), ("&gt;", ">"), ("&amp;", "&")):
        s = s.replace(a, b)
    return s


def person_words(p):
    t = p.lstrip()
    if not any(t.startswith(o) for o in ENVELOPES):
        return p
    if not t.startswith("<wake"):
        return None
    trigger, bodies = None, []
    for part in t.split("<message ")[1:]:
        if ">" not in part:
            continue
        attrs, rest = part.split(">", 1)
        words = attrs.split()
        if 'from="human"' not in words or "</message>" not in rest:
            continue
        body = unescape(rest.split("</message>", 1)[0].strip())
        if 'trigger="true"' in words and trigger is None:
            trigger = body
        bodies.append(body)
    return trigger if trigger is not None else ("\n".join(bodies) if bodies else None)


def prompt_text(key):
    if key.startswith("import:"):
        try:
            tool, rest = key[len("import:"):].split(":", 1)
            sid, seq = rest.rsplit(":", 1)
            r = con.execute("SELECT text FROM import_event WHERE tool = ? AND source_session_id = ? AND seq = ?",
                            (tool, sid, int(seq))).fetchone()
        except (ValueError, sqlite3.Error):
            return None
    else:
        r = con.execute("SELECT text FROM content WHERE event_id = ?", (key,)).fetchone() if has_table("content") else None
    if not r or not r[0]:
        return None
    w = person_words(r[0])
    return w if w and w.strip() else None


# ---------------------------------------------------------------- the readings (aiq-core coaching::readings)
if not (has_table("judge_check") and has_table("judge_check_run") and has_table("judgment")):
    print("no accuracy check in this store; nothing to review")
    sys.exit(0)

jcols = {r[1] for r in con.execute("PRAGMA table_info(judgment)")}
tcols = "task_type, task_type_confidence" if "task_type" in jcols else "NULL, NULL"
current = {}
for row in con.execute(
        f"""SELECT j.judgment_id, j.task_id, j.provider, j.judged_utc_ms, j.job, j.job_confidence, j.current_rung, j.state,
               j.read, j.evidence, j.judge_version, {tcols} FROM judgment j
            WHERE j.rowid = (SELECT x.rowid FROM judgment x WHERE x.task_id = j.task_id
                             ORDER BY x.judged_utc_ms DESC, x.rowid DESC LIMIT 1)"""):
    current[row[0]] = dict(zip(("jid", "task", "tool", "at", "job", "job_conf", "rung", "state", "read", "evidence",
                                "jv", "kind", "kind_conf"), row))

runs = {rid: ms for rid, ms in con.execute("SELECT run_id, started_utc_ms FROM judge_check_run")}
readings = []
seen_rows = collections.Counter()
for rid, jid, ans, grader in con.execute(
        "SELECT run_id, judgment_id, answer, grader_model FROM judge_check WHERE outcome = 'ok' ORDER BY checked_utc_ms"):
    if rid not in runs or jid not in current:
        seen_rows["no run" if rid not in runs else "analysis replaced since"] += 1
        continue
    try:
        a = json.loads(ans)
    except (TypeError, ValueError):
        continue
    readings.append({"run": rid, "run_ms": runs[rid], "j": current[jid], "a": a, "grader": grader})

# ---------------------------------------------------------------- the pause rule (aiq-domain check::verdicts)
order = sorted({(r["run"], r["run_ms"]) for r in readings}, key=lambda x: (x[1], x[0]), reverse=True)
scored = collections.defaultdict(list)
for run, _ in order:
    for r in (x for x in readings if x["run"] == run):
        jj, jk = r["j"]["job"], r["j"]["kind"]
        if jj in ("unclassified", "other"):
            continue
        job_ok = r["a"].get("job") == jj
        scored[("job", jj)].append((run, job_ok, r))
        if jk and jk != "other":
            scored[("kind", jj, jk)].append((run, job_ok and r["a"].get("task_type") == jk, r))
paused = []
for scope, lst in scored.items():
    need = MIN_JOB if scope[0] == "job" else MIN_KIND
    seen, window = [], []
    for run, hit, r in lst:
        if run not in seen:
            if len(window) >= need:
                break
            seen.append(run)
        window.append((hit, r))
    k = sum(1 for h, _ in window if h)
    if len(window) >= need and k < PAUSE_BAR * len(window):
        paused.append((scope, k, window, set(seen)))
paused.sort(key=lambda p: (p[0][0] != "kind", p[0]))


# ---------------------------------------------------------------- the page
def esc(s):
    return html.escape(s or "", quote=True)


def when(ms):
    return datetime.datetime.fromtimestamp(ms / 1000).strftime("%b %d, %Y") if ms else ""


def words_html(j):
    try:
        keys = json.loads(j["read"]).get("keys") or []
    except (TypeError, ValueError):
        keys = []
    texts = [t for t in (prompt_text(k) for k in keys) if t]
    if not texts:
        return '<p class="muted">The prompt text is no longer in AIQ\'s store.</p>', 0
    out = []
    for i, t in enumerate(texts[:MAX_PROMPTS]):
        short = t if len(t) <= MAX_CHARS else t[:MAX_CHARS].rstrip() + " …"
        out.append(f'<div class="prompt"><span class="n">{i + 1}</span><div class="pt">{esc(short)}</div></div>')
    if len(texts) > MAX_PROMPTS:
        out.append(f'<p class="muted">… and {len(texts) - MAX_PROMPTS} more prompts in this task.</p>')
    return "".join(out), len(texts)


def evidence_html(j):
    try:
        ev = [e for e in json.loads(j["evidence"] or "[]") if isinstance(e, str)]
    except (TypeError, ValueError):
        ev = []
    if not ev:
        return ""
    items = "".join(f"<li>{esc(e)}</li>" for e in ev[:4])
    return f'<details class="why"><summary>Why AIQ placed it</summary><ul>{items}</ul></details>'


MARK_AGREE = '<button data-v="right">Right</button><button data-v="wrong">Wrong</button>'
MARK_DISAGREE = ('<button data-v="aiq">AIQ</button><button data-v="second">Second reading</button>'
                 '<button data-v="both">Both fit</button><button data-v="neither">Neither</button>')
rows_out = 0
cards = []
for n, (scope, k, window, seen) in enumerate(paused):
    if scope[0] == "kind":
        title = kind_name(scope[1], scope[2])
        sub = JOBS.get(scope[1], scope[1])
        d = DEFS.get((scope[1], scope[2]), "")
    else:
        title, sub, d = JOBS.get(scope[1], scope[1]), "a whole job", ""
    rows = []
    window = sorted(window, key=lambda hr: hr[0])  # disagreements first: they are what the person judges
    for i, (hit, r) in enumerate(window):
        j, a = r["j"], r["a"]
        words, _ = words_html(j)
        rid = f"{n}-{i}"
        aiq_conf = f"{round((j['kind_conf'] or 0) * 100)}% sure" if j["kind_conf"] is not None else ""
        a_name = kind_name(a.get("job"), a.get("task_type"))
        a_job = JOBS.get(a.get("job"), a.get("job") or "")
        a_def = DEFS.get((a.get("job"), a.get("task_type")), "")
        badge = '<span class="badge ok">Agree</span>' if hit else '<span class="badge no">Disagree</span>'
        rows.append(f"""
<article class="task {'agree' if hit else 'disagree'}" data-id="{esc(j['jid'])}">
  <header><span class="date">{esc(when(j['at']))}</span><span class="tool">{esc(j['tool'])}</span>{badge}</header>
  <div class="words">{words}</div>
  <div class="said">
    <div class="side"><h4>AIQ said</h4><p class="kind">{esc(kind_name(j['job'], j['kind']))}</p>
      <p class="muted">{esc(JOBS.get(j['job'], j['job']))} · {esc(aiq_conf)} · {esc(RUNGS.get(j['rung'], ''))}</p>{evidence_html(j)}</div>
    <div class="side"><h4>Second reading said</h4><p class="kind">{esc(a_name)}</p>
      <p class="muted">{esc(a_job)} · {esc(RUNGS.get(a.get('current_rung'), ''))}</p>
      {f'<p class="def">“{esc(a_def)}”</p>' if a_def and not hit else ''}</div>
  </div>
  <div class="mark" role="group" aria-label="{'Is this Kind right?' if hit else 'Who was right?'}">
    <span>{'Is this Kind right?' if hit else 'Who was right?'}</span>
    {MARK_AGREE if hit else MARK_DISAGREE}
  </div>
</article>""")
        rows_out += 1
    # Tasks the second reading gave this Kind and AIQ did not, from the same runs.
    other = []
    if scope[0] == "kind":
        for r in readings:
            if r["run"] in seen and (r["a"].get("job"), r["a"].get("task_type")) == (scope[1], scope[2]) \
                    and (r["j"]["job"], r["j"]["kind"]) != (scope[1], scope[2]):
                j = r["j"]
                words, _ = words_html(j)
                other.append(f"""<article class="task small"><header><span class="date">{esc(when(j['at']))}</span>
<span class="tool">{esc(j['tool'])}</span></header><div class="words">{words}</div>
<p class="muted">AIQ said <b>{esc(kind_name(j['job'], j['kind']))}</b>; the second reading said <b>{esc(title)}</b>.</p></article>""")
    other_html = (f'<details class="others"><summary>{len(other)} more tasks the second reading called “{esc(title)}” '
                  f'and AIQ did not</summary>{"".join(other)}</details>') if other else ""
    cards.append(f"""
<section class="scope">
  <h2>{esc(title)} <span class="muted">({esc(sub)})</span></h2>
  <p class="score"><b>{k} of {len(window)}</b> checked tasks agreed. A Kind's tips pause below 8 in 10.</p>
  {f'<p class="def">What AIQ was told this Kind means: “{esc(d)}”</p>' if d else ''}
  {''.join(rows)}
  {other_html}
</section>""")

CSS = """
:root{--bg:#f7f7f5;--card:#fff;--ink:#1d1d1f;--muted:#5f6368;--line:#e2e2e0;--ok:#1a7f4b;--okb:#e3f4ea;--no:#b3261e;--nob:#fbe9e7;--accent:#2457c5}
@media (prefers-color-scheme:dark){:root{--bg:#161618;--card:#202124;--ink:#ececec;--muted:#a8abb0;--line:#34363a;--ok:#6fd39b;--okb:#163626;--no:#ff8a80;--nob:#3b1b19;--accent:#8ab4f8}}
*{box-sizing:border-box}body{margin:0;font:15px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif;background:var(--bg);color:var(--ink)}
main{max-width:980px;margin:0 auto;padding:28px 20px 60px}h1{font-size:24px;margin:0 0 6px}h2{font-size:19px;margin:34px 0 4px}
h4{margin:0 0 2px;font-size:12px;text-transform:uppercase;letter-spacing:.04em;color:var(--muted)}
.muted{color:var(--muted)}.lead{margin:0 0 14px;max-width:70ch}.score{margin:0 0 6px}
.def{font-size:13.5px;color:var(--muted);margin:4px 0 12px;max-width:80ch}
.tally{position:sticky;top:0;background:var(--bg);padding:10px 0;border-bottom:1px solid var(--line);z-index:2;font-size:14px}
.task{background:var(--card);border:1px solid var(--line);border-left:4px solid var(--line);border-radius:10px;padding:14px 16px;margin:12px 0}
.task.agree{border-left-color:var(--ok)}.task.disagree{border-left-color:var(--no)}
.task header{display:flex;gap:10px;align-items:center;font-size:13px;color:var(--muted);margin-bottom:8px}
.badge{margin-left:auto;font-weight:600;font-size:12px;padding:2px 8px;border-radius:99px}
.badge.ok{color:var(--ok);background:var(--okb)}.badge.no{color:var(--no);background:var(--nob)}
.prompt{display:flex;gap:10px;margin:6px 0}.prompt .n{flex:none;width:22px;height:22px;border-radius:50%;background:var(--line);font-size:12px;display:grid;place-items:center}
.pt{white-space:pre-wrap;word-break:break-word;max-height:16em;overflow:auto}
.said{display:grid;grid-template-columns:1fr 1fr;gap:14px;margin-top:12px;padding-top:10px;border-top:1px dashed var(--line)}
.kind{margin:0;font-weight:600}.side p{margin:2px 0}.why{font-size:13px;margin-top:6px}.why ul{margin:4px 0;padding-left:18px}
.mark{display:flex;flex-wrap:wrap;gap:8px;align-items:center;margin-top:12px;font-size:13px}
.mark button{font:inherit;padding:5px 12px;border-radius:8px;border:1px solid var(--line);background:transparent;color:var(--ink);cursor:pointer}
.mark button:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.mark button[aria-pressed=true]{background:var(--accent);border-color:var(--accent);color:#fff}
.others{margin:10px 0}.others summary{cursor:pointer;color:var(--accent)}.task.small{padding:10px 14px}
@media (max-width:640px){.said{grid-template-columns:1fr}}
"""
JS = """
const KEY='aiq-paused-review';let m={};try{m=JSON.parse(localStorage.getItem(KEY)||'{}')}catch(e){}
function draw(){const c={right:0,wrong:0,aiq:0,second:0,both:0,neither:0};let n=0;
 document.querySelectorAll('article.task[data-id]').forEach(a=>{n++;const v=m[a.dataset.id];if(v in c)c[v]++;
  a.querySelectorAll('.mark button').forEach(b=>b.setAttribute('aria-pressed',b.dataset.v===v?'true':'false'))});
 const done=Object.values(c).reduce((x,y)=>x+y,0);
 document.getElementById('tally').textContent=`You marked ${done} of ${n}. Where they agreed: right ${c.right}, wrong ${c.wrong}. `+
  `Where they differed: AIQ ${c.aiq}, second reading ${c.second}, both fit ${c.both}, neither ${c.neither}.`;}
document.addEventListener('click',e=>{const b=e.target.closest('.mark button');if(!b)return;
 const id=b.closest('article').dataset.id;m[id]=m[id]===b.dataset.v?undefined:b.dataset.v;
 try{localStorage.setItem(KEY,JSON.stringify(m))}catch(e){}draw();});draw();
"""
if paused:
    body = "".join(cards)
    lead = ("AIQ pauses tips for a Kind when a second, independent reading of its tasks agrees with AIQ on fewer "
            "than 8 in 10 of them. Below is every task behind each pause: your own words, what AIQ said, and what the "
            "second reading said. Mark who you think was right; your marks stay in this browser.")
else:
    body, lead = "<p>No Kind or job is paused right now.</p>", ""
page = f"""<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>AIQ: review the paused tips</title><style>{CSS}</style></head><body><main>
<h1>Review the paused tips</h1><p class="lead muted">{esc(lead)}</p>
<p class="muted">Made {esc(datetime.datetime.now().strftime('%b %d, %Y %H:%M'))} from your AIQ store. This page stays on this computer.</p>
<div class="tally" id="tally" aria-live="polite"></div>{body}</main><script>{JS}</script></body></html>"""
with open(OUT, "w", encoding="utf-8") as f:
    f.write(page)

print(f"paused scopes: {len(paused)} ({', '.join(('/'.join(s[1:]) + f' {k} of {len(w)}') for s, k, w, _ in paused)})")
print(f"check rows answered: {len(readings) + sum(seen_rows.values())}; left out {dict(seen_rows)}; check runs: {len(runs)}")
print(f"tasks on the page: {rows_out}; check readings read: {len(readings)}; page bytes: {os.path.getsize(OUT)}")
if OPEN and hasattr(os, "startfile"):
    os.startfile(OUT)
    print("opened in the default browser")
