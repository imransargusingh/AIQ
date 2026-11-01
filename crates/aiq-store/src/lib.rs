//! AIQ local store: one SQLite file per OS user (ADR-0002 shape: dimensions + append-only facts).
//!
//! Deviation (Imran, 2026-10-31): encryption is deferred. The file is plain SQLite, readable only by the
//! OS user (0600 on Unix; inherited user-only ACL under %USERPROFILE%\.aiq on Windows). ADR-0002/0003 flagged.
use aiq_contracts::codes;
use aiq_domain::clock::{Now, local_day};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

pub const SCHEMA_V1: &str = include_str!("schema_v1.sql");
pub const SCHEMA_V2: &str = include_str!("schema_v2.sql");
pub const SCHEMA_V3: &str = include_str!("schema_v3.sql");
/// Migration N takes the store from `user_version` N to N+1, in one transaction.
pub const MIGRATIONS: &[&str] = &[SCHEMA_V1, SCHEMA_V2, SCHEMA_V3];

#[derive(Debug, Clone, PartialEq)]
pub struct StoreError {
    pub code: &'static str,
    pub detail: String,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for StoreError {}

fn err(code: &'static str) -> impl Fn(rusqlite::Error) -> StoreError {
    move |e| StoreError { code, detail: e.to_string() }
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// One prompt row for the detector rebuild.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptRow {
    pub surface: String,
    pub session: String,
    pub event_id: String,
    pub ts_utc_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct NewEvent<'a> {
    pub event_id: &'a str,
    pub surface: &'a str,
    pub session: &'a str,
    pub kind: &'a str,
    pub ts_utc_ms: i64,
    pub prompt_id: Option<&'a str>,
    pub trace_id: Option<&'a str>,
    pub collector_version: &'a str,
    pub prompt_chars: Option<i64>,
    pub payload_meta: &'a str,
    pub text: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct NewSignal<'a> {
    pub signal_id: &'a str,
    pub task_id: &'a str,
    pub surface: &'a str,
    pub session: &'a str,
    pub detector_id: &'a str,
    pub detector_version: u32,
    pub pack_version: &'a str,
    pub opportunity_id: &'a str,
    pub reprompt_count: u32,
    pub score: f64,
    pub evidence: &'a [String],
}

#[derive(Debug, Clone)]
pub struct NewDecision<'a> {
    pub decision_id: &'a str,
    pub card_id: &'a str,
    pub signal_id: &'a str,
    pub surface: &'a str,
    pub session: &'a str,
    pub outcome: &'a str,
    pub reason_code: &'a str,
    pub rule_id: &'a str,
    pub pack_version: &'a str,
    pub card_json: &'a str,
}

/// A show-ready card that is neither delivered nor expired.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadyCard {
    pub card_id: String,
    pub signal_id: String,
    pub surface: String,
    pub session: String,
    pub card_json: String,
    pub decided_utc_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardRecord {
    pub card_json: String,
    pub outcome: String,
    pub reason_code: String,
    pub delivered: bool,
    /// (event ts, text or None when the content row was deleted), in evidence order.
    pub evidence: Vec<(i64, Option<String>)>,
}

pub struct Store {
    conn: Connection,
    path: PathBuf,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with(path, MIGRATIONS)
    }

    /// Open (creating a user-only file if needed) and migrate with the given migration list.
    pub fn open_with(path: &Path, migrations: &[&str]) -> Result<Self> {
        create_user_only(path).map_err(|e| StoreError { code: codes::STORE_OPEN, detail: e.to_string() })?;
        let conn = Connection::open(path).map_err(err(codes::STORE_OPEN))?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .map_err(err(codes::STORE_OPEN))?;
        let mut s = Self { conn, path: path.to_owned() };
        s.migrate(migrations)?;
        Ok(s)
    }

    /// Open an existing store for reading only (AIQ home): no create, no migration, no writes.
    /// `query_only` guards against a write even if a query tried one. WAL lets this read while the core writes.
    pub fn open_read_only(path: &Path) -> Result<Self> {
        use rusqlite::OpenFlags;
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI;
        let conn = Connection::open_with_flags(path, flags).map_err(err(codes::STORE_OPEN))?;
        conn.busy_timeout(std::time::Duration::from_secs(1)).map_err(err(codes::STORE_OPEN))?;
        conn.execute_batch("PRAGMA query_only = ON;").map_err(err(codes::STORE_OPEN))?;
        Ok(Self { conn, path: path.to_owned() })
    }

    /// The table exists (a store written by an older AIQ may not have every table yet).
    pub fn has_table(&self, name: &str) -> Result<bool> {
        self.conn
            .query_row("SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)", [name], |r| {
                r.get(0)
            })
            .map_err(err(codes::STORE_OPEN))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn user_version(&self) -> Result<i64> {
        self.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(err(codes::STORE_OPEN))
    }

    fn migrate(&mut self, migrations: &[&str]) -> Result<()> {
        let from = self.user_version()? as usize;
        for (i, sql) in migrations.iter().enumerate().skip(from) {
            let tx = self.conn.transaction().map_err(err(codes::STORE_MIGRATION))?;
            tx.execute_batch(sql).map_err(err(codes::STORE_MIGRATION))?;
            tx.pragma_update(None, "user_version", (i + 1) as i64).map_err(err(codes::STORE_MIGRATION))?;
            tx.commit().map_err(err(codes::STORE_MIGRATION))?;
        }
        Ok(())
    }

    /// Idempotent ingest: returns false when `event_id` was already stored (replay, retry).
    pub fn insert_event(&mut self, e: &NewEvent<'_>, now_ms: i64) -> Result<bool> {
        let tx = self.conn.transaction().map_err(err(codes::STORE_WRITE))?;
        tx.execute(
            "INSERT INTO surface_session (surface, surface_session_id, first_seen_utc_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT DO NOTHING",
            params![e.surface, e.session, now_ms],
        )
        .map_err(err(codes::STORE_WRITE))?;
        let n = tx
            .execute(
                "INSERT INTO event (event_id, surface, surface_session_id, kind, ts_utc_ms, received_utc_ms, prompt_id,
                                    trace_id, collector_version, prompt_chars, payload_meta)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) ON CONFLICT (event_id) DO NOTHING",
                params![
                    e.event_id,
                    e.surface,
                    e.session,
                    e.kind,
                    e.ts_utc_ms,
                    now_ms,
                    e.prompt_id,
                    e.trace_id,
                    e.collector_version,
                    e.prompt_chars,
                    e.payload_meta
                ],
            )
            .map_err(err(codes::STORE_WRITE))?;
        if n == 1
            && let Some(t) = e.text
        {
            tx.execute("INSERT INTO content (event_id, text) VALUES (?1, ?2)", params![e.event_id, t])
                .map_err(err(codes::STORE_WRITE))?;
        }
        tx.commit().map_err(err(codes::STORE_WRITE))?;
        Ok(n == 1)
    }

    /// Has the prompt event of this turn arrived? (Stop and UserPromptSubmit share `prompt_id`.)
    pub fn prompt_seen(&self, surface: &str, session: &str, prompt_id: &str) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM event WHERE surface = ?1 AND surface_session_id = ?2
                                AND prompt_id = ?3 AND kind = 'prompt')",
                params![surface, session, prompt_id],
                |r| r.get(0),
            )
            .map_err(err(codes::STORE_OPEN))
    }

    pub fn insert_signal(&self, s: &NewSignal<'_>, now_ms: i64) -> Result<()> {
        let ev = serde_json::to_string(s.evidence).expect("strings serialize");
        self.conn
            .execute(
                "INSERT INTO signal (signal_id, task_id, surface, surface_session_id, detector_id, detector_version,
                                     pack_version, opportunity_id, reprompt_count, score, evidence, utc_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    s.signal_id,
                    s.task_id,
                    s.surface,
                    s.session,
                    s.detector_id,
                    s.detector_version,
                    s.pack_version,
                    s.opportunity_id,
                    s.reprompt_count,
                    s.score,
                    ev,
                    now_ms
                ],
            )
            .map(|_| ())
            .map_err(err(codes::STORE_WRITE))
    }

    pub fn insert_decision(&self, d: &NewDecision<'_>, now_ms: i64) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO card_decision (decision_id, card_id, signal_id, surface, surface_session_id, outcome,
                                            reason_code, rule_id, pack_version, card_json, utc_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    d.decision_id,
                    d.card_id,
                    d.signal_id,
                    d.surface,
                    d.session,
                    d.outcome,
                    d.reason_code,
                    d.rule_id,
                    d.pack_version,
                    d.card_json,
                    now_ms
                ],
            )
            .map(|_| ())
            .map_err(err(codes::STORE_WRITE))
    }

    const READY_SQL: &str = "SELECT d.card_id, d.signal_id, d.surface, d.surface_session_id, d.card_json, d.utc_ms
         FROM card_decision d
         WHERE d.outcome = 'show_ready'
           AND NOT EXISTS (SELECT 1 FROM card_delivery v WHERE v.card_id = d.card_id)
           AND NOT EXISTS (SELECT 1 FROM card_decision x WHERE x.card_id = d.card_id AND x.outcome <> 'show_ready')";

    fn ready_rows(&self, filter: &str, p: &[&dyn rusqlite::ToSql]) -> Result<Vec<ReadyCard>> {
        let sql = format!("{} {filter} ORDER BY d.utc_ms", Self::READY_SQL);
        let mut st = self.conn.prepare(&sql).map_err(err(codes::STORE_OPEN))?;
        let rows = st
            .query_map(p, |r| {
                Ok(ReadyCard {
                    card_id: r.get(0)?,
                    signal_id: r.get(1)?,
                    surface: r.get(2)?,
                    session: r.get(3)?,
                    card_json: r.get(4)?,
                    decided_utc_ms: r.get(5)?,
                })
            })
            .map_err(err(codes::STORE_OPEN))?;
        rows.collect::<std::result::Result<_, _>>().map_err(err(codes::STORE_OPEN))
    }

    pub fn ready_card(&self, surface: &str, session: &str) -> Result<Option<ReadyCard>> {
        Ok(self
            .ready_rows("AND d.surface = ?1 AND d.surface_session_id = ?2", &[&surface, &session])?
            .into_iter()
            .next())
    }

    /// Show-ready cards decided before `before_ms` and still undelivered (to expire).
    pub fn stale_ready(&self, before_ms: i64) -> Result<Vec<ReadyCard>> {
        self.ready_rows("AND d.utc_ms < ?1", &[&before_ms])
    }

    pub fn delivered_in_session(&self, surface: &str, session: &str) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM card_delivery WHERE surface = ?1 AND surface_session_id = ?2)",
                params![surface, session],
                |r| r.get(0),
            )
            .map_err(err(codes::STORE_OPEN))
    }

    /// Time of the last delivery on any surface (the person-wide quiet period reads it).
    pub fn last_delivery_ms(&self) -> Result<Option<i64>> {
        self.conn
            .query_row("SELECT max(delivered_utc_ms) FROM card_delivery", [], |r| r.get(0))
            .map_err(err(codes::STORE_OPEN))
    }

    /// The session's latest event kind (`prompt` / `stop`), by event time then arrival.
    pub fn last_event_kind(&self, surface: &str, session: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT kind FROM event WHERE surface = ?1 AND surface_session_id = ?2
                 ORDER BY ts_utc_ms DESC, event_id DESC LIMIT 1",
                params![surface, session],
                |r| r.get(0),
            )
            .optional()
            .map_err(err(codes::STORE_OPEN))
    }

    /// The newest session of a surface that has an event (used when a renderer cannot name its session).
    pub fn latest_session(&self, surface: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT surface_session_id FROM event WHERE surface = ?1 ORDER BY ts_utc_ms DESC, event_id DESC LIMIT 1",
                [surface],
                |r| r.get(0),
            )
            .optional()
            .map_err(err(codes::STORE_OPEN))
    }

    /// Show-ready cards of a surface, oldest first.
    pub fn ready_cards_on(&self, surface: &str) -> Result<Vec<ReadyCard>> {
        self.ready_rows("AND d.surface = ?1", &[&surface])
    }

    /// The card delivered in this session, if any: (card_id, card_json, delivered_utc_ms).
    pub fn delivered_card(&self, surface: &str, session: &str) -> Result<Option<(String, String, i64)>> {
        self.conn
            .query_row(
                "SELECT v.card_id, d.card_json, v.delivered_utc_ms FROM card_delivery v
                 JOIN card_decision d ON d.card_id = v.card_id AND d.outcome = 'show_ready'
                 WHERE v.surface = ?1 AND v.surface_session_id = ?2 LIMIT 1",
                params![surface, session],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(err(codes::STORE_OPEN))
    }

    /// Prompts in the session with an event time after `after_ms`.
    pub fn prompts_after(&self, surface: &str, session: &str, after_ms: i64) -> Result<u32> {
        self.conn
            .query_row(
                "SELECT count(*) FROM event WHERE surface = ?1 AND surface_session_id = ?2 AND kind = 'prompt'
                 AND ts_utc_ms > ?3",
                params![surface, session, after_ms],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as u32)
            .map_err(err(codes::STORE_OPEN))
    }

    pub fn insert_folder_rule(
        &self,
        folder: &str,
        action: &str,
        reason: &str,
        source: &str,
        now_ms: i64,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO folder_rule (folder, action, reason, source, utc_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![folder, action, reason, source, now_ms],
            )
            .map(|_| ())
            .map_err(err(codes::STORE_WRITE))
    }

    /// Folders set aside now: the newest rule per folder is `set_aside`. (folder, reason, utc_ms), newest first.
    pub fn set_aside_folders(&self) -> Result<Vec<(String, String, i64)>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT folder, reason, utc_ms FROM folder_rule r WHERE action = 'set_aside' AND rule_id = \
                 (SELECT MAX(rule_id) FROM folder_rule WHERE folder = r.folder) ORDER BY rule_id DESC",
            )
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(err(codes::STORE_OPEN))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err(codes::STORE_OPEN))?;
        Ok(rows)
    }

    /// The store already holds events for this session.
    pub fn session_known(&self, surface: &str, session: &str) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM surface_session WHERE surface = ?1 AND surface_session_id = ?2)",
                params![surface, session],
                |r| r.get(0),
            )
            .map_err(err(codes::STORE_OPEN))
    }

    pub fn insert_open(&self, card_id: &str, surface: &str, now_ms: i64) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO card_open (card_id, surface, utc_ms) VALUES (?1, ?2, ?3)",
                params![card_id, surface, now_ms],
            )
            .map(|_| ())
            .map_err(err(codes::STORE_WRITE))
    }

    pub fn opened(&self, card_id: &str) -> Result<bool> {
        self.conn
            .query_row("SELECT EXISTS (SELECT 1 FROM card_open WHERE card_id = ?1)", [card_id], |r| r.get(0))
            .map_err(err(codes::STORE_OPEN))
    }

    /// Local day of every delivery (derived from the stored UTC time and offset; never stored twice).
    pub fn delivery_days(&self) -> Result<Vec<i64>> {
        let mut st = self
            .conn
            .prepare("SELECT delivered_utc_ms, local_offset_min FROM card_delivery")
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st.query_map([], |r| Ok(local_day(r.get(0)?, r.get(1)?))).map_err(err(codes::STORE_OPEN))?;
        rows.collect::<std::result::Result<_, _>>().map_err(err(codes::STORE_OPEN))
    }

    /// Record the hand-over. A second delivery in the same session fails with `AIQ-DELIVERY-CONFLICT`.
    pub fn insert_delivery(&self, card_id: &str, surface: &str, session: &str, now: Now) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO card_delivery (card_id, surface, surface_session_id, delivered_utc_ms, local_offset_min)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![card_id, surface, session, now.utc_ms, now.offset_min],
            )
            .map(|_| ())
            .map_err(|e| match e.sqlite_error_code() {
                Some(rusqlite::ErrorCode::ConstraintViolation) => {
                    StoreError { code: codes::DELIVERY_CONFLICT, detail: e.to_string() }
                }
                _ => StoreError { code: codes::STORE_WRITE, detail: e.to_string() },
            })
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM current_setting WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(err(codes::STORE_OPEN))
    }

    pub fn set_setting(&self, key: &str, value: &str, now_ms: i64) -> Result<()> {
        self.conn
            .execute("INSERT INTO setting_change (key, value, utc_ms) VALUES (?1, ?2, ?3)", params![key, value, now_ms])
            .map(|_| ())
            .map_err(err(codes::STORE_WRITE))
    }

    /// Prompts (with text) of sessions that had a prompt at or after `since_ms`, in event order.
    pub fn prompts_for_rebuild(&self, since_ms: i64) -> Result<Vec<PromptRow>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT e.surface, e.surface_session_id, e.event_id, e.ts_utc_ms, c.text
                 FROM event e JOIN content c ON c.event_id = e.event_id
                 WHERE e.kind = 'prompt' AND (e.surface, e.surface_session_id) IN
                   (SELECT surface, surface_session_id FROM event WHERE kind = 'prompt' AND ts_utc_ms >= ?1)
                 ORDER BY e.event_id",
            )
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st
            .query_map([since_ms], |r| {
                Ok(PromptRow {
                    surface: r.get(0)?,
                    session: r.get(1)?,
                    event_id: r.get(2)?,
                    ts_utc_ms: r.get(3)?,
                    text: r.get(4)?,
                })
            })
            .map_err(err(codes::STORE_OPEN))?;
        rows.collect::<std::result::Result<_, _>>().map_err(err(codes::STORE_OPEN))
    }

    pub fn card(&self, card_id: &str) -> Result<Option<CardRecord>> {
        let row: Option<(String, String, String, String)> = self
            .conn
            .query_row(
                "SELECT card_json, outcome, reason_code, signal_id FROM card_decision WHERE card_id = ?1
                 ORDER BY utc_ms DESC, rowid DESC LIMIT 1",
                [card_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(err(codes::STORE_OPEN))?;
        let Some((card_json, outcome, reason_code, signal_id)) = row else { return Ok(None) };
        let delivered: bool = self
            .conn
            .query_row("SELECT EXISTS (SELECT 1 FROM card_delivery WHERE card_id = ?1)", [card_id], |r| r.get(0))
            .map_err(err(codes::STORE_OPEN))?;
        let evidence_json: String = self
            .conn
            .query_row("SELECT evidence FROM signal WHERE signal_id = ?1", [&signal_id], |r| r.get(0))
            .map_err(err(codes::STORE_OPEN))?;
        let ids: Vec<String> = serde_json::from_str(&evidence_json).unwrap_or_default();
        let mut evidence = Vec::with_capacity(ids.len());
        for id in ids {
            let row: Option<(i64, Option<String>)> = self
                .conn
                .query_row(
                    "SELECT e.ts_utc_ms, c.text FROM event e LEFT JOIN content c ON c.event_id = e.event_id
                     WHERE e.event_id = ?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(err(codes::STORE_OPEN))?;
            if let Some(r) = row {
                evidence.push(r);
            }
        }
        Ok(Some(CardRecord { card_json, outcome, reason_code, delivered, evidence }))
    }

    pub fn events_by_kind(&self) -> Result<Vec<(String, u64)>> {
        self.pairs("SELECT kind, count(*) FROM event GROUP BY kind ORDER BY kind")
    }

    pub fn signal_count(&self) -> Result<u64> {
        self.conn
            .query_row("SELECT count(*) FROM signal", [], |r| r.get::<_, i64>(0))
            .map(|n| n as u64)
            .map_err(err(codes::STORE_OPEN))
    }

    /// Current state per card (latest decision, or `shown` once delivered), counted by (state, reason).
    pub fn decisions(&self) -> Result<Vec<(String, String, u64)>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT CASE WHEN EXISTS (SELECT 1 FROM card_delivery v WHERE v.card_id = d.card_id)
                             THEN 'shown' ELSE outcome END AS state, reason_code, count(*)
                 FROM card_decision d
                 WHERE rowid = (SELECT max(rowid) FROM card_decision x WHERE x.card_id = d.card_id)
                 GROUP BY state, reason_code ORDER BY state, reason_code",
            )
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u64)))
            .map_err(err(codes::STORE_OPEN))?;
        rows.collect::<std::result::Result<_, _>>().map_err(err(codes::STORE_OPEN))
    }

    fn pairs(&self, sql: &str) -> Result<Vec<(String, u64)>> {
        let mut st = self.conn.prepare(sql).map_err(err(codes::STORE_OPEN))?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64))).map_err(err(codes::STORE_OPEN))?;
        rows.collect::<std::result::Result<_, _>>().map_err(err(codes::STORE_OPEN))
    }

    /// Bytes on disk for the store, including the WAL.
    pub fn size_bytes(&self) -> u64 {
        let wal = PathBuf::from(format!("{}-wal", self.path.display()));
        [self.path.as_path(), wal.as_path()].iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum()
    }

    /// Raw connection: read models (AIQ home) and tests that must attack the schema directly.
    pub fn raw(&self) -> &Connection {
        &self.conn
    }
}

/// Create the file readable and writable by the owner only, if it does not exist yet.
pub fn create_user_only(path: &Path) -> std::io::Result<()> {
    let mut o = std::fs::OpenOptions::new();
    o.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path).map(|_| ())
}

/// Remove the store file and its SQLite side files. Missing files are fine.
pub fn delete_store_files(path: &Path) -> std::io::Result<()> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let p = PathBuf::from(format!("{}{suffix}", path.display()));
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
