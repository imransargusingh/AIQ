"""Bounded regular-file reads with pinned, non-reparse Windows ancestors.

The Windows handles deny delete sharing until the read ends. This protects the
path-to-handle transition; it is not isolation from hostile code as the same user.
"""
import contextlib
import os
from pathlib import Path
import stat


def protect_private_directory(path):
    """Apply an inheritable current-user/SYSTEM DACL, not POSIX chmod emulation."""
    if os.name != "nt":
        return
    import ctypes
    from ctypes import wintypes

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    advapi = ctypes.WinDLL("advapi32", use_last_error=True)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = (wintypes.HANDLE,)
    kernel.LocalFree.argtypes = (wintypes.HLOCAL,)
    kernel.LocalFree.restype = wintypes.HLOCAL
    advapi.OpenProcessToken.argtypes = (wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE))
    advapi.GetTokenInformation.argtypes = (wintypes.HANDLE, ctypes.c_int, wintypes.LPVOID,
                                           wintypes.DWORD, ctypes.POINTER(wintypes.DWORD))
    advapi.ConvertSidToStringSidW.argtypes = (wintypes.LPVOID, ctypes.POINTER(wintypes.LPWSTR))
    advapi.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes = (
        wintypes.LPCWSTR, wintypes.DWORD, ctypes.POINTER(wintypes.LPVOID), wintypes.LPVOID)
    advapi.GetSecurityDescriptorDacl.argtypes = (
        wintypes.LPVOID, ctypes.POINTER(wintypes.BOOL), ctypes.POINTER(wintypes.LPVOID),
        ctypes.POINTER(wintypes.BOOL))
    advapi.SetNamedSecurityInfoW.argtypes = (wintypes.LPWSTR, ctypes.c_int, wintypes.DWORD,
                                           wintypes.LPVOID, wintypes.LPVOID,
                                           wintypes.LPVOID, wintypes.LPVOID)
    advapi.SetNamedSecurityInfoW.restype = wintypes.DWORD
    token = wintypes.HANDLE()
    sid_text = wintypes.LPWSTR()
    descriptor = wintypes.LPVOID()
    try:
        if not advapi.OpenProcessToken(kernel.GetCurrentProcess(), 8, ctypes.byref(token)):
            raise ctypes.WinError(ctypes.get_last_error())
        length = wintypes.DWORD()
        advapi.GetTokenInformation(token, 1, None, 0, ctypes.byref(length))
        info = ctypes.create_string_buffer(length.value)
        if not advapi.GetTokenInformation(token, 1, info, length, ctypes.byref(length)):
            raise ctypes.WinError(ctypes.get_last_error())
        sid = ctypes.cast(info, ctypes.POINTER(wintypes.LPVOID))[0]
        if not advapi.ConvertSidToStringSidW(sid, ctypes.byref(sid_text)):
            raise ctypes.WinError(ctypes.get_last_error())
        sddl = "D:P(A;OICI;FA;;;" + sid_text.value + ")(A;OICI;FA;;;SY)"
        if not advapi.ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl, 1, ctypes.byref(descriptor), None):
            raise ctypes.WinError(ctypes.get_last_error())
        present, defaulted, acl = wintypes.BOOL(), wintypes.BOOL(), wintypes.LPVOID()
        if not advapi.GetSecurityDescriptorDacl(
                descriptor, ctypes.byref(present), ctypes.byref(acl), ctypes.byref(defaulted)):
            raise ctypes.WinError(ctypes.get_last_error())
        error = advapi.SetNamedSecurityInfoW(str(Path(path).absolute()), 1, 0x80000004,
                                            None, None, acl, None)
        if error:
            raise ctypes.WinError(error)
    finally:
        if descriptor:
            kernel.LocalFree(descriptor)
        if sid_text:
            kernel.LocalFree(ctypes.cast(sid_text, wintypes.HLOCAL))
        if token:
            kernel.CloseHandle(token)


@contextlib.contextmanager
def pinned_directory(path):
    """Hold every Windows ancestor without following a reparse point."""
    if os.name != "nt":
        yield
        return
    import ctypes
    from ctypes import wintypes

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateFileW.argtypes = (wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                                  wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD,
                                  wintypes.HANDLE)
    kernel.CreateFileW.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = (wintypes.HANDLE,)
    kernel.CloseHandle.restype = wintypes.BOOL
    kernel.GetFileInformationByHandle.argtypes = (wintypes.HANDLE, wintypes.LPVOID)
    kernel.GetFileInformationByHandle.restype = wintypes.BOOL
    absolute = Path(os.path.abspath(path))
    handles = []
    try:
        for directory in (*reversed(absolute.parents), absolute):
            handle = kernel.CreateFileW(str(directory), 0x80, 3, None, 3, 0x02200000, None)
            if handle == ctypes.c_void_p(-1).value:
                raise ctypes.WinError(ctypes.get_last_error())
            handles.append(handle)
            info = (wintypes.DWORD * 13)()
            if not kernel.GetFileInformationByHandle(handle, ctypes.byref(info)):
                raise ctypes.WinError(ctypes.get_last_error())
            if info[0] & 0x400 or not info[0] & 0x10:
                raise ValueError("invalid_runtime_control")
        yield
    finally:
        for handle in reversed(handles):
            kernel.CloseHandle(handle)


@contextlib.contextmanager
def open_regular(path, *, writable=False, create=False):
    """Open a non-reparse regular file; a writable shared handle supports locking."""
    path = Path(path)
    if os.name != "nt":
        flags = (os.O_RDWR if writable else os.O_RDONLY) | os.O_NOFOLLOW | os.O_NONBLOCK
        if create:
            flags |= os.O_CREAT
        fd = os.open(path, flags, 0o600)
        with os.fdopen(fd, "r+b" if writable else "rb") as stream:
            if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
                raise ValueError("invalid_runtime_control")
            yield stream
        return
    import ctypes
    from ctypes import wintypes
    import msvcrt

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateFileW.argtypes = (wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                                  wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD,
                                  wintypes.HANDLE)
    kernel.CreateFileW.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = (wintypes.HANDLE,)
    kernel.CloseHandle.restype = wintypes.BOOL
    kernel.GetFileInformationByHandle.argtypes = (wintypes.HANDLE, wintypes.LPVOID)
    kernel.GetFileInformationByHandle.restype = wintypes.BOOL
    with pinned_directory(path.parent):
        handle = kernel.CreateFileW(str(path.absolute()), 0xC0000000 if writable else 0x80000000,
                                    3 if writable else 1, None, 4 if create else 3,
                                    0x00200000, None)
        if handle == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            info = (wintypes.DWORD * 13)()
            if not kernel.GetFileInformationByHandle(handle, ctypes.byref(info)):
                raise ctypes.WinError(ctypes.get_last_error())
            if info[0] & (0x400 | 0x10):
                raise ValueError("invalid_runtime_control")
            fd = msvcrt.open_osfhandle(handle, (os.O_RDWR if writable else os.O_RDONLY)
                                      | os.O_BINARY | os.O_NOINHERIT)
        except BaseException:
            kernel.CloseHandle(handle)
            raise
        with os.fdopen(fd, "r+b" if writable else "rb") as stream:
            if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
                raise ValueError("invalid_runtime_control")
            yield stream


def read_regular(path, limit):
    with open_regular(path) as stream:
        before = os.fstat(stream.fileno())
        data = stream.read(limit + 1)
        after = os.fstat(stream.fileno())
        if (len(data) > limit or (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
                != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns)):
            raise ValueError("invalid_runtime_control")
        return data
