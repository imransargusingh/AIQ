//! Keep the person's computers in sync through a folder only their own computers hold (design multi-machine-sync).
//!
//! Every computer keeps its own change files in `<aiq home>/sync/<its id>/` (numbered `00000001.jsonl`, …, plus a
//! `computer.json` saying who it is and whether sync is on there). A computer that can see another computer's AIQ
//! home (today: the laptop's drive inside a Remote Desktop session, `\\tsclient\c\Users\<name>\.aiq`) copies its own
//! files there and applies the files it finds there. So the laptop needs no network, port or relay: the Devbox does
//! the moving whenever the person is connected, and both catch up the next time. Nothing goes anywhere else.
//!
//! The setting `sync` is `on` or `off`; unset = never decided. The laptop turns on by itself the first time a
//! computer the person connected from joins (its folder appears) — the person chose "Keep in sync" there.
use crate::paths::Paths;
use aiq_store::Store;
use aiq_store::sync::{Applied, Segment};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SETTING_SYNC: &str = "sync";
/// Lines per change file: the first sync of an 18 MB store (measured 2026-11-12) is a handful of files.
pub const MAX_LINES: usize = 5000;
/// How often the core looks for new files when sync is on, or for another computer when it is not decided.
pub const EVERY: std::time::Duration = std::time::Duration::from_secs(60);
/// How soon a changed setting is noticed.
const NOTICE: std::time::Duration = std::time::Duration::from_secs(3);
const SELF_FILE: &str = "computer.json";

/// What a computer says about itself, in its own folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Computer {
    pub id: String,
    /// Windows' computer name; the person sees it in Settings.
    pub name: String,
    pub version: String,
    pub schema: i64,
    /// False when the person turned sync off on that computer: the others stop and show "Paused there".
    pub on: bool,
    pub utc_ms: i64,
}

/// One other computer as this one sees it, for Settings > Your computers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub version: String,
    /// ok, paused (off on that computer), update (its files need a newer AIQ here), waiting (an earlier file has not
    /// arrived), error (a file could not be read or applied; tried again next time).
    pub state: String,
    pub detail: Option<String>,
    /// When a file from it was last applied here; None = nothing yet.
    pub last_ms: Option<i64>,
    /// Rows from it here, in all.
    pub rows: i64,
}

/// The status file Home reads (`sync-status.json`): written by the core after every pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Status {
    pub id: String,
    pub name: String,
    /// on, off, or unset (never decided).
    pub setting: String,
    /// Another computer's AIQ can be reached right now (Remote Desktop) and the person has not decided yet.
    pub offer: Option<Offer>,
    /// The other computer reached through Remote Desktop, when one is: Home names the link.
    pub via: Option<String>,
    pub peers: Vec<Peer>,
    /// Rows recorded here and not yet in a change file.
    pub pending: u64,
    pub utc_ms: i64,
    /// Codes of what failed this pass (never text from the files).
    pub errors: Vec<String>,
}

/// Another computer's AIQ found over Remote Desktop, before the person chose: what the offer card shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    /// Its computer name, when it already has a sync folder; else the Windows user folder it was found in.
    pub name: String,
}

pub fn status_file(paths: &Paths) -> PathBuf {
    paths.data.join("sync-status.json")
}

pub fn read_status(paths: &Paths) -> Option<Status> {
    serde_json::from_slice(&std::fs::read(status_file(paths)).ok()?).ok()
}

/// This computer's name as Windows (or the OS) gives it.
pub fn computer_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "This computer".to_owned())
}

/// Other computers' AIQ homes this computer can see right now. Inside a Remote Desktop session on Windows: every
/// `\\tsclient\c\Users\*\.aiq` holding a store. Outside one, nothing is probed (a `\\tsclient` lookup outside a
/// session can take seconds). `AIQ_SYNC_REMOTE` (paths split by `;`) stands in for tests and other setups.
pub fn reachable_homes(paths: &Paths) -> Vec<PathBuf> {
    if let Some(v) = std::env::var_os("AIQ_SYNC_REMOTE") {
        return std::env::split_paths(&v).filter(|p| p.join("aiq.db").exists() && p != &paths.data).collect();
    }
    #[cfg(windows)]
    {
        if !remote_session() {
            return Vec::new();
        }
        let users = Path::new(r"\\tsclient\c\Users");
        let Ok(rd) = std::fs::read_dir(users) else { return Vec::new() };
        rd.filter_map(Result::ok).map(|e| e.path().join(".aiq")).filter(|p| p.join("aiq.db").exists()).collect()
    }
    #[cfg(not(windows))]
    Vec::new()
}

#[cfg(windows)]
fn remote_session() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_REMOTESESSION};
    // SAFETY: a plain query with no pointers.
    unsafe { GetSystemMetrics(SM_REMOTESESSION) != 0 }
}

fn seg_name(seq: i64) -> String {
    format!("{seq:08}.jsonl")
}

/// Write a whole file: a temp file beside it, then one rename, so a reader normally never sees half a file.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_whole_with(path, bytes, |a, b| std::fs::rename(a, b))
}

/// Tries a rename this many times, waiting a little longer each time, before writing in place.
const RENAME_TRIES: u32 = 5;

fn write_whole_with(
    path: &Path,
    bytes: &[u8],
    rename: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("part");
    {
        let mut o = std::fs::OpenOptions::new();
        o.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        let mut f = o.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    // Over Remote Desktop's drive link a fresh file can refuse a rename for a while (2026-11-12: a Devbox's
    // third file stayed `.part` on the laptop pass after pass). Try again briefly, then write the file in place:
    // readers check each change file's line count against its header, so a half-written one is tried again next
    // pass and never applied, and a half-written `computer.json` does not parse.
    let mut last = None;
    for i in 0..RENAME_TRIES {
        match rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
        std::thread::sleep(std::time::Duration::from_millis(100 * u64::from(i + 1)));
    }
    if let Some(e) = last {
        tracing::warn!(error.code = aiq_contracts::codes::SYNC_WRITE, error = %e, "sync rename refused; writing in place");
    }
    std::fs::write(path, bytes)?;
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

fn read_computer(dir: &Path) -> Option<Computer> {
    serde_json::from_slice(&std::fs::read(dir.join(SELF_FILE)).ok()?).ok()
}

/// The other computers' folders in one sync folder (not this computer's own).
fn peer_dirs(sync: &Path, me: &str) -> Vec<(PathBuf, Computer)> {
    let Ok(rd) = std::fs::read_dir(sync) else { return Vec::new() };
    let mut v: Vec<(PathBuf, Computer)> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n != me))
        .filter_map(|p| read_computer(&p).map(|c| (p, c)))
        .filter(|(_, c)| c.id != me)
        .collect();
    v.sort_by(|a, b| a.1.id.cmp(&b.1.id));
    v
}

/// Copy this computer's files that are missing in `to` (another computer's sync folder), oldest first.
fn mirror(from: &Path, to: &Path) -> std::io::Result<u32> {
    std::fs::create_dir_all(to)?;
    let mut names: Vec<String> = std::fs::read_dir(from)?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".jsonl"))
        .collect();
    names.sort();
    let mut n = 0;
    for name in names {
        let dst = to.join(&name);
        if !dst.exists() {
            write_whole(&dst, &std::fs::read(from.join(&name))?)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Apply a peer's files in order from the next one this store needs, until one is missing, held or fails.
fn apply_from(store: &mut Store, dir: &Path, peer: &mut Peer, now_ms: i64) {
    loop {
        let want = match store.sync_applied_upto(&peer.id) {
            Ok(n) => n + 1,
            Err(e) => {
                (peer.state, peer.detail) = ("error".into(), Some(e.code.to_owned()));
                return;
            }
        };
        let path = dir.join(seg_name(want));
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let seg = match Segment::from_jsonl(&text) {
            Ok(s) if s.device == peer.id && s.seq == want => s,
            Ok(_) | Err(_) => {
                (peer.state, peer.detail) = ("error".into(), Some(aiq_contracts::codes::SYNC_READ.into()));
                return;
            }
        };
        match store.sync_apply(&seg, now_ms) {
            Ok(Applied::Done { .. } | Applied::Already) => {}
            Ok(Applied::Held { .. }) => {
                peer.state = "update".into();
                return;
            }
            Ok(Applied::Gap { .. }) => {
                peer.state = "waiting".into();
                return;
            }
            Err(e) => {
                (peer.state, peer.detail) = ("error".into(), Some(e.code.to_owned()));
                return;
            }
        }
    }
}

/// One pass: decide, cut this computer's new files, copy them to every other computer it can reach, apply what is
/// new from the others. Returns the status Home shows (also written to `sync-status.json`). Never panics on a
/// missing folder or a bad file: each failure is a code in `errors` or a peer's state, and the next pass tries again.
pub fn pass(store: &mut Store, paths: &Paths, remote: &[PathBuf], now_ms: i64) -> Status {
    let mut st = Status { name: computer_name(), utc_ms: now_ms, ..Default::default() };
    let Ok(me) = store.device_id() else {
        st.errors.push(aiq_contracts::codes::STORE_OPEN.into());
        return st;
    };
    st.id.clone_from(&me);
    let local = paths.data.join("sync");
    let mine = local.join(&me);
    let mut setting = store.setting(SETTING_SYNC).ok().flatten().unwrap_or_default();
    // A computer the person connected from chose "Keep in sync" and left its folder here: join it.
    if setting.is_empty() && !peer_dirs(&local, &me).is_empty() && store.set_setting(SETTING_SYNC, "on", now_ms).is_ok()
    {
        setting = "on".into();
    }
    let on = setting == "on";
    st.setting = if setting.is_empty() { "unset".into() } else { setting.clone() };
    let remote_sync: Vec<PathBuf> = remote.iter().map(|h| h.join("sync")).collect();
    if let Some(h) = remote.first() {
        let name = peer_dirs(&remote_sync[0], &me)
            .first()
            .map(|(_, c)| c.name.clone())
            .or_else(|| h.parent().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default();
        st.via = Some(name.clone());
        if setting.is_empty() {
            st.offer = Some(Offer { name });
        }
    }
    let schema = store.user_version().unwrap_or_default();
    let me_json = |on: bool| {
        serde_json::to_vec_pretty(&Computer {
            id: me.clone(),
            name: computer_name(),
            version: env!("CARGO_PKG_VERSION").into(),
            schema,
            on,
            utc_ms: now_ms,
        })
        .unwrap_or_default()
    };
    if !on {
        // Say "paused" where this computer was syncing, so the others show it rather than waiting quietly.
        if mine.exists() {
            let _ = write_whole(&mine.join(SELF_FILE), &me_json(false));
            for r in &remote_sync {
                if r.join(&me).exists() {
                    let _ = write_whole(&r.join(&me).join(SELF_FILE), &me_json(false));
                }
            }
        }
        st.peers = known_peers(store, &local, &remote_sync, &me);
        write_status(paths, &st);
        return st;
    }
    // Cut every new file.
    loop {
        let cut = store.sync_export(
            MAX_LINES,
            |seg| write_whole(&mine.join(seg_name(seg.seq)), seg.to_jsonl(env!("CARGO_PKG_VERSION")).as_bytes()),
            now_ms,
        );
        match cut {
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(e) => {
                st.errors.push(e.code.to_owned());
                break;
            }
        }
    }
    if write_whole(&mine.join(SELF_FILE), &me_json(true)).is_err() {
        st.errors.push(aiq_contracts::codes::SYNC_WRITE.into());
    }
    for r in &remote_sync {
        // This computer's entry goes first, so the other computer sees it even while a file won't copy.
        let to = r.join(&me);
        let copied = write_whole(&to.join(SELF_FILE), &me_json(true)).and_then(|()| mirror(&mine, &to));
        if let Err(e) = copied {
            tracing::warn!(error.code = aiq_contracts::codes::SYNC_WRITE, error = %e, "sync could not copy to another computer");
            st.errors.push(aiq_contracts::codes::SYNC_WRITE.into());
        }
    }
    // Apply the others' new files, from every sync folder this computer can see.
    let mut peers: Vec<Peer> = Vec::new();
    for dir in std::iter::once(&local).chain(&remote_sync) {
        for (pdir, c) in peer_dirs(dir, &me) {
            if peers.iter().any(|p| p.id == c.id) {
                continue;
            }
            let mut p = Peer {
                id: c.id.clone(),
                name: c.name.clone(),
                version: c.version.clone(),
                state: "ok".into(),
                ..Default::default()
            };
            if c.on {
                apply_from(store, &pdir, &mut p, now_ms);
            } else {
                p.state = "paused".into();
            }
            peers.push(p);
        }
    }
    // Computers synced before but out of reach now (the Remote Desktop session ended) stay listed.
    for p in known_peers(store, &local, &remote_sync, &me) {
        if !peers.iter().any(|q| q.id == p.id) {
            peers.push(p);
        }
    }
    let applied = store.sync_peers().unwrap_or_default();
    for p in &mut peers {
        if let Some((_, _, rows, at)) = applied.iter().find(|(id, ..)| *id == p.id) {
            (p.rows, p.last_ms) = (*rows, Some(*at));
        }
    }
    st.peers = peers;
    st.pending = store.sync_pending().unwrap_or_default();
    write_status(paths, &st);
    st
}

/// Computers this store has applied files from, named from the last status (they may be out of reach now).
fn known_peers(store: &Store, local: &Path, remote: &[PathBuf], me: &str) -> Vec<Peer> {
    let seen: Vec<(PathBuf, Computer)> =
        std::iter::once(local.to_path_buf()).chain(remote.iter().cloned()).flat_map(|d| peer_dirs(&d, me)).collect();
    let last = read_status_at(local.parent().unwrap_or(local));
    store
        .sync_peers()
        .unwrap_or_default()
        .into_iter()
        .map(|(id, _, rows, at)| {
            let c = seen.iter().find(|(_, c)| c.id == id).map(|(_, c)| c.clone());
            let old = last.as_ref().and_then(|s| s.peers.iter().find(|p| p.id == id).cloned());
            Peer {
                name: c
                    .as_ref()
                    .map(|c| c.name.clone())
                    .or_else(|| old.as_ref().map(|o| o.name.clone()))
                    .unwrap_or_default(),
                version: c.as_ref().map(|c| c.version.clone()).or_else(|| old.map(|o| o.version)).unwrap_or_default(),
                state: if c.as_ref().is_some_and(|c| !c.on) { "paused".into() } else { "away".into() },
                detail: None,
                last_ms: Some(at),
                rows,
                id,
            }
        })
        .collect()
}

fn read_status_at(data: &Path) -> Option<Status> {
    serde_json::from_slice(&std::fs::read(data.join("sync-status.json")).ok()?).ok()
}

fn write_status(paths: &Paths, st: &Status) {
    if let Ok(b) = serde_json::to_vec_pretty(st) {
        let _ = write_whole(&status_file(paths), &b);
    }
}

/// Held while the sync thread has the store open; `true` once the core is deleting everything.
static GATE: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

/// `forget --all`: wait for a pass in progress to finish, then keep the sync thread from opening the store again,
/// so the store can be deleted (Windows refuses to delete an open file) and is not created again afterwards.
pub fn stop() {
    *GATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
}

/// The core's sync thread: a pass every minute, logged with counts and codes only. It opens the store only for a
/// check or a pass and closes it between them, so nothing holds the store while the core deletes everything.
pub fn run(paths: Paths) {
    let mut last_setting: Option<String> = None;
    let mut last_pass: Option<std::time::Instant> = None;
    loop {
        {
            let stopped = GATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if *stopped {
                return;
            }
            let mut store = match Store::open(&paths.db()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error.code = e.code, "sync could not open the store");
                    drop(stopped);
                    std::thread::sleep(EVERY);
                    continue;
                }
            };
            // A choice the person just made ("Keep in sync", the switch) runs a pass within seconds, not a minute.
            let setting = store.setting(SETTING_SYNC).ok().flatten();
            if !(last_pass.is_some_and(|t| t.elapsed() < EVERY) && setting == last_setting) {
                last_pass = Some(std::time::Instant::now());
                let t = std::time::Instant::now();
                let remote = reachable_homes(&paths);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as i64);
                let st = pass(&mut store, &paths, &remote, now);
                if st.setting == "on" || st.offer.is_some() {
                    tracing::info!(
                        sync.setting = %st.setting,
                        sync.remote = remote.len(),
                        sync.peers = st.peers.len(),
                        sync.pending = st.pending,
                        sync.errors = %st.errors.join(","),
                        sync.ms = t.elapsed().as_millis() as u64,
                        "sync.pass"
                    );
                }
                last_setting = store.setting(SETTING_SYNC).ok().flatten();
            }
        }
        std::thread::sleep(NOTICE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rename_the_drive_refuses_still_leaves_the_whole_file_and_no_part() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sync").join("a").join("00000001.jsonl");
        let refused = |_: &Path, _: &Path| Err(std::io::Error::other("refused"));
        write_whole_with(&p, b"line 1\nline 2\n", refused).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"line 1\nline 2\n");
        assert!(!p.with_extension("part").exists());
    }

    #[test]
    fn a_rename_that_works_on_a_later_try_is_used() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("computer.json");
        let tries = std::cell::Cell::new(0);
        let flaky = |a: &Path, b: &Path| {
            tries.set(tries.get() + 1);
            if tries.get() < 3 { Err(std::io::Error::other("busy")) } else { std::fs::rename(a, b) }
        };
        write_whole_with(&p, b"{}", flaky).unwrap();
        assert_eq!((tries.get(), std::fs::read(&p).unwrap()), (3, b"{}".to_vec()));
    }
}
