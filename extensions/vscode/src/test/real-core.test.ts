// Round trip against the real Rust core (D4). Runs when AIQ_BIN points at a built aiq binary (CI sets it).
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { spawn } from "node:child_process";
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
