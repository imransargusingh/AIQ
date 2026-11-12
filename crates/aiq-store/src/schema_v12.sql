-- Schema v12 (sync between your computers, docs/design/multi-machine-sync.md; expand only).
-- Every synced fact gains `computer`: NULL = recorded on this computer, else the id of the computer it came from. A
-- row that arrives from another computer is inserted with it set and never changed, so nothing here rewrites a
-- past row. Per-computer state is NOT synced: setting_change, folder_rule (folder paths are local), import_run,
-- judge_check_run and judge_check (this computer's own accuracy checks).

ALTER TABLE surface_session ADD COLUMN computer TEXT;
ALTER TABLE event ADD COLUMN computer TEXT;
ALTER TABLE content ADD COLUMN computer TEXT;
ALTER TABLE signal ADD COLUMN computer TEXT;
ALTER TABLE card_decision ADD COLUMN computer TEXT;
ALTER TABLE card_delivery ADD COLUMN computer TEXT;
ALTER TABLE card_open ADD COLUMN computer TEXT;
ALTER TABLE set_aside_session ADD COLUMN computer TEXT;
ALTER TABLE turn_action ADD COLUMN computer TEXT;
ALTER TABLE type_guess ADD COLUMN computer TEXT;
ALTER TABLE kind_label ADD COLUMN computer TEXT;
ALTER TABLE tip_path ADD COLUMN computer TEXT;
ALTER TABLE tip_feedback ADD COLUMN computer TEXT;
ALTER TABLE guidance_flag ADD COLUMN computer TEXT;
ALTER TABLE import_session ADD COLUMN computer TEXT;
ALTER TABLE import_event ADD COLUMN computer TEXT;
ALTER TABLE judge_call ADD COLUMN computer TEXT;
ALTER TABLE judgment ADD COLUMN computer TEXT;

-- This computer's sync identity: one row, made with the store, so a fresh store (after Delete everything) is a new
-- computer to the others and never reuses an old change number.
CREATE TABLE sync_self (
  device_id TEXT PRIMARY KEY,
  created_utc_ms INTEGER NOT NULL
);
INSERT INTO sync_self (device_id, created_utc_ms)
  VALUES (lower(hex(randomblob(16))), CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER));

-- fact: one row is exactly one row this computer recorded, in the order it was recorded. Filled by the triggers
-- below; AUTOINCREMENT so a number is never reused. The change files are cut from it.
CREATE TABLE sync_outbox (
  n INTEGER PRIMARY KEY AUTOINCREMENT,
  tbl TEXT NOT NULL,
  row_id INTEGER NOT NULL
);

-- fact: one row is exactly one delete the person made that the other computers must repeat. `what` = content (key:
-- event id) or imported_all (key ''). computer NULL = made here (sent on); else the computer it came from (not sent on).
CREATE TABLE sync_delete (
  n INTEGER PRIMARY KEY AUTOINCREMENT,
  what TEXT NOT NULL CHECK (what IN ('content', 'imported_all')),
  key TEXT NOT NULL,
  computer TEXT,
  utc_ms INTEGER NOT NULL
);

-- fact: one row is exactly one change file this computer wrote: its number and the last outbox / delete row in it.
CREATE TABLE sync_export (
  seq INTEGER PRIMARY KEY,
  upto_row INTEGER NOT NULL,
  upto_delete INTEGER NOT NULL,
  rows INTEGER NOT NULL,                                 -- additive
  utc_ms INTEGER NOT NULL
);

-- fact: one row is exactly one change file from another computer, applied here in one transaction.
CREATE TABLE sync_applied (
  peer TEXT NOT NULL,
  seq INTEGER NOT NULL,
  rows INTEGER NOT NULL,                                 -- additive
  utc_ms INTEGER NOT NULL,
  PRIMARY KEY (peer, seq)
);

CREATE TRIGGER sync_out_surface_session AFTER INSERT ON surface_session WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('surface_session', NEW.rowid); END;
CREATE TRIGGER sync_out_event AFTER INSERT ON event WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('event', NEW.rowid); END;
CREATE TRIGGER sync_out_content AFTER INSERT ON content WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('content', NEW.rowid); END;
CREATE TRIGGER sync_out_signal AFTER INSERT ON signal WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('signal', NEW.rowid); END;
CREATE TRIGGER sync_out_card_decision AFTER INSERT ON card_decision WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('card_decision', NEW.rowid); END;
CREATE TRIGGER sync_out_card_delivery AFTER INSERT ON card_delivery WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('card_delivery', NEW.rowid); END;
CREATE TRIGGER sync_out_card_open AFTER INSERT ON card_open WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('card_open', NEW.rowid); END;
CREATE TRIGGER sync_out_set_aside_session AFTER INSERT ON set_aside_session WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('set_aside_session', NEW.rowid); END;
CREATE TRIGGER sync_out_turn_action AFTER INSERT ON turn_action WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('turn_action', NEW.rowid); END;
CREATE TRIGGER sync_out_type_guess AFTER INSERT ON type_guess WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('type_guess', NEW.rowid); END;
CREATE TRIGGER sync_out_kind_label AFTER INSERT ON kind_label WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('kind_label', NEW.rowid); END;
CREATE TRIGGER sync_out_tip_path AFTER INSERT ON tip_path WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('tip_path', NEW.rowid); END;
CREATE TRIGGER sync_out_tip_feedback AFTER INSERT ON tip_feedback WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('tip_feedback', NEW.rowid); END;
CREATE TRIGGER sync_out_guidance_flag AFTER INSERT ON guidance_flag WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('guidance_flag', NEW.rowid); END;
CREATE TRIGGER sync_out_import_session AFTER INSERT ON import_session WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('import_session', NEW.rowid); END;
CREATE TRIGGER sync_out_import_event AFTER INSERT ON import_event WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('import_event', NEW.rowid); END;
CREATE TRIGGER sync_out_judge_call AFTER INSERT ON judge_call WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('judge_call', NEW.rowid); END;
CREATE TRIGGER sync_out_judgment AFTER INSERT ON judgment WHEN NEW.computer IS NULL BEGIN INSERT INTO sync_outbox (tbl, row_id) VALUES ('judgment', NEW.rowid); END;

-- Everything recorded before v12 is this computer's own: queue it once, parents before children.
INSERT INTO sync_outbox (tbl, row_id) SELECT 'surface_session', rowid FROM surface_session ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'event', rowid FROM event ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'content', rowid FROM content ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'signal', rowid FROM signal ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'card_decision', rowid FROM card_decision ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'card_delivery', rowid FROM card_delivery ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'card_open', rowid FROM card_open ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'set_aside_session', rowid FROM set_aside_session ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'turn_action', rowid FROM turn_action ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'type_guess', rowid FROM type_guess ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'kind_label', rowid FROM kind_label ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'tip_path', rowid FROM tip_path ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'tip_feedback', rowid FROM tip_feedback ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'guidance_flag', rowid FROM guidance_flag ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'import_session', rowid FROM import_session ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'import_event', rowid FROM import_event ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'judge_call', rowid FROM judge_call ORDER BY rowid;
INSERT INTO sync_outbox (tbl, row_id) SELECT 'judgment', rowid FROM judgment ORDER BY rowid;

CREATE TRIGGER sync_self_no_update BEFORE UPDATE ON sync_self BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER sync_export_no_update BEFORE UPDATE ON sync_export BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER sync_applied_no_update BEFORE UPDATE ON sync_applied BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
