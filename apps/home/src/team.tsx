// The team's "Tool to try" line (team tips file, aiq.team-tips.v1). Home is AIQ's own window, so it shows the
// line for every tip; Claude Code never does (that rule lives in the terminal renderers).
import type { Snapshot, TeamTool } from "./types";

const NEXT = "next_rung.";

/** The team's tool for a habit, by move id or by a next-step tip's id; undefined for other tips. */
export function teamTool(snap: Pick<Snapshot, "team_tips">, id: string): TeamTool | undefined {
  const moveId = id.startsWith(NEXT) ? id.slice(NEXT.length) : id;
  return snap.team_tips?.tools.find((t) => t.move_id === moveId);
}

export function TeamToolLine({ t }: { t: TeamTool | undefined }) {
  if (!t) return null;
  return (
    <div className="team-tool">
      <b>Tool to try:</b> {t.tool}: {t.how}
      {t.link && <div className="meta">Guide: {t.link}</div>}
      <div className="meta">From your team tips file</div>
    </div>
  );
}
