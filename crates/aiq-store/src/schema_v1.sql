-- AIQ store schema v1 (docs/design/claude-code-skeleton.md, Data model).
-- Dimensions + append-only facts; current state is derived by query, never stored twice.

-- dimension: one row is exactly one surface session seen by the core. Type-0 (immutable).
CREATE TABLE surface_session (
  surface TEXT NOT NULL,
  surface_session_id TEXT NOT NULL,
  first_seen_utc_ms INTEGER NOT NULL,
  PRIMARY KEY (surface, surface_session_id)
);

-- fact: one row is exactly one collector event, as accepted at ingest.
CREATE TABLE event (
  event_id TEXT PRIMARY KEY,           -- UUIDv7 from the client; idempotency key
  surface TEXT NOT NULL,
  surface_session_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('prompt', 'stop')),
  ts_utc_ms INTEGER NOT NULL,          -- client clock
  received_utc_ms INTEGER NOT NULL,    -- core clock
  prompt_id TEXT,
  trace_id TEXT,
  collector_version TEXT NOT NULL,
  prompt_chars INTEGER,                -- additive within one event only
  payload_meta TEXT NOT NULL,          -- JSON, non-content fields only
  FOREIGN KEY (surface, surface_session_id) REFERENCES surface_session (surface, surface_session_id)
);
CREATE INDEX event_session ON event (surface, surface_session_id, event_id);
CREATE INDEX event_prompt ON event (surface, surface_session_id, prompt_id);

-- content: raw text of one event. Insert, then delete only by the person; never updated.
CREATE TABLE content (
  event_id TEXT PRIMARY KEY REFERENCES event (event_id),
  text TEXT NOT NULL
);

-- fact: one row is exactly one detector firing on one Task.
CREATE TABLE signal (
  signal_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  surface TEXT NOT NULL,
  surface_session_id TEXT NOT NULL,
  detector_id TEXT NOT NULL,
  detector_version INTEGER NOT NULL,
  pack_version TEXT NOT NULL,
  opportunity_id TEXT NOT NULL,
  reprompt_count INTEGER NOT NULL,     -- non-additive snapshot of one Task
  score REAL NOT NULL,                 -- non-additive
  evidence TEXT NOT NULL,              -- JSON array of event ids
  utc_ms INTEGER NOT NULL
);

-- fact: one row is exactly one show-ready / hold / drop decision on one card.
CREATE TABLE card_decision (
  decision_id TEXT PRIMARY KEY,
  card_id TEXT NOT NULL,
  signal_id TEXT NOT NULL REFERENCES signal (signal_id),
  surface TEXT NOT NULL,
  surface_session_id TEXT NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('show_ready', 'hold', 'drop')),
  reason_code TEXT NOT NULL,
  rule_id TEXT NOT NULL,
  pack_version TEXT NOT NULL,
  card_json TEXT NOT NULL,
  utc_ms INTEGER NOT NULL
);
CREATE INDEX card_decision_session ON card_decision (surface, surface_session_id);
CREATE INDEX card_decision_card ON card_decision (card_id);

-- fact: one row is exactly one card handed to a renderer. Count is additive.
CREATE TABLE card_delivery (
  card_id TEXT PRIMARY KEY,
  surface TEXT NOT NULL,
  surface_session_id TEXT NOT NULL,
  delivered_utc_ms INTEGER NOT NULL,
  local_offset_min INTEGER NOT NULL
);
-- Coaching-session invariant backstop: one in-the-moment card per session, even if the loop is bypassed.
CREATE UNIQUE INDEX card_delivery_one_per_session ON card_delivery (surface, surface_session_id);

-- fact: one row is exactly one change to one setting. Current value = latest row per key.
CREATE TABLE setting_change (
  change_id INTEGER PRIMARY KEY AUTOINCREMENT,
  key TEXT NOT NULL,
  value TEXT NOT NULL,
  utc_ms INTEGER NOT NULL
);
CREATE VIEW current_setting AS
  SELECT key, value FROM setting_change
  WHERE change_id IN (SELECT max(change_id) FROM setting_change GROUP BY key);

-- Append-only enforcement.
CREATE TRIGGER surface_session_no_update BEFORE UPDATE ON surface_session BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER surface_session_no_delete BEFORE DELETE ON surface_session BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER event_no_update BEFORE UPDATE ON event BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER event_no_delete BEFORE DELETE ON event BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER signal_no_update BEFORE UPDATE ON signal BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER signal_no_delete BEFORE DELETE ON signal BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER card_decision_no_update BEFORE UPDATE ON card_decision BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER card_decision_no_delete BEFORE DELETE ON card_decision BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER card_delivery_no_update BEFORE UPDATE ON card_delivery BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER card_delivery_no_delete BEFORE DELETE ON card_delivery BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER setting_change_no_update BEFORE UPDATE ON setting_change BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER setting_change_no_delete BEFORE DELETE ON setting_change BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER content_no_update BEFORE UPDATE ON content BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
