//! Sync between one person's computers (schema v12, docs/design/multi-machine-sync.md).
//!
//! Each computer cuts numbered change files from its own rows (`sync_outbox`, filled by insert triggers) and applies
//! the other computers' files, each exactly once and in order (`sync_applied`). Rows keep their keys, so a merge is
//! "add what you don't have"; a row from another computer carries that computer's id in `computer` and is never sent
//! on. Deletes the person makes travel as `sync_delete` rows and are repeated on arrival. Files move as plain JSON
//! lines; the folder they move through belongs to the person (aiq-core `sync`).
use super::{Result, Store, StoreError, err};
use aiq_contracts::codes;
use rusqlite::{OptionalExtension, params};
use serde_json::{Map, Value, json};

/// The synced tables, parents before children (the order foreign keys need), with the AUTOINCREMENT id column a
/// receiving computer leaves out so its own numbering continues.
pub const SYNCED: [(&str, Option<&str>); 18] = [
    ("surface_session", None),
    ("event", None),
    ("content", None),
    ("signal", None),
    ("card_decision", None),
    ("card_delivery", None),
    ("card_open", Some("open_id")),
    ("set_aside_session", None),
    ("turn_action", None),
    ("type_guess", None),
    ("kind_label", None),
    ("tip_path", None),
    ("tip_feedback", Some("feedback_id")),
    ("guidance_flag", Some("flag_id")),
    ("import_session", None),
    ("import_event", None),
    ("judge_call", None),
    ("judgment", None),
];

/// The change-file format. A computer that reads a higher number, or a store format above its own, holds the file
/// until it is updated rather than guess at it.
pub const FORMAT: i64 = 1;

/// One change file, as written: a header and its lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub device: String,
    pub seq: i64,
    pub schema: i64,
    pub lines: Vec<Value>,
}

impl Segment {
    /// JSON lines: the header first (`aiq_sync`, `device`, `seq`, `schema`, `rows`), then one line per row or delete.
    pub fn to_jsonl(&self, version: &str) -> String {
        let mut s = json!({"aiq_sync": FORMAT, "device": self.device, "seq": self.seq, "schema": self.schema,
                           "version": version, "rows": self.lines.len()})
        .to_string();
        s.push('\n');
        for l in &self.lines {
            s.push_str(&l.to_string());
            s.push('\n');
        }
        s
    }

    /// Parse a whole file. A file cut short (still being copied) or not JSON is an error, never a partial segment.
    pub fn from_jsonl(text: &str) -> std::result::Result<Self, String> {
        let mut it = text.lines().filter(|l| !l.trim().is_empty());
        let head: Value = serde_json::from_str(it.next().ok_or("empty change file")?).map_err(|e| e.to_string())?;
        let format = head["aiq_sync"].as_i64().ok_or("not an AIQ change file")?;
        if format > FORMAT {
            return Err(format!("change file format {format} is newer than this AIQ"));
        }
        let lines =
            it.map(serde_json::from_str).collect::<std::result::Result<Vec<Value>, _>>().map_err(|e| e.to_string())?;
        let rows = head["rows"].as_u64().ok_or("change file has no row count")? as usize;
        if rows != lines.len() {
            return Err(format!("change file is incomplete ({} of {rows} lines)", lines.len()));
        }
        Ok(Self {
            device: head["device"].as_str().ok_or("change file names no computer")?.to_owned(),
            seq: head["seq"].as_i64().ok_or("change file has no number")?,
            schema: head["schema"].as_i64().ok_or("change file has no store format")?,
            lines,
        })
    }
}

/// What applying one change file did.
#[derive(Debug, Clone, PartialEq)]
pub enum Applied {
    /// Applied: rows added (a row already here counts 0) and deletes repeated.
    Done { rows: u64, deletes: u64 },
    /// Already applied earlier: nothing changed.
    Already,
    /// From a newer AIQ (store format above this one): kept for after this computer updates.
    Held { schema: i64 },
    /// Not the next number from that computer: an earlier file has not arrived yet.
    Gap { want: i64 },
}

fn sql_value(v: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as V;
    match v {
        Value::Null => V::Null,
        Value::Bool(b) => V::Integer(i64::from(*b)),
        Value::Number(n) => n.as_i64().map_or_else(|| V::Real(n.as_f64().unwrap_or(0.0)), V::Integer),
        Value::String(s) => V::Text(s.clone()),
        other => V::Text(other.to_string()),
    }
}

fn row_json(r: &rusqlite::Row<'_>, names: &[String]) -> rusqlite::Result<Map<String, Value>> {
    use rusqlite::types::ValueRef;
    let mut obj = Map::with_capacity(names.len());
    for (i, name) in names.iter().enumerate() {
        let v = match r.get_ref(i)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(x) => x.into(),
            ValueRef::Real(x) => serde_json::Number::from_f64(x).map_or(Value::Null, Into::into),
            ValueRef::Text(s) => String::from_utf8_lossy(s).into_owned().into(),
            ValueRef::Blob(b) => b.iter().map(|x| format!("{x:02x}")).collect::<String>().into(),
        };
        obj.insert(name.clone(), v);
    }
    Ok(obj)
}

impl Store {
    /// This computer's sync id (made with the store at v12).
    pub fn device_id(&self) -> Result<String> {
        self.conn.query_row("SELECT device_id FROM sync_self LIMIT 1", [], |r| r.get(0)).map_err(err(codes::STORE_OPEN))
    }

    /// The highest change-file number this computer has written (0 = none yet).
    pub fn sync_last_export(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT coalesce(max(seq), 0) FROM sync_export", [], |r| r.get(0))
            .map_err(err(codes::STORE_OPEN))
    }

    /// Rows and deletes recorded here and not yet in a change file.
    pub fn sync_pending(&self) -> Result<u64> {
        self.conn
            .query_row(
                "SELECT (SELECT count(*) FROM sync_outbox WHERE n > coalesce((SELECT max(upto_row) FROM sync_export), 0))
                      + (SELECT count(*) FROM sync_delete WHERE computer IS NULL
                           AND n > coalesce((SELECT max(upto_delete) FROM sync_export), 0))",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(err(codes::STORE_OPEN))
    }

    /// Cut the next change file from rows and deletes recorded here since the last one, at most `max_lines`, hand it
    /// to `write` (which must store it whole, e.g. write a temp file then rename), and only then mark it written.
    /// Returns None when there is nothing new. If `write` fails nothing is marked, and the next call cuts the same
    /// file again with the same number.
    pub fn sync_export(
        &mut self,
        max_lines: usize,
        write: impl FnOnce(&Segment) -> std::io::Result<()>,
        now_ms: i64,
    ) -> Result<Option<Segment>> {
        let tx = self.conn.transaction().map_err(err(codes::STORE_OPEN))?;
        let (last_seq, upto_row, upto_del): (i64, i64, i64) = tx
            .query_row(
                "SELECT coalesce(max(seq), 0), coalesce(max(upto_row), 0), coalesce(max(upto_delete), 0) FROM sync_export",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(err(codes::STORE_OPEN))?;
        let device: String = tx
            .query_row("SELECT device_id FROM sync_self LIMIT 1", [], |r| r.get(0))
            .map_err(err(codes::STORE_OPEN))?;
        let schema: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(err(codes::STORE_OPEN))?;
        let mut lines = Vec::new();
        let mut new_row = upto_row;
        {
            let mut out = tx
                .prepare("SELECT n, tbl, row_id FROM sync_outbox WHERE n > ?1 ORDER BY n LIMIT ?2")
                .map_err(err(codes::STORE_OPEN))?;
            let queued: Vec<(i64, String, i64)> = out
                .query_map(params![upto_row, max_lines as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .and_then(|r| r.collect())
                .map_err(err(codes::STORE_OPEN))?;
            for (n, tbl, row_id) in queued {
                new_row = n;
                // Only known tables, so the name is never built from data.
                let Some((t, _)) = SYNCED.iter().find(|(t, _)| *t == tbl) else { continue };
                let mut st = tx
                    .prepare_cached(&format!("SELECT * FROM {t} WHERE rowid = ?1 AND computer IS NULL"))
                    .map_err(err(codes::STORE_OPEN))?;
                // Every column by position (a later store format adds columns after `computer`); `computer` is dropped below.
                let names: Vec<String> = st.column_names().map(str::to_owned).collect();
                // The row may be gone since (a deleted prompt, a re-imported session): then nothing to send.
                let row = st.query_row([row_id], |r| row_json(r, &names)).optional().map_err(err(codes::STORE_OPEN))?;
                if let Some(mut r) = row {
                    r.remove("computer");
                    lines.push(json!({"t": t, "r": r}));
                }
            }
        }
        let mut new_del = upto_del;
        {
            let mut st = tx
                .prepare("SELECT n, what, key FROM sync_delete WHERE n > ?1 AND computer IS NULL ORDER BY n")
                .map_err(err(codes::STORE_OPEN))?;
            let dels: Vec<(i64, String, String)> = st
                .query_map([upto_del], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .and_then(|r| r.collect())
                .map_err(err(codes::STORE_OPEN))?;
            for (n, what, key) in dels {
                new_del = n;
                lines.push(json!({"del": what, "key": key}));
            }
        }
        if new_row == upto_row && new_del == upto_del {
            return Ok(None);
        }
        let seg = Segment { device, seq: last_seq + 1, schema, lines };
        write(&seg).map_err(|e| StoreError { code: codes::SYNC_WRITE, detail: e.to_string() })?;
        tx.execute(
            "INSERT INTO sync_export (seq, upto_row, upto_delete, rows, utc_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![seg.seq, new_row, new_del, seg.lines.len() as i64, now_ms],
        )
        .map_err(err(codes::STORE_WRITE))?;
        tx.commit().map_err(err(codes::STORE_WRITE))?;
        Ok(Some(seg))
    }

    /// The last change-file number applied from `peer` (0 = none).
    pub fn sync_applied_upto(&self, peer: &str) -> Result<i64> {
        self.conn
            .query_row("SELECT coalesce(max(seq), 0) FROM sync_applied WHERE peer = ?1", [peer], |r| r.get(0))
            .map_err(err(codes::STORE_OPEN))
    }

    /// Per other computer: (id, last file applied, rows applied in all, when last applied).
    pub fn sync_peers(&self) -> Result<Vec<(String, i64, i64, i64)>> {
        let mut st = self
            .conn
            .prepare("SELECT peer, max(seq), sum(rows), max(utc_ms) FROM sync_applied GROUP BY peer ORDER BY peer")
            .map_err(err(codes::STORE_OPEN))?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .and_then(|r| r.collect())
            .map_err(err(codes::STORE_OPEN))
    }

    /// Apply one change file from another computer, all of it or none of it.
    pub fn sync_apply(&mut self, seg: &Segment, now_ms: i64) -> Result<Applied> {
        let me = self.device_id()?;
        if seg.device == me {
            return Ok(Applied::Already);
        }
        let done = self.sync_applied_upto(&seg.device)?;
        if seg.seq <= done {
            return Ok(Applied::Already);
        }
        if seg.seq != done + 1 {
            return Ok(Applied::Gap { want: done + 1 });
        }
        if seg.schema > self.user_version()? {
            return Ok(Applied::Held { schema: seg.schema });
        }
        let tx = self.conn.transaction().map_err(err(codes::SYNC_APPLY))?;
        let mut rows = 0u64;
        let mut deletes = 0u64;
        for line in &seg.lines {
            if let Some(what) = line["del"].as_str() {
                let key = line["key"].as_str().unwrap_or_default();
                apply_delete(&tx, what, key)?;
                tx.execute(
                    "INSERT INTO sync_delete (what, key, computer, utc_ms) VALUES (?1, ?2, ?3, ?4)",
                    params![what, key, seg.device, now_ms],
                )
                .map_err(err(codes::SYNC_APPLY))?;
                deletes += 1;
                continue;
            }
            let t = line["t"].as_str().unwrap_or_default();
            let Some((t, autoinc)) = SYNCED.iter().find(|(name, _)| *name == t) else {
                return Err(StoreError { code: codes::SYNC_APPLY, detail: format!("unknown table {t}") });
            };
            let Some(obj) = line["r"].as_object() else {
                return Err(StoreError { code: codes::SYNC_APPLY, detail: format!("{t} line has no row") });
            };
            let known: Vec<String> = {
                let mut st = tx.prepare_cached(&format!("PRAGMA table_info({t})")).map_err(err(codes::SYNC_APPLY))?;
                st.query_map([], |r| r.get::<_, String>(1)).and_then(|r| r.collect()).map_err(err(codes::SYNC_APPLY))?
            };
            let mut cols = Vec::new();
            let mut vals = Vec::new();
            for (k, v) in obj {
                if Some(k.as_str()) == *autoinc || k == "computer" {
                    continue;
                }
                if !known.iter().any(|c| c == k) {
                    // The header said this store's format or lower, so this cannot happen; never drop a value.
                    return Err(StoreError { code: codes::SYNC_APPLY, detail: format!("{t} has no column {k}") });
                }
                cols.push(k.clone());
                vals.push(sql_value(v));
            }
            if *t == "import_session" {
                replace_older_import(&tx, obj)?;
            }
            cols.push("computer".to_owned());
            vals.push(rusqlite::types::Value::Text(seg.device.clone()));
            let marks = vec!["?"; cols.len()].join(", ");
            let sql = format!("INSERT OR IGNORE INTO {t} ({}) VALUES ({marks})", cols.join(", "));
            let n = tx
                .execute(&sql, rusqlite::params_from_iter(vals))
                .map_err(|e| StoreError { code: codes::SYNC_APPLY, detail: format!("{t}: {e}") })?;
            rows += n as u64;
        }
        tx.execute(
            "INSERT INTO sync_applied (peer, seq, rows, utc_ms) VALUES (?1, ?2, ?3, ?4)",
            params![seg.device, seg.seq, rows as i64, now_ms],
        )
        .map_err(err(codes::SYNC_APPLY))?;
        tx.commit().map_err(err(codes::SYNC_APPLY))?;
        if deletes > 0 {
            let _ = self.conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
        }
        Ok(Applied::Done { rows, deletes })
    }

    /// Note a delete the person made here, so the other computers repeat it. A store before v12 has nowhere to note it.
    pub(crate) fn record_delete(conn: &rusqlite::Connection, what: &str, key: &str) -> Result<()> {
        let has: bool = conn
            .query_row("SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'sync_delete')", [], |r| r.get(0))
            .map_err(err(codes::STORE_WRITE))?;
        if has {
            conn.execute(
                "INSERT INTO sync_delete (what, key, utc_ms)
                 VALUES (?1, ?2, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))",
                params![what, key],
            )
            .map_err(err(codes::STORE_WRITE))?;
        }
        Ok(())
    }
}

/// Repeat a delete made on another computer. Same effect as the person's own delete here.
fn apply_delete(tx: &rusqlite::Transaction<'_>, what: &str, key: &str) -> Result<()> {
    match what {
        "content" => {
            tx.execute_batch("PRAGMA secure_delete = ON;").map_err(err(codes::SYNC_APPLY))?;
            tx.execute("DELETE FROM content WHERE event_id = ?1", [key]).map_err(err(codes::SYNC_APPLY))?;
        }
        "imported_all" => {
            super::forget_imported_in(tx).map_err(|e| StoreError { code: codes::SYNC_APPLY, ..e })?;
        }
        other => return Err(StoreError { code: codes::SYNC_APPLY, detail: format!("unknown delete {other}") }),
    }
    Ok(())
}

/// An imported session arriving again with a newer source file time replaces the copy here (its events go with it,
/// ON DELETE CASCADE). Returns true when an older copy was removed.
fn replace_older_import(tx: &rusqlite::Transaction<'_>, obj: &Map<String, Value>) -> Result<bool> {
    let (Some(tool), Some(sid), Some(mtime)) = (
        obj.get("tool").and_then(Value::as_str),
        obj.get("source_session_id").and_then(Value::as_str),
        obj.get("source_mtime_ms").and_then(Value::as_i64),
    ) else {
        return Ok(false);
    };
    let n = tx
        .execute(
            "DELETE FROM import_session WHERE tool = ?1 AND source_session_id = ?2 AND source_mtime_ms < ?3",
            params![tool, sid, mtime],
        )
        .map_err(err(codes::SYNC_APPLY))?;
    Ok(n > 0)
}
