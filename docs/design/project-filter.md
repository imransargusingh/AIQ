---
id: design-project-filter
title: "Design: filter tasks by project"
type: design
status: proposed
owner: "Imran"
phase: "5"
tags: [design, aiq-home, coaching, projects]
links:
  - { to: spec-aiq-home, rel: implements }
  - { to: design-coaching-from-rungs, rel: depends-on }
  - { to: design-aiq-home-app, rel: depends-on }
review-by: 2027-05-03
summary: >-
  People work differently in different projects. AIQ home gets one Project picker at the top of Home and
  Tasks ("All projects" by default) and a "Your projects" table that compares them: tasks, most common rung,
  agents finished, tasks with a next step. A project is the folder the session ran in; AIQ names it from the
  folder and the person can rename it in one click. No tagging.
---

# Design: filter tasks by project

Imran, 2026-11-03: *"Users might work differently in different projects/context and this would help them
better understand in which scenarios they have more/less opportunity to leverage AI further. What would that
look like in the UI?"*

## Goal state

| | |
|---|---|
| **Goal** | A person can see their numbers for one project, and see which project has the most room to grow |
| **Done when** | Home and Tasks filter by project; Home compares projects; names are automatic with an easy rename |
| **Not in scope** | Grouping a repo's subfolders into one project (P7, next step); tips filtered by project (P6) |
| **Tier** | T1 (new read-model shape and one new setting key) |

## What the store knows today (measured from the code)

Every task already carries the folder it ran in. Checked in the code on 2026-11-03, not on Imran's store:

| Source | Folder recorded | Where |
|---|---|---|
| Claude Code hook | Yes, the hook's `cwd` | `crates/aiq/src/hook.rs` `cwd_of`, stored as `payload_meta.folder` by `crates/aiq-core/src/service.rs` |
| Copilot CLI hook | Yes, `cwd` | same path |
| VS Code agent hook | Yes, `cwd` | same path |
| Imported Claude Code | Yes, the transcript's `cwd` (falls back to the project folder slug) | `crates/aiq/src/import.rs` `read_claude` |
| Imported Copilot CLI | Yes when the session's start record has `cwd`, else `(none)` | `crates/aiq/src/import.rs` |

The folder is normalised (`folder_key`: Windows paths lower-cased, trailing slash dropped), so the same folder
always groups together. **No git remote or repo root is stored.** So today AIQ can group by folder, not by
repo. A session started in `repo\src` is a different project from one started in `repo`.

Not measured: how many of Imran's tasks have a folder, and how many folders there are. Earlier laptop reads
name at least AIQ, Foresight, executive_presence and tacit as folders with tasks. The judge thread can read
the per-folder counts from the new Home table after the next install; it is a read of numbers the app now
shows, no extra script.

## Decisions

**P1. A project is the folder a session ran in.** No manual tagging (Imran's rule: no burden on users).
Tasks with no folder (some imported Copilot sessions, `(none)`) form one row, "Folder not recorded", so the
counts always add up to All projects.

**P2. AIQ names it; one click renames it.** The name is the folder's last part (`…\repos\Foresight` →
"Foresight"). When two folders share a last part, both show their parent too (`work\api`, `oss\api`). A
**Rename** button on the project's row saves `project_name.<folder>` as a setting (append-only, like every
setting); an empty name goes back to the automatic one. The full path shows on hover.

**P3. One Project picker, top of Home and Tasks.** A dropdown, not a chip row: a person can have dozens of
folders, and a chip row of 30 wraps into noise. Default "All projects". The choice carries between Home and
Tasks so moving pages keeps the view. It resets when the app restarts (no hidden state to forget about).

**P4. What the picker changes.** Everything on Home that is about the person's tasks:

| Changes with the picker | Stays the same (and says so) |
|---|---|
| Most common rung and the rung scale | Your latest tip, Recent tips (P6) |
| Tasks agents finished, "could be", the weekly chart | Set aside folders |
| Is each kind of task moving up? | Judging today |
| Where agents could do more | |
| How you work with agents (rung shares) | |
| Tasks page list | |

**P5. Project numbers use the same rules on the project's tasks only.** The split, the kind rows and the
progress rows run the same functions over that project's judged tasks. So a kind of task with 12 tasks over
all projects but 3 in one project shows "too few to say" in that project. That is honest (3 tasks are too few
to coach on) but it means a small project shows fewer next steps than All projects. The alternative (borrow
the all-projects verdict for each kind) would show advice the project's own tasks don't support.

**P5b. "Your projects" compares them.** A table on Home, under All projects only, one row per project with
a task in the last 30 days:

| Project | Tasks | Most common rung | Agents finished | With a next step |
|---|---|---|---|---|
| Foresight | 48 | Assisted | 0% | 31 (65%) |
| AIQ | 120 | Directed | 4% | 22 (18%) |
| tacit | 6 | Assisted | too few | too few |

*(Numbers above are illustrative, not measured.)*

"With a next step" counts judged tasks whose kind of task has a next step in that project: the tasks that
could move up a rung. The rows are ranked by it, most first, so the top row is "where you have the most room
to use agents more". Projects with fewer judged tasks than the split minimum show "too few" and go last.
Selecting a row sets the picker to that project.

**P6. Tips are not filtered yet.** A tip is stored against a session, not a task, and the snapshot does not
join tips to folders. Filtering tips needs that join; it is cheap but separate. Until then the tip cards say
nothing about projects and stay visible.

**P7. Next step, not built: a repo is one project.** The core could record the repo root (walk up to
`.git`) and the remote name once per session at capture time, so subfolders of one repo group together and
the name can be `owner/repo`. That is a store change on the hot path (the 100 ms hook budget), so it waits
for a measured need: if Imran's table shows the same repo split over several rows.

## Interaction with the folder set-aside rule (PR #87)

Set-aside folders never become projects: their tasks are not tasks. PR #87 counts a person's sessions in a
set-aside folder (two or more prompts) as tasks again; those tasks appear under that folder's project like
any other. So a folder can appear both in "Set aside" and in "Your projects". That is correct (the folder
has both automated sessions and the person's own) and the table counts only the person's tasks.

## Layout question for Imran

One judgement call is on a card in the thread: **dropdown at the top of Home and Tasks (built) vs a
separate Projects page.** Recommendation: the dropdown plus the table on Home, because the question
"where do I have more room?" is answered on the page people already open, and the filter reuses every
Home section instead of a second copy of them.

## Test plan

- Rust: a seeded store with two folders and one task with no folder: project rows add up to All projects,
  names come from the folder, a rename setting wins, duplicate last parts get their parent, scoped split
  and rungs match a hand count.
- UI e2e: picker present on Home and Tasks; choosing a project changes the headline and the task list;
  selecting a table row sets the picker; rename saves through `set_setting`; axe stays clean.
