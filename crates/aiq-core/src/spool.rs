//! Store-and-forward spool: while the core is down (or a send misses its budget) the hook keeps each frame
//! (without the token) for the core. Bounded at 1 MB, oldest frames dropped. The core drains it at start,
//! before its next command, and on a timer.
//!
//! One file per frame in `spool.d/`, written under a temporary name and renamed into place (DC-048): the
//! single `spool.jsonl` lost a frame whenever the core's drain renamed and deleted the file between the
//! hook's open and its write (Windows CI 2026-11-09, `e2e_reword_three_times_one_card_second_run_held`:
//! `AIQ-HOOK-SPOOLED 1`, no spool left, 7 of 8 prompts stored). A rename is atomic, so the core only ever
//! sees whole frames, and a frame written after a drain is a new file the next drain takes. `spool.jsonl`
//! is still drained, for frames an older hook wrote.
//!
//! Accepted risk (Imran, 2026-10-31): the spool holds recent prompt text in plain text, owner-only,
//! until the next drain.
use aiq_contracts::frames::SpoolLine;
use std::io::{self, Write};
use std::path::Path;

pub const MAX_SPOOL_BYTES: u64 = 1024 * 1024;
const FRAME_EXT: &str = "jsonl";
/// A temporary file older than this was left by a hook that died mid-write: the drain removes it.
const STALE_TMP: std::time::Duration = std::time::Duration::from_secs(3600);

/// Keep one frame as its own file in `dir`, named for `name` (time-ordered, so the oldest drops first). If
/// the spool would pass `max` bytes, the oldest frames go first. Returns frames dropped.
pub fn append_frame(dir: &Path, name: &str, line: &str, max: u64) -> io::Result<usize> {
    let need = line.len() as u64 + 1;
    if need > max {
        return Ok(1); // a single frame bigger than the spool is itself dropped
    }
    let safe: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(80).collect();
    if safe.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "frame name"));
    }
    create_dir_user_only(dir)?;
    let mut files = frames(dir)?;
    let mut size: u64 = files.iter().map(|(_, n)| n).sum();
    let mut dropped = 0;
    for (p, n) in &files {
        if size + need <= max {
            break;
        }
        if std::fs::remove_file(p).is_ok() {
            dropped += 1;
        }
        size -= n;
    }
    files.clear();
    let tmp = dir.join(format!(".{safe}.{}.tmp", std::process::id()));
    {
        let mut o = std::fs::OpenOptions::new();
        o.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        o.open(&tmp)?.write_all(format!("{line}\n").as_bytes())?;
    }
    std::fs::rename(&tmp, dir.join(format!("{safe}.{FRAME_EXT}"))).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(dropped)
}

/// Whole frame files in `dir`, oldest name first, with their sizes. Missing dir: none.
fn frames(dir: &Path) -> io::Result<Vec<(std::path::PathBuf, u64)>> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut v: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == FRAME_EXT))
        .map(|p| {
            let n = std::fs::metadata(&p).map_or(0, |m| m.len());
            (p, n)
        })
        .collect();
    v.sort();
    Ok(v)
}

/// Frames waiting in `dir` (for `aiq status`).
pub fn pending(dir: &Path) -> usize {
    frames(dir).map_or(0, |v| v.len())
}

/// Take every whole frame in `dir`: read each file, then remove it. A temporary file is a frame still being
/// written and is left for the next drain, unless it is stale.
pub fn take_frames(dir: &Path) -> io::Result<Vec<String>> {
    let mut lines = Vec::new();
    for (p, _) in frames(dir)? {
        match std::fs::read_to_string(&p) {
            Ok(s) => lines.extend(s.lines().map(str::to_owned)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        }
        std::fs::remove_file(&p)?;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            let stale = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
            if p.extension().is_some_and(|x| x == "tmp") && stale.is_some_and(|a| a > STALE_TMP) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    Ok(lines)
}

fn create_dir_user_only(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Append one line to the single-file spool an older hook wrote (kept for tests of that path and for `take`).
/// If the file would pass `max`, drop the oldest lines first. Returns lines dropped.
pub fn append(path: &Path, line: &str, max: u64) -> io::Result<usize> {
    let need = line.len() as u64 + 1;
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if len + need <= max {
        let mut o = std::fs::OpenOptions::new();
        o.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        o.open(path)?.write_all(format!("{line}\n").as_bytes())?;
        return Ok(0);
    }
    if need > max {
        return Ok(1); // a single line bigger than the spool is itself dropped
    }
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = old.lines().collect();
    let mut keep_from = 0;
    let mut size: u64 = lines.iter().map(|l| l.len() as u64 + 1).sum();
    while size + need > max && keep_from < lines.len() {
        size -= lines[keep_from].len() as u64 + 1;
        keep_from += 1;
    }
    let mut out = String::with_capacity(size as usize + need as usize);
    for l in &lines[keep_from..] {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(line);
    out.push('\n');
    crate::paths::write_user_only(path, out.as_bytes())?;
    Ok(keep_from)
}

/// Move the spool aside and return its lines. A left-over draining file from a crash is read first.
pub fn take(spool: &Path, draining: &Path) -> io::Result<Vec<String>> {
    let mut lines = Vec::new();
    if draining.exists() {
        lines.extend(std::fs::read_to_string(draining)?.lines().map(str::to_owned));
        std::fs::remove_file(draining)?;
    }
    match std::fs::rename(spool, draining) {
        Ok(()) => {
            lines.extend(std::fs::read_to_string(draining)?.lines().map(str::to_owned));
            std::fs::remove_file(draining)?;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    Ok(lines)
}

/// Parse lines; skip bad or partial ones; order by event id (UUIDv7 = time order) and remove duplicates,
/// so any interleaving or duplication of the same frames replays identically.
pub fn parse(lines: &[String]) -> (Vec<SpoolLine>, usize) {
    let mut bad = 0;
    let mut out: Vec<SpoolLine> = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| match serde_json::from_str::<SpoolLine>(l) {
            Ok(s) => Some(s),
            Err(_) => {
                bad += 1;
                None
            }
        })
        .collect();
    out.sort_by(|a, b| a.event.event_id.cmp(&b.event.event_id));
    out.dedup_by(|a, b| a.event.event_id == b.event.event_id);
    (out, bad)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn append_over_limit_drops_oldest_keeps_newest() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("s");
        for i in 0..10 {
            append(&p, &format!("line{i:02}-xxxxxxxxx"), 100).unwrap(); // 19 bytes + \n
        }
        let s = std::fs::read_to_string(&p).unwrap();
        assert!(s.len() as u64 <= 100);
        assert!(s.ends_with("line09-xxxxxxxxx\n"));
        assert!(!s.contains("line00"));
        assert_eq!(s.lines().count(), 5);
    }

    #[test]
    fn append_reports_dropped_count() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("s");
        assert_eq!(append(&p, "aaaa", 10).unwrap(), 0);
        assert_eq!(append(&p, "bbbb", 10).unwrap(), 0);
        assert_eq!(append(&p, "cccc", 10).unwrap(), 1);
        assert_eq!(append(&p, &"z".repeat(20), 10).unwrap(), 1);
    }

    /// DC-048: the old single file lost a frame when the drain ran between the hook's open and its write. A
    /// frame file appears only whole, so a drain at any moment takes it whole or leaves it for the next one.
    #[test]
    fn a_drain_between_the_hooks_open_and_write_loses_nothing() {
        let d = TempDir::new().unwrap();
        let dir = d.path().join("spool.d");
        // The old file: open, drain, write -> the frame is gone (this is the bug the frame files fix).
        let (s, dr) = (d.path().join("s"), d.path().join("d"));
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&s).unwrap();
        assert!(take(&s, &dr).unwrap().is_empty());
        f.write_all(b"frame\n").unwrap();
        drop(f);
        assert!(take(&s, &dr).unwrap().is_empty(), "old spool: the frame was lost");
        // Frame files: a drain while one is being written sees only its temporary file and leaves it.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".e1.1.tmp"), "half").unwrap();
        assert!(take_frames(&dir).unwrap().is_empty());
        assert!(dir.join(".e1.1.tmp").exists(), "a frame being written is left alone");
        assert_eq!(append_frame(&dir, "e2", "frame two", MAX_SPOOL_BYTES).unwrap(), 0);
        assert_eq!(pending(&dir), 1);
        assert_eq!(take_frames(&dir).unwrap(), vec!["frame two"]);
        assert_eq!(pending(&dir), 0);
    }

    #[test]
    fn frame_files_drop_the_oldest_past_the_limit_and_refuse_odd_names() {
        let d = TempDir::new().unwrap();
        let dir = d.path().join("spool.d");
        for i in 0..10 {
            let n = append_frame(&dir, &format!("e{i:02}"), &format!("line{i:02}-xxxxxxxxx"), 100).unwrap();
            assert_eq!(n, usize::from(i >= 5), "{i}");
        }
        let got = take_frames(&dir).unwrap();
        assert_eq!(got.first().map(String::as_str), Some("line05-xxxxxxxxx"));
        assert_eq!(got.last().map(String::as_str), Some("line09-xxxxxxxxx"));
        assert_eq!(append_frame(&dir, "big", &"z".repeat(200), 100).unwrap(), 1);
        assert!(append_frame(&dir, "../..", "x", 100).is_err());
        assert_eq!(append_frame(&dir, "a/../b", "x", 100).unwrap(), 0, "only the safe characters are kept");
        assert!(dir.join("ab.jsonl").exists());
    }

    #[test]
    fn take_empties_spool_and_reads_leftover_draining() {
        let d = TempDir::new().unwrap();
        let (s, dr) = (d.path().join("s"), d.path().join("d"));
        std::fs::write(&dr, "old\n").unwrap();
        std::fs::write(&s, "new\n").unwrap();
        assert_eq!(take(&s, &dr).unwrap(), vec!["old", "new"]);
        assert!(!s.exists() && !dr.exists());
        assert!(take(&s, &dr).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn spool_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let d = TempDir::new().unwrap();
        let p = d.path().join("s");
        append(&p, "x", 100).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        for _ in 0..30 {
            append(&p, "yyyyyyyyy", 100).unwrap(); // forces the rewrite path
        }
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
