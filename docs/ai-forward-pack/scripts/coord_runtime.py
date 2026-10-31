"""Private, bounded, append-only runtime controls for an already admitted run.

One record is one input or decision fact. Filesystem ownership is the authority
boundary; this does not isolate mutually hostile programs running as the same user.
"""
import contextlib
import errno
import hashlib
import itertools
import json
import os
from pathlib import Path
import stat
import tempfile
import time

MAX_RECORDS = 128
MAX_RECORD_BYTES = 256 * 1024
LOCK_ATTEMPT_SECONDS = .05
OPERATION_ATTEMPT_SECONDS = .35
PUBLICATION_ATTEMPT_SECONDS = 2.0


class _RuntimeControlBusy(ValueError):
    pass


def encoded(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, allow_nan=False,
                      separators=(",", ":")).encode("utf-8")


def digest(value):
    return hashlib.sha256(encoded(value)).hexdigest()


def require(condition):
    if not condition:
        raise ValueError("invalid_runtime_control")


def _busy():
    raise _RuntimeControlBusy("runtime_control_busy")


class Controls:
    """Pattern: serialized append-only mailbox; state is derived, never overwritten."""

    def __init__(self, directory):
        self.directory = Path(directory)

    @contextlib.contextmanager
    def locked(self):
        if os.name == "nt":
            import msvcrt
            from coord_files import open_regular, pinned_directory, protect_private_directory
            self.directory.mkdir(parents=True, exist_ok=True)
            with pinned_directory(self.directory):
                protect_private_directory(self.directory)
                with open_regular(self.directory / ".lock", writable=True, create=True) as stream:
                    deadline = time.monotonic() + LOCK_ATTEMPT_SECONDS
                    acquired = False
                    try:
                        while True:
                            try:
                                stream.seek(0)
                                msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
                                acquired = True
                                break
                            except OSError as exc:
                                if exc.errno not in (errno.EACCES, errno.EAGAIN, errno.EDEADLK):
                                    raise
                                if time.monotonic() >= deadline:
                                    _busy()
                                time.sleep(.005)
                        yield
                    finally:
                        if acquired:
                            stream.seek(0)
                            msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
            return
        import fcntl
        require(str(self.directory.absolute()) == str(self.directory.resolve()))
        self.directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        info = self.directory.lstat()
        require(stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid()
                and info.st_mode & 0o077 == 0)
        fd = os.open(self.directory / ".lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        try:
            require(stat.S_ISREG(os.fstat(fd).st_mode))
            deadline = time.monotonic() + LOCK_ATTEMPT_SECONDS
            while True:
                try:
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    if time.monotonic() >= deadline:
                        _busy()
                    time.sleep(.005)
            yield
        finally:
            os.close(fd)

    def _retry_busy(self, action, attempt_seconds=OPERATION_ATTEMPT_SECONDS):
        deadline = time.monotonic() + attempt_seconds
        while True:
            try:
                return action()
            except _RuntimeControlBusy:
                if time.monotonic() >= deadline:
                    raise
                time.sleep(.01)

    def _records(self):
        paths = sorted(itertools.islice(self.directory.glob("*.json"), MAX_RECORDS + 1))
        require(len(paths) <= MAX_RECORDS)
        rows = []
        previous = ""
        for sequence, path in enumerate(paths, 1):
            require(path.name == f"{sequence:06d}.json")
            if os.name == "nt":
                from coord_files import read_regular
                row = json.loads(read_regular(path, MAX_RECORD_BYTES))
            else:
                fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                with os.fdopen(fd, "rb") as stream:
                    info = os.fstat(stream.fileno())
                    require(stat.S_ISREG(info.st_mode) and info.st_size <= MAX_RECORD_BYTES
                            and info.st_uid == os.getuid() and info.st_mode & 0o077 == 0)
                    row = json.loads(stream.read(MAX_RECORD_BYTES + 1))
            require(isinstance(row, dict) and row.get("sequence") == sequence
                    and row.get("id") == f"{sequence:06d}" and row.get("previous") == previous)
            checksum = row.get("sha256")
            require(checksum == digest({k: v for k, v in row.items() if k != "sha256"}))
            require(row.get("kind") in ("prompt", "dispatch", "finish", "permission", "decision"))
            previous = checksum
            rows.append(row)
        return rows

    def records(self):
        def action():
            with self.locked():
                return self._records()
        return self._retry_busy(action)

    def _append(self, rows, kind, **fields):
        require(len(rows) < MAX_RECORDS)
        sequence = len(rows) + 1
        row = dict(fields, id=f"{sequence:06d}", sequence=sequence, kind=kind,
                   at=time.time(), previous=rows[-1]["sha256"] if rows else "")
        row["sha256"] = digest(row)
        data = encoded(row)
        require(len(data) <= MAX_RECORD_BYTES)
        fd, name = tempfile.mkstemp(prefix=".publishing-", dir=self.directory)
        try:
            with os.fdopen(fd, "wb") as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
            # Atomic no-replace publication; a half-written record is never visible.
            os.link(name, self.directory / (row["id"] + ".json"))
            if os.name != "nt":
                directory_fd = os.open(self.directory, os.O_RDONLY)
                try:
                    os.fsync(directory_fd)
                finally:
                    os.close(directory_fd)
        finally:
            os.unlink(name)
        return row

    def enqueue(self, compilation_id, prompt_sha256, capacity):
        require(isinstance(compilation_id, str) and 0 < len(compilation_id) <= 256
                and isinstance(prompt_sha256, str) and 0 < len(prompt_sha256) <= 64
                and type(capacity) is int and 0 <= capacity <= 8)
        def action():
            with self.locked():
                rows = self._records()
                prompts = [r for r in rows if r["kind"] == "prompt"]
                require(len(prompts) < capacity and not any(r["kind"] == "finish" for r in rows)
                        and not any(r["compilation_id"] == compilation_id for r in prompts))
                return self._append(rows, "prompt", compilation_id=compilation_id,
                                    prompt_sha256=prompt_sha256)
        return self._retry_busy(action, PUBLICATION_ATTEMPT_SECONDS)

    def finish(self):
        def action():
            with self.locked():
                rows = self._records()
                require(not any(r["kind"] == "finish" for r in rows))
                return self._append(rows, "finish")
        return self._retry_busy(action, PUBLICATION_ATTEMPT_SECONDS)

    def next_prompt(self):
        rows = self.records()
        sent = {r["prompt_id"] for r in rows if r["kind"] == "dispatch"}
        pending = [r for r in rows if r["kind"] == "prompt" and r["id"] not in sent]
        if pending:
            return pending[0]
        return False if any(r["kind"] == "finish" for r in rows) else None

    def dispatched(self, prompt_id):
        def action():
            with self.locked():
                rows = self._records()
                sent = {r["prompt_id"] for r in rows if r["kind"] == "dispatch"}
                pending = [r for r in rows if r["kind"] == "prompt" and r["id"] not in sent]
                require(bool(pending) and pending[0]["id"] == prompt_id)
                return self._append(rows, "dispatch", prompt_id=prompt_id)
        return self._retry_busy(action, PUBLICATION_ATTEMPT_SECONDS)

    def permission(self, request, expires_at):
        require(isinstance(request, dict) and isinstance(request.get("sessionId"), str)
                and request["sessionId"] and "requestId" in request
                and isinstance(request.get("toolCall"), dict)
                and isinstance(request.get("options"), list) and 1 <= len(request["options"]) <= 32
                and type(expires_at) in (int, float) and 0 < expires_at < float("inf"))
        ids = set()
        for option in request["options"]:
            require(isinstance(option, dict) and isinstance(option.get("optionId"), str)
                    and 0 < len(option["optionId"]) <= 256 and option["optionId"] not in ids
                    and option.get("kind") in ("allow_once", "allow_always", "reject_once", "reject_always"))
            ids.add(option["optionId"])
        def action():
            with self.locked():
                return self._append(self._records(), "permission", request=request, expires_at=expires_at)
        return self._retry_busy(action, PUBLICATION_ATTEMPT_SECONDS)

    def decide(self, request_id, option_id, now=None):
        def action():
            with self.locked():
                rows = self._records()
                effective_now = time.time() if now is None else now
                matches = [r for r in rows if r["kind"] == "permission" and r["id"] == request_id]
                require(len(matches) == 1)
                request = matches[0]
                require(effective_now < request["expires_at"] and not any(
                    r["kind"] == "decision" and r["request_id"] == request_id for r in rows))
                options = [o for o in request["request"]["options"] if o["optionId"] == option_id]
                require(len(options) == 1 and options[0]["kind"] in ("allow_once", "reject_once", "reject_always"))
                return self._append(rows, "decision", request_id=request_id,
                                    request_sha256=request["sha256"], option_id=option_id)
        return self._retry_busy(action, PUBLICATION_ATTEMPT_SECONDS)

    def answer(self, request_id, now=None):
        rows = self.records()
        effective_now = time.time() if now is None else now
        requests = [r for r in rows if r["kind"] == "permission" and r["id"] == request_id]
        require(len(requests) == 1)
        request = requests[0]
        require(effective_now < request["expires_at"])
        decisions = [r for r in rows if r["kind"] == "decision" and r["request_id"] == request_id]
        require(len(decisions) <= 1)
        if not decisions:
            return None
        decision = decisions[0]
        require(decision["request_sha256"] == request["sha256"])
        choices = [o for o in request["request"]["options"] if o["optionId"] == decision["option_id"]]
        require(len(choices) == 1 and choices[0]["kind"] in ("allow_once", "reject_once", "reject_always"))
        return decision["option_id"]

    def pending(self, now=None):
        now = time.time() if now is None else now
        rows = self.records()
        answered = {r["request_id"] for r in rows if r["kind"] == "decision"}
        return [dict(r, state="pending" if now < r["expires_at"] else "expired")
                for r in rows if r["kind"] == "permission" and r["id"] not in answered]
