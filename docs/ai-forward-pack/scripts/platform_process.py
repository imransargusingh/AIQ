#!/usr/bin/env python3
"""Cross-platform owned-process helpers for bounded local subprocesses."""

from __future__ import annotations

import ctypes
import os
import signal
import subprocess
import sys
from ctypes import wintypes


_WINDOWS_GATE_WRAPPER = (
    "import os,subprocess,sys;"
    "gate=os.read(0,1);"
    "result=subprocess.run(sys.argv[1:]) if gate==b'1' else None;"
    "raise SystemExit(result.returncode if result is not None else 125)"
)
_WINDOWS_GATE_CLOSED_STDIN_WRAPPER = (
    "import os,subprocess,sys;"
    "gate=os.read(0,1);"
    "result=subprocess.run(sys.argv[1:],input=b'') if gate==b'1' else None;"
    "raise SystemExit(result.returncode if result is not None else 125)"
)


class WindowsJob:
    def __init__(self, process, memory_limit=None, process_limit=None):
        self.handle = None
        self.error = None
        if os.name != "nt":
            return

        class JOBOBJECT_BASIC_LIMIT_INFORMATION(ctypes.Structure):
            _fields_ = [
                ("PerProcessUserTimeLimit", ctypes.c_longlong),
                ("PerJobUserTimeLimit", ctypes.c_longlong),
                ("LimitFlags", wintypes.DWORD),
                ("MinimumWorkingSetSize", ctypes.c_size_t),
                ("MaximumWorkingSetSize", ctypes.c_size_t),
                ("ActiveProcessLimit", wintypes.DWORD),
                ("Affinity", ctypes.c_size_t),
                ("PriorityClass", wintypes.DWORD),
                ("SchedulingClass", wintypes.DWORD),
            ]

        class IO_COUNTERS(ctypes.Structure):
            _fields_ = [
                ("ReadOperationCount", ctypes.c_ulonglong),
                ("WriteOperationCount", ctypes.c_ulonglong),
                ("OtherOperationCount", ctypes.c_ulonglong),
                ("ReadTransferCount", ctypes.c_ulonglong),
                ("WriteTransferCount", ctypes.c_ulonglong),
                ("OtherTransferCount", ctypes.c_ulonglong),
            ]

        class JOBOBJECT_EXTENDED_LIMIT_INFORMATION(ctypes.Structure):
            _fields_ = [
                ("BasicLimitInformation", JOBOBJECT_BASIC_LIMIT_INFORMATION),
                ("IoInfo", IO_COUNTERS),
                ("ProcessMemoryLimit", ctypes.c_size_t),
                ("JobMemoryLimit", ctypes.c_size_t),
                ("PeakProcessMemoryUsed", ctypes.c_size_t),
                ("PeakJobMemoryUsed", ctypes.c_size_t),
            ]

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.CreateJobObjectW.restype = wintypes.HANDLE
        kernel32.SetInformationJobObject.argtypes = [
            wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD,
        ]
        kernel32.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
        kernel32.TerminateJobObject.argtypes = [wintypes.HANDLE, wintypes.UINT]
        kernel32.CloseHandle.argtypes = [wintypes.HANDLE]

        handle = kernel32.CreateJobObjectW(None, None)
        if not handle:
            self.error = f"CreateJobObjectW failed ({ctypes.get_last_error()})"
            return

        info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION()
        flags = 0x00002000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        if process_limit is not None:
            flags |= 0x00000008  # JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            info.BasicLimitInformation.ActiveProcessLimit = process_limit
        if memory_limit is not None:
            flags |= 0x00000200  # JOB_OBJECT_LIMIT_JOB_MEMORY
            info.JobMemoryLimit = memory_limit
        info.BasicLimitInformation.LimitFlags = flags

        configured = kernel32.SetInformationJobObject(
            handle,
            9,  # JobObjectExtendedLimitInformation
            ctypes.byref(info),
            ctypes.sizeof(info),
        )
        assigned = configured and kernel32.AssignProcessToJobObject(handle, wintypes.HANDLE(process._handle))
        if not assigned:
            self.error = f"AssignProcessToJobObject failed ({ctypes.get_last_error()})"
            kernel32.CloseHandle(handle)
            return
        self.handle = handle
        self._kernel32 = kernel32

    def terminate(self):
        if self.handle and not self._kernel32.TerminateJobObject(self.handle, 1):
            return f"TerminateJobObject failed ({ctypes.get_last_error()})"
        return None

    def close(self):
        if self.handle:
            self._kernel32.CloseHandle(self.handle)
            self.handle = None


def spawn_windows_gate(command, cwd=None, env=None, stdout=None, stderr=None, closed_stdin=False):
    return subprocess.Popen(
        [sys.executable, "-c",
         _WINDOWS_GATE_CLOSED_STDIN_WRAPPER if closed_stdin else _WINDOWS_GATE_WRAPPER,
         *command],
        cwd=cwd,
        env=env,
        stdin=subprocess.PIPE,
        stdout=stdout,
        stderr=stderr,
        creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        bufsize=0,
        start_new_session=False,
    )


def release_windows_gate(process, close_after=False):
    try:
        process.stdin.write(b"1")
        process.stdin.flush()
        if close_after:
            process.stdin.close()
        return None
    except OSError as exc:
        try:
            process.stdin.close()
        except OSError:
            pass
        return f"Windows launch gate failed ({type(exc).__name__})"


def terminate_owned_process(process, windows_job=None):
    if os.name == "nt":
        if windows_job and windows_job.handle:
            return windows_job.terminate()
        try:
            process.kill()
        except (ProcessLookupError, PermissionError, OSError):
            pass
        return None
    process.poll()
    for attempt in range(2):
        try:
            os.killpg(process.pid, signal.SIGKILL)
            return None
        except ProcessLookupError:
            return None
        except PermissionError:
            if attempt == 0 and process.poll() is not None:
                continue
            return "group_signal_failed"
        except OSError:
            return "group_signal_failed"
    return "group_signal_failed"


def wait_after_termination(process, windows_job=None, terminate=None, timeout=2):
    try:
        return process.wait(timeout=timeout), None
    except subprocess.TimeoutExpired:
        cleanup_error = (
            terminate() if terminate is not None else terminate_owned_process(process, windows_job)
        )
        try:
            return process.wait(timeout=timeout), cleanup_error
        except subprocess.TimeoutExpired:
            final_error = "process did not terminate after the final kill attempt"
            return -9, f"{cleanup_error}; {final_error}" if cleanup_error else final_error
