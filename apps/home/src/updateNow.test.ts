import { describe, expect, it } from "vitest";
import { checkOutcome, outcomeText } from "./updateNow";
import type { Update } from "./types";

const base: Update = {
  supported: true,
  mode: "auto",
  running: "0.1.3",
  note: "none",
  available: null,
  blocked: null,
  from: null,
  at_ms: null,
  message: null,
  checked_ms: 1_000_000,
  check_error: null,
  latest: "v0.1.3",
  can_rollback: false,
};

describe("checkOutcome", () => {
  it("waits for the script to write a check time at or after the press (whole seconds)", () => {
    expect(checkOutcome(base, 1_002_000)).toBe("pending");
    expect(checkOutcome({ ...base, checked_ms: null }, 0)).toBe("pending");
    expect(checkOutcome(base, 1_000_900)).toBe("newest");
  });
  it("installs as soon as a download waits, even before the check time is read", () => {
    expect(checkOutcome({ ...base, note: "waiting", available: "v0.1.4", checked_ms: 0 }, 1_002_000)).toBe("found");
  });
  it("says a failed check, and a newer release the person went back from", () => {
    expect(checkOutcome({ ...base, check_error: "403" }, 1_000_000)).toBe("error");
    const skipped = { ...base, latest: "v0.1.10" };
    expect(checkOutcome(skipped, 1_000_000)).toBe("skipped");
    expect(outcomeText("skipped", skipped)).toBe("AIQ 0.1.10 is the version you went back from, so it isn't installed.");
    expect(outcomeText("newest", base)).toBe("You have the newest AIQ (0.1.3).");
  });
  it("treats an older or unparsable latest as up to date", () => {
    expect(checkOutcome({ ...base, latest: "v0.1.2" }, 1_000_000)).toBe("newest");
    expect(checkOutcome({ ...base, latest: "nightly" }, 1_000_000)).toBe("newest");
    expect(checkOutcome({ ...base, latest: null }, 1_000_000)).toBe("newest");
  });
});
