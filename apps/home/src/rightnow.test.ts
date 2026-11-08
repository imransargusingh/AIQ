import { describe, expect, it } from "vitest";
import { chipOrder, chosen } from "./pages/RightNow";
import type { Live, Snapshot } from "./types";
import fixture from "../tests/fixtures/live-many.json";

const live = (fixture as unknown as Snapshot).live;

describe("Right now, several open tasks (option A)", () => {
  it("chooses the newest activity until a chip is picked", () => {
    expect(chosen(live, null)).toBe(live[0]);
    expect(chosen(live, live[2].task_id)).toBe(live[2]);
  });
  it("falls back to the newest activity when the picked task ends", () => {
    expect(chosen(live.slice(0, 2), live[2].task_id)).toBe(live[0]);
    expect(chosen([], null)).toBeUndefined();
  });
  it("keeps chips in start order whatever the activity order", () => {
    const ids = (l: Live[]) => chipOrder(l).map((x) => x.task_id);
    expect(ids(live)).toEqual(ids([...live].reverse()));
    const starts = chipOrder(live).map((x) => x.first_ms);
    expect(starts).toEqual([...starts].sort((a, b) => a - b));
  });
});

import { ago, quiet, QUIET_HINT_MS } from "./pages/RightNow";
import full from "../tests/fixtures/full.json";

describe("Right now with nothing open (Imran 2026-11-08)", () => {
  const base = full as unknown as Snapshot;
  const at = (s: Partial<Snapshot>) => ({ ...base, ...s }) as Snapshot;
  it("names the state: paused, then not running, then starting, then no tool, then idle", () => {
    expect(quiet(at({ status: { ...base.status, paused: true } })).state).toBe("paused");
    expect(quiet(at({ status: { ...base.status, core_state: "failed" } })).state).toBe("stopped");
    expect(base.starting.length).toBeGreaterThan(0);
    expect(quiet(base).state).toBe("starting");
    expect(quiet(at({ starting: [], setup: { ...base.setup, tools: [] } })).state).toBe("none");
    expect(quiet(at({ starting: [] })).state).toBe("idle");
  });
  it("hints only for connected tools silent longer than the span, or never heard", () => {
    const tools = (["claude-code", "copilot", "vscode"] as const).map((tool) => ({ tool, hooked: true, present: true, install: "" }));
    const seen = [
      { surface: "copilot-cli", last_ms: base.now_ms - QUIET_HINT_MS + 60_000 },
      { surface: "claude-code", last_ms: base.now_ms - QUIET_HINT_MS - 60_000 },
    ];
    const q = quiet(at({ starting: [], seen, setup: { ...base.setup, tools } }));
    expect(q.silent.map((t) => t.name)).toEqual(["Claude Code", "VS Code"]);
  });
  it("says how long ago in minutes within the hour, then the day and time", () => {
    const now = base.now_ms;
    expect(ago(now - 20_000, now, 0)).toBe("just now");
    expect(ago(now - 12 * 60_000, now, 0)).toBe("12 min ago");
    expect(ago(now - 3 * 3_600_000, now, 0)).toMatch(/^(Today|Yesterday) \d\d:\d\d$/);
  });
});
