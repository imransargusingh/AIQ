// "Update now" (design auto-update U5h, Imran 2026-11-04): one press checks, downloads and installs. A waiting
// update installs at once; otherwise AIQ home runs the check with the window open, watches update.json through
// the snapshot, and installs as soon as the download is checked. Every step shows on screen, not only to
// screen readers (2026-11-04 laptop: "Check for updates now" said nothing, so it was pressed twice).
import type { Update } from "./types";

export type UpdatePhase = "idle" | "checking" | "installing";

export const ver = (v: string | null | undefined) => (v ?? "").replace(/^v/, "");

/** `1.2.3` as numbers; null for anything else. */
function nums(v: string): number[] | null {
  const p = ver(v).split(".").map(Number);
  return p.every((n) => Number.isInteger(n) && n >= 0) ? p : null;
}

function newer(a: string, b: string): boolean {
  const x = nums(a);
  const y = nums(b);
  if (!x || !y) return false;
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d) return d > 0;
  }
  return false;
}

/** What a check started at `since` (ms) came to, read from the snapshot. `pending` until the script writes its
 * check time (whole seconds, so a second of slack). */
export function checkOutcome(u: Update, since: number): "pending" | "found" | "newest" | "skipped" | "error" {
  if (u.note === "waiting") return "found";
  if (u.checked_ms == null || u.checked_ms < since - 1000) return "pending";
  if (u.check_error) return "error";
  if (u.latest && newer(u.latest, u.running)) return "skipped";
  return "newest";
}

/** The sentence for an outcome; null for found (it installs) and pending. */
export function outcomeText(o: ReturnType<typeof checkOutcome>, u: Update): string | null {
  switch (o) {
    case "newest":
      return `You have the newest AIQ (${ver(u.running)}).`;
    case "skipped":
      return `AIQ ${ver(u.latest)} is the version you went back from, so it isn't installed.`;
    case "error":
      return "Couldn't reach GitHub. Try again in a few minutes.";
    default:
      return null;
  }
}

/** How long Home waits for a check before saying it is still running. */
export const CHECK_WAIT_MS = 180_000;
export const CHECK_POLL_MS = 2_000;

/** AIQ home installs a waiting update by itself once its window has been in the background this long (design
 * auto-update U5n; crates/aiq-core/src/update.rs AWAY_INSTALL_MS). Laptop, 2026-11-08: Home stayed open for
 * hours, so 0.1.19 to 0.1.21 each downloaded and none installed. */
export const AWAY_INSTALL_MS = 10 * 60_000;

/** Should Home ask the shell for an away install now? `awaySince`: when the window last lost focus or was
 * minimised, null while the person is in it. The shell checks the rest again (the stored setting, a try in the
 * last 30 minutes), so this only keeps Home from asking when nothing can happen. */
export function awayInstallDue(u: Update, awaySince: number | null, now: number, phase: UpdatePhase): boolean {
  return (
    u.supported &&
    u.mode === "auto" &&
    u.note === "waiting" &&
    phase === "idle" &&
    awaySince !== null &&
    now - awaySince >= AWAY_INSTALL_MS
  );
}
