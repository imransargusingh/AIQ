-- Schema v7 (accuracy check of the rung judge, docs/design/judge-accuracy-check.md; expand only).
-- Agreement numbers are derived from these rows and judgment; none is stored.

-- fact: one row is exactly one `aiq judge --check` run.
CREATE TABLE judge_check_run (
  run_id TEXT PRIMARY KEY,
  seed INTEGER NOT NULL,
  requested INTEGER NOT NULL,                            -- sample size asked for
  sample TEXT NOT NULL,                                  -- JSON array of judgment ids, in check order
  started_utc_ms INTEGER NOT NULL,
  check_version TEXT NOT NULL
);

-- fact: one row is exactly one second reading of one judgment, recorded when the grader call finished.
-- answer holds ids and booleans only (aiq.check.v1), never task text.
CREATE TABLE judge_check (
  check_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL,
  judgment_id TEXT NOT NULL,
  task_id TEXT NOT NULL,
  grader_provider TEXT NOT NULL,
  grader_model TEXT,
  outcome TEXT NOT NULL,                                 -- ok, schema, error, timeout, not_signed_in, missing
  answer TEXT,                                           -- JSON aiq.check.v1 when outcome = ok, else NULL
  latency_ms INTEGER NOT NULL,                           -- additive
  cost_usd REAL,                                         -- additive; NULL when the tool doesn't say
  checked_utc_ms INTEGER NOT NULL
);
CREATE INDEX judge_check_run_ix ON judge_check (run_id);

CREATE TRIGGER judge_check_run_no_update BEFORE UPDATE ON judge_check_run BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER judge_check_no_update BEFORE UPDATE ON judge_check BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
