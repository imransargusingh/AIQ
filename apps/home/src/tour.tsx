// The tour (design home-tour): nine steps over Home's views, shown on a fixed example snapshot so every view is
// full on a new install, whose own Home stays empty for days. Never starts by itself: Home offers it once after
// setup, and Ctrl K, Settings and the getting-started list start it again. A spotlight on each view and a coach
// mark beside it; the page under it can't be clicked, Esc ends it on any step, and the person's own data is back
// when it ends. Nothing it shows is written anywhere.
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Page } from "./ui";
import type { Snapshot } from "./types";

export interface TourStep {
  page: Exclude<Page, "task" | "kind" | "setup">;
  /** Open Tasks on this view. */
  view?: "day";
  /** The element the spotlight goes round; a list is tried in order, the first found wins. */
  target: string | string[];
  title: string;
  /** Any number in it (days, tips a day) comes from the snapshot the tour shows, never typed here: a typed
   * number drifts when the app's own window changes (the 14-day trend that became 30 days, #202). */
  body: (s: Snapshot) => string;
  /** A sample of what a tip looks like in the person's tool. */
  tip?: string;
}

export const STEPS: TourStep[] = [
  {
    page: "home",
    target: "[data-testid=right-now]",
    title: "Right now",
    body: () => "Tasks open now, with how far agents are taking each. When nothing runs, it lists each connected tool and when it was last active.",
  },
  {
    page: "home",
    target: "section[aria-labelledby=rg]",
    title: "How you work with agents",
    body: (s) =>
      `Your analyzed tasks by rung, from Assisted to Factory. The change compares the last ${s.coaching.shift_days} days with the ${s.coaching.shift_days} before.`,
  },
  {
    page: "home",
    target: "section[aria-labelledby=pg]",
    title: "Is each kind of task moving up?",
    body: (s) =>
      `Each kind of task on the ladder, last ${s.coaching.progress_days} days against the ${s.coaching.progress_days} before. Below: the next step up, and habits that are growing.`,
  },
  {
    page: "home",
    target: "section[aria-labelledby=hb]",
    title: "Automated, not counted",
    body: () => "AIQ leaves out runs an app or script started, so your counts show your own work. If a folder is yours, choose Count this folder.",
  },
  {
    page: "tasks",
    target: "table[aria-label='Kinds of task']",
    title: "Tasks, by kind",
    body: (s) =>
      `Each kind of task: where it sits today, where it could be, and how it moved over ${s.coaching.progress_days} days. Open a kind for its page.`,
  },
  {
    page: "tasks",
    view: "day",
    target: "section[aria-labelledby=dayhd]",
    title: "Your day",
    body: () => "One row per session, named by kind, with each task's rung. Long quiet gaps shrink so the work stands out.",
  },
  {
    page: "tips",
    // The rung whose habits include one growing lately, so its change line is in the spotlight; else the first.
    target: ["section[aria-labelledby^=hab-]:has(.hc)", "section[aria-labelledby^=hab-]"],
    title: "Habits",
    body: () => "Habits by the rung they help you reach: the ones you're trying, the ones you've adopted, and which are growing lately.",
  },
  {
    page: "tips",
    target: "section[aria-labelledby=th]",
    title: "Tips show where you work",
    body: (s) =>
      `Up to ${s.settings.daily_cap} a day, in Claude Code, Copilot CLI or VS Code after a turn ends. Never while you type. Settings sets how many.`,
    tip: "Give the agent the failing test and let it run it until it passes.",
  },
  {
    page: "settings",
    target: ".secnav",
    title: "Settings",
    body: () => "Tips, what AIQ keeps, analysis, updates, theme and tools. Take this tour again from Settings or Ctrl K.",
  },
];

/** The example snapshot, loaded only when a tour starts. It keeps the person's own status and update state, so
 * the side panel and titlebar stay true while the views show the example. */
export async function exampleSnapshot(own: Snapshot): Promise<Snapshot> {
  const m = await import("../tests/fixtures/tour.json");
  const ex = structuredClone(m.default) as unknown as Snapshot;
  return { ...ex, status: own.status, update: own.update, settings: { ...ex.settings, daily_cap: own.settings.daily_cap } };
}

interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

const PAD = 6;
const GAP = 16;
const POP_W = 320;

/** Where the coach mark goes: below the target when it fits, else above, else beside it, kept on screen. */
export function place(t: Box | null, pop: { w: number; h: number }, vw: number, vh: number): { left: number; top: number } {
  const clampX = (x: number) => Math.max(GAP, Math.min(vw - pop.w - GAP, x));
  const clampY = (y: number) => Math.max(GAP, Math.min(vh - pop.h - GAP, y));
  if (!t) return { left: clampX((vw - pop.w) / 2), top: clampY((vh - pop.h) / 2) };
  const below = t.top + t.height + PAD + GAP;
  if (below + pop.h <= vh - GAP) return { left: clampX(t.left), top: below };
  const above = t.top - PAD - GAP - pop.h;
  if (above >= GAP) return { left: clampX(t.left), top: above };
  const right = t.left + t.width + PAD + GAP;
  if (right + pop.w <= vw - GAP) return { left: right, top: clampY(t.top) };
  return { left: clampX(vw - pop.w - GAP), top: clampY(vh - pop.h - GAP) };
}

export function Tour({ snap, step, setStep, end }: { snap: Snapshot; step: number; setStep: (n: number) => void; end: () => void }) {
  const s = STEPS[step];
  const last = step === STEPS.length - 1;
  const [box, setBox] = useState<Box | null>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const pop = useRef<HTMLDivElement>(null);
  const next = useRef<HTMLButtonElement>(null);

  // Find the step's view once its page has drawn it (a page renders a frame or two after navigation), bring it
  // into view, then follow it on resize and scroll. No target after a second: the coach mark sits centred.
  useEffect(() => {
    let raf = 0;
    let el: Element | null = null;
    const started = performance.now();
    const measure = () => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      setBox({ left: r.left, top: r.top, width: r.width, height: r.height });
    };
    const find = () => {
      el = ([] as string[]).concat(s.target).reduce<Element | null>((f, q) => f ?? document.querySelector(q), null);
      if (el) {
        // Scroll AIQ home's own scroller (main), never the window, so the titlebar and side panel stay put. The
        // view sits a third of the way down, leaving room for the coach mark.
        const main = document.getElementById("main");
        if (main?.contains(el)) {
          const r = el.getBoundingClientRect();
          const m = main.getBoundingClientRect();
          main.scrollTop += r.top - m.top - Math.max(GAP, (main.clientHeight - r.height) / 3);
        }
        measure();
      } else if (performance.now() - started < 1000) raf = requestAnimationFrame(find);
      else setBox(null);
    };
    setBox(null);
    setPos(null);
    raf = requestAnimationFrame(find);
    window.addEventListener("resize", measure);
    window.addEventListener("scroll", measure, true);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", measure, true);
    };
  }, [s, step]);

  useLayoutEffect(() => {
    const r = pop.current?.getBoundingClientRect();
    if (!r) return;
    setPos(place(box, { w: r.width, h: r.height }, window.innerWidth, window.innerHeight));
  }, [box]);

  // A frame later, so a closing palette's focus return can't take it back.
  useEffect(() => {
    const r = requestAnimationFrame(() => next.current?.focus());
    return () => cancelAnimationFrame(r);
  }, [step]);

  const go = useCallback((n: number) => (n < 0 ? undefined : n >= STEPS.length ? end() : setStep(n)), [end, setStep]);

  // Keys work wherever focus is (a closing palette can hand focus back to the page under the tour).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        end();
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        go(step + 1);
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        go(step - 1);
      } else if (e.key === "Tab") {
        // Focus stays in the coach mark while the tour is open.
        const f = Array.from(pop.current?.querySelectorAll<HTMLElement>("button") ?? []);
        if (!f.length) return;
        const i = f.indexOf(document.activeElement as HTMLElement);
        e.preventDefault();
        f[i < 0 ? f.length - 1 : (i + (e.shiftKey ? f.length - 1 : 1)) % f.length].focus();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [step, go, end]);

  return (
    <div className="tour">
      {/* The page stays visible but can't be clicked; a click outside the coach mark does nothing. */}
      <div className={`tour-block${box ? "" : " dim"}`} aria-hidden="true" />
      {box && (
        <div
          className="tour-spot"
          aria-hidden="true"
          style={{ left: box.left - PAD, top: box.top - PAD, width: box.width + 2 * PAD, height: box.height + 2 * PAD }}
        />
      )}
      <div
        ref={pop}
        className="tour-pop"
        role="dialog"
        aria-modal="true"
        aria-labelledby="tourhd"
        aria-describedby="tourbd"
        style={{ width: POP_W, left: pos?.left ?? -9999, top: pos?.top ?? 0 }}
      >
        <button className="tour-x" onClick={end} aria-label="Close the tour">✕</button>
        <h2 id="tourhd">{s.title}</h2>
        <p id="tourbd" className="mute">{s.body(snap)}</p>
        {s.tip && (
          <p className="tour-tip">
            <span className="b">AIQ tip:</span> {s.tip}
          </p>
        )}
        <div className="tour-dots" aria-hidden="true">
          {STEPS.map((_, n) => (
            <i key={n} className={n === step ? "on" : undefined} />
          ))}
        </div>
        <div className="row tour-row">
          <span className="meta">
            {step + 1} of {STEPS.length} · Esc to close
          </span>
          {step > 0 && (
            <button className="btn" onClick={() => go(step - 1)}>Back</button>
          )}
          <button ref={next} className="btn pri" onClick={() => go(step + 1)}>
            {last ? "Done" : "Next"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Home's one-time offer, in the corner after setup: not a modal, and either answer means it never shows again. */
export function TourOffer({ start, decline }: { start: () => void; decline: () => void }) {
  return (
    <aside className="tour-offer" aria-labelledby="tourof">
      <h2 id="tourof">New to AIQ?</h2>
      <p className="mute">A one-minute tour of what each view will show, on example data.</p>
      <div className="row">
        <button className="btn pri" onClick={start}>Take the tour</button>
        <button className="btn" onClick={decline}>Not now</button>
      </div>
    </aside>
  );
}
