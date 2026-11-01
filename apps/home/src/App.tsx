// The shell (DESIGN.md §4–5, spec Part B IA): titlebar, left nav with key hints, status block, command
// palette (Ctrl+K), G-letter navigation, P to pause, one status region for every announced outcome.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { toolName } from "./format";
import type { Snapshot } from "./types";
import { Dialog, PAGES, Sk, type Ctx, type Page } from "./ui";
import { Home } from "./pages/Home";
import { Tasks } from "./pages/Tasks";
import { Tips } from "./pages/Tips";
import { Data } from "./pages/Data";
import { Settings } from "./pages/Settings";

const REFRESH_MS = 15_000;
const KEYS_PREF = "aiq.singleKeys";

function readPref(): boolean {
  try {
    return localStorage.getItem(KEYS_PREF) !== "off";
  } catch {
    return true;
  }
}

function hashPage(): Page {
  const h = location.hash.slice(1) as Page;
  return PAGES.some((p) => p.id === h) ? h : "home";
}

export function App() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [page, setPage] = useState<Page>(hashPage);
  const [palette, setPalette] = useState(false);
  const [said, setSaid] = useState("");
  const [singleKeys, setSingleKeysState] = useState(readPref);
  const gPending = useRef(0);

  const announce = useCallback((m: string) => {
    setSaid("");
    requestAnimationFrame(() => setSaid(m));
  }, []);
  const refresh = useCallback(() => {
    api.snapshot().then(
      (s) => {
        setSnap(s);
        setLoadErr(null);
      },
      (e) => setLoadErr(String(e?.message ?? e)),
    );
  }, []);
  const go = useCallback((p: Page) => {
    setPage(p);
    history.replaceState(null, "", `#${p}`);
  }, []);
  const togglePause = useCallback(() => {
    if (!snap) return;
    const on = !snap.status.paused;
    api.pause(on).then(
      (s) => {
        setSnap(s);
        announce(on ? "AIQ is paused. Nothing is captured and no tips show." : "AIQ resumed.");
      },
      () => announce("Could not change the pause setting."),
    );
  }, [snap, announce]);
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
    const t = setInterval(refresh, REFRESH_MS);
    window.addEventListener("focus", refresh);
    return () => {
      clearInterval(t);
      window.removeEventListener("focus", refresh);
    };
  }, [refresh]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
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
  }, [singleKeys, palette, go, togglePause]);

  const ctx: Ctx | null = snap && { snap, go, announce, togglePause, refresh };
  const watching = snap?.home.by_tool.map(([t]) => toolName(t)) ?? [];

  return (
    <div className="app">
      <div className="titlebar">
        <b style={{ color: "var(--ink)" }}>AIQ</b>
        {snap?.status.paused && <span className="chip pausedchip">Paused</span>}
        <button className="search" onClick={() => setPalette(true)} aria-label="Command palette (Ctrl+K)">
          Search or jump to… <kbd>Ctrl K</kbd>
        </button>
      </div>
      <div className="shell">
        <nav className="side" aria-label="AIQ">
          {PAGES.map((p) => (
            <a
              key={p.id}
              href={`#${p.id}`}
              aria-current={page === p.id ? "page" : undefined}
              onClick={(e) => {
                e.preventDefault();
                go(p.id);
              }}
            >
              <span>{p.label}</span>
              {singleKeys && <span className="k" aria-hidden="true">G {p.key}</span>}
            </a>
          ))}
          <div className="foot">
            {snap && (
              <>
                <span>
                  <span className={`dot${snap.status.paused || !snap.status.core ? " paused" : ""}`} aria-hidden="true" />
                  {snap.status.paused ? "Paused" : !snap.status.core ? "Not running" : watching.length ? `Watching ${watching.join(", ")}` : "Watching quietly"}
                </span>
                <span>
                  Tips today: {snap.home.tips_today} of {snap.settings.daily_cap}
                </span>
                {singleKeys && <span>P to {snap.status.paused ? "resume" : "pause"}</span>}
              </>
            )}
          </div>
        </nav>
        <main>
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
          {ctx && page === "home" && <Home key="home" {...ctx} />}
          {ctx && page === "tasks" && <Tasks key="tasks" {...ctx} />}
          {ctx && page === "tips" && <Tips key="tips" {...ctx} />}
          {ctx && page === "data" && <Data key="data" {...ctx} />}
          {ctx && page === "settings" && <Settings key="settings" {...ctx} singleKeys={singleKeys} setSingleKeys={setSingleKeys} />}
        </main>
      </div>
      <p className="sr" role="status" aria-live="polite">{said}</p>
      {palette && ctx && <Palette ctx={ctx} onClose={() => setPalette(false)} />}
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
      { label: "Refresh", run: ctx.refresh },
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
