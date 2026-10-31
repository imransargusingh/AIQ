#!/usr/bin/env python3
"""Bounded subprocess execution for pack-owned tool invocations."""

import os
import signal
import subprocess
import threading
import time
import ctypes
import sys
from pathlib import Path


HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))
from platform_process import (  # noqa: E402
    WindowsJob as _WindowsJob,
    release_windows_gate as _release_windows_gate,
    spawn_windows_gate,
    terminate_owned_process as _terminate_tree,
    wait_after_termination as _wait_after_termination,
)


_WINDOWS_GATE_WRAPPER = (
    "import subprocess,sys;"
    "gate=sys.stdin.buffer.read(1);"
    "result=subprocess.run(sys.argv[1:]) if gate==b'1' else None;"
    "raise SystemExit(result.returncode if result is not None else 125)"
)
_POSIX_LIMIT_WRAPPER = (
    "import os,resource,sys\n"
    "memory=int(sys.argv[1])\n"
    "try:\n"
    " resource.setrlimit(resource.RLIMIT_AS,(memory,memory))\n"
    "except (ValueError,OSError):\n"
    " pass\n"
    "os.execvpe(sys.argv[2],sys.argv[2:],os.environ)\n"
)


class ProcessResult:
    def __init__(
        self,
        returncode,
        stdout,
        stderr,
        timed_out=False,
        limit_exceeded=None,
        contained=True,
        containment_error=None,
        cleanup_error=None,
        containment_mode=None,
        process_limit_enforced=False,
        aggregate_memory_limit_enforced=False,
        cancelled=False,
    ):
        self.cancelled = cancelled
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr
        self.timed_out = timed_out
        self.limit_exceeded = limit_exceeded
        self.contained = contained
        self.containment_error = containment_error
        self.cleanup_error = cleanup_error
        self.containment_mode = containment_mode
        self.process_limit_enforced = process_limit_enforced
        self.aggregate_memory_limit_enforced = aggregate_memory_limit_enforced


def _merge_errors(*errors):
    unique = []
    for error in errors:
        if error and error not in unique:
            unique.append(error)
    return "; ".join(unique) if unique else None


def _read_chunk(stream, size):
    """Read one bounded chunk from buffered or raw subprocess pipes."""
    reader = getattr(stream, "read1", None)
    if callable(reader):
        return reader(size)
    return stream.read(size)

def run_bounded(
    command,
    cwd=None,
    env=None,
    timeout_seconds=10,
    stdout_limit=1024 * 1024,
    stderr_limit=64 * 1024,
    memory_limit=512 * 1024 * 1024,
    process_limit=64,
    cancelled=None,
):
    """Run one process with concurrent draining, hard output caps, and tree cleanup.

    On Windows the timeout includes the contained gate wrapper's process-start cost.
    Callers running substantive commands should set an explicit workload budget.
    """
    creationflags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    containment_mode = "windows-job" if os.name == "nt" else "posix-process-group-rlimit-as"
    launch_command = command
    stdin = subprocess.DEVNULL
    if os.name == "nt":
        # The gated wrapper cannot launch the requested command until Job Object
        # containment succeeds, closing the child-start-before-assignment race.
        stdin = subprocess.PIPE
    else:
        launch_command = [
            sys.executable,
            "-c",
            _POSIX_LIMIT_WRAPPER,
            str(memory_limit),
            *command,
        ]
    process = (
        spawn_windows_gate(command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                           closed_stdin=True)
        if os.name == "nt"
        else subprocess.Popen(
            launch_command,
            cwd=cwd,
            env=env,
            stdin=stdin,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            creationflags=creationflags,
            start_new_session=True,
        )
    )
    windows_job = _WindowsJob(process, memory_limit, process_limit)
    if os.name == "nt" and not windows_job.handle:
        cleanup_error = _terminate_tree(process, windows_job)
        returncode, termination_error = _wait_after_termination(process, windows_job)
        process.stdin.close()
        process.stdout.close()
        process.stderr.close()
        detail = windows_job.error or "Windows Job Object containment is unavailable"
        if cleanup_error:
            detail = f"{detail}; {cleanup_error}"
        if termination_error:
            detail = f"{detail}; {termination_error}"
        return ProcessResult(
            returncode,
            "",
            detail,
            contained=False,
            containment_error=detail,
            cleanup_error=cleanup_error or termination_error,
            containment_mode=containment_mode,
        )
    if os.name == "nt":
        gate_error = _release_windows_gate(process, close_after=True)
        if gate_error:
            cleanup_error = _terminate_tree(process, windows_job)
            returncode, termination_error = _wait_after_termination(process, windows_job)
            process.stdout.close()
            process.stderr.close()
            process.stdin.close()
            windows_job.close()
            detail = "; ".join(
                item for item in [gate_error, cleanup_error, termination_error] if item
            )
            return ProcessResult(
                returncode,
                "",
                detail,
                contained=False,
                containment_error=detail,
                cleanup_error=cleanup_error or termination_error,
                containment_mode=containment_mode,
            )
    stdout_parts, stderr_parts = [], []
    cleanup_errors = []
    exceeded_names = []
    state_lock = threading.Lock()
    termination_lock = threading.Lock()
    termination_attempted = False
    exceeded = threading.Event()
    stop = threading.Event()

    def terminate_once():
        nonlocal termination_attempted
        with termination_lock:
            if termination_attempted:
                return None
            termination_attempted = True
            error = _terminate_tree(process, windows_job)
            if error:
                cleanup_errors.append(error)
            return error

    def drain(stream, parts, limit, name):
        total = 0
        try:
            while not stop.is_set():
                remaining = max(1, limit - total + 1)
                chunk = _read_chunk(stream, min(65536, remaining))
                if not chunk:
                    return
                total += len(chunk)
                if total > limit:
                    with state_lock:
                        if not exceeded.is_set():
                            exceeded_names.append(name)
                            terminate_once()
                            exceeded.set()
                            stop.set()
                    return
                parts.append(chunk)
        except (OSError, ValueError):
            return

    readers = [
        threading.Thread(
            target=drain,
            args=(process.stdout, stdout_parts, stdout_limit, "stdout"),
            daemon=True,
        ),
        threading.Thread(
            target=drain,
            args=(process.stderr, stderr_parts, stderr_limit, "stderr"),
            daemon=True,
        ),
    ]
    for reader in readers:
        reader.start()

    deadline = time.monotonic() + timeout_seconds
    timed_out = False
    was_cancelled = False
    cleanup_error = None
    while not stop.is_set():
        if cancelled is not None:
            try:
                was_cancelled = bool(cancelled())
            except Exception:
                was_cancelled = True
            if was_cancelled:
                stop.set()
                terminate_once()
                break
        process_running = process.poll() is None
        readers_running = any(reader.is_alive() for reader in readers)
        if not process_running and not readers_running:
            break
        if time.monotonic() >= deadline:
            timed_out = True
            stop.set()
            terminate_once()
            break
        time.sleep(0.01)

    returncode, termination_error = _wait_after_termination(
        process, windows_job, terminate_once
    )
    for reader in readers:
        reader.join(timeout=2)
    try:
        process.stdout.close()
        process.stderr.close()
    finally:
        windows_job.close()

    stdout = b"".join(stdout_parts).decode("utf-8", errors="replace")
    stderr = b"".join(stderr_parts).decode("utf-8", errors="replace")
    cleanup_error = _merge_errors(cleanup_error, termination_error)
    if cleanup_errors:
        cleanup_error = _merge_errors(cleanup_error, *cleanup_errors)
    if cleanup_error:
        stderr = f"{stderr}\nPROCESS_CLEANUP_FAILED: {cleanup_error}".strip()
    limit_exceeded = exceeded_names[0] if exceeded_names else None
    # Failed commands may emit untrusted partial data; callers consume stderr diagnostics only.
    if was_cancelled or timed_out or limit_exceeded or returncode != 0:
        stdout = ""
    return ProcessResult(
        returncode,
        stdout,
        stderr,
        timed_out,
        limit_exceeded,
        contained=cleanup_error is None,
        containment_error=cleanup_error,
        cleanup_error=cleanup_error,
        containment_mode=containment_mode,
        process_limit_enforced=os.name == "nt",
        aggregate_memory_limit_enforced=os.name == "nt",
        cancelled=was_cancelled,
    )
