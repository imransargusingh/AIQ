//! The local API transport: a Unix domain socket in a 0700 directory, or a Windows named pipe whose
//! DACL grants only the current user (spike S1). One newline-terminated JSON frame per connection,
//! guarded by a per-boot token from a user-only file.
use crate::paths::Paths;
use interprocess::local_socket::{Listener, ListenerOptions, Stream, prelude::*};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::mpsc;
use std::time::Duration;

pub fn listen(paths: &Paths) -> io::Result<Listener> {
    #[cfg(not(windows))]
    {
        use interprocess::local_socket::GenericFilePath;
        let p = paths.socket();
        // Only the core that holds core.lock reaches here, so a leftover socket file is stale.
        let _ = std::fs::remove_file(&p);
        let name = p.as_path().to_fs_name::<GenericFilePath>()?;
        let l = ListenerOptions::new().name(name).create_sync()?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))?;
        Ok(l)
    }
    #[cfg(windows)]
    {
        use interprocess::local_socket::GenericNamespaced;
        use interprocess::os::windows::{local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor};
        let name = paths.pipe_name();
        let sddl = format!("D:P(A;;GA;;;{})", win::user_sid()?);
        let w = widestring::U16CString::from_str(&sddl).map_err(|e| io::Error::other(e.to_string()))?;
        ListenerOptions::new()
            .name(name.to_ns_name::<GenericNamespaced>()?)
            .security_descriptor(SecurityDescriptor::deserialize(&w)?)
            .create_sync()
    }
}

pub fn connect(paths: &Paths) -> io::Result<Stream> {
    #[cfg(not(windows))]
    {
        use interprocess::local_socket::GenericFilePath;
        let p = paths.socket();
        Stream::connect(p.as_path().to_fs_name::<GenericFilePath>()?)
    }
    #[cfg(windows)]
    {
        use interprocess::local_socket::GenericNamespaced;
        Stream::connect(paths.pipe_name().to_ns_name::<GenericNamespaced>()?)
    }
}

/// Read one frame line, at most `max` bytes. Returns None on EOF or an over-long frame.
pub fn read_line_capped<R: Read>(r: R, max: usize) -> io::Result<Option<String>> {
    let mut br = BufReader::new(r.take(max as u64 + 1));
    let mut buf = Vec::new();
    br.read_until(b'\n', &mut buf)?;
    if buf.is_empty() || buf.len() > max {
        return Ok(None);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
    }
    Ok(String::from_utf8(buf).ok())
}

/// Send one frame and, if `want_reply`, read one reply line; all within `timeout`. The I/O runs on a
/// helper thread so a stuck peer can never hold the caller past its budget.
pub fn request(paths: &Paths, frame: &str, want_reply: bool, timeout: Duration) -> io::Result<Option<String>> {
    let (tx, rx) = mpsc::channel();
    let paths = paths.clone();
    let line = format!("{frame}\n");
    std::thread::spawn(move || {
        let r = (|| -> io::Result<Option<String>> {
            let mut s = connect(&paths)?;
            s.write_all(line.as_bytes())?;
            s.flush()?;
            if !want_reply {
                return Ok(None);
            }
            read_line_capped(&mut s, aiq_contracts::frames::MAX_FRAME_BYTES)
        })();
        let _ = tx.send(r);
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => r,
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "core did not answer in time")),
    }
}

pub fn read_token(paths: &Paths) -> Option<String> {
    std::fs::read_to_string(paths.token()).ok().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// 256 random bits as hex, from two v4 UUIDs (122 random bits each, OS randomness).
pub fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// Constant-time comparison of the presented token with the expected one.
pub fn token_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(windows)]
pub mod win {
    use std::io;
    use std::ptr::null_mut;
    use widestring::U16CStr;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    /// String SID of the current user (from spike S1).
    pub fn user_sid() -> io::Result<String> {
        // SAFETY: standard token query; every handle and buffer is checked and freed on each path.
        unsafe {
            let mut tok: HANDLE = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut len = 0u32;
            GetTokenInformation(tok, TokenUser, null_mut(), 0, &mut len);
            let mut buf = vec![0u8; len as usize];
            let ok = GetTokenInformation(tok, TokenUser, buf.as_mut_ptr().cast(), len, &mut len);
            CloseHandle(tok);
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            let tu = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s: *mut u16 = null_mut();
            if ConvertSidToStringSidW(tu.User.Sid, &mut s) == 0 {
                return Err(io::Error::last_os_error());
            }
            let out = U16CStr::from_ptr_str(s).to_string_lossy();
            LocalFree(s.cast());
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_eq_exact_only() {
        assert!(token_eq("abc", "abc"));
        assert!(!token_eq("abc", "abd"));
        assert!(!token_eq("abc", "abcd"));
        assert!(!token_eq("", "a"));
    }

    #[test]
    fn new_token_is_64_hex_and_fresh() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn read_line_capped_rejects_over_long_frame() {
        assert_eq!(read_line_capped(&b"abc\nrest"[..], 10).unwrap().as_deref(), Some("abc"));
        assert_eq!(read_line_capped(&b"abcdefghijk"[..], 10).unwrap(), None);
        assert_eq!(read_line_capped(&b""[..], 10).unwrap(), None);
        assert_eq!(read_line_capped(&b"no newline"[..], 10).unwrap().as_deref(), Some("no newline"));
    }
}
