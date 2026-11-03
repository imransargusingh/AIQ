-- Schema v8 (kind-of-task label, docs/design/kind-label.md; expand only).
-- type_guess gains how the guess was made: 'words' (nearest neighbours, D5) or 'label' (a small model read the
-- first prompt). Rows before v8 are word guesses. The D10 self-check is derived per method on read.
ALTER TABLE type_guess ADD COLUMN method TEXT NOT NULL DEFAULT 'words';

-- fact: one row is exactly one label call for one live task's first prompt, recorded when the call finished.
-- Holds ids, the answer's ids and numbers, never prompt text. The daily label budget is counted from these rows.
CREATE TABLE kind_label (
  label_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  surface TEXT NOT NULL,
  surface_session_id TEXT NOT NULL,
  provider TEXT NOT NULL,
  model TEXT,
  outcome TEXT NOT NULL,                                 -- ok, schema, error, timeout, not_signed_in, missing
  job TEXT,                                              -- NULL unless outcome = ok
  job_confidence REAL,                                   -- non-additive
  task_type TEXT,
  task_type_confidence REAL,                             -- non-additive
  requested_utc_ms INTEGER NOT NULL,                     -- the first prompt's arrival at the core
  started_utc_ms INTEGER NOT NULL,
  latency_ms INTEGER NOT NULL,                           -- additive
  tokens_in INTEGER,                                     -- additive; NULL when the tool doesn't say
  tokens_out INTEGER,
  cost_usd REAL,                                         -- additive; NULL for Copilot (premium requests)
  utc_ms INTEGER NOT NULL
);
CREATE INDEX kind_label_task ON kind_label (task_id);
CREATE INDEX kind_label_time ON kind_label (started_utc_ms);

CREATE TRIGGER kind_label_no_update BEFORE UPDATE ON kind_label BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
