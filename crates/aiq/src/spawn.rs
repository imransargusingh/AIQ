//! Start `aiq core` fully detached, so it survives the hook's process tree ending (spike S3b).
use std::process::{Command, Stdio};

pub fn core_detached() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mk = || {
        let mut c = Command::new(&exe);
        c.arg("core").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        c
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{
            CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS,
        };
        let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        // Breakaway is refused when the caller's job forbids it; the plain detached core still survived (S3b).
        mk().creation_flags(base | CREATE_BREAKAWAY_FROM_JOB)
            .spawn()
            .or_else(|_| mk().creation_flags(base).spawn())
            .map(|_| ())
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::process::CommandExt;
        // New session: no controlling terminal, not in the hook's process group.
        let mut c = mk();
        unsafe {
            // SAFETY: setsid is async-signal-safe and touches no Rust state between fork and exec.
            c.pre_exec(|| {
                if libc_setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        c.spawn().map(|_| ())
    }
}

#[cfg(not(windows))]
unsafe extern "C" {
    #[link_name = "setsid"]
    safe fn libc_setsid() -> i32;
}
