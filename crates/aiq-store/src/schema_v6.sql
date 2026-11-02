-- Schema v6 (coaching from rung results, docs/design/coaching-from-rungs.md; expand only).
-- judgment gains the task type the judge named (NULL = judged before v6, type unknown; re-judged lazily).
-- Three new append-only facts; current values (latest per key) are derived on read, never stored.
ALTER TABLE judgment ADD COLUMN task_type TEXT;            -- pack task-type id within the job; NULL before v6
ALTER TABLE judgment ADD COLUMN task_type_confidence REAL; -- non-additive

-- fact: one row is exactly one guess of one live task's type, made at its first prompt (D5).
CREATE TABLE type_guess (
  guess_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  surface TEXT NOT NULL,
  job TEXT NOT NULL,
  task_type TEXT NOT NULL,
  similarity REAL NOT NULL,                                -- non-additive
  neighbours TEXT NOT NULL,                                -- JSON array of judged task ids (never text)
  utc_ms INTEGER NOT NULL
);
CREATE INDEX type_guess_task ON type_guess (task_id);

-- fact: one row is exactly one answer the person gave to one card, or to one tip id from Settings (D8).
CREATE TABLE tip_feedback (
  feedback_id INTEGER PRIMARY KEY AUTOINCREMENT,
  card_id TEXT,
  tip_id TEXT NOT NULL,
  answer TEXT NOT NULL CHECK (answer IN ('helpful', 'not_helpful', 'snooze', 'off', 'on', 'no_answer')),
  surface TEXT NOT NULL,
  utc_ms INTEGER NOT NULL
);
CREATE INDEX tip_feedback_tip ON tip_feedback (tip_id, utc_ms);

-- fact: one row is exactly one flag or undo on one (job, task type, move) (D8, "This doesn't fit").
CREATE TABLE guidance_flag (
  flag_id INTEGER PRIMARY KEY AUTOINCREMENT,
  job TEXT NOT NULL,
  task_type TEXT NOT NULL,
  move TEXT NOT NULL,
  action TEXT NOT NULL CHECK (action IN ('flag', 'undo')),
  reason TEXT CHECK (reason IN ('evidence_wrong', 'not_useful', 'blocked', 'other')),
  note TEXT CHECK (length(note) <= 200),
  utc_ms INTEGER NOT NULL
);

CREATE TRIGGER type_guess_no_update BEFORE UPDATE ON type_guess BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER tip_feedback_no_update BEFORE UPDATE ON tip_feedback BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER guidance_flag_no_update BEFORE UPDATE ON guidance_flag BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
