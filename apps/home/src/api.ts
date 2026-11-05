// The bridge to the Tauri shell (apps/home/src-tauri). Sixteen commands; nothing else leaves the page.
// Outside Tauri (the UI tests) a fixture stands in, set on window by the test before the page loads.
import type { Outcome, Snapshot, TaskDetail } from "./types";

export type SettingKey = "daily_cap" | "copilot_tip_stays" | "copilot_tip_in_chat" | "tip_popup" | "tip_highlight" | "live_trend" | "next_rung_tips" | "judge" | "judge_daily_tasks" | "copilot_mid_turn" | "agent_actions" | "update" | `keep_on.${string}` | `project_name.${string}`;
/** The reasons the core accepts (crates/aiq-core/src/coaching.rs FLAG_REASONS). */
export type FlagReason = "evidence_wrong" | "not_useful" | "blocked" | "other";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __AIQ_FIXTURE__?: { snapshot: Snapshot; task?: TaskDetail; calls: { cmd: string; args: unknown }[]; fail?: string[]; checkFinds?: string };
  }
}

async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (window.__TAURI_INTERNALS__) {
    const core = await import("@tauri-apps/api/core");
    return core.invoke<T>(cmd, args);
  }
  return fixture<T>(cmd, args);
}

/** Test stand-in: behaves like the shell over a fixed snapshot, and records each call. */
function fixture<T>(cmd: string, args: Record<string, unknown>): Promise<T> {
  const f = window.__AIQ_FIXTURE__;
  if (!f) return Promise.reject(new Error("AIQ home must run inside the AIQ app."));
  f.calls.push({ cmd, args });
  const s = f.snapshot;
  let out: unknown;
  switch (cmd) {
    case "snapshot":
      out = s;
      break;
    case "pause":
      s.status.paused = Boolean(args.on);
      out = s;
      break;
    case "set_setting":
      if (!s.status.core) out = { ok: false, message: "AIQ is starting, so this can't change yet. Try again in a moment." };
      else {
        if (args.key === "daily_cap") s.settings.daily_cap = Number(args.value);
        if (args.key === "judge") [s.settings.judge, s.settings.judge_since_ms] = [args.value as Snapshot["settings"]["judge"], s.now_ms];
        if (args.key === "judge_daily_tasks") s.settings.judge_daily_tasks = Number(args.value);
        if (args.key === "copilot_mid_turn") s.settings.copilot_mid_turn = args.value as Snapshot["settings"]["copilot_mid_turn"];
        if (args.key === "agent_actions") s.settings.agent_actions = args.value as Snapshot["settings"]["agent_actions"];
        if (args.key === "update") s.update.mode = args.value as Snapshot["update"]["mode"];
        if (args.key === "copilot_tip_stays") s.settings.copilot_tip_stays = args.value as Snapshot["settings"]["copilot_tip_stays"];
        if (args.key === "copilot_tip_in_chat" || args.key === "tip_popup" || args.key === "tip_highlight" || args.key === "live_trend") s.settings[args.key] = args.value === "on";
        const keep = String(args.key).startsWith("keep_on.") ? s.settings.accuracy_paused.find((p) => `keep_on.${p.scope}` === args.key) : undefined;
        if (String(args.key).startsWith("keep_on.") && !keep) {
          out = { ok: false, message: "Not changed: AIQ hasn't paused tips for that." };
          break;
        }
        if (keep) keep.kept_on = args.value === "on";
        if (String(args.key).startsWith("project_name.")) {
          const p = s.projects.find((x) => `project_name.${x.key}` === args.key);
          const v = String(args.value).trim();
          if (p) [p.name, p.renamed] = v ? [v, true] : [p.key.split(/[\\/]/).filter(Boolean).pop() ?? p.key, false];
        }
        if (args.key === "next_rung_tips") {
          s.settings.next_rung_tips = args.value as Snapshot["settings"]["next_rung_tips"];
          s.settings.next_rung_on = args.value === "on" || (args.value === "auto" && !s.settings.guess_auto_off);
        }
        out = { ok: true };
      }
      break;
    case "forget_all":
      if (args.confirm !== "forget") out = { ok: false, message: "Type forget to confirm." };
      else {
        s.data = { ...s.data, events: 0, prompts: 0, turn_ends: 0, imported_events: 0, imported_sessions: 0, items: [], store_bytes: 0 };
        s.tasks = [];
        s.tips = [];
        s.home = { ...s.home, tasks_in_window: 0, imported_in_window: 0, by_tool: [], tips_today: 0, weeks: s.home.weeks.map((w) => ({ ...w, tasks: 0 })) };
        s.status.last_event_ms = null;
        out = { ok: true };
      }
      break;
    case "task":
      out = f.task && f.task.task.task_id === args.taskId ? f.task : null;
      break;
    case "forget_item": {
      const item = s.data.items.find((i) => i.event_id === args.eventId && i.text != null);
      if (!item) out = { ok: false, message: "Not deleted: that item has no stored text to delete." };
      else {
        item.text = null;
        item.deleted = true;
        for (const p of f.task?.prompts ?? []) if (p.event_id === args.eventId) [p.text, p.deleted] = [null, true];
        out = { ok: true };
      }
      break;
    }
    case "guidance_flag": {
      // Written to the store even while the core is down (control falls back to a direct write).
      const k = s.coaching.kinds.find((x) => x.job === args.job && x.task_type === args.taskType);
      if (!k || !k.next || k.next.move_id !== args.moveId) out = { ok: false, message: "Not changed: no such next step." };
      else {
        k.state = args.action === "flag" ? "flagged" : "coach";
        s.coaching.opportunities = s.coaching.kinds.filter((x) => x.state === "coach").slice(0, 3);
        out = { ok: true };
      }
      break;
    }
    case "tip_on":
      s.settings.tips_off = s.settings.tips_off.filter((t) => t !== args.tipId);
      out = { ok: true };
      break;
    case "tip_off":
      if (!s.settings.tips_off.includes(args.tipId as string)) s.settings.tips_off.push(args.tipId as string);
      out = { ok: true };
      break;
    case "connect_tools":
      if (f.fail?.includes(cmd)) {
        out = { ok: false, message: "AIQ couldn't connect your tools. Could not write the settings file. Run aiq install in a terminal to see why.", changed: [] };
        break;
      }
      for (const t of s.setup.tools) if (t.present) t.hooked = true;
      // aiq install turns background judging on when it was never set (the fixtures' "off" stands for unset here).
      s.settings.judge = "auto";
      out = { ok: true, changed: ["C:\\Users\\you\\.copilot\\hooks\\aiq.json"] };
      break;
    case "run_update":
      // The real shell closes the window after replying; the fixture only records the call. A "check" behaves
      // like upgrade.ps1 when the test sets `checkFinds`: a version stages it (waiting), "none" finds nothing,
      // "error" can't reach GitHub; it writes its check time.
      out = f.fail?.includes(cmd) ? { ok: false, message: "Could not start: updates run only for AIQ installed with install.ps1 on Windows." } : { ok: true };
      if (args.action === "check" && f.checkFinds !== undefined && !f.fail?.includes(cmd)) {
        const found = f.checkFinds;
        setTimeout(() => {
          s.update.checked_ms = Date.now();
          s.update.check_error = found === "error" ? "Unable to connect to the remote server" : null;
          if (found !== "none" && found !== "error") Object.assign(s.update, { note: "waiting", available: found, latest: found });
          else if (found === "none") s.update.latest = `v${s.update.running}`;
        }, 300);
      }
      break;
    case "disconnect_tool": {
      const t = s.setup.tools.find((x) => x.tool === args.tool);
      if (!t || !t.hooked) out = { ok: false, message: "Not changed: AIQ isn't connected to that tool.", changed: [] };
      else {
        t.hooked = false;
        out = { ok: true, changed: [t.tool === "copilot" ? "C:\\Users\\you\\.copilot\\hooks\\aiq.json" : "C:\\Users\\you\\.claude\\settings.json"] };
      }
      break;
    }
    case "include_folder":
      if (!s.status.core) out = { ok: false, message: "AIQ is starting, so this can't change yet. Try again in a moment." };
      else if (!s.home.set_aside.some((x) => x.folder === args.folder)) out = { ok: false, message: "Not changed: that folder isn't set aside." };
      else {
        s.home.set_aside = s.home.set_aside.filter((x) => x.folder !== args.folder);
        out = { ok: true };
      }
      break;
    case "judge_preview":
      out = { ok: true, text: "AIQ analyzes each task you finish with AI.\n\n1 task is waiting to be analyzed. Nothing was sent.\n\nThe next task would be sent to your Copilot CLI like this:\n\n<task>refactor the login handler</task>" };
      break;
    case "open_privacy":
      out = { ok: true };
      break;
    case "finish_setup":
      s.setup.done = true;
      out = s;
      break;
    case "tour_seen":
      s.setup.tour_seen = true;
      out = s;
      break;
    default:
      return Promise.reject(new Error(`unknown command ${cmd}`));
  }
  return Promise.resolve(structuredClone(out) as T);
}

export const api = {
  snapshot: () => invoke<Snapshot>("snapshot"),
  pause: (on: boolean) => invoke<Snapshot>("pause", { on }),
  setSetting: (key: SettingKey, value: string) => invoke<Outcome>("set_setting", { key, value }),
  forgetAll: (confirm: string) => invoke<Outcome>("forget_all", { confirm }),
  task: (taskId: string) => invoke<TaskDetail | null>("task", { taskId }),
  forgetItem: (eventId: string) => invoke<Outcome>("forget_item", { eventId }),
  finishSetup: () => invoke<Snapshot>("finish_setup"),
  /** The tour was taken, skipped or declined: Home stops offering it. */
  tourSeen: () => invoke<Snapshot>("tour_seen"),
  /** Connect every tool found but not hooked yet (runs `aiq install --yes`). */
  connectTools: () => invoke<{ ok: boolean; message?: string; changed: string[] }>("connect_tools"),
  /** "This doesn't fit" (flag, with a reason) or its undo, for one kind of task's next step. */
  guidanceFlag: (job: string, taskType: string, moveId: string, action: "flag" | "undo", reason: FlagReason | null) =>
    invoke<Outcome>("guidance_flag", { job, taskType, moveId, action, reason }),
  /** Turn a tip back on after "Don't show this again". */
  tipOn: (tipId: string) => invoke<Outcome>("tip_on", { tipId }),
  /** Settings' per-tip switch: turn a tip off before it shows (same as "Don't show this again"). */
  tipOff: (tipId: string) => invoke<Outcome>("tip_off", { tipId }),
  /** Settings' Disconnect: `aiq uninstall <tool> --yes`, which restores the tool's settings from AIQ's backup. */
  disconnectTool: (tool: "claude-code" | "copilot") => invoke<{ ok: boolean; message?: string; changed: string[] }>("disconnect_tool", { tool }),
  /** Home's Include: AIQ watches a folder it set aside as automated again. */
  includeFolder: (folder: string) => invoke<Outcome>("include_folder", { folder }),
  /** What judging would send for the next task (`aiq judge --dry-run`). Nothing is sent. */
  judgePreview: () => invoke<{ ok: boolean; text: string }>("judge_preview"),
  /** Open the privacy page in the default browser (a fixed address). */
  openPrivacy: () => invoke<Outcome>("open_privacy"),
  /** Install the waiting update or go back (AIQ home closes and reopens), or check now (it stays open);
   * `check_if_due` checks only when an hour has passed since the last check (Home's own hourly ask). */
  runUpdate: (action: "install" | "rollback" | "check" | "check_if_due" | "check_on_open") => invoke<Outcome>("run_update", { action }),
};
