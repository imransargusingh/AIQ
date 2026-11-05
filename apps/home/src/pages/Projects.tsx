// "Your projects" (design project-filter P5b), on the Tasks page under the kinds of task: one row per project,
// most room to grow first. Selecting a name shows that project; Rename keeps a name the person chose.
import { useState } from "react";
import { api } from "../api";
import { pct } from "../coach";
import { folderName, toolName } from "../format";
import { Rung, type Ctx } from "../ui";
import type { Project } from "../types";

/** "Your projects" (design project-filter P5b): one row per project, most room to grow first. Selecting a name
 * shows that project; Rename keeps a name the person chose (empty goes back to the folder's name). */
export function Projects(ctx: Ctx) {
  const { snap, setProject, announce, refresh } = ctx;
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const min = snap.coaching.split_min;
  const save = async (p: Project) => {
    const r = await api.setSetting(`project_name.${p.key}`, draft);
    announce(r.ok ? (draft.trim() ? `Renamed to ${draft.trim()}.` : "AIQ names this project from its folder again.") : r.message);
    if (r.ok) {
      setEditing(null);
      refresh();
    }
  };
  return (
    <section className="group" aria-labelledby="pj">
      <div className="head">
        <h2 id="pj">Your projects</h2>
        <span className="meta">
          A project is the folder you worked in. Most tasks with an easy next step first. Counts are measured; rungs are AIQ's analysis.
        </span>
      </div>
      <div className="tablewrap">
        <table aria-label="Your projects">
          <thead>
            <tr>
              <th>Project</th>
              <th className="r">Tasks</th>
              <th>Most common rung</th>
              <th className="r">Agents finished</th>
              <th className="r">Easy next step</th>
              <th><span className="sr">Rename</span></th>
            </tr>
          </thead>
          <tbody>
            {snap.projects.map((p) => {
              const sp = p.coaching.split;
              const enough = sp.tasks >= min;
              return (
                <tr key={p.key}>
                  <td title={p.key || "Tasks from before AIQ saved the folder, or from tools that don't send it"}>
                    {editing === p.key ? (
                      <form
                        className="row"
                        onSubmit={(e) => {
                          e.preventDefault();
                          save(p);
                        }}
                      >
                        <input
                          className="input"
                          aria-label={`New name for ${p.name}`}
                          maxLength={60}
                          value={draft}
                          placeholder={folderName(p.key)}
                          autoFocus
                          onChange={(e) => setDraft(e.target.value)}
                          onKeyDown={(e) => e.key === "Escape" && setEditing(null)}
                        />
                        <button className="btn pri" type="submit" disabled={!snap.status.core}>Save</button>
                        <button className="btn" type="button" onClick={() => setEditing(null)}>Cancel</button>
                      </form>
                    ) : (
                      <button
                        className="btnlink"
                        aria-label={`Show ${p.name} only`}
                        onClick={() => {
                          setProject(p.key);
                          // Back to the top, on the picker, so the person sees the whole page change.
                          requestAnimationFrame(() => document.getElementById("projsel")?.focus());
                        }}
                      >
                        {p.name}
                      </button>
                    )}{" "}
                    {p.by_tool.map(([t]) => (
                      <span className="chip" key={t}>{toolName(t)}</span>
                    ))}
                  </td>
                  <td className="r num">{p.tasks_in_window}</td>
                  <td className={p.most_common_rung ? undefined : "mute"}>{p.most_common_rung ? <Rung r={p.most_common_rung} /> : "Not analyzed yet"}</td>
                  <td className={`r num${enough ? "" : " mute"}`}>{enough ? `${pct(sp.finished, sp.tasks)}%` : p.judged > 0 ? "Too few analyzed" : "—"}</td>
                  <td className={`r num${enough ? "" : " mute"}`}>
                    {enough ? `${sp.up} (${pct(sp.up, sp.tasks)}%)` : p.judged > 0 ? "Too few analyzed" : "—"}
                  </td>
                  <td>
                    {p.key !== "" && editing !== p.key && (
                      <button
                        className="btnlink"
                        disabled={!snap.status.core}
                        aria-label={`Rename ${p.name}`}
                        onClick={() => {
                          setEditing(p.key);
                          setDraft(p.renamed ? p.name : "");
                        }}
                      >
                        Rename
                      </button>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="meta">
        "Easy next step" counts analyzed tasks not finished by agents where AIQ's analysis sees a next step at least one rung up. A project needs {min} analyzed tasks before AIQ shows its numbers.
      </p>
    </section>
  );
}
