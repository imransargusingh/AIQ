# Team tips file

A team can add one **Tool to try** line to AIQ's habit tips, naming a tool it uses internally. The file
lives in the team's own folder (an internal repo clone or a share), never in this repo. AIQ reads it in
place, so updating the folder updates the tips. AIQ never downloads it.

## Set it up

1. Copy [example-team-tips.toml](example-team-tips.toml) to `<your folder>/team-tips.toml` and fill it in.
2. Run `aiq team-tips add <your folder>`.
3. `aiq team-tips check` shows how many habits have a tool and lists entries it skipped.
   `aiq team-tips remove` stops using the file.

## The file

```toml
schema = "aiq.team-tips.v1"

[[tools]]
move_id = "check_code"     # the habit's id; see move-ids.txt
tool = "Example Test Runner" # at most 32 characters
how = "runs the team's whole test suite in one step."   # one sentence, at most 160 characters
link = "https://example.com/test-runner"                 # optional, https only
```

One entry per habit at most. Habit ids stay the same when a habit's words change. Every id is listed in
[move-ids.txt](move-ids.txt), and a test fails if one disappears from AIQ without notice.

## Where the line shows

| Surface | Shows the tool? |
|---|---|
| Copilot CLI | Yes: in the tip in the conversation, and on the status line when the line stays within 104 characters |
| VS Code | Yes: in the tip notification and in its details |
| AIQ Home | Yes: under "Try this" on a kind's next step and in tip history |
| `aiq show` | Yes, except when run inside Claude Code (`CLAUDECODE` set) |
| Claude Code | **Never** |

## What it never does

- It never changes when or whether a tip shows. A missing or broken file means tips show as before, and
  Home and `aiq team-tips check` say what's wrong without quoting the file.
- Its text is never stored in AIQ's database, written to a log, included in `aiq export`, or sent in an
  analysis call. The database keeps the tip as shown without the tool line.
- `aiq team-tips check` prints counts and habit ids, never tool names.

## What this repo must never contain

Real tool names, links, internal host names, or a real team tips file. `team-tips.toml` is git-ignored,
and a test (`team_tips_files_stay_out_of_the_repo`) fails if any file other than the example uses the
schema.
