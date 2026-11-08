import { describe, expect, it } from "vitest";
import { teamTool } from "./team";

const snap = { team_tips: { set: true, problem: null, tools: [{ move_id: "check_code", tool: "T", how: "h." }] } };

describe("teamTool", () => {
  it("finds a habit's tool by move id or next-step tip id, and nothing for other tips", () => {
    expect(teamTool(snap, "check_code")?.tool).toBe("T");
    expect(teamTool(snap, "next_rung.check_code")?.tool).toBe("T");
    expect(teamTool(snap, "next_rung.brief")).toBeUndefined();
    expect(teamTool(snap, "reprompt.add-what-changed")).toBeUndefined();
    expect(teamTool({}, "check_code")).toBeUndefined();
  });
});
