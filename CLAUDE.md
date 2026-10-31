# CLAUDE.md

@AGENTS.md

<!-- AI-FORWARD-PACK:BEGIN (managed block — the Claude Code addendum; the full pack block lives ONCE in AGENTS.md and is imported by the `@AGENTS.md` line above; replace this addendum wholesale on pack updates) -->
## Claude Code

- **`@AGENTS.md` above is the instruction set.** Everything in `AGENTS.md` — the repo conventions and the managed AI-Forward block — applies here verbatim. It is not duplicated in this file: Copilot CLI loads *both* files, so a copy would be paid twice on every request (defect class **CTX-B**), and Claude Code reads only this file, expanding the import at launch.
- **Paths.** Where `AGENTS.md` cites `.github/instructions/<name>.instructions.md`, read **`.claude/knowledge/<name>.md`** — the same document, unwrapped. Docs with `load: skill` or `load: reference` are there too (Copilot keeps them in `.github/knowledge/`). Skills are `.claude/skills/<name>/SKILL.md` and apply by their description; the `reference/*.md` files beside a `SKILL.md` are the stage detail it reads on demand — read them at the stage, never by re-invoking the skill. Personas are `.claude/agents/<name>.md`.
- **Hooks.** The re-read guard (`docs/ai-forward-pack/hooks/reread-guard.py`) is wired in `.claude/settings.json`; a warning from it means the file is already in your context — use what you have.
- **Sessions.** A new task starts a new session in the same worktree (WT1a); `/compact` before a long session turns to unrelated work.
<!-- AI-FORWARD-PACK:END -->
