// Tasks (spec US-H3 Tasks page). Opens on the kinds of task (UI review 2026-11-04, item 4): each kind's rung today,
// what it could be, how often, and whether it moved in the last 14 days, with Your projects below. "All tasks"
// lists tasks (the Task rule: one session, split at 30 idle minutes), each with its judged job and rung, or "Not
// judged yet" (design rung-judge); "Day" is the Day view. States other than a plain judgment carry a text label
// (Uncertain, Right rung, Capped by a blocker), never colour alone.
import { useMemo, useRef, useState, type KeyboardEvent } from "react";
import { jobName, jobOf, jobOptions, perWeekShort, plural, rungLabel, sortTasks, stateLabel, toolName, when, type Dir, type SortKey } from "../format";
import { KIND_STATE } from "../coach";
import { Banners, Chip, ProjectPicker, Rung, Title, type Ctx } from "../ui";
import { projectName, projectOf } from "../project";
import type { KindRow, ProgressRow } from "../types";
import { DayView } from "./DayView";
import { Projects } from "./Projects";

const COLS: { key: SortKey; label: string; right?: boolean }[] = [
  { key: "when", label: "Started" },
  { key: "tool", label: "Tool" },
  { key: "prompts", label: "Prompts", right: true },
];

/** `start`: open on this view (the tour's Day step). */
export function Tasks({ start, ...ctx }: Ctx & { start?: View }) {
  const { snap } = ctx;
  const [sort, setSort] = useState<{ key: SortKey; dir: Dir }>({ key: "when", dir: -1 });
  const [job, setJob] = useState("All");
  const [sel, setSel] = useState(0);
  const hasKinds = snap.coaching.kinds.length > 0;
  const [picked, setView] = useState<View | null>(start ?? null);
  // Kinds first when there are kinds; until then the list (nothing to group yet).
  const view: View = picked ?? (hasKinds ? "kinds" : "list");
  const body = useRef<HTMLTableSectionElement>(null);
  const { options, dropdown } = jobOptions(snap.tasks);
  const rows = useMemo(
    () => sortTasks(snap.tasks.filter((t) => job === "All" || jobOf(t) === job), sort.key, sort.dir),
    [snap.tasks, job, sort],
  );

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Enter" && rows[sel] && (e.target as HTMLElement).tagName === "TR") {
      e.preventDefault();
      ctx.openTask(rows[sel].task_id);
      return;
    }
    const move = e.key === "j" || e.key === "ArrowDown" ? 1 : e.key === "k" || e.key === "ArrowUp" ? -1 : 0;
    if (!move || rows.length === 0) return;
    e.preventDefault();
    const n = Math.min(rows.length - 1, Math.max(0, sel + move));
    setSel(n);
    body.current?.querySelectorAll<HTMLElement>("tr")[n]?.focus();
  };

  const flip = (key: SortKey) => setSort((s) => ({ key, dir: s.key === key ? ((-s.dir) as Dir) : key === "tool" ? 1 : -1 }));

  return (
    <div className="page wide">
      <Banners {...ctx} />
      <ProjectPicker {...ctx} />
      <header className="head">
        <div className="group">
          <Title>Tasks</Title>
          <p className="mute">
            {plural(snap.home.tasks_in_window, "task")} in the last {snap.home.window_days} days, {snap.home.judged} analyzed.
            {snap.home.judged < snap.home.tasks_in_window && " AIQ analyzes finished tasks through your own Claude Code or Copilot CLI, within a daily limit."}
          </p>
        </div>
        {snap.tasks.length > 0 && (
          <div className="row">
            <div className="seg" role="group" aria-label="View">
              {hasKinds && <button aria-pressed={view === "kinds"} onClick={() => setView("kinds")}>Kinds</button>}
              <button aria-pressed={view === "list"} onClick={() => setView("list")}>All tasks</button>
              <button aria-pressed={view === "day"} onClick={() => setView("day")}>Day</button>
            </div>
            <span className="meta" id="joblbl">Job</span>
            {dropdown ? (
              <select className="input" aria-labelledby="joblbl" value={job} onChange={(e) => setJob(e.target.value)}>
                {options.map((o) => (
                  <option key={o}>{o}</option>
                ))}
              </select>
            ) : (
              <div className="seg" role="group" aria-labelledby="joblbl">
                {options.map((o) => (
                  <button key={o} aria-pressed={job === o} onClick={() => setJob(o)}>{o}</button>
                ))}
              </div>
            )}
          </div>
        )}
      </header>
      {snap.tasks.length > 0 && view === "kinds" ? (
        <Kinds {...ctx} rows={snap.coaching.kinds.filter((k) => job === "All" || jobName(k.job) === job)} />
      ) : snap.tasks.length > 0 && view === "day" ? (
        <DayView {...ctx} snap={{ ...snap, tasks: snap.tasks.filter((t) => job === "All" || jobOf(t) === job) }} />
      ) : snap.tasks.length === 0 ? (
        <div className="empty">
          <b>No tasks yet.</b>
          <span className="mute">A task shows here after you work with Claude Code, Copilot CLI or VS Code agent mode while AIQ is on.</span>
        </div>
      ) : (
        <div className="tablewrap">
          <table aria-label="Tasks" onKeyDown={onKey}>
            <thead>
              <tr>
                {COLS.map((c) => (
                  <th key={c.key} className={c.right ? "r" : undefined} aria-sort={sort.key === c.key ? (sort.dir < 0 ? "descending" : "ascending") : "none"}>
                    <button onClick={() => flip(c.key)}>
                      {c.label}
                      {sort.key === c.key ? (sort.dir < 0 ? " ↓" : " ↑") : ""}
                    </button>
                  </th>
                ))}
                <th>Project</th>
                <th>Job and kind</th>
                <th>
                  <span className="rung" title="How much of the task you handed to the agent, from Assisted (1) to Factory (5). Hover a rung to see what it means.">Rung</span>
                </th>
              </tr>
            </thead>
            <tbody ref={body}>
              {rows.map((t, i) => (
                <tr key={t.task_id} tabIndex={i === sel ? 0 : -1} aria-selected={i === sel} onFocus={() => setSel(i)} onClick={() => ctx.openTask(t.task_id)}>
                  <td className="num">
                    <a
                      href={`#task/${encodeURIComponent(t.task_id)}`}
                      tabIndex={-1}
                      onClick={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        ctx.openTask(t.task_id);
                      }}
                    >
                      {when(t.first_ms, snap.now_ms, snap.offset_min)}
                    </a>
                  </td>
                  <td>
                    {toolName(t.tool)} {t.origin === "imported" && <Chip>Imported</Chip>}
                  </td>
                  <td className="r num">{t.prompts}</td>
                  <td title={t.folder ?? "Unknown for tasks before this version"}>{projectName(snap, projectOf(t))}</td>
                  <td className={t.job ? undefined : "mute"}>
                    {jobOf(t)}
                    {t.judgment?.task_type_label && <span className="meta"> · {t.judgment.task_type_label}</span>}
                  </td>
                  <td className={t.judgment ? undefined : "mute"}>
                    {t.judgment ? (
                      <>
                        <Rung r={t.judgment.current} />
                        {stateLabel(t.judgment.state) && <> <Chip>{stateLabel(t.judgment.state)}</Chip></>}
                      </>
                    ) : (
                      "Not analyzed yet"
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {snap.tasks.length > 0 && view === "list" && <p className="meta">J and K move between rows, Enter opens a task. Select a column heading to sort.</p>}
      {(view === "kinds" || (!hasKinds && view === "list")) && ctx.project === null && snap.projects.length > 1 && <Projects {...ctx} />}
    </div>
  );
}

type View = "kinds" | "list" | "day";

/** "↑ up from Directed (2)", from the same 14-day comparison as Home's progress (US-H5b). */
function trendText(p: ProgressRow | undefined): { text: string; tone?: "ok" | "warn" } {
  if (!p || p.trend === "too_few") return { text: "Too few tasks to say" };
  if (p.trend === "same") return { text: "– same rung" };
  const from = p.earlier_rung ? ` from ${rungLabel(p.earlier_rung)}` : "";
  return p.trend === "up" ? { text: `↑ up${from}`, tone: "ok" } : { text: `↓ down${from}`, tone: "warn" };
}

/** The kinds of task as a table: a row opens the kind (its next step and starter prompt). */
function Kinds(ctx: Ctx & { rows: KindRow[] }) {
  const { snap, rows } = ctx;
  const c = snap.coaching;
  const progress = new Map(c.progress.map((p) => [`${p.job}/${p.task_type}`, p]));
  return (
    <>
      <div className="tablewrap">
        <table aria-label="Kinds of task">
          <thead>
            <tr>
              <th>Kind of task</th>
              <th>
                <span className="rung" title="The rung most of this kind's analyzed tasks sit at.">Today</span>
              </th>
              <th>Could be</th>
              <th className="r">A week</th>
              <th className="r">Tasks</th>
              <th>
                Last {c.progress_days} days against the {c.progress_days} before
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((k) => {
              const tr = trendText(progress.get(`${k.job}/${k.task_type}`));
              const up = k.today != null && k.potential != null && k.potential > k.today;
              return (
                <tr key={`${k.job}/${k.task_type}`} onClick={() => ctx.openKind(k.job, k.task_type)}>
                  <td>
                    <a
                      href={`#kind/${encodeURIComponent(k.job)}/${encodeURIComponent(k.task_type)}`}
                      onClick={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        ctx.openKind(k.job, k.task_type);
                      }}
                    >
                      <b>{k.label}</b>
                    </a>
                    <div className="meta">{jobName(k.job)}</div>
                  </td>
                  <td>{k.today != null ? <Chip><Rung r={k.today} /></Chip> : <span className="mute">—</span>}</td>
                  <td>
                    {up ? (
                      <Chip tone="tried"><Rung r={k.potential!} /></Chip>
                    ) : (
                      <Chip tone={k.state === "right_rung" ? "ok" : undefined}>{KIND_STATE[k.state]}</Chip>
                    )}
                  </td>
                  <td className="r num">{perWeekShort(k.per_week)}</td>
                  <td className="r num">{k.tasks}</td>
                  <td>{tr.tone ? <Chip tone={tr.tone}>{tr.text}</Chip> : <span className="meta">{tr.text}</span>}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="meta">
        Most tasks first. The kind of each task and its rung are AIQ's analysis; counts are measured. Open a kind for its next step and a starter prompt.
        {c.untyped > 0 && ` ${plural(c.untyped, "task")} analyzed before kinds existed ${c.untyped === 1 ? "isn't" : "aren't"} grouped.`}
      </p>
    </>
  );
}
