// Small shared pieces: banners, dialog with focus trap, skeletons, chips.
import { useEffect, useRef, type ReactNode } from "react";
import type { Snapshot } from "./types";
import { when } from "./format";

export type Page = "home" | "tasks" | "tips" | "data" | "settings";

export const PAGES: { id: Page; label: string; key: string }[] = [
  { id: "home", label: "Home", key: "H" },
  { id: "tasks", label: "Tasks", key: "T" },
  { id: "tips", label: "Habits and tips", key: "B" },
  { id: "data", label: "Your data", key: "D" },
  { id: "settings", label: "Settings", key: "S" },
];

export interface Ctx {
  snap: Snapshot;
  go: (p: Page) => void;
  announce: (msg: string) => void;
  togglePause: () => void;
  refresh: () => void;
}

/** The page title takes focus on every page change, so a screen reader says where the person is. */
export function Title({ children, sr = false }: { children: ReactNode; sr?: boolean }) {
  const ref = useRef<HTMLHeadingElement>(null);
  useEffect(() => ref.current?.focus({ preventScroll: true }), []);
  return (
    <h1 ref={ref} tabIndex={-1} className={sr ? "sr" : undefined} style={{ outline: "none" }}>
      {children}
    </h1>
  );
}

/** Error, core down, paused: shown at the top of every page. */
export function Banners({ snap, togglePause, refresh }: Pick<Ctx, "snap" | "togglePause" | "refresh">) {
  const s = snap.status;
  return (
    <>
      {s.error && (
        <div className="banner bad" role="alert">
          <div className="grow">
            <b>AIQ can't open its data right now.</b> Close other AIQ windows, then try again. Nothing was changed.{" "}
            <span className="meta">({s.error.split(":")[0]})</span>
          </div>
          <button className="btn" onClick={refresh}>Try again</button>
        </div>
      )}
      {!s.core && !s.error && (
        <div className="banner">
          <div className="grow">
            {s.last_event_ms != null ? (
              <>AIQ isn't running, so this shows what was stored at {when(s.last_event_ms, snap.now_ms, snap.offset_min)}.</>
            ) : (
              <>AIQ isn't running.</>
            )}{" "}
            It starts with your next prompt in Claude Code or Copilot.
          </div>
        </div>
      )}
      {s.paused && (
        <div className="banner">
          <div className="grow">
            <b>AIQ is paused.</b> Nothing is captured and no tips show.
          </div>
          <button className="btn" onClick={togglePause}>
            Resume <kbd>P</kbd>
          </button>
        </div>
      )}
    </>
  );
}

/** Modal dialog: focus starts on the first marked element (Cancel), Tab is trapped, Esc closes, focus returns. */
export function Dialog({ label, onClose, children, className = "" }: { label: string; onClose: () => void; children: ReactNode; className?: string }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current!;
    const back = document.activeElement as HTMLElement | null;
    if (!d.open) d.showModal();
    (d.querySelector<HTMLElement>("[data-autofocus]") ?? d.querySelector<HTMLElement>("button,input"))?.focus();
    const onCancel = (e: Event) => {
      e.preventDefault();
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const f = [...d.querySelectorAll<HTMLElement>("button:not([disabled]),input,[tabindex='0'],[role='option']")].filter((x) => x.tabIndex >= 0);
      if (f.length === 0) return;
      const first = f[0], last = f[f.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };
    d.addEventListener("cancel", onCancel);
    d.addEventListener("keydown", onKey);
    return () => {
      d.removeEventListener("cancel", onCancel);
      d.removeEventListener("keydown", onKey);
      if (d.open) d.close();
      back?.focus?.();
    };
  }, [onClose]);
  return (
    <dialog ref={ref} aria-label={label} className={className}>
      {children}
    </dialog>
  );
}

export const Sk = ({ w = "100%", h = "14px" }: { w?: string; h?: string }) => <span className="sk" style={{ width: w, height: h }} />;

export const Chip = ({ children, tone }: { children: ReactNode; tone?: "ok" | "warn" | "tried" }) => (
  <span className={`chip${tone ? ` ${tone}` : ""}`}>{children}</span>
);
