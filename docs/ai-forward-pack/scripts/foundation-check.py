#!/usr/bin/env python3
"""foundation-check.py — vendored-foundation drift detection (AI-Forward Pack).

The pack vendors foundation docs from the base Agent Knowledge Pack into knowledge/.
They WILL diverge over time; this makes divergence visible instead of surprising.
Hashes are computed over NORMALIZED content (CRLF->LF, trailing-space strip) so line
endings never masquerade as drift. Stdlib only.

Usage
  foundation-check.py                 # verify knowledge/ matches FOUNDATION.md manifest
  foundation-check.py --base <dir>    # additionally diff vendored vs base-pack copies
  foundation-check.py --update        # rewrite manifest hashes from current knowledge/
Exit: 0 clean, 1 drift/missing.
"""
import argparse, hashlib, os, re, sys

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


HERE = os.path.dirname(os.path.abspath(__file__))
PACK = os.path.dirname(HERE)
MANIFEST = os.path.join(PACK, "knowledge", "FOUNDATION.md")

def nhash(path):
    with open(path, "rb") as f:
        t = f.read().decode("utf-8", "replace")
    t = "\n".join(ln.rstrip() for ln in t.replace("\r\n", "\n").replace("\r", "\n").split("\n"))
    return hashlib.sha256(t.encode("utf-8")).hexdigest()[:16]

def read_manifest():
    rows = []
    with open(MANIFEST, encoding="utf-8") as f:
        for ln in f:
            m = re.match(r"\|\s*`([\w.-]+\.md)`\s*\|[^|]*\|\s*`([0-9a-f]{16})`\s*\|", ln)
            if m: rows.append((m.group(1), m.group(2)))
    if not rows: sys.exit("no manifest rows found in FOUNDATION.md")
    return rows

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default=None, help="path to the base Agent Knowledge Pack")
    ap.add_argument("--update", action="store_true", help="rewrite manifest hashes from current files")
    args = ap.parse_args()
    rows, bad = read_manifest(), 0
    if args.update:
        # newline="" keeps the manifest's own line endings intact: the rewrite is a hash
        # substitution, not a re-encoding, so a CRLF checkout must not come back as LF.
        with open(MANIFEST, encoding="utf-8", newline="") as source:
            text = source.read()
        for f, old in rows:
            text = text.replace(f"`{old}`", f"`{nhash(os.path.join(PACK,'knowledge',f))}`", 1)
        with open(MANIFEST, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(text)
        print(f"manifest updated for {len(rows)} docs"); return 0
    for f, want in rows:
        p = os.path.join(PACK, "knowledge", f)
        if not os.path.exists(p): print(f"MISSING  {f}"); bad += 1; continue
        got = nhash(p)
        if got != want: print(f"DRIFT    {f}: vendored {got} != manifest {want} (uncatalogued edit — update FOUNDATION.md)"); bad += 1
        else: print(f"ok       {f}")
        if args.base:
            bp = os.path.join(args.base, f)
            if not os.path.exists(bp): print(f"  base:  {f} not found in --base"); continue
            bh = nhash(bp)
            print(f"  base:  {'matches vendored' if bh == got else 'DIFFERS from vendored ' + bh + ' (check FOUNDATION.md known-divergence list)'}")
    print("clean" if not bad else f"{bad} finding(s)")
    return 1 if bad else 0

if __name__ == "__main__":
    sys.exit(main())
