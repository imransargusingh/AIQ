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
  set_aside: { folder: string; reason: string }[];
  tips_today: number;
}

export interface TaskRow {
  task_id: string;
  tool: string;
  folder: string | null;
  first_ms: number;
  last_ms: number;
  prompts: number;
  origin: "live" | "imported";
  /** code_change, write_doc, ask_research or unclassified once judged. */
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
  split: { tasks: number; finished: number; could: number; needs: number; uncertain: number };
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
  judge: "auto" | "off";
  judge_daily_tasks: number;
  next_rung_tips: "auto" | "on" | "off";
  /** Next-step tips may show now. */
  next_rung_on: boolean;
  guess_checked: number;
  guess_right: number;
  guess_auto_off: boolean;
  /** Tips turned off with "Don't show this again". */
  tips_off: string[];
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
