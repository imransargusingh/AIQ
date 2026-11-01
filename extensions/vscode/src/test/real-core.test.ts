// Round trip against the real Rust core (D4). Runs when AIQ_BIN points at a built aiq binary (CI sets it).
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { ShowReply, coreClient, loadEndpoint, request } from "../core";

const bin = process.env.AIQ_BIN;

test("realCore_endpointTakeCardAnyAndStatus", { skip: !bin && "AIQ_BIN not set" }, async () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "aiq-ext-core-"));
  process.env.AIQ_HOME = home;
  const core = spawn(bin!, ["core"], { env: { ...process.env, AIQ_HOME: home }, stdio: "ignore" });
  try {
    const ep = await loadEndpoint(bin!);
    let status: Record<string, unknown> | null = null;
    for (let i = 0; i < 200 && !status; i++) {
      status = await request<Record<string, unknown>>(ep, { op: "status" });
      if (!status) await new Promise((r) => setTimeout(r, 25));
    }
    assert.ok(status && "daily_cap" in status, "core answers status");
    assert.equal(await coreClient(ep).takeCard(), null, "no card yet");
    const shown = await request<ShowReply>(ep, { op: "show", card_id: "nope", opened_on: "vscode" });
    assert.equal(shown?.card, null);
  } finally {
    core.kill();
  }
});

// Laptop probe run 2: VS Code agent mode runs ~/.copilot/hooks with COPILOT_CLI_RUN_AS_NODE set and no
// COPILOT_LOADER_PID. A reworded run there must reach this extension, not the Copilot CLI status line.
test("realCore_vscodeAgentTurnReachesExtension", { skip: !bin && "AIQ_BIN not set" }, async () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "aiq-ext-vsc-"));
  process.env.AIQ_HOME = home;
  const env: NodeJS.ProcessEnv = { ...process.env, AIQ_HOME: home, COPILOT_CLI_RUN_AS_NODE: "1" };
  delete env.COPILOT_LOADER_PID;
  delete env.AIQ_INTERNAL;
  const core = spawn(bin!, ["core"], { env, stdio: "ignore" });
  const hook = (event: string, payload: object) => {
    const r = spawnSync(bin!, ["hook", "copilot", event], { env, input: JSON.stringify(payload) });
    assert.equal(r.status, 0);
    assert.equal(r.stdout.length + r.stderr.length, 0, "Copilot hooks never print");
  };
  try {
    const ep = await loadEndpoint(bin!);
    for (let i = 0; i < 200 && !(await request(ep, { op: "status" })); i++) await new Promise((r) => setTimeout(r, 25));
    for (const [i, prompt] of [
      "refactor the login handler to return typed errors",
      "refactor login handler so it returns typed errors",
      "login handler refactor: return typed errors please",
      "please refactor the login handler, typed errors returned",
    ].entries()) {
      // VS Code agent mode can open a new Copilot session for every turn (probe run 2): one session per prompt.
      const sessionId = `v${i}`;
      hook("userPromptSubmitted", { sessionId, timestamp: Date.now(), cwd: "/repo", prompt });
      hook("agentStop", { sessionId, timestamp: Date.now(), stopReason: "end_turn", transcriptPath: "/t" });
    }
    let card = null;
    for (let i = 0; i < 200 && !card; i++) {
      card = await coreClient(ep).takeCard();
      if (!card) await new Promise((r) => setTimeout(r, 25));
    }
    assert.ok(card, "the VS Code turn's tip reaches the extension");
    const line = spawnSync(bin!, ["statusline", "copilot"], { env, input: JSON.stringify({ session_id: "v3" }) });
    assert.equal(line.stdout.toString(), "", "and never the Copilot CLI status line");
  } finally {
    core.kill();
  }
});
