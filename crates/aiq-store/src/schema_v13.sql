-- Schema v13 (weekly re-check of paused Kinds, design kinds-recut K17; expand only).
-- A check run says what it sampled and who started it, so the pause rule can keep a run that read one Kind out of
-- that Kind's job (ten tasks of one Kind say nothing about the job as a whole), and so how often AIQ re-checks by
-- itself is counted. NULL on runs made before v13: not recorded (they sampled every job, a job, or tasks with no job).
--   scope: all | no_job | job:<job> | kind:<job>/<kind>
--   started_by: person (aiq analyze --check) | weekly (AIQ, a paused Kind's re-check)
ALTER TABLE judge_check_run ADD COLUMN scope TEXT;
ALTER TABLE judge_check_run ADD COLUMN started_by TEXT;
