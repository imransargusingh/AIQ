#!/usr/bin/env python3
"""design-lint.py — token-reference linter for design-language docs (AI-Forward).

Makes UI Standard U3 ("reference a token, never an arbitrary value") a *forcing
function* for a DESIGN.md-style design-language doc (templates/design-language.template.md).
Stdlib only; lives in the script bundle (deployed to docs/ai-forward-pack/scripts/).

Checks:
  1. FAIL  — every `{group.token}` reference in the body resolves to a token declared
            in the frontmatter (group in: colors, typography, rounded, spacing,
            elevation, motion). An unresolved reference is a broken token contract.
  2. FAIL  — the frontmatter declares at least a `colors:` and a `typography:` block
            (a design language without them is not one).
  3. WARN  — raw hex (`#rrggbb`) found in the body. Hex in the palette *table* is fine
            (that's documentation); hex in a component/layout spec should be a `{token}`.
            Non-failing — surfaced for human review.

Exit 0 clean (warnings allowed), 1 on any FAIL. Usage: design-lint.py <file.md> [...]
"""
import argparse, re, sys

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


GROUPS = ("colors", "typography", "rounded", "spacing", "elevation", "motion")
REF_RE = re.compile(r"\{(" + "|".join(GROUPS) + r")\.([\w-]+)\}")
HEX_RE = re.compile(r"#[0-9a-fA-F]{3}(?:[0-9a-fA-F]{3})?\b")


def split_frontmatter(text):
    """Return (frontmatter_lines, body_text). Frontmatter is the first --- ... --- block."""
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return [], text
    for i in range(1, len(lines)):
        if lines[i].strip() == "---":
            return lines[1:i], "\n".join(lines[i + 1:])
    return [], text  # unterminated frontmatter -> treat as no frontmatter


def parse_tokens(fm_lines):
    """Map each token-group to the set of token names declared under it.

    Handles both block style (`colors:` then indented `  primary: ...`) and
    inline-flow style (`rounded: { sm: 6px, md: 8px }` / `spacing: { scale: [...] }`).
    """
    tokens = {g: set() for g in GROUPS}
    current = None
    for ln in fm_lines:
        if not ln.strip() or ln.lstrip().startswith("#"):
            continue
        indent = len(ln) - len(ln.lstrip())
        m = re.match(r"^([\w-]+):\s*(.*)$", ln.strip())
        if indent == 0:
            current = None
            if m and m.group(1) in GROUPS:
                rest = m.group(2)
                if rest.startswith("{"):  # inline-flow mapping on the same line
                    for k in re.findall(r"([\w-]+)\s*:", rest):
                        tokens[m.group(1)].add(k)
                else:  # block mapping (value empty, a comment, or |/>); children follow
                    current = m.group(1)
            continue
        # a child line under a block group; the shallowest child key is a token name
        if current and indent <= 4 and m:
            tokens[current].add(m.group(1))
    return tokens


def lint(path):
    fails, warns = [], []
    try:
        with open(path, encoding="utf-8") as f:
            text = f.read()
    except OSError as e:
        return [f"{path}: cannot read ({e})"], []
    fm, body = split_frontmatter(text)
    tokens = parse_tokens(fm)

    if not tokens["colors"]:
        fails.append(f"{path}: frontmatter declares no `colors:` tokens")
    if not tokens["typography"]:
        fails.append(f"{path}: frontmatter declares no `typography:` tokens")

    for i, line in enumerate(body.splitlines(), 1):
        for g, tok in REF_RE.findall(line):
            if tok not in tokens[g]:
                fails.append(f"{path}:{i}: unresolved token reference {{{g}.{tok}}} "
                             f"(no `{tok}` under `{g}:` in frontmatter)")
        for hexv in HEX_RE.findall(line):
            warns.append(f"{path}:{i}: raw hex {hexv} in body — use a {{colors.*}} token "
                         f"if this is a component/layout value (palette-table hex is fine)")
    return fails, warns


def main():
    ap = argparse.ArgumentParser(description="Lint design-language token references (U3).")
    ap.add_argument("files", nargs="+", help="design-language markdown file(s)")
    ap.add_argument("--strict", action="store_true", help="treat warnings as failures too")
    args = ap.parse_args()

    all_fail, all_warn = [], []
    for f in args.files:
        fails, warns = lint(f)
        all_fail += fails
        all_warn += warns

    for w in all_warn:
        print(f"  warn: {w}")
    for f in all_fail:
        print(f"  FAIL: {f}")

    bad = bool(all_fail) or (args.strict and bool(all_warn))
    if bad:
        print(f"\n{len(all_fail)} failure(s), {len(all_warn)} warning(s).")
        return 1
    print(f"clean - all token references resolve ({len(all_warn)} warning(s)).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
