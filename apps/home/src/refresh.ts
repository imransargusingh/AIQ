/**
 * At most one snapshot read at a time. On a busy laptop a read can take seconds (measured 2026-11-01:
 * up to 17 s while the whole machine was slow), so timer ticks and focus events must not pile up reads,
 * and an older answer must never land after a newer one. A request made during a read runs once after it.
 */
export function singleFlight(run: () => Promise<void>): () => void {
  let busy = false;
  let again = false;
  const go = (): void => {
    if (busy) {
      again = true;
      return;
    }
    busy = true;
    run().finally(() => {
      busy = false;
      if (again) {
        again = false;
        go();
      }
    });
  };
  return go;
}
