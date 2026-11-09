// The team's "Tool to try" line (team tips file, aiq.team-tips.v1). Home is AIQ's own window, so it shows the
// line for every tip; Claude Code never does (that rule lives in the terminal renderers).
import type { Snapshot, TeamTips, TeamTool } from "./types";

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

/** Settings > Team tips: where the lines come from, in words, and the one action that fits. Never the repo address. */
export function teamTipsText(t: TeamTips): { line: string; action?: { label: string; does: "off" | "auto" | "sign_in" } } {
  const n = t.tools.length;
  const habits = `${n} ${n === 1 ? "habit has" : "habits have"} a Tool to try line`;
  switch (t.state) {
    case "download":
      return { line: `${habits}. AIQ gets them from your team's repo every hour.`, action: { label: "Stop using", does: "off" } };
    case "found":
      return { line: `${habits}, from your team's folder on this computer.`, action: { label: "Stop using", does: "off" } };
    case "chosen":
      return { line: `${habits}, from the folder you chose.`, action: { label: "Stop using", does: "off" } };
    case "off":
      return { line: "Off. Tips show without your team's tools.", action: { label: "Use again", does: "auto" } };
    case "sign_in":
      return { line: "Sign in to your team's GitHub once, and tips can name the tools your team uses.", action: { label: "Sign in", does: "sign_in" } };
    case "no_git":
      return { line: "Install Git for Windows, and tips can name the tools your team uses." };
    default:
      return { line: "No team tool suggestions." };
  }
}
