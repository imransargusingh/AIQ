import { describe, expect, it } from "vitest";
import { AWAY_INSTALL_MS, awayInstallDue, checkOutcome, outcomeText } from "./updateNow";
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

describe("awayInstallDue (design auto-update U5n)", () => {
  const waiting: Update = { ...base, note: "waiting", available: "v0.1.4", blocked: "AIQ home is open" };
  const away = 5_000_000;
  it("installs a waiting update once Home has been in the background for 10 minutes", () => {
    expect(awayInstallDue(waiting, away, away + AWAY_INSTALL_MS, "idle")).toBe(true);
    expect(awayInstallDue(waiting, away, away + AWAY_INSTALL_MS - 1, "idle")).toBe(false);
  });
  it("never while the person is in the window, or a check or install already runs", () => {
    expect(awayInstallDue(waiting, null, away + 10 * AWAY_INSTALL_MS, "idle")).toBe(false);
    expect(awayInstallDue(waiting, away, away + AWAY_INSTALL_MS, "checking")).toBe(false);
    expect(awayInstallDue(waiting, away, away + AWAY_INSTALL_MS, "installing")).toBe(false);
  });
  it("only under Update automatically, only when something waits, only for an installed copy", () => {
    const t = away + AWAY_INSTALL_MS;
    expect(awayInstallDue({ ...waiting, mode: "ask" }, away, t, "idle")).toBe(false);
    expect(awayInstallDue({ ...waiting, mode: "off" }, away, t, "idle")).toBe(false);
    expect(awayInstallDue({ ...waiting, note: "none" }, away, t, "idle")).toBe(false);
    expect(awayInstallDue({ ...waiting, supported: false }, away, t, "idle")).toBe(false);
  });
});
