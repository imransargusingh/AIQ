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

import { ago, asideText, leftOut, ownWorkLabel, quiet, QUIET_HINT_MS } from "./pages/RightNow";
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
  it("counts set-aside activity as activity and says it isn't counted (Imran 2026-11-08)", () => {
    const tools = (["claude-code", "copilot"] as const).map((tool) => ({ tool, hooked: true, present: true, install: "" }));
    const seen = [
      { surface: "claude-code", last_ms: base.now_ms - 26 * 3_600_000 },
      { surface: "copilot-cli", last_ms: base.now_ms - 60_000 },
    ];
    const aside = [
      { surface: "claude-code", last_ms: base.now_ms - 2 * 60_000, reason: "projects", left_out_today: 0, counted_today: 0 },
      { surface: "copilot-cli", last_ms: base.now_ms - 30 * 60_000, reason: "burst", left_out_today: 0, counted_today: 0 },
    ];
    const q = quiet(at({ starting: [], seen, aside, setup: { ...base.setup, tools } }));
    // Imran 2026-11-11: the automated time sits beside the person's own last turn, never in its place.
    expect(q.tools).toEqual([
      { name: "Claude Code", last: base.now_ms - 26 * 3_600_000, own: base.now_ms - 2 * 60_000, aside: "projects", latest: base.now_ms - 2 * 60_000 },
      { name: "Copilot", last: base.now_ms - 60_000, own: null, aside: null, latest: base.now_ms - 60_000 },
    ]);
    expect(q.silent).toEqual([]);
    expect(asideText(q.aside)).toBe("Claude's own work in a claude.ai project isn't counted. Turns you type yourself count.");
    expect(asideText([{ name: "Copilot", aside: "burst" }])).toBe("Work in Copilot that an app or script runs isn't counted. Turns you type yourself count.");
    expect([ownWorkLabel("projects"), ownWorkLabel("not-typed"), ownWorkLabel("burst")]).toEqual(["Claude's own work", "started by an app", "automated"]);
  });
  it("keeps a tool with only automated work apart from one never heard (Imran 2026-11-11)", () => {
    const tools = [{ tool: "claude-code" as const, hooked: true, present: true, install: "" }];
    const aside = [{ surface: "claude-code", last_ms: base.now_ms - 60_000, reason: "projects", left_out_today: 1, counted_today: 0 }];
    const q = quiet(at({ starting: [], seen: [], aside, setup: { ...base.setup, tools } }));
    expect(q.tools[0]).toMatchObject({ last: null, own: base.now_ms - 60_000, latest: base.now_ms - 60_000 });
    expect(q.silent).toEqual([]);
  });
  it("says when most of a tool's prompts today were left out: 10 or more, and 90% or more (Imran 2026-11-09)", () => {
    const row = (left_out_today: number, counted_today: number, reason = "not-typed") => ({
      surface: "claude-code",
      last_ms: base.now_ms,
      reason,
      left_out_today,
      counted_today,
    });
    expect(leftOut(at({ aside: [row(18, 2)] }))).toEqual([
      { surface: "claude-code", text: "Today AIQ left out 18 of 20 Claude Code prompts as automated, not counted: scheduled or started by an app, not typed." },
    ]);
    expect(leftOut(at({ aside: [row(46, 2, "projects")] }))[0].text).toMatch(/46 of 48 Claude Code prompts.*claude\.ai project\.$/);
    expect(leftOut(at({ aside: [row(9, 0)] }))).toEqual([]);
    expect(leftOut(at({ aside: [row(17, 3)] }))).toEqual([]);
    expect(asideText([{ name: "Claude Code", aside: "not-typed" }])).toBe(
      "Work in Claude Code that an app scheduled or started isn't counted. Turns you type yourself count.",
    );
  });
  it("says how long ago in minutes within the hour, then the day and time", () => {
    const now = base.now_ms;
    expect(ago(now - 20_000, now, 0)).toBe("just now");
    expect(ago(now - 12 * 60_000, now, 0)).toBe("12 min ago");
    expect(ago(now - 3 * 3_600_000, now, 0)).toMatch(/^(Today|Yesterday) \d\d:\d\d$/);
  });
});
