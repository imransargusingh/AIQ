// UI end-to-end checks over the built app (dist/) with snapshots the Rust read model wrote
// (tests/fixtures, from `cargo run -p aiq-home --example fixture`). Design aiq-home-app, Test plan.
// Run: npm run build && npm run e2e. Screenshots go to $AIQ_SHOTS (optional).
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync, existsSync, mkdirSync } from "node:fs";
import { join, extname, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "..", "dist");
const fixtures = join(here, "fixtures");
const axeSrc = readFileSync(join(here, "..", "node_modules", "axe-core", "axe.min.js"), "utf8");
const shots = process.env.AIQ_SHOTS;
if (shots) mkdirSync(shots, { recursive: true });

const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css" };
const server = createServer((req, res) => {
  // Some Chromium builds ask for a favicon the app doesn't have; the Tauri window never does.
  if (req.url === "/favicon.ico") return res.writeHead(204).end();
  const p = join(dist, decodeURIComponent(new URL(req.url, "http://x").pathname).replace(/^\/$/, "/index.html"));
  if (!p.startsWith(dist) || !existsSync(p)) return res.writeHead(404).end();
  res.writeHead(200, { "content-type": types[extname(p)] ?? "application/octet-stream" }).end(readFileSync(p));
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;

const browser = await chromium.launch(process.env.PW_CHROMIUM ? { executablePath: process.env.PW_CHROMIUM } : {});
const results = [];
async function test(name, fn) {
  try {
    await fn();
    results.push([name, "pass"]);
    console.log(`ok   ${name}`);
  } catch (e) {
    results.push([name, "FAIL"]);
    console.log(`FAIL ${name}\n     ${e.message.split("\n").slice(0, 6).join("\n     ")}`);
  }
}

async function open(fixture, { theme, hash = "", width = 1280, height = 800, forced = false, edit, fail = [], clock = false } = {}) {
  const ctx = await browser.newContext({ viewport: { width, height }, forcedColors: forced ? "active" : "none" });
  const page = await ctx.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  const snapshot = JSON.parse(readFileSync(join(fixtures, `${fixture}.json`), "utf8"));
  const task = JSON.parse(readFileSync(join(fixtures, "task.json"), "utf8"));
  if (edit) edit(snapshot);
  await page.addInitScript(([s, t, f]) => {
    window.__AIQ_FIXTURE__ = { snapshot: s, task: t, calls: [], fail: f };
  }, [snapshot, task, fail]);
  if (clock) await page.clock.install();
  await page.goto(base + hash);
  if (theme) await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
  await page.waitForSelector("main h1");
  if (theme) await settle(page);
  page.errors = errors;
  return page;
}

/** Wait out the colour fades a theme switch starts (nav links and buttons fade their background over 100 ms):
 * axe reading mid-fade sees dark ink on the old light background (PR #150's flaky dark run, 5 in 60 runs locally). */
async function settle(page) {
  await page.evaluate(() => Promise.all(document.getAnimations().map((a) => a.finished.catch(() => {}))));
}

async function axe(page) {
  await page.evaluate(axeSrc);
  const r = await page.evaluate(() => window.axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"] } }));
  return r.violations.map((v) => `${v.id}: ${v.nodes.slice(0, 3).map((n) => n.target.join(" ")).join(" | ")}`);
}

const pages = [
  ["home", "Home"],
  ["tasks", "Tasks"],
  ["guide", "Rung guide"],
  ["tips", "Habits and tips"],
  ["data", "Your data"],
  ["settings", "Settings"],
];

await test("contract: fixtures are aiq.home.v1 snapshots from the Rust read model", async () => {
  for (const f of ["full", "down", "paused", "empty", "first-run", "coaching", "progress", "live", "live-many", "tour"]) {
    const s = JSON.parse(readFileSync(join(fixtures, `${f}.json`), "utf8"));
    assert.equal(s.schema, "aiq.home.v1");
    for (const k of ["status", "home", "tasks", "tips", "data", "settings", "setup", "coaching"]) assert.ok(k in s, `${f}: ${k}`);
    assert.equal(s.setup.done, f !== "first-run", f);
  }
  const t = JSON.parse(readFileSync(join(fixtures, "task.json"), "utf8"));
  for (const k of ["task", "prompts", "read_ms"]) assert.ok(k in t, `task: ${k}`);
});

for (const theme of ["light", "dark", "contrast"]) {
  await test(`every page renders and has no axe violations (${theme})`, async () => {
    for (const [id, title] of pages) {
      const page = await open("full", { theme: theme === "light" ? undefined : theme, hash: `#${id}` });
      assert.equal(await page.locator("main h1").first().textContent(), title);
      const v = await axe(page);
      assert.deepEqual(v, [], `${id}: ${v.join("; ")}`);
      assert.deepEqual(page.errors, []);
      if (shots) await page.screenshot({ path: join(shots, `${id}-${theme}.png`), fullPage: true });
      await page.context().close();
    }
  });
}

for (const theme of ["light", "dark", "contrast"]) {
  await test(`coaching: every page and a kind of task have no axe violations (${theme})`, async () => {
    for (const [id] of [...pages, ["kind/code_change/tests"], ["kind/code_change/fix_bug"]]) {
      const page = await open("coaching", { theme: theme === "light" ? undefined : theme, hash: `#${id}` });
      const v = await axe(page);
      assert.deepEqual(v, [], `${id}: ${v.join("; ")}`);
      assert.deepEqual(page.errors, []);
      if (shots) await page.screenshot({ path: join(shots, `coaching-${id.replaceAll("/", "-")}-${theme}.png`), fullPage: true });
      await page.context().close();
    }
  });
}

await test("forced colours (Windows high contrast) has no axe violations", async () => {
  const page = await open("full", { forced: true });
  assert.deepEqual(await axe(page), []);
  await page.context().close();
});

await test("Home shows analyzed rungs, progress toward the split, and only measured counts", async () => {
  const page = await open("full", { edit: (s) => Object.assign(s.home.analysis, { state: "working" }) });
  const t = await page.locator("main").innerText();
  // Analysis under way: progress, a forecast from the daily share, nothing to do, and the first results.
  assert.match(t, /Tasks agents finished\s+Analyzing your tasks\s+3 of 6 analyzed · about 500 a day, all by today/);
  assert.match(t, /Nothing to do\. AIQ works in the background, newest first\./);
  assert.match(t, /So far, from 2 tasks: mostly Directed/);
  assert.match(t, /How much agents finish shows here from 10 analyzed tasks \(2 so far; 1 task AIQ isn't sure about doesn't count\)/);
  await page.getByText("How AIQ counts").click();
  assert.match(await page.locator("details.how").innerText(), /7 tasks in the last 30 days · 1 imported task/);
  assert.doesNotMatch(t, /\d+%/, "no percentages under 10 analyzed tasks");
  assert.match(t, /Your most common rung\s+Directed \(2\)\s+3 of 6 tasks analyzed/);
  assert.match(t, /1 task AIQ wasn't sure about is left out · See them/);
  assert.match(t, /Next steps\s+[^\n]*\n+Next steps show once AIQ knows the kind of each task/);
  // The rung mix: counts only under 10 judged tasks (no share), each rung's meaning on hover, the change waits.
  const mix = await page.locator("section", { has: page.locator("h2#rg") }).innerText();
  assert.match(mix, /Directed is your most common rung so far\./);
  assert.match(mix, /The change shows once the last 14 days and the 14 before each have 10 analyzed tasks\./);
  for (const r of ["Assisted", "Directed", "Delegated", "Orchestrated", "Factory"]) assert.ok(mix.includes(r), r);
  assert.match((await page.locator(".mix-r span[title]").nth(2).getAttribute("title")) ?? "", /hand the agent a whole outcome/);
  assert.equal(await page.locator(".wk-col").count(), 4);
  assert.equal(await page.locator(".mix-col").count(), 4);
  // Selecting a rung lists its kinds of task.
  const row = page.getByRole("button", { name: /^Delegated \(3\): / });
  await row.click();
  assert.equal(await row.getAttribute("aria-pressed"), "true");
  assert.match(await page.locator(".mix-kinds").innerText(), /On Delegated:/);
  await page.context().close();
});

await test("first-run analysis: says what AIQ is doing, that nothing is needed, and one action only when it can't run", async () => {
  const fresh = (a) => (s) => {
    s.settings.judge = "auto";
    Object.assign(s.home, { judged: 0, rungs: [0, 0, 0, 0, 0], most_common_rung: null, uncertain: 0 });
    Object.assign(s.coaching.split, { tasks: 0, uncertain: 0 });
    Object.assign(s.home.analysis, { state: "working", waiting: 221, analyzed: 0, today: 0, done_by_ms: s.now_ms, blocked: null }, a);
  };
  const page = await open("full", { edit: fresh({}) });
  let t = await page.locator("main").innerText();
  assert.match(t, /Your most common rung\s+Not analyzed yet\s+Analyzing 221 tasks, newest first/);
  assert.match(t, /Analyzing your tasks\s+0 of 221 analyzed · about 500 a day, all by today/);
  assert.match(t, /Nothing to do\./);
  const card = await page.locator("section", { has: page.locator("h2#kr") }).innerText();
  assert.doesNotMatch((await page.locator("section.mainrung").innerText()) + card.split("How AIQ counts")[0], /judg/i, "new text says analyze");
  assert.match(await page.locator("section[aria-labelledby=gs]").innerText(), /Analysis is on: 0 of 221 tasks\s+About 500 a day, newest first\. Nothing to do\./);
  assert.equal(await page.getByRole("button", { name: /Sign in/ }).count(), 0, "no action while it works");
  assert.deepEqual(await axe(page), []);
  if (shots) await page.screenshot({ path: join(shots, "home-analysis-working.png") });
  await page.context().close();
  // Today's share used up: the header says more come tomorrow.
  const later = await open("full", { edit: fresh({ state: "today_done", analyzed: 50, waiting: 171, today: 50, daily: 50 }) });
  t = await later.locator("main").innerText();
  assert.match(t, /Today's analysis is done[\s\S]*AIQ does the next 50 tomorrow\./);
  await later.context().close();
  // Copilot CLI signed out: the reason and one button, which opens the sign-in.
  const blocked = await open("full", { edit: fresh({ state: "blocked", blocked: { tool: "copilot-cli", why: "not_signed_in" } }) });
  t = await blocked.locator("main").innerText();
  assert.match(t, /Analysis can't run\s+Copilot CLI isn't signed in, and AIQ analyzes your tasks through it\. 221 tasks wait\./);
  assert.match(t, /Analysis can't run: Copilot CLI not signed in/);
  await blocked.getByRole("button", { name: "Sign in to Copilot" }).click();
  await blocked.waitForFunction(() => window.__AIQ_FIXTURE__.calls.some((c) => c.cmd === "open_sign_in" && c.args.tool === "copilot-cli"));
  assert.deepEqual(await axe(blocked), []);
  if (shots) await blocked.screenshot({ path: join(shots, "home-analysis-blocked.png") });
  assert.deepEqual(blocked.errors, []);
  await blocked.context().close();
});

await test("every rung name says what it means on hover: Home ladder, Tasks column, task detail", async () => {
  const page = await open("full");
  const scale = page.locator(".scale > span");
  assert.equal(await scale.count(), 5);
  assert.match((await scale.nth(2).getAttribute("title")) ?? "", /hand the agent a whole outcome/);
  await page.context().close();
  const tasks = await open("full", { hash: "#tasks" });
  assert.match((await tasks.locator("th .rung").getAttribute("title")) ?? "", /from Assisted \(1\) to Factory \(5\)/);
  const named = tasks.locator("table[aria-label=Tasks] td .rung");
  assert.ok((await named.count()) > 0, "analyzed rows name a rung");
  for (const r of await named.all()) assert.ok(((await r.getAttribute("title")) ?? "").length > 20, await r.innerText());
  assert.deepEqual(await axe(tasks), []);
  await tasks.locator("tbody tr").first().focus();
  await tasks.keyboard.press("j");
  await tasks.keyboard.press("Enter");
  await tasks.waitForSelector("#jd");
  assert.ok(((await tasks.locator("section .chip .rung").first().getAttribute("title")) ?? "").length > 20, "task detail rung chip");
  assert.deepEqual(tasks.errors, []);
  await tasks.context().close();
});

await test("Home answers the headline without scrolling at 1280x800", async () => {
  const page = await open("coaching");
  const box = await page.locator("#kr").boundingBox();
  assert.ok(box && box.y + box.height < 800, `headline at ${box?.y}`);
  assert.equal(await page.locator("#gs").count(), 0, "no getting-started card once the numbers show");
  await page.context().close();
  // Before the first numbers, the getting-started card is the answer to "is it working?".
  const early = await open("full");
  const gs = await early.locator("#gs").boundingBox();
  assert.ok(gs && gs.y + gs.height < 800, `getting started at ${gs?.y}`);
  await early.context().close();
});

await test("Getting started ticks itself from the snapshot (item 7)", async () => {
  const page = await open("empty");
  assert.equal(await page.locator("main h1").innerText(), "AIQ is getting to know your work");
  const items = await page.locator("ul.cklist li").allInnerTexts();
  assert.equal(items.length, 4, items.join("|"));
  assert.match(items[0], /Claude Code connected[\s\S]*GitHub Copilot is on this computer but not connected\. Connect/);
  assert.match(items[1], /No prompt yet/);
  assert.match(items[2], /Analysis is off[\s\S]*Turn on in Settings/);
  assert.match(items[3], /Your first numbers[\s\S]*0 of 10 analyzed tasks/);
  assert.equal(await page.locator("ul.cklist .mark.on").count(), 1);
  assert.deepEqual(await page.locator("ul.cklist .sr").allInnerTexts(), ["Done: ", "Not yet: ", "Not yet: ", "Not yet: "].map((x) => x.trim()));
  assert.deepEqual(await axe(page), []);
  if (shots) await page.screenshot({ path: join(shots, "home-getting-started.png") });
  await page.getByRole("button", { name: "Turn on in Settings" }).click();
  await page.waitForFunction(() => location.hash === "#settings");
  await page.context().close();
  const full = await open("full");
  const card = full.locator("section[aria-labelledby=gs]");
  assert.match(await card.innerText(), /Getting started\s+2 of 4 done[\s\S]*Prompts arriving[\s\S]*11 prompts and 7 tasks so far in Copilot CLI, Claude Code, VS Code[\s\S]*Analysis is off[\s\S]*2 of 10 analyzed tasks/);
  assert.deepEqual(await axe(full), []);
  assert.deepEqual(full.errors, []);
  await full.context().close();
});

await test("screen readers hear clean names: nav, rung rows, skip link", async () => {
  const page = await open("full");
  // Names come from the accessibility tree, as a screen reader gets them (laptop check 2026-11-01).
  const nav = await page.getByRole("navigation", { name: "AIQ" }).ariaSnapshot();
  assert.match(nav, /link "Home"/, nav);
  assert.equal(await page.getByRole("link", { name: "Home", exact: true }).getAttribute("aria-keyshortcuts"), "G H");
  // The G-letter shortcuts still work but the nav shows no hint text (Imran 2026-11-02: "Get rid of the hints").
  assert.doesNotMatch(await page.locator("nav.side a").allInnerTexts().then((t) => t.join("|")), /\bG [A-Z]\b/);
  for (const name of ["Factory (5): 0 tasks", "Directed (2): 1 task"]) {
    assert.equal(await page.getByRole("button", { name, exact: true }).count(), 1, name);
  }
  // Focus starts on the page title; the skip link is the first stop of the window chrome, before the nav.
  assert.equal(await page.evaluate(() => document.querySelector("a[href], button")?.textContent), "Skip to content");
  await page.getByRole("link", { name: "Skip to content" }).focus();
  assert.ok((await page.getByRole("link", { name: "Skip to content" }).boundingBox()).y >= 0, "visible when focused");
  await page.keyboard.press("Enter");
  assert.equal(await page.evaluate(() => document.activeElement?.tagName), "MAIN");
  await page.context().close();
});

await test("keyboard: G-letter navigation, P pause, Ctrl+K palette", async () => {
  const page = await open("full");
  await page.keyboard.press("g");
  await page.keyboard.press("t");
  assert.equal(await page.locator("main h1").textContent(), "Tasks");
  assert.equal(await page.evaluate(() => document.activeElement?.tagName), "H1", "focus moves to the page title");
  await page.keyboard.press("p");
  await page.waitForSelector("text=AIQ is paused.");
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.includes("paused"));
  assert.equal(await page.locator(".titlebar .pausedchip").count(), 1);
  await page.keyboard.press("p");
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "AIQ resumed.");
  await page.keyboard.press("Control+k");
  await page.waitForSelector("dialog[open] [role=combobox]");
  assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("role")), "combobox");
  await page.keyboard.type("your data");
  await page.keyboard.press("Enter");
  assert.equal(await page.locator("main h1").textContent(), "Your data");
  assert.equal(await page.locator("dialog[open]").count(), 0);
  await page.keyboard.press("Control+k");
  await page.keyboard.type("zzz");
  assert.ok((await page.locator("dialog[open]").innerText()).includes("No command matches."));
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("dialog[open]").count(), 0);
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.map((c) => c.cmd));
  assert.deepEqual(calls.filter((c) => c === "pause").length, 2);
  await page.context().close();
});

await test("Settings: a switch for Copilot messages sent while the agent works, on by default", async () => {
  const page = await open("full", { hash: "#settings" });
  const sw = page.getByRole("switch", { name: "Copilot messages you send while the agent is still working" });
  assert.equal(await sw.getAttribute("aria-checked"), "true");
  assert.ok((await page.locator("main").innerText()).includes("Nothing leaves your computer."));
  await sw.click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.startsWith("AIQ no longer keeps"));
  assert.equal(await sw.getAttribute("aria-checked"), "false");
  assert.deepEqual(await axe(page), []);
  await page.context().close();
});

await test("Settings: a switch for what the agent did in each turn, on by default, counts only", async () => {
  const page = await open("full", { hash: "#settings" });
  const sw = page.getByRole("switch", { name: "What the agent did in each turn" });
  assert.equal(await sw.getAttribute("aria-checked"), "true");
  assert.ok((await page.locator("main").innerText()).includes("No command text or file names."));
  await sw.click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.includes("what the agent did"));
  assert.equal(await sw.getAttribute("aria-checked"), "false");
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls);
  assert.ok(calls.some((c) => c.cmd === "set_setting" && c.args?.key === "agent_actions" && c.args?.value === "off"), JSON.stringify(calls));
  assert.deepEqual(await axe(page), []);
  await page.context().close();
});

await test("keyboard: single-key shortcuts can be turned off", async () => {
  const page = await open("full", { hash: "#settings" });
  await page.getByRole("switch", { name: /Single-key shortcuts/ }).click();
  await page.locator("body").click();
  await page.keyboard.press("g");
  await page.keyboard.press("h");
  assert.equal(await page.locator("main h1").textContent(), "Settings");
  await page.keyboard.press("Control+k");
  assert.equal(await page.locator("dialog[open]").count(), 1, "Ctrl+K still works");
  await page.context().close();
});

await test("Tasks: sort, J/K, imported and not-judged labels; prompt markup is text", async () => {
  const page = await open("full", { hash: "#tasks" });
  assert.equal(await page.locator("table[aria-label=Tasks] tbody tr").count(), 7);
  const prompts = page.getByRole("button", { name: /^Prompts/ });
  await prompts.click();
  assert.equal(await page.locator("th").nth(2).getAttribute("aria-sort"), "descending");
  assert.equal((await page.locator("table[aria-label=Tasks] tbody tr").first().locator("td").nth(2).textContent()).trim(), "3");
  await page.locator("table[aria-label=Tasks] tbody tr").first().focus();
  await page.keyboard.press("j");
  assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("aria-selected")), "true");
  assert.equal(await page.locator("tbody tr[aria-selected=true]").count(), 1);
  const t = await page.locator("table[aria-label=Tasks] tbody").innerText();
  assert.ok(t.includes("Imported"));
  assert.ok(t.includes("Not analyzed yet") && t.includes("Not classified yet"));
  assert.ok(t.includes("Uncertain"), "an Uncertain judgment says so");
  assert.ok(t.includes("Code a change") && t.includes("Write a spec or doc"), "analyzed jobs in words");
  await page.locator("a[href='#data']").click();
  assert.equal(await page.locator("main img").count(), 0, "a prompt containing <img> renders as text");
  assert.ok((await page.locator("main").innerText()).includes("<img src=x onerror=alert(1)> add tests"));
  assert.ok((await page.locator("main").innerText()).includes("Text deleted"));
  await page.context().close();
});

await test("Your data: long prompts are collapsed to a preview, full text one click away", async () => {
  // Real prompts can be many KB (laptop check 2026-11-01: the page was the slowest to check).
  const long = "start " + "word ".repeat(2000) + "end";
  const page = await open("full", { hash: "#data", edit: (s) => s.data.items.forEach((i) => i.text && (i.text = long)) });
  const first = page.locator("tbody tr").first().locator("details");
  assert.equal(await first.count(), 1);
  assert.ok(!(await first.evaluate((d) => d.open)));
  assert.ok((await first.locator("summary").innerText()).length < 320);
  assert.ok(!(await first.locator(".rest").isVisible()), "full text hidden until opened");
  await first.locator("summary").focus();
  await page.keyboard.press("Enter");
  assert.ok(await first.locator(".rest").isVisible());
  assert.ok((await first.innerText()).includes(" end"));
  assert.deepEqual(await axe(page), []);
  await page.context().close();
});

await test("Delete everything: focus on Cancel, trapped, typed forget, empty state announced", async () => {
  const page = await open("full", { hash: "#data" });
  const opener = page.getByRole("button", { name: "Delete everything…" });
  await opener.click();
  await page.waitForSelector("dialog[open]");
  assert.equal(await page.evaluate(() => document.activeElement?.textContent), "Cancel");
  const del = page.locator("dialog").getByRole("button", { name: "Delete everything" });
  assert.equal(await del.isDisabled(), true);
  for (let i = 0; i < 5; i++) await page.keyboard.press("Tab");
  assert.ok(await page.evaluate(() => !!document.activeElement?.closest("dialog")), "focus stays in the dialog");
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("dialog[open]").count(), 0);
  assert.equal(await page.evaluate(() => document.activeElement?.textContent), "Delete everything…", "focus returns");
  await opener.click();
  await page.locator("#forget").fill("forge");
  assert.equal(await del.isDisabled(), true);
  await page.locator("#forget").fill("forget");
  await del.click();
  await page.waitForSelector("text=AIQ has no stored history.");
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.startsWith("Deleted."));
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "forget_all"));
  assert.deepEqual(calls.map((c) => c.args.confirm), ["forget"]);
  await page.context().close();
});

await test("Settings change through the core; tips a day", async () => {
  const page = await open("full", { hash: "#settings" });
  // 5, 15, 30 and Custom (Imran 2026-11-07). The fixture's 3 isn't a button, so Custom is pressed and shows it.
  const cap = page.getByRole("group", { name: "Tips a day, across all your tools" });
  assert.deepEqual(await cap.getByRole("button").allInnerTexts(), ["5", "15", "30", "Custom"]);
  assert.equal(await cap.getByRole("button", { name: "Custom" }).getAttribute("aria-pressed"), "true");
  const box = page.getByLabel("Tips a day, 0 to 50");
  assert.equal(await box.inputValue(), "3");
  await cap.getByRole("button", { name: "5", exact: true }).click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Tips a day set to 5.");
  assert.equal(await cap.getByRole("button", { name: "5", exact: true }).getAttribute("aria-pressed"), "true");
  assert.equal(await box.count(), 0, "the number box folds away on a button value");
  // Custom: any whole number up to 50; anything else can't be set and the hint turns red.
  await cap.getByRole("button", { name: "Custom" }).click();
  await box.fill("51");
  assert.equal(await box.getAttribute("aria-invalid"), "true");
  assert.equal(await page.getByRole("button", { name: "Set", exact: true }).isDisabled(), true);
  await box.fill("12");
  await box.press("Enter");
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Tips a day set to 12.");
  assert.equal(await cap.getByRole("button", { name: "Custom" }).getAttribute("aria-pressed"), "true");
  assert.equal(await box.inputValue(), "12");
  if (shots) await page.locator("section[aria-labelledby=st1]").screenshot({ path: join(shots, "settings-tips-custom.png") });
  // Where tips show and how long they stay is folded under one line that says how it's set (item 8).
  const fold = page.locator("details.fold > summary");
  assert.equal((await fold.innerText()).replace(/\s+/g, " "), "Where tips show and how long they stay · 3 of 3 on · until your next prompt");
  await fold.click();
  await page.getByLabel("Until you open it").check();
  await page.getByRole("group", { name: /Tasks analyzed a day/ }).getByRole("button", { name: "100", exact: true }).click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Tasks analyzed a day set to 100.");
  assert.equal(await page.getByRole("group", { name: /Tasks analyzed a day/ }).getByRole("button", { name: "100", exact: true }).getAttribute("aria-pressed"), "true");
  // Judging has a switch here, not only a terminal command.
  const jsw = page.getByRole("switch", { name: "Analyze finished tasks in the background" });
  assert.equal(await jsw.getAttribute("aria-checked"), "false");
  await jsw.click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Analysis is on.");
  assert.equal(await jsw.getAttribute("aria-checked"), "true");
  assert.ok((await page.locator("main").innerText()).includes("Analysis in the background"));
  // Where tips show: three switches, on by default, each with its own off.
  for (const name of ["Show Copilot CLI tips in the conversation", "Show a Windows pop-up when a tip appears in Claude Code or Copilot CLI", "Highlight tips in yellow"]) {
    assert.equal(await page.getByRole("switch", { name }).getAttribute("aria-checked"), "true", name);
  }
  const pop = page.getByRole("switch", { name: "Show a Windows pop-up when a tip appears in Claude Code or Copilot CLI" });
  await pop.click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.endsWith(": off."));
  assert.equal(await pop.getAttribute("aria-checked"), "false");
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "set_setting").map((c) => c.args));
  assert.deepEqual(calls, [
    { key: "daily_cap", value: "5" },
    { key: "daily_cap", value: "12" },
    { key: "copilot_tip_stays", value: "until-opened" },
    { key: "judge_daily_tasks", value: "100" },
    { key: "judge", value: "auto" },
    { key: "tip_popup", value: "off" },
  ]);
  await page.context().close();
});

await test("Settings: a section index, Sharing and Keyboard folded in, analysis off says since when (item 8)", async () => {
  const page = await open("full", { hash: "#settings" });
  const nav = page.getByRole("navigation", { name: "Settings sections" });
  assert.deepEqual(await nav.getByRole("button").allInnerTexts(), ["Tips", "What AIQ keeps", "Analysis", "Updates", "Theme", "Tools"]);
  assert.deepEqual(await page.locator("main h2").allInnerTexts(), ["Tips", "What AIQ keeps", "Analysis", "Updates", "Theme", "Tools and keyboard"]);
  await nav.getByRole("button", { name: "Updates" }).click();
  await page.waitForFunction(() => document.activeElement?.id === "st6");
  // The index stays in view while the page scrolls.
  await page.waitForFunction(() => document.querySelector("main").scrollTop > 0 && document.querySelector(".secnav").getBoundingClientRect().top < 80);
  assert.equal(await page.evaluate(() => scrollY), 0, "only the page scrolls, not the window");
  assert.match(await page.getByTestId("privacy-line").innerText(), /^Sharing is off: AIQ shares nothing with anyone\. .*Read the privacy page/);
  assert.equal(await page.getByRole("switch", { name: /Single-key shortcuts/ }).count(), 1);
  // Off and never switched (no stored time): no invented "since", and today's counts say they're judged, not running.
  const judging = page.locator("section[aria-labelledby=st3]");
  assert.equal(await judging.locator(".chip").innerText(), "Off");
  assert.equal(await page.getByTestId("judge-today").innerText(), "AIQ analyzes no new task while this is off. Analyzed today: 3 tasks, 6 calls, $0.04 reported by Claude Code.");
  // axe's target-size rule counts a control scrolled under the sticky index as too small, so axe reads the page
  // from the top, where nothing sits under the index (CI run 41892919844 caught the 'Off' radio under it).
  await page.evaluate(() => document.querySelector("main").scrollTo({ top: 0, behavior: "instant" }));
  assert.deepEqual(await axe(page), []);
  if (shots) await page.screenshot({ path: join(shots, "settings-index.png") });
  await page.context().close();
  // Switched off this morning, after judging 3 tasks: the counts say they came before.
  const off = await open("full", {
    hash: "#settings",
    edit: (s) => {
      s.settings.judge = "off";
      s.settings.judge_since_ms = s.now_ms - 3_600_000;
      s.home.judge_today = { tasks: 3, calls: 6, failed_calls: 0, cost_usd: null };
    },
  });
  assert.match(await off.locator("section[aria-labelledby=st3] .chip").innerText(), /^Off since \d\d:\d\d$/);
  assert.equal(await off.getByTestId("judge-today").innerText(), "AIQ analyzes no new task while this is off. Earlier today, before it was off: 3 tasks, 6 calls.");
  await off.getByRole("switch", { name: "Analyze finished tasks in the background" }).click();
  await off.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Analysis is on.");
  assert.match(await off.getByTestId("judge-today").innerText(), /^Up to 500 tasks a day\. Today: 3 tasks, 6 calls\.$/);
  await off.context().close();
  for (const theme of ["dark", "contrast"]) {
    const p = await open("full", { hash: "#settings", theme });
    await p.locator("details.fold > summary").click();
    await p.evaluate(() => document.querySelector("main").scrollTo({ top: 0, behavior: "instant" }));
    assert.deepEqual(await axe(p), [], theme);
    await p.context().close();
  }
});

await test("core failed to start: one short banner with its code on every page; settings disabled", async () => {
  for (const [id] of pages) {
    const page = await open("down", { hash: `#${id}` });
    const alert = page.getByRole("alert").filter({ hasText: "AIQ couldn't start." });
    assert.equal(await alert.count(), 1);
    assert.match(await alert.innerText(), /AIQ couldn't start\. It keeps trying\. \(AIQ-STORE-OPEN\)/);
    assert.equal(await alert.getByRole("button", { name: "Try again" }).count(), 1);
    assert.doesNotMatch(await page.locator("main").innerText(), /isn't running|starts with your next prompt/);
    if (id === "settings") assert.equal(await page.getByRole("button", { name: "5", exact: true }).isDisabled(), true);
    await page.context().close();
  }
});

await test("core starting or busy: no banner, the status line says Starting", async () => {
  for (const state of ["starting", "busy"]) {
    const page = await open("full", { edit: (s) => ((s.status.core = false), (s.status.core_state = state)) });
    assert.equal(await page.getByText("AIQ couldn't start.").count(), 0);
    assert.match(await page.locator(".foot").innerText(), /Starting/);
    await page.context().close();
  }
});

await test("paused: banner with Resume on every page, and in the nav", async () => {
  for (const [id] of pages) {
    const page = await open("paused", { hash: `#${id}` });
    assert.ok((await page.locator("main").innerText()).includes("AIQ is paused."));
    assert.ok((await page.locator("nav").innerText()).includes("Paused"));
    await page.context().close();
  }
});

await test("empty store: teaching empty states, no errors", async () => {
  for (const [id, , expect] of [["home", 0, "AIQ is getting to know your work"], ["tasks", 0, "No tasks yet."], ["tips", 0, "No tips yet."], ["data", 0, "AIQ has no stored history."]]) {
    const page = await open("empty", { hash: `#${id}` });
    assert.ok((await page.locator("main").innerText()).includes(expect), id);
    assert.deepEqual(await axe(page), []);
    if (shots) await page.screenshot({ path: join(shots, `${id}-empty.png`) });
    await page.context().close();
  }
});

await test("Tasks Day view: a row per session, rung named on hover and focus, Enter opens the task, no axe violations", async () => {
  // Three sessions on one day, from the fixture's own tasks moved onto the newest task's day.
  const page = await open("full", {
    hash: "#tasks",
    edit: (s) => {
      const base = Math.max(...s.tasks.map((t) => t.first_ms));
      const day0 = base - (base % 86_400_000);
      s.tasks.forEach((t, i) => {
        const first = day0 + (8 + i) * 3_600_000;
        t.prompt_ms = t.prompt_ms.map((p) => first + (p - t.first_ms));
        t.last_ms = first + (t.last_ms - t.first_ms);
        t.first_ms = first;
      });
    },
  });
  await page.getByRole("button", { name: "Day", exact: true }).click();
  const lanes = page.getByRole("listitem");
  assert.ok((await lanes.count()) >= 3, "one row per session");
  assert.equal(await page.locator("table[aria-label=Tasks]").count(), 0, "the list is replaced by the Day view");
  // Item 5a: one line sums the day; blocks carry their rung number; rows are named by kind.
  assert.match(await page.getByTestId("day-summary").innerText(), /^7 tasks( · \d+ kinds? of task)? · mostly \w+ \(\d\) · \d+ not analyzed yet · .* inside tasks · \d+ sessions?/);
  assert.match(await page.locator(".dv-task:not(.dv-none) .dv-n").first().innerText(), /^[1-5]$/);
  assert.equal(await page.locator(".dv-task.dv-none .dv-n").first().innerText(), "–");
  const names = await page.locator(".dv-name .dv-t b").allInnerTexts();
  assert.ok(names.some((n) => n === "Not analyzed yet"), names.join("|"));
  assert.ok(await page.locator(".dv-break").count() > 0, "gaps of hours shrink to breaks");
  // Row summary on focus: tool, folder, time span, tasks, prompts.
  const name = page.locator(".dv-name").first();
  await name.focus();
  const tip = name.locator(".tip");
  assert.ok(await tip.isVisible(), "row summary shows on focus");
  assert.match(await tip.innerText(), /session in .+\n.*\d+ tasks? · \d+ prompts?/s);
  // A judged block names its rung in words.
  const judged = page.locator(".dv-task:not(.dv-none)").first();
  await judged.focus();
  assert.match(await judged.locator(".tip").innerText(), /\((\d)\)/);
  assert.match((await judged.getAttribute("aria-label")) ?? "", /Open task$/);
  assert.deepEqual(await axe(page), []);
  await page.keyboard.press("Enter");
  await page.waitForSelector("main h1");
  assert.ok(page.url().includes("#task/"), "Enter on a block opens its task");
  assert.deepEqual(page.errors, []);
  await page.context().close();
  // Narrow window: the chart scrolls inside its own box, never the page.
  const narrow = await open("full", { hash: "#tasks", width: 720, height: 520 });
  await narrow.getByRole("button", { name: "Day", exact: true }).click();
  const over = await narrow.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  assert.ok(over <= 0, `Day view overflows the page by ${over}px`);
  await narrow.context().close();
});

for (const theme of ["light", "dark", "contrast"]) {
  await test(`Right now card: the open task's estimate per turn, rung in words, tip at its turn, no axe violations (${theme})`, async () => {
    const page = await open("live", { theme: theme === "light" ? undefined : theme });
    const card = page.getByTestId("right-now");
    const t = await card.innerText();
    assert.match(t, /Right now · Writing a spec or PRD/);
    assert.match(t, /Copilot CLI · since 11:38 · 5 prompts · 22 min · agent working/);
    assert.match(t, /Heading for Directed \(2\)\s*· could reach Delegated \(3\)/);
    assert.match(t, /Agent made the edits itself \(turns 2, 3\)/);
    assert.match(t, /Step by step: 5 prompts/);
    assert.match(t, /Turn 4: no details \(the log couldn't be read\)/);
    assert.match(t, /AIQ tip:.*from turn 3/);
    assert.match(t, /analyzed rung replaces it/);
    const img = await card.getByRole("img").getAttribute("aria-label");
    assert.equal(img, "Estimate after each turn: turn 1: Assisted (1); turn 2: Delegated (3); turn 3: Directed (2); turn 4: Directed (2) (no details)");
    assert.deepEqual(await axe(page), []);
    if (shots) await page.screenshot({ path: join(shots, `right-now-${theme}.png`) });
    if (theme === "light") {
      await card.getByRole("button", { name: "Open task" }).click();
      await page.waitForFunction(() => location.hash.startsWith("#task/"));
    }
    assert.deepEqual(page.errors, []);
    await page.context().close();
  });
}

// Imran 2026-11-08: the card stays when nothing is open, so Home keeps its shape and the person can tell "nothing
// in progress" from "AIQ hears nothing". Gone only when switched off.
for (const theme of ["light", "dark", "contrast"]) {
  await test(`Right now card, nothing open: stays, says so, names each connected tool's last activity, no axe violations (${theme})`, async () => {
    const page = await open("full", {
      theme: theme === "light" ? undefined : theme,
      edit: (s) => {
        s.starting = [];
        s.setup.tools = ["claude-code", "copilot", "vscode"].map((tool) => ({ tool, hooked: true, present: true, install: "" }));
        s.seen = [
          { surface: "copilot-cli", last_ms: s.now_ms - 12 * 60_000 },
          { surface: "claude-code", last_ms: s.now_ms - 26 * 3_600_000 },
        ];
      },
    });
    const card = page.getByTestId("right-now");
    assert.equal(await card.getAttribute("data-state"), "idle");
    const t = await card.innerText();
    assert.match(t, /Right now · Nothing in progress/);
    assert.match(t, /Copilot\s*last activity 12 min ago/);
    assert.match(t, /Claude Code\s*last activity Yesterday/);
    assert.match(t, /VS Code\s*nothing yet/);
    assert.match(await page.getByTestId("right-now-hint").innerText(), /^Using VS Code with an agent right now\? AIQ has had nothing from it lately\./);
    assert.doesNotMatch(t, /Watching|Monitoring/);
    assert.deepEqual(await axe(page), []);
    if (shots) await page.screenshot({ path: join(shots, `right-now-idle-${theme}.png`) });
    if (theme === "light") {
      await page.getByTestId("right-now-hint").getByRole("button", { name: "Check it in Settings" }).click();
      await page.waitForFunction(() => location.hash === "#settings");
    }
    assert.deepEqual(page.errors, []);
    await page.context().close();
  });
}

// Imran 2026-11-08: Claude Code read "last activity Yesterday" while he typed to it all evening, because every turn
// was set aside. Set-aside activity is activity: the card names it and says it isn't counted.
await test("Right now card, nothing open: a tool whose newest activity was set aside says so, no axe violations", async () => {
  const page = await open("full", {
    edit: (s) => {
      s.starting = [];
      s.setup.tools = ["claude-code", "copilot"].map((tool) => ({ tool, hooked: true, present: true, install: "" }));
      s.seen = [
        { surface: "copilot-cli", last_ms: s.now_ms - 12 * 60_000 },
        { surface: "claude-code", last_ms: s.now_ms - 26 * 3_600_000 },
      ];
      s.aside = [{ surface: "claude-code", last_ms: s.now_ms - 2 * 60_000, reason: "projects", left_out_today: 3, counted_today: 0 }];
    },
  });
  const card = page.getByTestId("right-now");
  const t = await card.innerText();
  assert.match(t, /Claude Code\s*last activity 2 min ago · automated, not counted/);
  assert.match(t, /Copilot\s*last activity 12 min ago\n/);
  assert.equal(await page.getByTestId("right-now-hint").count(), 0, "not called silent");
  const hint = page.getByTestId("right-now-aside");
  assert.match(await hint.innerText(), /^Claude Code's newest activity is Claude's own work in a claude.ai project, so AIQ doesn't count it\. Turns you type yourself count\./);
  assert.doesNotMatch(t, /Watching|Monitoring|judge/i);
  assert.deepEqual(await axe(page), []);
  if (shots) await page.screenshot({ path: join(shots, "right-now-aside-light.png") });
  await hint.getByRole("button", { name: "See what's not counted" }).click();
  assert.deepEqual(page.errors, []);
  await page.context().close();
});

// Imran 2026-11-09: 269 Claude Code prompts were left out over five days and nothing said so. When most of a tool's
// prompts today were left out, the card says so in every state, an open task included.
for (const fixture of ["full", "live"]) {
  await test(`Right now card: most of a tool's prompts left out today says so (${fixture}), no axe violations`, async () => {
    const page = await open(fixture, {
      edit: (s) => {
        if (fixture === "full") s.starting = [];
        s.aside = [{ surface: "claude-code", last_ms: s.now_ms - 60_000, reason: "not-typed", left_out_today: 18, counted_today: 2 }];
      },
    });
    const warn = page.getByTestId("right-now-left-out");
    assert.match(
      await warn.innerText(),
      /^Today AIQ left out 18 of 20 Claude Code prompts as automated, not counted: scheduled or started by an app, not typed\.\s*See what's not counted$/,
    );
    assert.equal(await page.getByTestId("right-now-aside").count(), 0, "one line, not two");
    assert.doesNotMatch(await page.getByTestId("right-now").innerText(), /Watching|Monitoring|judge|warning/i);
    assert.deepEqual(await axe(page), []);
    if (shots) await page.screenshot({ path: join(shots, `right-now-left-out-${fixture}.png`) });
    await warn.getByRole("button", { name: "See what's not counted" }).click();
    assert.deepEqual(page.errors, []);
    await page.context().close();
  });
}

await test("Right now card: a task whose first turn is running shows as starting, not as nothing", async () => {
  const page = await open("full");
  const card = page.getByTestId("right-now");
  assert.equal(await card.getAttribute("data-state"), "starting");
  const t = await card.innerText();
  assert.match(t, /Right now · Task starting/);
  assert.match(t, /Copilot CLI · since \d\d:\d\d · 1 prompt · agent working/);
  assert.match(t, /Where it is heading shows after the agent's first reply/);
  await page.context().close();
});

await test("Right now card: paused, not running and no tool connected each say so instead of hiding", async () => {
  const quiet = { starting: [] };
  const paused = await open("full", { edit: (s) => Object.assign(s, quiet, { status: { ...s.status, paused: true } }) });
  assert.match(await paused.getByTestId("right-now").innerText(), /Right now · Paused[\s\S]*AIQ records nothing new while paused/);
  await paused.getByTestId("right-now").getByRole("button", { name: "Resume" }).click();
  assert.ok((await paused.evaluate(() => window.__AIQ_FIXTURE__.calls)).some((c) => JSON.stringify(c).includes("pause")));
  await paused.context().close();
  const down = await open("full", { edit: (s) => Object.assign(s, quiet, { status: { ...s.status, core_state: "failed" } }) });
  assert.match(await down.getByTestId("right-now").innerText(), /Right now · Waiting for AIQ to start/);
  await down.context().close();
  const none = await open("full", { edit: (s) => Object.assign(s, quiet, { setup: { ...s.setup, tools: s.setup.tools.map((x) => ({ ...x, hooked: false })) } }) });
  assert.match(await none.getByTestId("right-now").innerText(), /Right now · No tool connected/);
  await none.context().close();
});

await test("Right now card: gone only when switched off; the switch is in Settings > Tips", async () => {
  const unread = await open("live", {
    edit: (s) => Object.assign(s.live[0], { rung: null, could: null, did_turns: [], steps: [{ turn: 1, at_ms: s.live[0].first_ms, rung: null, recorded: false }], unrecorded: [1], unrecorded_why: ["no_start"] }),
  });
  const ut = await unread.getByTestId("right-now").innerText();
  assert.match(ut, /No estimate yet/);
  assert.match(ut, /Turn 1: no details \(AIQ restarted during the turn\)/);
  assert.equal(await unread.getByTestId("right-now").getByRole("img").count(), 0, "no empty chart");
  await unread.context().close();
  const off = await open("live", { edit: (s) => Object.assign(s, { live: [], starting: [], settings: { ...s.settings, live_trend: false } }) });
  assert.equal(await off.getByTestId("right-now").count(), 0);
  await off.context().close();
  const st = await open("live", { hash: "#settings" });
  const sw = st.getByRole("switch", { name: "Show where an open task is heading" });
  assert.equal(await sw.getAttribute("aria-checked"), "true");
  await sw.click();
  const calls = await st.evaluate(() => window.__AIQ_FIXTURE__.calls);
  assert.ok(calls.some((c) => JSON.stringify(c).includes("live_trend")), JSON.stringify(calls).slice(0, 300));
  await st.context().close();
});

for (const theme of ["light", "dark", "contrast"]) {
  await test(`Right now card, several open tasks: a chip per task, the newest activity's chart, a picked chip stays (${theme})`, async () => {
    const page = await open("live-many", { theme: theme === "light" ? undefined : theme });
    const card = page.getByTestId("right-now");
    assert.match(await card.locator("h2").innerText(), /Right now · 3 open tasks/);
    const tabs = card.getByRole("tab");
    // Chips in start order; each names the kind, project and tool and says the rung in words.
    const names = await tabs.allInnerTexts();
    assert.equal(names.length, 3);
    assert.match(names[0], /Fixing a bug or failing test\s*aiq · VS Code · 4 prompts.*heading for Assisted \(1\), agent working/s);
    assert.match(names[1], /Writing a spec or PRD\s*aiq · Copilot CLI · 5 prompts.*heading for Directed \(2\), agent working/s);
    assert.match(names[2], /Writing tests\s*foresight · Claude Code · 1 prompt.*heading for Delegated \(3\), waiting for you/s);
    // The newest activity is chosen first: the VS Code task.
    assert.equal(await tabs.nth(0).getAttribute("aria-selected"), "true");
    const panel = card.getByRole("tabpanel");
    assert.match(await panel.innerText(), /aiq · VS Code · since 11:20 · 4 prompts · 40 min · agent working/);
    assert.deepEqual(await axe(page), []);
    if (shots) await page.screenshot({ path: join(shots, `right-now-many-${theme}.png`) });
    if (theme === "light") {
      await tabs.nth(2).click();
      assert.equal(await tabs.nth(2).getAttribute("aria-selected"), "true");
      const t = await panel.innerText();
      assert.match(t, /foresight · Claude Code · since 11:48 · 1 prompt · 12 min · waiting for you/);
      assert.match(t, /Heading for Delegated \(3\)/);
      // Arrow keys move between chips (tabs pattern).
      await page.keyboard.press("ArrowRight");
      assert.equal(await tabs.nth(0).getAttribute("aria-selected"), "true", "wraps to the first");
      assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("role")), "tab");
      await page.keyboard.press("ArrowLeft");
      await panel.getByRole("button", { name: "Open task" }).click();
      await page.waitForFunction(() => location.hash.startsWith("#task/"));
    }
    assert.deepEqual(page.errors, []);
    await page.context().close();
  });
}

await test("Right now card, several open tasks: the picked chip stays chosen after a refresh moves the others", async () => {
  const page = await open("live-many");
  const card = page.getByTestId("right-now");
  await card.getByRole("tab").nth(1).click();
  // A refresh where the spec task's agent went quiet and the order of activity changed.
  await page.evaluate(() => {
    const f = window.__AIQ_FIXTURE__;
    f.snapshot = { ...f.snapshot, live: [f.snapshot.live[2], f.snapshot.live[0], f.snapshot.live[1]] };
  });
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await page.waitForTimeout(300);
  const tabs = card.getByRole("tab");
  assert.equal(await tabs.nth(1).getAttribute("aria-selected"), "true");
  assert.match(await card.getByRole("tabpanel").innerText(), /Copilot CLI · since 11:38/);
  await page.context().close();
});

await test("Right now card, two chips that would read the same say when each started", async () => {
  const page = await open("live-many", {
    edit: (s) => Object.assign(s.live[2], { kind: s.live[0].kind, folder: s.live[0].folder, surface: s.live[0].surface }),
  });
  const names = await page.getByTestId("right-now").getByRole("tab").allInnerTexts();
  assert.match(names[0], /aiq · VS Code · since 11:20/);
  assert.match(names[2], /aiq · VS Code · since 11:48/);
  assert.match(names[1], /Copilot CLI · 5 prompts/, "a chip with no twin keeps its prompt count");
  await page.context().close();
});

// The core sends the pack's text, written to follow "AIQ tip:" in a terminal, so it starts lower case.
await test("Right now card: the tip starts with a capital, as the pack's lower-case text is shown alone", async () => {
  const page = await open("live", {
    edit: (s) => [s.live].flat().forEach((l) => l && l.tip && (l.tip.try_this = "end your request with: run the tests and show me what passed and failed.")),
  });
  const t = await page.getByTestId("right-now").innerText();
  assert.match(t, /AIQ tip:\s*End your request with: run the tests/);
  await page.context().close();
});

await test("Right now card at 720 px: no page scroll, with one task or several", async () => {
  for (const f of ["live", "live-many"]) {
    const page = await open(f, { width: 720, height: 520 });
    const w = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    assert.ok(w <= 0, `${f}: page scrolls by ${w}px`);
    await page.context().close();
  }
});

await test("rung guide: steps by kind of work, where you are, your next step open, copy a starter", async () => {
  const page = await open("progress", { hash: "#guide" });
  assert.equal(await page.locator("main h1").innerText(), "Rung guide");
  const grid = page.getByRole("table", { name: "Habits by rung step and kind of work" });
  assert.equal(await grid.getByRole("row").count(), 5, "a header and four steps");
  // Every pack move shows in each cell its job fits ("any" moves in all four).
  const s = JSON.parse(readFileSync(join(fixtures, "progress.json"), "utf8"));
  const cells = s.guide.reduce((n, m) => n + (m.job === "any" ? 4 : 1), 0);
  assert.equal(await grid.locator(".gd-pill").count(), cells);
  // Each job's cell at its most common rung says so; the first job with a coached next step opens first.
  const head = await grid.getByRole("columnheader", { name: /Write a spec or doc/ }).innerText();
  assert.match(head, /Mostly Directed \(2\)/);
  assert.equal(await grid.locator(".gd-cell.here").count(), 2, "one per job with analyzed kinds");
  const side = page.getByRole("complementary", { name: "The habit you picked" });
  assert.match(await side.locator("h2").innerText(), /Keep an instructions file in the repo and point to it/);
  assert.match(await side.innerText(), /Code a change · Delegated \(3\) → Orchestrated \(4\)/);
  assert.match(await side.innerText(), /How AIQ notices it: Counts when you give the agent a written brief, spec or instructions file\./);
  // Writing tests took up this habit (seeded growing), so its next step moved on; adding a feature still needs it.
  assert.match(await side.innerText(), /Your next step for adding a feature/);
  await grid.getByRole("button", { name: /Ask for the whole draft and list what it must cover/ }).click();
  assert.match(await side.innerText(), /Your next step for writing a spec or PRD/);
  // Pick another habit: the side panel follows; the starter copies.
  await grid.getByRole("button", { name: "Send all your changes in one message, then review the draft" }).click();
  assert.match(await side.locator("h2").innerText(), /Send all your changes in one message/);
  assert.match(await side.innerText(), /Write a spec or doc · Directed \(2\) → Delegated \(3\)/);
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await side.getByRole("button", { name: "Copy starter" }).click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.startsWith("Starter copied"));
  assert.deepEqual(await axe(page), []);
  assert.deepEqual(page.errors, []);
  await page.context().close();
});

await test("narrow window (720x520) has no horizontal page scroll", async () => {
  for (const [id] of pages) {
    const page = await open("full", { hash: `#${id}`, width: 720, height: 520 });
    const over = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    assert.ok(over <= 0, `${id} overflows by ${over}px`);
    await page.context().close();
  }
});

// ---- Deferred pages (design aiq-home-deferred-pages) ----

const TASK_TITLE = "refactor the login handler to return typed errors";

await test("task detail: Enter on a Tasks row opens it; prompts in order; judgment as plain text; back", async () => {
  const page = await open("full", { hash: "#tasks" });
  await page.locator("tbody tr").first().focus();
  await page.keyboard.press("j");
  await page.keyboard.press("Enter");
  await page.waitForFunction((t) => document.querySelector("main h1")?.textContent === t, TASK_TITLE);
  assert.equal(await page.evaluate(() => document.activeElement?.tagName), "H1", "focus moves to the title");
  assert.ok(page.url().endsWith("#task/019000000001-0000-7000-8000-000000000000"), page.url());
  const t = await page.locator("main").innerText();
  assert.match(t, /Claude Code · started 2 days ago|Claude Code · started/);
  assert.match(t, /3 prompts/);
  assert.match(t, /Rung: Directed \(2\)\s+Job: Code a change/);
  assert.ok(t.includes("You checked the diff before keeping it <b>bold</b>."), "model text renders as text");
  assert.equal(await page.locator("ul.evidence b").count(), 0);
  assert.match(t, /Next step \(Directed \(2\) → Delegated \(3\)\): Hand the agent the whole change/);
  assert.match(t, /by your Claude Code/);
  const items = page.locator("ol.prompts li");
  assert.equal(await items.count(), 3);
  assert.ok((await items.nth(2).innerText()).includes("please refactor the login handler, typed errors returned"));
  assert.equal(await page.locator("nav a[aria-current=page]").innerText().then((x) => x.split("\n")[0]), "Tasks");
  assert.deepEqual(page.errors, []);
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("main h1").textContent(), "Tasks", "Esc goes back");
  // A click on a row opens it too, and the link goes back.
  await page.locator("tbody tr").nth(1).click();
  await page.waitForFunction((t) => document.querySelector("main h1")?.textContent === t, TASK_TITLE);
  await page.getByRole("link", { name: "← Tasks" }).click();
  assert.equal(await page.locator("main h1").textContent(), "Tasks");
  await page.context().close();
});

for (const theme of ["light", "dark", "contrast"]) {
  await test(`task detail has no axe violations (${theme})`, async () => {
    const page = await open("full", { theme: theme === "light" ? undefined : theme, hash: "#task/019000000001-0000-7000-8000-000000000000" });
    await page.waitForSelector("ol.prompts");
    assert.deepEqual(await axe(page), []);
    if (shots) await page.screenshot({ path: join(shots, `task-${theme}.png`), fullPage: true });
    await page.context().close();
  });
}

await test("task detail: a task no longer listed says so, with a way back", async () => {
  const page = await open("full", { hash: "#task/nope" });
  await page.waitForSelector("text=This task isn't listed any more.");
  assert.deepEqual(await axe(page), []);
  await page.getByRole("link", { name: "← Tasks" }).click();
  assert.equal(await page.locator("main h1").textContent(), "Tasks");
  await page.context().close();
});

await test("setup: first run walks five steps with Back; Start only after I understand; then Home", async () => {
  const page = await open("first-run");
  const h1 = () => page.locator("main h1").textContent();
  const current = () => page.locator(".steps li[aria-current=step]").innerText();
  assert.equal(await h1(), "Your coach, on your computer only");
  assert.equal(await page.locator("nav").count(), 0, "no nav during first-run setup");
  assert.match(await current(), /Privacy/);
  assert.ok((await page.locator("main").innerText()).includes("No manager or team view exists."));
  assert.deepEqual(await axe(page), []);
  const cont = page.getByRole("button", { name: "Continue" });
  await cont.click();
  assert.equal(await h1(), "Tools AIQ connects to");
  assert.equal(await page.evaluate(() => document.activeElement?.tagName), "H1", "focus moves to the step's heading");
  assert.match(await page.locator(".steps li.done").innerText(), /Privacy\s*\(done\)/);
  // First run connects the tool it found by itself, and says how to undo it.
  await page.waitForFunction(() => document.querySelector("main")?.innerText.includes("GitHub Copilot · connected just now"));
  let t = await page.locator("main").innerText();
  assert.match(t, /Claude Code · connected\n/);
  assert.ok(t.includes("To undo, use Disconnect in Settings."));
  assert.deepEqual(await page.getByRole("list", { name: "Files AIQ changed" }).locator("li").allInnerTexts(), ["C:\\Users\\you\\.copilot\\hooks\\aiq.json"]);
  assert.ok(!t.includes("aiq uninstall claude-code"), "only the tool connected just now shows the undo");
  assert.equal(await page.getByRole("button", { name: /^Connect/ }).count(), 0);
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Connected GitHub Copilot.");
  assert.deepEqual(await axe(page), []);
  await page.getByRole("button", { name: "Back" }).click();
  assert.equal(await h1(), "Your coach, on your computer only");
  await cont.click();
  await cont.click();
  assert.equal(await h1(), "How AIQ will analyze your tasks");
  t = await page.locator("main").innerText();
  assert.ok(!t.includes("later version"), "the rung model is here, not coming");
  assert.ok(t.includes("Analysis is on. AIQ turned it on when it connected your tools"), t);
  assert.ok(t.includes("Up to 500 tasks a day"), t);
  assert.ok(t.includes("AIQ never reads or stores your sign-in."));
  const jsw = page.getByRole("switch", { name: "Analyze finished tasks in the background" });
  assert.equal(await jsw.getAttribute("aria-checked"), "true");
  await jsw.click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Analysis is off. AIQ analyzes no new tasks.");
  assert.equal(await jsw.getAttribute("aria-checked"), "false");
  assert.ok((await page.locator("main").innerText()).includes("Analysis is off. Without it AIQ can't tell the rung"));
  await jsw.click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Analysis is on.");
  assert.deepEqual(await axe(page), []);
  assert.ok(t.includes("On a work account, your organisation's policies apply"));
  await cont.click();
  assert.equal(await h1(), "How many tips a day?");
  await page.getByRole("button", { name: "3", exact: true }).click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Tips a day set to 3.");
  assert.deepEqual(await axe(page), []);
  await cont.click();
  assert.equal(await h1(), "One thing to know before you start");
  const start = page.getByRole("button", { name: "Start AIQ" });
  assert.equal(await start.isDisabled(), true);
  await page.getByLabel("I understand").check();
  assert.equal(await start.isDisabled(), false);
  assert.deepEqual(await axe(page), []);
  if (shots) await page.screenshot({ path: join(shots, "setup-last.png"), fullPage: true });
  await start.click();
  // The empty store's Home is its teaching state.
  await page.waitForFunction(() => document.querySelector("main h1")?.textContent === "AIQ is getting to know your work");
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Setup done. AIQ is on.");
  assert.equal(await page.locator("nav").count(), 1);
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => !String(c.args?.action ?? "").startsWith("check_")).map((c) => c.cmd).filter((c) => c !== "snapshot"));
  assert.deepEqual(calls, ["connect_tools", "set_setting", "set_setting", "set_setting", "finish_setup"], "connected once, not again on Back");
  await page.context().close();
});

for (const theme of ["dark", "contrast"]) {
  await test(`setup has no axe violations (${theme})`, async () => {
    const page = await open("first-run", { theme });
    for (let i = 0; i < 5; i++) {
      assert.deepEqual(await axe(page), [], `step ${i + 1}`);
      if (i < 4) await page.getByRole("button", { name: "Continue" }).click();
    }
    await page.context().close();
  });
}

await test("setup: core down keeps the default tips a day and says so; keys don't leave setup", async () => {
  const page = await open("first-run", { edit: (s) => ((s.status.core = false), (s.status.core_state = "starting")) });
  await page.keyboard.press("g");
  await page.keyboard.press("d");
  await page.keyboard.press("Control+k");
  assert.equal(await page.locator("main h1").textContent(), "Your coach, on your computer only");
  assert.equal(await page.locator("dialog[open]").count(), 0);
  for (let i = 0; i < 3; i++) await page.getByRole("button", { name: "Continue" }).click();
  assert.ok((await page.locator("main").innerText()).includes("AIQ is starting, so this stays at 5 for now."));
  assert.equal(await page.getByRole("button", { name: "5 (default)", exact: true }).isDisabled(), true);
  assert.deepEqual(await axe(page), []);
  await page.context().close();
});

await test("setup: connecting fails, says why and offers Connect again", async () => {
  const page = await open("first-run", { fail: ["connect_tools"] });
  await page.getByRole("button", { name: "Continue" }).click();
  await page.waitForSelector("[role=alert]");
  const t = await page.locator("main").innerText();
  assert.ok(t.includes("AIQ couldn't connect your tools."), t);
  assert.match(t, /GitHub Copilot · found, not connected yet/);
  assert.ok(t.includes("aiq install copilot"));
  assert.equal(await page.getByRole("button", { name: "Connect GitHub Copilot" }).count(), 1);
  assert.deepEqual(await axe(page), []);
  await page.context().close();
});

// Reported 2026-11-05: setup said "Claude Code · connected" on a computer without Claude Code, because 0.1.4
// counted a bare ~/.claude folder and wrote its hooks there. A tool that isn't installed is "not installed" in
// setup, Settings and the side panel, even when an earlier AIQ left its hooks in that tool's settings.
await test("setup: a tool that isn't installed says so, even with AIQ's hooks in its settings", async () => {
  const page = await open("full", {
    hash: "#settings",
    edit: (s) => {
      Object.assign(s.setup.tools.find((x) => x.tool === "claude-code"), { hooked: true, present: false });
      Object.assign(s.setup.tools.find((x) => x.tool === "copilot"), { hooked: true, present: true });
    },
  });
  const settings = await page.locator("main").innerText();
  assert.match(settings, /Claude Code · not installed, AIQ's hooks still in its settings/);
  assert.equal(await page.getByRole("button", { name: "Disconnect Claude Code" }).count(), 1, "the hooks can still be removed");
  const foot = await page.locator("nav .foot").innerText();
  assert.ok(!foot.includes("Claude Code"), foot);
  assert.ok(foot.includes("Connected: Copilot"), foot);
  await page.getByRole("button", { name: "Run setup again" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  const t = await page.locator("main").innerText();
  assert.match(t, /Claude Code · not installed/);
  assert.ok(!/Claude Code · connected/.test(t), t);
  assert.ok(t.includes("They do nothing until you install it."), t);
  if (shots) await page.screenshot({ path: join(shots, "setup-hooks-left-not-installed.png") });
  assert.deepEqual(await axe(page), []);
  await page.context().close();
});

await test("setup: a tool never installed says AIQ connects it once it is", async () => {
  const page = await open("first-run", { edit: (s) => Object.assign(s.setup.tools.find((x) => x.tool === "claude-code"), { hooked: false, present: false }) });
  await page.getByRole("button", { name: "Continue" }).click();
  await page.waitForFunction(() => document.querySelector("main")?.innerText.includes("GitHub Copilot · connected just now"));
  const t = await page.locator("main").innerText();
  assert.match(t, /Claude Code · not installed/);
  assert.ok(t.includes("If you install it later, AIQ connects it the next time AIQ starts."), t);
  if (shots) await page.screenshot({ path: join(shots, "setup-not-installed.png") });
  await page.context().close();
});

// VS Code found: setup adds AIQ's extension with the other tools, says where its tips show (Agent mode, target
// Copilot: the "Local" target sends AIQ nothing), and Settings can remove it.
await test("setup: VS Code gets AIQ's extension, says where tips show, and Settings can remove it", async () => {
  const vscode = { tool: "vscode", hooked: false, present: true, install: "aiq install vscode" };
  const page = await open("first-run", { edit: (s) => s.setup.tools.push({ ...vscode }) });
  await page.getByRole("button", { name: "Continue" }).click();
  await page.waitForFunction(() => document.querySelector("main")?.innerText.includes("VS Code · connected just now"));
  const t = await page.locator("main").innerText();
  assert.ok(t.includes("Tips show in VS Code after a Copilot agent turn, when Copilot Chat is in Agent mode with the session target set to Copilot."), t);
  assert.deepEqual(await axe(page), []);
  await page.context().close();

  const s2 = await open("full", { hash: "#settings", edit: (s) => s.setup.tools.push({ ...vscode, hooked: true }) });
  assert.match(await s2.locator("main").innerText(), /VS Code · connected/);
  assert.ok((await s2.locator("nav .foot").innerText()).includes("VS Code"));
  await s2.getByRole("button", { name: "Disconnect VS Code" }).click();
  await s2.waitForFunction(() => document.querySelector("main")?.innerText.includes("VS Code · not connected"));
  assert.ok((await s2.locator("main").innerText()).includes("extensions.json"));
  await s2.context().close();

  // Before connecting, the offer names VS Code's own command, not hooks or a backup.
  const s3 = await open("full", { hash: "#settings", edit: (s) => s.setup.tools.push({ ...vscode }) });
  await s3.getByRole("button", { name: "Run setup again" }).click();
  await s3.getByRole("button", { name: "Continue" }).click();
  const offer = await s3.locator("main").innerText();
  assert.ok(offer.includes("AIQ adds its extension with VS Code's own command. Or run aiq install vscode"), offer);
  await s3.context().close();
});

await test("setup: run again never reconnects by itself; Connect does", async () => {
  const page = await open("full", { hash: "#settings", edit: (s) => (s.setup.tools.find((x) => x.tool === "copilot").hooked = false) });
  await page.getByRole("button", { name: "Run setup again" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  assert.match(await page.locator("main").innerText(), /GitHub Copilot · found, not connected yet/);
  const calls = () => page.evaluate(() => window.__AIQ_FIXTURE__.calls.map((c) => c.cmd).filter((c) => c === "connect_tools"));
  assert.deepEqual(await calls(), []);
  await page.getByRole("button", { name: "Connect GitHub Copilot" }).click();
  await page.waitForFunction(() => document.querySelector("main")?.innerText.includes("GitHub Copilot · connected just now"));
  assert.deepEqual(await calls(), ["connect_tools"]);
  await page.context().close();
});

await test("setup: Run setup again from Settings, and Close returns", async () => {
  const page = await open("full", { hash: "#settings" });
  await page.getByRole("button", { name: "Run setup again" }).click();
  assert.equal(await page.locator("main h1").textContent(), "Your coach, on your computer only");
  await page.getByRole("button", { name: "Close" }).click();
  assert.equal(await page.locator("main h1").textContent(), "Settings");
  await page.context().close();
});

await test("narrow window (720x520): setup and task detail have no horizontal page scroll", async () => {
  for (const [f, hash] of [["first-run", ""], ["full", "#task/019000000001-0000-7000-8000-000000000000"]]) {
    const page = await open(f, { hash, width: 720, height: 520 });
    const over = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    assert.ok(over <= 0, `${f} overflows by ${over}px`);
    await page.context().close();
  }
});

await test("delete one prompt: names it, focus on Cancel, deleted text everywhere, focus kept on the table", async () => {
  const page = await open("full", { hash: "#data" });
  const firstDelete = page.locator("tbody").getByRole("button", { name: /^Delete prompt from/ }).first();
  const label = await firstDelete.getAttribute("aria-label");
  await firstDelete.click();
  await page.waitForSelector("dialog[open]");
  assert.equal(await page.evaluate(() => document.activeElement?.textContent), "Cancel");
  const dlg = await page.locator("dialog[open]").innerText();
  assert.ok(dlg.includes("draft the release notes"), dlg);
  assert.ok(dlg.includes("It keeps only that a prompt was sent"));
  assert.deepEqual(await axe(page), []);
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("dialog[open]").count(), 0);
  assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("aria-label")), label, "focus returns to Delete");
  await firstDelete.click();
  await page.locator("dialog").getByRole("button", { name: "Delete prompt" }).click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.startsWith("Prompt deleted."));
  assert.equal(await page.locator("dialog[open]").count(), 0);
  assert.ok(!(await page.locator("tbody").innerText()).includes("draft the release notes"));
  await page.waitForFunction(() => document.activeElement?.tagName === "TABLE");
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "forget_item").map((c) => Object.keys(c.args)));
  assert.deepEqual(calls, [["eventId"]]);
  await page.context().close();
});

await test("coaching Home: finished and could be, the rule, uncertain left out, top kinds with a next step", async () => {
  const page = await open("coaching");
  const t = await page.locator("main").innerText();
  assert.match(t, /Tasks agents finished\s+33%\s+could be 67%/);
  assert.equal(await page.getByRole("img", { name: "Agents finished 33%, could be 67%" }).count(), 1);
  // Outcome first, then the measured counts on one line; how it is counted stays folded away until asked for.
  assert.match(t, /47% have an easy next step, most often to Delegated \(3\)\.\s+5 of 15 analyzed tasks finished by agents · 5 could be · 5 need you · 1 unsure, not counted/);
  assert.ok(!t.includes("A task counts as finished"), "the rule is folded away");
  await page.getByText("How AIQ counts").click();
  const how = await page.locator("details.how").innerText();
  assert.match(how, /A task counts as finished when it was analyzed as Delegated or higher and you sent no re-prompt correcting the agent\./);
  assert.match(how, /"Could be" adds the tasks of kinds whose next step would let agents finish them\./);
  assert.match(how, /Analyzed by your Copilot CLI/);
  // The weekly chart says what its two shades mean (dark = agents finished, light = the rest).
  assert.match(t, /Tasks each week\s+Finished by agents\s+Other tasks/);
  const rows = page.getByRole("list", { name: "Kinds of task with a next step" }).getByRole("listitem");
  assert.equal(await rows.count(), 2);
  const first = await rows.first().innerText();
  assert.match(first, /Writing tests\s+Keep an instructions file in the repo and point to it\s+→ Orchestrated \(4\)/);
  // The rung mix: 15 judged tasks, so shares show; the kinds on the highest rung reached are listed.
  const mix = await page.locator("section", { has: page.locator("h2#rg") }).innerText();
  assert.match(mix, /Delegated\s+40%\s+6 tasks/);
  // Shades alone can't tell five rungs apart: every swatch and bar segment also carries the rung number.
  assert.deepEqual(await page.locator(".mix-r i").allInnerTexts(), ["1", "2", "3", "4", "5"]);
  assert.deepEqual(await page.locator(".rmix span").allInnerTexts(), ["1", "2", "3"]);
  assert.match(await page.locator(".mix-kinds").innerText(), /On Delegated:\s+Writing tests · 6/);
  assert.ok(!(await page.locator("main").innerHTML()).includes("Give the agent <b>"), "analyze text is not on Home");
  // A week with fewer than 3 judged tasks says so instead of a share.
  assert.ok((await page.locator(".wk-col").first().getAttribute("aria-label")).includes("too few analyzed tasks to say"));
  await rows.first().getByRole("button").click();
  await page.waitForSelector("main h1:text('Writing tests')");
  assert.ok(page.url().endsWith("#kind/code_change/tests"));
  await page.context().close();
});

await test("progress Home: kinds that moved up or down beyond chance, counts behind them, too few said plainly, the habits line, no axe violations", async () => {
  for (const theme of ["light", "dark", "contrast"]) {
    const page = await open("progress", { theme: theme === "light" ? undefined : theme });
    const sec = page.locator("section", { has: page.locator("h2#pg") });
    const t = await sec.innerText();
    assert.match(t, /Is each kind of task moving up\?\s+1\s+of 4 kinds moved up a rung · 1 at a lower rung/);
    assert.match(t, /Last 30 days\s+The 30 before, when it moved\s+Where the next step gets you/);
    const rows = page.getByRole("list", { name: "Rung progress by kind of task" }).getByRole("button");
    assert.equal(await rows.count(), 4, "too-few kinds are one line, not rows");
    // Movers first in their pack; the row's name carries what the dots show.
    assert.equal(await rows.nth(0).getAttribute("aria-label"), "Writing tests, Delegated (3) in the last 30 days, moved up from Directed (2), next step reaches Orchestrated (4), 5 tasks in the last 30 days, 5 before");
    assert.match(await rows.nth(0).innerText(), /Writing tests\s+↑ up/);
    // Its typical rung went from Assisted to Delegated, but only by chance: drawn where it is, no ring, no arrow.
    assert.equal(await rows.nth(1).getAttribute("aria-label"), "Adding a feature, Delegated (3) in the last 30 days, same rung, next step reaches Orchestrated (4), 3 tasks in the last 30 days, 3 before");
    assert.equal(await rows.nth(2).getAttribute("aria-label"), "Fixing a bug or failing test, Assisted (1) in the last 30 days, same rung, 3 tasks in the last 30 days, 3 before");
    assert.match(await rows.nth(3).innerText(), /Writing a spec or PRD\s+↓ down/);
    // Habit changes are one line under the map; the habits themselves live in Habits and tips.
    assert.match(t, /1 habit is growing: the steps that move a kind of task up\. See them in Habits and tips/);
    assert.match(t, /1 more kind of task: too few to say if it moved\. Each needs at least 3 analyzed tasks in both halves\. Show/);
    await page.getByRole("button", { name: "Show" }).click();
    assert.equal(await page.getByRole("list", { name: "Kinds of task with too few tasks to say" }).getByRole("button").count(), 1);
    const v = await axe(page);
    assert.deepEqual(v, [], v.join("; "));
    assert.deepEqual(page.errors, []);
    if (shots) await page.screenshot({ path: join(shots, `progress-home-${theme}.png`), fullPage: true });
    if (theme === "light") {
      await rows.first().click();
      await page.waitForSelector("main h1:text('Writing tests')");
      assert.ok(page.url().endsWith("#kind/code_change/tests"));
    }
    await page.context().close();
  }
  // Nothing moved yet, and no kinds: the empty wording, never a guessed trend.
  const same = await open("coaching");
  // Nothing has a reading in both halves: no headline number, kinds with a next step still on the ladder.
  const st = await same.locator("section", { has: same.locator("h2#pg") }).innerText();
  assert.doesNotMatch(st, /moved up a rung/);
  assert.match(st, /2 more kinds of task: too few to say if they moved/);
  await same.context().close();
  const none = await open("coaching", { edit: (s) => ((s.coaching.progress = []), (s.coaching.kinds = [])) });
  assert.match(await none.locator("main").innerText(), /Progress shows once AIQ knows the kind of your tasks from the last 60 days\./);
  await none.context().close();
});

await test("kind of task: the pack's step, copy starter, analyze words as text, This doesn't fit with a reason, Undo", async () => {
  const page = await open("coaching", { hash: "#kind/code_change/tests" });
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"], { origin: base });
  const t = await page.locator("main").innerText();
  assert.match(t, /Next step: Keep an instructions file in the repo and point to it/);
  assert.match(t, /1 task AIQ isn't sure about is left out of this\./);
  assert.match(t, /For your last task\s+Give the agent <b>a brief<\/b>\./, "model text rendered literally");
  assert.equal(await page.locator("main b:text('a brief')").count(), 0);
  const starter = await page.locator("pre.starter").innerText();
  assert.ok(starter.length > 0 && /</.test(starter), "starter has placeholders");
  await page.getByRole("button", { name: "Copy starter" }).click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.startsWith("Starter copied."));
  await page.getByRole("button", { name: "This doesn't fit" }).click();
  const stop = page.getByRole("button", { name: "Stop suggesting it" });
  assert.equal(await stop.isDisabled(), true, "a reason is required");
  await page.getByLabel("The step isn't useful for this work").check();
  await stop.click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.startsWith("AIQ won't suggest this step for Writing tests"));
  await page.waitForSelector(`main h2:text("You said this step doesn't fit")`);
  await page.getByRole("button", { name: "Undo" }).click();
  await page.waitForSelector("main h2:text('Next step: Keep an instructions file in the repo and point to it')");
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "guidance_flag").map((c) => c.args));
  assert.deepEqual(calls, [
    { job: "code_change", taskType: "tests", moveId: "written_brief", action: "flag", reason: "not_useful" },
    { job: "code_change", taskType: "tests", moveId: "written_brief", action: "undo", reason: null },
  ]);
  // Other states say why there is no step; an unknown kind has a way back.
  const fix = await open("coaching", { hash: "#kind/code_change/fix_bug" });
  assert.match(await fix.locator("main").innerText(), /This is the right rung for this task\./);
  assert.equal(await fix.getByRole("button", { name: "This doesn't fit" }).count(), 0);
  await fix.context().close();
  const few = await open("coaching", { hash: "#kind/code_change/refactor" });
  assert.match(await few.locator("main").innerText(), /AIQ isn't sure yet/);
  await few.context().close();
  const none = await open("coaching", { hash: "#kind/code_change/deps" });
  assert.match(await none.locator("main").innerText(), /Kind of task not found[\s\S]*Back to Tasks/);
  await none.context().close();
  await page.context().close();
});

await test("Tasks opens on kinds of task: rung today, could be, a week, the 30-day move; a row opens the kind (item 4)", async () => {
  const page = await open("progress", { hash: "#tasks" });
  assert.equal(await page.getByRole("button", { name: "Kinds", exact: true }).getAttribute("aria-pressed"), "true", "Kinds is the default");
  const table = page.getByRole("table", { name: "Kinds of task" });
  const rows = table.locator("tbody tr");
  assert.equal(await rows.count(), 5);
  const t = await table.innerText();
  assert.match(t, /Writing tests[\s\S]*Code a change[\s\S]*↑ up from Directed \(2\)/);
  assert.match(t, /Writing a spec or PRD[\s\S]*↓ down from Delegated \(3\)/);
  assert.match(t, /Fixing a bug or failing test[\s\S]*– same rung/);
  assert.match(t, /Refactoring or cleaning up[\s\S]*Too few tasks to say/);
  assert.match(await page.locator("main header.head").innerText(), /Kinds\s+All tasks\s+Day/);
  assert.equal(await page.locator("table[aria-label=Tasks]").count(), 0, "the task list waits under All tasks");
  assert.deepEqual(await axe(page), []);
  await rows.first().click();
  await page.waitForFunction(() => location.hash.startsWith("#kind/"));
  await page.context().close();

  const c = await open("coaching", { hash: "#tasks" });
  const ct = await c.getByRole("table", { name: "Kinds of task" }).innerText();
  for (const k of ["Writing tests", "Writing a spec or PRD", "Fixing a bug or failing test", "Right rung", "Refactoring or cleaning up", "Too few tasks yet"]) assert.ok(ct.includes(k), k);
  assert.equal(await c.getByRole("list", { name: "Kinds of task" }).count(), 0, "no second kinds list below");
  // The chosen view sits directly under the switch.
  const afterHeader = () => c.evaluate(() => {
    const n = document.querySelector("main header.head")?.nextElementSibling;
    return n ? (n.matches(".tablewrap") ? n.querySelector("table")?.getAttribute("aria-label") : n.querySelector("#dayhd") ? "day" : n.outerHTML.slice(0, 80)) : "none";
  });
  assert.equal(await afterHeader(), "Kinds of task");
  await c.getByRole("button", { name: "All tasks", exact: true }).click();
  assert.equal(await afterHeader(), "Tasks");
  assert.match(await c.locator("tbody").innerText(), /Code a change · Writing tests/);
  await c.getByRole("button", { name: "Day", exact: true }).click();
  assert.equal(await afterHeader(), "day");
  assert.ok((await c.locator("#dayhd").boundingBox()).y < 800, "Day view starts on screen");
  await c.context().close();

  // No kinds yet: no Kinds button, the list first.
  const f = await open("full", { hash: "#tasks" });
  assert.equal(await f.getByRole("button", { name: "Kinds", exact: true }).count(), 0);
  assert.equal(await f.locator("table[aria-label=Tasks]").count(), 1);
  await f.context().close();
});

await test("Habits grouped by the rung they help you reach, with an adopted bar; a habit two jobs share is one line; tips name their rung and fold with what happened after (item 6, option B)", async () => {
  const page = await open("coaching", { hash: "#tips" });
  const t = await page.locator("main").innerText();
  // One section per rung a habit helps you reach, lowest first, each with what the rung means and how many are adopted.
  const heads = await page.locator("main h2").allInnerTexts();
  assert.deepEqual(heads.map((h) => h.replace(/\s+/g, " ").trim()), ["2 Directed habits 4", "3 Delegated habits 5", "4 Orchestrated habits 4", "Tip history"]);
  assert.match(t, /Directed habits[\s\S]*?4 of 4 adopted\s+You give the agent a clear goal/);
  assert.match(t, /Orchestrated habits[\s\S]*?0 of 4 adopted/);
  const adopted = await page.getByRole("list", { name: "Delegated habits" }).innerText();
  assert.match(adopted, /Hand over the whole task and list what done looks like\s+Code a change · Adopted[\s\S]*?Seen in 6 of 6 tasks/);
  const notYet = await page.getByRole("list", { name: "Orchestrated habits" }).innerText();
  assert.match(notYet, /Keep an instructions file in the repo and point to it\s+Code a change, Write a spec or doc · Not yet[\s\S]*?Seen in 0 of 9 tasks/, "one line for both jobs, counts added");
  assert.equal(await page.locator(".banner.info").count(), 0, "no banners repeating the rows");
  assert.match(adopted, /Adopted \w{3} \d+ \w{3}/, "a recent move shows its date on the row");
  // Tip history: what happened after each next-step tip.
  const tips = await page.getByRole("table", { name: "Tip history" }).innerText();
  assert.match(tips, /2\s*Directed\s+This looks like a kind of task you repeat\.\s+Try this: Check the diff or run the tests before using the result\.[\s\S]*Did it in 3 of the next 3 tasks/);
  assert.match(tips, /3\s*Delegated\s+This looks like a kind of task you repeat\.\s+Try this: Ask for the whole draft with what done means\.[\s\S]*Did it in 0 of the next 3 tasks/);
  assert.deepEqual(await axe(page), []);
  // Stop tips like this goes through the core.
  await page.getByRole("button", { name: "Stop tips like this" }).first().click();
  await page.waitForFunction(() => window.__AIQ_FIXTURE__.calls.some((c) => c.cmd === "tip_off"));
  const call = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.find((c) => c.cmd === "tip_off").args);
  assert.deepEqual(call, { tipId: "next_rung.check_code" });
  assert.ok(!t.includes("Includes tasks AIQ imported"), "no imported tasks in this fixture");
  await page.context().close();

  // A tip shown twice is one row; a tip with no habit says so.
  const full = await open("full", { hash: "#tips" });
  const ft = await full.getByRole("table", { name: "Tip history" }).innerText();
  assert.equal(await full.locator("table[aria-label='Tip history'] tbody tr").count(), 1);
  assert.match(ft, /Shown 2 times[\s\S]*×2[\s\S]*You opened it[\s\S]*Copilot CLI, Claude Code[\s\S]*No habit to follow/);
  await full.context().close();

  const imp = await open("coaching", { hash: "#tips", edit: (s) => (s.coaching.habits_imported = true) });
  assert.match(await imp.locator("main").innerText(), /Includes tasks AIQ imported from your history\./);
  await imp.context().close();
  const lapse = await open("coaching", {
    hash: "#tips",
    edit: (s) => {
      s.coaching.habits = [{ ...s.coaching.habits[0], state: "tried", moved_to: "tried", moved_ms: s.now_ms - 86_400_000, moved_back: true, seen: 1, eligible: 4 }];
    },
  });
  const lt = await lapse.getByRole("list", { name: "Directed habits" }).innerText();
  assert.match(lt, /Trying · Back to trying since \w{3} \d+ \w{3}[\s\S]*Seen in 1 of 4 tasks/);
  await lapse.context().close();
  const empty = await open("empty", { hash: "#tips" });
  assert.match(await empty.locator("main").innerText(), /No habits to show yet\./);
  await empty.context().close();
});

await test("Kind page: Moving up? draws each analyzed task's rung, says the 30-day move, names every dot for screen readers (item 9)", async () => {
  const page = await open("progress", { hash: "#kind/code_change/tests" });
  const card = page.locator("section", { has: page.locator("#km") });
  assert.match(await card.innerText(), /Moved up\s*Directed \(2\) → Delegated \(3\), last 30 days against the 30 before\./);
  const label = await page.getByRole("img", { name: /Rung of each analyzed task/ }).getAttribute("aria-label");
  const dots = await page.locator(".km-dot").count();
  assert.equal(label.split(";").length, dots, "every dot is named in the label");
  assert.ok(dots >= 6);
  assert.deepEqual(await axe(page), []);
  await page.context().close();
  const c = await open("coaching", { hash: "#kind/code_change/tests" });
  assert.match(await c.locator("section", { has: c.locator("#km") }).innerText(), /Too few tasks to say/);
  assert.equal(await c.locator(".km-dot.unsure").count(), 1, "the Uncertain task is a hollow dot");
  assert.equal(await c.locator(".km-could").count(), 1, "dashed line where the kind could reach");
  await c.context().close();
});

await test("Settings: next-step tips auto/on/off, AIQ's self-check stop with Turn back on, a switch per tip", async () => {
  const page = await open("coaching", { hash: "#settings" });
  assert.equal(await page.getByLabel(/On, unless AIQ's guesses are often wrong/).isChecked(), true);
  await page.getByLabel("Off", { exact: true }).check();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Next-step tips: off.");
  // Every next-step tip has its own switch, by job; one turned off earlier shows unchecked.
  await page.getByText("Choose which next-step tips AIQ may show").click();
  const tips = page.getByRole("group", { name: "Next-step tips AIQ may show" });
  const checks = tips.getByRole("checkbox");
  assert.equal(await checks.count(), 28);
  assert.equal(await tips.getByRole("group", { name: "Run or ship it" }).getByRole("checkbox").count(), 6);
  const agentChecks = tips.getByLabel("Put the test command in your instructions file");
  assert.equal(await agentChecks.isChecked(), false);
  await agentChecks.check();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.endsWith("tip turned back on."));
  const docBrief = tips.getByRole("group", { name: "Write a spec or doc" }).getByLabel("Name the reader, and give an example doc to copy");
  await docBrief.uncheck();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.endsWith("tip turned off."));
  assert.equal(await docBrief.isChecked(), false);
  assert.equal(await page.getByRole("list", { name: "Tips you turned off" }).count(), 0, "next-step tips are not listed twice");
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => ["set_setting", "tip_on", "tip_off"].includes(c.cmd)).map((c) => [c.cmd, c.args]));
  assert.deepEqual(calls, [
    ["set_setting", { key: "next_rung_tips", value: "off" }],
    ["tip_on", { tipId: "next_rung.agent_checks" }],
    ["tip_off", { tipId: "next_rung.brief_doc" }],
  ]);
  assert.deepEqual(await axe(page), []);
  await page.context().close();
  const stopped = await open("coaching", {
    hash: "#settings",
    edit: (s) => Object.assign(s.settings, { guess_checked: 10, guess_right: 7, guess_auto_off: true, next_rung_on: false }),
  });
  assert.match(await stopped.locator("main").innerText(), /AIQ guessed the kind of task right 7 of 10 times, so it stopped these tips\./);
  await stopped.getByRole("button", { name: "Turn back on" }).first().click();
  await stopped.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Next-step tips are back on.");
  assert.deepEqual(await axe(stopped), []);
  await stopped.context().close();
});

await test("Accuracy pause (K6b): Home and Settings say why, Keep showing and Let AIQ decide go through the core", async () => {
  const paused = (s) => {
    s.settings.accuracy_paused = [
      { scope: "run_ship", label: "Run or ship it", agreed: 6, checked: 10, kept_on: false },
      { scope: "code_change/fix_bug", label: "Code a change: Fixing a bug", agreed: 4, checked: 6, kept_on: true },
    ];
  };
  const home = await open("coaching", { edit: paused });
  const ht = await home.locator("main").innerText();
  assert.match(ht, /Run or ship it: AIQ paused these next-step tips\. A second model agreed with AIQ's analysis only 6 of 10 times, under 8 in 10\./);
  assert.match(ht, /Code a change: Fixing a bug: next-step tips stay on because you chose to keep them/);
  await home.getByRole("button", { name: "Keep showing" }).click();
  await home.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Run or ship it: next-step tips kept on.");
  assert.deepEqual(await axe(home), []);
  await home.context().close();
  const st = await open("coaching", { hash: "#settings", edit: paused });
  await st.getByRole("button", { name: "Let AIQ decide" }).click();
  await st.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Code a change: Fixing a bug: AIQ decides again.");
  const calls = await st.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "set_setting").map((c) => c.args));
  assert.deepEqual(calls, [{ key: "keep_on.code_change/fix_bug", value: "auto" }]);
  assert.deepEqual(await axe(st), []);
  await st.context().close();
  // "Always on" ignores pauses, so nothing is shown; with none paused Settings says the rule exists.
  const on = await open("coaching", { hash: "#settings", edit: (s) => (paused(s), (s.settings.next_rung_tips = "on")) });
  assert.equal(await on.getByRole("button", { name: "Keep showing" }).count(), 0);
  await on.context().close();
  const none = await open("coaching", { hash: "#settings" });
  assert.match(await none.locator("main").innerText(), /AIQ also pauses the tips for a job or kind of task by itself/);
  await none.context().close();
});

await test("auto-update: the titlebar marker, one-press Update now, Go back, the Settings switch (design auto-update U5, U5h)", async () => {
  const none = await open("full");
  assert.equal(await none.getByTestId("update-line").count(), 0, "nothing waiting: no line");
  assert.equal(await none.getByTestId("update-chip").count(), 0, "nothing waiting: no marker");
  // Home asks the shell on open (10-minute bar) and hourly whether a check is due, so a new release shows soon (U5c).
  await none.waitForFunction(() => window.__AIQ_FIXTURE__.calls.some((c) => c.cmd === "run_update" && c.args.action === "check_on_open"));
  await none.context().close();
  const waiting = (s) => Object.assign(s.update, { note: "waiting", available: "v0.2.0", blocked: "AIQ home is open" });
  const upd = (p) => p.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "run_update" && !c.args.action.startsWith("check_")).map((c) => c.args));
  // A waiting update: a marker in the titlebar on every page, not a banner in the page; one press installs.
  for (const hash of ["", "#tasks", "#settings"]) {
    const p = await open("full", { hash, edit: waiting, width: 800 });
    assert.equal(await p.getByTestId("update-chip").innerText(), "Update to 0.2.0", `marker on ${hash || "#home"}, narrow window`);
    assert.equal(await p.getByTestId("update-line").count(), 0, "no Home banner for a waiting update");
    await p.context().close();
  }
  // The same ready update beside the version, bottom left (Imran 2026-11-05, option B): a small box replaces the
  // version line on every page ("AIQ x.y.z is ready", "You have", Update); one press installs. Nothing waiting: the plain link (checked below).
  for (const hash of ["", "#tasks", "#settings"]) {
    const p = await open("full", { hash, edit: waiting });
    const nav = p.locator("nav .foot");
    assert.match(await nav.getByTestId("update-ready").innerText(), /^AIQ 0\.2\.0 is ready\s+You have 0\.1\.0\s+Update\s+Or it installs after 10 min in the background$/, `bottom-left notice on ${hash || "#home"}`);
    assert.equal(await nav.getByRole("button", { name: "Check for updates" }).count(), 0, "the ready button replaces the link");
    await p.context().close();
  }
  const side = await open("full", { edit: waiting });
  await side.locator("nav").getByRole("button", { name: "Update", exact: true }).click();
  await side.waitForFunction(() => window.__AIQ_FIXTURE__.calls.some((c) => c.cmd === "run_update" && c.args.action === "install"));
  assert.deepEqual(await upd(side), [{ action: "install" }]);
  await side.context().close();
  for (const theme of ["light", "dark"]) {
    const p = await open("full", { theme, edit: waiting });
    assert.deepEqual(await axe(p), [], `ready box contrast, ${theme}`);
    if (shots) await p.locator("nav.side").screenshot({ path: join(shots, `update-ready-${theme}.png`) });
    await p.context().close();
  }
  const home = await open("full", { edit: waiting });
  assert.match(await home.getByTestId("update-chip").getAttribute("title"), /installs by itself when AIQ home has been in the background for 10 minutes, or when you close it\. Last try waited: AIQ home is open\./);
  await home.getByTestId("update-chip").click();
  await home.waitForFunction(() => document.querySelector("[data-testid=update-chip]")?.textContent === "Installing AIQ 0.2.0. AIQ home closes and opens again.");
  assert.deepEqual(await upd(home), [{ action: "install" }]);
  assert.deepEqual(await axe(home), []);
  await home.context().close();
  // Away install (design U5n; laptop 2026-11-08: Home open for hours, 0.1.19 to 0.1.21 downloaded, none
  // installed): 10 minutes in the background, Home asks the shell to install; not while the person is in it,
  // and never under Ask first, where the box says nothing about installing by itself.
  const background = (p, out) =>
    p.evaluate((o) => {
      document.hasFocus = () => !o;
      window.dispatchEvent(new Event(o ? "blur" : "focus"));
    }, out);
  const aways = (p) => p.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "run_update" && c.args.action === "install_if_away").length);
  const left = await open("full", { edit: waiting, clock: true });
  await background(left, false);
  await left.clock.runFor(30 * 60_000);
  assert.equal(await aways(left), 0, "in the window: no away install");
  await background(left, true);
  await left.clock.runFor(9 * 60_000);
  assert.equal(await aways(left), 0, "9 minutes away: not yet");
  const minuteAsks = (p) =>
    p.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "run_update" && ["check_if_due", "install_if_away"].includes(c.args.action)).length);
  const beforeDue = await minuteAsks(left);
  await left.clock.runFor(2 * 60_000);
  assert.ok((await aways(left)) >= 1, "10 minutes away: Home asks for the install");
  // One ask a minute (DC-045, laptop 2026-11-09 02:52:28Z): the install replaces that minute's check, never both.
  assert.equal((await minuteAsks(left)) - beforeDue, 2, "two minutes: two update asks, not a check and an install each");
  const asked = await aways(left);
  await background(left, false);
  await left.clock.runFor(30 * 60_000);
  assert.equal(await aways(left), asked, "back in the window: no more asks");
  await left.context().close();
  const ask = await open("full", { edit: (s) => (waiting(s), (s.update.mode = "ask")), clock: true });
  assert.equal(await ask.locator("nav .foot").getByTestId("update-when").count(), 0, "Ask first: no line about installing by itself");
  await background(ask, true);
  await ask.clock.runFor(30 * 60_000);
  assert.equal(await aways(ask), 0, "Ask first: never installs by itself");
  await ask.context().close();
  // A newer release while one waits (laptop 2026-11-05: 0.1.8 waited from 18:58Z; 0.1.9 at 20:32Z never showed):
  // Home's timed check still runs, the newer download replaces the waiting one, and the box names it.
  const newer = await open("full", { edit: (s) => Object.assign(s.update, { note: "waiting", available: "v0.1.8", latest: "v0.1.8", blocked: "AIQ home is open" }) });
  const box = newer.locator("nav .foot").getByTestId("update-ready");
  assert.match(await box.innerText(), /^AIQ 0\.1\.8 is ready/);
  await newer.evaluate(() => Object.assign(window.__AIQ_FIXTURE__, { checkDue: true, checkFinds: "v0.1.9" }));
  await newer.evaluate(() => window.dispatchEvent(new Event("focus")));
  await newer.waitForFunction(() => /^AIQ 0\.1\.9 is ready/.test(document.querySelector("nav .foot [data-testid=update-ready]")?.textContent ?? ""), null, { timeout: 20000 });
  await box.getByRole("button", { name: "Update", exact: true }).click();
  await newer.waitForFunction(() => window.__AIQ_FIXTURE__.calls.some((c) => c.cmd === "run_update" && c.args.action === "install"));
  await newer.context().close();
  // Nothing known yet: Update now checks with Home open, shows each step, and installs once the download lands.
  const found = await open("full", { edit: (s) => (s.update.latest = "v0.1.0") });
  await found.evaluate(() => (window.__AIQ_FIXTURE__.checkFinds = "v0.2.0"));
  await found.getByRole("button", { name: "Check for updates" }).click();
  assert.equal(await found.getByTestId("update-chip").innerText(), "Checking for a newer AIQ…");
  assert.equal(await found.getByRole("button", { name: "Check for updates" }).isDisabled(), true, "a second press while checking does nothing");
  await found.waitForFunction(() => document.querySelector("[data-testid=update-chip]")?.textContent.startsWith("Installing AIQ 0.2.0."), null, { timeout: 8000 });
  assert.deepEqual(await upd(found), [{ action: "check" }, { action: "install" }]);
  await found.context().close();
  // Up to date, and can't reach GitHub: said on screen, nothing installed.
  for (const [finds, text] of [["none", "You have the newest AIQ (0.1.0)."], ["error", "Couldn't reach GitHub. Try again in a few minutes."]]) {
    const p = await open("full", { hash: "#settings" });
    await p.evaluate((f) => (window.__AIQ_FIXTURE__.checkFinds = f), finds);
    await p.locator("main").getByRole("button", { name: "Check for updates" }).click();
    assert.equal(await p.getByTestId("update-progress").innerText(), "Checking for a newer AIQ…", "Settings shows the check at once");
    await p.waitForFunction((t) => document.querySelector("[data-testid=update-progress]")?.textContent === t, text, { timeout: 8000 });
    assert.equal(await p.getByTestId("update-chip").innerText(), text);
    assert.deepEqual(await upd(p), [{ action: "check" }]);
    await p.context().close();
  }
  // From the command palette too (the nav foot is hidden in a narrow window).
  const pal = await open("full", { width: 800, edit: waiting });
  await pal.keyboard.press("Control+k");
  await pal.getByRole("combobox", { name: "Type a page or action" }).fill("update");
  await pal.keyboard.press("Enter");
  await pal.waitForFunction(() => window.__AIQ_FIXTURE__.calls.some((c) => c.cmd === "run_update" && c.args.action === "install"));
  await pal.context().close();
  const applied = (s) => Object.assign(s.update, { note: "applied", from: "0.0.9", at_ms: s.now_ms - 3_600_000 });
  const done = await open("full", { edit: applied });
  const line = done.getByTestId("update-line");
  assert.equal(await line.evaluate((e) => e.tagName), "P", "a slim line, not a banner (U5g)");
  assert.match(await line.innerText(), /^Updated to AIQ 0\.1\.0\./);
  assert.ok((await line.boundingBox()).height <= 40, "one line high");
  assert.deepEqual(await axe(done), []);
  await done.getByRole("button", { name: "Go back to 0.0.9" }).click();
  const back = await upd(done);
  assert.deepEqual(back, [{ action: "rollback" }]);
  await done.getByRole("button", { name: "Dismiss update note" }).click();
  assert.equal(await done.getByTestId("update-line").count(), 0, "dismissed");
  // Seen once: the next time Home opens it says nothing.
  await done.reload();
  await done.waitForSelector("main h1");
  assert.equal(await done.getByTestId("update-line").count(), 0, "seen on an earlier Home session: gone");
  await done.context().close();
  // No previous version known: no Go back that names nothing.
  const nofrom = await open("full", { edit: (s) => (applied(s), (s.update.from = null)) });
  assert.equal(await nofrom.getByRole("button", { name: /Go back/ }).count(), 0);
  await nofrom.context().close();
  const st = await open("full", { hash: "#settings" });
  assert.equal(await st.getByTestId("app-version").innerText(), "AIQ 0.1.0");
  assert.equal(await st.locator("nav").getByRole("button", { name: "Check for updates" }).count(), 1, "Check for updates beside the version");
  assert.equal(await st.getByTestId("update-ready").count(), 0, "nothing waiting: no ready box");
  assert.equal(await st.getByRole("radio", { name: /Update automatically/ }).isChecked(), true);
  assert.equal(await st.locator("main").getByRole("button", { name: "Check for updates" }).count(), 1, "Check for updates is offered under every choice");
  await st.getByRole("radio", { name: "Tell me first" }).check();
  await st.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Updates: tell me first.");
  const set = await st.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "set_setting").map((c) => c.args));
  assert.deepEqual(set, [{ key: "update", value: "ask" }]);
  await st.getByRole("radio", { name: "Don't check for updates" }).check();
  await st.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Updates: don't check for updates.");
  assert.match(await st.locator("main").innerText(), /AIQ doesn't check for updates and makes no network call of its own\. You're on AIQ 0\.1\.0\./);
  const set2 = await st.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "set_setting").map((c) => c.args));
  assert.deepEqual(set2.at(-1), { key: "update", value: "off" });
  await st.locator("main").getByRole("button", { name: "Check for updates" }).click();
  await st.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Checking for a newer AIQ…");
  const chk = await upd(st);
  assert.deepEqual(chk, [{ action: "check" }]);
  assert.deepEqual(await axe(st), []);
  await st.context().close();
  const dev = await open("full", { hash: "#settings", edit: (s) => (s.update.supported = false) });
  assert.match(await dev.locator("main").innerText(), /wasn't installed from a release, so it doesn't update itself/);
  assert.equal(await dev.getByRole("radio", { name: "Tell me first" }).count(), 0);
  assert.equal(await dev.getByRole("button", { name: "Check for updates" }).count(), 0, "a dev copy offers no Check for updates");
  await dev.context().close();
});

await test("Home drops 'could be' when no kind of task is one step from agents finishing it, and says how many could move up a rung", async () => {
  const page = await open("coaching", { edit: (s) => { s.coaching.split = { ...s.coaching.split, finished: 1, could: 0, needs: 14, up: 13, up_to: 2 }; } });
  const t = await page.locator("main").innerText();
  assert.ok(!t.includes("could be"), "no 'could be' figure");
  assert.match(t, /1 of 15 analyzed tasks finished by agents · 14 need you · 1 unsure, not counted/);
  assert.match(t, /87% have an easy next step, most often to Directed \(2\)\./);
  assert.equal(await page.getByRole("img", { name: "Agents finished 7%" }).count(), 1);
  await page.context().close();
});

await test("no terminal needed: Include a set-aside folder, Disconnect a tool, see what analysis sends, open the privacy page", async () => {
  const home = await open("full");
  // Set aside says what it is for, which folder (path), how much and when; AIQ's own test folders fold away.
  const aside = await home.locator("section[aria-labelledby=hb]").innerText();
  assert.match(aside, /AIQ leaves out runs an app or script started here\. A session where you send a second prompt still counts\./);
  assert.match(aside, /pipeline ~\\source\\repos\\pipeline\nMany sessions at once · 12 runs left out · last /);
  assert.match(aside, /AIQ's own test folders \(2, 2 runs\)/);
  assert.equal(await home.locator("section[aria-labelledby=hb] details").getAttribute("open"), null, "folded by default");
  assert.equal(await home.getByRole("button", { name: "Count run0" }).first().isVisible(), false);
  await home.getByRole("button", { name: "Count pipeline" }).click();
  await home.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "AIQ will include sessions in pipeline again.");
  const inc = await home.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "include_folder").map((c) => c.args));
  assert.deepEqual(inc, [{ folder: "c:\\users\\imran\\source\\repos\\pipeline" }]);
  assert.ok(!(await home.locator("main").innerText()).includes("aiq include"));
  await home.context().close();

  const st = await open("full", { hash: "#settings" });
  assert.ok(!(await st.locator("main").innerText()).includes("In a terminal"));
  await st.getByRole("button", { name: "See what would be sent" }).click();
  await st.waitForSelector("pre.preview");
  assert.match(await st.locator("pre.preview").innerText(), /Nothing was sent/);
  await st.getByRole("button", { name: "Disconnect Claude Code" }).click();
  await st.waitForFunction(() => document.querySelector("[role=status]")?.textContent.startsWith("Disconnected Claude Code."));
  assert.match(await st.getByRole("list", { name: "Files AIQ changed" }).innerText(), /settings\.json/);
  assert.equal(await st.getByRole("button", { name: "Disconnect Claude Code" }).count(), 0);
  await st.getByRole("button", { name: "Read the privacy page" }).click();
  const cmds = await st.evaluate(() => window.__AIQ_FIXTURE__.calls.map((c) => c.cmd));
  for (const c of ["judge_preview", "disconnect_tool", "open_privacy"]) assert.ok(cmds.includes(c), c);
  assert.deepEqual(await axe(st), []);
  await st.context().close();
});

await test("projects: Your projects ranks folders on Tasks, the picker scopes Home and Tasks, rename saves", async () => {
  const home = await open("coaching");
  assert.equal(await home.getByRole("table", { name: "Your projects" }).count(), 0, "Your projects moved from Home to Tasks");
  await home.context().close();
  const page = await open("coaching", { hash: "#tasks" });
  const table = page.getByRole("table", { name: "Your projects" });
  const rows = table.locator("tbody tr");
  assert.equal(await rows.count(), 2);
  assert.match(await rows.nth(0).innerText(), /aiq[\s\S]*12[\s\S]*Delegated \(3\)[\s\S]*45%[\s\S]*3 \(27%\)/);
  assert.match(await rows.nth(1).innerText(), /foresight[\s\S]*4[\s\S]*Directed \(2\)[\s\S]*Too few analyzed/);
  // Selecting a name scopes Tasks, and the table goes.
  await page.getByRole("button", { name: "Show foresight only" }).click();
  await page.waitForFunction(() => !document.querySelector("table[aria-label='Your projects']"));
  assert.equal(await page.getByLabel("Project").inputValue(), "c:\\users\\imran\\source\\repos\\foresight");
  assert.deepEqual(await axe(page), []);
  // The choice carries to Home.
  await page.locator("nav a", { hasText: "Home" }).click();
  await page.waitForSelector("[data-testid=project-note]");
  const t = await page.locator("main").innerText();
  assert.match(t, /Showing foresight only\. Tips and Automated, not counted cover all projects\./);
  // This fixture has analysis off, which holds for every project.
  assert.match(t, /Tasks agents finished\s+Analysis is off/);
  // On Tasks: only foresight's four tasks.
  await page.locator("nav a", { hasText: "Tasks" }).click();
  await page.getByRole("button", { name: "All tasks", exact: true }).click();
  await page.waitForSelector("table[aria-label=Tasks]");
  assert.equal(await page.locator("table[aria-label=Tasks] tbody tr").count(), 4);
  assert.match(await page.locator("main").innerText(), /4 tasks in the last 30 days/);
  await page.getByRole("button", { name: "Show all projects" }).click();
  assert.equal(await page.locator("table[aria-label=Tasks] tbody tr").count(), 16);
  await page.context().close();

  const r = await open("coaching", { hash: "#tasks" });
  await r.getByRole("button", { name: "Rename aiq" }).click();
  await r.getByLabel("New name for aiq").fill("AI coach");
  await r.getByRole("button", { name: "Save" }).click();
  await r.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Renamed to AI coach.");
  const set = await r.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "set_setting").map((c) => c.args));
  assert.deepEqual(set, [{ key: "project_name.c:\\users\\imran\\source\\repos\\aiq", value: "AI coach" }]);
  await r.context().close();

  // One project: no picker, no table.
  const solo = await open("full", { hash: "#tasks", edit: (s) => (s.projects = s.projects.slice(0, 1)) });
  assert.equal(await solo.getByLabel("Project").count(), 0);
  assert.equal(await solo.getByRole("table", { name: "Your projects" }).count(), 0);
  await solo.context().close();
});

// The tour (design home-tour): offered once after setup, nine steps on the example snapshot, Esc on any step.
const TOUR = ["Right now", "How you work with agents", "Is each kind of task moving up?", "Automated, not counted", "Tasks, by kind", "Your day", "Habits", "Tips show where you work", "Settings"];
const READS = new Set(["snapshot", "run_update", "task"]);

await test("tour: offered once after setup; Not now means never again", async () => {
  const page = await open("empty");
  await page.waitForSelector(".tour-offer");
  assert.match(await page.locator(".tour-offer").innerText(), /New to AIQ\?/);
  assert.deepEqual(await axe(page), []);
  await page.getByRole("button", { name: "Not now" }).click();
  await page.waitForFunction(() => !document.querySelector(".tour-offer"));
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.map((c) => c.cmd));
  assert.ok(calls.includes("tour_seen"), "Not now saves that the tour was seen");
  // The getting-started list is still the whole of empty Home, and still starts the tour.
  assert.match(await page.locator("main").innerText(), /AIQ is getting to know your work/);
  assert.equal(await page.getByRole("button", { name: "Take the tour" }).count(), 1);
  await page.context().close();

  const seen = await open("full", { edit: (s) => (s.setup.tour_seen = true) });
  await seen.waitForTimeout(200);
  assert.equal(await seen.locator(".tour-offer").count(), 0, "no offer once seen");
  await seen.context().close();

  const setup = await open("first-run");
  assert.equal(await setup.locator(".tour-offer").count(), 0, "no offer during setup");
  await setup.context().close();

  // Only on Home: another page never carries it.
  const other = await open("full", { hash: "#tasks" });
  await other.waitForTimeout(200);
  assert.equal(await other.locator(".tour-offer").count(), 0);
  await other.context().close();
});

await test("tour: nine steps on example data, every view found, own data back after", async () => {
  const page = await open("empty");
  await page.getByRole("button", { name: "Take the tour" }).first().click();
  for (let i = 0; i < TOUR.length; i++) {
    await page.waitForFunction((t) => document.querySelector("#tourhd")?.textContent === t, TOUR[i]);
    // The spotlight found the step's view (no centred fallback on the example data).
    await page.waitForSelector(".tour-spot", { timeout: 2000 });
    assert.equal(await page.locator(".titlebar .examplechip").innerText(), "Example data");
    assert.match(await page.locator(".tour-pop").innerText(), new RegExp(`${i + 1} of ${TOUR.length}`));
    if (i === 0 || i === 3 || i === 5) assert.deepEqual(await axe(page), [], `step ${i + 1}`);
    // The example data fills what each step names: a growing habit under the ladder map and on Habits, and a
    // script-driven folder with its Count this folder button.
    if (TOUR[i] === "Is each kind of task moving up?") assert.match(await page.locator(".hc-line").innerText(), /1 habit is growing/);
    if (TOUR[i] === "Automated, not counted") assert.equal(await page.getByRole("button", { name: "Count nightly-report" }).count(), 1);
    if (TOUR[i] === "Habits") {
      assert.match(await page.locator(".hc-sum").innerText(), /1 growing/);
      assert.match(await page.locator("section[aria-labelledby^=hab-]:has(.hc)").innerText(), /Growing in Writing tests/, "the spotlit rung has the growing habit");
    }
    if (shots) await page.screenshot({ path: join(shots, `tour-${i + 1}.png`) });
    await page.getByRole("button", { name: i === TOUR.length - 1 ? "Done" : "Next", exact: true }).click();
  }
  await page.waitForFunction(() => !document.querySelector(".tour-pop"));
  assert.equal(await page.locator(".examplechip").count(), 0);
  assert.match(await page.locator("main").innerText(), /AIQ is getting to know your work/, "the person's own (empty) Home is back");
  assert.equal(await page.locator(".tour-offer").count(), 0, "taking the tour counts as seen");
  const writes = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.map((c) => c.cmd));
  assert.deepEqual([...new Set(writes.filter((c) => !READS.has(c)))], ["tour_seen"], "the tour writes nothing but its flag");
  assert.deepEqual(page.errors, []);
  await page.context().close();
});

await test("tour: Esc on any step, arrows move, focus stays in the coach mark, the page can't be clicked", async () => {
  const page = await open("full", { edit: (s) => (s.setup.tour_seen = true) });
  await page.keyboard.press("Control+k");
  await page.getByLabel("Type a page or action").fill("tour");
  await page.keyboard.press("Enter");
  await page.waitForSelector("#tourhd");
  // The tour focuses Next a frame after it opens (tour.tsx), so the closing palette can't take focus back:
  // wait for that frame instead of reading focus the moment the heading exists (ui flake on main 69ad8fe).
  await page
    .waitForFunction(() => document.activeElement?.textContent === "Next", null, { timeout: 2000 })
    .catch(() => {});
  assert.equal(await page.evaluate(() => document.activeElement?.textContent), "Next", "focus on Next");
  await page.keyboard.press("ArrowRight");
  await page.waitForFunction((t) => document.querySelector("#tourhd")?.textContent === t, TOUR[1]);
  await page.keyboard.press("ArrowLeft");
  await page.waitForFunction((t) => document.querySelector("#tourhd")?.textContent === t, TOUR[0]);
  for (let n = 0; n < 4; n++) {
    await page.keyboard.press("Tab");
    assert.ok(await page.evaluate(() => !!document.activeElement?.closest(".tour-pop")), "Tab stays in the coach mark");
  }
  // Single-key shortcuts are off while the tour is open.
  await page.keyboard.press("g");
  await page.keyboard.press("s");
  assert.equal(await page.locator("#tourhd").innerText(), TOUR[0]);
  // A click on the side nav lands on the blocking layer.
  await page.mouse.click(60, 160);
  assert.equal(await page.locator("#tourhd").innerText(), TOUR[0]);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await page.waitForFunction((t) => document.querySelector("#tourhd")?.textContent === t, TOUR[3]);
  assert.match(page.url(), /#$|\/$|#home$/, "the tour moves pages without rewriting the address");
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !document.querySelector(".tour-pop"));
  assert.equal(await page.locator("nav a[aria-current=page]").innerText(), "Home");
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.map((c) => c.cmd));
  assert.ok(!calls.includes("tour_seen"), "already seen: nothing written");
  await page.context().close();

  // From Settings, in dark and high contrast.
  for (const theme of ["dark", "contrast"]) {
    const s = await open("full", { hash: "#settings", theme });
    await s.getByRole("button", { name: "Take the tour" }).click();
    await s.waitForSelector(".tour-spot");
    assert.deepEqual(await axe(s), [], theme);
    await s.getByRole("button", { name: "Close the tour" }).click();
    await s.waitForFunction(() => !document.querySelector(".tour-pop"));
    await s.context().close();
  }
});

await test("Theme: Match Windows by default, Light or Dark win over Windows, kept after a restart, also in Ctrl K", async () => {
  // Windows set to dark: the default follows it.
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, colorScheme: "dark" });
  const page = await ctx.newPage();
  const snapshot = JSON.parse(readFileSync(join(fixtures, "full.json"), "utf8"));
  await page.addInitScript((s) => (window.__AIQ_FIXTURE__ = { snapshot: s, calls: [] }), snapshot);
  const bg = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  const attr = () => page.evaluate(() => document.documentElement.dataset.theme ?? null);
  const DARK = "rgb(27, 29, 30)";
  const LIGHT = "rgb(243, 244, 244)";
  await page.goto(base + "#settings");
  await page.waitForSelector("main h1");
  const group = page.getByRole("group", { name: "Theme" });
  assert.deepEqual(await group.getByRole("button").allInnerTexts(), ["Match Windows", "Light", "Dark"]);
  assert.equal(await group.getByRole("button", { name: "Match Windows" }).getAttribute("aria-pressed"), "true");
  assert.equal(await attr(), null);
  assert.equal(await bg(), DARK);
  // Light wins over a dark Windows, at once, and says so.
  await group.getByRole("button", { name: "Light" }).click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Theme: light.");
  assert.equal(await attr(), "light");
  assert.equal(await bg(), LIGHT);
  assert.equal(await group.getByRole("button", { name: "Light" }).getAttribute("aria-pressed"), "true");
  await settle(page);
  assert.deepEqual(await axe(page), []);
  if (shots) await page.screenshot({ path: join(shots, "theme-light.png") });
  // Dark, kept after AIQ home opens again: set before React draws, so no light flash.
  await group.getByRole("button", { name: "Dark" }).click();
  assert.equal(await attr(), "dark");
  await page.reload();
  assert.equal(await attr(), "dark");
  await page.waitForSelector("main h1");
  assert.equal(await page.getByRole("group", { name: "Theme" }).getByRole("button", { name: "Dark" }).getAttribute("aria-pressed"), "true");
  if (shots) await page.screenshot({ path: join(shots, "theme-dark.png") });
  // Ctrl K offers the other two; Match Windows goes back to following Windows and clears the saved choice.
  await page.keyboard.press("Control+k");
  await page.keyboard.type("theme");
  const opts = await page.locator(".pal-dlg [role=option]").allInnerTexts();
  assert.ok(opts.some((o) => o.includes("Theme: match Windows")) && opts.some((o) => o.includes("Theme: light")) && !opts.some((o) => o.includes("Theme: dark")), opts.join(" | "));
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Theme matches Windows.");
  assert.equal(await attr(), null);
  assert.equal(await page.evaluate(() => localStorage.getItem("aiq.theme")), null);
  await page.emulateMedia({ colorScheme: "light" });
  assert.equal(await bg(), LIGHT, "follows Windows again");
  assert.deepEqual(page.errors ?? [], []);
  await ctx.close();
});

// Tip funnel (design tip-funnel): the full fixture's yesterday had six tasks and no tip shown.
for (const theme of ["light", "dark"]) {
  await test(`tip funnel: Tips shows where tips went; Home notes a dry day once (${theme})`, async () => {
    const page = await open("full", { theme: theme === "light" ? undefined : theme, hash: "#tips" });
    const card = page.locator("section[aria-labelledby=tf]");
    const insight = await card.locator(".tf-insight").textContent();
    assert.match(insight, /^\d+ of \d+ new tasks had a tip ready this week\. Most common reason: kind of task not recognised \(\d+\)\.$/);
    assert.match(await card.locator(".tf-help").textContent(), /^AIQ reads how a task starts\./, "what helps sits under the headline");
    assert.equal(await card.locator(".tf-loss .tf-label").textContent(), "Kind of task clear", "most were lost before the kind was clear");
    const ns = (await card.locator(".tf-bars .tf-n").allTextContents()).map(Number);
    ns.slice(1).forEach((n, i) => assert.ok(n <= ns[i], `bar ${i + 1} (${n}) passes the one above (${ns[i]})`));
    const more = card.locator("button.tf-more");
    assert.equal(await more.textContent(), "Most lost here: see the 5 tasks");
    assert.equal(await more.getAttribute("aria-expanded"), "false");
    await more.click();
    assert.equal(await more.getAttribute("aria-expanded"), "true");
    assert.equal(await card.locator("#tf-lost li").count(), 5);
    assert.equal(await card.locator("#tf-lost h4").first().innerText(), "Kind of task not recognised (4)", "grouped by reason, biggest first");
    // The seeded tip-path tasks have no stored prompt: the row says so rather than showing nothing or a guess.
    assert.match(await card.locator("#tf-lost li").first().innerText(), /Prompt not stored/);
    assert.equal(await card.locator(".tf-why", { hasText: "Why tasks had no tip" }).count(), 0, "the summary hides while the list is open");
    assert.deepEqual(await axe(page), [], "the open list passes accessibility checks");
    if (shots) await card.screenshot({ path: join(shots, `tips-funnel-open-${theme}.png`) });
    await more.click();
    const why = await card.locator(".tf-why", { hasText: "Why tasks had no tip" }).innerText();
    assert.ok(why.includes("Kind labels on hold") && why.includes("Too few tasks of this kind"), why);
    assert.ok(!(await card.innerText()).toLowerCase().includes("judge"));
    assert.deepEqual(await axe(page), []);
    if (shots) await card.screenshot({ path: join(shots, `tips-funnel-${theme}.png`) });
    await page.context().close();

    // A row opens the task's page (the rendered page, not just the address: 0.1.31 changed only the hash).
    const real = await open("full", {
      theme: theme === "light" ? undefined : theme,
      hash: "#tips",
      edit: (s) => (s.tip_funnel.week.lost[0].task_id = s.tasks[0].task_id),
    });
    const rc = real.locator("section[aria-labelledby=tf]");
    await rc.locator("button.tf-more").click();
    await rc.locator("#tf-lost a").first().click();
    await real.waitForSelector(".backlink");
    assert.notEqual(await real.locator("main h1").first().textContent(), "Habits and tips", "the task page replaced Tips");
    assert.deepEqual(real.errors ?? [], []);
    await real.context().close();

    const home = await open("full", { theme: theme === "light" ? undefined : theme, hash: "#home" });
    const note = home.locator(".banner.drought");
    assert.match(await note.locator(".grow").textContent(), /^No tips yesterday across 6 tasks\. Main reason: kind of task not recognised\.$/);
    assert.deepEqual(await axe(home), []);
    if (shots) await home.screenshot({ path: join(shots, `home-drought-${theme}.png`) });
    await note.getByRole("button", { name: "See why" }).click();
    assert.equal(await home.locator("main h1").first().textContent(), "Habits and tips");
    await home.locator('a[href="#home"]').first().click();
    await home.waitForSelector(".banner.drought");
    await home.getByRole("button", { name: "Close this note" }).click();
    assert.equal(await home.locator(".banner.drought").count(), 0);
    await home.reload();
    await home.waitForSelector("main h1");
    assert.equal(await home.locator(".banner.drought").count(), 0, "closed stays closed for that day");
    assert.deepEqual(home.errors, []);
    await home.context().close();
  });
}

await test("tip funnel: nothing shows with no task on the tip path", async () => {
  const page = await open("full", { hash: "#tips", edit: (s) => { s.tip_funnel.week = { ...s.tip_funnel.today, tasks: 0, tips: 0 }; s.tip_funnel.droughts = []; } });
  assert.equal(await page.locator("section[aria-labelledby=tf]").count(), 0);
  await page.context().close();
});

await browser.close();
server.close();
const failed = results.filter((r) => r[1] !== "pass").length;
console.log(`\n${results.length - failed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
