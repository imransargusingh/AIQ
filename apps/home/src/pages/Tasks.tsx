// Tasks (spec US-H3 Tasks page). Lists tasks (the Task rule: one session, split at 30 idle minutes), each
// with its judged job and rung, or "Not judged yet" (design rung-judge). States other than a plain judgment
// carry a text label (Uncertain, Right rung, Capped by a blocker), never colour alone.
import { useMemo, useRef, useState, type KeyboardEvent } from "react";
import { folderName, jobOf, jobOptions, plural, rungLabel, sortTasks, stateLabel, toolName, when, type Dir, type SortKey } from "../format";
import { KindList } from "../coach";
import { Banners, Chip, Title, type Ctx } from "../ui";

const COLS: { key: SortKey; label: string; right?: boolean }[] = [
  { key: "when", label: "Started" },
  { key: "tool", label: "Tool" },
  { key: "prompts", label: "Prompts", right: true },
];

export function Tasks(ctx: Ctx) {
  const { snap } = ctx;
  const [sort, setSort] = useState<{ key: SortKey; dir: Dir }>({ key: "when", dir: -1 });
  const [job, setJob] = useState("All");
  const [sel, setSel] = useState(0);
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
      <header className="head">
        <div className="group">
          <Title>Tasks</Title>
          <p className="mute">
            {plural(snap.home.tasks_in_window, "task")} in the last {snap.home.window_days} days, {snap.home.judged} judged.
            {snap.home.judged < snap.home.tasks_in_window && " AIQ judges finished tasks through your own Claude Code or Copilot CLI, within a daily limit."}
          </p>
        </div>
        {snap.tasks.length > 0 && (
          <div className="row">
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
      {snap.coaching.kinds.length > 0 && (
        <section className="group" aria-labelledby="kinds">
          <div className="head">
            <h2 id="kinds">Kinds of task</h2>
            <span className="meta">
              The kind of each task is the judge's estimate. Most tasks first.
              {snap.coaching.untyped > 0 && ` ${plural(snap.coaching.untyped, "task")} judged before kinds existed ${snap.coaching.untyped === 1 ? "isn't" : "aren't"} grouped.`}
            </span>
          </div>
          <KindList label="Kinds of task" rows={snap.coaching.kinds} open={(k) => ctx.openKind(k.job, k.task_type)} />
        </section>
      )}
      {snap.tasks.length === 0 ? (
        <div className="empty">
          <b>No tasks yet.</b>
          <span className="mute">A task shows here after you work with Claude Code, Copilot CLI or VS Code agent mode while AIQ is watching.</span>
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
                <th>Folder</th>
                <th>Job and kind</th>
                <th>Rung</th>
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
                  <td title={t.folder ?? "Not recorded for tasks before this version"}>{folderName(t.folder)}</td>
                  <td className={t.job ? undefined : "mute"}>
                    {jobOf(t)}
                    {t.judgment?.task_type_label && <span className="meta"> · {t.judgment.task_type_label}</span>}
                  </td>
                  <td className={t.judgment ? undefined : "mute"}>
                    {t.judgment ? (
                      <>
                        {rungLabel(t.judgment.current)}
                        {stateLabel(t.judgment.state) && <> <Chip>{stateLabel(t.judgment.state)}</Chip></>}
                      </>
                    ) : (
                      "Not judged yet"
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {snap.tasks.length > 0 && <p className="meta">J and K move between rows, Enter opens a task. Select a column heading to sort.</p>}
    </div>
  );
}
