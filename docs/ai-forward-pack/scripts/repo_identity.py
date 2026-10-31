#!/usr/bin/env python3
"""repo_identity.py - the canonical project name, in ONE place (class PACK-P).

WHY THIS EXISTS. The pack's own worktree discipline (WT1) sends a writing session with a
concurrent-writer risk into its own linked worktree, and `coord worktree new` names that directory
`<repo>-<branch-slug>`. Any generator that names the project `basename(cwd)` therefore stamps
the WORKTREE folder into a committed artifact the moment the discipline is followed:
`docs/audit/audit-data.js` ("project"), `docs/audit/index.html` (<title>), the Docs Explorer
surface titles. Following one rule guaranteed corrupting the other -- a tool that infers
identity from the filesystem, inside a system whose own discipline moves work around the
filesystem. Observed in this repo (a commit stamped `ai-forward-feature-audit-signals-writer`)
and repeatedly in a consuming repo, always caught by eye or by a bundle gate, never by a test.

RESOLUTION ORDER, and why each rung is where it is:
  1. an explicit name the caller was given (`--project`)  -- configuration beats inference,
     always, and this is the rung that makes the others a fallback rather than a guess;
  2. `remote.origin.url` -- explicit git configuration, shared by every worktree of the repo,
     and the only rung that is stable across a clone whose directory was renamed;
  3. the PRIMARY checkout's directory name, via `git rev-parse --git-common-dir` -- one call
     that resolves to the primary repo's `.git` even from a linked worktree;
  4. the directory name -- last resort, for a tree that is not a git repository at all.

VERIFIED, not assumed (Windows, git 2.x, 2026-09-10):
  * from a linked worktree, `--git-common-dir` -> `C:/Projects/ai-forward/.git` (absolute);
  * from the PRIMARY checkout it returns the RELATIVE string `.git`, whose dirname is ''.
    So rung 3 MUST resolve it against the repo path before taking a basename; a naive
    `dirname(common_dir)` yields an empty name in the most common case of all.
  * outside a git repository, `git rev-parse` exits 128 with empty stdout, and
    `git config --get remote.origin.url` exits 1 -- so both rungs must tolerate failure
    rather than raise.

Stdlib only, Python 3.8+. The underscore in the filename is deliberate: a hyphen is not
importable, which is why `coord_ids.py` and `bounded_process.py` are named the way they are.
"""
import os
import subprocess

__all__ = ["canonical_project"]


def _git(root, *args):
    """Run git in `root`; return stripped stdout, or "" for any failure at all.

    Outside a repo git exits non-zero with empty stdout, so a caller can never tell a real
    empty answer from a failed one -- and must not need to: every caller here treats "" as
    'this rung had no answer' and falls through to the next.
    """
    try:
        done = subprocess.run(["git"] + list(args), cwd=root, capture_output=True)
    except (OSError, ValueError):
        return ""
    if done.returncode != 0:
        return ""
    try:
        return done.stdout.decode("utf-8", "replace").strip()
    except AttributeError:
        return ""


def _from_remote(root):
    url = _git(root, "config", "--get", "remote.origin.url").rstrip("/")
    if not url:
        return ""
    name = url.replace("\\", "/").rsplit("/", 1)[-1]
    if name.endswith(".git"):
        name = name[:-4]
    return name


def _from_primary_checkout(root):
    common = _git(root, "rev-parse", "--git-common-dir")
    if not common:
        return ""
    # The relative form ('.git' from the primary checkout) is the common case -- resolve it
    # against `root` before taking a basename, or the answer is the empty string.
    primary = os.path.dirname(os.path.abspath(os.path.join(root, common)))
    return os.path.basename(primary)


def canonical_project(root, explicit=None):
    """The project's canonical name -- never the worktree folder it happened to run in.

    `root` is any path inside the repository (a worktree root, or a directory under it).
    `explicit` is a caller-supplied override (`--project`) and always wins when non-empty.
    Returns a non-empty string; falls back to "repo" only when there is no name to be had.
    """
    if explicit:
        return str(explicit)
    root = os.path.abspath(root)
    for rung in (_from_remote, _from_primary_checkout):
        name = rung(root)
        if name:
            return name
    return os.path.basename(root.replace("\\", "/").rstrip("/")) or "repo"
