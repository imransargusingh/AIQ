#!/usr/bin/env python3
"""Conditional delivery routing and local checkpoint integrity, not a workflow runner."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import secrets
import stat
import subprocess
import sys
import tempfile
import time
from typing import Any, cast

# PLAT-A: direct routes/refusals must not depend on compiler imports to configure stdio.
for _stream in (sys.stdout, sys.stderr):
    _reconfigure = getattr(_stream, "reconfigure", None)
    if callable(_reconfigure):
        try:
            cast(Any, _stream).reconfigure(encoding="utf-8", errors="replace")
        except (ValueError, OSError):
            pass


def route(facts):
    required = {"kind", "tier", "questions", "risks", "ui", "coordination"}
    risks = {"security", "identity", "data", "contracts", "money", "concurrency"}
    if (not isinstance(facts, dict) or not required <= set(facts) or set(facts) - required - {"design_ready"}
            or facts["kind"] not in ("feature", "defect", "migration", "docs")
            or facts["tier"] not in ("T0", "T1", "T2")
            or type(facts.get("design_ready", False)) is not bool
            or type(facts["ui"]) is not bool or type(facts["coordination"]) is not bool
            or not isinstance(facts["questions"], list) or not isinstance(facts["risks"], list)
            or any(q not in ("requirements", "architecture", "design") for q in facts["questions"])
            or any(not isinstance(r, str) or r not in risks for r in facts["risks"])):
        raise ValueError("invalid facts: use the documented closed routing schema")
    tier = "T2" if (facts["risks"] or facts["kind"] == "migration" or facts["coordination"]
                    or "architecture" in facts["questions"]) else facts["tier"]
    questions = list(facts["questions"])
    if (facts["risks"] or tier != "T0") and not facts.get("design_ready", False) and "design" not in questions:
        questions.append("design")
    stages = []
    for question, skill in (("requirements", "specify"), ("architecture", "define-architecture"),
                            ("design", "design-slice")):
        if question in questions:
            stages.append(skill)
    if facts["ui"]:
        stages.append("ui-design")
    if facts["kind"] == "defect":
        stages = ["investigate", "repair-review"] + stages
    implementation = {"feature": "implement", "defect": "implement", "migration": "migrate", "docs": "document"}
    if facts["coordination"]:
        if facts["kind"] == "migration":
            stages.append("migration-characterization")
        stages.extend(["prepare-for-coordination", "execute-with-coordination"])
    else:
        stages.append(implementation[facts["kind"]])
    stages.append("verify")
    return {"stages": stages, "tier": tier}


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False).encode("utf-8")).hexdigest()


def git(repo, *args, optional=False):
    # git -C does not isolate inherited repository-local overrides (as from
    # hooks or another worktree). Keep credentials/global configuration intact.
    env = os.environ.copy()
    for key in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR", "GIT_NAMESPACE",
                "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_OBJECT_DIRECTORY", "GIT_IMPLICIT_WORK_TREE",
                "GIT_GRAFT_FILE", "GIT_NO_REPLACE_OBJECTS", "GIT_REPLACE_REF_BASE", "GIT_PREFIX",
                "GIT_SHALLOW_FILE", "GIT_CONFIG", "GIT_CONFIG_PARAMETERS", "GIT_CONFIG_COUNT"):
        env.pop(key, None)
    result = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, timeout=30, env=env)
    if result.returncode and not optional:
        raise ValueError("repository required: delivery needs a Git worktree")
    text = result.stdout.decode("utf-8") if not result.returncode else ""
    return text if "-z" in args else text.strip()


def identity(repo):
    repo = Path(repo).resolve()
    if not repo.is_dir():
        raise ValueError("project refused: --repo must name an existing directory")
    top = git(repo, "rev-parse", "--show-toplevel", optional=True)
    if not top:
        return {"root": str(repo), "kind": "plain"}
    root = Path(top).resolve()
    def absolute(value):
        return str((root / value).resolve())
    return {"root": str(root), "git_dir": absolute(git(root, "rev-parse", "--git-dir")),
            "common_dir": absolute(git(root, "rev-parse", "--git-common-dir"))}


# Exact ephemeral duration stores, not the durable audit/coordination log directories.
# Payload cwd may be a descendant; tracked Git markers are ephemeral too.
# A caller needing these runtime bytes as task inputs registers them with --input.
RUNTIME_START_MARKERS = {
    "docs/audit/.run-starts.json", "docs/audit/.run-starts.json.tmp",
    ".agents/log/audit/.run-starts.json", ".agents/log/audit/.run-starts.json.tmp",
}


def runtime_start_marker_name(path, root):
    """Match a marker suffix lexically, without granting a runtime exemption."""
    relative = path.relative_to(root)
    return any(relative.parts[-len(Path(name).parts):] == Path(name).parts for name in RUNTIME_START_MARKERS)


def is_link(path):
    """A symlink, or a Windows reparse point such as a junction, which `is_symlink()` does not report and which an
    unprivileged process can create. Every link check here uses this one rule."""
    try:
        info = Path(path).lstat()
    except OSError:
        return False
    return stat.S_ISLNK(info.st_mode) or bool(getattr(info, "st_file_attributes", 0)
                                              & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0))


def _link_ancestor(repo, name):
    """The nearest folder of `name` (relative to `repo`, in `name`'s own separators) that is a link, or None."""
    sep = "\\" if "\\" in name else "/"
    parts = name.split(sep)
    current = Path(repo)
    for depth, part in enumerate(parts[:-1], start=1):
        current = current / part
        if is_link(current):
            return sep.join(parts[:depth])
    return None


def runtime_start_marker(path, root, allow_missing=False):
    """Exact regular stores; snapshots may also omit genuinely absent stores."""
    if not runtime_start_marker_name(path, root):
        return False
    relative = path.relative_to(root)
    current = root
    for part in relative.parts:
        current = current / part
        try:
            info = current.lstat()
        except FileNotFoundError:
            # Git cached paths survive os.replace consuming a tracked .tmp.
            # Never treat an alias or a marker-directory descendant as absent
            # runtime bookkeeping. Explicit inputs use independent file records.
            return (allow_missing and path.resolve() == path.absolute()
                    and not any(runtime_start_marker_name(parent, root) for parent in path.parents
                                if parent != root and parent.is_relative_to(root)))
        if is_link(current):
            return False
        if current == path:
            return stat.S_ISREG(info.st_mode) and path.resolve().is_relative_to(root)
        if not stat.S_ISDIR(info.st_mode) or runtime_start_marker_name(current, root):
            # A marker-shaped directory's descendants are durable, even when a
            # later leaf happens to repeat a duration-store suffix.
            return False
    return False


def snapshot(repo, local_area=None, details=False, _depth=0):
    # Ignored/runtime inputs require explicit --input registration.
    project = identity(repo)
    repo = Path(project["root"])
    if project.get("kind") == "plain":
        names = []
        for directory, folders, files in os.walk(repo, followlinks=False):
            names.extend(str(Path(directory, name).relative_to(repo))
                         for name in folders if is_link(Path(directory, name))
                         or runtime_start_marker_name(Path(directory, name), repo))
            folders[:] = [name for name in folders
                          if name not in {".git", "node_modules", "__pycache__", ".venv", "venv"}
                          and not (local_area and Path(directory, name).is_relative_to(Path(local_area)))
                          and not is_link(Path(directory, name))]
            names.extend(str(Path(directory, name).relative_to(repo)) for name in files)
    else:
        names = git(repo, "ls-files", "-z", "--cached", "--others", "--exclude-standard").split("\0")
        # Git marker ignores also hide directories' durable descendants. Recover
        # only these exact suffixes and their children, not all ignored artifacts.
        exact_patterns = [":(glob)**/" + name for name in sorted(RUNTIME_START_MARKERS)]
        patterns = exact_patterns + [pattern + "/**" for pattern in exact_patterns]
        ignored = git(repo, "ls-files", "-z", "--others", "--ignored", "--exclude-standard",
                      "--", *patterns).split("\0")
        names.extend(name for name in ignored if name and not runtime_start_marker(repo / name, repo))
        # Git omits FIFOs/sockets and can miss explicitly unignored empty
        # directories. Discover exact stores independently of Git/ignore policy;
        # never add ordinary ignored regular files or traverse filesystem links.
        for directory, folders, _ in os.walk(repo, followlinks=False):
            folders[:] = [name for name in folders
                          if name not in {".git", "node_modules", "__pycache__", ".venv", "venv"}
                          and not (local_area and Path(directory, name).is_relative_to(Path(local_area)))
                          and not is_link(Path(directory, name))]
            relative = Path(directory).relative_to(repo).parts
            if relative[-2:] != ("docs", "audit") and relative[-3:] != (".agents", "log", "audit"):
                continue
            for marker in (".run-starts.json", ".run-starts.json.tmp"):
                path = Path(directory, marker)
                try:
                    info = path.lstat()
                except FileNotFoundError:
                    continue
                if (not stat.S_ISREG(info.st_mode) or getattr(info, "st_file_attributes", 0)
                        & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0)):
                    names.append(str(path.relative_to(repo)))
    gitlinks = {}
    indexed = {}
    if project.get("kind") != "plain":
        for entry in filter(None, git(repo, "ls-files", "--stage", "-z").split("\0")):
            metadata, name = entry.split("\t", 1)
            indexed.setdefault(name, []).append(metadata)
            if metadata.startswith("160000 "):
                gitlinks.setdefault(name, []).append(metadata)
    # Git lists a symlink itself but walks into a Windows junction and lists the files behind it: record the link
    # (its target text), never content read through it from outside the project.
    names = [_link_ancestor(repo, name) or name for name in names if name]
    files = {}
    for name in sorted(set(names)):
        path = Path(repo) / name
        if local_area and path.is_relative_to(Path(local_area)):
            continue
        if runtime_start_marker(path, repo, allow_missing=True):
            continue
        if is_link(path):
            files[name] = {"link": os.readlink(path)}
        elif runtime_start_marker_name(path, repo) and path.is_dir():
            files[name] = {"directory": True}
        elif runtime_start_marker_name(path, repo) and path.exists() and not path.is_file():
            # Nonregular stores are durable type sentinels, never content reads
            # (opening a FIFO could block; a socket has no file byte stream).
            files[name] = {"special": stat.S_IFMT(path.lstat().st_mode)}
        elif name in gitlinks:
            # An uninitialized checkout must not accidentally recurse into its
            # parent repository. Never follow directory symlinks to submodules.
            value: dict[str, Any] = {"gitlink": gitlinks[name], "checkout": "unavailable"}
            if path.resolve() != path.absolute():
                raise ValueError("input refused: submodule path traverses a filesystem symlink")
            if (path / ".git").exists():
                if _depth >= 16:
                    raise ValueError("input refused: submodule nesting exceeds the 16-level snapshot bound")
                if Path(git(path, "rev-parse", "--show-toplevel")).resolve() != path.resolve():
                    raise ValueError("input refused: submodule checkout does not identify its own Git worktree")
                value["checkout"] = snapshot(path, local_area, _depth=_depth + 1)
            files[name] = value
        else:
            files[name] = ({"sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                            "executable": path.stat().st_mode & 0o111 if os.name != "nt" else 0}
                           if path.is_file() else None)
    result: dict[str, Any] = {"head": git(repo, "rev-parse", "HEAD", optional=True) if project.get("kind") != "plain" else "",
            "branch": git(repo, "symbolic-ref", "HEAD", optional=True) if project.get("kind") != "plain" else "",
            "files": digest(files)}
    if gitlinks:
        result["submodules"] = {name: files[name] for name in gitlinks if name in files}
    if _depth:
        result["index"] = digest(git(repo, "ls-files", "--stage", "-z"))
    if details:
        result["entries"] = files
        result["index_entries"] = indexed
    return result


def file_record(path):
    return capture_file(path)[1]


def capture_file(path):
    """Interpretation and evidence digest must originate in one byte capture."""
    path = Path(path).absolute()
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        raise ValueError("evidence missing: supply a nonempty regular file")
    content = path.read_bytes()
    if not content.strip():
        raise ValueError("evidence missing: supply a nonempty regular file")
    return content, {"path": str(path), "sha256": hashlib.sha256(content).hexdigest()}


def read_json_record(path):
    content, record = capture_file(path)
    return json.loads(content.decode("utf-8")), record


def validate_scoped_product_path(path, root):
    """Refuse aliases and special files; a missing leaf remains a valid create/delete scope."""
    path = Path(path)
    root = Path(root).resolve()
    if not path.absolute().is_relative_to(root) or path.resolve() != path.absolute():
        raise ValueError("repair refused: scope must remain inside the project without filesystem aliases")
    relative = path.absolute().relative_to(root)
    current = root
    for part in relative.parts:
        current = current / part
        try:
            info = current.lstat()
        except FileNotFoundError:
            return
        reparse = (getattr(info, "st_file_attributes", 0)
                   & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0))
        if reparse or stat.S_ISLNK(info.st_mode):
            raise ValueError("repair refused: scoped product paths cannot traverse links or reparse points")
        if current != path:
            if not stat.S_ISDIR(info.st_mode):
                raise ValueError("repair refused: scoped product path ancestor is not a directory")
        elif not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
            raise ValueError("repair refused: existing scoped product paths must be single-link regular files")


def check_records(records, label):
    for record in records:
        if (not isinstance(record, dict) or set(record) != {"path", "sha256"}
                or not isinstance(record["path"], str) or not record["path"]
                or not Path(record["path"]).is_absolute()
                or not isinstance(record["sha256"], str) or not re.fullmatch(r"[a-f0-9]{64}", record["sha256"])):
            raise ValueError("checkpoint refused: malformed file evidence record")
        try:
            current = file_record(record["path"])
        except (ValueError, OSError):
            raise ValueError(label + " drift: recorded file is missing") from None
        if current != record:
            raise ValueError(label + " drift: recorded file changed")


def compiler():
    spec = importlib.util.spec_from_file_location("delivery_compile", Path(__file__).with_name("prompt-compile.py"))
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def compilation(audit_root, compiled_id):
    engine = compiler()
    entry = engine.find_entry(str(audit_root), compiled_id, "compilation")
    if not entry or not isinstance(entry.get("compiled"), dict):
        raise ValueError("contract refused: use a recorded compilation id with a compiled document")
    doc = entry["compiled"]
    raw = engine.find_entry(str(audit_root), doc.get("raw_id"), "prompt")
    if not raw or engine.check_schema(doc) or engine._gate().verify_document(doc, raw["prompt"]):
        raise ValueError("contract refused: compiler provenance gate failed")
    return {"entry": entry, "raw_entry": raw}


def contract(audit_root, compiled_id):
    accepted = compilation(audit_root, compiled_id)
    if accepted["entry"].get("dispatchable") is not True:
        raise ValueError("contract refused: use a finished dispatchable compilation id")
    doc = accepted["entry"]["compiled"]
    if (doc.get("mode") == "not-compiled"
            or any(not isinstance(request, dict) or not isinstance(request.get("answer"), str)
                   or not request["answer"].strip() or request["answer"].strip().lower() == "unanswered"
                   for request in doc.get("decision_requests", []))):
        raise ValueError("contract refused: unresolved compiler decisions")
    return accepted


def local_area(repo, state_root=None):
    project = identity(repo)
    if state_root is not None:
        requested = Path(state_root)
        area = requested.resolve()
        if is_link(requested) or Path(project["root"]).is_relative_to(area):
            raise ValueError("state path refused: choose a dedicated nonsymlink local area, not the project or its ancestor")
        return area
    if "git_dir" in project:
        return Path(project["git_dir"]) / "ai-forward/delivery"
    home = Path(os.environ.get("XDG_STATE_HOME") or Path.home() / ".local/state")
    return home / "ai-forward/delivery"


def state_path(repo, task, state_root=None):
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", task):
        raise ValueError("invalid task: use a lowercase slug, at most 64 characters")
    project = identity(repo)
    area = local_area(repo, state_root)
    if state_root is not None or project.get("kind") == "plain":
        area = area / digest(project)
    return area / (task + ".json")


def save(path, state, reviewed_snapshot=None):
    if not isinstance(state, dict) or state.get("version") != 2:
        raise ValueError("checkpoint refused: corrupt or unsupported checkpoint; do not infer progress")
    if state.get("phase") == "prestart":
        candidate = {**state, "integrity": digest({k: v for k, v in state.items() if k != "integrity"})}
        check_prestart(candidate, state["identity"]["root"], state["task"], candidate["recovery"]["state_root"])
    else:
        validate_progress(state)
        check_gate_binding(state)
    # Recheck captured inputs immediately before persistence. Refusal leaves the
    # prior checkpoint intact; this is observation, not filesystem isolation.
    check_records(state.get("inputs", []), "input")
    for row in state.get("completed", []) + state.get("partial", []) + state.get("decisions", []) + state.get("repairs", []):
        check_records(row["evidence"], "evidence")
    if state.get("gate"):
        check_records(state["gate"]["evidence"], "evidence")
    if state.get("prestart"):
        check_records(state["prestart"]["evidence"], "evidence")
    if state.get("phase") == "prestart":
        check_records(state["evidence"], "evidence")
    if (reviewed_snapshot is not None
            and snapshot(state["identity"]["root"], state.get("local_area")) != reviewed_snapshot):
        raise ValueError("input drift: verification cannot adopt changed workspace; reconcile and repair the authored stage before independent re-review")
    state["integrity"] = digest({k: v for k, v in state.items() if k != "integrity"})
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", newline="\n", dir=path.parent, delete=False) as handle:
        json.dump(state, handle, ensure_ascii=False, indent=2)
        handle.flush()
        os.fsync(handle.fileno())
        temporary = handle.name
    os.replace(temporary, path)


def view(state):
    if state.get("phase") == "prestart":
        return {**state, "next": None}
    done = len(state["completed"])
    return {**state, "next": state["stages"][done] if done < len(state["stages"]) else None}


def check_displaced_partials(state):
    """Prove a legacy reopen from its exact append suffix and prior gate binding.

    Only the latest active repair can displace the immediate successor. After
    recheck restores the completed row, ordinary prefix validation applies.
    This interpretation neither rewrites history nor authenticates consent.
    """
    repair = state["repairs"][-1]
    done = len(state["completed"])
    prior_gate = repair["prior_gate"]
    if (repair["status"] != "active" or repair["stage"] != state["stages"][done]
            or len(repair["reopened"]) != 1
            or repair["reopened"][0]["stage"] != repair["stage"]
            or prior_gate.get("stage") != state["stages"][done + 1]
            or prior_gate.get("kind") != "hard-veto" or prior_gate.get("authority") != "reviewer"
            or not state["gate"] or state["gate"]["kind"] != "hard-veto"
            or state["gate"]["authority"] != "reviewer" or state["gate"]["stage"] != repair["stage"]):
        raise ValueError("displaced partial relationship")
    suffix = [{k: repair["reopened"][0][k] for k in ("stage", "actor", "evidence")}]
    suffix.extend({"stage": repair["stage"], "actor": actor, "evidence": repair["evidence"]}
                  for actor in repair["actors"])
    if state["partial"][-len(suffix):] != suffix:
        raise ValueError("displaced partial suffix")
    previous_repairs = [dict(row) for row in state["repairs"][:-1]]
    if previous_repairs and previous_repairs[-1]["status"] == "superseded":
        previous_repairs[-1]["status"] = "review"
    bound = {"identity": state["identity"], "task": state["task"], "contract": state["contract"],
             "snapshot": {k: v for k, v in repair["before"].items() if k not in ("entries", "index_entries")},
             "completed": state["completed"] + repair["reopened"],
             "partial": state["partial"][:-len(suffix)]}
    if previous_repairs:
        bound["repairs"] = previous_repairs
    if digest(bound) != prior_gate.get("binding"):
        raise ValueError("displaced partial prior binding")
    if repair["before"]["files"] != digest(repair["before"]["entries"]):
        raise ValueError("displaced partial before manifest")
    check_records(repair["evidence"], "evidence")
    review, review_record = read_json_record(repair["evidence"][0]["path"])
    authorization, authorization_record = read_json_record(repair["evidence"][2]["path"])
    if review_record != repair["evidence"][0] or authorization_record != repair["evidence"][2]:
        raise ValueError("evidence drift: recorded repair receipt changed")
    authors = {row["actor"] for row in bound["completed"] + bound["partial"]} | set(repair["actors"])
    if (not isinstance(review, dict) or not isinstance(authorization, dict)
            or any(record.get(k) != v for record in (review, authorization)
                   for k, v in (("task", state["task"]), ("gate", prior_gate["id"]), ("binding", prior_gate["binding"])))
            or review.get("authority") != "reviewer" or review.get("source") != "reviewer-report"
            or review.get("decision") != "blocked" or not isinstance(review.get("actor"), str)
            or not review["actor"].strip() or review["actor"] in authors
            or authorization.get("source") != "human-message" or authorization.get("decision") != "approved"
            or not isinstance(authorization.get("actor"), str) or not authorization["actor"].strip()
            or authorization.get("stage") != repair["stage"] or authorization.get("paths") != repair["paths"]
            or any(not isinstance(record.get("evidence"), str) or not record["evidence"].strip()
                   for record in (review, authorization))):
        raise ValueError("displaced partial repair receipts")
    # Slots 1/3 are the absolute byte captures made by begin_repair, not paths
    # to reinterpret against this process's cwd. Trust them only within this
    # exact active history binding; the checksum is not source authentication.
    if gate_binding(state) != state["gate"]["binding"]:
        raise ValueError("displaced partial active binding")
    captured = [repair["evidence"][1], repair["evidence"][3]]
    check_records(captured, "evidence")
    bases = set()
    for receipt, record in zip((review, authorization), captured):
        raw = Path(receipt["evidence"])
        absolute = Path(record["path"])
        if raw.is_absolute():
            if absolute != raw:
                raise ValueError("displaced partial absolute evidence path")
        else:
            # Path.absolute() preserves '..' and ancestor aliases at capture.
            # Match that lexical boundary, never resolve() to another spelling.
            if (not raw.parts or len(absolute.parts) <= len(raw.parts)
                    or absolute.parts[-len(raw.parts):] != raw.parts):
                raise ValueError("displaced partial relative evidence path")
            bases.add(Path(*absolute.parts[:-len(raw.parts)]))
    if len(bases) > 1:
        raise ValueError("displaced partial evidence capture cwd")
    expected = [review_record, captured[0], authorization_record, captured[1]]
    expected += prior_gate["evidence"] + repair["reopened"][0]["evidence"]
    if repair["evidence"] != expected:
        raise ValueError("displaced partial repair evidence")
    if any(row["stage"] != prior_gate["stage"] for row in state["partial"]
           if row["stage"] not in state["stages"][:done + 1]):
        raise ValueError("unrelated future partial")


def valid_snapshot(value, depth=0):
    """Validate existing recursive Gitlink records without changing snapshot policy."""
    if (depth > 16 or not isinstance(value, dict)
            or any(not isinstance(value.get(k), str) for k in ("head", "branch", "files"))
            or not re.fullmatch(r"[a-f0-9]{64}", value["files"])
            or "index" in value and (not isinstance(value["index"], str)
                                     or not re.fullmatch(r"[a-f0-9]{64}", value["index"]))):
        return False
    children = value.get("submodules", {})
    if not isinstance(children, dict):
        return False
    for name, row in children.items():
        if not isinstance(name, str) or not name or not isinstance(row, dict):
            return False
        # An indexed Gitlink may have a symlink worktree leaf. The producer
        # records that leaf without traversing it, even inside a checkout.
        if set(row) == {"link"}:
            if not isinstance(row["link"], str) or not row["link"]:
                return False
            continue
        if set(row) in ({"directory"}, {"special"}):
            marker = Path(name)
            if (marker.is_absolute() or ".." in marker.parts
                    or not runtime_start_marker_name(marker, Path("."))):
                return False
        if set(row) == {"directory"}:
            if row["directory"] is not True:
                return False
            continue
        if set(row) == {"special"}:
            if type(row["special"]) is not int or row["special"] not in (
                    stat.S_IFIFO, stat.S_IFSOCK, stat.S_IFCHR, stat.S_IFBLK):
                return False
            continue
        if (set(row) != {"gitlink", "checkout"}
                or not isinstance(row.get("gitlink"), list) or not row["gitlink"]
                or any(not isinstance(entry, str) or not entry for entry in row["gitlink"])):
            return False
        checkout = row.get("checkout")
        if checkout != "unavailable" and not valid_snapshot(checkout, depth + 1):
            return False
    return True


def validate_progress(state):
    """Shared schema-2 routing/history guard, before persistence or recovery."""
    try:
        selected = route(state["facts"])
        valid = (state.get("version") == 2
                 and all(isinstance(state[k], str) and state[k].strip() for k in ("task", "audit_root", "compiled_id", "raw"))
                 and isinstance(state["identity"], dict) and isinstance(state["contract"], dict)
                 and valid_snapshot(state["snapshot"])
                 and all(isinstance(state[k], list) for k in ("stages", "inputs", "completed", "partial", "decisions"))
                 and state["stages"] == selected["stages"] and state["tier"] == selected["tier"])
        if not valid or state["raw"] != state["contract"]["raw_entry"]["prompt"]:
            raise ValueError("shape or original raw")
        for record in state["inputs"]:
            if (not isinstance(record, dict) or not isinstance(record.get("path"), str)
                    or not record["path"] or not isinstance(record.get("sha256"), str)):
                raise ValueError("input record")
        for completed in state["completed"]:
            if (not isinstance(completed, dict) or not isinstance(completed.get("stage"), str)
                    or not isinstance(completed.get("actor"), str) or not completed["actor"].strip()
                    or not isinstance(completed.get("evidence"), list) or not completed["evidence"]):
                raise ValueError("stage record")
        if [row["stage"] for row in state["completed"]] != state["stages"][:len(state["completed"])]:
            raise ValueError("stage order")
        for partial in state["partial"]:
            if (not isinstance(partial, dict) or set(partial) != {"stage", "actor", "evidence"}
                    or partial["stage"] not in state["stages"]
                    or not isinstance(partial["actor"], str) or not partial["actor"].strip()
                    or not isinstance(partial["evidence"], list)):
                raise ValueError("partial author record")
        for record in state["decisions"]:
            if not isinstance(record, dict) or not isinstance(record.get("evidence"), list) or not record["evidence"]:
                raise ValueError("decision record")
        if not isinstance(state.get("repairs", []), list):
            raise ValueError("repair history")
        for repair in state.get("repairs", []):
            if (not isinstance(repair, dict) or repair.get("status") not in ("active", "review", "cleared", "superseded")
                    or not isinstance(repair.get("evidence"), list) or not repair["evidence"]
                    or not isinstance(repair.get("before"), dict) or not isinstance(repair["before"].get("entries"), dict)
                    or not isinstance(repair["before"].get("index_entries"), dict)
                    or any(not isinstance(repair["before"].get(k), str) for k in ("head", "branch", "files"))
                    or not isinstance(repair.get("paths"), list) or not repair["paths"]
                    or any(not isinstance(p, str) for p in repair["paths"])
                    or not isinstance(repair.get("actors"), list) or not repair["actors"]
                    or any(not isinstance(a, str) or not a.strip() for a in repair["actors"])
                    or repair.get("stage") not in state["stages"]
                    or not isinstance(repair.get("prior_gate"), dict)
                    or not isinstance(repair.get("reopened"), list)):
                raise ValueError("repair record")
        gate = state["gate"]
        if gate is not None and (not isinstance(gate, dict) or gate.get("authority") not in ("human", "reviewer")
                or gate.get("kind") not in ("decision", "permission", "hard-veto", "release")
                or any(not isinstance(gate.get(k), str) or not gate[k].strip() for k in ("id", "binding", "question", "stage"))
                or not isinstance(gate.get("evidence"), list)):
            raise ValueError("gate record")
        active_reopen = (state.get("repairs") and state["repairs"][-1]["status"] == "active"
                         and state["repairs"][-1]["reopened"])
        if (active_reopen
                or any(row["stage"] not in state["stages"][:len(state["completed"]) + 1] for row in state["partial"])):
            check_displaced_partials(state)
    except (ValueError, TypeError, KeyError, IndexError) as exc:
        if isinstance(exc, ValueError) and str(exc).startswith(("evidence drift:", "input drift:")):
            raise
        raise ValueError("checkpoint refused: malformed routing or progress records; do not infer progress") from None


def load(repo, task, check_tree=True, state_root=None):
    path = state_path(repo, task, state_root)
    state = read_json(path)
    if (not isinstance(state, dict) or state.get("version") != 2
            or state.get("integrity") != digest({k: v for k, v in state.items() if k != "integrity"})):
        raise ValueError("checkpoint refused: corrupt or unsupported checkpoint; do not infer progress")
    if state.get("phase") == "prestart":
        check_prestart(state, repo, task, state_root)
        return path, state
    validate_progress(state)
    if (state["task"] != task or state["identity"] != identity(repo)
            or state.get("local_area", str(local_area(repo, state_root))) != str(local_area(repo, state_root))):
        raise ValueError("identity drift: checkpoint belongs to a different task or worktree")
    check_gate_binding(state)
    if contract(state["audit_root"], state["compiled_id"]) != state["contract"]:
        raise ValueError("contract drift: re-ground and request a new accepted contract")
    if "prestart" in state:
        check_prestart(state["prestart"], repo, task, state_root)
        if (state["prestart"]["raw_id"] != state["contract"]["entry"]["compiled"]["raw_id"]
                or state["prestart"]["draft"]["raw_entry"] != state["contract"]["raw_entry"]):
            raise ValueError("contract drift: prestart original request changed")
        check_prestart_conversion(state)
    elif any("scope_change" in row or "scope_change_record" in row for row in state["decisions"]):
        raise ValueError("prestart refused: scope reconciliation requires its original prestart record")
    check_records(state["inputs"], "input")
    for completed in state["completed"] + state["partial"] + state["decisions"] + state.get("repairs", []):
        check_records(completed["evidence"], "evidence")
    if state["gate"]:
        check_records(state["gate"]["evidence"], "evidence")
    if check_tree and snapshot(state["identity"]["root"], state.get("local_area")) != state["snapshot"]:
        raise ValueError("input drift: workspace changed since the checkpoint")
    return path, state


def check_prestart(state, repo, task, state_root):
    """A recovery pointer, not accepted work or permission to dispatch."""
    if (not isinstance(state, dict) or state.get("version") != 2 or state.get("phase") != "prestart"
            or state.get("integrity") != digest({k: v for k, v in state.items() if k != "integrity"})
            or any(not isinstance(state.get(k), str) or not state[k].strip()
                   for k in ("task", "audit_root", "compiled_id", "raw", "raw_id", "question", "local_area"))
            or not isinstance(state.get("evidence"), list) or not state["evidence"]):
        raise ValueError("checkpoint refused: malformed prestart recovery record")
    project = identity(repo)
    area = str(local_area(repo, state_root))
    if (state["task"] != task or state.get("identity") != project or state["local_area"] != area
            or state.get("recovery") != {"task": task, "repo": project["root"],
                                         "state_root": area if state_root is not None else None,
                                         "checkpoint": str(state_path(repo, task, state_root))}):
        raise ValueError("identity drift: prestart belongs to a different task or project/local area")
    draft = compilation(state["audit_root"], state["compiled_id"])
    if (draft != state.get("draft") or state["raw_id"] != draft["entry"]["compiled"]["raw_id"]
            or state["raw"] != draft["raw_entry"]["prompt"]):
        raise ValueError("contract drift: prestart draft or original request changed")
    check_records(state["evidence"], "evidence")


def check_scope_change(previous, accepted, receipt):
    """Validate an agent-recorded explicit change, not authenticate human consent.

    The agent must check the original answer's meaning and authority before
    authoring this record; arbitrary nonempty evidence is not reconciliation.
    """
    required = {"task", "raw_id", "draft_id", "question", "before", "after",
                "source", "actor", "evidence", "decision"}
    before = {k: previous["draft"]["entry"]["compiled"]["goal_state"][k]
              for k in ("done_when", "not_in_scope")}
    after = {k: accepted["entry"]["compiled"]["goal_state"][k] for k in before}
    if (not isinstance(receipt, dict) or set(receipt) != required
            or any(receipt.get(k) != v for k, v in (("task", previous["task"]),
                ("raw_id", previous["raw_id"]), ("draft_id", previous["compiled_id"]),
                ("question", previous["question"]), ("before", before), ("after", after)))
            or receipt.get("source") != "human-message" or receipt.get("decision") != "approved"
            or any(not isinstance(receipt.get(k), str) or not receipt[k].strip() for k in ("actor", "evidence"))
            or not after["done_when"]
            or any(not isinstance(values, list) or any(not isinstance(v, str) or not v.strip() for v in values)
                   for values in after.values())):
        raise ValueError("prestart refused: scope change needs explicit approved original-to-settled criteria, bound to this task/raw/draft/question and original human answer")


def check_prestart_conversion(state):
    previous = state["prestart"]
    if (not state["decisions"] or state["decisions"][0].get("phase") != "prestart"
            or state["decisions"][0].get("question") != previous["question"]
            or state["decisions"][0].get("draft_id") != previous["compiled_id"]
            or state["decisions"][0].get("compiled_id") != state["compiled_id"]):
        raise ValueError("prestart refused: missing or unrelated conversion history")
    row = state["decisions"][0]
    if (any(record not in row["evidence"] for record in previous["evidence"])
            or not any(record not in previous["evidence"] for record in row["evidence"])):
        raise ValueError("prestart refused: retain original question and answer evidence")
    if "decision_evidence" in row:
        answers = row["decision_evidence"]
        if (not isinstance(answers, list) or not answers
                or any(record not in row["evidence"] for record in answers)):
            raise ValueError("prestart refused: missing original answer evidence")
        check_records(answers, "evidence")
    original = previous["draft"]["entry"]["compiled"]["goal_state"]
    settled = state["contract"]["entry"]["compiled"]["goal_state"]
    if "scope_change" not in row and "scope_change_record" not in row:
        if any(original[k] != settled[k] for k in ("done_when", "not_in_scope")):
            raise ValueError("prestart refused: preserve original completion conditions and exclusions; changed scope requires --scope-change")
        return
    record = row.get("scope_change_record")
    check_records([record], "evidence")
    receipt, captured = read_json_record(record["path"])
    if (captured != record or receipt != row.get("scope_change") or record not in row["evidence"]):
        raise ValueError("evidence drift: scope change receipt changed")
    check_scope_change(previous, state["contract"], receipt)
    if not any(r["path"] == str(Path(receipt["evidence"]).absolute()) for r in row["evidence"]):
        raise ValueError("prestart refused: missing original scope-change answer evidence")


def gate_binding(state):
    bound = {"identity": state["identity"], "task": state["task"],
             "contract": state["contract"], "snapshot": state["snapshot"],
             "completed": state["completed"], "partial": state["partial"]}
    if state.get("repairs"):
        bound["repairs"] = state["repairs"]
    return digest(bound)


def check_gate_binding(state):
    if state["gate"] is not None and gate_binding(state) != state["gate"]["binding"]:
        raise ValueError("checkpoint refused: malformed current gate binding; do not infer progress")


def new_gate(state, kind, authority, question):
    binding = gate_binding(state)
    return {"id": secrets.token_hex(16), "kind": kind, "authority": authority,
            "question": question, "binding": binding, "stage": view(state)["next"], "evidence": []}


def resume(state, receipt_path):
    gate = state["gate"]
    if state.get("repairs") and state["repairs"][-1]["status"] == "active":
        raise ValueError("decision refused: checkpoint the scoped repair and request re-review first")
    receipt, receipt_record = read_json_record(receipt_path)
    if (not gate or not isinstance(receipt, dict)
            or receipt.get("task") != state["task"] or receipt.get("gate") != gate["id"]
            or receipt.get("binding") != gate["binding"] or receipt.get("authority") != gate["authority"]
            or receipt.get("decision") != "approved"
            or not isinstance(receipt.get("actor"), str) or not receipt["actor"].strip()
            or not isinstance(receipt.get("evidence"), str) or not receipt["evidence"].strip()
            or receipt.get("source") != {"human": "human-message", "reviewer": "reviewer-report"}[gate["authority"]]):
        raise ValueError("decision refused: need explicit approval from the gate's authority, bound to this checkpoint")
    if gate["authority"] == "reviewer" and receipt["actor"] in {r["actor"] for r in state["completed"] + state["partial"]}:
        raise ValueError("decision refused: author cannot clear their own hard veto")
    records = [receipt_record, file_record(receipt["evidence"])] + gate["evidence"]
    state["decisions"].append({"gate": gate, "receipt": receipt, "evidence": records})
    if gate["stage"] == "repair-review":
        state["completed"].append({"stage": "repair-review", "actor": receipt["actor"], "evidence": records})
    if state.get("repairs") and state["repairs"][-1]["status"] == "review":
        state["repairs"][-1]["status"] = "cleared"
    state["gate"] = None


def begin_repair(state, args):
    gate = state["gate"]
    review, review_record = read_json_record(args.review)
    authorization, authorization_record = read_json_record(args.authorization)
    if (not gate or gate["kind"] != "hard-veto" or gate["authority"] != "reviewer"
            or state.get("repairs") and state["repairs"][-1]["status"] == "active"
            or not isinstance(review, dict) or not isinstance(authorization, dict)):
        raise ValueError("repair refused: need a current independent hard veto and bound repair scope")
    for record in (review, authorization):
        if (any(record.get(k) != v for k, v in (("task", state["task"]), ("gate", gate["id"]), ("binding", gate["binding"])))
                or not isinstance(record.get("actor"), str) or not record["actor"].strip()
                or not isinstance(record.get("evidence"), str) or not record["evidence"].strip()):
            raise ValueError("repair refused: review and authorization must bind the current gate with original evidence")
    authors = {r["actor"] for r in state["completed"] + state["partial"]}
    if (review.get("authority") != "reviewer" or review.get("source") != "reviewer-report"
            or review.get("decision") != "blocked" or review["actor"] in authors
            or authorization.get("source") != "human-message" or authorization.get("decision") != "approved"
            or authorization.get("stage") != args.stage
            or authorization.get("paths") != args.path
            or any(not actor.strip() for actor in args.actor) or review["actor"] in args.actor):
        raise ValueError("repair refused: need independent BLOCK and explicit human repair scope; no permissions are granted")
    root = Path(state["identity"]["root"])
    for name in args.path:
        relative = Path(name)
        path = root / relative
        if (not name or relative.is_absolute() or ".." in relative.parts or relative.as_posix() != name
                or name == "." or ".git" in relative.parts
                or path.is_relative_to(Path(state["local_area"]))):
            raise ValueError("repair refused: scope must list exact nonsymlink product files, not directories or state")
        validate_scoped_product_path(path, root)
        if (state["identity"].get("kind") == "plain"
                and any(part in {"node_modules", "__pycache__", ".venv", "venv"} for part in relative.parts[:-1])):
            raise ValueError("repair refused: excluded runtime files need separate input reconciliation")
        if state["identity"].get("kind") != "plain" and git(root, "check-ignore", "--", name, optional=True):
            raise ValueError("repair refused: ignored files need separate input reconciliation, not a drift waiver")
    # Only fingerprint content after every authorized path is known to be a
    # bounded regular file or an explicitly missing create/delete leaf.
    before = snapshot(args.repo, state["local_area"], details=True)
    for name in args.path:
        if any(name == submodule or name.startswith(submodule + "/") for submodule in before.get("submodules", {})):
            raise ValueError("repair refused: submodule corrections require separate dependency/input reconciliation")
    reopened = []
    if args.stage != view(state)["next"]:
        if not state["completed"] or state["completed"][-1]["stage"] != args.stage:
            raise ValueError("repair refused: reopen only the current or latest completed stage")
        reopened = [state["completed"][-1]]
    if args.stage in ("verify", "repair-review"):
        raise ValueError("repair refused: select the affected authored stage, not closure or human review")
    evidence = [review_record, file_record(review["evidence"]), authorization_record, file_record(authorization["evidence"])]
    evidence += gate["evidence"] + [r for row in reopened for r in row["evidence"]]
    if {k: v for k, v in before.items() if k not in ("entries", "index_entries")} != state["snapshot"]:
        raise ValueError("input drift: authorize scoped remediation before editing the workspace")
    if reopened:
        state["completed"].pop()
        state["partial"].extend({"stage": row["stage"], "actor": row["actor"], "evidence": row["evidence"]}
                                for row in reopened)
    if state.get("repairs") and state["repairs"][-1]["status"] == "review":
        state["repairs"][-1]["status"] = "superseded"
    repair = {"status": "active", "stage": args.stage, "paths": args.path, "actors": list(dict.fromkeys(args.actor)),
              "prior_gate": gate, "before": before, "reopened": reopened, "evidence": evidence}
    state.setdefault("repairs", []).append(repair)
    state["partial"].extend({"stage": args.stage, "actor": actor, "evidence": evidence} for actor in repair["actors"])
    state["gate"] = new_gate(state, "hard-veto", "reviewer", "Scoped repair in progress; independent re-review required")


def recheck_repair(state, args):
    if not state["gate"] or not state.get("repairs") or state["repairs"][-1]["status"] != "active":
        raise ValueError("repair refused: no authorized repair is active")
    repair = state["repairs"][-1]
    for name in repair["paths"]:
        path = Path(state["identity"]["root"]) / name
        validate_scoped_product_path(path, state["identity"]["root"])
    current = snapshot(args.repo, state["local_area"], details=True)
    def outside(value):
        return [{k: v for k, v in value.get(field, {}).items() if k not in repair["paths"]}
                for field in ("entries", "index_entries")]
    if (current["head"] != repair["before"]["head"] or current["branch"] != repair["before"]["branch"]
            or outside(current) != outside(repair["before"])):
        raise ValueError("repair refused: unrelated workspace drift outside the authorized exact file scope")
    records = [file_record(p) for p in args.evidence]
    repair["evidence"].extend(records)
    repair["status"] = "review"
    if repair["reopened"]:
        state["completed"].append({"stage": repair["stage"], "actor": repair["actors"][0], "evidence": records})
    state["snapshot"] = {k: v for k, v in current.items() if k not in ("entries", "index_entries")}
    state["gate"] = new_gate(state, "hard-veto", "reviewer", "Independently review the scoped repaired artifact; clear with evidence or BLOCK")
    state["gate"]["evidence"] = records


def close_outcome(state, path):
    if path is None:
        raise ValueError("closure refused: map every original criterion to real-path evidence")
    report, report_record = read_json_record(path)
    required = {"criteria", "changes", "tests", "skips", "limits", "remaining_gates"}
    original = state["contract"]["entry"]["compiled"]["goal_state"]["done_when"]
    if (not isinstance(report, dict) or set(report) != required
            or any(not isinstance(report[k], list) for k in required)
            or report["remaining_gates"]
            or any(not isinstance(row, dict) for row in report["criteria"])
            or [row.get("criterion") for row in report["criteria"]] != original):
        raise ValueError("closure refused: original criteria must match exactly; remaining gates cannot be closed")
    records = [report_record]
    for row in report["criteria"]:
        if (not isinstance(row.get("observed"), str) or not row["observed"].strip()
                or not isinstance(row.get("evidence"), list) or not row["evidence"]
                or any(not isinstance(p, str) or not p.strip() for p in row["evidence"])):
            raise ValueError("closure refused: each criterion needs observations and evidence")
        records.extend(file_record(p) for p in row["evidence"])
    return report, records


def execute(args):
    if args.verb == "route":
        return route(read_json(args.facts))
    if args.verb == "forget":
        # FR-107: the one deletion path. Removes only this task's checkpoint, under the writer
        # lock, and the plain-project digest directory if that leaves it empty.
        path = state_path(args.repo, args.task, args.state_root)
        if not path.is_file():
            raise ValueError("unknown task: no checkpoint for {0} in this project".format(args.task))
        path.unlink()
        return {"task": args.task, "forgotten": str(path)}
    if args.verb == "prestart":
        path = state_path(args.repo, args.task, args.state_root)
        if path.exists():
            raise ValueError("task exists: recover it with status instead of overwriting prior work")
        draft = compilation(args.audit_root, args.compiled_id)
        if not args.question.strip():
            raise ValueError("prestart refused: preserve the exact material question before asking")
        project = identity(args.repo)
        area = str(local_area(args.repo, args.state_root))
        state = {"version": 2, "phase": "prestart", "task": args.task, "identity": project, "local_area": area,
                 "audit_root": str(args.audit_root.resolve()), "compiled_id": args.compiled_id, "draft": draft,
                 "raw_id": draft["entry"]["compiled"]["raw_id"], "raw": draft["raw_entry"]["prompt"],
                 "question": args.question, "evidence": [file_record(p) for p in args.evidence],
                 "recovery": {"task": args.task, "repo": project["root"],
                              "state_root": area if args.state_root is not None else None, "checkpoint": str(path)}}
        save(path, state)
        return view(state)
    if args.verb == "start":
        path = state_path(args.repo, args.task, args.state_root)
        previous = None
        if path.exists():
            _, previous = load(args.repo, args.task, state_root=args.state_root)
            if previous.get("phase") != "prestart":
                raise ValueError("task exists: resume it instead of overwriting completed work")
        facts, facts_record = read_json_record(args.facts)
        accepted = contract(args.audit_root, args.compiled_id)
        scope_change = None
        scope_records = []
        if previous and (str(args.audit_root.resolve()) != previous["audit_root"]
                or args.compiled_id == previous["compiled_id"]
                or accepted["entry"]["compiled"]["raw_id"] != previous["raw_id"]
                or accepted["raw_entry"] != previous["draft"]["raw_entry"]
                or not args.decision_evidence):
            raise ValueError("prestart refused: need a new settled compilation from the same original raw id and actual decision source evidence")
        if previous:
            original_goal = previous["draft"]["entry"]["compiled"]["goal_state"]
            settled_goal = accepted["entry"]["compiled"]["goal_state"]
            if args.scope_change:
                scope_change, scope_record = read_json_record(args.scope_change)
                check_scope_change(previous, accepted, scope_change)
                if Path(scope_change["evidence"]).absolute() not in {p.absolute() for p in args.decision_evidence}:
                    raise ValueError("prestart refused: --decision-evidence must include the original scope-change answer")
                scope_records = [scope_record, file_record(scope_change["evidence"])]
            elif any(settled_goal[k] != original_goal[k] for k in ("done_when", "not_in_scope")):
                raise ValueError("prestart refused: preserve original completion conditions and exclusions; changed scope requires --scope-change")
        elif args.scope_change:
            raise ValueError("prestart refused: scope reconciliation requires an existing prestart task")
        compiled_tier = accepted["entry"]["compiled"]["goal_state"]["tier"]
        if compiled_tier not in ("T0", "T1", "T2"):
            raise ValueError("contract refused: unknown cost-of-error tier")
        route(facts)  # Validate caller input before applying the higher safety floor.
        facts["tier"] = max(facts["tier"], compiled_tier)
        selected = route(facts)
        area = str(local_area(args.repo, args.state_root))
        state = {"version": 2, "task": args.task, "identity": identity(args.repo), "local_area": area,
                 "audit_root": str(args.audit_root.resolve()), "compiled_id": args.compiled_id,
                 "contract": accepted, "raw": accepted["raw_entry"]["prompt"], "facts": facts,
                 **selected, "completed": [], "partial": [], "gate": None, "decisions": [],
                 "inputs": [facts_record, *[file_record(p) for p in args.input]],
                 "snapshot": snapshot(args.repo, area)}
        if previous:
            state["prestart"] = previous
            answer_records = [file_record(p) for p in args.decision_evidence]
            state["decisions"].append({"phase": "prestart", "question": previous["question"],
                                       "draft_id": previous["compiled_id"], "compiled_id": args.compiled_id,
                                       "decision_evidence": answer_records,
                                       "evidence": previous["evidence"] + scope_records + answer_records})
            if scope_change is not None:
                state["decisions"][0].update(scope_change=scope_change, scope_change_record=scope_records[0])
        elif args.decision_evidence:
            raise ValueError("prestart refused: decision evidence conversion requires an existing prestart task")
        save(path, state)
        return view(state)
    path, state = load(args.repo, args.task, check_tree=args.verb not in ("complete", "pause", "recheck"), state_root=args.state_root)
    if state.get("phase") == "prestart":
        if args.verb != "status":
            raise ValueError("prestart paused: preserve the human answer, compile a new settled contract from this raw id, then start the same task with --decision-evidence")
        return view(state)
    # Verification observes the reviewed artifact; neither closure nor a human
    # pause may silently turn it into another authoring stage. Fresh proof lives
    # in the excluded local/external evidence area, not among product files.
    if (args.verb in ("complete", "pause") and view(state)["next"] == "verify"
            and snapshot(args.repo, state.get("local_area")) != state["snapshot"]):
        raise ValueError("input drift: verification cannot adopt changed workspace; reconcile and repair the authored stage before independent re-review")
    if args.verb == "complete":
        if state["gate"] or view(state)["next"] != args.stage or not args.actor.strip():
            raise ValueError("stage refused: complete only the current ungated stage")
        records = [file_record(p) for p in args.evidence]
        if args.stage == "verify":
            state["closure"], closure_records = close_outcome(state, args.closure)
            records.extend(closure_records)
        state["completed"].append({"stage": args.stage, "actor": args.actor, "evidence": records})
        current = snapshot(args.repo, state.get("local_area"))
        if args.stage == "verify":
            if current != state["snapshot"]:
                raise ValueError("input drift: verification cannot adopt changed workspace; reconcile and repair the authored stage before independent re-review")
        else:
            state["snapshot"] = current
        trivial = (state["tier"] == "T0" and not state["facts"]["ui"]
                   and not state["facts"]["coordination"])
        if args.stage in ("implement", "migrate", "document", "execute-with-coordination") and not trivial:
            state["gate"] = new_gate(state, "hard-veto", "reviewer", "Independent applicable reviewer: clear the exit checklist with evidence, or BLOCK")
        if view(state)["next"] == "repair-review":
            state["gate"] = new_gate(state, "decision", "human", "Approve the diagnosed repair phases (or provide explicit prior human authorization)?")
        save(path, state, reviewed_snapshot=state["snapshot"] if args.stage == "verify" else None)
    elif args.verb == "pause":
        expected = "reviewer" if args.kind == "hard-veto" else "human"
        if state["gate"] or view(state)["next"] is None or not args.question.strip() or args.authority != expected:
            raise ValueError("gate refused: need an active ungated task and a specific question")
        records = [file_record(p) for p in args.evidence]
        current = snapshot(args.repo, state.get("local_area"))
        verifying = view(state)["next"] == "verify"
        if verifying and current != state["snapshot"]:
            raise ValueError("input drift: verification cannot adopt changed workspace; reconcile and repair the authored stage before independent re-review")
        if current != state["snapshot"] and not records:
            raise ValueError("gate refused: changed in-progress work requires partial-work evidence")
        if ((records or args.kind == "hard-veto") and not args.actor
                or any(not actor.strip() for actor in args.actor)):
            raise ValueError("gate refused: record actual stage authors with --actor for partial work or a hard veto")
        state["partial"].extend({"stage": view(state)["next"], "actor": actor, "evidence": records}
                                for actor in dict.fromkeys(args.actor))
        if not verifying:
            state["snapshot"] = current
        state["gate"] = new_gate(state, args.kind, args.authority, args.question)
        state["gate"]["evidence"] = records
        save(path, state, reviewed_snapshot=state["snapshot"] if verifying else None)
    elif args.verb == "resume":
        resume(state, args.receipt)
        save(path, state)
    elif args.verb == "repair":
        begin_repair(state, args)
        save(path, state)
    elif args.verb == "recheck":
        recheck_repair(state, args)
        save(path, state)
    return view(state)


def _lock_holder(path):
    """The lock's writer and age, or "holder unknown" for a lock this helper cannot read."""
    try:
        holder = json.loads(path.read_text(encoding="utf-8"))
        pid, age = int(holder["pid"]), max(0, int(time.time() - float(holder["created"])))
    except (OSError, ValueError, TypeError, KeyError):
        return "holder unknown"
    return "pid {}, age {}h{:02d}m".format(pid, age // 3600, age % 3600 // 60)


def locked_execute(args):
    if args.verb in ("route", "status"):
        return execute(args)
    path = state_path(args.repo, args.task, args.state_root).with_suffix(".lock")
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError:
        raise ValueError("checkpoint busy: another writer or interrupted operation holds the lock ({}); "
                         "if that process is gone, remove {}".format(_lock_holder(path), path)) from None
    try:
        # FR-097: who holds it and since when, so a stale lock can be told from a live writer.
        os.write(descriptor, json.dumps({"pid": os.getpid(), "created": time.time()}).encode("utf-8"))
        os.close(descriptor)
        return execute(args)
    finally:
        path.unlink()
        if args.verb == "forget" and path.parent != local_area(args.repo, args.state_root):
            try:
                path.parent.rmdir()   # the plain project's digest directory, only if now empty
            except OSError:
                pass


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="verb", required=True)
    for verb in ("route", "prestart", "start", "status", "complete", "pause", "resume", "repair", "recheck",
                 "forget"):
        command = commands.add_parser(verb)
        if verb in ("route", "start"):
            command.add_argument("--facts", type=Path, required=True)
        if verb != "route":
            command.add_argument("--repo", type=Path, default=Path.cwd())
            command.add_argument("--state-root", type=Path)
            command.add_argument("--task", required=True)
        if verb in ("start", "prestart"):
            command.add_argument("--audit-root", type=Path, required=True)
            command.add_argument("--compiled-id", required=True)
        if verb == "prestart":
            command.add_argument("--question", required=True)
            command.add_argument("--evidence", type=Path, action="append", required=True)
        if verb == "start":
            command.add_argument("--input", type=Path, action="append", default=[])
            command.add_argument("--decision-evidence", type=Path, action="append", default=[])
            command.add_argument("--scope-change", type=Path,
                                 help="Explicit agent-recorded human scope reconciliation for this prestart task; not consent authentication")
        if verb == "complete":
            command.add_argument("--stage", required=True)
            command.add_argument("--closure", type=Path)
            command.add_argument("--actor", required=True)
            command.add_argument("--evidence", type=Path, action="append", required=True)
        if verb == "pause":
            command.add_argument("--kind", choices=("decision", "permission", "hard-veto", "release"), required=True)
            command.add_argument("--authority", choices=("human", "reviewer"), required=True)
            command.add_argument("--question", required=True)
            command.add_argument("--actor", action="append", default=[])
            command.add_argument("--evidence", type=Path, action="append", default=[])
        if verb == "resume":
            command.add_argument("--receipt", type=Path, required=True)
        if verb == "repair":
            command.add_argument("--stage", required=True)
            command.add_argument("--review", type=Path, required=True)
            command.add_argument("--authorization", type=Path, required=True)
            command.add_argument("--actor", action="append", required=True)
            command.add_argument("--path", action="append", required=True)
        if verb == "recheck":
            command.add_argument("--evidence", type=Path, action="append", required=True)
    args = parser.parse_args(argv)
    try:
        print(json.dumps(locked_execute(args), ensure_ascii=False))
    except (ValueError, OSError, TypeError, KeyError, subprocess.SubprocessError) as exc:
        print(str(exc), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
