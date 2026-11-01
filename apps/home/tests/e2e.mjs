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

async function open(fixture, { theme, hash = "", width = 1280, height = 800, forced = false } = {}) {
  const ctx = await browser.newContext({ viewport: { width, height }, forcedColors: forced ? "active" : "none" });
  const page = await ctx.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  const snapshot = JSON.parse(readFileSync(join(fixtures, `${fixture}.json`), "utf8"));
  await page.addInitScript((s) => {
    window.__AIQ_FIXTURE__ = { snapshot: s, calls: [] };
  }, snapshot);
  await page.goto(base + hash);
  if (theme) await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
  await page.waitForSelector("main h1");
  page.errors = errors;
  return page;
}

async function axe(page) {
  await page.evaluate(axeSrc);
  const r = await page.evaluate(() => window.axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"] } }));
  return r.violations.map((v) => `${v.id}: ${v.nodes.slice(0, 3).map((n) => n.target.join(" ")).join(" | ")}`);
}

const pages = [
  ["home", "Home"],
  ["tasks", "Tasks"],
  ["tips", "Habits and tips"],
  ["data", "Your data"],
  ["settings", "Settings"],
];

await test("contract: fixtures are aiq.home.v1 snapshots from the Rust read model", async () => {
  for (const f of ["full", "down", "paused", "empty"]) {
    const s = JSON.parse(readFileSync(join(fixtures, `${f}.json`), "utf8"));
    assert.equal(s.schema, "aiq.home.v1");
    for (const k of ["status", "home", "tasks", "tips", "data", "settings"]) assert.ok(k in s, `${f}: ${k}`);
  }
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

await test("forced colours (Windows high contrast) has no axe violations", async () => {
  const page = await open("full", { forced: true });
  assert.deepEqual(await axe(page), []);
  await page.context().close();
});

await test("Home is honest without a judge and shows measured counts", async () => {
  const page = await open("full");
  const t = await page.locator("main").innerText();
  assert.match(t, /Tasks agents finished\s+Not judged yet/);
  assert.match(t, /7 tasks in the last 30 days · 1 imported task/);
  assert.doesNotMatch(t, /\d+%/, "no percentages without judged tasks");
  assert.match(t, /Your most common rung\s+Not judged yet/);
  for (const r of ["Assisted (1)", "Directed (2)", "Delegated (3)", "Orchestrated (4)", "Factory (5)"]) assert.ok(t.includes(r), r);
  assert.equal(await page.locator(".wk-col").count(), 4);
  // Rung rows expand to say what the rung means.
  const row = page.getByRole("button", { name: /Delegated \(3\)/ });
  await row.click();
  assert.equal(await row.getAttribute("aria-expanded"), "true");
  assert.match(await page.locator("main").innerText(), /hand the agent a whole outcome/);
  await page.context().close();
});

await test("Home answers the headline without scrolling at 1280x800", async () => {
  const page = await open("full");
  const box = await page.locator("#kr").boundingBox();
  assert.ok(box && box.y + box.height < 800, `headline at ${box?.y}`);
  await page.context().close();
});

await test("screen readers hear clean names: nav, rung rows, skip link", async () => {
  const page = await open("full");
  // Names come from the accessibility tree, as a screen reader gets them (laptop check 2026-11-01).
  const nav = await page.getByRole("navigation", { name: "AIQ" }).ariaSnapshot();
  assert.match(nav, /link "Home"/, nav);
  assert.equal(await page.getByRole("link", { name: "Home", exact: true }).getAttribute("aria-keyshortcuts"), "G H");
  for (const name of ["Factory (5), not judged yet", "Assisted (1), not judged yet"]) {
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

await test("keyboard: single-key shortcuts can be turned off", async () => {
  const page = await open("full", { hash: "#settings" });
  await page.getByRole("switch").click();
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
  assert.equal(await page.locator("tbody tr").count(), 7);
  const prompts = page.getByRole("button", { name: /^Prompts/ });
  await prompts.click();
  assert.equal(await page.locator("th").nth(2).getAttribute("aria-sort"), "descending");
  assert.equal((await page.locator("tbody tr").first().locator("td").nth(2).textContent()).trim(), "3");
  await page.locator("tbody tr").first().focus();
  await page.keyboard.press("j");
  assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("aria-selected")), "true");
  assert.equal(await page.locator("tbody tr[aria-selected=true]").count(), 1);
  const t = await page.locator("tbody").innerText();
  assert.ok(t.includes("Imported"));
  assert.ok(t.includes("Not judged yet") && t.includes("Not classified yet"));
  await page.locator("a[href='#data']").click();
  assert.equal(await page.locator("main img").count(), 0, "a prompt containing <img> renders as text");
  assert.ok((await page.locator("main").innerText()).includes("<img src=x onerror=alert(1)> add tests"));
  assert.ok((await page.locator("main").innerText()).includes("Text deleted"));
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
  assert.equal(await page.getByRole("button", { name: "3", exact: true }).getAttribute("aria-pressed"), "true");
  await page.getByRole("button", { name: "5", exact: true }).click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Tips a day set to 5.");
  await page.getByLabel("Until you open it").check();
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => c.cmd === "set_setting").map((c) => c.args));
  assert.deepEqual(calls, [{ key: "daily_cap", value: "5" }, { key: "copilot_tip_stays", value: "until-opened" }]);
  await page.context().close();
});

await test("core down: banner with stored time on every page; settings disabled", async () => {
  for (const [id] of pages) {
    const page = await open("down", { hash: `#${id}` });
    assert.match(await page.locator("main").innerText(), /AIQ isn't running, so this shows what was stored at Today 11:30/);
    if (id === "settings") assert.equal(await page.getByRole("button", { name: "5", exact: true }).isDisabled(), true);
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
  for (const [id, , expect] of [["home", 0, "AIQ is watching quietly"], ["tasks", 0, "No tasks yet."], ["tips", 0, "No tips yet."], ["data", 0, "AIQ has no stored history."]]) {
    const page = await open("empty", { hash: `#${id}` });
    assert.ok((await page.locator("main").innerText()).includes(expect), id);
    assert.deepEqual(await axe(page), []);
    if (shots) await page.screenshot({ path: join(shots, `${id}-empty.png`) });
    await page.context().close();
  }
});

await test("narrow window (720x520) has no horizontal page scroll", async () => {
  for (const [id] of pages) {
    const page = await open("full", { hash: `#${id}`, width: 720, height: 520 });
    const over = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    assert.ok(over <= 0, `${id} overflows by ${over}px`);
    await page.context().close();
  }
});

await browser.close();
server.close();
const failed = results.filter((r) => r[1] !== "pass").length;
console.log(`\n${results.length - failed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
