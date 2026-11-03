-- Schema v9 (live agent actions; expand only).
-- fact: one row is exactly one live turn, keyed by the stop event that ended it, with what the agent did in it as
-- read from the tool's own log after that stop (Claude Code transcript, Copilot session log). Counts and tool
-- names only: never command text, file names or output. A turn AIQ could not read is a row with outcome
-- 'not_recorded' and NULL counts, never a guess. Imported turns keep their actions on import_event; a turn seen
-- both ways is joined by session (design docs/design/live-agent-actions.md).
CREATE TABLE turn_action (
  stop_event_id TEXT PRIMARY KEY,                        -- event.event_id of the stop
  surface TEXT NOT NULL,
  surface_session_id TEXT NOT NULL,
  ts_utc_ms INTEGER NOT NULL,                            -- the stop's time
  outcome TEXT NOT NULL,                                 -- recorded, not_recorded
  why TEXT,                                              -- NULL when recorded; no_log, read_error, too_large, unparsed
  tool_calls INTEGER,                                    -- additive; NULL when not recorded
  commands INTEGER,                                      -- additive: shell commands run
  files INTEGER,                                         -- non-additive across turns (distinct per turn, at most 50)
  subagents INTEGER,                                     -- additive
  tools TEXT,                                            -- JSON {tool name: calls}
  bytes_read INTEGER NOT NULL,                           -- additive: the read's cost
  read_ms INTEGER NOT NULL,                              -- additive: the read's cost
  utc_ms INTEGER NOT NULL
);
CREATE INDEX turn_action_session ON turn_action (surface, surface_session_id, ts_utc_ms);
CREATE INDEX turn_action_time ON turn_action (utc_ms);

CREATE TRIGGER turn_action_no_update BEFORE UPDATE ON turn_action BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
