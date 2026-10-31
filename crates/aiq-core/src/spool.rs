//! Store-and-forward spool: while the core is down the hook appends one JSON line per frame (without the
//! token). Bounded at 1 MB, oldest lines dropped. The core drains it at start and on a timer.
//!
//! Accepted risk (Imran, 2026-10-31): the spool holds recent prompt text in plain text, owner-only,
//! until the next drain.
use aiq_contracts::frames::SpoolLine;
use std::io::{self, Write};
use std::path::Path;

pub const MAX_SPOOL_BYTES: u64 = 1024 * 1024;

/// Append one line. If the file would pass `max`, drop the oldest lines first. Returns lines dropped.
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
