#!/usr/bin/env python3
"""Render a human-facing Markdown artifact as a self-contained HTML companion."""

import argparse
import html
import os
import re
import sys
import tempfile
from pathlib import Path
from urllib.parse import urlparse


for stream in (sys.stdout, sys.stderr):
    if hasattr(stream, "reconfigure"):
        stream.reconfigure(encoding="utf-8", errors="replace")


STYLE = """
:root { color-scheme: light dark; --bg:#f7f5ef; --panel:#fff; --text:#20201d;
  --muted:#66645f; --line:#d9d5ca; --accent:#3157a4; --code:#f0eee7; }
@media (prefers-color-scheme: dark) {
  :root { --bg:#171816; --panel:#20221f; --text:#f2f1eb; --muted:#b8b6ae;
    --line:#3d403a; --accent:#8fb0ff; --code:#292c27; }
}
* { box-sizing:border-box; }
body { margin:0; background:var(--bg); color:var(--text);
  font:16px/1.65 system-ui,-apple-system,"Segoe UI",sans-serif; }
main { width:min(920px,calc(100% - 32px)); margin:32px auto; padding:40px;
  background:var(--panel); border:1px solid var(--line); border-radius:12px; }
h1,h2,h3,h4,h5,h6 { line-height:1.2; margin:1.5em 0 .55em; }
h1 { margin-top:0; font-size:2.2rem; } h2 { border-bottom:1px solid var(--line); padding-bottom:.3em; }
a { color:var(--accent); } code { background:var(--code); padding:.12em .35em; border-radius:4px; }
pre { overflow:auto; padding:16px; background:var(--code); border-radius:8px; }
pre code { padding:0; } blockquote { margin:1em 0; padding:.2em 1em; color:var(--muted);
  border-left:4px solid var(--line); }
table { width:100%; border-collapse:collapse; margin:1em 0; display:block; overflow:auto; }
th,td { padding:.55em .75em; border:1px solid var(--line); text-align:left; vertical-align:top; }
th { background:var(--code); } hr { border:0; border-top:1px solid var(--line); }
.source { color:var(--muted); font-size:.9rem; margin-bottom:2rem; }
@media (max-width:640px) { main { width:100%; margin:0; padding:24px 18px; border:0; border-radius:0; } }
"""

SPECIAL = re.compile(r"^(#{1,6})\s+|^```|^>\s?|^[-*+]\s+|^\d+[.)]\s+|^---+$")
TABLE_DIVIDER = re.compile(r"^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*$")
INLINE = re.compile(r"(`[^`\n]+`|\[[^\]\n]+\]\([^)]+\)|\*\*[^*\n]+\*\*|\*[^*\n]+\*)")


def _safe_href(value):
    parsed = urlparse(value)
    if parsed.scheme.lower() in ("", "http", "https", "mailto"):
        return value
    return None


def render_inline(text):
    parts = []
    offset = 0
    for match in INLINE.finditer(text):
        parts.append(html.escape(text[offset:match.start()]))
        token = match.group(0)
        if token.startswith("`"):
            parts.append(f"<code>{html.escape(token[1:-1])}</code>")
        elif token.startswith("["):
            label, href = token[1:-1].split("](", 1)
            safe = _safe_href(href)
            if safe is None:
                parts.append(html.escape(label))
            else:
                parts.append(
                    f'<a href="{html.escape(safe, quote=True)}">{html.escape(label)}</a>'
                )
        elif token.startswith("**"):
            parts.append(f"<strong>{html.escape(token[2:-2])}</strong>")
        else:
            parts.append(f"<em>{html.escape(token[1:-1])}</em>")
        offset = match.end()
    parts.append(html.escape(text[offset:]))
    return "".join(parts)


def _cells(line):
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def _strip_frontmatter(lines):
    if not lines or lines[0].strip() != "---":
        return lines
    for index in range(1, len(lines)):
        if lines[index].strip() == "---":
            return lines[index + 1:]
    return lines


def render_markdown(text):
    lines = _strip_frontmatter(text.replace("\r\n", "\n").replace("\r", "\n").split("\n"))
    output = []
    index = 0
    while index < len(lines):
        line = lines[index]
        stripped = line.strip()
        if not stripped:
            index += 1
            continue
        if stripped.startswith("```"):
            language = stripped[3:].strip()
            code = []
            index += 1
            while index < len(lines) and not lines[index].strip().startswith("```"):
                code.append(lines[index])
                index += 1
            index += 1 if index < len(lines) else 0
            class_name = f' class="language-{html.escape(language, quote=True)}"' if language else ""
            output.append(f"<pre><code{class_name}>{html.escape(chr(10).join(code))}</code></pre>")
            continue
        heading = re.match(r"^(#{1,6})\s+(.*)$", stripped)
        if heading:
            level = len(heading.group(1))
            output.append(f"<h{level}>{render_inline(heading.group(2))}</h{level}>")
            index += 1
            continue
        if index + 1 < len(lines) and "|" in line and TABLE_DIVIDER.match(lines[index + 1]):
            headers = _cells(line)
            index += 2
            rows = []
            while index < len(lines) and "|" in lines[index] and lines[index].strip():
                rows.append(_cells(lines[index]))
                index += 1
            head = "".join(f"<th>{render_inline(cell)}</th>" for cell in headers)
            body = "".join(
                "<tr>" + "".join(f"<td>{render_inline(cell)}</td>" for cell in row) + "</tr>"
                for row in rows
            )
            output.append(f"<table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table>")
            continue
        unordered = re.match(r"^[-*+]\s+(.*)$", stripped)
        ordered = re.match(r"^\d+[.)]\s+(.*)$", stripped)
        if unordered or ordered:
            tag = "ul" if unordered else "ol"
            items = []
            pattern = r"^[-*+]\s+(.*)$" if unordered else r"^\d+[.)]\s+(.*)$"
            while index < len(lines):
                item = re.match(pattern, lines[index].strip())
                if not item:
                    break
                items.append(f"<li>{render_inline(item.group(1))}</li>")
                index += 1
            output.append(f"<{tag}>{''.join(items)}</{tag}>")
            continue
        if stripped.startswith(">"):
            output.append(f"<blockquote>{render_inline(stripped.lstrip('>').strip())}</blockquote>")
            index += 1
            continue
        if re.fullmatch(r"---+", stripped):
            output.append("<hr>")
            index += 1
            continue
        paragraph = [stripped]
        index += 1
        while index < len(lines):
            candidate = lines[index].strip()
            if not candidate or SPECIAL.match(candidate):
                break
            if index + 1 < len(lines) and "|" in lines[index] and TABLE_DIVIDER.match(lines[index + 1]):
                break
            paragraph.append(candidate)
            index += 1
        output.append(f"<p>{render_inline(' '.join(paragraph))}</p>")
    return "\n".join(output)


def render_document(source, title=None):
    text = source.read_text(encoding="utf-8")
    document_title = title or source.stem.replace("-", " ").replace("_", " ").title()
    body = render_markdown(text)
    return (
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">"
        "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">"
        f"<title>{html.escape(document_title)}</title><style>{STYLE}</style></head>"
        f"<body><main><div class=\"source\">Rendered from {html.escape(source.name)}</div>"
        f"{body}</main></body></html>\n"
    )


def write_atomic(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    handle, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=str(path.parent))
    try:
        with os.fdopen(handle, "w", encoding="utf-8", newline="\n") as stream:
            stream.write(content)
        os.replace(temporary, path)
    except Exception:
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--title")
    args = parser.parse_args(argv)
    if args.source.suffix.lower() != ".md":
        parser.error("source must be a Markdown (.md) file")
    output = args.output or args.source.with_suffix(".html")
    try:
        write_atomic(output, render_document(args.source, args.title))
    except (OSError, UnicodeError) as exc:
        print(f"RENDER-MARKDOWN.ERROR: {exc}", file=sys.stderr)
        return 1
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
