#!/usr/bin/env python3
"""ui-craft-gate.py - the UI craft gate for AI-Forward.

Runs the Impeccable deterministic detector (`impeccable detect --json`) over a UI
surface and translates its findings into the pack's own review shape, per
`ui-craft-detection.md` CD11: every finding gains a **dimension** (one of the
DX22 rubric dimensions), a pack **severity** (Nit/Minor/Major/Blocker) with the
CD12 accessibility and token-discipline floors applied, and the owning pack
directive. Stdlib only; lives in the script bundle (deployed to
docs/ai-forward-pack/scripts/).

Why this exists: the detector is the rung-2 automated control under the pack's UI
craft doctrine (`continuous-improvement.md` CI6), but its raw output is not a
review finding. This performs the translation once, in a script, rather than by
hand in every session - and it applies the severity floors that U16 (accessibility
hard veto) and U3/U20 (token discipline) require and a linter's own defaults do not.

Modes
  (default)   report  - print the measurement + the rubric table; exit 0
  --gate      gate    - exit 1 if any Blocker-mapped finding is present
  --a11y-obligation   accessibility findings become Blockers (CD12)
  --markdown          emit a paste-ready markdown section for docs/reviews/ui-<surface>.md
  --json              emit the translated findings as JSON

Exit codes: 0 clean/report, 1 blockers present (with --gate), 2 detector unavailable
or it scanned nothing (CD9 - an empty corpus is a success-shaped failure).

Usage:
  ui-craft-gate.py <file-or-dir-or-url> [...] [--gate] [--a11y-obligation]
                   [--markdown] [--json] [--impeccable <cmd>]
"""
import argparse
import re
import shlex
import json
import os
import shutil
import subprocess
import sys
from collections import Counter

# Windows consoles default to cp1252, which cannot encode the glyphs this tool prints
# (DC-211/PLAT-A). Without this the script dies with UnicodeEncodeError on output alone.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass

# --- CD6: every detector rule maps to a pack cluster, a DX22 rubric dimension,
# --- and the pack directive it enforces. Keeping this table here is what keeps
# --- the detector subordinate to the pack rather than a second source of truth.
TOKEN = ("Token discipline", "13 Token discipline", "U3/U20")
A11Y = ("Accessibility & legibility", "14 Accessibility", "U16")
TELLS = ("Generic-AI-look tells", "17 Craft", "DX3")
CRAFT = ("Hierarchy, rhythm & space", "17 Craft", "DX12/DX13/DX17")
MOTION = ("Motion & stability", "15 Performance & stability", "U10/U17/DX19")
COPY = ("Copy", "16 Content & copy", "U11/DX21")
STATE = ("State & overflow integrity", "12 State completeness", "U9/DX9/DX16")

CLUSTERS = {
    # Token discipline - U3/U20, enforced against DESIGN.md
    "design-system-color": TOKEN, "design-system-font": TOKEN,
    "design-system-font-size": TOKEN, "design-system-radius": TOKEN,
    # Accessibility & legibility - U16
    "low-contrast": A11Y, "gray-on-color": A11Y, "tiny-text": A11Y,
    "undersized-ui-text": A11Y, "skipped-heading": A11Y,
    "justified-text": A11Y, "all-caps-body": A11Y,
    # The generic-AI-look tells - DX3, mechanized
    "ai-color-palette": TELLS, "cream-palette": TELLS, "gradient-text": TELLS,
    "side-tab": TELLS, "nested-cards": TELLS, "icon-tile-stack": TELLS,
    "kicker-above-heading": TELLS, "hero-eyebrow-chip": TELLS,
    "numbered-section-labels": TELLS, "overused-font": TELLS,
    "radial-halo": TELLS, "radial-spotlight-glow": TELLS, "dark-glow": TELLS,
    "codex-grid-background": TELLS, "repeating-stripes-gradient": TELLS,
    "gpt-thin-border-wide-shadow": TELLS, "border-accent-on-rounded": TELLS,
    "italic-serif-display": TELLS, "shape-assembled-illustration": TELLS,
    # Hierarchy, rhythm & space - DX12/DX13/DX15/DX17
    "flat-type-hierarchy": CRAFT, "heading-rhythm": CRAFT,
    "monotonous-spacing": CRAFT, "cramped-padding": CRAFT,
    "oversized-h1": CRAFT, "line-length": CRAFT, "tight-leading": CRAFT,
    "wide-tracking": CRAFT, "extreme-negative-tracking": CRAFT,
    "edge-flush-cards": CRAFT,
    # Motion & stability - U10/U17/DX19-DX20
    "layout-transition": MOTION, "bounce-easing": MOTION, "marquee": MOTION,
    "pulsing-dot": MOTION, "blinking-cursor": MOTION,
    "image-hover-transform": MOTION,
    # Copy - U11/DX21
    "marketing-buzzword": COPY, "em-dash-overuse": COPY,
    "aphoristic-cadence": COPY, "theater-slop-phrase": COPY,
    "repeated-container-text": COPY,
    # State & overflow integrity - U9/DX9/DX16
    "broken-image": STATE, "text-overflow": STATE, "text-occlusion": STATE,
    "clipped-overflow-container": STATE, "content-hidden-at-rest": STATE,
    "first-viewport-column-overflow": STATE, "body-text-viewport-edge": STATE,
    "script-error": STATE,
}

# CD7: a rule the pack has not yet articulated is still actionable, but it is a
# candidate doctrine change - flagged, never silently absorbed.
UNMAPPED = ("Unmapped (candidate doctrine)", "-- unmapped --", "CD7")

SEV_ORDER = ["Blocker", "Major", "Minor", "Nit"]
# The detector's own severity is advisory (CD12); this is the base mapping.
BASE_SEVERITY = {"error": "Major", "warning": "Minor", "info": "Nit"}


def resolve_detector(explicit=None):
    """Return an argv prefix that runs the detector, or None.

    Order: explicit --impeccable > `impeccable` on PATH > local node_modules >
    `npx impeccable`. Establishing the tool rather than assuming it is NG1.
    """
    if explicit:
        # A quoted path with spaces must survive the split, and on Windows POSIX-mode
        # shlex eats the backslashes in `C:\Program Files\...` (DC-211). Non-POSIX mode
        # keeps them but leaves the quotes on the token, so they are stripped here.
        if os.name == "nt":
            return [t.strip('"') for t in shlex.split(explicit, posix=False) if t.strip('"')]
        return shlex.split(explicit)
    exe = shutil.which("impeccable")
    if exe:
        return [exe]
    local = os.path.join("node_modules", "impeccable", "cli", "bin", "cli.js")
    if os.path.isfile(local) and shutil.which("node"):
        return [shutil.which("node"), local]
    if shutil.which("npx"):
        return [shutil.which("npx"), "--yes", "impeccable"]
    return None


def run_detector(prefix, targets):
    """Run `detect --json` and return the parsed finding list.

    The detector exits non-zero when it finds anti-patterns, which is a *result*,
    not a failure - so the exit code is deliberately not treated as an error
    (`end-to-end-integrity.md` E14: read the state, do not read the exit code).
    """
    cmd = prefix + ["detect", "--json"] + list(targets)
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8",
                              errors="replace", timeout=600)
    except (OSError, subprocess.SubprocessError) as exc:
        return None, "could not run the detector: %s" % exc
    out = (proc.stdout or "").strip()
    if not out:
        # A detector that produced NO JSON did not scan anything, and "nothing scanned"
        # must never render as "nothing wrong". The detector uses exit 1 for "found
        # anti-patterns", so exit-1-with-empty-output is ambiguous between a real clean
        # run and a failed invocation - and defaulting that to clean is exactly the
        # failure end-to-end-integrity E13 names (a gate's green result is evidence the
        # gate passed, not that its contents passed) and defect class E2E-H.
        #
        # Observed: `npx --yes impeccable` on a machine without the package exits 1 with
        # "'impeccable' is not recognized" on stderr and no stdout, and this function
        # previously returned [] - so the gate reported a clean scan of a file it had
        # never opened.
        detail = (proc.stderr or "").strip() or "the detector produced no output"
        return None, (
            "the detector produced no JSON, so nothing was scanned (exit %d): %s\n"
            "  A clean report here would be a false pass. Install the detector "
            "(`npm i -g impeccable`) or pass --impeccable <command>." % (proc.returncode, detail[:400]))
    # A detector can also fail while emitting perfectly valid EMPTY JSON: URL scanning
    # without puppeteer prints "[]" to stdout, "Error: puppeteer is required" to stderr,
    # and exits 0. That is the same false pass as the empty-output case wearing a
    # different costume - and it survived the first fix, in this very function, because
    # the fix stopped at the instance (RIG-C). Empty findings plus an error on stderr is
    # never reported as a clean scan.
    stderr = (proc.stderr or "").strip()
    if stderr and re.search(r"\b(error|required|not recognized|cannot|failed|unable)\b",
                            stderr, re.I):
        stripped = out.strip()
        if stripped in ("[]", "[ ]", ""):
            return None, (
                "the detector reported no findings but also reported an error, so nothing "
                "was scanned: %s\n  A clean report here would be a false pass."
                % stderr[:400])

    start = out.find("[")
    if start == -1:
        return None, (
            "the detector emitted output but no JSON array, so no findings could be read: %s"
            % out[:300])
    try:
        return json.loads(out[start:]), None
    except json.JSONDecodeError as exc:
        return None, "could not parse detector JSON: %s" % exc


def translate(findings, a11y_obligation):
    """CD11 - give each finding the pack's shape; CD12 - apply the severity floors."""
    rows = []
    for f in findings:
        rule = f.get("antipattern") or "unknown"
        cluster, dimension, directive = CLUSTERS.get(rule, UNMAPPED)
        severity = BASE_SEVERITY.get(str(f.get("severity", "")).lower(), "Minor")
        # CD12 floor 1 - accessibility outranks the tool's own opinion.
        if cluster == A11Y[0]:
            severity = "Blocker" if a11y_obligation else "Major"
        # CD12 floor 2 - token discipline is the contract the design language rests on.
        elif cluster == TOKEN[0] and severity in ("Minor", "Nit"):
            severity = "Major"
        line = f.get("line") or 0
        loc = f.get("file") or "?"
        rows.append({
            "rule": rule,
            "location": "%s%s" % (loc, ":%d" % line if line else ""),
            "dimension": dimension,
            "cluster": cluster,
            "directive": directive,
            "severity": severity,
            "evidence": "%s - `%s`" % (f.get("name") or rule, f.get("snippet") or ""),
            "fix": f.get("description") or "",
            "confidence": "Verified",  # CD13 - observed by execution
        })
    rows.sort(key=lambda r: (SEV_ORDER.index(r["severity"]), r["cluster"], r["rule"]))
    return rows


def measurement(rows):
    """DX23 - measure before you diagnose. Counts are the diagnosis."""
    return Counter(r["rule"] for r in rows), Counter(r["cluster"] for r in rows), \
        Counter(r["severity"] for r in rows)


def render_markdown(rows, targets):
    by_rule, by_cluster, by_sev = measurement(rows)
    out = ["## UI craft detection", "",
           "_Deterministic control (`ui-craft-detection.md`). Target(s): %s._"
           % ", ".join(targets), ""]
    out += ["### Measurement (DX23)", "",
            "| Severity | Count |", "|---|---|"]
    for sev in SEV_ORDER:
        if by_sev.get(sev):
            out.append("| **%s** | %d |" % (sev, by_sev[sev]))
    out += ["", "| Cluster | Count |", "|---|---|"]
    for cluster, n in by_cluster.most_common():
        out.append("| %s | %d |" % (cluster, n))
    out += ["", "| Rule | Count |", "|---|---|"]
    for rule, n in by_rule.most_common():
        out.append("| `%s` | %d |" % (rule, n))
    out += ["", "### Findings (DX22 shape)", "",
            "| Severity | Dimension | Location | Evidence | Fix | Directive | Conf. |",
            "|---|---|---|---|---|---|---|"]
    for r in rows:
        out.append("| %s | %s | `%s` | %s | %s | %s | %s |" % (
            r["severity"], r["dimension"], r["location"],
            r["evidence"].replace("|", "\\|"), r["fix"].replace("|", "\\|"),
            r["directive"], r["confidence"]))
    if not rows:
        out.append("| - | - | - | _no findings_ | - | - | - |")
    out += ["", "> **CD13/CD14 - a clean run is a floor, never a verdict.** The detector "
            "cannot judge archetype fit, information architecture, whether the hard states "
            "exist at all, whether the copy is true, or whether the focal point is defended. "
            "Those remain the human and adversarial layers."]
    return "\n".join(out)


def render_text(rows, targets):
    by_rule, by_cluster, by_sev = measurement(rows)
    lines = ["ui-craft-gate: %s" % ", ".join(targets), ""]
    if not rows:
        lines.append("  no findings (CD13: this is a floor, not a verdict)")
        return "\n".join(lines)
    lines.append("  measurement (DX23):")
    for sev in SEV_ORDER:
        if by_sev.get(sev):
            lines.append("    %-8s %d" % (sev, by_sev[sev]))
    lines.append("")
    for cluster, n in by_cluster.most_common():
        lines.append("    %-30s %d" % (cluster, n))
    lines.append("")
    lines.append("  findings:")
    for r in rows:
        lines.append("    [%s] %s  %s" % (r["severity"], r["rule"], r["location"]))
        lines.append("        %s -> %s" % (r["dimension"], r["directive"]))
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description="Run the UI craft detector and translate "
                                             "its findings into the pack's review shape.")
    ap.add_argument("targets", nargs="+", help="file, directory, or URL to scan")
    ap.add_argument("--gate", action="store_true",
                    help="exit 1 if any Blocker-mapped finding is present")
    ap.add_argument("--a11y-obligation", action="store_true",
                    help="the product is under an accessibility obligation; "
                         "accessibility findings become Blockers (CD12)")
    ap.add_argument("--markdown", action="store_true",
                    help="emit a paste-ready markdown review section")
    ap.add_argument("--json", action="store_true", dest="as_json",
                    help="emit the translated findings as JSON")
    ap.add_argument("--impeccable", default=None,
                    help="explicit detector command (default: auto-resolve)")
    args = ap.parse_args(argv)

    prefix = resolve_detector(args.impeccable)
    if not prefix:
        sys.stderr.write(
            "ui-craft-gate: detector not available. Install it with "
            "`npm i -D impeccable` (Apache-2.0) or pass --impeccable <cmd>.\n"
            "  See ui-craft-detection.md CD1.\n")
        return 2

    findings, err = run_detector(prefix, args.targets)
    if err:
        sys.stderr.write("ui-craft-gate: %s\n" % err)
        return 2

    # CD9 / E14 - an empty corpus exits clean and proves nothing. Guard the
    # success-shaped failure: if nothing was scanned, say so rather than pass.
    local = [t for t in args.targets if not t.startswith(("http://", "https://"))]
    missing = [t for t in local if not os.path.exists(t)]
    if missing:
        sys.stderr.write("ui-craft-gate: target(s) do not exist: %s\n"
                         "  A detector that scanned nothing is not a passing gate (CD9).\n"
                         % ", ".join(missing))
        return 2

    rows = translate(findings, args.a11y_obligation)

    if args.as_json:
        print(json.dumps(rows, indent=2))
    elif args.markdown:
        print(render_markdown(rows, args.targets))
    else:
        print(render_text(rows, args.targets))

    if args.gate and any(r["severity"] == "Blocker" for r in rows):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
