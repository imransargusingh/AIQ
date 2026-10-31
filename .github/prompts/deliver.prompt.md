---
mode: agent
description: Deliver one grounded task through applicable skills, with durable pause/resume and real-path proof.
---
Read and follow the installed **deliver** skill at `.claude/skills/deliver/SKILL.md` (or `.agents/skills/deliver/SKILL.md`), and only its applicable references. Do not expand this wrapper into a second workflow. CO-S0 consumes the accepted compiled contract without changing requirements; select stages by actual unresolved work, task type and risk. Continue automatically between approved stages, not through human/permission/reviewer/release gates. `/deliver resume <task-id>` validates durable checkpoint, identity, contract, inputs, decisions and evidence before continuing the same task. No default spawning, adoption, docs/audit bootstrap, identity/configuration change or release. Final handback maps original criteria to real-path evidence, actual changes, tests/skips and remaining limits.

With no task input, ask only "What outcome would you like me to deliver?" and stop before compilation, setup or checkpoint writes.

Keep review proportional: genuine T0 code/docs may self-check; T1/T2 and every actual hard veto retain applicable independent review. Load-bearing architecture and coordinated execution impose T2 and missing applicable design. At partial-work or hard-veto pauses, record all actual stage authors with repeated `--actor`; an author never clears their own veto, including midstage. Identities are cooperative labels, not human authentication or permission.

${input}
