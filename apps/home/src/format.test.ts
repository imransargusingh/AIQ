import { describe, expect, it } from "vitest";
import { bytes, folderName, jobName, jobOf, jobOptions, plural, rungLabel, sentence, sortTasks, stateLabel, topPlaces, when } from "./format";
import type { TaskRow } from "./types";

const t = (over: Partial<TaskRow>): TaskRow => ({
  task_id: "x", tool: "claude-code", folder: null, first_ms: 0, last_ms: 0, prompts: 1, origin: "live", job: null, rung: null, judgment: null, ...over,
});

describe("format", () => {
  it("names jobs and judgment states in words", () => {
    expect(jobName("code_change")).toBe("Code a change");
    expect(jobName("unclassified")).toBe("Unclassified");
    expect(jobOf(t({}))).toBe("Not classified yet");
    expect(jobOf(t({ job: "write_doc" }))).toBe("Write a spec or doc");
    expect(stateLabel("uncertain")).toBe("Uncertain");
    expect(stateLabel("blocked")).toBe("Capped by a blocker");
    expect(stateLabel("judged")).toBe("");
  });

  it("names rungs first, numbers second", () => {
    expect(rungLabel(2)).toBe("Directed (2)");
    expect(rungLabel(5)).toBe("Factory (5)");
  });
  it("shows the folder's own name", () => {
    expect(folderName("c:\\users\\imran\\source\\repos\\aiq")).toBe("aiq");
    expect(folderName("/home/me/work/")).toBe("work");
    expect(folderName(null)).toBe("—");
  });
  it("says today, yesterday, else the date, in the core's local time", () => {
    const now = Date.UTC(2026, 10, 1, 12, 0); // Sun 1 Nov 12:00 UTC
    expect(when(now - 30 * 60_000, now, 0)).toBe("Today 11:30");
    expect(when(now - 86_400_000, now, 0)).toBe("Yesterday 12:00");
    expect(when(now - 2 * 86_400_000, now, 60)).toBe("Fri 30 Oct 13:00");
    // 23:30 UTC on 31 Oct is 00:30 on 1 Nov at +60: today, not yesterday.
    expect(when(Date.UTC(2026, 9, 31, 23, 30), now, 60)).toBe("Today 00:30");
  });
  it("formats sizes and counts", () => {
    expect(bytes(512)).toBe("512 bytes");
    expect(bytes(2048)).toBe("2 KB");
    expect(bytes(3 * 1048576)).toBe("3.0 MB");
    expect(plural(1, "task")).toBe("1 task");
    expect(plural(1284, "task")).toBe("1,284 tasks");
  });
  it("sorts tasks and keeps newest first on ties", () => {
    const rows = [t({ task_id: "a", prompts: 2, first_ms: 1 }), t({ task_id: "b", prompts: 5, first_ms: 2 }), t({ task_id: "c", prompts: 2, first_ms: 3 })];
    expect(sortTasks(rows, "prompts", -1).map((r) => r.task_id)).toEqual(["b", "c", "a"]);
    expect(sortTasks(rows, "when", 1).map((r) => r.task_id)).toEqual(["a", "b", "c"]);
  });
  it("job filter: buttons up to 4 jobs, dropdown beyond", () => {
    expect(jobOptions([t({})])).toEqual({ options: ["All", "Not classified yet"], dropdown: false });
    const many = ["a", "b", "c", "d", "e"].map((j) => t({ job: j }));
    expect(jobOptions(many).dropdown).toBe(true);
  });
  it("ranks places by tasks", () => {
    const rows = [t({ folder: "f1" }), t({ folder: "f2" }), t({ folder: "f2" })];
    expect(topPlaces(rows)[0]).toMatchObject({ folder: "f2", tasks: 2 });
  });
  it("capitalises tip text shown on its own", () => {
    expect(sentence("you've reworded this")).toBe("You've reworded this");
    expect(sentence("")).toBe("");
  });
});
