-- Schema v3 (Phase 2 follow-up, expand only): folders whose sessions are automated (import spec US-6,
-- US-12). Grain: one row is exactly one decision about one folder: AIQ set it aside (reason burst,
-- template) or the person included it again ("Include them"). Append-only; the newest row per folder is
-- its current state, derived on read. Shared with history import, which writes the same rows.
CREATE TABLE folder_rule (
  rule_id INTEGER PRIMARY KEY AUTOINCREMENT,
  folder TEXT NOT NULL,                -- normalized key (automated::folder_key)
  action TEXT NOT NULL CHECK (action IN ('set_aside', 'include')),
  reason TEXT NOT NULL,                -- burst, template, person
  source TEXT NOT NULL,                -- live, import, person
  utc_ms INTEGER NOT NULL
);
CREATE INDEX folder_rule_folder ON folder_rule (folder, rule_id);
CREATE TRIGGER folder_rule_no_update BEFORE UPDATE ON folder_rule BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER folder_rule_no_delete BEFORE DELETE ON folder_rule BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
