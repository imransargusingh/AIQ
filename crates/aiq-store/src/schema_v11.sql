-- Schema v11 (tip funnel, docs/design/tip-funnel.md; expand only).
-- fact: one row is exactly one point where a live new task's next-step tip path ended or began to wait. A task
-- whose kind waits on a label has two rows: `label_asked`, then the label's end. Readers take the latest row per
-- task. No prompt text: ids, the step code and the time only. The tip itself, once decided, is in card_decision.
CREATE TABLE tip_path (
  task_id TEXT NOT NULL,
  surface TEXT NOT NULL,
  step TEXT NOT NULL,                                    -- aiq_domain::funnel::Step code
  utc_ms INTEGER NOT NULL,
  local_offset_min INTEGER NOT NULL
);
CREATE INDEX tip_path_time ON tip_path (utc_ms);

CREATE TRIGGER tip_path_no_update BEFORE UPDATE ON tip_path BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
CREATE TRIGGER tip_path_no_delete BEFORE DELETE ON tip_path BEGIN SELECT RAISE(ABORT, 'AIQ-STORE-APPEND-ONLY'); END;
