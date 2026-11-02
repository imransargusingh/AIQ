// The bridge to the Tauri shell (apps/home/src-tauri). Ten commands; nothing else leaves the page.
// Outside Tauri (the UI tests) a fixture stands in, set on window by the test before the page loads.
import type { Outcome, Snapshot, TaskDetail } from "./types";

export type SettingKey = "daily_cap" | "copilot_tip_stays" | "next_rung_tips";
/** The reasons the core accepts (crates/aiq-core/src/coaching.rs FLAG_REASONS). */
export type FlagReason = "evidence_wrong" | "not_useful" | "blocked" | "other";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __AIQ_FIXTURE__?: { snapshot: Snapshot; task?: TaskDetail; calls: { cmd: string; args: unknown }[]; fail?: string[] };
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
      if (!s.status.core) out = { ok: false, message: "AIQ isn't running, so settings can't change right now. It starts with your next prompt in Claude Code or Copilot." };
      else {
        if (args.key === "daily_cap") s.settings.daily_cap = Number(args.value);
        if (args.key === "copilot_tip_stays") s.settings.copilot_tip_stays = args.value as Snapshot["settings"]["copilot_tip_stays"];
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
    case "connect_tools":
      if (f.fail?.includes(cmd)) {
        out = { ok: false, message: "AIQ couldn't connect your tools. Could not write the settings file. Run aiq install in a terminal to see why.", changed: [] };
        break;
      }
      for (const t of s.setup.tools) if (t.present) t.hooked = true;
      out = { ok: true, changed: ["C:\\Users\\you\\.copilot\\hooks\\aiq.json"] };
      break;
    case "finish_setup":
      s.setup.done = true;
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
  /** Connect every tool found but not hooked yet (runs `aiq install --yes`). */
  connectTools: () => invoke<{ ok: boolean; message?: string; changed: string[] }>("connect_tools"),
  /** "This doesn't fit" (flag, with a reason) or its undo, for one kind of task's next step. */
  guidanceFlag: (job: string, taskType: string, moveId: string, action: "flag" | "undo", reason: FlagReason | null) =>
    invoke<Outcome>("guidance_flag", { job, taskType, moveId, action, reason }),
  /** Turn a tip back on after "Don't show this again". */
  tipOn: (tipId: string) => invoke<Outcome>("tip_on", { tipId }),
};
