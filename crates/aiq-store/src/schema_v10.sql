-- Schema v10 (set-aside sessions; expand only).
-- fact: one row is exactly one live session AIQ set aside at its first prompt (an app, script or AIQ's own check drove
-- it). No prompt text: the folder key, the rule's reason and the time only. A set-aside session whose person sent a
-- second prompt is recorded as a task (rule H >= 2); it keeps its row here, and readers derive "still set aside" as
-- "no surface_session row" rather than storing a second definition. Home counts these per set-aside folder.
CREATE TABLE set_aside_session (
  surface TEXT NOT NULL,
  surface_session_id TEXT NOT NULL,
  folder TEXT NOT NULL,                                  -- folder key (automated::folder_key)
  reason TEXT NOT NULL,                                  -- aiq-check, folder, template, burst
  utc_ms INTEGER NOT NULL,                               -- the first prompt's arrival
  PRIMARY KEY (surface, surface_session_id)
);
CREATE INDEX set_aside_session_folder ON set_aside_session (folder, utc_ms);

CREATE TRIGGER set_aside_session_no_update BEFORE UPDATE ON set_aside_session BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
