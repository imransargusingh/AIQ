//! Spikes S1 and S3 for docs/design/claude-code-skeleton.md (run on Windows; also builds on Linux).
//!
//! aiq-spike-win s1                 S1: default vs user-only named-pipe DACL, same-user connect, squatting
//! aiq-spike-win hook [--spawn] [--card]   minimal hook client: stdin -> core pipe, or spool (+ start core)
//! aiq-spike-win core [secs]        stand-in core: user-only pipe, heartbeat file, connection log
//! aiq-spike-win bench [n]          S3: whole-process hook time, direct and via Git Bash, core up and down
//! aiq-spike-win env                print which CLAUDE_* variables a hook process sees (names only)
use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions, Stream};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs, path::PathBuf, thread};

const CORE_PIPE: &str = "aiq-spike-core";

fn tmp(name: &str) -> PathBuf {
    env::temp_dir().join(name)
}

fn now_s() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn listener(name: &str, user_only: bool) -> std::io::Result<interprocess::local_socket::Listener> {
    let n = name.to_ns_name::<GenericNamespaced>()?;
    let opts = ListenerOptions::new().name(n);
    #[cfg(windows)]
    let opts = if user_only {
        use interprocess::os::windows::{local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor};
        let sddl = format!("D:P(A;;GA;;;{})", win::user_sid()?);
        let w = widestring::U16CString::from_str(&sddl).expect("sddl has no NUL");
        opts.security_descriptor(SecurityDescriptor::deserialize(&w)?)
    } else {
        opts
    };
    #[cfg(not(windows))]
    let _ = user_only;
    opts.create_sync()
}

fn s1() {
    let pid = std::process::id();
    let def = format!("aiq-s1-default-{pid}");
    let usr = format!("aiq-s1-user-{pid}");
    let l_def = listener(&def, false);
    println!("default_create: {:?}", l_def.as_ref().map(|_| "ok"));
    #[cfg(windows)]
    println!("default_sddl: {:?}", win::pipe_sddl(&def));
    let l_usr = listener(&usr, true);
    println!("user_only_create: {:?}", l_usr.as_ref().map(|_| "ok"));
    #[cfg(windows)]
    {
        println!("current_user_sid: {:?}", win::user_sid());
        println!("user_only_sddl: {:?}", win::pipe_sddl(&usr));
    }
    if let Ok(l) = l_usr {
        let h = thread::spawn(move || {
            let mut s = l.accept().ok()?;
            let mut b = String::new();
            s.read_to_string(&mut b).ok()?;
            Some(b)
        });
        let r = usr.as_str().to_ns_name::<GenericNamespaced>().and_then(Stream::connect).and_then(|mut s| s.write_all(b"hello"));
        println!("same_user_connect: {:?}", r);
        println!("server_received: {:?}", h.join().ok().flatten());
    }
    // Squatting: a second listener on a name that already exists must fail (FILE_FLAG_FIRST_PIPE_INSTANCE).
    let _keep = listener(&def, false);
    let again = listener(&def, true);
    println!("second_create_same_name: {:?}", again.map(|_| "UNEXPECTED ok"));
}

fn hook(spawn: bool, card: bool) {
    let mut buf = Vec::new();
    let _ = std::io::stdin().take(256 * 1024).read_to_end(&mut buf);
    let id = uuid::Uuid::now_v7();
    let frame = format!("{{\"op\":\"ingest\",\"event_id\":\"{id}\",\"bytes\":{}}}\n", buf.len());
    let sent = CORE_PIPE.to_ns_name::<GenericNamespaced>().and_then(Stream::connect).and_then(|mut s| s.write_all(frame.as_bytes()));
    if sent.is_err() {
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(tmp("aiq-spike-spool.jsonl")) {
            let _ = f.write_all(frame.as_bytes());
        }
        if spawn {
            let lock = tmp("aiq-spike-start.lock");
            let recent = fs::read_to_string(&lock).ok().and_then(|s| s.trim().parse::<u64>().ok()).is_some_and(|t| now_s() < t + 10);
            if !recent {
                let _ = fs::write(&lock, now_s().to_string());
                let r = spawn_detached(&["core", "60"]);
                let _ = fs::write(tmp("aiq-spike-spawn.txt"), format!("{r:?}\n"));
            }
        }
    }
    if card {
        println!("{{\"systemMessage\": \"AIQ spike: compiled hook client ran (event {id}).\"}}");
    }
}

fn spawn_detached(args: &[&str]) -> Result<&'static str, String> {
    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let mk = || {
        let mut c = Command::new(&exe);
        c.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        c
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};
        let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        match mk().creation_flags(base | CREATE_BREAKAWAY_FROM_JOB).spawn() {
            Ok(_) => Ok("spawned with breakaway"),
            Err(e1) => match mk().creation_flags(base).spawn() {
                Ok(_) => Ok("spawned WITHOUT breakaway (job refused it)"),
                Err(e2) => Err(format!("{e1}; {e2}")),
            },
        }
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::process::CommandExt;
        mk().process_group(0).spawn().map(|_| "spawned in new process group").map_err(|e| e.to_string())
    }
}

fn core(secs: u64) {
    let l = match listener(CORE_PIPE, true) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("core: cannot create pipe: {e}");
            return;
        }
    };
    let start = now_s();
    thread::spawn(move || loop {
        let _ = fs::write(tmp("aiq-spike-heartbeat.txt"), format!("{} {}\n", now_s(), std::process::id()));
        if now_s() >= start + secs {
            std::process::exit(0);
        }
        thread::sleep(Duration::from_secs(1));
    });
    let mut n = 0u64;
    for c in l.incoming() {
        if let Ok(mut s) = c {
            let mut b = String::new();
            let _ = s.read_to_string(&mut b);
            n += 1;
            let _ = fs::write(tmp("aiq-spike-core-count.tmp"), n.to_string());
            let _ = fs::rename(tmp("aiq-spike-core-count.tmp"), tmp("aiq-spike-core-count.txt"));
        }
    }
}

fn pct(mut v: Vec<f64>) -> String {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p = |q: f64| v[((v.len() as f64 - 1.0) * q).round() as usize];
    format!("{{\"n\":{},\"p50_ms\":{:.1},\"p95_ms\":{:.1},\"max_ms\":{:.1}}}", v.len(), p(0.5), p(0.95), v[v.len() - 1])
}

fn bench(n: usize) {
    let exe = env::current_exe().unwrap();
    let exe_fwd = exe.to_string_lossy().replace('\\', "/");
    let bash = env::var("AIQ_BASH").unwrap_or_else(|_| "bash".into());
    let payload = br#"{"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":"refactor the parser to stream input","prompt_id":"p","cwd":"C:/tmp","transcript_path":"C:/tmp/t.jsonl","permission_mode":"default"}"#;
    let run = |via_bash: bool| -> f64 {
        let mut c = if via_bash {
            let mut c = Command::new(&bash);
            c.args(["-c", &format!("'{exe_fwd}' hook")]);
            c
        } else {
            let mut c = Command::new(&exe);
            c.arg("hook");
            c
        };
        let t = Instant::now();
        let mut ch = c.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("spawn");
        ch.stdin.take().unwrap().write_all(payload).unwrap();
        ch.wait().unwrap();
        t.elapsed().as_secs_f64() * 1000.0
    };
    for core_up in [true, false] {
        let mut core = if core_up {
            let c = Command::new(&exe).args(["core", "300"]).stdin(Stdio::null()).stdout(Stdio::null()).spawn().ok();
            thread::sleep(Duration::from_millis(500));
            c
        } else {
            None
        };
        for via_bash in [false, true] {
            let _ = run(via_bash); // warm the file cache once
            let v: Vec<f64> = (0..n).map(|_| run(via_bash)).collect();
            println!("bench core_up={core_up} via_bash={via_bash}: {}", pct(v));
        }
        if let Some(c) = core.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if core_up {
            println!("core_connections_received: {:?}", fs::read_to_string(tmp("aiq-spike-core-count.txt")).ok());
        }
    }
}

fn main() {
    let a: Vec<String> = env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("s1") => s1(),
        Some("hook") => hook(a.iter().any(|x| x == "--spawn"), a.iter().any(|x| x == "--card")),
        Some("core") => core(a.get(2).and_then(|s| s.parse().ok()).unwrap_or(60)),
        Some("bench") => bench(a.get(2).and_then(|s| s.parse().ok()).unwrap_or(50)),
        Some("env") => {
            let mut k: Vec<String> = env::vars().map(|(k, _)| k).filter(|k| k.starts_with("CLAUDE")).collect();
            k.sort();
            println!("{}", k.join("\n"));
        }
        _ => eprintln!("usage: aiq-spike-win s1 | hook [--spawn] [--card] | core [secs] | bench [n] | env"),
    }
}

#[cfg(windows)]
mod win {
    use std::io;
    use std::ptr::{null, null_mut};
    use widestring::{U16CStr, U16CString};
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW, GetSecurityInfo, SDDL_REVISION_1,
        SE_KERNEL_OBJECT,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenUser, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING, READ_CONTROL};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    pub fn user_sid() -> io::Result<String> {
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

    pub fn pipe_sddl(name: &str) -> io::Result<String> {
        let path = U16CString::from_str(format!(r"\\.\pipe\{name}")).expect("no NUL");
        unsafe {
            let h = CreateFileW(path.as_ptr(), READ_CONTROL, 0, null(), OPEN_EXISTING, 0, null_mut());
            if h == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let info = DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION;
            let mut psd: PSECURITY_DESCRIPTOR = null_mut();
            let r = GetSecurityInfo(h, SE_KERNEL_OBJECT, info, null_mut(), null_mut(), null_mut(), null_mut(), &mut psd);
            CloseHandle(h);
            if r != 0 {
                return Err(io::Error::from_raw_os_error(r as i32));
            }
            let mut s: *mut u16 = null_mut();
            let ok = ConvertSecurityDescriptorToStringSecurityDescriptorW(psd, SDDL_REVISION_1, info, &mut s, null_mut());
            LocalFree(psd);
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            let out = U16CStr::from_ptr_str(s).to_string_lossy();
            LocalFree(s.cast());
            Ok(out)
        }
    }
}
