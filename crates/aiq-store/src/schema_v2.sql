-- Schema v2 (Phase 2, expand only): the person opened a card's details. Grain: one row is exactly one
-- open of one card, on one surface. Append-only. Read by the Copilot CLI tip-line lifetime rule.
CREATE TABLE card_open (
  open_id INTEGER PRIMARY KEY AUTOINCREMENT,
  card_id TEXT NOT NULL,
  surface TEXT NOT NULL,               -- where it was opened: cli, vscode
  utc_ms INTEGER NOT NULL
);
CREATE INDEX card_open_card ON card_open (card_id);
CREATE TRIGGER card_open_no_update BEFORE UPDATE ON card_open BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER card_open_no_delete BEFORE DELETE ON card_open BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
