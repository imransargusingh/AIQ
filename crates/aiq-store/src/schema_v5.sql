-- Schema v5 (rung judge slice 1, docs/design/rung-judge.md; expand only).
-- Judgments and judge calls are append-only facts: never updated. A re-judgment is a new row; the newest
-- row per task is its current judgment, derived on read. Rows are deleted only by the person (forget).

-- fact: one row is exactly one call to the person's own provider for one run of one task.
CREATE TABLE judge_call (
  call_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  run INTEGER NOT NULL,                 -- 1 or 2
  provider TEXT NOT NULL CHECK (provider IN ('claude-code', 'copilot-cli')),
  model TEXT,                           -- as the tool reported it; NULL when not reported
  started_utc_ms INTEGER NOT NULL,
  latency_ms INTEGER NOT NULL,          -- additive
  outcome TEXT NOT NULL CHECK (outcome IN ('ok', 'schema', 'error', 'timeout', 'not_signed_in', 'missing')),
  code TEXT,                            -- stable error code when outcome != ok
  input_chars INTEGER NOT NULL,         -- additive
  tool_session_id TEXT,                 -- Copilot: the session id AIQ gave the call (never imported)
  cost_usd REAL,                        -- additive; NULL = not recorded
  tokens_in INTEGER,                    -- additive; NULL = not recorded
  tokens_out INTEGER                    -- additive; NULL = not recorded
);
CREATE INDEX judge_call_task ON judge_call (task_id, started_utc_ms);
CREATE INDEX judge_call_started ON judge_call (started_utc_ms);

-- fact: one row is exactly one judgment version of one task.
CREATE TABLE judgment (
  judgment_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  origin TEXT NOT NULL CHECK (origin IN ('live', 'imported')),
  provider TEXT NOT NULL,
  model TEXT,
  judged_utc_ms INTEGER NOT NULL,
  job TEXT NOT NULL,                    -- code_change, write_doc, unclassified
  job_confidence REAL NOT NULL,         -- non-additive
  model_rung INTEGER NOT NULL CHECK (model_rung BETWEEN 1 AND 5),
  current_rung INTEGER NOT NULL CHECK (current_rung BETWEEN 1 AND 5),
  current_confidence REAL NOT NULL,
  potential_rung INTEGER NOT NULL CHECK (potential_rung BETWEEN current_rung AND current_rung + 1),
  potential_confidence REAL NOT NULL,
  ceiling_rung INTEGER NOT NULL CHECK (ceiling_rung BETWEEN 1 AND 5),
  state TEXT NOT NULL CHECK (state IN ('judged', 'uncertain', 'right_rung', 'blocked')),
  coachable INTEGER NOT NULL CHECK (coachable IN (0, 1)),
  facts TEXT NOT NULL,                  -- JSON: fact id -> {value, confidence, source, disputed}
  evidence TEXT NOT NULL CHECK (json_array_length(evidence) >= 1),
  gap TEXT NOT NULL,
  blockers TEXT NOT NULL,               -- JSON array
  placement TEXT NOT NULL,              -- JSON array: what moved the rung, in plain words
  read TEXT NOT NULL,                   -- JSON: what was sent ("What was read?")
  call_ids TEXT NOT NULL,               -- JSON array
  fact_set_version TEXT NOT NULL,
  rubric_version TEXT NOT NULL,
  judge_version TEXT NOT NULL
);
CREATE INDEX judgment_task ON judgment (task_id, judged_utc_ms);

CREATE TRIGGER judge_call_no_update BEFORE UPDATE ON judge_call BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER judgment_no_update BEFORE UPDATE ON judgment BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
