import { describe, expect, it } from "vitest";
import { singleFlight } from "./refresh";

describe("singleFlight", () => {
  it("runs one read at a time and one more for requests made during it", async () => {
    let running = 0;
    let max = 0;
    let calls = 0;
    const done: (() => void)[] = [];
    const go = singleFlight(
      () =>
        new Promise<void>((r) => {
          calls++;
          running++;
          max = Math.max(max, running);
          done.push(() => {
            running--;
            r();
          });
        }),
    );
    go();
    go();
    go();
    expect(calls).toBe(1);
    done.shift()!();
    await new Promise((r) => setTimeout(r));
    expect(calls).toBe(2);
    done.shift()!();
    await new Promise((r) => setTimeout(r));
    expect(calls).toBe(2);
    expect(max).toBe(1);
  });
  it("keeps working after a failed read", async () => {
    let calls = 0;
    const go = singleFlight(() => (calls++, Promise.reject(new Error("x"))).catch(() => {}));
    go();
    await new Promise((r) => setTimeout(r));
    go();
    await new Promise((r) => setTimeout(r));
    expect(calls).toBe(2);
  });
});
