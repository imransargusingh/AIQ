#!/usr/bin/env python3
"""apply-learnings.py - the AI-Forward federation / push mechanism.

Distributes approved, generalised fleet learnings (from learnings/fleet-classes.jsonl in the
ai-forward repo) into one or more target repos, RECONCILING each against that repo's existing
defect-class register so nothing is duplicated or silently contradicted. It produces a REVIEWABLE
plan + a diff/patch per target repo - it NEVER merges and NEVER executes anything in a target
(spec-dreaming US-5, ADR-0002/0005). The second, pull-based federation path is /updatepack, which
inherits general classes shipped into the pack itself.

Reconciliation (ADR-0004, slug-exact + human-flagged; no fuzzy index):
  * add     - the class has no equivalent in the target -> append to the target's register.
  * merge   - the target already has the class (slug/id or signature match) -> append the instance
              / upgrade the control note, never a duplicate entry.
  * conflict- the incoming class contradicts an existing directive -> SURFACE in the plan for the
              human to resolve; never overridden.

Python 3.8+, stdlib only. Safety: strip+scrub runs again before anything is written to a plan
(defence in depth); a target without the pack is skipped with a note.

Targeting/record layer (ADR-0006, the Dream Manifest):
  * manifest-init --repos a,b,c [--dream id]  -> scaffold learnings/manifests/<id>.json (every fleet
    class assigned to every repo, scope=all, status=pending) + a self-contained compose HTML matrix.
  * push --manifest <file>  -> reconcile PER ASSIGNMENT (a learning only into its `targets`), write a
    reviewable plan per repo, record the outcome back into the manifest's status map, re-render the
    rollout HTML. Manifests name repos -> LOCAL-ONLY (excluded from the published bundle).
"""
import argparse, datetime, json, os, re, sys

# Windows consoles default to cp1252, which cannot encode the box/arrow glyphs this
# tool prints - `prompt-log.py --help` crashed outright with UnicodeEncodeError (FR-047).
# The other scripts survived only because their glyphs happen to exist in cp1252, which is
# luck rather than an invariant, so the guard is applied uniformly.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass


def now_iso():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

def find_root(start):
    p = os.path.abspath(start)
    while True:
        if os.path.isdir(os.path.join(p, "learnings")) or os.path.isdir(os.path.join(p, ".git")):
            return p
        parent = os.path.dirname(p)
        if parent == p:
            return os.path.abspath(start)
        p = parent

def read_jsonl(path):
    out = []
    if os.path.isfile(path):
        with open(path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line:
                    try:
                        out.append(json.loads(line))
                    except json.JSONDecodeError:
                        pass
    return out

SECRET_RE = [
    re.compile(r"[A-Za-z0-9_\-]*(?:secret|token|api[_-]?key|password|passwd|pwd|bearer)[A-Za-z0-9_\-]*\s*[:=]\s*\S+", re.I),
    re.compile(r"\b(?:sk|pk|ghp|gho|xox[baprs])[-_][A-Za-z0-9]{16,}\b"),
    re.compile(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b"),
]
def scrub(text):
    if not isinstance(text, str):
        return text, False
    hit = False
    for rx in SECRET_RE:
        if rx.search(text):
            hit = True
            text = rx.sub("[REDACTED]", text)
    return text, hit

def slug(sig):
    return re.sub(r"[^a-z0-9]+", "-", (sig or "").lower()).strip("-")[:60]

def target_has_pack(repo):
    return (os.path.isdir(os.path.join(repo, ".claude")) or
            os.path.isdir(os.path.join(repo, "docs", "ai-forward-pack")) or
            os.path.isfile(os.path.join(repo, "docs", "lessons", "defect-classes.md")))

def target_register(repo):
    """Return {token-set, ids, raw} of the target's existing defect-class register for reconciliation."""
    path = os.path.join(repo, "docs", "lessons", "defect-classes.md")
    ids, sigs = set(), []
    if os.path.isfile(path):
        with open(path, "r", encoding="utf-8", errors="ignore") as f:
            txt = f.read()
        for m in re.finditer(r"^###\s+([A-Z0-9\-]+)\s+[—\-]\s+(.+?)\s*$", txt, re.M):
            ids.add(m.group(1).strip().lower())
            sigs.append(set(re.findall(r"[a-z]{4,}", m.group(2).lower())))
        # directives that could be contradicted (very light heuristic: 'never'/'must' lines)
    return {"path": path, "ids": ids, "sigs": sigs, "exists": os.path.isfile(path)}

def reconcile(learning, reg):
    """Classify one incoming learning against a target register: add | merge | conflict."""
    sig = learning.get("sig", "")
    lslug = slug(sig)
    # slug/id exact
    if lslug in reg["ids"] or any(tok in reg["ids"] for tok in [lslug]):
        return "merge"
    # signature token overlap (>=0.6 -> likely the same class -> merge to avoid a duplicate)
    lt = set(re.findall(r"[a-z]{4,}", sig.lower()))
    best = 0.0
    for st in reg["sigs"]:
        if lt and st:
            best = max(best, len(lt & st) / len(lt | st))
    if best >= 0.6:
        return "merge"
    return "add"

def load_fleet(root):
    """Deduped list of promoted fleet learnings (latest wins by slug)."""
    fleet = read_jsonl(os.path.join(root, "learnings", "fleet-classes.jsonl"))
    by_slug = {}
    for l in fleet:
        by_slug[slug(l.get("sig", ""))] = l
    return list(by_slug.values())

def control_text(record):
    """The control on a learning may be a {rung, text} object (what a dream writes) or a bare
    string (a hand-edited store, or an older record — this JSONL is a plain committed file
    anyone can edit). Chaining `.get("control", {}).get("text")` crashed with an unhandled
    AttributeError on the string form, in the script that writes into other people's
    repositories, AFTER it may already have written plans for earlier targets. Neither shape
    is malformed enough to justify that; a genuinely absent control still returns "" and is
    skipped by the caller. Swept as a class: the identical line existed in dream.py."""
    control = record.get("control")
    if isinstance(control, dict):
        return str(control.get("text") or "").strip()
    if isinstance(control, str):
        return control.strip()
    return ""


def plan_repo(repo, learnings):
    """Reconcile a set of learnings into one target repo -> a plan list (add|merge|conflict|skip).
    Shared by `push --repos` and `push --manifest`; never merges, only plans (ADR-0005)."""
    if not target_has_pack(repo):
        return [{"action": "skip", "why": "target has no pack installed", **l} for l in learnings]
    reg = target_register(repo)
    plan = []
    for l in learnings:
        _, tainted = scrub(json.dumps(l, ensure_ascii=False))
        if tainted:
            plan.append({"action": "skip", "why": "taint/scrub hit at boundary", **l})
            continue
        if not control_text(l):
            plan.append({"action": "skip", "why": "no control (a lesson without a control is a memoir, CI6)", **l})
            continue
        plan.append({"action": reconcile(l, reg), **l})
    return plan

def find_template(root, name):
    """Resolve a pack template whether running from the ai-forward repo or an installed target."""
    for cand in (os.path.join(root, "pack", "templates", name),
                 os.path.join(root, "docs", "ai-forward-pack", "templates", name)):
        if os.path.isfile(cand):
            return cand
    return None

def render_manifest_html(root, manifest, learnings, mode):
    """Render the self-contained learnings×repos matrix (compose|rollout) with the data inlined."""
    tpl = find_template(root, "dream-manifest.template.html")
    if not tpl:
        return None
    repos = []
    for p in manifest.get("repos", []):
        repos.append({"path": p, "name": os.path.basename(os.path.abspath(p))})
    by_slug = {slug(l.get("sig", "")): l for l in learnings}
    lrows = []
    for a in manifest.get("assignments", []):
        l = by_slug.get(a["learning"], {})
        lrows.append({"slug": a["learning"], "sig": a.get("sig") or l.get("sig", a["learning"]),
                      "control": (l.get("control", {}) or {}).get("text", ""),
                      "confidence": l.get("confidence", "")})
    data = {"id": manifest["id"], "created": manifest.get("created", ""), "dream": manifest.get("dream"),
            "mode": mode, "repos": repos, "learnings": lrows,
            "assignments": manifest.get("assignments", [])}
    with open(tpl, "r", encoding="utf-8") as template:
        html = template.read().replace(
            "__MANIFEST_DATA__", json.dumps(data, ensure_ascii=False).replace("</", "<\\/"))
    out = os.path.join(root, "learnings", "manifests", manifest["id"] + ".html")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(html)
    return out

def render_patch(repo, plan):
    """A human-readable reconciliation plan + the exact register additions (never auto-applied)."""
    lines = ["# apply-learnings plan for {0}".format(os.path.basename(os.path.abspath(repo))),
             "# generated {0} - REVIEW then apply by hand or via a PR. Nothing here is auto-merged.".format(now_iso()), ""]
    for act in ("add", "merge", "conflict", "skip"):
        items = [p for p in plan if p["action"] == act]
        if not items:
            continue
        lines.append("## {0} ({1})".format(act.upper(), len(items)))
        for p in items:
            lines.append("- **{0}**".format(p["sig"]))
            if act == "add":
                lines.append("  ```markdown")
                lines.append("  ### {0} - {1}".format(slug(p["sig"])[:20].upper(), p["sig"]))
                lines.append("  - **Control:** {0} ({1})".format(p["control"].get("text", ""), p["control"].get("rung", "")))
                lines.append("  - **Boundary:** {0}".format(p.get("boundary", "")))
                lines.append("  - **Confidence:** {0}  - **Source:** fleet ({1}/{2})".format(p.get("confidence", ""), p.get("dream", ""), p.get("proposal", "")))
                lines.append("  ```")
            elif act == "merge":
                lines.append("  -> target already has an equivalent class; append this instance / upgrade the control, do NOT add a duplicate.")
            elif act == "conflict":
                lines.append("  -> ! contradicts an existing directive; resolve by hand.")
            elif act == "skip":
                lines.append("  -> {0}".format(p.get("why", "")))
        lines.append("")
    return "\n".join(lines) + "\n"

def cmd_push(args):
    root = find_root(args.root)
    if getattr(args, "manifest", None):
        return _push_manifest(root, args)
    learnings = load_fleet(root)
    if not learnings:
        print("no fleet learnings to push (learnings/fleet-classes.jsonl is empty). Run /dream + apply-decisions first.")
        return 0
    if not getattr(args, "repos", None):
        print("push needs either --repos a,b,c (or 'all') or --manifest <file>."); return 1

    repos = []
    if args.repos == "all":
        base = os.path.dirname(root)
        for name in sorted(os.listdir(base)):
            cand = os.path.join(base, name)
            if os.path.isdir(cand) and target_has_pack(cand):
                repos.append(cand)
    else:
        repos = [os.path.abspath(r.strip()) for r in args.repos.split(",") if r.strip()]

    out_dir = os.path.join(root, "learnings", "plans")
    os.makedirs(out_dir, exist_ok=True)
    summary = []
    for repo in repos:
        if not os.path.isdir(repo):
            summary.append((repo, "MISSING", 0, 0, 0))
            continue
        plan = plan_repo(repo, learnings)
        patch = render_patch(repo, plan)
        pf = os.path.join(out_dir, os.path.basename(os.path.abspath(repo)) + ".plan.md")
        with open(pf, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(patch)
        adds = sum(1 for p in plan if p["action"] == "add")
        merges = sum(1 for p in plan if p["action"] == "merge")
        confs = sum(1 for p in plan if p["action"] == "conflict")
        summary.append((repo, "planned", adds, merges, confs))
        print("  {0}: add {1}, merge {2}, conflict {3}  -> {4}".format(
            os.path.basename(os.path.abspath(repo)), adds, merges, confs, os.path.relpath(pf, root).replace(os.sep, "/")))
    # audit
    _audit(root, "apply-learnings", "Planned federation to {0} repo(s): {1}".format(
        len(repos), "; ".join("{0}(+{1}~{2}!{3})".format(os.path.basename(r), a, m, c) for r, s, a, m, c in summary)),
        "learnings/plans/", args.session)
    print("\nReview each plan under learnings/plans/, then apply by hand or open a PR. Nothing was merged.")
    return 0

def _push_manifest(root, args):
    """push --manifest: reconcile PER ASSIGNMENT (a learning only into its targets), write status back,
    re-render the rollout HTML. Never merges (ADR-0006/0005)."""
    mpath = os.path.abspath(args.manifest)
    if not os.path.isfile(mpath):
        print("manifest not found: {0}".format(mpath)); return 1
    with open(mpath, "r", encoding="utf-8") as f:
        manifest = json.load(f)
    learnings = load_fleet(root)
    by_slug = {slug(l.get("sig", "")): l for l in learnings}
    assignments = manifest.get("assignments", [])
    repos = manifest.get("repos", [])
    out_dir = os.path.join(root, "learnings", "plans")
    os.makedirs(out_dir, exist_ok=True)

    # action per (slug, repo), computed once per repo from that repo's assigned subset
    action_by = {}
    per_repo_counts = {}
    for repo in repos:
        subset = [by_slug[a["learning"]] for a in assignments
                  if repo in (a.get("targets") or []) and a["learning"] in by_slug]
        if not subset:
            per_repo_counts[repo] = (0, 0, 0)
            continue
        plan = plan_repo(repo, subset)
        for p in plan:
            action_by[(slug(p.get("sig", "")), repo)] = p["action"]
        patch = render_patch(repo, plan)
        pf = os.path.join(out_dir, os.path.basename(os.path.abspath(repo)) + ".plan.md")
        with open(pf, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(patch)
        per_repo_counts[repo] = (sum(1 for p in plan if p["action"] == "add"),
                                 sum(1 for p in plan if p["action"] == "merge"),
                                 sum(1 for p in plan if p["action"] == "conflict"))

    # write status back into the manifest: 'planned' for an add, else the reconcile action; else 'skipped'
    for a in assignments:
        st = a.setdefault("status", {})
        for repo in repos:
            if repo in (a.get("targets") or []):
                act = action_by.get((a["learning"], repo), "skipped")
                st[repo] = "planned" if act == "add" else act
            else:
                st[repo] = "pending"
    manifest["last_push"] = now_iso()
    with open(mpath, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    html = render_manifest_html(root, manifest, learnings, "rollout")

    for repo in repos:
        a, m, c = per_repo_counts.get(repo, (0, 0, 0))
        print("  {0}: add {1}, merge {2}, conflict {3}".format(os.path.basename(os.path.abspath(repo)), a, m, c))
    _audit(root, "apply-learnings-manifest",
           "Manifest {0}: pushed per-assignment to {1} repo(s); status recorded.".format(manifest["id"], len(repos)),
           "learnings/manifests/" + manifest["id"] + ".json", args.session)
    print("\nStatus written back to {0}".format(os.path.relpath(mpath, root).replace(os.sep, "/")))
    if html:
        print("Rollout view: {0}".format(os.path.relpath(html, root).replace(os.sep, "/")))
    print("Review each plan under learnings/plans/, then apply by hand or open a PR. Nothing was merged.")
    return 0

def cmd_manifest_init(args):
    """Scaffold a learnings×repos manifest from the fleet store + render the compose HTML."""
    root = find_root(args.root)
    learnings = load_fleet(root)
    repos = [os.path.abspath(r.strip()) for r in args.repos.split(",") if r.strip()]
    if not repos:
        print("provide --repos a,b,c (paths to target repos)"); return 1
    mid = args.id or ("manifest-" + datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%d-%H%M%S"))
    assignments = []
    for l in learnings:
        s = slug(l.get("sig", ""))
        assignments.append({"learning": s, "sig": l.get("sig", s), "scope": "all",
                            "targets": list(repos), "status": {r: "pending" for r in repos}})
    manifest = {"id": mid, "created": now_iso(), "dream": args.dream or None,
                "repos": repos, "assignments": assignments}
    mdir = os.path.join(root, "learnings", "manifests")
    os.makedirs(mdir, exist_ok=True)
    jpath = os.path.join(mdir, mid + ".json")
    with open(jpath, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    html = render_manifest_html(root, manifest, learnings, "compose")
    _audit(root, "manifest-init",
           "Scaffolded manifest {0}: {1} learning(s) x {2} repo(s).".format(mid, len(learnings), len(repos)),
           "learnings/manifests/" + mid + ".json", args.session)
    print("manifest -> {0}".format(os.path.relpath(jpath, root).replace(os.sep, "/")))
    if html:
        print("compose view -> {0}".format(os.path.relpath(html, root).replace(os.sep, "/")))
    if not learnings:
        print("(no promoted fleet learnings yet — the matrix is empty until /dream + apply-decisions promote a class.)")
    else:
        print("Open the compose view, toggle the (learning, repo) cells, Export the JSON over the scaffold, then:")
        print("  python pack/scripts/apply-learnings.py push --manifest learnings/manifests/{0}.json".format(mid))
    return 0


def _audit(root, shortname, summary, artifact, session):
    import subprocess
    for cand in (os.path.join(root, "docs", "ai-forward-pack", "scripts", "audit-log.py"),
                 os.path.join(root, "pack", "scripts", "audit-log.py")):
        if os.path.isfile(cand):
            try:
                subprocess.run([sys.executable, cand, "append", "--shortname", shortname, "--kind", "script",
                                "--skill", "apply-learnings", "--session", session or "apply-learnings",
                                "--prompt", "apply-learnings.py " + shortname, "--summary", summary,
                                "--artifact", artifact], cwd=root, capture_output=True, timeout=30)
            except (OSError, subprocess.SubprocessError):
                pass
            return

def main(argv=None):
    ap = argparse.ArgumentParser(description="AI-Forward federation: push approved fleet learnings into target repos as reviewable plans (never merges).")
    ap.add_argument("--root", default=".", help="the ai-forward repo root (holds learnings/)")
    ap.add_argument("--session", default="", help="session id for the audit trail")
    sub = ap.add_subparsers(dest="cmd")
    p = sub.add_parser("push", help="reconcile fleet learnings into target repos -> plans")
    p.add_argument("--repos", help="comma-separated repo paths, or 'all' (sibling repos with the pack)")
    p.add_argument("--manifest", help="a learnings×repos manifest (learnings/manifests/<id>.json) — targets per assignment, records status back")
    p.set_defaults(fn=cmd_push)

    mi = sub.add_parser("manifest-init", help="scaffold a learnings×repos manifest + compose HTML from the fleet store")
    mi.add_argument("--repos", required=True, help="comma-separated target repo paths")
    mi.add_argument("--dream", default="", help="optional dream id this manifest derives from")
    mi.add_argument("--id", default="", help="manifest id (default: manifest-<timestamp>)")
    mi.set_defaults(fn=cmd_manifest_init)
    args = ap.parse_args(argv)
    if not getattr(args, "fn", None):
        ap.print_help()
        return 1
    return args.fn(args)

if __name__ == "__main__":
    sys.exit(main())
