#!/usr/bin/env python3
"""docs-graph.py — the AI-Forward Pack docs script bundle (knowledge-visualization.md V18).

Deterministic mechanics for the knowledge graph, so skills invoke ONE tool instead of
generating ad-hoc scripts at prompt time. Python 3.8+, stdlib only (a built-in parser
covers the V2 frontmatter subset; no PyYAML needed).

Subcommands
  inventory   Scan the graph: artifacts, missing/invalid frontmatter, bad links,
              unregistered rels, orphans, stale (V13), flagged (V16), index drift. JSON out.
  derive      Full derivation sweep: frontmatter -> docs/docs-index.js (V2/V10).
  validate    inventory + nonzero exit on DEFECTS (CI-able). `--gate fail` also fails on
              suggestions (V16 flags, V13 staleness), which only warn by default.
  freshness   The freshness gate's time-based half: stale + flagged + orphans; exit code.
  flag        V16 propagation: --changed <id> --reason "..." flags inbound neighbors.
  clear-flag  Clear a review-suggested flag (--id <artifact> --by <changed-id>) and
              optionally --bump-review <days>.
  stub        Scaffold a new artifact file with schema-correct frontmatter.
  snapshot    Append a graph-health record to docs/health-history.jsonl (governance trend).
  rollup      Aggregate per-artifact markdown tables under a heading (e.g. the designs'
              STRIDE / privacy tables) into one register, each row prefixed with its
              source artifact — paste-ready for the threat-model / privacy-review docs.
  context     Build one deterministic, bounded, provenance-rich grounding packet.

Conventions
  --root defaults to docs/. Excluded from the graph: docs/ai-forward-pack/**, docs/_site/**,
  docs/index.html, docs/docs-index.js, non-.md files. Frontmatter is the record; this tool
  never invents metadata — files without frontmatter are reported, not silently indexed.
"""
import argparse, concurrent.futures, contextlib, datetime, hashlib, json, os, re, stat, sys, tempfile, time
from html.parser import HTMLParser

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


REL_REGISTRY = ["implements","refines","depends-on","supersedes","tested-by","documents","uses-term","relates-to"]
TYPES = ["knowledge","glossary","spec","architecture","adr","design","design-language","investigation","proof-pack","decision-note","threat-model","privacy-review","api","source","doc","index","plan"]
REQUIRED = ["id","title","type","status","summary"]
EXCLUDE_DIRS = {"ai-forward-pack","_site","node_modules",".git"}
TODAY = datetime.date.today().isoformat()
POLICY_VERSION = "traversal-policy/v1"
POLICIES = {
    "grounding": [
        {"rel":"implements","direction":"outbound","priority":0},
        {"rel":"refines","direction":"outbound","priority":1},
        {"rel":"depends-on","direction":"outbound","priority":2},
        {"rel":"uses-term","direction":"outbound","priority":3},
        {"rel":"tested-by","direction":"outbound","priority":4},
        {"rel":"documents","direction":"outbound","priority":5},
    ],
    "impact": [
        {"rel":"implements","direction":"inbound","priority":0},
        {"rel":"refines","direction":"inbound","priority":1},
        {"rel":"depends-on","direction":"inbound","priority":2},
        {"rel":"tested-by","direction":"inbound","priority":3},
        {"rel":"uses-term","direction":"inbound","priority":4},
    ],
    "proof": [{"rel":"tested-by","direction":"outbound","priority":0}],
    "explore-neighborhood": [
        {"rel":rel,"direction":direction,"priority":priority}
        for priority, rel in enumerate(sorted(REL_REGISTRY))
        for direction in ("outbound","inbound")
    ],
}
CONTEXT_LIMITS = {
    "artifacts": 2000, "relationships": 20000,
    "admittedSourceBytes": 64 * 1024 * 1024, "fileBytes": 1024 * 1024,
    "generatedChunks": 1024, "chunkBytes": 64 * 1024, "stderrBytes": 64 * 1024,
}
PACKET_MIN_BYTES = 4096
PACKET_MAX_BYTES = 1024 * 1024
SURFACE_TITLE_BYTES = 256 * 1024
SURFACE_LIMIT = 100
INDEX_BYTE_LIMIT = 5 * 1024 * 1024
INDEX_ARTIFACT_LIMIT = 1000
RELATIONSHIP_LIMIT = 5000
SOURCE_TOTAL_LIMIT = 64 * 1024 * 1024
SURFACE_KIND_ORDER = {
    "audit": 0,
    "documentation": 1,
    "guide": 2,
    "design-preview": 3,
    "knowledge-tool": 4,
}


class DocsGraphError(Exception):
    def __init__(self, code, message, details=None, exit_code=1):
        super().__init__(message)
        self.code = code
        self.message = message
        self.details = details or {}
        self.exit_code = exit_code


class _SourceChangedError(OSError):
    pass


def _source_changed_error(artifact_id):
    return DocsGraphError(
        "SOURCE_CHANGED_DURING_READ",
        "An admitted source changed while its snapshot was captured.",
        {"artifactId": artifact_id},
        3,
    )


def _source_read_failed_error(artifact_id, exc):
    return DocsGraphError(
        "SOURCE_READ_FAILED",
        "An admitted source could not be read.",
        {"artifactId": artifact_id, "reason": type(exc).__name__},
        3,
    )


def canonical_json(value):
    return json.dumps(value, sort_keys=True, separators=(",",":"), ensure_ascii=False)


def sha256_text(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def normalized_source(text):
    return text.replace("\r\n","\n").replace("\r","\n")

# ---------- minimal YAML-subset parser (the V2 frontmatter schema only) ----------
def parse_scalar(v):
    v = v.strip()
    if v == "" or v == "~": return ""
    if v.startswith('"') and v.endswith('"') and len(v) >= 2: return v[1:-1]
    if v.startswith("'") and v.endswith("'") and len(v) >= 2: return v[1:-1]
    if v == "[]": return []
    if v.startswith("[") and v.endswith("]"):
        inner = v[1:-1].strip()
        return [parse_scalar(x) for x in split_flow(inner)] if inner else []
    if v.startswith("{") and v.endswith("}"): return parse_flow_map(v)
    return v

def split_flow(s):
    out, depth, cur, q = [], 0, "", None
    for c in s:
        if q:
            cur += c
            if c == q: q = None
            continue
        if c in "\"'": q = c; cur += c; continue
        if c in "[{": depth += 1
        if c in "]}": depth -= 1
        if c == "," and depth == 0: out.append(cur); cur = ""; continue
        cur += c
    if cur.strip(): out.append(cur)
    return [x.strip() for x in out]

def parse_flow_map(v):
    inner = v.strip()[1:-1]
    d = {}
    for part in split_flow(inner):
        if ":" not in part: continue
        k, val = part.split(":", 1)
        d[k.strip()] = parse_scalar(val)
    return d

def parse_frontmatter(text):
    """Returns (dict, error). Supports: scalars, '>-' folded blocks, '- item' lists,
    '- { k: v }' lists, flow lists/maps. That is the whole V2 schema."""
    if text.startswith("---\n"):
        start, marker = 4, "\n---\n"
    elif text.startswith("---\r\n"):
        start, marker = 5, "\r\n---\r\n"
    else:
        return None, "no frontmatter"
    end = text.find(marker, start)
    if end < 0:
        return None, "no frontmatter"
    lines, fm, i = text[start:end].replace("\r\n","\n").split("\n"), {}, 0
    try:
        while i < len(lines):
            ln = lines[i]
            if not ln.strip() or ln.lstrip().startswith("#"): i += 1; continue
            if ":" not in ln:
                return None, f"unparseable line {i+1}: {ln.strip()[:60]}"
            key, rest = ln.split(":",1)
            if not key or not key[0].isalpha() or any(
                not (character.isalnum() or character in "_-") for character in key
            ):
                return None, f"unparseable line {i+1}: {ln.strip()[:60]}"
            rest = rest.strip()
            if rest in (">-", ">", "|", "|-"):                       # folded/literal block
                i += 1; buf = []
                while i < len(lines) and (lines[i].startswith("  ") or not lines[i].strip()):
                    buf.append(lines[i].strip()); i += 1
                fm[key] = " ".join(b for b in buf if b)
                continue
            if rest == "":                                            # block list, or an empty scalar
                items = []; i += 1
                while i < len(lines) and lines[i].lstrip().startswith("- "):
                    items.append(parse_scalar(lines[i].lstrip()[2:])); i += 1
                # No "- " items after an empty value => an empty scalar (e.g. `review-by:`),
                # not a list. Returning "" keeps derive/validate/Explorer consistent; list
                # consumers coalesce with `or []`, so a genuinely-empty list is unaffected.
                fm[key] = items if items else ""
                continue
            fm[key] = parse_scalar(rest); i += 1
        return fm, None
    except Exception as e:
        return None, f"frontmatter parse error: {e}"

def extract_mermaid_blocks(text):
    lines = normalized_source(text).splitlines()
    diagrams, nearest_heading, i = [], None, 0
    while i < len(lines):
        heading = re.match(r"^\s{0,3}#{1,6}\s+(.+?)\s*$", lines[i])
        if heading:
            nearest_heading = heading.group(1).strip()
            i += 1
            continue
        if re.match(r"^\s*```mermaid\s*$", lines[i], re.I):
            block = []; i += 1
            while i < len(lines) and not re.match(r"^\s*```\s*$", lines[i]):
                block.append(lines[i]); i += 1
            diagrams.append((nearest_heading or f"Diagram {len(diagrams)+1}", "\n".join(block).rstrip()))
        i += 1
    return diagrams


# ---------- scanning ----------
def _is_reparse_point(path, metadata=None):
    try:
        attributes = getattr(metadata or os.lstat(path), "st_file_attributes", 0)
        return bool(attributes & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400))
    except OSError:
        return False


def _safe_path_kind(path, expected):
    try:
        metadata = os.lstat(path)
    except OSError:
        return False
    if stat.S_ISLNK(metadata.st_mode) or _is_reparse_point(path, metadata):
        return False
    return stat.S_ISDIR(metadata.st_mode) if expected == "directory" else stat.S_ISREG(metadata.st_mode)


def _file_identity(metadata):
    return metadata.st_dev, metadata.st_ino


def _snapshot_signature(metadata):
    return (
        metadata.st_dev,
        metadata.st_ino,
        metadata.st_size,
        getattr(metadata, "st_mtime_ns", int(metadata.st_mtime * 1_000_000_000)),
        getattr(metadata, "st_ctime_ns", int(metadata.st_ctime * 1_000_000_000)),
    )


def _open_verified_binary(path, expected_signature=None):
    before = os.stat(path, follow_symlinks=False)
    if (
        not stat.S_ISREG(before.st_mode)
        or stat.S_ISLNK(before.st_mode)
        or _is_reparse_point(path, before)
    ):
        raise _SourceChangedError("source is no longer a regular file")
    if expected_signature is not None and _snapshot_signature(before) != expected_signature:
        raise _SourceChangedError("source changed after admission")
    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        opened = os.fstat(descriptor)
        after = os.stat(path, follow_symlinks=False)
        if (
            not stat.S_ISREG(opened.st_mode)
            or not stat.S_ISREG(after.st_mode)
            or _is_reparse_point(path, after)
            or _file_identity(before) != _file_identity(opened)
            or _file_identity(opened) != _file_identity(after)
        ):
            raise _SourceChangedError("source changed while it was opened")
        return os.fdopen(descriptor, "rb"), opened
    except Exception:
        os.close(descriptor)
        raise


def _verify_open_snapshot(path, source_file, opened):
    current = os.fstat(source_file.fileno())
    after = os.stat(path, follow_symlinks=False)
    if (
        _snapshot_signature(current) != _snapshot_signature(opened)
        or not stat.S_ISREG(after.st_mode)
        or _is_reparse_point(path, after)
        or _file_identity(current) != _file_identity(after)
    ):
        raise _SourceChangedError("source changed during read")


def _reject_unsafe_destination(destination):
    """Reject a write to a destination that exists and is NOT a regular file (symlink, reparse
    point, device, ...). Runs BEFORE any lock or temp-file side effect, so an append rejects a
    bad destination deterministically on every OS (PACK-J) rather than tripping over a lock-file
    FileExistsError first. Returns the existing file mode (or None) for callers that preserve it."""
    if os.path.lexists(destination):
        metadata = os.stat(destination, follow_symlinks=False)
        if (
            not stat.S_ISREG(metadata.st_mode)
            or stat.S_ISLNK(metadata.st_mode)
            or _is_reparse_point(destination, metadata)
        ):
            raise DocsGraphError(
                "SOURCE_WRITE_REJECTED",
                "The destination is not a regular file.",
                {"artifactId": os.path.basename(destination)},
                3,
            )
        return stat.S_IMODE(metadata.st_mode)
    return None


def _atomic_write_text(path, text):
    destination = os.path.abspath(path)
    directory = os.path.dirname(destination)
    os.makedirs(directory, exist_ok=True)
    existing_mode = _reject_unsafe_destination(destination)
    # No text=True: the fd is reopened below with an explicit encoding and newline="\n".
    # On Windows text=True would set O_TEXT on the descriptor and translate "\n" to CRLF
    # underneath that newline="" contract; elsewhere the flag does nothing at all.
    descriptor, temporary = tempfile.mkstemp(
        prefix=".docs-graph-",
        suffix=".tmp",
        dir=directory,
    )
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as output:
            output.write(normalized_source(text))
            output.flush()
            os.fsync(output.fileno())
        if existing_mode is not None:
            os.chmod(temporary, existing_mode)
        os.replace(temporary, destination)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


@contextlib.contextmanager
def _exclusive_file_lock(path, timeout_seconds=10.0):
    lock_path = os.path.abspath(path) + ".lock"
    os.makedirs(os.path.dirname(lock_path), exist_ok=True)
    flags = (
        os.O_RDWR
        | os.O_CREAT
        | getattr(os, "O_BINARY", 0)
        | getattr(os, "O_NOFOLLOW", 0)
    )
    descriptor = os.open(lock_path, flags, 0o600)
    locked = False
    try:
        opened = os.fstat(descriptor)
        current = os.stat(lock_path, follow_symlinks=False)
        if (
            not stat.S_ISREG(opened.st_mode)
            or not stat.S_ISREG(current.st_mode)
            or _is_reparse_point(lock_path, current)
            or _file_identity(opened) != _file_identity(current)
        ):
            raise DocsGraphError(
                "SOURCE_WRITE_REJECTED",
                "The append lock is not a regular file.",
                {"artifactId": os.path.basename(path)},
                3,
            )

        deadline = time.monotonic() + timeout_seconds
        if os.name == "nt":
            import msvcrt

            while True:
                try:
                    os.lseek(descriptor, 0, os.SEEK_SET)
                    msvcrt.locking(descriptor, msvcrt.LK_NBLCK, 1)
                    locked = True
                    break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise DocsGraphError(
                            "SOURCE_WRITE_LOCK_TIMEOUT",
                            "Timed out waiting to append to the destination.",
                            {"artifactId": os.path.basename(path)},
                            3,
                        )
                    time.sleep(0.01)
        else:
            import fcntl

            while True:
                try:
                    fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    locked = True
                    break
                except BlockingIOError:
                    if time.monotonic() >= deadline:
                        raise DocsGraphError(
                            "SOURCE_WRITE_LOCK_TIMEOUT",
                            "Timed out waiting to append to the destination.",
                            {"artifactId": os.path.basename(path)},
                            3,
                        )
                    time.sleep(0.01)
        yield
    finally:
        if locked:
            if os.name == "nt":
                import msvcrt

                os.lseek(descriptor, 0, os.SEEK_SET)
                msvcrt.locking(descriptor, msvcrt.LK_UNLCK, 1)
            else:
                import fcntl

                fcntl.flock(descriptor, fcntl.LOCK_UN)
        os.close(descriptor)


def _atomic_append_text(path, text):
    # Reject an unsafe destination (symlink / non-regular / reparse) BEFORE taking the lock, so the
    # rejection is deterministic across OSes rather than racing a lock-file side effect (PACK-J).
    _reject_unsafe_destination(os.path.abspath(path))
    with _exclusive_file_lock(path):
        existing = ""
        if os.path.lexists(path):
            try:
                source, opened = _open_verified_binary(path)
                with source:
                    existing = source.read().decode("utf-8")
                    _verify_open_snapshot(path, source, opened)
            except (_SourceChangedError, OSError, UnicodeError) as exc:
                raise DocsGraphError(
                    "SOURCE_WRITE_REJECTED",
                    "The destination could not be safely read before writing.",
                    {"artifactId": os.path.basename(path)},
                    3,
                ) from exc
        _atomic_write_text(path, existing + text)


def _read_metadata(path, limit=256 * 1024):
    chunks, total, delimiters = [], 0, 0
    source, opened = _open_verified_binary(path)
    with source:
        for line in source:
            total += len(line)
            if total > limit:
                raise ValueError("frontmatter exceeds metadata limit")
            chunks.append(line)
            if line.strip() == b"---":
                delimiters += 1
                if delimiters == 2:
                    break
        _verify_open_snapshot(path, source, opened)
    return b"".join(chunks).decode("utf-8")


def _read_source_bounded(path, artifact_id):
    try:
        source_file, opened = _open_verified_binary(path)
        with source_file:
            data = source_file.read(CONTEXT_LIMITS["fileBytes"] + 1)
            _verify_open_snapshot(path, source_file, opened)
    except _SourceChangedError:
        raise _source_changed_error(artifact_id) from None
    except OSError as exc:
        raise _source_read_failed_error(artifact_id, exc) from exc
    if len(data) > CONTEXT_LIMITS["fileBytes"]:
        raise DocsGraphError(
            "SOURCE_FILE_TOO_LARGE",
            "A Markdown source exceeds the per-file limit.",
            {"artifactId":artifact_id,"maximum":CONTEXT_LIMITS["fileBytes"]},
            3,
        )
    try:
        return data.decode("utf-8"), len(data)
    except UnicodeError as exc:
        raise _source_read_failed_error(artifact_id, exc) from exc


def _markdown_candidates(root, artifact_limit=None):
    candidates, problems = [], []
    absolute_root = os.path.abspath(root)
    public_parent = os.path.dirname(absolute_root)
    for dp, dns, fns in os.walk(absolute_root):
        retained = []
        for directory in sorted(dns):
            candidate = os.path.join(dp, directory)
            rel = os.path.relpath(candidate, public_parent).replace("\\", "/")
            if directory in EXCLUDE_DIRS:
                continue
            if (
                not _safe_path_kind(candidate, "directory")
            ):
                problems.append(
                    {"file": rel, "problem": "graph directory escapes the approved root"}
                )
                continue
            retained.append(directory)
        dns[:] = retained
        for filename in sorted(fns):
            if not filename.endswith(".md"):
                continue
            path = os.path.join(dp, filename)
            rel = os.path.relpath(path, public_parent).replace("\\", "/")
            if (
                not _safe_path_kind(path, "file")
            ):
                problems.append(
                    {"file": rel, "problem": "graph source is not a regular file"}
                )
                continue
            candidates.append((path, rel))
            if artifact_limit is not None and len(candidates) > artifact_limit:
                raise DocsGraphError(
                    "SCAN_LIMIT_EXCEEDED",
                    "The artifact scan limit was exceeded.",
                    {"limit": "artifacts", "maximum": artifact_limit},
                    4,
                )
    candidates.sort(key=lambda candidate: candidate[1])
    return candidates, problems


class _TitleParser(HTMLParser):
    def __init__(self):
        HTMLParser.__init__(self, convert_charrefs=True)
        self.in_title = False
        self.current_parts = []
        self.titles = []

    def handle_starttag(self, tag, attrs):
        if tag.lower() == "title" and not self.titles:
            self.in_title = True
            self.current_parts = []

    def handle_endtag(self, tag):
        if tag.lower() == "title" and self.in_title:
            self.in_title = False
            # <title> is an RCDATA element: on newer CPython (3.12.11+/3.13/3.14) a script/style-
            # shaped title arrives as raw text that KEEPS its tags, while older builds parsed the
            # inner tag away. Strip any residual tag spans so the extracted title is identical across
            # versions and platforms (PACK-J) - a title is display text, never markup.
            joined = re.sub(r"<[^>]+>", "", " ".join(self.current_parts))
            title = re.sub(r"\s+", " ", joined).strip()
            if title:
                self.titles.append(title)
            self.current_parts = []

    def handle_data(self, data):
        if self.in_title:
            self.current_parts.append(data)

    def title(self):
        return self.titles[0] if self.titles else ""


def _surface_title(path, relative_to_root):
    normalized = relative_to_root.replace("\\", "/").lower()
    explicit = {
        "_site/index.html": "AI-Forward Documentation",
        "_site/bundle.html": "AI-Forward Bundle View",
        "portal/index.html": "AI-Forward Portal",
        "mockups/documentation-portal.html": "Portal Mockup",
    }
    if normalized in explicit:
        return explicit[normalized]
    try:
        source, opened = _open_verified_binary(path)
        with source:
            data = source.read(SURFACE_TITLE_BYTES)
            _verify_open_snapshot(path, source, opened)
        parser = _TitleParser()
        parser.feed(data.decode("utf-8"))
        title = parser.title()
        if title:
            return title
    except (OSError, UnicodeError, _SourceChangedError):
        return None
    stem = os.path.splitext(os.path.basename(relative_to_root))[0]
    if stem.lower() == "index":
        stem = os.path.basename(os.path.dirname(relative_to_root)) or "Documentation"
    return re.sub(r"[-_]+", " ", stem).strip().title()


def _surface_kind(relative_to_root):
    normalized = relative_to_root.replace("\\", "/")
    if normalized == "audit/index.html":
        return "audit"
    if normalized == "_site/index.html":
        return "documentation"
    if normalized.startswith("design/"):
        return "design-preview"
    if normalized == "guide.html" or normalized.endswith("-guide.html"):
        return "guide"
    return "knowledge-tool"


def _surface_description(kind):
    return {
        "audit": "Browse the committed audit and change timeline.",
        "documentation": "Open the generated documentation bundle.",
        "guide": "Read a how-to guide for a capability in this repository.",
        "design-preview": "Inspect a rendered design or design-language preview.",
        "knowledge-tool": "Open an interactive knowledge artifact.",
    }[kind]


def _surface_id(relative_to_root):
    without_extension = os.path.splitext(relative_to_root.replace("\\", "/"))[0]
    slug = re.sub(r"[^a-z0-9]+", "-", without_extension.lower()).strip("-")
    return "surface-" + (slug or "knowledge")


def discover_html_surfaces(root, artifacts):
    absolute_root = os.path.abspath(root)
    public_parent = os.path.dirname(absolute_root)
    candidates = []
    for directory_path, directory_names, file_names in os.walk(absolute_root):
        retained = []
        for directory_name in sorted(directory_names):
            candidate = os.path.join(directory_path, directory_name)
            relative_directory = os.path.relpath(candidate, absolute_root).replace("\\", "/")
            if (
                relative_directory == "ai-forward-pack/templates"
                or relative_directory.startswith("ai-forward-pack/templates/")
                or directory_name in {"node_modules", ".git", "__pycache__"}
            ):
                continue
            if not _safe_path_kind(candidate, "directory"):
                continue
            retained.append(directory_name)
        directory_names[:] = retained
        for file_name in sorted(file_names):
            if not file_name.lower().endswith(".html"):
                continue
            path = os.path.join(directory_path, file_name)
            relative_to_root = os.path.relpath(path, absolute_root).replace("\\", "/")
            if relative_to_root == "index.html":
                continue
            if (
                relative_to_root == "ai-forward-pack/templates"
                or relative_to_root.startswith("ai-forward-pack/templates/")
            ):
                continue
            if not _safe_path_kind(path, "file"):
                continue
            if len(candidates) >= SURFACE_LIMIT:
                raise DocsGraphError(
                    "SURFACE_LIMIT_EXCEEDED",
                    "The HTML knowledge-surface limit was exceeded.",
                    {"limit": "surfaces", "maximum": SURFACE_LIMIT},
                    4,
                )
            title = _surface_title(path, relative_to_root)
            if not title:
                continue
            public_path = os.path.relpath(path, public_parent).replace("\\", "/")
            candidates.append(
                {
                    "id": _surface_id(relative_to_root),
                    "path": public_path,
                    "title": title,
                    "kind": _surface_kind(relative_to_root),
                    "description": _surface_description(_surface_kind(relative_to_root)),
                    "_relative": relative_to_root,
                }
            )
    candidates.sort(
        key=lambda surface: (
            SURFACE_KIND_ORDER.get(surface["kind"], 999),
            surface["title"].casefold(),
            surface["path"],
        )
    )
    artifacts_by_directory = {}
    artifacts_by_stem = {}
    for artifact in artifacts:
        artifact_path = artifact.get("path") or artifact.get("_path") or ""
        directory = os.path.dirname(artifact_path).replace("\\", "/")
        artifacts_by_directory.setdefault(directory, []).append(artifact)
        artifacts_by_stem[os.path.splitext(artifact_path)[0].replace("\\", "/")] = artifact
    seen_ids = set()
    for surface in candidates:
        base_id = surface["id"]
        if base_id in seen_ids:
            surface["id"] = "{0}-{1}".format(
                base_id, hashlib.sha256(surface["path"].encode("utf-8")).hexdigest()[:8]
            )
        seen_ids.add(surface["id"])
        html_stem = os.path.splitext(surface["path"])[0]
        artifact = artifacts_by_stem.get(html_stem)
        if artifact is None and os.path.basename(surface["path"]).lower() == "index.html":
            directory_artifacts = artifacts_by_directory.get(
                os.path.dirname(surface["path"]).replace("\\", "/"), []
            )
            if len(directory_artifacts) == 1:
                artifact = directory_artifacts[0]
        if artifact is not None:
            surface["artifactId"] = artifact["id"]
        surface.pop("_relative", None)
    return candidates


def scan(root, metadata_only=False, artifact_limit=None):
    candidates, problems = _markdown_candidates(root, artifact_limit)
    arts = []
    for path, rel in candidates:
        try:
            if metadata_only:
                text = _read_metadata(path)
            else:
                source, opened = _open_verified_binary(path)
                with source:
                    text = source.read().decode("utf-8")
                    _verify_open_snapshot(path, source, opened)
        except (OSError, UnicodeError, ValueError) as e:
            problems.append({"file": rel, "problem": f"unreadable: {type(e).__name__}"})
            continue
        fm, problem = _validate_frontmatter(text, rel)
        if problem:
            problems.append(problem)
            continue
        fm["_fs_path"] = path
        fm["_diagrams"] = [] if metadata_only else [
            {"kind": sniff_kind(code), "title": title, "mermaid": code}
            for title, code in extract_mermaid_blocks(text)
        ]
        arts.append(fm)
    return arts, problems


def _validate_frontmatter(text, rel):
    fm, err = parse_frontmatter(text)
    if err:
        return None, {"file": rel, "problem": err}
    missing = [key for key in REQUIRED if not fm.get(key)]
    if missing:
        return None, {
            "file": rel,
            "problem": "missing required keys: {0}".format(",".join(missing)),
        }
    if fm.get("type") and fm["type"] not in TYPES:
        return None, {
            "file": rel,
            "problem": "unknown type: {0}".format(fm["type"]),
        }
    links = [link for link in (fm.get("links") or []) if isinstance(link, dict)]
    fm["links"] = [
        {"to": link.get("to", ""), "rel": link.get("rel", "")}
        for link in links
    ]
    fm["_path"] = rel
    return fm, None


def _prepare_context_candidates(candidates):
    prepared, admitted = [], 0
    for candidate in candidates:
        path, _ = candidate
        source_label = os.path.splitext(os.path.basename(path))[0]
        try:
            metadata = os.stat(path, follow_symlinks=False)
        except OSError as exc:
            raise _source_read_failed_error(source_label, exc) from exc
        if metadata.st_size > CONTEXT_LIMITS["fileBytes"]:
            raise DocsGraphError(
                "SOURCE_FILE_TOO_LARGE",
                "A Markdown source exceeds the per-file limit.",
                {"artifactId": source_label, "maximum": CONTEXT_LIMITS["fileBytes"]},
                3,
            )
        if admitted + metadata.st_size > CONTEXT_LIMITS["admittedSourceBytes"]:
            raise DocsGraphError(
                "SCAN_LIMIT_EXCEEDED",
                "The admitted source byte limit was exceeded.",
                {
                    "limit": "admittedSourceBytes",
                    "maximum": CONTEXT_LIMITS["admittedSourceBytes"],
                },
                4,
            )
        admitted += metadata.st_size
        prepared.append((candidate, metadata.st_size, _snapshot_signature(metadata)))
    return prepared, admitted


def _read_context_candidate(prepared):
    candidate, expected_size, expected_signature = prepared
    path, rel = candidate
    source_label = os.path.splitext(os.path.basename(path))[0]
    try:
        source_file, opened = _open_verified_binary(path, expected_signature)
        with source_file:
            if opened.st_size != expected_size:
                raise _SourceChangedError("source size changed after admission")
            data = source_file.read(expected_size)
            if len(data) != expected_size:
                raise _SourceChangedError("source length changed during read")
            _verify_open_snapshot(path, source_file, opened)
    except _SourceChangedError:
        raise _source_changed_error(source_label) from None
    except OSError as exc:
        raise _source_read_failed_error(source_label, exc) from exc
    try:
        text = data.decode("utf-8")
    except UnicodeError as exc:
        raise _source_read_failed_error(source_label, exc) from exc
    actual_bytes = len(data)
    source_hash = (
        hashlib.sha256(data).hexdigest()
        if b"\r" not in data
        else sha256_text(normalized_source(text))
    )
    fm, problem = _validate_frontmatter(text, rel)
    if problem:
        return None, None, actual_bytes, problem
    fm["_fs_path"] = path
    fm["_diagrams"] = []
    return fm, source_hash, actual_bytes, None


def scan_context(root):
    candidates, problems = _markdown_candidates(root, CONTEXT_LIMITS["artifacts"])
    arts, source_hashes = [], {}
    prepared, admitted = _prepare_context_candidates(candidates)
    workers = min(16, max(1, len(prepared)))
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as executor:
        results = list(executor.map(_read_context_candidate, prepared))
    for artifact, source_hash, _, problem in results:
        if problem:
            problems.append(problem)
            continue
        arts.append(artifact)
        source_hashes[artifact["id"]] = source_hash
    return arts, problems, source_hashes, admitted


def read_context_source(artifact, expected_hash):
    source, _ = _read_source_bounded(artifact["_fs_path"], artifact["id"])
    normalized = normalized_source(source)
    if sha256_text(normalized) != expected_hash:
        raise _source_changed_error(artifact["id"])
    return normalized

def sniff_kind(code):
    head = code.strip().split(None, 1)[0].lower() if code.strip() else ""
    return {"sequencediagram":"sequence","classdiagram":"class","statediagram":"state",
            "statediagram-v2":"state","flowchart":"flowchart","graph":"flowchart",
            "erdiagram":"er","timeline":"timeline"}.get(head, "c4" if head.startswith("c4") else head or "diagram")

def analyze(arts, problems):
    by_id, dup = {}, []
    for a in arts:
        artifact_id = a.get("id")
        if not artifact_id:
            continue
        if artifact_id in by_id:
            dup.append(artifact_id)
        by_id[artifact_id] = a
    inbound = {}
    for a in arts:
        artifact_id = a.get("id")
        if not artifact_id:
            continue
        for l in a["links"]:
            if l["rel"] not in REL_REGISTRY:
                problems.append({"file": a["_path"], "problem": f"unregistered rel: {l['rel']} (V14)"})
            if l["to"] and l["to"] not in by_id:
                problems.append({"file": a["_path"], "problem": f"dangling link target: {l['to']}"})
            inbound.setdefault(l["to"], []).append({"from": artifact_id, "rel": l["rel"]})
    for d in dup: problems.append({"file": by_id[d]["_path"], "problem": f"duplicate id: {d}"})
    frozen = {"superseded", "resolved"}
    stale   = [a["id"] for a in arts if a.get("id") and a.get("review-by") and str(a["review-by"]) < TODAY and a.get("status") not in frozen]
    flagged = [a["id"] for a in arts if a.get("id") and a.get("review-suggested")]
    orphans = [a["id"] for a in arts if a.get("id") and not a["links"] and a["id"] not in inbound]
    return by_id, inbound, stale, flagged, orphans

# ---------- subcommands ----------
def cmd_inventory(args, exit_on_findings=False):
    arts, problems = scan(args.root, metadata_only=True, artifact_limit=CONTEXT_LIMITS["artifacts"])
    by_id, inbound, stale, flagged, orphans = analyze(arts, problems)
    drift = index_drift(args, arts)
    # FR-056. Findings are two different KINDS and must not share one exit code.
    #   defects     - the graph is wrong: invalid frontmatter, dangling link, unregistered
    #                 rel, duplicate id, orphan (V10), index drift (V11). An author can fix
    #                 every one of these before pushing. These always fail the gate.
    #   suggestions - `review-suggested` (V16) and `review-by` decay (V13). V16 defines a
    #                 flag as "a suggestion with provenance, not a status change", cleared
    #                 by the NEIGHBOUR'S OWNER on a human timescale. Failing a build on one
    #                 makes the correct act - propagating a material change - turn main red
    #                 for everyone, so the rational move becomes skipping the propagation
    #                 the mandate requires. A control that punishes compliance is worse than
    #                 no control: it is unobservable non-compliance.
    # Both stay in the JSON either way (IO4: a suggestion that vanishes is worse than one
    # that fails); --gate fail restores the strict behaviour for maintainers who want it.
    defects = {"problems": problems, "orphans": orphans, "index_drift": drift}
    suggestions = {"flagged": flagged, "stale": stale}
    n_defects = sum(len(v) for v in defects.values())
    n_suggestions = sum(len(v) for v in suggestions.values())
    report = {"root": args.root, "today": TODAY, "artifacts": len(arts),
              "problems": problems, "stale": stale, "flagged": flagged,
              "orphans": orphans, "index_drift": drift,
              "defects": n_defects, "suggestions": n_suggestions,
              "by_type": count_by(arts, "type")}
    print(json.dumps(report, indent=2))
    if not exit_on_findings:
        return 0
    strict = getattr(args, "gate", "warn") == "fail"
    if n_suggestions and not strict:
        # Visible on stderr so a warn is never silence (IO8: degrade, and say so).
        print(f"warning: {n_suggestions} suggestion(s) - "
              f"{len(flagged)} review-suggested (V16), {len(stale)} stale (V13). "
              f"Not failing the gate; run with --gate fail to treat them as errors.",
              file=sys.stderr)
    if n_defects:
        print(f"validate: {n_defects} defect(s) - "
              f"{len(problems)} problem(s), {len(orphans)} orphan(s), "
              f"{len(drift)} index-drift item(s).", file=sys.stderr)
    return 1 if (n_defects or (n_suggestions and strict)) else 0

def count_by(arts, key):
    d = {}
    for a in arts: d[a.get(key, "?")] = d.get(a.get(key, "?"), 0) + 1
    return d

def index_drift(args, arts):
    """Compare derived-from-frontmatter against the existing docs-index.js (ids + shallow fields)."""
    p = args.out if hasattr(args, "out") and args.out else os.path.join(args.root, "docs-index.js")
    if not os.path.exists(p): return ["docs-index.js missing"]
    try:
        with open(p, encoding="utf-8") as source:
            m = re.search(
                r"window\.DOCS_INDEX\s*=\s*(\{.*\});?\s*$",
                source.read(),
                re.S,
            )
        idx = json.loads(m.group(1)) if m else None
    except Exception:
        return ["docs-index.js unparseable as JSON (regenerate with `derive`)"]
    if not idx: return ["docs-index.js unparseable"]
    have = {e.get("id"): e for e in idx.get("artifacts", [])}
    want = {a.get("id"): a for a in arts}
    drift = [f"in index, file gone: {i}" for i in have if i not in want]
    drift += [f"file not in index: {i}" for i in want if i not in have]
    for i in set(have) & set(want):
        for k_idx, k_fm in [("status","status"), ("owner","owner"), ("reviewBy","review-by"), ("summary","summary")]:
            if str(have[i].get(k_idx) or "") != str(want[i].get(k_fm) or ""):
                drift.append(f"{i}: index.{k_idx} != frontmatter.{k_fm}")
    return drift


def policy_hash():
    return sha256_text(canonical_json({"version":POLICY_VERSION,"policies":POLICIES}))


def graph_hash(entries, project):
    canonical = []
    for entry in sorted(entries, key=lambda item: item["id"]):
        canonical.append({
            "id":entry["id"], "path":entry["path"], "title":entry["title"],
            "type":entry["type"], "status":entry.get("status",""),
            "owner":entry.get("owner",""), "reviewBy":entry.get("reviewBy",""),
            "reviewSuggested":entry.get("reviewSuggested",[]),
            "tags":sorted(entry.get("tags",[])),
            "links":sorted(entry.get("links",[]), key=lambda link:(link["to"],link["rel"])),
            "summary":entry.get("summary",""), "sourceSha256":entry.get("sourceSha256",""),
        })
    return sha256_text(canonical_json({"schemaVersion":"docs-index/v2","project":project,"artifacts":canonical}))


def project_identity(root, explicit=None):
    if explicit:
        return explicit
    index_path = os.path.join(root, "docs-index.js")
    if os.path.exists(index_path):
        try:
            with open(index_path, encoding="utf-8") as source:
                match = re.search(
                    r"window\.DOCS_INDEX\s*=\s*(\{.*\});?\s*$",
                    source.read(),
                    re.S,
                )
            index = json.loads(match.group(1)) if match else None
            if index and index.get("project"):
                return str(index["project"])
        except (OSError, UnicodeError, ValueError):
            pass
    # Never basename(cwd): from a linked worktree that bakes the WORKTREE name into the
    # index, and the index is then read back as the answer above -- self-perpetuating (PACK-P).
    here = os.path.dirname(os.path.abspath(__file__))
    if here not in sys.path:
        sys.path.insert(0, here)
    try:
        from repo_identity import canonical_project
    except ImportError:
        return os.path.basename(os.path.abspath(os.path.join(root, "..")))
    return canonical_project(os.path.join(root, ".."))


def project_root_id(entries):
    ids = {entry["id"] for entry in entries}
    for candidate in ("architecture","docs","skills"):
        if candidate in ids: return candidate
    return sorted(ids)[0] if ids else None


def _frontmatter_snapshot(artifact):
    return {
        key: value
        for key, value in artifact.items()
        if not key.startswith("_")
    }


def _opted_into_docs_explorer(root, dst):
    """pack-onoff-analysis.html #3 (class PK-03, F-6): docs-index.js is a generated pack
    artifact; it must not appear, unasked, in a repo that never opted into the Docs
    Explorer. 'Opted in' is read from what is ALREADY on disk, independent of this call --
    the destination file itself, or docs/index.html (the Explorer shell a skill
    instantiates before calling derive). A repo with neither has not opted in; `derive`
    then refuses to seed the FIRST one and says so, rather than creating docs/ in a product
    tree nobody asked to put it in."""
    if os.path.exists(dst):
        return True
    return os.path.exists(os.path.join(root, "index.html"))


def cmd_derive(args):
    arts, problems = scan(
        args.root,
        metadata_only=True,
        artifact_limit=INDEX_ARTIFACT_LIMIT,
    )
    analyze(arts, problems)   # populate problems (reported to stderr, not blocking)
    entries = []
    source_snapshots = {}
    source_bytes = 0
    relationships = 0
    for a in sorted(arts, key=lambda x: (x.get("type",""), x.get("id",""))):
        text, _ = _read_source_bounded(a["_fs_path"], a["id"])
        source_bytes += len(text.encode("utf-8"))
        if source_bytes > SOURCE_TOTAL_LIMIT:
            raise DocsGraphError(
                "DERIVE_SOURCE_LIMIT_EXCEEDED",
                "The derivation source byte limit was exceeded.",
                {"maximum": SOURCE_TOTAL_LIMIT, "observed": source_bytes},
                4,
            )
        current, error = parse_frontmatter(text)
        if error:
            raise _source_changed_error(a["id"])
        links = [link for link in (current.get("links") or []) if isinstance(link, dict)]
        current["links"] = [
            {"to":link.get("to",""),"rel":link.get("rel","")}
            for link in links
        ]
        relationships += len(current["links"])
        if relationships > RELATIONSHIP_LIMIT:
            raise DocsGraphError(
                "DERIVE_RELATIONSHIP_LIMIT_EXCEEDED",
                "The derivation relationship limit was exceeded.",
                {"maximum": RELATIONSHIP_LIMIT, "observed": relationships},
                4,
            )
        if canonical_json(_frontmatter_snapshot(a)) != canonical_json(current):
            raise _source_changed_error(a["id"])
        source = normalized_source(text)
        source_snapshots[a["_fs_path"]] = sha256_text(source)
        diagrams = [
            {"kind": sniff_kind(code), "title": title, "mermaid": code}
            for title, code in extract_mermaid_blocks(text)
        ]
        entries.append({"id": a.get("id"), "path": a["_path"], "title": a.get("title"),
                        "type": a.get("type"), "status": a.get("status"),
                        "owner": a.get("owner",""), "phase": str(a.get("phase","")),
                        "reviewBy": str(a.get("review-by","")),
                        "reviewSuggested": a.get("review-suggested") or [],
                        "summary": a.get("summary",""), "tags": a.get("tags") or [],
                        "links": a["links"], "diagrams": diagrams,
                        "sourceSha256": sha256_text(source)})
    project = project_identity(args.root, args.project)
    surfaces = discover_html_surfaces(args.root, entries)
    out = {"schemaVersion":"docs-index/v2", "project": project,
           # FR-068. No build timestamp. A wall-clock stamp made this file differ on every run,
           # so docs-index.js could never be byte-stable and therefore could never sit inside a
           # drift gate - which in turn blocked FR-060's fix (having sync regenerate the index
           # BEFORE the portal and web index that read it, since every skill runs `derive` as
           # its last action). The field was read by nothing: no consumer in docs/index.html,
           # docs-explorer-core.js, docs/portal, build-docs-portal.py, or any test.
           # This is the same call the sibling generator already made and documented for
           # web/pack-index.js (PACK-I / the FR-048 timestamp class); docs-graph.py had simply
           # never learned it. Git already records when the file changed, and far more reliably.
           "generator": args.generator, "rootId":project_root_id(entries),
           "artifactTypes":TYPES, "relationRegistry":REL_REGISTRY,
           "policyVersion":POLICY_VERSION, "policySha256":policy_hash(),
           "traversalPolicies":POLICIES,
           "limits":{"indexBytes":INDEX_BYTE_LIMIT,"artifacts":INDEX_ARTIFACT_LIMIT,
                     "relationships":RELATIONSHIP_LIMIT,
                     "spatialNodes":500,"spatialEdges":1000,"visibleLabels":150,
                     "surfaces":SURFACE_LIMIT},
           "artifacts": entries, "surfaces": surfaces}
    out["graphSha256"] = graph_hash(entries, project)
    body = ("// Derived from artifact frontmatter by scripts/docs-graph.py — DO NOT hand-edit"
            " (frontmatter wins; see knowledge-visualization.md V2/V18).\n"
            "window.DOCS_INDEX = " + json.dumps(out, indent=2, ensure_ascii=False) + ";\n")
    body_bytes = len(body.encode("utf-8"))
    if body_bytes > INDEX_BYTE_LIMIT:
        raise DocsGraphError(
            "DERIVE_INDEX_LIMIT_EXCEEDED",
            "The serialized index limit was exceeded.",
            {"maximum": INDEX_BYTE_LIMIT, "observed": body_bytes},
            4,
        )
    dst = args.out or os.path.join(args.root, "docs-index.js")
    # Explicit --root or --out is a direct ask (sandboxed tests and callers that name their
    # own location both count) and bypasses the opt-in check below; only the DEFAULT -- no
    # flag, the documented skill invocation -- is guarded.
    root_explicit = "--root" in sys.argv[1:]
    if not args.out and not root_explicit and not _opted_into_docs_explorer(args.root, dst):
        print("docs-graph derive: skipped writing {0} -- the repo has not opted into the "
              "Docs Explorer (neither it nor {1} exists yet). Pass --out to write it "
              "anyway, or instantiate docs/index.html first "
              "(pack-onoff-analysis.html #3, class PK-03).".format(
                  dst, os.path.join(args.root, "index.html")), file=sys.stderr)
        return 0
    for a in arts:
        current, _ = _read_source_bounded(a["_fs_path"], a["id"])
        if sha256_text(normalized_source(current)) != source_snapshots[a["_fs_path"]]:
            raise _source_changed_error(a["id"])
    destination_directory = os.path.dirname(os.path.abspath(dst))
    os.makedirs(destination_directory, exist_ok=True)
    # No text=True — see _atomic_write_text: the fd carries its own encoding and newline.
    fd, temporary = tempfile.mkstemp(
        prefix=".docs-index-",
        suffix=".tmp",
        dir=destination_directory,
    )
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as index_file:
            index_file.write(body)
            index_file.flush()
            os.fsync(index_file.fileno())
        os.replace(temporary, dst)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    for p in problems: print(f"finding: {p['file']}: {p['problem']}", file=sys.stderr)
    print(f"derived {len(entries)} entries -> {dst}" + (f" ({len(problems)} findings on stderr)" if problems else ""))
    return 0

def cmd_freshness(args):
    arts, problems = scan(args.root)
    _, _, stale, flagged, orphans = analyze(arts, problems)
    for i in stale:   print(f"STALE   (V13): {i}")
    for i in flagged: print(f"FLAGGED (V16): {i}")
    for i in orphans: print(f"ORPHAN  (V10): {i}")
    bad = [p for p in problems if "unreadable" in p["problem"] or "no frontmatter" in p["problem"] or "parse" in p["problem"]]
    for p in bad: print(f"INVALID      : {p['file']}: {p['problem']}")
    n = len(stale) + len(flagged) + len(orphans) + len(bad)
    print(f"freshness: {n} finding(s)")
    return 1 if (n and args.gate == "fail") else 0

# ---- V16 flag mechanics: targeted textual frontmatter edits (preserve the file) ----
def edit_frontmatter_flags(path, mutate):
    text, _ = _read_source_bounded(path, os.path.basename(path))
    m = re.match(r"^(---\r?\n)(.*?)(\r?\n---\r?\n)(.*)$", text, re.S)
    if not m: raise SystemExit(f"{path}: no frontmatter to edit")
    head = m.group(2)
    fm, err = parse_frontmatter(text)
    if err: raise SystemExit(f"{path}: {err}")
    flags = [f for f in (fm.get("review-suggested") or []) if isinstance(f, dict)]
    flags, bump = mutate(flags, fm)
    block = "review-suggested: []" if not flags else \
        "review-suggested:\n" + "\n".join(
            '  - { by: %s, on: %s, reason: "%s" }' % (f.get("by",""), f.get("on",""), str(f.get("reason","")).replace('"', "'"))
            for f in flags)
    if re.search(r"^review-suggested:.*$(?:\n\s+-\s.*$)*", head, re.M):
        head = re.sub(r"^review-suggested:.*$(?:\n\s+-\s.*$)*", block, head, count=1, flags=re.M)
    else:
        head = head.rstrip("\n") + "\n" + block
    if bump:
        new_date = (datetime.date.today() + datetime.timedelta(days=bump)).isoformat()
        if re.search(r"^review-by:.*$", head, re.M):
            head = re.sub(r"^review-by:.*$", f"review-by: {new_date}", head, count=1, flags=re.M)
        else:
            head += f"\nreview-by: {new_date}"
    _atomic_write_text(path, m.group(1) + head + m.group(3) + m.group(4))

def cmd_flag(args):
    arts, problems = scan(args.root)
    by_id, inbound, *_ = analyze(arts, problems)
    if args.changed not in by_id: raise SystemExit(f"unknown artifact id: {args.changed}")
    nbrs = inbound.get(args.changed, [])
    if not nbrs: print(f"no inbound neighbors of {args.changed}; nothing to flag"); return 0
    for n in nbrs:
        a = by_id[n["from"]]
        def add(flags, _fm, _by=args.changed, _r=args.reason):
            if any(f.get("by") == _by for f in flags): return flags, 0
            flags.append({"by": _by, "on": TODAY, "reason": _r}); return flags, 0
        edit_frontmatter_flags(a["_fs_path"], add)
        print(f"flagged {a['id']} ({n['rel']} -> {args.changed})")
    return cmd_derive(args)

def cmd_clear_flag(args):
    arts, problems = scan(args.root)
    by_id, *_ = analyze(arts, problems)
    if args.id not in by_id: raise SystemExit(f"unknown artifact id: {args.id}")
    def rm(flags, _fm):
        kept = [f for f in flags if f.get("by") != args.by]
        if len(kept) == len(flags): print(f"warning: no flag by {args.by} on {args.id}", file=sys.stderr)
        return kept, (args.bump_review or 0)
    edit_frontmatter_flags(by_id[args.id]["_fs_path"], rm)
    print(f"cleared flag by {args.by} on {args.id}" + (f"; review-by +{args.bump_review}d" if args.bump_review else ""))
    return cmd_derive(args)

def cmd_rollup(args):
    """Extract the markdown table under --heading from every matching artifact and merge."""
    arts, problems = scan(args.root)
    if args.type: arts=[a for a in arts if a.get("type")==args.type]
    header, out = None, []
    for a in sorted(arts, key=lambda x: x.get("id","")):
        text, _ = _read_source_bounded(a["_fs_path"], a["id"])
        m = re.search(r"^#{2,3}\s+" + re.escape(args.heading) + r"\s*$([\s\S]*?)(?=^#{1,3}\s|\Z)", text, re.M)
        if not m: continue
        rows = [ln for ln in m.group(1).split("\n") if ln.strip().startswith("|")]
        if len(rows) < 3: continue            # header + separator + at least one data row
        if header is None: header = rows[0], rows[1]
        for r in rows[2:]:
            if r.strip().strip("|").strip(): out.append("| [" + a.get("id","?") + "](" +
                os.path.relpath(a["_fs_path"], os.path.abspath(args.root)).replace(os.sep,"/") + ") " + r.strip())
    if header is None:
        print(f"no '{args.heading}' tables found", file=sys.stderr); return 1
    lines = [
        "| source " + header[0].strip(),
        "|---" + header[1].strip(),
        *out,
        "",
        f"<!-- rolled up from {len(set(r.split(']')[0] for r in out))} artifact(s) by docs-graph.py rollup on {TODAY} -->",
    ]
    payload = ("\n".join(lines) + "\n").encode("utf-8")
    stream = getattr(sys.stdout, "buffer", None)
    if stream is not None:
        stream.write(payload)
    else:
        sys.stdout.write(payload.decode("utf-8"))
    return 0

def cmd_snapshot(args):
    arts, problems = scan(args.root)
    _, _, stale, flagged, orphans = analyze(arts, problems)
    rec = {"on": TODAY, "artifacts": len(arts), "by_type": count_by(arts, "type"),
           "orphans": len(orphans), "stale": len(stale), "flagged": len(flagged),
           "notes": count_by(arts, "type").get("decision-note", 0), "problems": len(problems)}
    dst = os.path.join(args.root, "health-history.jsonl")
    _atomic_append_text(dst, json.dumps(rec) + "\n")
    print(json.dumps(rec)); print(f"appended -> {dst}")
    return 0

def cmd_stub(args):
    parsed_links = []
    for link in args.link or []:
        if ":" not in link:
            raise SystemExit(f"--link must be to:rel: {link!r}")
        target, relation = link.split(":", 1)
        if not target or not relation:
            raise SystemExit(f"--link must be to:rel: {link!r}")
        parsed_links.append((target, relation))
    links = "\n".join(
        "  - { to: %s, rel: %s }" % link for link in parsed_links
    ) or "  []"
    links_block = "links:\n" + links if links.strip() != "[]" else "links: []"
    fm = f"""---
id: {args.id}
title: "{args.title}"
type: {args.type}
status: draft
owner: "{args.owner}"
phase: "{args.phase or ''}"
tags: [{', '.join(args.tag or [])}]
{links_block}
review-by: {args.review_by or ''}
review-suggested: []
summary: >-
  {args.summary or 'TODO — real 1-3 sentence summary (V2).'}
---

# {args.title}
"""
    if os.path.exists(args.file) and not args.force: raise SystemExit(f"{args.file} exists (use --force)")
    _atomic_write_text(args.file, fm)
    print(f"stubbed {args.file}")
    return 0


def context_graph(arts, problems):
    by_id, _, _, _, _ = analyze(arts, problems)
    if problems:
        raise DocsGraphError("GRAPH_INVALID","The documentation graph is invalid.",
                             {"issueCount":len(problems)},3)
    seen, relationships = set(), 0
    for a in arts:
        for link in a["links"]:
            relationships += 1
            key = (a["id"],link["rel"],link["to"])
            if key in seen:
                raise DocsGraphError("GRAPH_INVALID",
                    "The documentation graph contains an exact duplicate typed link.",
                    {"source":key[0],"rel":key[1],"target":key[2]},3)
            seen.add(key)
    if len(arts) > CONTEXT_LIMITS["artifacts"]:
        raise DocsGraphError("SCAN_LIMIT_EXCEEDED","The artifact scan limit was exceeded.",
                             {"limit":"artifacts","maximum":CONTEXT_LIMITS["artifacts"]},4)
    if relationships > CONTEXT_LIMITS["relationships"]:
        raise DocsGraphError("SCAN_LIMIT_EXCEEDED","The relationship scan limit was exceeded.",
                             {"limit":"relationships","maximum":CONTEXT_LIMITS["relationships"]},4)
    return by_id, relationships


def traverse_context(by_id, root_id, policy_name, hops):
    inbound = {}
    for a in by_id.values():
        for link in a["links"]:
            inbound.setdefault(link["to"],[]).append({"source":a["id"],**link})
    depth, priority, paths, queue = {root_id:0}, {root_id:-1}, [], [root_id]
    while queue:
        current = queue.pop(0)
        if depth[current] >= hops: continue
        candidates = []
        for rule in POLICIES[policy_name]:
            if rule["direction"] == "outbound":
                links = [{"source":current,**link} for link in by_id[current]["links"]]
                target_key = "to"
            else:
                links = inbound.get(current,[])
                target_key = "source"
            for link in links:
                if link["rel"] != rule["rel"]: continue
                target = link[target_key]
                if target not in by_id: continue
                candidates.append({"from":current,"to":target,"rel":link["rel"],
                                   "direction":rule["direction"],"priority":rule["priority"]})
        candidates.sort(key=lambda item:(item["priority"],item["to"],item["rel"]))
        for item in candidates:
            if item["to"] in depth: continue
            depth[item["to"]] = depth[current] + 1
            priority[item["to"]] = item["priority"]
            path = {k:item[k] for k in ("from","to","rel","direction")}
            path["depth"] = depth[item["to"]]
            paths.append(path)
            queue.append(item["to"])
    ordered = sorted(depth, key=lambda i:(depth[i],priority[i],i))
    return ordered, depth, priority, paths


def _utf8_split_end(data, start, maximum):
    end = min(len(data), start + maximum)
    while end > start:
        try:
            data[start:end].decode("utf-8")
            return end
        except UnicodeDecodeError:
            end -= 1
    return min(len(data), start + maximum)


def _section_ranges(data, start, end, maximum):
    ranges = []
    cursor = start
    while cursor < end:
        candidate = min(end, cursor + maximum)
        if candidate < end:
            paragraph = data.rfind(b"\n\n", cursor, candidate)
            line = data.rfind(b"\n", cursor, candidate)
            split = paragraph + 2 if paragraph >= cursor else line + 1
            candidate = split if split > cursor else _utf8_split_end(data, cursor, maximum)
        ranges.append((cursor, candidate))
        cursor = candidate
    return ranges


def markdown_chunks(artifact, source, depth, relation_priority, query_terms):
    lines = source.splitlines(keepends=True)
    body_start = 0
    if lines and lines[0].strip() == "---":
        for index in range(1, len(lines)):
            if lines[index].strip() == "---":
                body_start = index + 1
                break
    line_offsets, offset = [], 0
    for line in lines:
        line_offsets.append(offset)
        offset += len(line.encode("utf-8"))
    source_bytes = source.encode("utf-8")
    starts = [
        i for i,line in enumerate(lines)
        if i >= body_start and re.match(r"^\s{0,3}#{1,6}\s+",line)
    ]
    if not starts:
        return []
    boundaries = starts + [len(lines)]
    chunks = []
    source_hash = sha256_text(source)
    for pos in range(len(boundaries)-1):
        start, end = boundaries[pos], boundaries[pos+1]
        if start == end: continue
        heading_match = re.match(r"^\s{0,3}(#{1,6}\s+.+?)\s*$",lines[start].rstrip("\n"))
        heading = heading_match.group(1)[:512]
        start_byte = line_offsets[start]
        end_byte = line_offsets[end] if end < len(lines) else len(source_bytes)
        for range_start, range_end in _section_ranges(
            source_bytes, start_byte, end_byte, CONTEXT_LIMITS["chunkBytes"]
        ):
            raw_text = source_bytes[range_start:range_end].decode("utf-8")
            text = raw_text.rstrip("\n")
            if not text.strip(): continue
            text_bytes = text.encode("utf-8")
            line_start = source_bytes[:range_start].count(b"\n") + 1
            line_end = line_start + text.count("\n")
            score_text = f"{artifact.get('title','')} {heading} {text}".lower()
            term_score = sum(score_text.count(term) for term in query_terms)
            reasons = ["root" if depth == 0 else "root-neighbor"]
            if term_score: reasons.append("query-match")
            chunks.append({
                "artifactId":artifact["id"],"path":artifact["_path"],"heading":heading,
                "lineStart":line_start,"lineEnd":line_end,
                "sourceSha256":source_hash,"sourceByteStart":range_start,
                "sourceByteEnd":range_start+len(text_bytes),
                "sha256":sha256_text(text),"reason":reasons,"text":text,
                "_rank":(depth,-term_score,relation_priority,artifact["id"],line_start,range_start),
            })
    return chunks


def context_health(arts):
    issues = []
    for a in arts:
        if a.get("review-by") and str(a["review-by"]) < TODAY and a.get("status") not in {"resolved","superseded"}:
            issues.append({"code":"STALE","artifactId":a["id"]})
        if a.get("review-suggested"):
            issues.append({"code":"REVIEW_SUGGESTED","artifactId":a["id"]})
    return sorted(issues,key=lambda issue:(issue["code"],issue["artifactId"]))


def active_changes(root, paths):
    log_path = os.path.join(root,"audit","change-log.jsonl")
    if not os.path.exists(log_path): return []
    entries, superseded = [], set()
    with open(log_path,encoding="utf-8") as change_log:
        for line in change_log:
            try: entry = json.loads(line)
            except json.JSONDecodeError: continue
            if entry.get("supersedes"): superseded.add(entry["supersedes"])
            entries.append(entry)
    result = []
    for entry in entries:
        if entry.get("id") in superseded or entry.get("status") == "superseded": continue
        artifacts = sorted(set(entry.get("artifacts",[])))
        if not set(artifacts) & set(paths): continue
        result.append({"id":entry.get("id",""),"datetime":entry.get("datetime",""),
                       "summary":entry.get("summary",""),"artifacts":artifacts})
    return sorted(result,key=lambda entry:(entry["datetime"],entry["id"]))


def packet_bytes(packet):
    previous, current = -1, 0
    while previous != current:
        previous = current
        packet["budget"]["bytesUsed"] = current
        current = len(canonical_json(packet).encode("utf-8"))
    packet["budget"]["bytesUsed"] = current
    return len(canonical_json(packet).encode("utf-8"))


def cmd_context(args, timings=None):
    total_started = time.perf_counter()
    if args.max_bytes < PACKET_MIN_BYTES or args.max_bytes > PACKET_MAX_BYTES:
        raise DocsGraphError("BUDGET_OUT_OF_RANGE",
            f"Packet budget must be between {PACKET_MIN_BYTES} and {PACKET_MAX_BYTES} bytes.",
            {"maxBytes":args.max_bytes},2)
    if args.policy not in ("grounding","impact"):
        raise DocsGraphError("POLICY_UNSUPPORTED",f"Traversal policy '{args.policy}' is not supported.",
                             {"policy":args.policy},2)
    if args.hops < 0 or args.hops > 2:
        raise DocsGraphError("HOPS_OUT_OF_RANGE","Traversal hops must be between 0 and 2.",
                             {"hops":args.hops},2)
    phase_started = time.perf_counter()
    arts, problems, source_hashes, admitted = scan_context(args.root)
    if timings is not None:
        timings["scan"] = (time.perf_counter() - phase_started) * 1000
    phase_started = time.perf_counter()
    by_id, relationships = context_graph(arts,problems)
    if args.id not in by_id:
        raise DocsGraphError("ROOT_NOT_FOUND",f"Artifact '{args.id}' was not found.",
                             {"artifactId":args.id},2)
    traversed, depths, priorities, paths = traverse_context(by_id,args.id,args.policy,args.hops)
    if timings is not None:
        timings["traverse"] = (time.perf_counter() - phase_started) * 1000
    phase_started = time.perf_counter()
    terms = sorted(set(re.findall(r"[a-z0-9]+",(args.query or "").lower())))
    chunks, chunk_limit_exceeded = [], False
    for artifact_id in traversed:
        text = read_context_source(by_id[artifact_id], source_hashes[artifact_id])
        for chunk in markdown_chunks(
            by_id[artifact_id],text,depths[artifact_id],priorities[artifact_id],terms
        ):
            chunks.append(chunk)
            if len(chunks) > CONTEXT_LIMITS["generatedChunks"]:
                chunk_limit_exceeded = True
                break
        if chunk_limit_exceeded:
            break
    chunks.sort(key=lambda chunk:chunk.pop("_rank"))
    if timings is not None:
        timings["chunk"] = (time.perf_counter() - phase_started) * 1000
    phase_started = time.perf_counter()
    entries = []
    for a in sorted(arts,key=lambda item:item["id"]):
        entries.append({"id":a["id"],"path":a["_path"],"title":a.get("title",""),
                        "type":a.get("type",""),"status":a.get("status",""),
                        "owner":a.get("owner",""),"reviewBy":str(a.get("review-by","")),
                        "reviewSuggested":a.get("review-suggested") or [],
                        "tags":a.get("tags") or [],"links":a["links"],
                        "summary":a.get("summary",""),"sourceSha256":source_hashes[a["id"]]})
    traversed_paths = [by_id[artifact_id]["_path"] for artifact_id in traversed]
    project = project_identity(args.root, getattr(args, "project", None))
    packet = {
        "schemaVersion":"grounding-packet/v1","policyVersion":POLICY_VERSION,
        "policySha256":policy_hash(),
        "graphSha256":graph_hash(entries,project),
        "rootId":args.id,
        "request":{"policy":args.policy,"hops":args.hops,"query":args.query or "",
                   "maxBytes":args.max_bytes,"includeChanges":bool(args.include_changes)},
        "healthIssues":context_health(arts),
        "coverage":{"roots":[project],
                    "artifactsScanned":len(arts),"relationshipsScanned":relationships,
                    "admittedSourceBytes":admitted},
        "paths":paths,"chunks":[],
        "changes":active_changes(args.root,traversed_paths) if args.include_changes else [],
        "budget":{"bytesUsed":0,"chunksIncluded":0,"truncated":False,
                  "omittedChunkCount":len(chunks)},
    }
    envelope = packet_bytes(packet)
    if envelope > PACKET_MAX_BYTES:
        raise DocsGraphError("ENVELOPE_TOO_LARGE",
            "The mandatory packet envelope exceeds the supported maximum.",{"bytes":envelope},4)
    if envelope > args.max_bytes:
        raise DocsGraphError("BUDGET_TOO_SMALL",
            "The requested budget cannot contain the mandatory packet envelope.",
            {"bytes":envelope,"maxBytes":args.max_bytes},4)
    if chunk_limit_exceeded:
        raise DocsGraphError("SCAN_LIMIT_EXCEEDED","The generated chunk limit was exceeded.",
                             {"limit":"generatedChunks","maximum":CONTEXT_LIMITS["generatedChunks"]},4)
    low, high = 0, len(chunks)
    while low < high:
        candidate = (low + high + 1) // 2
        packet["chunks"] = chunks[:candidate]
        packet["budget"]["chunksIncluded"] = candidate
        packet["budget"]["omittedChunkCount"] = len(chunks)-candidate
        packet["budget"]["truncated"] = candidate < len(chunks)
        if packet_bytes(packet) <= args.max_bytes:
            low = candidate
        else:
            high = candidate - 1
    packet["chunks"] = chunks[:low]
    packet["budget"]["chunksIncluded"] = low
    packet["budget"]["omittedChunkCount"] = len(chunks)-low
    packet["budget"]["truncated"] = low < len(chunks)
    packet_bytes(packet)
    if timings is not None:
        timings["serialize"] = (time.perf_counter() - phase_started) * 1000
        timings["total"] = (time.perf_counter() - total_started) * 1000
    return packet


def write_context_error(error):
    details = {k:v for k,v in error.details.items()
               if isinstance(v,(str,int,float,bool)) or v is None}
    serialized = canonical_json({"schemaVersion":"docs-graph-error/v1",
                                 "error":{"code":error.code,"message":error.message,"details":details}})
    if len(serialized.encode("utf-8")) > CONTEXT_LIMITS["stderrBytes"]:
        serialized = canonical_json({"schemaVersion":"docs-graph-error/v1",
                                     "error":{"code":error.code,"message":error.message,"details":{}}})
    sys.stderr.buffer.write(serialized.encode("utf-8"))

def main():
    ap = argparse.ArgumentParser(prog="docs-graph.py", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--root", default="docs", help="docs root (default: docs)")
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("inventory")
    d = sub.add_parser("derive"); va = sub.add_parser("validate"); fr = sub.add_parser("freshness")
    d.add_argument("--out", default=None); d.add_argument("--project", default=None)
    d.add_argument("--generator", default="docs-graph.py derive")
    # FR-056: defects always fail; suggestions (V16 flags, V13 staleness) warn by default.
    va.add_argument("--gate", choices=["warn","fail"], default="warn",
                    help="fail = treat review-suggested/stale as errors too (default: warn)")
    fr.add_argument("--gate", choices=["warn","fail"], default="warn")
    fl = sub.add_parser("flag"); fl.add_argument("--changed", required=True); fl.add_argument("--reason", required=True)
    fl.add_argument("--out", default=None); fl.add_argument("--project", default=None); fl.add_argument("--generator", default="docs-graph.py flag")
    cf = sub.add_parser("clear-flag"); cf.add_argument("--id", required=True); cf.add_argument("--by", required=True)
    cf.add_argument("--bump-review", type=int, default=0)
    cf.add_argument("--out", default=None); cf.add_argument("--project", default=None); cf.add_argument("--generator", default="docs-graph.py clear-flag")
    st = sub.add_parser("stub")
    st.add_argument("--file", required=True); st.add_argument("--id", required=True); st.add_argument("--title", required=True)
    st.add_argument("--type", required=True, choices=TYPES); st.add_argument("--owner", default="@owner")
    st.add_argument("--phase", default=""); st.add_argument("--tag", action="append"); st.add_argument("--link", action="append",
        help="to:rel (repeatable), e.g. --link spec-checkout:implements")
    st.add_argument("--review-by", default=""); st.add_argument("--summary", default=""); st.add_argument("--force", action="store_true")
    sub.add_parser("snapshot")
    ru = sub.add_parser("rollup"); ru.add_argument("--heading", required=True)
    ru.add_argument("--type", default=None, help="restrict to one artifact type (e.g. design)")
    cx = sub.add_parser("context")
    cx.add_argument("--id", required=True)
    cx.add_argument("--query", default="")
    cx.add_argument("--policy", default="grounding")
    cx.add_argument("--hops", type=int, default=2)
    cx.add_argument("--max-bytes", type=int, default=65536)
    cx.add_argument("--include-changes", action="store_true")
    cx.add_argument("--project", default=None)
    cx.add_argument(
        "--timings",
        action="store_true",
        help="emit phase timings as structured JSON on stderr after a successful packet",
    )
    args = ap.parse_args()
    if args.cmd == "context":
        try:
            timings = {} if args.timings else None
            sys.stdout.buffer.write(
                canonical_json(cmd_context(args, timings=timings)).encode("utf-8")
            )
            if timings is not None:
                diagnostics = {
                    "schemaVersion": "docs-context-timings/v1",
                    "phasesMilliseconds": {
                        phase: round(milliseconds, 3)
                        for phase, milliseconds in timings.items()
                    },
                }
                sys.stderr.buffer.write(canonical_json(diagnostics).encode("utf-8"))
            return 0
        except DocsGraphError as error:
            write_context_error(error)
            return error.exit_code
        except Exception as unexpected:
            error = DocsGraphError(
                "INTERNAL_ERROR",
                "An unexpected internal error occurred.",
                {"exceptionType": type(unexpected).__name__},
                1,
            )
            write_context_error(error)
            return error.exit_code
    if args.cmd == "inventory":  sys.exit(cmd_inventory(args))
    if args.cmd == "validate":   sys.exit(cmd_inventory(args, exit_on_findings=True))
    if args.cmd == "derive":     sys.exit(cmd_derive(args))
    if args.cmd == "freshness":  sys.exit(cmd_freshness(args))
    if args.cmd == "flag":       sys.exit(cmd_flag(args))
    if args.cmd == "clear-flag": sys.exit(cmd_clear_flag(args))
    if args.cmd == "stub":       sys.exit(cmd_stub(args))
    if args.cmd == "snapshot":   sys.exit(cmd_snapshot(args))
    if args.cmd == "rollup":     sys.exit(cmd_rollup(args))

if __name__ == "__main__":
    raise SystemExit(main() or 0)
