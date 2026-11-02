//! AIQ local store: one SQLite file per OS user (ADR-0002 shape: dimensions + append-only facts).
//!
//! Deviation (Imran, 2026-10-31): encryption is deferred. The file is plain SQLite, readable only by the
//! OS user (0600 on Unix; inherited user-only ACL under %USERPROFILE%\.aiq on Windows). ADR-0002/0003 flagged.
use aiq_contracts::codes;
use aiq_domain::clock::{Now, local_day};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const SCHEMA_V1: &str = include_str!("schema_v1.sql");
pub const SCHEMA_V2: &str = include_str!("schema_v2.sql");
pub const SCHEMA_V3: &str = include_str!("schema_v3.sql");
pub const SCHEMA_V4: &str = include_str!("schema_v4.sql");
pub const SCHEMA_V5: &str = include_str!("schema_v5.sql");
pub const SCHEMA_V6: &str = include_str!("schema_v6.sql");
/// Migration N takes the store from `user_version` N to N+1, in one transaction.
pub const MIGRATIONS: &[&str] = &[SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6];

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

/// One imported source session and its events (history import, schema v3).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedSession {
    pub tool: String,
    pub source_session_id: String,
    pub source_mtime_ms: i64,
    pub folder: String,
    pub started_utc_ms: Option<i64>,
    pub human_turns: u32,
    pub non_interactive: Option<bool>,
    /// Stable hash of the normalised first prompt (the automated rule's template signal).
    pub template_key: Option<i64>,
    /// `imported`, `automated` or `error`.
    pub outcome: String,
    pub why: String,
    pub events: Vec<ImportedEvent>,
}

/// What an earlier run stored about one source session: enough to re-apply the automated rule
/// without re-reading the file.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownImport {
    pub tool: String,
    pub source_session_id: String,
    pub source_mtime_ms: i64,
    pub folder: String,
    pub started_utc_ms: Option<i64>,
    pub human_turns: u32,
    pub non_interactive: Option<bool>,
    pub template_key: Option<i64>,
    pub outcome: String,
    /// The automated rule's signals at the last run (`NI,T,B,F,RC`); '' when imported.
    pub why: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedEvent {
    /// `prompt` or `stop`.
    pub kind: String,
    pub ts_utc_ms: Option<i64>,
    pub text: Option<String>,
    pub actions: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportRunCounts {
    pub found: u64,
    pub imported: u64,
    pub automated: u64,
    pub unchanged: u64,
    pub errors: u64,
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
    /// The signal's detector (`reprompt`, `task_type`).
    pub detector_id: String,
    /// The signal's evidence ids as stored (event ids, or judged task ids for `task_type`).
    pub evidence_ids: Vec<String>,
    /// (event ts, text or None when the content row was deleted), in evidence order. Empty for `task_type`
    /// signals: their evidence is other tasks, whose text a card never shows (design coaching-from-rungs B-J).
    pub evidence: Vec<(i64, Option<String>)>,
}

/// One call to the person's provider (schema v5 `judge_call`).
#[derive(Debug, Clone, PartialEq)]
pub struct NewJudgeCall<'a> {
    pub call_id: &'a str,
    pub task_id: &'a str,
    pub run: u8,
    pub provider: &'a str,
    pub model: Option<&'a str>,
    pub started_utc_ms: i64,
    pub latency_ms: i64,
    pub outcome: &'a str,
    pub code: Option<&'a str>,
    pub input_chars: i64,
    pub tool_session_id: Option<&'a str>,
    pub cost_usd: Option<f64>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
}

/// One judgment version (schema v5 `judgment`). JSON columns are passed already serialised.
#[derive(Debug, Clone, PartialEq)]
pub struct JudgmentRow {
    pub judgment_id: String,
    pub task_id: String,
    pub origin: String,
    pub provider: String,
    pub model: Option<String>,
    pub judged_utc_ms: i64,
    pub job: String,
    pub job_confidence: f64,
    pub model_rung: u8,
    pub current_rung: u8,
    pub current_confidence: f64,
    pub potential_rung: u8,
    pub potential_confidence: f64,
    pub ceiling_rung: u8,
    pub state: String,
    pub coachable: bool,
    pub facts: String,
    pub evidence: String,
    pub gap: String,
    pub blockers: String,
    pub placement: String,
    pub read: String,
    pub call_ids: String,
    pub fact_set_version: String,
    pub rubric_version: String,
    pub judge_version: String,
    /// Schema v6: the task type within the job; None when judged before v6 (or job unclassified).
    pub task_type: Option<String>,
    pub task_type_confidence: Option<f64>,
}

/// One live task's type guess (schema v6 `type_guess`).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeGuessRow {
    pub guess_id: String,
    pub task_id: String,
    pub surface: String,
    pub job: String,
    pub task_type: String,
    pub similarity: f64,
    /// Judged task ids the guess leaned on (never text).
    pub neighbours: Vec<String>,
    pub utc_ms: i64,
}

/// One answer to a tip (schema v6 `tip_feedback`).
#[derive(Debug, Clone, PartialEq)]
pub struct TipFeedbackRow {
    pub card_id: Option<String>,
    pub tip_id: String,
    pub answer: String,
    pub surface: String,
    pub utc_ms: i64,
}

/// One "This doesn't fit" flag or undo (schema v6 `guidance_flag`).
#[derive(Debug, Clone, PartialEq)]
pub struct GuidanceFlagRow {
    pub job: String,
    pub task_type: String,
    pub move_id: String,
    pub action: String,
    pub reason: Option<String>,
    pub note: Option<String>,
    pub utc_ms: i64,
}

/// A judge call as the queue needs it: when, and whether it worked.
#[derive(Debug, Clone, PartialEq)]
pub struct CallOutcome {
    pub task_id: String,
    pub started_utc_ms: i64,
    pub outcome: String,
    pub cost_usd: Option<f64>,
}

const JUDGMENT_COLS: &str = "judgment_id, task_id, origin, provider, model, judged_utc_ms, job, job_confidence,
    model_rung, current_rung, current_confidence, potential_rung, potential_confidence, ceiling_rung, state,
    coachable, facts, evidence, gap, blockers, placement, read, call_ids, fact_set_version, rubric_version,
    judge_version";

fn judgment_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<JudgmentRow> {
    Ok(JudgmentRow {
        judgment_id: r.get(0)?,
        task_id: r.get(1)?,
        origin: r.get(2)?,
        provider: r.get(3)?,
        model: r.get(4)?,
        judged_utc_ms: r.get(5)?,
        job: r.get(6)?,
        job_confidence: r.get(7)?,
        model_rung: r.get(8)?,
        current_rung: r.get(9)?,
        current_confidence: r.get(10)?,
        potential_rung: r.get(11)?,
        potential_confidence: r.get(12)?,
        ceiling_rung: r.get(13)?,
        state: r.get(14)?,
        coachable: r.get(15)?,
        facts: r.get(16)?,
        evidence: r.get(17)?,
        gap: r.get(18)?,
        blockers: r.get(19)?,
        placement: r.get(20)?,
        read: r.get(21)?,
        call_ids: r.get(22)?,
        fact_set_version: r.get(23)?,
        rubric_version: r.get(24)?,
        judge_version: r.get(25)?,
        task_type: r.get(26)?,
        task_type_confidence: r.get(27)?,
    })
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
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000;")
            .map_err(err(codes::STORE_OPEN))?;
        let mut s = Self { conn, path: path.to_owned() };
        s.migrate(migrations)?;
        Ok(s)
    }

    /// Open an existing store for reading only (AIQ home, `aiq status`, import counts): never creates the
    /// file, no migration, no writes.
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

    /// The table has the column (a store an older AIQ wrote may lack later columns).
    pub fn has_column(&self, table: &str, column: &str) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
                params![table, column],
                |r| r.get(0),
            )
            .map_err(err(codes::STORE_OPEN))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    // ------------------------------------------------------------ coaching facts (schema v6)

    pub fn insert_type_guess(&self, g: &TypeGuessRow) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO type_guess (guess_id, task_id, surface, job, task_type, similarity, neighbours, utc_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    g.guess_id,
                    g.task_id,
                    g.surface,
                    g.job,
                    g.task_type,
                    g.similarity,
                    serde_json::to_string(&g.neighbours).expect("strings serialize"),
                    g.utc_ms
                ],
            )
            .map(|_| ())
            .map_err(err(codes::STORE_WRITE))
    }

    /// Every type guess, oldest first. Empty before v6.
    pub fn type_guesses(&self) -> Result<Vec<TypeGuessRow>> {
        if !self.has_table("type_guess")? {
            return Ok(vec![]);
        }
        let mut st = self
            .conn
            .prepare(
                "SELECT guess_id, task_id, surface, job, task_type, similarity, neighbours, utc_ms FROM type_guess
                 ORDER BY utc_ms, rowid",
            )
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st
            .query_map([], |r| {
                let n: String = r.get(6)?;
                Ok(TypeGuessRow {
                    guess_id: r.get(0)?,
                    task_id: r.get(1)?,
                    surface: r.get(2)?,
                    job: r.get(3)?,
                    task_type: r.get(4)?,
                    similarity: r.get(5)?,
                    neighbours: serde_json::from_str(&n).unwrap_or_default(),
                    utc_ms: r.get(7)?,
                })
            })
            .map_err(err(codes::STORE_OPEN))?;
        rows.collect::<rusqlite::Result<_>>().map_err(err(codes::STORE_OPEN))
    }

    pub fn insert_tip_feedback(&self, f: &TipFeedbackRow) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO tip_feedback (card_id, tip_id, answer, surface, utc_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![f.card_id, f.tip_id, f.answer, f.surface, f.utc_ms],
            )
            .map(|_| ())
            .map_err(err(codes::STORE_WRITE))
    }

    /// Every tip answer, oldest first. Empty before v6.
    pub fn tip_feedback_all(&self) -> Result<Vec<TipFeedbackRow>> {
        if !self.has_table("tip_feedback")? {
            return Ok(vec![]);
        }
        let mut st = self
            .conn
            .prepare("SELECT card_id, tip_id, answer, surface, utc_ms FROM tip_feedback ORDER BY utc_ms, feedback_id")
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st
            .query_map([], |r| {
                Ok(TipFeedbackRow {
                    card_id: r.get(0)?,
                    tip_id: r.get(1)?,
                    answer: r.get(2)?,
                    surface: r.get(3)?,
                    utc_ms: r.get(4)?,
                })
            })
            .map_err(err(codes::STORE_OPEN))?;
        rows.collect::<rusqlite::Result<_>>().map_err(err(codes::STORE_OPEN))
    }

    pub fn insert_guidance_flag(&self, f: &GuidanceFlagRow) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO guidance_flag (job, task_type, move, action, reason, note, utc_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![f.job, f.task_type, f.move_id, f.action, f.reason, f.note, f.utc_ms],
            )
            .map(|_| ())
            .map_err(err(codes::STORE_WRITE))
    }

    /// Every flag and undo, oldest first. Empty before v6.
    pub fn guidance_flags_all(&self) -> Result<Vec<GuidanceFlagRow>> {
        if !self.has_table("guidance_flag")? {
            return Ok(vec![]);
        }
        let mut st = self
            .conn
            .prepare(
                "SELECT job, task_type, move, action, reason, note, utc_ms FROM guidance_flag ORDER BY utc_ms, flag_id",
            )
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st
            .query_map([], |r| {
                Ok(GuidanceFlagRow {
                    job: r.get(0)?,
                    task_type: r.get(1)?,
                    move_id: r.get(2)?,
                    action: r.get(3)?,
                    reason: r.get(4)?,
                    note: r.get(5)?,
                    utc_ms: r.get(6)?,
                })
            })
            .map_err(err(codes::STORE_OPEN))?;
        rows.collect::<rusqlite::Result<_>>().map_err(err(codes::STORE_OPEN))
    }

    /// A task's first prompt: (time, text). Live task ids are the first prompt's event id; imported ids are
    /// `import:<tool>:<session>:<seq>`. Text is None when deleted. None when the task is unknown.
    pub fn task_first_prompt(&self, task_id: &str) -> Result<Option<(i64, Option<String>)>> {
        if let Some(rest) = task_id.strip_prefix("import:") {
            let mut parts = rest.rsplitn(2, ':');
            let (Some(seq), Some(head)) = (parts.next().and_then(|s| s.parse::<i64>().ok()), parts.next()) else {
                return Ok(None);
            };
            let Some((tool, sid)) = head.split_once(':') else { return Ok(None) };
            return self
                .conn
                .query_row(
                    "SELECT ts_utc_ms, text FROM import_event WHERE tool = ?1 AND source_session_id = ?2 AND seq = ?3",
                    params![tool, sid, seq],
                    |r| Ok((r.get::<_, Option<i64>>(0)?.unwrap_or(0), r.get(1)?)),
                )
                .optional()
                .map_err(err(codes::STORE_OPEN));
        }
        self.conn
            .query_row(
                "SELECT e.ts_utc_ms, c.text FROM event e LEFT JOIN content c ON c.event_id = e.event_id
                 WHERE e.event_id = ?1",
                [task_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(err(codes::STORE_OPEN))
    }

    /// Delivery times of cards carrying `tip_id`, newest first (the per-move cooldown reads the first).
    pub fn deliveries_of_tip(&self, tip_id: &str) -> Result<Vec<i64>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT v.delivered_utc_ms FROM card_delivery v
                 JOIN card_decision d ON d.card_id = v.card_id AND d.outcome = 'show_ready'
                 WHERE json_extract(d.card_json, '$.tip_id') = ?1 ORDER BY v.delivered_utc_ms DESC",
            )
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st.query_map([tip_id], |r| r.get(0)).map_err(err(codes::STORE_OPEN))?;
        rows.collect::<rusqlite::Result<_>>().map_err(err(codes::STORE_OPEN))
    }

    /// A cheap fingerprint of everything the coaching cache is derived from: when it changes, rebuild.
    pub fn coach_fingerprint(&self) -> Result<(i64, i64, i64)> {
        if !self.has_table("guidance_flag")? {
            return Ok((0, 0, 0));
        }
        self.conn
            .query_row(
                "SELECT (SELECT coalesce(max(rowid), 0) FROM judgment), (SELECT coalesce(max(flag_id), 0) FROM guidance_flag),
                        (SELECT coalesce(max(feedback_id), 0) FROM tip_feedback)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(err(codes::STORE_OPEN))
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

    /// Every folder's current rule: the newest action per folder (`set_aside` or `include`).
    pub fn folder_rules_now(&self) -> Result<HashMap<String, String>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT folder, action FROM folder_rule r WHERE rule_id = \
                 (SELECT MAX(rule_id) FROM folder_rule WHERE folder = r.folder)",
            )
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(err(codes::STORE_OPEN))?;
        rows.collect::<std::result::Result<_, _>>().map_err(err(codes::STORE_OPEN))
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
        let (evidence_json, detector_id): (String, String) = self
            .conn
            .query_row("SELECT evidence, detector_id FROM signal WHERE signal_id = ?1", [&signal_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(err(codes::STORE_OPEN))?;
        let ids: Vec<String> = serde_json::from_str(&evidence_json).unwrap_or_default();
        let mut evidence = Vec::with_capacity(ids.len());
        for id in ids.iter().filter(|_| detector_id == "reprompt") {
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
        Ok(Some(CardRecord { card_json, outcome, reason_code, delivered, detector_id, evidence_ids: ids, evidence }))
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

    /// Every source session an earlier run stored.
    pub fn known_imports(&self) -> Result<Vec<KnownImport>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT tool, source_session_id, source_mtime_ms, folder, started_utc_ms, human_turns, non_interactive,
                        template_key, outcome, why
                 FROM import_session",
            )
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st
            .query_map([], |r| {
                Ok(KnownImport {
                    tool: r.get(0)?,
                    source_session_id: r.get(1)?,
                    source_mtime_ms: r.get(2)?,
                    folder: r.get(3)?,
                    started_utc_ms: r.get(4)?,
                    human_turns: r.get(5)?,
                    non_interactive: r.get(6)?,
                    template_key: r.get(7)?,
                    outcome: r.get(8)?,
                    why: r.get(9)?,
                })
            })
            .map_err(err(codes::STORE_OPEN))?;
        rows.collect::<std::result::Result<_, _>>().map_err(err(codes::STORE_OPEN))
    }

    /// Write one source session and its events, replacing what an earlier run stored for it.
    pub fn put_import_session(&mut self, run_id: &str, s: &ImportedSession) -> Result<()> {
        let tx = self.conn.transaction().map_err(err(codes::STORE_WRITE))?;
        tx.execute(
            "DELETE FROM import_session WHERE tool = ?1 AND source_session_id = ?2",
            params![s.tool, s.source_session_id],
        )
        .map_err(err(codes::STORE_WRITE))?;
        tx.execute(
            "INSERT INTO import_session (tool, source_session_id, source_mtime_ms, run_id, folder, started_utc_ms,
                                         human_turns, non_interactive, template_key, outcome, why)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                s.tool,
                s.source_session_id,
                s.source_mtime_ms,
                run_id,
                s.folder,
                s.started_utc_ms,
                s.human_turns,
                s.non_interactive,
                s.template_key,
                s.outcome,
                s.why
            ],
        )
        .map_err(err(codes::STORE_WRITE))?;
        for (i, e) in s.events.iter().enumerate() {
            tx.execute(
                "INSERT INTO import_event (tool, source_session_id, seq, kind, ts_utc_ms, text, actions)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![s.tool, s.source_session_id, i as i64, e.kind, e.ts_utc_ms, e.text, e.actions],
            )
            .map_err(err(codes::STORE_WRITE))?;
        }
        tx.commit().map_err(err(codes::STORE_WRITE))
    }

    pub fn insert_import_run(
        &self,
        run_id: &str,
        started_ms: i64,
        finished_ms: i64,
        c: &ImportRunCounts,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO import_run (run_id, started_utc_ms, finished_utc_ms, found, imported, automated, unchanged,
                                         errors, aiq_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    run_id,
                    started_ms,
                    finished_ms,
                    c.found as i64,
                    c.imported as i64,
                    c.automated as i64,
                    c.unchanged as i64,
                    c.errors as i64,
                    env!("CARGO_PKG_VERSION")
                ],
            )
            .map(|_| ())
            .map_err(err(codes::STORE_WRITE))
    }

    /// Imported sessions now held, by (tool, outcome).
    pub fn import_counts(&self) -> Result<Vec<(String, String, u64)>> {
        let mut st = self
            .conn
            .prepare("SELECT tool, outcome, count(*) FROM import_session GROUP BY tool, outcome ORDER BY tool, outcome")
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u64)))
            .map_err(err(codes::STORE_OPEN))?;
        rows.collect::<std::result::Result<_, _>>().map_err(err(codes::STORE_OPEN))
    }

    pub fn last_import_ms(&self) -> Result<Option<i64>> {
        self.conn
            .query_row("SELECT max(finished_utc_ms) FROM import_run", [], |r| r.get(0))
            .map_err(err(codes::STORE_OPEN))
    }

    pub fn insert_judge_call(&self, c: &NewJudgeCall<'_>) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO judge_call (call_id, task_id, run, provider, model, started_utc_ms, latency_ms, outcome,
                   code, input_chars, tool_session_id, cost_usd, tokens_in, tokens_out)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    c.call_id,
                    c.task_id,
                    c.run,
                    c.provider,
                    c.model,
                    c.started_utc_ms,
                    c.latency_ms,
                    c.outcome,
                    c.code,
                    c.input_chars,
                    c.tool_session_id,
                    c.cost_usd,
                    c.tokens_in,
                    c.tokens_out
                ],
            )
            .map(|_| ())
            .map_err(err(codes::STORE_WRITE))
    }

    pub fn insert_judgment(&self, j: &JudgmentRow) -> Result<()> {
        self.conn
            .execute(
                &format!(
                    "INSERT INTO judgment ({JUDGMENT_COLS}, task_type, task_type_confidence) VALUES (?1, ?2, ?3, ?4,
                     ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24,
                     ?25, ?26, ?27, ?28)"
                ),
                params![
                    j.judgment_id,
                    j.task_id,
                    j.origin,
                    j.provider,
                    j.model,
                    j.judged_utc_ms,
                    j.job,
                    j.job_confidence,
                    j.model_rung,
                    j.current_rung,
                    j.current_confidence,
                    j.potential_rung,
                    j.potential_confidence,
                    j.ceiling_rung,
                    j.state,
                    j.coachable,
                    j.facts,
                    j.evidence,
                    j.gap,
                    j.blockers,
                    j.placement,
                    j.read,
                    j.call_ids,
                    j.fact_set_version,
                    j.rubric_version,
                    j.judge_version,
                    j.task_type,
                    j.task_type_confidence
                ],
            )
            .map(|_| ())
            .map_err(err(codes::STORE_WRITE))
    }

    /// The current judgment of every judged task: the newest version per task (derived, never stored).
    /// Empty on a store from before schema v5.
    pub fn current_judgments(&self) -> Result<HashMap<String, JudgmentRow>> {
        if !self.has_table("judgment")? {
            return Ok(HashMap::new());
        }
        // A store from before v6 (read by AIQ home without migrating) has no task-type columns: type unknown.
        let typed =
            if self.has_column("judgment", "task_type")? { "task_type, task_type_confidence" } else { "NULL, NULL" };
        let mut st = self
            .conn
            .prepare(&format!(
                "SELECT {JUDGMENT_COLS}, {typed} FROM judgment j
                 WHERE j.rowid = (SELECT x.rowid FROM judgment x WHERE x.task_id = j.task_id
                                  ORDER BY x.judged_utc_ms DESC, x.rowid DESC LIMIT 1)"
            ))
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st.query_map([], judgment_from).map_err(err(codes::STORE_OPEN))?;
        let mut out = HashMap::new();
        for r in rows {
            let r = r.map_err(err(codes::STORE_OPEN))?;
            out.insert(r.task_id.clone(), r);
        }
        Ok(out)
    }

    /// Judge calls started at or after `since_ms`, oldest first (budget and retry back-off are derived from these).
    pub fn judge_calls_since(&self, since_ms: i64) -> Result<Vec<CallOutcome>> {
        if !self.has_table("judge_call")? {
            return Ok(vec![]);
        }
        let mut st = self
            .conn
            .prepare(
                "SELECT task_id, started_utc_ms, outcome, cost_usd FROM judge_call WHERE started_utc_ms >= ?1
                 ORDER BY started_utc_ms, rowid",
            )
            .map_err(err(codes::STORE_OPEN))?;
        let rows = st
            .query_map([since_ms], |r| {
                Ok(CallOutcome {
                    task_id: r.get(0)?,
                    started_utc_ms: r.get(1)?,
                    outcome: r.get(2)?,
                    cost_usd: r.get(3)?,
                })
            })
            .map_err(err(codes::STORE_OPEN))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(err(codes::STORE_OPEN))
    }

    /// Delete everything history import stored (US-8), including imported tasks' judgments and judge calls.
    /// Live capture is untouched.
    pub fn forget_imported(&mut self) -> Result<u64> {
        let tx = self.conn.transaction().map_err(err(codes::STORE_WRITE))?;
        tx.execute("DELETE FROM judgment WHERE origin = 'imported'", []).map_err(err(codes::STORE_WRITE))?;
        tx.execute("DELETE FROM judge_call WHERE task_id LIKE 'import:%'", []).map_err(err(codes::STORE_WRITE))?;
        tx.execute("DELETE FROM import_event", []).map_err(err(codes::STORE_WRITE))?;
        let n = tx.execute("DELETE FROM import_session", []).map_err(err(codes::STORE_WRITE))?;
        tx.execute("DELETE FROM import_run", []).map_err(err(codes::STORE_WRITE))?;
        tx.commit().map_err(err(codes::STORE_WRITE))?;
        Ok(n as u64)
    }

    /// Delete one prompt's text, the person's "delete this item" (design aiq-home-deferred-pages E1, E2).
    /// The event row stays (append-only fact: a prompt happened). The bytes go too: `secure_delete` zeroes
    /// the freed pages, and a TRUNCATE checkpoint empties the WAL that still held the deleted row.
    /// Returns false when the event has no stored text (unknown id, a turn end, or already deleted).
    pub fn forget_content(&self, event_id: &str) -> Result<bool> {
        self.conn.execute_batch("PRAGMA secure_delete = ON;").map_err(err(codes::STORE_WRITE))?;
        let n = self
            .conn
            .execute("DELETE FROM content WHERE event_id = ?1", [event_id])
            .map_err(err(codes::STORE_WRITE))?;
        if n > 0 {
            // A reader (AIQ home) can hold the WAL open; then the checkpoint is partial and runs again on the next delete.
            let _ = self.conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
        }
        Ok(n > 0)
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
