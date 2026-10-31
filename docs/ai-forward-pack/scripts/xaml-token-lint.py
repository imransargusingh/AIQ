#!/usr/bin/env python3
"""xaml-token-lint.py — first-slice token linter for XAML/native UI markup.

Checks the deterministic subset of the native-client UI design:
- raw colors in visual properties,
- inline SolidColorBrush colors,
- raw dimensions in common layout/type properties.

It is intentionally not a XAML compiler. It uses bounded text scanning, never fetches
external resources, and refuses paths outside the declared root.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass


COLOR_RE = re.compile(r"#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b")
ATTR_RE = re.compile(r"(?P<name>[A-Za-z_:][\w:.-]*)\s*=\s*(?P<quote>[\"'])(?P<value>.*?)(?P=quote)")
NAMED_COLOR_RE = re.compile(
    r"^(AliceBlue|AntiqueWhite|Aqua|Aquamarine|Azure|Beige|Bisque|Black|BlanchedAlmond|Blue|BlueViolet|Brown|BurlyWood|CadetBlue|Chartreuse|Chocolate|Coral|CornflowerBlue|Cornsilk|Crimson|Cyan|DarkBlue|DarkCyan|DarkGoldenrod|DarkGray|DarkGreen|DarkKhaki|DarkMagenta|DarkOliveGreen|DarkOrange|DarkOrchid|DarkRed|DarkSalmon|DarkSeaGreen|DarkSlateBlue|DarkSlateGray|DarkTurquoise|DarkViolet|DeepPink|DeepSkyBlue|DimGray|DodgerBlue|Firebrick|FloralWhite|ForestGreen|Fuchsia|Gainsboro|GhostWhite|Gold|Goldenrod|Gray|Green|GreenYellow|Honeydew|HotPink|IndianRed|Indigo|Ivory|Khaki|Lavender|LavenderBlush|LawnGreen|LemonChiffon|LightBlue|LightCoral|LightCyan|LightGoldenrodYellow|LightGray|LightGreen|LightPink|LightSalmon|LightSeaGreen|LightSkyBlue|LightSlateGray|LightSteelBlue|LightYellow|Lime|LimeGreen|Linen|Magenta|Maroon|MediumAquamarine|MediumBlue|MediumOrchid|MediumPurple|MediumSeaGreen|MediumSlateBlue|MediumSpringGreen|MediumTurquoise|MediumVioletRed|MidnightBlue|MintCream|MistyRose|Moccasin|NavajoWhite|Navy|OldLace|Olive|OliveDrab|Orange|OrangeRed|Orchid|PaleGoldenrod|PaleGreen|PaleTurquoise|PaleVioletRed|PapayaWhip|PeachPuff|Peru|Pink|Plum|PowderBlue|Purple|Red|RosyBrown|RoyalBlue|SaddleBrown|Salmon|SandyBrown|SeaGreen|SeaShell|Sienna|Silver|SkyBlue|SlateBlue|SlateGray|Snow|SpringGreen|SteelBlue|Tan|Teal|Thistle|Tomato|Transparent|Turquoise|Violet|Wheat|White|WhiteSmoke|Yellow|YellowGreen)$",
    re.IGNORECASE,
)
VISUAL_COLOR_ATTRS = {
    "Background",
    "Foreground",
    "BorderBrush",
    "Fill",
    "Stroke",
    "Color",
}
DIMENSION_ATTRS = {
    "Margin",
    "Padding",
    "CornerRadius",
    "Width",
    "Height",
    "MinWidth",
    "MinHeight",
    "MaxWidth",
    "MaxHeight",
    "FontSize",
}
RESOURCE_MARKERS = ("{StaticResource", "{DynamicResource", "{ThemeResource", "{Binding")
DIMENSION_LITERAL_RE = re.compile(r"^\s*-?\d+(?:\.\d+)?(?:\s*,\s*-?\d+(?:\.\d+)?){0,3}\s*$")
TEXT_EXTENSIONS = {".xaml", ".axaml"}


def is_resource(value: str) -> bool:
    return any(marker in value for marker in RESOURCE_MARKERS)


def is_allowed_dimension(value: str) -> bool:
    stripped = value.strip()
    return (
        not stripped
        or stripped.lower() in {"auto", "nan"}
        or "*" in stripped
        or is_resource(stripped)
    )


def finding(path: Path, line: int, rule: str, severity: str, message: str, attribute: str) -> dict:
    return {
        "file": str(path),
        "line": line,
        "rule": rule,
        "severity": severity,
        "message": message,
        "attribute": attribute,
    }


def lint_text(path: Path, text: str) -> list[dict]:
    findings: list[dict] = []
    for line_no, line in enumerate(text.splitlines(), 1):
        for match in ATTR_RE.finditer(line):
            name = match.group("name").split(":")[-1]
            value = match.group("value")
            if is_resource(value):
                continue
            if name in VISUAL_COLOR_ATTRS and (COLOR_RE.search(value) or NAMED_COLOR_RE.match(value)):
                findings.append(
                    finding(
                        path,
                        line_no,
                        "xaml-raw-color",
                        "major",
                        f"{name} uses a raw color; reference a XAML resource token.",
                        name,
                    )
                )
            if name == "Color" and NAMED_COLOR_RE.match(value):
                findings.append(
                    finding(
                        path,
                        line_no,
                        "xaml-raw-brush",
                        "major",
                        "SolidColorBrush/Color uses a named raw color; reference a XAML resource token.",
                        name,
                    )
                )
            if name in DIMENSION_ATTRS and not is_allowed_dimension(value) and DIMENSION_LITERAL_RE.match(value):
                findings.append(
                    finding(
                        path,
                        line_no,
                        "xaml-raw-dimension",
                        "minor" if name in {"Width", "Height", "FontSize"} else "major",
                        f"{name} uses a raw dimension; reference a spacing/type/radius resource token.",
                        name,
                    )
                )
    return findings


def resolve_under_root(root: Path, path: Path) -> Path:
    root_resolved = root.resolve()
    candidate = (root_resolved / path).resolve() if not path.is_absolute() else path.resolve()
    try:
        candidate.relative_to(root_resolved)
    except ValueError as exc:
        raise ValueError(f"{candidate} is outside root {root_resolved}") from exc
    return candidate


def is_under_root(root_resolved: Path, candidate: Path) -> bool:
    try:
        candidate.resolve().relative_to(root_resolved)
        return True
    except (OSError, ValueError):
        return False


def expand_paths(root: Path, inputs: list[str]) -> list[Path]:
    candidates: list[Path] = []
    root_resolved = root.resolve()
    for item in inputs:
        resolved = resolve_under_root(root, Path(item))
        if resolved.is_dir():
            for extension in TEXT_EXTENSIONS:
                for child in sorted(resolved.rglob(f"*{extension}")):
                    if is_under_root(root_resolved, child):
                        candidates.append(child)
        else:
            candidates.append(resolved)
    return candidates


def lint_file(path: Path) -> list[dict]:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        raise OSError(f"cannot read {path}: {exc}") from exc
    return lint_text(path, text)


def print_text(findings: list[dict]) -> None:
    if not findings:
        print("clean - no XAML token findings")
        return
    for f in findings:
        print(f"{f['file']}:{f['line']}: {f['severity']} {f['rule']}: {f['message']}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Lint XAML/Axaml native UI token usage.")
    parser.add_argument("paths", nargs="+", help="XAML/Axaml files or directories to scan")
    parser.add_argument("--root", default=".", help="repo root; scanned paths must stay inside it")
    parser.add_argument("--format", choices=("text", "json"), default="text")
    args = parser.parse_args(argv)

    try:
        root = Path(args.root).resolve()
        files = expand_paths(root, args.paths)
        findings: list[dict] = []
        for path in files:
            if path.suffix.lower() not in TEXT_EXTENSIONS:
                continue
            findings.extend(lint_file(path))
    except (OSError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.format == "json":
        print(json.dumps(findings, indent=2))
    else:
        print_text(findings)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
