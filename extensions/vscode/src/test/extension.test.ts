// The extension's own wiring (extension.ts) against the real Rust core, with a stand-in `vscode` module.
// core.test.ts covers the helpers; nothing else runs activate(), so a broken poll, notification or answer
// button would lose every VS Code tip without a failing test. Runs when AIQ_BIN points at a built aiq.
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { FEEDBACK_BUTTONS, loadEndpoint, request } from "../core";

const bin = process.env.AIQ_BIN;

type Shown = { text: string; buttons: string[]; answer: (pick: string | undefined) => void };

/** A stand-in for the parts of the `vscode` API extension.ts uses, recording what it shows. */
function fakeVscode(executable: string) {
  const shown: Shown[] = [];
  const commands = new Map<string, (...a: unknown[]) => unknown>();
  const item = { text: "", tooltip: "", command: "", backgroundColor: undefined as unknown, accessibilityInformation: {}, show() {} };
  const output = { text: "", shownPreservingFocus: false, clear() { this.text = ""; }, append(s: string) { this.text += s; }, show(p: boolean) { this.shownPreservingFocus = p; } };
  const vscode = {
    StatusBarAlignment: { Right: 2 },
    ThemeColor: class { constructor(public id: string) {} },
    window: {
      state: { focused: true },
      createStatusBarItem: () => item,
      createOutputChannel: () => output,
      showInformationMessage: (text: string, ...buttons: string[]) =>
        new Promise<string | undefined>((answer) => shown.push({ text, buttons, answer })),
      showQuickPick: async () => undefined,
    },
    workspace: { getConfiguration: () => ({ get: (k: string, d: unknown) => (k === "executable" ? executable : d) }) },
    commands: { registerCommand: (id: string, f: (...a: unknown[]) => unknown) => (commands.set(id, f), { dispose() {} }) },
  };
  return { vscode, shown, commands, item, output };
}

/** Load extension.ts with `vscode` resolved to the stand-in (VS Code provides that module only at runtime). */
function loadExtension(vscode: object): typeof import("../extension") {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const Module = require("node:module");
  const original = Module._load;
  Module._load = function (req: string, ...rest: unknown[]) {
    return req === "vscode" ? vscode : original.call(this, req, ...rest);
  };
  try {
    delete require.cache[require.resolve("../extension")];
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    return require("../extension");
  } finally {
    Module._load = original;
  }
}

async function until<T>(f: () => T | undefined | Promise<T | undefined>, ms: number, what: string): Promise<T> {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = await f();
    if (v !== undefined) return v;
    if (Date.now() > deadline) throw new Error(`timed out: ${what}`);
    await new Promise((r) => setTimeout(r, 25));
  }
}

test("activate_agentTurnTip_notifiesWithButtons_answerSavedAndTryThisOpensDetails", { skip: !bin && "AIQ_BIN not set" }, async () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "aiq-ext-wiring-"));
  process.env.AIQ_HOME = home;
  const env: NodeJS.ProcessEnv = { ...process.env, AIQ_HOME: home, COPILOT_CLI_RUN_AS_NODE: "1" };
  delete env.COPILOT_LOADER_PID;
  delete env.AIQ_INTERNAL;
  const core = spawn(bin!, ["core"], { env, stdio: "ignore" });
  const hook = (event: string, payload: object) => {
    const r = spawnSync(bin!, ["hook", "copilot", event], { env, input: JSON.stringify(payload) });
    assert.equal(r.status, 0);
  };
  const f = fakeVscode(bin!);
  const ext = loadExtension(f.vscode);
  try {
    const ep = await loadEndpoint(bin!);
    await until(async () => (await request(ep, { op: "status" })) ?? undefined, 5000, "core up");
    for (const [i, prompt] of [
      "refactor the login handler to return typed errors",
      "refactor login handler so it returns typed errors",
      "login handler refactor: return typed errors please",
      "please refactor the login handler, typed errors returned",
    ].entries()) {
      hook("userPromptSubmitted", { sessionId: `w${i}`, timestamp: Date.now(), cwd: "/repo", prompt });
      hook("agentStop", { sessionId: `w${i}`, timestamp: Date.now(), stopReason: "end_turn", transcriptPath: "/t" });
    }
    // Activation polls at once; give the core time to decide the tip if the first poll beat it.
    await ext.activate({ subscriptions: [] } as never);
    const first = await until(() => f.shown[0], 15_000, "a notification for the agent turn's tip");
    assert.ok(first.text.startsWith("AIQ tip: this request went round 3 times"), first.text);
    assert.deepEqual(first.buttons, ["Try this", ...FEEDBACK_BUTTONS.map(([label]) => label)]);
    assert.equal(f.item.text, "$(lightbulb) AIQ: 1 tip waiting");
    assert.ok(f.item.backgroundColor, "a waiting tip is highlighted");

    // An answer button records the answer with the core: no "wasn't saved" message follows.
    first.answer("Helpful");
    await new Promise((r) => setTimeout(r, 500));
    assert.equal(f.shown.length, 1, f.shown.map((s) => s.text).join(" | "));

    // "Show tip" (the Try this path) opens the details without taking focus, then clears the waiting tip.
    await f.commands.get("aiq.showTip")!();
    assert.ok(f.output.text.length > 0 && !f.output.text.includes("can't open"), f.output.text);
    assert.equal(f.output.shownPreservingFocus, true, "never takes focus from the editor");
    assert.equal(f.item.text.includes("tip waiting"), false, f.item.text);
  } finally {
    ext.deactivate();
    core.kill();
  }
});
