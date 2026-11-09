import { describe, expect, it } from "vitest";
import { teamTipsText, teamTool } from "./team";

const snap = { team_tips: { set: true, state: "download" as const, repo: true, problem: null, tools: [{ move_id: "check_code", tool: "T", how: "h." }] } };

describe("teamTool", () => {
  it("finds a habit's tool by move id or next-step tip id, and nothing for other tips", () => {
    expect(teamTool(snap, "check_code")?.tool).toBe("T");
    expect(teamTool(snap, "next_rung.check_code")?.tool).toBe("T");
    expect(teamTool(snap, "next_rung.brief")).toBeUndefined();
    expect(teamTool(snap, "reprompt.add-what-changed")).toBeUndefined();
    expect(teamTool({}, "check_code")).toBeUndefined();
  });
});

describe("teamTipsText", () => {
  it("says where the lines come from and offers the one action that fits, never an address", () => {
    const t = snap.team_tips;
    expect(teamTipsText(t)).toEqual({ line: "1 habit has a Tool to try line. AIQ gets them from your team's repo every hour.", action: { label: "Stop using", does: "off" } });
    expect(teamTipsText({ ...t, state: "off" }).action?.does).toBe("auto");
    expect(teamTipsText({ ...t, state: "sign_in", tools: [] }).action?.does).toBe("sign_in");
    expect(teamTipsText({ ...t, state: "no_git" }).action).toBeUndefined();
    for (const state of ["download", "found", "chosen", "off", "sign_in", "no_git", "none"] as const) {
      expect(teamTipsText({ ...t, state }).line).not.toMatch(/https?:|ghe\.com|github\.com/);
    }
  });
});
