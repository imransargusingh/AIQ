"""Merge source-owned hook bundles while preserving project-owned bundle names."""
import argparse
import os
from pathlib import Path
import runpy
import tempfile


# Keep the bootstrap installer standalone: the shared pure merge lives in that script.
_merges = runpy.run_path(str(Path(__file__).with_name("pack-apply.py")))
merge_named_hook_bundles = _merges["merge_named_hook_bundles"]
merge_claude_settings = _merges["merge_claude_settings"]
merge_hook_ownership = _merges["merge_hook_ownership"]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("target", type=Path)
    parser.add_argument("--claude-settings", action="store_true")
    parser.add_argument("--ownership-only", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.target.is_symlink():
            raise ValueError("Refuse a symlink hook target")
        old = args.target.read_text(encoding="utf-8") if args.target.exists() else None
        if args.ownership_only:
            if old is None:
                return 0
            merged = merge_hook_ownership(old)
        else:
            merge = merge_claude_settings if args.claude_settings else merge_named_hook_bundles
            merged = merge(args.source.read_text(encoding="utf-8"), old)
        if old == merged:
            return 0
        args.target.parent.mkdir(parents=True, exist_ok=True)
        name = None
        try:
            with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="\n",
                    dir=args.target.parent, delete=False) as handle:
                name = handle.name
                handle.write(merged)
            os.chmod(name, args.target.stat().st_mode & 0o777 if args.target.exists() else 0o644)
            os.replace(name, args.target)
        finally:
            if name and os.path.exists(name):
                os.unlink(name)
        return 0
    except (OSError, ValueError) as exc:
        parser.exit(1, "HOOK-BUNDLE-NOT-CHECKED: " + type(exc).__name__ + "; existing target retained\n")


if __name__ == "__main__":
    raise SystemExit(main())
