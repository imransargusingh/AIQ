import { describe, expect, it } from "vitest";
import { answerWords, tipView } from "./taskTips";
import type { HeldTip, TaskTips, TipRow } from "./types";

const tip: TipRow = {
  card_id: "c1", tool: "copilot-cli", delivered_ms: 1, noticed: "n", why: "w", try_this: "t", opened: false,
  tip_id: "reprompt.restate", after: null, task_id: "t1", answer: null,
};
const held: HeldTip = {
  card_id: "c2", decided_ms: 1, noticed: "n", try_this: "t", tip_id: "x", outcome: "hold", reason: "daily_cap",
  reason_words: "daily tip limit reached", reason_help: "You can change Tips a day in Settings.",
};
const none: TaskTips = { shown: [], held: null, step: null, step_words: "", step_help: "" };
const live = { origin: "live" as const };
const done = { state: "analyzed" as const };

describe("tipView (design task-tips)", () => {
  it("a shown tip wins over a held one", () => {
    expect(tipView({ ...none, shown: [tip], held }, live, done).kind).toBe("shown");
  });
  it("a held tip says why when nothing was shown", () => {
    expect(tipView({ ...none, held }, live, done)).toEqual({ kind: "held", tip: held });
  });
  it("no tip: the funnel's reason in a sentence, with what helps", () => {
    const v = tipView({ ...none, step: "no_match", step_words: "kind of task not recognised", step_help: "AIQ reads how a task starts." }, live, done);
    expect(v).toEqual({ kind: "none", why: "Kind of task not recognised.", help: "AIQ reads how a task starts." });
  });
  it("a label still on its way on an open task is not called lost", () => {
    const t = { ...none, step: "label_asked", step_words: "kind label never came back" };
    expect(tipView(t, live, { state: "open" })).toMatchObject({ why: "AIQ is still working out what kind of task this is." });
    expect(tipView(t, live, done)).toMatchObject({ why: "Kind label never came back." });
  });
  it("imported tasks and unrecorded ones say so, never a guessed reason", () => {
    expect(tipView(none, { origin: "imported" }, done)).toMatchObject({ why: "AIQ gives tips while you work, not for tasks it imported." });
    expect(tipView(none, live, done)).toMatchObject({ why: "AIQ didn't record why for this task." });
    expect(tipView({ ...none, step: "decided", step_words: "x" }, live, done)).toMatchObject({ why: "AIQ didn't record why for this task." });
  });
  it("answers read as the tip's own buttons", () => {
    expect([answerWords("helpful"), answerWords("not_helpful"), answerWords("snooze"), answerWords("off"), answerWords(null)]).toEqual([
      "You said: Helpful", "You said: Not helpful", "You snoozed it", "You turned it off", null,
    ]);
  });
});
