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
