// The bridge to the Tauri shell (apps/home/src-tauri). Four commands; nothing else leaves the page.
// Outside Tauri (the UI tests) a fixture stands in, set on window by the test before the page loads.
import type { Outcome, Snapshot } from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __AIQ_FIXTURE__?: { snapshot: Snapshot; calls: { cmd: string; args: unknown }[] };
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
    default:
      return Promise.reject(new Error(`unknown command ${cmd}`));
  }
  return Promise.resolve(structuredClone(out) as T);
}

export const api = {
  snapshot: () => invoke<Snapshot>("snapshot"),
  pause: (on: boolean) => invoke<Snapshot>("pause", { on }),
  setSetting: (key: "daily_cap" | "copilot_tip_stays", value: string) => invoke<Outcome>("set_setting", { key, value }),
  forgetAll: (confirm: string) => invoke<Outcome>("forget_all", { confirm }),
};
