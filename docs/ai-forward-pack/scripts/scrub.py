#!/usr/bin/env python3
"""scrub.py — first-pass PII/secret redaction for Markdown (deployable).

Redacts OBVIOUS personal data (emails) and common secret shapes (token prefixes, private keys)
from Markdown, with --check (report; nonzero on hit) and --write (redact in place). It is the
on-brand, dependency-free analogue of Squad's `scrub-emails`.

  ⚠ THIS IS A FIRST-PASS, NOT CI-GRADE. Regex recall is limited — it will miss PII/secrets that
  NLP (Microsoft Presidio) or entropy/rule engines (gitleaks, detect-secrets, TruffleHog) catch.
  Use those in CI for real enforcement, and git-filter-repo / BFG to PURGE anything already
  committed (scrub redacts going forward; it does not rewrite history).

Design: docs/design/rai-and-scrub.md. Stdlib only. Never prints a raw secret (preview is redacted).

Usage
  scrub.py [paths...] [--check | --write] [--aggressive] [--json]
  (default paths: docs/ and pack/ ; default mode: --check)
Exit: --check -> 1 if any finding else 0 ; --write -> 0.
"""
import argparse, json, os, re, sys

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


# Pattern: Pattern registry — (category, compiled regex). Email + known secret prefixes by default;
# the broad high-length token is gated behind --aggressive to control false positives.
_BASE = [
    # GitHub noreply addresses are a *pseudonymous* identifier - they are literally
    # what GitHub issues to keep a real address private, and every Co-authored-by
    # trailer in this repo carries one. Flagging them made the tool report the
    # repo's own commit metadata as PII, which trains people to ignore it (FR-042).
    ("email",  re.compile(r"(?!\S*@users\.noreply\.github\.com)"
                          r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}")),
    ("secret", re.compile(r"\b(?:ghp_[A-Za-z0-9]{20,}"
                          r"|github_pat_[A-Za-z0-9_]{20,}"
                          r"|xox[baprs]-[A-Za-z0-9\-]{10,}"
                          r"|sk-[A-Za-z0-9]{20,}"
                          r"|AKIA[0-9A-Z]{16}"
                          r"|AIza[0-9A-Za-z_\-]{35})\b")),
    ("private-key", re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")),
]
_AGGRESSIVE = ("token", re.compile(r"\b[A-Za-z0-9_\-]{32,}\b"))
_REDACTED = re.compile(r"\[REDACTED:[a-z\-]+\]")  # idempotency guard


def _preview(category, match):
    """Return a SAFE preview — never the raw secret. Show only shape, not value."""
    if category == "email":
        # a••••@d•••• : enough to locate, not to read
        local, _, dom = match.partition("@")
        return f"{local[:1]}{'•' * max(len(local) - 1, 1)}@{dom[:1]}{'•' * 4}"
    return f"{match[:4]}••••"


def _patterns(aggressive):
    return _BASE + ([_AGGRESSIVE] if aggressive else [])


def scan_text(text, aggressive=False):
    """Yield (lineno, category, raw_match) for each finding (skips already-redacted spans)."""
    pats = _patterns(aggressive)
    for i, line in enumerate(text.splitlines(), 1):
        if _REDACTED.search(line):
            # remove redacted spans so they don't trip the broad pattern
            line = _REDACTED.sub("", line)
        for category, rx in pats:
            for m in rx.finditer(line):
                yield i, category, m.group(0)


def redact_text(text, aggressive=False):
    """Return text with every match replaced by [REDACTED:<category>] (idempotent)."""
    for category, rx in _patterns(aggressive):
        text = rx.sub(f"[REDACTED:{category}]", text)
    return text


def _iter_md(paths):
    for p in paths:
        if os.path.isfile(p) and p.endswith(".md"):
            yield p
        elif os.path.isdir(p):
            for dirpath, dirs, files in os.walk(p):
                dirs[:] = [d for d in dirs if d not in (".git", "node_modules", "__pycache__")]
                for f in files:
                    if f.endswith(".md"):
                        yield os.path.join(dirpath, f)


def _report_undecodable(undecodable, as_json):
    """Name every file that is not valid UTF-8. It was scanned lossily and left untouched;
    the exit code is non-zero so a --write run cannot look clean while files were skipped."""
    if undecodable and not as_json:
        print("\n  %d file(s) are not valid UTF-8 - scanned lossily, NOT rewritten:" % len(undecodable))
        for path, why in undecodable:
            print("    %s  (%s)" % (path, why))


def main():
    ap = argparse.ArgumentParser(description="First-pass PII/secret redaction for Markdown (NOT CI-grade).")
    ap.add_argument("paths", nargs="*", default=["docs", "pack"])
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="report findings, nonzero exit on any (default)")
    mode.add_argument("--write", action="store_true", help="redact findings in place")
    ap.add_argument("--aggressive", action="store_true", help="also flag any 32+ char token (more false positives)")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()
    paths = args.paths or ["docs", "pack"]

    findings, scanned, written, undecodable = [], 0, 0, []
    for path in _iter_md(paths):
        scanned += 1
        try:
            with open(path, "rb") as f:
                raw = f.read()
        except OSError:
            continue
        # Decode STRICTLY first. errors="replace" turns every byte it cannot read into
        # U+FFFD, and --write then persists that loss: the tool meant to protect the file
        # silently destroys the bytes it could not read. So a file that will not decode is
        # reported and NEVER rewritten. Scanning still runs on the lossy text - reporting a
        # secret costs nothing, missing one does.
        lossy = False
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError as exc:
            undecodable.append((path, "byte %d: %s" % (exc.start, exc.reason)))
            text = raw.decode("utf-8", "replace")
            lossy = True
        if args.write and not lossy:
            new = redact_text(text, args.aggressive)
            if new != text:
                tmp = path + ".scrub.tmp"
                # newline="" - the decode above did no newline translation either, so a
                # redaction rewrites only the matched spans and leaves the file's own line
                # endings byte-identical (PLAT-A: a CRLF file must not come back as LF).
                with open(tmp, "w", encoding="utf-8", newline="") as fh:
                    fh.write(new)
                os.replace(tmp, path)  # atomic
                written += 1
        elif not args.write:
            for line, category, raw_match in scan_text(text, args.aggressive):
                findings.append({"path": path, "line": line, "category": category,
                                 "preview": _preview(category, raw_match)})

    if not args.json:
        print("scrub — PII/secret first-pass (NOT a substitute for gitleaks/Presidio in CI)\n")
    if args.write:
        out = {"redacted_files": written, "scanned": scanned,
               "not_utf8": [{"path": q, "error": why} for q, why in undecodable]}
        print(json.dumps(out) if args.json else
              f"  redacted {written} file(s) of {scanned} scanned\n"
              f"  (history not rewritten — use git-filter-repo/BFG to purge already-committed secrets)")
        _report_undecodable(undecodable, args.json)
        return 1 if undecodable else 0
    # check mode
    if args.json:
        print(json.dumps({"findings": findings, "scanned": scanned,
                          "not_utf8": [{"path": q, "error": why} for q, why in undecodable]}, indent=2))
    else:
        for f in findings:
            print(f"  {f['path']}:{f['line']}  {f['category']:11} {f['preview']} -> [REDACTED:{f['category']}]")
        n = len(findings)
        print(f"\n  {n} finding(s) in {scanned} file(s) · run with --write to redact · CI: use gitleaks + Presidio"
              if n else f"  no findings in {scanned} file(s)")
    _report_undecodable(undecodable, args.json)
    return 1 if (findings or undecodable) else 0


if __name__ == "__main__":
    sys.exit(main())
