// The shell (DESIGN.md §4–5, spec Part B IA): titlebar, left nav with key hints, status block, command
// palette (Ctrl+K), G-letter navigation, P to pause, one status region for every announced outcome.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { singleFlight } from "./refresh";
import type { Snapshot } from "./types";
import { scoped } from "./project";
import { Dialog, PAGES, Sk, UpdateChip, type Ctx, type Page } from "./ui";
import { Home } from "./pages/Home";
import { Tasks } from "./pages/Tasks";
import { Tips } from "./pages/Tips";
import { Data } from "./pages/Data";
import { Settings } from "./pages/Settings";
import { Task } from "./pages/Task";
import { Setup } from "./pages/Setup";
import { Kind } from "./pages/Kind";
import { STEPS, Tour, TourOffer, exampleSnapshot } from "./tour";
import { CHECK_POLL_MS, CHECK_WAIT_MS, checkOutcome, outcomeText, ver, type UpdatePhase } from "./updateNow";

const REFRESH_MS = 15_000;
/** While the core starts (Home started it, or it is opening its store), look again soon. */
const STARTING_MS = 2_000;
const UPDATE_ASK_MS = 3_600_000;
const KEYS_PREF = "aiq.singleKeys";

function readPref(): boolean {
  try {
    return localStorage.getItem(KEYS_PREF) !== "off";
  } catch {
    return true;
  }
}

interface Route {
  page: Page;
  /** The task id (page task), or "job/type" (page kind). */
  task: string | null;
}

function hashRoute(): Route {
  const h = location.hash.slice(1);
  if (h.startsWith("task/")) return { page: "task", task: decodeURIComponent(h.slice(5)) };
  if (h.startsWith("kind/")) return { page: "kind", task: h.slice(5).split("/").map(decodeURIComponent).join("/") };
  if (h === "setup") return { page: "setup", task: null };
  return { page: PAGES.some((p) => p.id === h) ? (h as Page) : "home", task: null };
}

export function App() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [route, setRoute] = useState<Route>(hashRoute);
  const page = route.page;
  const [palette, setPalette] = useState(false);
  const [said, setSaid] = useState("");
  const [singleKeys, setSingleKeysState] = useState(readPref);
  // The project Home, Tasks and a kind's page show; all until the person picks one (design project-filter P3).
  const [project, setProjectState] = useState<string | null>(null);
  const gPending = useRef(0);
  // "Update now" (U5h): one press checks, downloads and installs; its state shows in the titlebar and beside it.
  const [upd, setUpd] = useState<{ phase: UpdatePhase; since: number; msg: string | null }>({ phase: "idle", since: 0, msg: null });
  // The tour (design home-tour): its step and the example snapshot the views show meanwhile; null when off.
  const [tour, setTour] = useState<{ step: number; snap: Snapshot } | null>(null);

  const announce = useCallback((m: string) => {
    setSaid("");
    requestAnimationFrame(() => setSaid(m));
  }, []);
  const refresh = useMemo(
    () =>
      singleFlight(() =>
        api.snapshot().then(
          (s) => {
            setSnap(s);
            setLoadErr(null);
          },
          (e) => setLoadErr(String(e?.message ?? e)),
        ),
      ),
    [],
  );
  const go = useCallback((p: Page) => {
    setRoute({ page: p, task: null });
    history.replaceState(null, "", `#${p}`);
  }, []);
  const openTask = useCallback((id: string) => {
    setRoute({ page: "task", task: id });
    history.replaceState(null, "", `#task/${encodeURIComponent(id)}`);
  }, []);
  const openKind = useCallback((job: string, type: string) => {
    setRoute({ page: "kind", task: `${job}/${type}` });
    history.replaceState(null, "", `#kind/${encodeURIComponent(job)}/${encodeURIComponent(type)}`);
  }, []);
  const togglePause = useCallback(() => {
    if (!snap) return;
    const on = !snap.status.paused;
    api.pause(on).then(
      (s) => {
        setSnap(s);
        announce(on ? "AIQ is paused. It saves nothing new and shows no tips." : "AIQ resumed.");
      },
      () => announce("Could not change the pause setting."),
    );
  }, [snap, announce]);
  const markSeen = useCallback(() => {
    api.tourSeen().then(setSnap, () => undefined);
  }, []);
  const startTour = useCallback(() => {
    if (!snap) return;
    if (!snap.setup.tour_seen) markSeen();
    exampleSnapshot(snap).then(
      (ex) => {
        setTour({ step: 0, snap: ex });
        setRoute({ page: STEPS[0].page, task: null });
        announce("Tour started, on example data. Esc closes it.");
      },
      () => announce("The tour couldn't load."),
    );
  }, [snap, markSeen, announce]);
  const tourStep = useCallback((n: number) => {
    setTour((t) => t && { ...t, step: n });
    setRoute({ page: STEPS[n].page, task: null });
  }, []);
  const endTour = useCallback(() => {
    setTour(null);
    go("home");
    announce("Tour closed. Your own data is back.");
    requestAnimationFrame(() => document.getElementById("main")?.focus());
  }, [go, announce]);
  const say = useCallback(
    (phase: UpdatePhase, msg: string | null, since = 0) => {
      setUpd({ phase, since, msg });
      if (msg) announce(msg);
    },
    [announce],
  );
  const install = useCallback(
    async (available: string | null) => {
      say("installing", `Installing AIQ ${ver(available)}. AIQ home closes and opens again.`);
      const r = await api.runUpdate("install");
      if (!r.ok) say("idle", r.message);
    },
    [say],
  );
  const updateNow = useCallback(async () => {
    if (!snap || !snap.update.supported || upd.phase !== "idle") return;
    if (snap.update.note === "waiting") return install(snap.update.available);
    const since = Date.now();
    say("checking", "Checking for a newer AIQ…", since);
    const r = await api.runUpdate("check");
    if (!r.ok) say("idle", r.message);
    else refresh();
  }, [snap, upd.phase, install, say, refresh]);
  // While a check runs, read the snapshot every 2 s; install the moment the download is checked.
  useEffect(() => {
    if (upd.phase !== "checking") return;
    const t = setInterval(refresh, CHECK_POLL_MS);
    return () => clearInterval(t);
  }, [upd.phase, refresh]);
  useEffect(() => {
    if (upd.phase !== "checking" || !snap) return;
    const o = checkOutcome(snap.update, upd.since);
    if (o === "found") void install(snap.update.available);
    else if (o !== "pending") say("idle", outcomeText(o, snap.update));
    else if (Date.now() - upd.since > CHECK_WAIT_MS) say("idle", "Still checking. Update to the new version shows here once it's downloaded.");
  }, [snap, upd.phase, upd.since, install, say]);
  // A result line clears itself after 20 s.
  useEffect(() => {
    if (upd.phase !== "idle" || !upd.msg) return;
    const t = setTimeout(() => setUpd((u) => (u.phase === "idle" ? { ...u, msg: null } : u)), 20_000);
    return () => clearTimeout(t);
  }, [upd]);

  const setSingleKeys = (on: boolean) => {
    setSingleKeysState(on);
    try {
      localStorage.setItem(KEYS_PREF, on ? "on" : "off");
    } catch {
      /* private storage off: the choice lasts this session */
    }
    announce(on ? "Single-key shortcuts on." : "Single-key shortcuts off.");
  };

  useEffect(() => {
    refresh();
    // No reads while the window is minimised or hidden; catch up as soon as it shows again.
    const tick = () => document.visibilityState === "visible" && refresh();
    const t = setInterval(tick, REFRESH_MS);
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", tick);
    return () => {
      clearInterval(t);
      window.removeEventListener("focus", refresh);
      document.removeEventListener("visibilitychange", tick);
    };
  }, [refresh]);
  const coreState = snap && !snap.status.core ? snap.status.core_state : null;
  useEffect(() => {
    if (coreState !== "starting" && coreState !== "busy") return;
    const t = setTimeout(refresh, STARTING_MS);
    return () => clearTimeout(t);
  }, [coreState, snap, refresh]);

  // A newer AIQ shows on Home within the hour (design auto-update U5c): on open, hourly, and when the window
  // gets focus, ask the shell to check. It checks only when one is due (an hour since the last, never with
  // updates off), so this is cheap; the next snapshot read shows the line once the download is checked.
  useEffect(() => {
    const ask = () => void api.runUpdate("check_if_due").catch(() => undefined);
    // On open the bar is 10 minutes since the last check, not 55 (update::HOME_OPEN_MS).
    void api.runUpdate("check_on_open").catch(() => undefined);
    const t = setInterval(ask, UPDATE_ASK_MS);
    window.addEventListener("focus", ask);
    return () => {
      clearInterval(t);
      window.removeEventListener("focus", ask);
    };
  }, []);

  // First run: setup until the person finishes it once; re-run from Settings (#setup).
  const inSetup = !!snap && (!snap.setup.done || page === "setup");

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (inSetup || tour) return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette((p) => !p);
        return;
      }
      if (!singleKeys || palette || e.ctrlKey || e.metaKey || e.altKey) return;
      const t = e.target as HTMLElement;
      if (t.closest("input,textarea,select,[contenteditable='true'],dialog")) return;
      const k = e.key.toLowerCase();
      if (gPending.current && Date.now() - gPending.current < 1500) {
        gPending.current = 0;
        const p = PAGES.find((x) => x.key.toLowerCase() === k);
        if (p) {
          e.preventDefault();
          go(p.id);
        }
        return;
      }
      if (k === "g") {
        gPending.current = Date.now();
      } else if (k === "p") {
        e.preventDefault();
        togglePause();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [singleKeys, palette, go, togglePause, inSetup, tour]);

  const setProject = useCallback(
    (key: string | null) => {
      setProjectState(key);
      const name = key === null ? null : snap?.projects.find((p) => p.key === key)?.name;
      announce(name ? `Showing ${name}.` : "Showing all projects.");
    },
    [snap, announce],
  );
  const update = useMemo(() => ({ phase: upd.phase, msg: upd.msg, run: updateNow }), [upd.phase, upd.msg, updateNow]);
  const ctx: Ctx | null = snap && { snap: tour?.snap ?? snap, go, openTask, openKind, announce, togglePause, refresh, project, setProject, update, startTour };
  // The tour shows every project of the example; the person's project choice is back after it.
  const one: Ctx | null = ctx && { ...ctx, snap: tour ? ctx.snap : scoped(ctx.snap, project), project: tour ? null : project };
  const step = tour ? STEPS[tour.step] : null;
  // The tools AIQ is connected to and that are installed, not the tools it holds tasks from (an imported or
  // disconnected tool doesn't count).
  const connected = snap?.setup.tools.filter((t) => t.hooked && t.present).map((t) => (t.tool === "copilot" ? "Copilot" : "Claude Code")) ?? [];

  if (ctx && inSetup) {
    return (
      <div className="app">
        <div className="titlebar">
          <b style={{ color: "var(--ink)" }}>AIQ</b>
        </div>
        <main id="main" tabIndex={-1}>
          <Setup
            {...ctx}
            rerun={snap!.setup.done}
            finish={(s) => {
              setSnap(s);
              go("home");
              announce("Setup done. AIQ is on.");
            }}
            close={() => go("settings")}
          />
        </main>
        <p className="sr" role="status" aria-live="polite">{said}</p>
      </div>
    );
  }

  return (
    <div className="app">
      <a
        className="skip"
        href="#main"
        onClick={(e) => {
          e.preventDefault();
          document.getElementById("main")?.focus();
        }}
      >
        Skip to content
      </a>
      <div className="titlebar">
        <b style={{ color: "var(--ink)" }}>AIQ</b>
        {tour && <span className="chip examplechip">Example data</span>}
        {snap?.status.paused && <span className="chip pausedchip">Paused</span>}
        {ctx && <UpdateChip {...ctx} />}
        <button className="search" onClick={() => setPalette(true)} aria-label="Command palette (Ctrl+K)">
          Search or jump to… <kbd>Ctrl K</kbd>
        </button>
      </div>
      {/* inert while the tour is open: the page under it can't be clicked, focused or read out. */}
      <div className="shell" inert={!!tour}>
        <nav className="side" aria-label="AIQ">
          {PAGES.map((p) => (
            <a
              key={p.id}
              href={`#${p.id}`}
              aria-current={page === p.id || ((page === "task" || page === "kind") && p.id === "tasks") ? "page" : undefined}
              aria-keyshortcuts={singleKeys ? `G ${p.key}` : undefined}
              onClick={(e) => {
                e.preventDefault();
                go(p.id);
              }}
            >
              <span>{p.label}</span>
            </a>
          ))}
          <div className="foot">
            {snap && (
              <>
                <span>
                  <span className={`dot${snap.status.paused || !snap.status.core ? " paused" : ""}`} aria-hidden="true" />
                  {snap.status.paused ? "Paused" : snap.status.core_state === "failed" ? "Not running" : !snap.status.core ? "Starting" : connected.length ? `Connected: ${connected.join(", ")}` : "No tool connected"}
                </span>
                <span>
                  Tips today: {snap.home.tips_today} of {snap.settings.daily_cap}
                </span>
                {singleKeys && <span>P to {snap.status.paused ? "resume" : "pause"}</span>}
                <span>
                  <span data-testid="app-version">AIQ {snap.update.running}</span>
                  {snap.update.supported && (
                    <>
                      {" · "}
                      <button className="btnlink" onClick={update.run} disabled={update.phase !== "idle"} title="Check for a newer AIQ and install it">
                        Update now
                      </button>
                    </>
                  )}
                </span>
              </>
            )}
          </div>
        </nav>
        <main id="main" tabIndex={-1}>
          {loadErr && !snap && (
            <div className="page">
              <div className="banner bad" role="alert">
                <div className="grow">
                  <b>AIQ home can't read its data.</b> {loadErr}
                </div>
                <button className="btn" onClick={refresh}>Try again</button>
              </div>
            </div>
          )}
          {!snap && !loadErr && (
            <div className="page" aria-busy="true">
              <Sk w="200px" h="12px" />
              <div className="top">
                <div className="card">
                  <Sk w="60%" />
                  <Sk w="140px" h="44px" />
                </div>
                <div className="card">
                  <Sk w="40%" />
                  <Sk w="90%" />
                </div>
              </div>
            </div>
          )}
          {one && page === "home" && <Home key="home" {...one} />}
          {one && page === "tasks" && <Tasks key={`tasks-${step?.view ?? ""}`} {...one} start={step?.view} />}
          {ctx && page === "tips" && <Tips key="tips" {...ctx} />}
          {ctx && page === "data" && <Data key="data" {...ctx} />}
          {ctx && page === "settings" && <Settings key="settings" {...ctx} singleKeys={singleKeys} setSingleKeys={setSingleKeys} />}
          {ctx && page === "task" && route.task && <Task key={`task-${route.task}`} {...ctx} taskId={route.task} />}
          {one && page === "kind" && route.task && <Kind key={`kind-${route.task}`} {...one} kindId={route.task} />}
        </main>
      </div>
      <p className="sr" role="status" aria-live="polite">{said}</p>
      {palette && ctx && <Palette ctx={ctx} onClose={() => setPalette(false)} />}
      {tour && <Tour step={tour.step} setStep={tourStep} end={endTour} />}
      {snap && !tour && page === "home" && snap.setup.done && !snap.setup.tour_seen && <TourOffer start={startTour} decline={markSeen} />}
    </div>
  );
}

interface Cmd {
  label: string;
  hint?: string;
  run: () => void;
}

function Palette({ ctx, onClose }: { ctx: Ctx; onClose: () => void }) {
  const [q, setQ] = useState("");
  const [i, setI] = useState(0);
  const cmds: Cmd[] = useMemo(
    () => [
      ...PAGES.map((p) => ({ label: `Go to ${p.label}`, hint: `G ${p.key}`, run: () => ctx.go(p.id) })),
      { label: ctx.snap.status.paused ? "Resume AIQ" : "Pause AIQ", hint: "P", run: ctx.togglePause },
      { label: "Delete everything…", run: () => ctx.go("data") },
      { label: "Run setup again", run: () => ctx.go("setup") },
      { label: "Take the tour", run: ctx.startTour },
      { label: "Refresh", run: ctx.refresh },
      ...(ctx.snap.update.supported ? [{ label: "Update AIQ now", run: ctx.update.run }] : []),
    ],
    [ctx],
  );
  const items = cmds.filter((c) => c.label.toLowerCase().includes(q.trim().toLowerCase()));
  const pick = (c: Cmd | undefined) => {
    if (!c) return;
    onClose();
    c.run();
  };
  return (
    <Dialog label="Command palette" onClose={onClose} className="pal-dlg">
      <div className="pal">
        <input
          data-autofocus
          role="combobox"
          aria-expanded="true"
          aria-controls="palList"
          aria-activedescendant={items.length ? `opt${Math.min(i, items.length - 1)}` : undefined}
          aria-label="Type a page or action"
          placeholder="Type a page or action"
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setI(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setI((x) => Math.min(items.length - 1, x + 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setI((x) => Math.max(0, x - 1));
            } else if (e.key === "Enter") {
              e.preventDefault();
              pick(items[Math.min(i, items.length - 1)]);
            }
          }}
        />
        <ul id="palList" role="listbox" aria-label="Commands">
          {items.length === 0 && <li className="mute" role="presentation">No command matches.</li>}
          {items.map((c, n) => (
            <li
              key={c.label}
              id={`opt${n}`}
              role="option"
              aria-selected={n === i}
              tabIndex={-1}
              className={n === i ? "act" : undefined}
              onMouseEnter={() => setI(n)}
              onClick={() => pick(c)}
            >
              <span>{c.label}</span>
              {c.hint && <kbd>{c.hint}</kbd>}
            </li>
          ))}
        </ul>
      </div>
    </Dialog>
  );
}
