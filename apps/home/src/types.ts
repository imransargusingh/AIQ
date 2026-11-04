// Mirrors crates/aiq-home/src/lib.rs `Snapshot` (aiq.home.v1). The UI tests load JSON that the Rust crate
// writes (tests/fixtures, from `cargo run -p aiq-home --example fixture`), so a renamed field fails them.
export interface Snapshot {
  schema: "aiq.home.v1";
  now_ms: number;
  offset_min: number;
  status: Status;
  home: Home;
  tasks: TaskRow[];
  tips: TipRow[];
  data: Data;
  settings: Settings;
  setup: Setup;
  coaching: Coaching;
  update: Update;
  /** One row per project with a task in the window, most room to grow first (design project-filter). */
  projects: Project[];
  /** The open task's live rung trend (design live-rung-trend C); null when no task is open or it is off. */
  live: Live | null;
}

/** Mirrors crates/aiq-core `live_trend::LiveTrend` plus Home's task id and kind name. An estimate from what the
 * agent did; the judge's rung replaces it when the task ends. */
export interface Live {
  surface: string;
  session: string;
  first_ms: number;
  last_ms: number;
  prompts: number;
  /** The agent is working on the last prompt. */
  working: boolean;
  job: string | null;
  task_type: string | null;
  steps: { turn: number; at_ms: number; rung: number | null; recorded: boolean }[];
  rung: number | null;
  could: number | null;
  did_turns: number[];
  /** The work counted is looking things up (an answer or run job), not edits. */
  did_is_lookup: boolean;
  /** The first prompt had room for a goal and context; null when its text is gone. */
  brief: boolean | null;
  unrecorded: number[];
  /** For each of `unrecorded`: no_log, read_error, too_large, unparsed, no_start or not_read. */
  unrecorded_why: string[];
  subagents: number;
  tip: { card_id: string; turn: number; try_this: string } | null;
  task_id: string;
  kind: string | null;
}

/** One project: tasks whose session ran in one folder (crates/aiq-home `Project`, design project-filter). */
export interface Project {
  /** The folder key the tasks carry (`TaskRow.folder`); "" for tasks with no folder recorded. */
  key: string;
  name: string;
  /** The person chose the name; false when AIQ named it from the folder. */
  renamed: boolean;
  tasks_in_window: number;
  judged: number;
  uncertain: number;
  rungs: [number, number, number, number, number];
  most_common_rung: number | null;
  imported_in_window: number;
  weeks: { start_ms: number; tasks: number }[];
  by_tool: [string, number][];
  coaching: Coaching;
}

/** Auto-update (crates/aiq-core/src/update.rs `View`, design auto-update U5). */
export interface Update {
  /** False for a copy not installed with install.ps1: it never updates itself. */
  supported: boolean;
  mode: "auto" | "ask" | "off";
  running: string;
  note: "none" | "waiting" | "applied" | "failed" | "rolled_back";
  available: string | null;
  blocked: string | null;
  from: string | null;
  at_ms: number | null;
  message: string | null;
  checked_ms: number | null;
  check_error: string | null;
  /** The newest release the last good check saw. */
  latest: string | null;
  can_rollback: boolean;
}

export interface Status {
  core: boolean;
  paused: boolean;
  store_found: boolean;
  last_event_ms: number | null;
  read_ms: number;
  error: string | null;
}

export interface Home {
  window_days: number;
  tasks_in_window: number;
  judged: number;
  /** Judged Uncertain: shown, never coached, not in `rungs`. */
  uncertain: number;
  /** Judged, not-Uncertain tasks per current rung; index 0 = Assisted. */
  rungs: [number, number, number, number, number];
  most_common_rung: number | null;
  judge_today: JudgeToday;
  imported_in_window: number;
  weeks: { start_ms: number; tasks: number }[];
  by_tool: [string, number][];
  set_aside: SetAside[];
  tips_today: number;
}

/** A folder AIQ leaves out of coaching (design aiq-home-app D11). */
export interface SetAside {
  /** The full folder path (key). */
  folder: string;
  /** aiq-check, template, burst. */
  reason: string;
  since_ms: number;
  /** AIQ's own check and probe folders: folded into one line. */
  own: boolean;
  /** Sessions AIQ left out here (live and imported). */
  runs: number;
  /** The newest of them; null when none is recorded. */
  last_ms: number | null;
  /** The person's own tasks here in the window, which AIQ kept. */
  kept: number;
}

export interface TaskRow {
  task_id: string;
  tool: string;
  folder: string | null;
  first_ms: number;
  last_ms: number;
  prompts: number;
  origin: "live" | "imported";
  /** The tool's session the task's first prompt came from (Day view lanes). */
  session: string;
  /** Times of the task's prompts, in order (Day view ticks). */
  prompt_ms: number[];
  /** code_change, write_doc, ask_research, run_ship or unclassified once judged. */
  job: string | null;
  rung: number | null;
  judgment: TaskJudgment | null;
}

/** One task's current judgment. Model-written text (evidence, gap, blockers) is plain text: render as text only. */
export interface TaskJudgment {
  judged_ms: number;
  state: "judged" | "uncertain" | "right_rung" | "blocked";
  current: number;
  potential: number;
  ceiling: number;
  current_confidence: number;
  potential_confidence: number;
  job: string;
  coachable: boolean;
  evidence: string[];
  gap: string;
  blockers: string[];
  provider: "claude-code" | "copilot-cli";
  model: string | null;
  read_prompts: number;
  read_chars: number;
  read_actions: boolean;
  versions: string;
  task_type: string | null;
  task_type_label: string | null;
}

/** Coaching from rung results (design coaching-from-rungs). Every number is derived from judgments. */
export interface Coaching {
  /** up: not finished, with a next step at least one rung up; up_to: the rung those steps reach most often. */
  split: { tasks: number; finished: number; could: number; needs: number; uncertain: number; up: number; up_to: number | null };
  /** The split shows with at least this many judged, not-Uncertain tasks. */
  split_min: number;
  /** The rule shown under "Tasks agents finished". */
  rule: string;
  weeks: { start_ms: number; tasks: number; finished: number }[];
  week_min: number;
  /** At most 3 kinds of task with a next step, ranked. */
  opportunities: KindRow[];
  kinds: KindRow[];
  habits: HabitRow[];
  habits_imported: boolean;
  /** Judged before kinds of task existed. */
  untyped: number;
  /** Each kind's typical rung, last `progress_days` against the `progress_days` before. Moved up first. */
  progress: ProgressRow[];
  progress_days: number;
  /** Judged tasks each half needs before AIQ says a kind moved. */
  progress_min: number;
}

export type Trend = "up" | "down" | "same" | "too_few";

export interface ProgressRow {
  job: string;
  task_type: string;
  label: string;
  /** Judged, not-Uncertain tasks in the last `progress_days`, and in the `progress_days` before. */
  recent: number;
  earlier: number;
  uncertain: number;
  recent_rung: number | null;
  earlier_rung: number | null;
  trend: Trend;
}

export type KindState = "coach" | "too_few" | "right_rung" | "blocked" | "not_sure" | "flagged";

export interface KindRow {
  job: string;
  task_type: string;
  label: string;
  tasks: number;
  uncertain: number;
  per_week: number;
  today: number | null;
  potential: number | null;
  state: KindState;
  next: MoveOut | null;
  ranked_last: boolean;
  last_task_id: string | null;
  /** The judge's own words: plain text only (TR-5). */
  last_gap: string | null;
  task_ids: string[];
}

export interface MoveOut {
  move_id: string;
  habit: string;
  why: string;
  try_this: string;
  starter: string;
}

export interface HabitRow {
  job: string;
  move_id: string;
  name: string;
  state: "not_yet" | "tried" | "adopted";
  eligible: number;
  seen: number;
  moved_to: "tried" | "adopted" | null;
  moved_ms: number | null;
  /** The move was back from Adopted to Tried. */
  moved_back: boolean;
}

export interface JudgeToday {
  tasks: number;
  calls: number;
  failed_calls: number;
  /** Sum the tool reported; null = not recorded (Copilot reports none). */
  cost_usd: number | null;
}

export interface TipRow {
  card_id: string;
  tool: string;
  delivered_ms: number;
  noticed: string;
  why: string;
  try_this: string;
  opened: boolean;
  /** The pack's tip id: `next_rung.<move>` for a next-step tip; "" when the card couldn't be read. */
  tip_id: string;
  /** A next-step tip's habit in the first judged tasks after it where it fits; null for tips with no habit. */
  after: { seen: number; checked: number } | null;
}

export interface Data {
  events: number;
  prompts: number;
  turn_ends: number;
  imported_sessions: number;
  imported_events: number;
  store_bytes: number;
  items: Item[];
  items_capped: boolean;
  location: string;
}

export interface Item {
  event_id: string;
  ts_ms: number;
  tool: string;
  kind: "prompt" | "stop";
  text: string | null;
  deleted: boolean;
}

export interface Settings {
  daily_cap: number;
  copilot_tip_stays: "next-prompt" | "two-prompts-or-5-min" | "until-opened";
  /** Where tips show; each on unless the person turned it off. */
  copilot_tip_in_chat: boolean;
  tip_popup: boolean;
  tip_highlight: boolean;
  /** Show where the open task is heading: Home's Right now card and the Copilot CLI status line. */
  live_trend: boolean;
  judge: "auto" | "off";
  judge_daily_tasks: number;
  /** When judging was last switched on or off; null when it was never switched. */
  judge_since_ms: number | null;
  /** Read Copilot's own history file for messages sent while the agent works. */
  copilot_mid_turn: "on" | "off";
  /** At each turn's end, count what the agent did (tool calls, commands, files, sub-agents). */
  agent_actions: "on" | "off";
  next_rung_tips: "auto" | "on" | "off";
  /** Next-step tips may show now. */
  next_rung_on: boolean;
  guess_checked: number;
  guess_right: number;
  guess_auto_off: boolean;
  /** Tips turned off with "Don't show this again". */
  tips_off: string[];
  /** Every next-step tip AIQ can show, in pack order. */
  next_tips: NextTip[];
  /** Jobs and kinds of task whose next-step tips the accuracy check paused (still paused first). */
  accuracy_paused: PausedScope[];
}

/** A job or kind of task the accuracy check paused: a second model agreed with the judge under 8 in 10. */
export interface PausedScope {
  /** run_ship or code_change/fix_bug. */
  scope: string;
  /** "Run or ship it" or "Code a change: Fixing a bug". */
  label: string;
  agreed: number;
  checked: number;
  /** The person chose "Keep showing" for this evidence. */
  kept_on: boolean;
}

export interface NextTip {
  tip_id: string;
  habit: string;
  /** code_change, write_doc, ask_research, run_ship or any. */
  job: string;
}

/** A command's answer: done, or a sentence the person can act on. */
export type Outcome = { ok: true } | { ok: false; message: string };

/** First-run setup (design aiq-home-deferred-pages E5–E7). */
export interface Setup {
  done: boolean;
  tools: ToolFound[];
}

export interface ToolFound {
  tool: "claude-code" | "copilot";
  hooked: boolean;
  present: boolean;
  install: string;
}

/** One task with its prompts, from the `task` command (crates/aiq-home/src/detail.rs). */
export interface TaskDetail {
  task: TaskRow;
  prompts: TaskPromptText[];
  read_ms: number;
}

export interface TaskPromptText {
  event_id: string;
  ts_ms: number | null;
  text: string | null;
  deleted: boolean;
}
