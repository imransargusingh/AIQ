// Small shared pieces: banners, dialog with focus trap, skeletons, chips.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api } from "./api";
import type { Snapshot } from "./types";
import { RUNGS, rungLabel, plural, when } from "./format";
import { failedLine, ver, type UpdatePhase } from "./updateNow";
import type { Theme } from "./theme";

/** Nav pages, plus places outside the nav: one task's detail, one kind of task (from Home or Tasks), setup. */
export type Page = "home" | "tasks" | "milestones" | "guide" | "tips" | "data" | "settings" | "task" | "kind" | "setup";

export const PAGES: { id: Exclude<Page, "task" | "kind" | "setup">; label: string; key: string }[] = [
  { id: "home", label: "Home", key: "H" },
  { id: "tasks", label: "Tasks", key: "T" },
  { id: "milestones", label: "Milestones", key: "M" },
  { id: "guide", label: "Rung guide", key: "R" },
  { id: "tips", label: "Habits and tips", key: "B" },
  { id: "data", label: "Your data", key: "D" },
  { id: "settings", label: "Settings", key: "S" },
];

export interface Ctx {
  snap: Snapshot;
  go: (p: Page) => void;
  /** Open one task's detail page. */
  openTask: (taskId: string) => void;
  /** Open one kind of task's page (its next step and "This doesn't fit"). */
  openKind: (job: string, taskType: string) => void;
  announce: (msg: string) => void;
  togglePause: () => void;
  refresh: () => void;
  /** The project Home and Tasks show (a folder key, "" for no folder), or null for all (design project-filter P3). */
  project: string | null;
  setProject: (key: string | null) => void;
  /** "Check for updates" (design auto-update U5h): what it is doing, its last result, and the press itself. */
  update: { phase: UpdatePhase; msg: string | null; run: () => void };
  /** Start the tour (design home-tour): Ctrl K, Settings and the getting-started list. */
  startTour: () => void;
  /** The theme picked in Settings or Ctrl K (theme.ts); "system" follows Windows. */
  theme: Theme;
  setTheme: (t: Theme) => void;
}

/** The picker's "All projects" value: a NUL can't be in any folder path, so no project key equals it. */
const ALL = "\u0000";

/** The Project picker at the top of Home and Tasks (design project-filter P3). Hidden with one project or none. */
export function ProjectPicker({ snap, project, setProject }: Pick<Ctx, "snap" | "project" | "setProject">) {
  if (snap.projects.length < 2) return null;
  const shown = project !== null && snap.projects.some((p) => p.key === project) ? project : null;
  return (
    <div className="row projpick">
      <label className="meta" htmlFor="projsel">Project</label>
      <select
        id="projsel"
        className="input"
        value={shown === null ? ALL : shown}
        onChange={(e) => setProject(e.target.value === ALL ? null : e.target.value)}
      >
        <option value={ALL}>All projects</option>
        {snap.projects.map((p) => (
          <option key={p.key} value={p.key}>
            {p.name} ({p.tasks_in_window})
          </option>
        ))}
      </select>
      {shown !== null && (
        <button className="btnlink" onClick={() => setProject(null)}>Show all projects</button>
      )}
    </div>
  );
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

const TOOL_NAME = { "copilot-cli": "Copilot CLI", "claude-code": "Claude Code" } as const;
const SIGN_IN_NAME = { "copilot-cli": "Copilot", "claude-code": "Claude Code" } as const;

/** "14:05 today", "09:12 yesterday", "Sat 31 Oct 18:40". */
function sinceWords(ms: number, nowMs: number, offsetMin: number): string {
  return when(ms, nowMs, offsetMin).replace(/^(Today|Yesterday) (.*)$/, (_, d: string, t: string) => `${t} ${d.toLowerCase()}`);
}

/** Analysis can't run because the tool it goes through is signed out or missing: on every page, until it runs again. */
function AnalysisBlocked({ snap, announce }: Pick<Ctx, "snap" | "announce">) {
  const a = snap.home.analysis;
  const b = a.blocked;
  if (a.state !== "blocked" || !b) return null;
  const out = b.why === "not_signed_in";
  const act = async () => {
    const r = out ? await api.openSignIn(b.tool) : await api.openHelp(b.tool);
    announce(r.ok ? (out ? `Opened ${SIGN_IN_NAME[b.tool]}. Type /login there; AIQ picks up by itself.` : "Opened the install page.") : r.message);
  };
  return (
    <div className="banner" role="alert" data-testid="analysis-blocked">
      <div className="grow">
        <b>{out ? `${TOOL_NAME[b.tool]} is signed out, so AIQ can't analyze your tasks.` : `AIQ can't find ${TOOL_NAME[b.tool]}, so it can't analyze your tasks.`}</b>{" "}
        Since {sinceWords(b.since_ms, snap.now_ms, snap.offset_min)}.{" "}
        {plural(a.waiting, "task")} wait.{" "}
        {out
          ? "A window that is already open can keep working; AIQ needs a fresh sign-in. Sign in once and AIQ catches up by itself."
          : "Install it and AIQ catches up by itself."}
      </div>
      <button className="btn pri" onClick={act}>{out ? `Sign in to ${SIGN_IN_NAME[b.tool]}` : `Get ${TOOL_NAME[b.tool]}`}</button>
    </div>
  );
}

/** Analysis stuck, error, core down, paused: shown at the top of every page. */
export function Banners({ snap, togglePause, refresh, announce }: Pick<Ctx, "snap" | "togglePause" | "refresh" | "announce">) {
  const s = snap.status;
  return (
    <>
      {!s.paused && <AnalysisBlocked snap={snap} announce={announce} />}
      {s.error && (
        <div className="banner bad" role="alert">
          <div className="grow">
            <b>AIQ can't open its data right now.</b> Close other AIQ windows, then try again. Nothing was changed.{" "}
            <span className="meta">({s.error.split(":")[0]})</span>
          </div>
          <button className="btn" onClick={refresh}>Try again</button>
        </div>
      )}
      {s.core_state === "failed" && !s.error && (
        <div className="banner bad" role="alert">
          <div className="grow">
            <b>AIQ couldn't start.</b> It keeps trying.{s.core_error && <> <span className="meta">({s.core_error})</span></>}
          </div>
          <button className="btn" onClick={refresh}>Try again</button>
        </div>
      )}
      {s.paused && (
        <div className="banner">
          <div className="grow">
            <b>AIQ is paused.</b> It saves nothing new and shows no tips.
          </div>
          <button className="btn" onClick={togglePause}>
            Resume <kbd>P</kbd>
          </button>
        </div>
      )}
    </>
  );
}

const UPDATE_SEEN = "aiq.updateNoteSeen";
/** The note this Home session already showed: it stays until Home closes, then never shows again. */
let noteThisSession: string | null = null;

function seenBefore(key: string): boolean {
  if (noteThisSession === key) return false;
  let before = false;
  try {
    before = localStorage.getItem(UPDATE_SEEN) === key;
    localStorage.setItem(UPDATE_SEEN, key);
  } catch {
    /* No storage: the core's one-day limit still ends it. */
  }
  if (!before) noteThisSession = key;
  return before;
}

/** The titlebar's update marker, on every page (design auto-update U5h): "Update to x.y.z" while one waits,
 * else what Check for updates is doing or found. Nothing otherwise, so it takes no space in the page. */
export function UpdateChip({ snap, update }: Pick<Ctx, "snap" | "update">) {
  const u = snap.update;
  if (!u.supported) return null;
  if (update.phase === "idle" && u.note === "waiting") {
    const when = u.mode === "auto" ? " It also installs by itself when AIQ home has been in the background for 10 minutes, or when you close it." : "";
    const waited = u.blocked ? ` Last try waited: ${u.blocked}.` : "";
    return (
      <button className="chip updchip" data-testid="update-chip" onClick={update.run} title={`AIQ home closes, installs AIQ ${ver(u.available)} and opens again.${when}${waited}`}>
        Update to {ver(u.available)}
      </button>
    );
  }
  if (!update.msg) return null;
  return (
    <span className={`chip${update.phase === "idle" ? "" : " tried"}`} data-testid="update-chip">
      {update.msg}
    </span>
  );
}

/** Home's one line after an update ran (design auto-update U5, U5g): installed, failed or undone, a slim line
 * shown on one Home session, a day at most (core), with a dismiss. A waiting update is the titlebar's marker. */
export function UpdateLine({ snap, announce }: Pick<Ctx, "snap" | "announce">) {
  const u = snap.update;
  const [msg, setMsg] = useState<string | null>(null);
  const key = `${u.note}:${u.at_ms ?? ""}`;
  const [hidden, setHidden] = useState(() => u.note !== "none" && u.note !== "waiting" && seenBefore(key));
  if (!u.supported || u.note === "none" || u.note === "waiting" || hidden) return null;
  const run = async () => {
    const r = await api.runUpdate("rollback");
    const m = r.ok ? "Going back. AIQ home closes and opens again." : r.message;
    setMsg(m);
    announce(m);
  };
  let text: ReactNode;
  let action: ReactNode = null;
  if (u.note === "applied") {
    text = <>Updated to AIQ {ver(u.running)}.</>;
    if (u.can_rollback && u.from)
      action = (
        <button className="btnlink" onClick={run}>
          Go back to {ver(u.from)}
        </button>
      );
  } else if (u.note === "failed") {
    text = <>{failedLine(u)}</>;
  } else {
    text = <>Back on AIQ {ver(u.running)}. {u.from ? ver(u.from) : "That version"} won't install again by itself.</>;
  }
  const dismiss = () => {
    setHidden(true);
    announce("Update note dismissed.");
  };
  return (
    <p className="updnote meta" data-testid="update-line">
      <span>
        {text}
        {msg && <> {msg}</>}
      </span>
      {action}
      <button className="btnlink x" aria-label="Dismiss update note" title="Dismiss" onClick={dismiss}>
        ×
      </button>
    </p>
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

/** A rung's name; hovering it says what the rung means (the full ladder is on Home, How you work with agents). */
export const Rung = ({ r }: { r: number }) => (
  <span className="rung" title={RUNGS[r - 1].means}>
    {rungLabel(r)}
  </span>
);

/** Prompts can be many KB; show a preview and keep the full text one click (or Enter) away. */
const PREVIEW = 280;
export function Content({ text }: { text: string }) {
  if (text.length <= PREVIEW) return <>{text}</>;
  return (
    <details>
      <summary>
        {text.slice(0, PREVIEW).trimEnd()}… <span className="mute">({text.length.toLocaleString("en-GB")} characters, show all)</span>
      </summary>
      <span className="rest">{text.slice(PREVIEW)}</span>
    </details>
  );
}

/** A one-line name for a prompt: its first words. */
export function preview(text: string, max = 80): string {
  const t = text.replace(/\s+/g, " ").trim();
  return t.length <= max ? t : `${t.slice(0, max).trimEnd()}…`;
}

/** Next-step tips the accuracy check paused (design K6b): why, in measured counts, and one-click overrides. */
export function AccuracyPaused({ snap, announce, refresh }: Pick<Ctx, "snap" | "announce" | "refresh">) {
  const ps = snap.settings.next_rung_tips === "auto" ? snap.settings.accuracy_paused : [];
  if (ps.length === 0) return null;
  const down = !snap.status.core;
  const change = async (key: `keep_on.${string}`, value: string, done: string) => {
    const r = await api.setSetting(key, value);
    announce(r.ok ? done : r.message);
    if (r.ok) refresh();
  };
  return (
    <ul className="group" aria-label="Tips paused by the accuracy check" style={{ listStyle: "none", margin: 0, padding: 0 }}>
      {ps.map((p) => (
        <li key={p.scope} className="banner">
          <div className="grow">
            {p.kept_on ? (
              <>
                <b>{p.label}:</b> next-step tips stay on because you chose to keep them, though a second model agreed with AIQ's analysis only {p.agreed} of {p.checked} times.
              </>
            ) : (
              <>
                <b>{p.label}:</b> AIQ paused these next-step tips. A second model agreed with AIQ's analysis only {p.agreed} of {p.checked} times, under 8 in 10. They come back by
                themselves when a later accuracy check clears the bar.
              </>
            )}
          </div>
          {p.kept_on ? (
            <button className="btn" disabled={down} onClick={() => change(`keep_on.${p.scope}`, "auto", `${p.label}: AIQ decides again.`)}>
              Let AIQ decide
            </button>
          ) : (
            <button className="btn" disabled={down} onClick={() => change(`keep_on.${p.scope}`, "on", `${p.label}: next-step tips kept on.`)}>
              Keep showing
            </button>
          )}
        </li>
      ))}
    </ul>
  );
}
