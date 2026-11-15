//! DC-089: the read-only Python scripts that read a person's store must see what AIQ sees. `immutable=1` tells
//! SQLite the file never changes, so it skips the write-ahead log, where AIQ's newest rows sit until a
//! checkpoint: measured 2026-11-15, a review page built that way showed none of the person's 14 paused readings.
//! A script opens the store `mode=ro` (best with `PRAGMA query_only = ON`, as `Store::open_read_only` does).
use std::path::{Path, PathBuf};

fn scripts(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            scripts(&p, out);
        } else if p.extension().is_some_and(|x| x == "py") {
            out.push(p);
        }
    }
}

#[test]
fn read_only_scripts_never_open_the_store_immutable() {
    let docs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/investigations");
    let mut all = Vec::new();
    scripts(&docs, &mut all);
    let mut opened = 0;
    let mut bad = Vec::new();
    for p in &all {
        let text = std::fs::read_to_string(p).unwrap_or_default();
        for line in text.lines().filter(|l| l.contains("sqlite3.connect(") && l.contains("mode=ro")) {
            opened += 1;
            if line.contains("immutable=1") {
                bad.push(format!("{}: {}", p.display(), line.trim()));
            }
        }
    }
    assert!(opened >= 8, "the sweep found the scripts ({opened} read-only opens)");
    assert!(bad.is_empty(), "open the store mode=ro (and PRAGMA query_only), never immutable=1:\n{}", bad.join("\n"));
}
