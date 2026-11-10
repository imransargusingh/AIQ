// Unit tests (D7: fake CoreClient) and a real-socket round trip (D4) for the extension's core client.
import { test } from "node:test";
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as net from "node:net";
import * as os from "node:os";
import * as path from "node:path";
import { Card, CoreClient, Note, DOWN_POLL_MS, FEEDBACK_BUTTONS, POLL_MS, coreClient, detailsText, executableCandidates, notificationText, pollOnce, request, statusText } from "../core";

const card: Card = {
  schema: "aiq.card.v1",
  card_id: "01J",
  opportunity_id: "reprompt",
  tip_id: "reprompt.add-what-changed",
  noticed: "this request went round 3 times. The agent may need one new detail.",
  why: "rewording alone rarely changes the result.",
  try_this: "say what was wrong with the last answer.",
  evidence_ref: "aiq show 01J",
  pack_version: "bundled-2",
};

function fake(o: { paused?: boolean; take?: Card | Note | null | undefined }): CoreClient & { asked: number } {
  const c = {
    asked: 0,
    paused: () => o.paused ?? false,
    takeCard: async () => {
      c.asked++;
      return o.take;
    },
    show: async () => null,
    feedback: async () => true,
  };
  return c;
}

test("pollOnce_paused_neverAsksCore", async () => {
  const c = fake({ paused: true, take: card });
  assert.deepEqual(await pollOnce(c, true, undefined), { state: "paused", nextMs: POLL_MS });
  assert.equal(c.asked, 0);
});

test("pollOnce_unfocused_neverTakesACard", async () => {
  const c = fake({ take: card });
  assert.equal((await pollOnce(c, false, undefined)).state, "on");
  assert.equal(c.asked, 0);
});

test("pollOnce_card_notifiesAndShowsWaiting", async () => {
  const r = await pollOnce(fake({ take: card }), true, undefined);
  assert.equal(r.state, "tip");
  assert.equal(r.card, card);
});

test("pollOnce_coreDown_backsOffAndKeepsWaitingTip", async () => {
  assert.deepEqual(await pollOnce(fake({ take: undefined }), true, undefined), { state: "down", nextMs: DOWN_POLL_MS });
  assert.equal((await pollOnce(fake({ take: undefined }), true, card)).state, "tip");
});

test("pollOnce_milestoneNote_notifiesOnceWithoutATipWaiting", async () => {
  const note = "AIQ milestone: Writing tests reached Delegated · see it in AIQ Home";
  assert.deepEqual(await pollOnce(fake({ take: { note } }), true, undefined), { state: "on", note, nextMs: POLL_MS });
  assert.equal((await pollOnce(fake({ take: { note: "a\u001b[31mb" } }), true, card)).note, "a [31mb", "plain text, tip still waiting");
  assert.equal((await pollOnce(fake({ take: { note } }), true, card)).state, "tip");
});

test("pollOnce_noCard_on", async () => {
  assert.deepEqual(await pollOnce(fake({ take: null }), true, undefined), { state: "on", nextMs: POLL_MS });
});

test("notificationText_plainText_controlAndMarkupNotInterpreted", () => {
  const evil = { ...card, noticed: "x\u001b[31m [click](command:workbench.action.reloadWindow)", try_this: "y\n" };
  const t = notificationText(evil);
  assert.ok(!t.includes("\u001b") && !t.includes("\n"));
  assert.ok(t.includes("[click](command:workbench.action.reloadWindow)"), "shown literally; messages are not markdown");
  assert.equal(notificationText(card), "AIQ tip: this request went round 3 times. The agent may need one new detail. Try this: say what was wrong with the last answer.");
});

test("detailsText_evidenceAndDeleted", () => {
  const r = { card, outcome: "show_ready", reason_code: "ok", delivered: true, evidence: [{ ts: "t1", text: "fix it" }, { ts: "t2", text: null }] };
  const d = detailsText(r);
  assert.ok(d.includes("1. t1  fix it") && d.includes("2. t2  (deleted)") && d.includes("AIQ: Pause"));
  assert.ok(detailsText({ ...r, evidence: [{ ts: "t", text: null }] }).includes("prompts behind this tip were deleted"));
  assert.equal(detailsText({ ...r, card: null }), "No tip with that id.\n");
});

test("statusText_everyStateHasTextAndTooltip", () => {
  for (const s of ["on", "paused", "down", "tip"] as const) {
    const t = statusText(s);
    assert.ok(t.label.startsWith("AIQ") && !t.label.includes("$(") && t.tooltip.length > 0);
    assert.equal(t.highlight, s === "tip", "only a waiting tip is highlighted");
  }
  assert.equal(statusText("tip").text, "$(lightbulb) AIQ: 1 tip waiting");
  assert.equal(statusText("on").text, "AIQ on");
});

test("executableCandidates_settingFirstThenPathThenInstallDirs", () => {
  const c = executableCandidates(" C:\\x\\aiq.exe ", { Path: "C:\\a;C:\\b", LOCALAPPDATA: "C:\\L", USERPROFILE: "C:\\U" }, "win32");
  assert.equal(c[0], "C:\\x\\aiq.exe");
  assert.ok(c.some((p) => p.includes("AIQ") && p.endsWith("aiq.exe")));
  // The installer's folder, right after PATH: an open VS Code doesn't see the PATH install.ps1 just set.
  assert.ok(c[3].startsWith("C:\\U") && c[3].includes(".aiq") && c[3].endsWith("aiq.exe"), c[3]);
  const u = executableCandidates("", { PATH: "/usr/bin", HOME: "/h" }, "linux");
  assert.deepEqual(u, ["/usr/bin/aiq", "/h/.aiq/bin/aiq", "/h/.local/bin/aiq"]);
});

test("request_realSocket_sendsTokenAndParsesReply_wrongTokenGetsNull", async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "aiq-ext-"));
  const sock = path.join(dir, "core.sock");
  fs.writeFileSync(path.join(dir, "token"), "secret\n");
  const seen: Record<string, unknown>[] = [];
  let plainReply = false;
  let noteReply = false;
  const server = net.createServer((c) => {
    c.setEncoding("utf8");
    c.once("data", (d: string) => {
      const f = JSON.parse(d.trim());
      seen.push(f);
      if (f.token !== "secret") return c.destroy(); // the core closes a bad-token frame without reply
      const reply = noteReply ? { card: null, note: "AIQ milestone: x" } : plainReply ? { card, plain: true } : { card };
      c.end(JSON.stringify(reply) + "\n");
    });
  });
  await new Promise<void>((r) => server.listen(sock, r));
  try {
    const ep = { socket: sock, token: path.join(dir, "token"), paused_flag: path.join(dir, "paused") };
    assert.deepEqual(await coreClient(ep).takeCard(), card);
    plainReply = true;
    assert.deepEqual(await coreClient(ep).takeCard(), { ...card, plain: true }, "highlight off reaches the extension");
    noteReply = true;
    assert.deepEqual(await coreClient(ep).takeCard(), { note: "AIQ milestone: x" }, "a milestone note when no tip waits");
    assert.deepEqual(seen[0], { op: "take_card_any", surface: "vscode", token: "secret" });
    fs.writeFileSync(ep.token, "stale");
    assert.equal(await request(ep, { op: "status" }), null);
    assert.equal(coreClient(ep).paused(), false);
    fs.writeFileSync(ep.paused_flag, "paused\n");
    assert.equal(coreClient(ep).paused(), true);
    assert.equal(await coreClient({ ...ep, socket: path.join(dir, "none.sock") }).takeCard(), undefined);
  } finally {
    server.close();
  }
});

test("detailsText_nextRung_starterAndKindsNeverPromptText", () => {
  const c: Card = { ...card, opportunity_id: "next_rung", tip_id: "next_rung.agent_checks", starter: "Run <command>." };
  const d = detailsText({
    card: c,
    outcome: "show_ready",
    reason_code: "ok",
    delivered: true,
    evidence: [],
    leaned_on: [{ ts: "2026-11-01T10:00:00Z", tool: "copilot-cli", task_type: "Writing tests" }],
  });
  assert.ok(d.includes("Run <command>."));
  assert.ok(d.includes("copilot-cli  Writing tests"));
  assert.ok(!d.includes("prompts it counted"));
});

test("feedbackButtons_matchTheCoreAnswers", () => {
  assert.deepEqual(FEEDBACK_BUTTONS.map(([, a]) => a), ["helpful", "not_helpful", "snooze", "off"]);
});

test("teamTool_addedToNotificationAndDetails_onlyWhenTheCoreSendsIt", () => {
  const c = { ...card, team_tool: "Tool to try: X: does Y." };
  assert.equal(notificationText(c), `${notificationText(card)} Tool to try: X: does Y.`);
  const r = { card, outcome: "show_ready", reason_code: "ok", delivered: true, evidence: [{ ts: "t", text: "p" }] };
  assert.ok(!detailsText(r).includes("Tool to try"));
  assert.ok(detailsText({ ...r, team_tool: "Tool to try: X: does Y." }).includes("Try this: say what was wrong with the last answer.\nTool to try: X: does Y.\n\n"));
});
