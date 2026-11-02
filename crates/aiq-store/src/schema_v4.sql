-- Schema v4 (history import, docs/specs/history-import.md; expand only).
-- Imported sessions live in their own tables, never in `event`: the live coaching loop and the
-- detector rebuild read only `event`, so an imported prompt can never raise a card (US-5) by construction.
-- The person can delete imported data on its own (US-8), so these tables carry no no-delete trigger;
-- rows are never updated (a session that grew is deleted and re-imported in one transaction).

-- fact: one row is exactly one finished `aiq import` run. Counts are additive across runs of new sessions.
CREATE TABLE import_run (
  run_id TEXT PRIMARY KEY,
  started_utc_ms INTEGER NOT NULL,
  finished_utc_ms INTEGER NOT NULL,
  found INTEGER NOT NULL,
  imported INTEGER NOT NULL,
  automated INTEGER NOT NULL,
  unchanged INTEGER NOT NULL,
  errors INTEGER NOT NULL,
  aiq_version TEXT NOT NULL
);

-- fact: one row is exactly one source session as last read (tool + the tool's session id).
-- It keeps the automated rule's inputs, so a re-run applies the rule without re-reading unchanged files.
CREATE TABLE import_session (
  tool TEXT NOT NULL CHECK (tool IN ('claude-code', 'copilot-cli')),
  source_session_id TEXT NOT NULL,
  source_mtime_ms INTEGER NOT NULL,     -- re-read when the source file changed since
  run_id TEXT NOT NULL,
  folder TEXT NOT NULL,                 -- working folder the session ran in
  started_utc_ms INTEGER,
  human_turns INTEGER NOT NULL,         -- non-additive per session
  non_interactive INTEGER,              -- 1 when the tool marked a non-interactive start; NULL when unknown
  template_key INTEGER,                 -- stable hash of the normalised first prompt; NULL when empty
  outcome TEXT NOT NULL CHECK (outcome IN ('imported', 'automated', 'error')),
  why TEXT NOT NULL,                    -- automated-rule signals (NI,T,B,F) or error code; '' when imported
  PRIMARY KEY (tool, source_session_id)
);
CREATE INDEX import_session_folder ON import_session (folder, outcome);

-- fact: one row is exactly one human prompt or one agent turn end of an imported session.
CREATE TABLE import_event (
  tool TEXT NOT NULL,
  source_session_id TEXT NOT NULL,
  seq INTEGER NOT NULL,                 -- order within the session
  kind TEXT NOT NULL CHECK (kind IN ('prompt', 'stop')),
  ts_utc_ms INTEGER,
  text TEXT,                            -- prompt text (content; deleted only by the person)
  actions TEXT,                         -- stop only: JSON summary of the agent's tool calls in the turn
  PRIMARY KEY (tool, source_session_id, seq),
  FOREIGN KEY (tool, source_session_id) REFERENCES import_session (tool, source_session_id) ON DELETE CASCADE
);
CREATE TRIGGER import_event_no_update BEFORE UPDATE ON import_event BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER import_run_no_update BEFORE UPDATE ON import_run BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
