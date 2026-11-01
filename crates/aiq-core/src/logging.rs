//! Local structured logs: JSON lines, OTel severity names via `tracing`, owner-only file, rotated at
//! 5 MB × 3 files. Never prompt text (O11): only ids, counts and codes are passed to log macros.
use std::io;
use std::path::{Path, PathBuf};

pub const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
pub const KEEP_FILES: usize = 3;

pub fn init(path: &Path) {
    let path = path.to_owned();
    let _ = tracing_subscriber::fmt()
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .with_ansi(false)
        .with_writer(move || open_rotating(&path))
        .try_init();
}

/// Writer for one log record: rotate if the file is over the limit, then append.
pub fn open_rotating(path: &Path) -> Box<dyn io::Write> {
    let rotated = |n: usize| PathBuf::from(format!("{}.{n}", path.display()));
    if std::fs::metadata(path).map(|m| m.len() > MAX_LOG_BYTES).unwrap_or(false) {
        for n in (1..KEEP_FILES).rev() {
            let _ = std::fs::rename(if n == 1 { path.to_owned() } else { rotated(n - 1) }, rotated(n));
        }
    }
    let mut o = std::fs::OpenOptions::new();
    o.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    match o.open(path) {
        Ok(f) => Box::new(f),
        Err(_) => Box::new(io::sink()),
    }
}
