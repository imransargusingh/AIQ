//! `aiq export [<folder>]`: everything AIQ stored on this machine, as plain JSON lines (private-alpha
//! design A6; spec coaching-loop NFR Privacy "full export"). One file per store table, one line per
//! stored row, nothing summed or derived, plus README.txt naming each file and its row count.
//! Reads the store directly (read-only, one snapshot), so it works whether or not the core runs.
use aiq_contracts::codes;
use aiq_core::paths::Paths;
use aiq_store::Store;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// What each table holds, in words, for README.txt. A table not named here still exports.
const ABOUT: &[(&str, &str)] = &[
    ("surface_session", "each Claude Code, Copilot or VS Code session AIQ saw"),
    ("event", "each prompt and each agent turn end AIQ recorded (times and ids)"),
    ("content", "the text of your prompts, with secrets already replaced by [secret removed]"),
    ("signal", "each time a coaching rule matched one of your tasks"),
    ("card_decision", "each decision to show, hold or drop a tip"),
    ("card_delivery", "each tip AIQ showed you, and where"),
    ("card_open", "each time you opened a tip's details"),
    ("setting_change", "each change to a setting; the newest row per setting is its current value"),
    ("folder_rule", "folders AIQ set aside as automated, and folders you included again"),
    ("import_run", "each run of aiq import"),
    ("import_session", "each past session aiq import read"),
    ("import_event", "each prompt and turn end aiq import added from past sessions"),
    ("judge_call", "each call AIQ made to your own Claude Code or Copilot to judge a task"),
    ("judgment", "each judgment of a task: job, kind of task, rung and the facts behind it"),
    ("type_guess", "each live guess of a task's kind, made at its first prompt"),
    ("tip_feedback", "each answer you gave to a tip (helpful, not for me, snooze, off, on)"),
    ("guidance_flag", "each \"This doesn't fit\" you flagged or undid"),
    ("judge_check_run", "each aiq judge --check run"),
    ("judge_check", "each second reading in an accuracy check (ids and yes/no answers only)"),
];

pub fn cli(rest: &[&str]) -> ExitCode {
    let paths = Paths::from_env();
    let (dir, default_place) = match rest.iter().find(|a| !a.starts_with('-')) {
        Some(d) => (PathBuf::from(d), false),
        None => (paths.data.join("exports").join(chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()), true),
    };
    match export(&paths.db(), &dir) {
        Ok(counts) => {
            let rows: u64 = counts.iter().map(|(_, n)| n).sum();
            println!("Exported to {}. {} files, {rows} rows.", dir.display(), counts.len() + 1);
            if default_place {
                println!("It is inside AIQ's own folder, so only you can read it.");
            } else {
                println!("It holds the text of your prompts. Keep the folder where only you can read it.");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("Couldn't export to {}. Nothing was exported. ({e})", dir.display());
            ExitCode::from(1)
        }
    }
}

/// Write the export into `dir`, which must not exist yet or be empty. On any failure the folder is
/// removed if this call created it, or emptied of what this call wrote, so no partial export is left.
pub fn export(db: &Path, dir: &Path) -> Result<Vec<(String, u64)>, String> {
    if !db.exists() {
        return Err(format!("{}: AIQ has stored nothing yet", codes::STORE_OPEN));
    }
    let created = !dir.exists();
    if !created && std::fs::read_dir(dir).map_err(|e| write_err(&e))?.next().is_some() {
        return Err(format!("{}: the folder is not empty; pick a new or empty folder", codes::EXPORT_WRITE));
    }
    let mut written: Vec<PathBuf> = Vec::new();
    let r = write_all(db, dir, &mut written);
    if r.is_err() {
        for f in &written {
            let _ = std::fs::remove_file(f);
        }
        if created {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    r
}

fn write_all(db: &Path, dir: &Path, written: &mut Vec<PathBuf>) -> Result<Vec<(String, u64)>, String> {
    make_private_dir(dir).map_err(|e| write_err(&e))?;
    let mut store = Store::open_read_only(db).map_err(|e| format!("{}: {}", e.code, e.detail))?;
    let mut files: HashMap<String, BufWriter<File>> = HashMap::new();
    let counts = store
        .export_rows(|table, row| {
            if !files.contains_key(table) {
                let p = dir.join(format!("{table}.jsonl"));
                written.push(p.clone());
                files.insert(table.to_owned(), BufWriter::new(private_file(&p)?));
            }
            let w = files.get_mut(table).expect("inserted above");
            serde_json::to_writer(&mut *w, &row)?;
            w.write_all(b"\n")
        })
        .map_err(|e| format!("{}: {}", e.code, e.detail))?;
    for (table, _) in &counts {
        match files.get_mut(table) {
            Some(w) => w.flush().map_err(|e| write_err(&e))?,
            None => {
                // A table with no rows still gets its (empty) file, so the folder always shows every table.
                let p = dir.join(format!("{table}.jsonl"));
                written.push(p.clone());
                private_file(&p).map_err(|e| write_err(&e))?;
            }
        }
    }
    let readme = dir.join("README.txt");
    written.push(readme.clone());
    let mut f = private_file(&readme).map_err(|e| write_err(&e))?;
    f.write_all(readme_text(&counts).as_bytes()).map_err(|e| write_err(&e))?;
    Ok(counts)
}

fn readme_text(counts: &[(String, u64)]) -> String {
    let mut s = format!(
        "AIQ export, {} (AIQ {}).\n\n\
         Everything AIQ stored on this machine. Each .jsonl file is one table of AIQ's store; each line is\n\
         one stored row as a JSON object, column name to value, oldest first. Nothing is summed or left out.\n\
         Times ending in _ms are milliseconds since 1970-01-01 UTC.\n\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M UTC"),
        env!("CARGO_PKG_VERSION")
    );
    for (t, n) in counts {
        let about = ABOUT.iter().find(|(k, _)| k == t).map_or("AIQ's own bookkeeping", |(_, v)| v);
        s += &format!("{t}.jsonl ({n} rows): {about}\n");
    }
    s += "\nThis folder is a copy. To delete everything AIQ stored: aiq forget --all (it does not delete this\n\
          folder; delete it yourself when you no longer need it).\n";
    s
}

fn write_err(e: &std::io::Error) -> String {
    format!("{}: {e}", codes::EXPORT_WRITE)
}

/// A new user-only directory (0700 on Unix). On Windows the folder inherits its parent's ACL.
/// assume: a folder the person names outside their profile may be readable by others on Windows;
/// confirm with icacls at gate G3; if false, set an owner-only ACL here. The CLI tells the person so.
fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir)
}

fn private_file(p: &Path) -> std::io::Result<File> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with_rows(dir: &Path) -> PathBuf {
        let db = dir.join("aiq.db");
        let s = Store::open(&db).unwrap();
        drop(s);
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch(
            "INSERT INTO setting_change (key, value, utc_ms) VALUES ('daily_cap', '5', 1);
             INSERT INTO setting_change (key, value, utc_ms) VALUES ('daily_cap', '3', 2);",
        )
        .unwrap();
        db
    }

    #[test]
    fn every_table_exports_one_line_per_row() {
        let d = tempfile::tempdir().unwrap();
        let db = store_with_rows(d.path());
        let out = d.path().join("out");
        let counts = export(&db, &out).unwrap();

        let c = rusqlite::Connection::open(&db).unwrap();
        let tables: Vec<String> = c
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(counts.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>(), tables);
        for (t, n) in &counts {
            let stored: i64 = c.query_row(&format!("SELECT COUNT(*) FROM \"{t}\""), [], |r| r.get(0)).unwrap();
            let stored = stored as u64;
            let text = std::fs::read_to_string(out.join(format!("{t}.jsonl"))).unwrap();
            assert_eq!(*n, stored, "{t}");
            assert_eq!(text.lines().count() as u64, stored, "{t}");
        }
        let lines = std::fs::read_to_string(out.join("setting_change.jsonl")).unwrap();
        let first: serde_json::Value = serde_json::from_str(lines.lines().next().unwrap()).unwrap();
        assert_eq!(first["key"], "daily_cap");
        assert_eq!(first["value"], "5");
        assert!(std::fs::read_to_string(out.join("README.txt")).unwrap().contains("setting_change.jsonl (2 rows)"));
    }

    #[test]
    fn refuses_a_folder_that_is_not_empty_and_leaves_it_alone() {
        let d = tempfile::tempdir().unwrap();
        let db = store_with_rows(d.path());
        let out = d.path().join("out");
        std::fs::create_dir(&out).unwrap();
        std::fs::write(out.join("mine.txt"), "keep").unwrap();
        let e = export(&db, &out).unwrap_err();
        assert!(e.starts_with(codes::EXPORT_WRITE), "{e}");
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
    }

    #[test]
    fn no_store_means_nothing_to_export_and_no_folder() {
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("out");
        assert!(export(&d.path().join("aiq.db"), &out).is_err());
        assert!(!out.exists());
    }

    #[cfg(unix)]
    #[test]
    fn export_is_user_only() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let db = store_with_rows(d.path());
        let out = d.path().join("out");
        export(&db, &out).unwrap();
        assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(std::fs::metadata(out.join("content.jsonl")).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
