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

async function open(fixture, { theme, hash = "", width = 1280, height = 800, forced = false, edit, fail = [] } = {}) {
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
  for (const f of ["full", "down", "paused", "empty", "first-run", "coaching"]) {
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

await test("Home shows judged rungs, progress toward the split, and only measured counts", async () => {
  const page = await open("full");
  const t = await page.locator("main").innerText();
  assert.match(t, /Tasks agents finished\s+Not enough judged tasks yet/);
  assert.match(t, /2 of 10 judged tasks needed to show what agents finished \(1 AIQ isn't sure about don't count\)/);
  assert.match(t, /7 tasks in the last 30 days · 1 imported task/);
  assert.doesNotMatch(t, /\d+%/, "no percentages under 10 judged tasks");
  assert.match(t, /Your most common rung\s+Directed \(2\)\s+3 tasks judged in 30 days/);
  assert.match(t, /\(1 uncertain not counted\)/);
  assert.match(t, /Where agents could do more\s+[^\n]*\n+Next steps show once AIQ knows the kind of each task/);
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
  for (const name of ["Factory (5), 0 tasks", "Directed (2), 1 task"]) {
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
  assert.ok(t.includes("Uncertain"), "an Uncertain judgment says so");
  assert.ok(t.includes("Code a change") && t.includes("Write a spec or doc"), "judged jobs in words");
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
  assert.equal(await h1(), "Tools AIQ will watch");
  assert.equal(await page.evaluate(() => document.activeElement?.tagName), "H1", "focus moves to the step's heading");
  assert.match(await page.locator(".steps li.done").innerText(), /Privacy\s*\(done\)/);
  // First run connects the tool it found by itself, and says how to undo it.
  await page.waitForFunction(() => document.querySelector("main")?.innerText.includes("GitHub Copilot · connected just now"));
  let t = await page.locator("main").innerText();
  assert.match(t, /Claude Code · connected\n/);
  assert.ok(t.includes("To undo, run aiq uninstall copilot."));
  assert.ok(!t.includes("aiq uninstall claude-code"), "only the tool connected just now shows the undo");
  assert.equal(await page.getByRole("button", { name: /^Connect/ }).count(), 0);
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Connected GitHub Copilot.");
  assert.deepEqual(await axe(page), []);
  await page.getByRole("button", { name: "Back" }).click();
  assert.equal(await h1(), "Your coach, on your computer only");
  await cont.click();
  await cont.click();
  assert.equal(await h1(), "How AIQ will judge your tasks");
  t = await page.locator("main").innerText();
  assert.ok(t.includes("Nothing to connect yet."));
  assert.ok(t.includes("AIQ never reads or stores your sign-in."));
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
  await page.waitForFunction(() => document.querySelector("main h1")?.textContent === "AIQ is watching quietly");
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Setup done. AIQ is watching.");
  assert.equal(await page.locator("nav").count(), 1);
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.map((c) => c.cmd).filter((c) => c !== "snapshot"));
  assert.deepEqual(calls, ["connect_tools", "set_setting", "finish_setup"], "connected once, not again on Back");
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
  const page = await open("first-run", { edit: (s) => (s.status.core = false) });
  await page.keyboard.press("g");
  await page.keyboard.press("d");
  await page.keyboard.press("Control+k");
  assert.equal(await page.locator("main h1").textContent(), "Your coach, on your computer only");
  assert.equal(await page.locator("dialog[open]").count(), 0);
  for (let i = 0; i < 3; i++) await page.getByRole("button", { name: "Continue" }).click();
  assert.ok((await page.locator("main").innerText()).includes("AIQ isn't running yet, so this stays at 5 for now."));
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
  assert.match(t, /Of 15 judged tasks: 5 finished by agents, 5 could be, 5 need you\. 1 task AIQ isn't sure about is left out\./);
  assert.match(t, /A task counts as finished when it was judged Delegated or higher and you sent no re-prompt correcting the agent\./);
  assert.match(t, /Judged by your Copilot CLI/);
  const rows = page.getByRole("list", { name: "Kinds of task with a next step" }).getByRole("listitem");
  assert.equal(await rows.count(), 2);
  const first = await rows.first().innerText();
  assert.match(first, /Writing tests[\s\S]*Delegated \(3\) → Orchestrated \(4\)[\s\S]*7 tasks · 1\.6 a week[\s\S]*Next step: Give the agent a written brief or instructions file/);
  assert.ok(!(await page.locator("main").innerHTML()).includes("Give the agent <b>"), "judge text is not on Home");
  // A week with fewer than 3 judged tasks says so instead of a share.
  assert.ok((await page.locator(".wk-col").first().getAttribute("aria-label")).includes("too few judged tasks to say"));
  await rows.first().getByRole("button").click();
  await page.waitForSelector("main h1:text('Writing tests')");
  assert.ok(page.url().endsWith("#kind/code_change/tests"));
  await page.context().close();
});

await test("kind of task: the pack's step, copy starter, judge words as text, This doesn't fit with a reason, Undo", async () => {
  const page = await open("coaching", { hash: "#kind/code_change/tests" });
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"], { origin: base });
  const t = await page.locator("main").innerText();
  assert.match(t, /Next step: Give the agent a written brief or instructions file/);
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
  await page.waitForSelector("main h2:text('You flagged this step')");
  await page.getByRole("button", { name: "Undo" }).click();
  await page.waitForSelector("main h2:text('Next step: Give the agent a written brief or instructions file')");
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

await test("Tasks: kinds of task listed with their state; job column names the kind", async () => {
  const page = await open("coaching", { hash: "#tasks" });
  const kinds = await page.getByRole("list", { name: "Kinds of task" }).innerText();
  for (const k of ["Writing tests", "Writing a spec or PRD", "Fixing a bug or failing test", "Right rung", "Refactoring or cleaning up", "Too few tasks yet"]) assert.ok(kinds.includes(k), k);
  assert.match(await page.locator("tbody").innerText(), /Code a change · Writing tests/);
  await page.context().close();
});

await test("Habits: words for each state, seen in n of m, banners, imported footnote", async () => {
  const page = await open("coaching", { hash: "#tips" });
  const t = await page.locator("main").innerText();
  assert.ok(!t.includes("Habit tracking comes in a later version"));
  assert.match(t, /Give the agent a written brief or instructions file\s+Not yet\s+Seen in 0 of 6 tasks in 14 days/);
  assert.match(t, /Hand over the whole task with a done test\s+Adopted\s+Seen in 6 of 6 tasks in 14 days/);
  const banners = await page.locator(".banner.info").count();
  assert.ok(banners >= 1 && banners <= 3, `one to three banners, got ${banners}`);
  assert.match(t, /You adopted a habit:/);
  assert.ok(!t.includes("Includes tasks AIQ imported"), "no imported tasks in this fixture");
  const imp = await open("coaching", { hash: "#tips", edit: (s) => (s.coaching.habits_imported = true) });
  assert.match(await imp.locator("main").innerText(), /Includes tasks AIQ imported from your history\./);
  await imp.context().close();
  const lapse = await open("coaching", {
    hash: "#tips",
    edit: (s) => {
      s.coaching.habits = [{ ...s.coaching.habits[0], state: "tried", moved_to: "tried", moved_ms: s.now_ms - 86_400_000, moved_back: true, seen: 1, eligible: 4 }];
    },
  });
  assert.match(await lapse.locator("main").innerText(), /Back to Tried: .* You did it in 1 of 4 recent tasks where it fits\./);
  await lapse.context().close();
  const empty = await open("empty", { hash: "#tips" });
  assert.match(await empty.locator("main").innerText(), /No habits to show yet\./);
  await empty.context().close();
  await page.context().close();
});

await test("Settings: next-step tips auto/on/off, AIQ's self-check stop with Turn back on, tips turned off", async () => {
  const page = await open("coaching", { hash: "#settings" });
  assert.equal(await page.getByLabel(/On, unless AIQ's guesses are often wrong/).isChecked(), true);
  await page.getByLabel("Off", { exact: true }).check();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent === "Next-step tips: off.");
  const off = page.getByRole("list", { name: "Tips you turned off" });
  assert.match(await off.innerText(), /Ask the agent to run the tests itself/);
  await off.getByRole("button", { name: "Turn back on" }).click();
  await page.waitForFunction(() => document.querySelector("[role=status]")?.textContent?.endsWith("tip turned back on."));
  assert.equal(await page.getByRole("list", { name: "Tips you turned off" }).count(), 0);
  const calls = await page.evaluate(() => window.__AIQ_FIXTURE__.calls.filter((c) => ["set_setting", "tip_on"].includes(c.cmd)).map((c) => c.args));
  assert.deepEqual(calls, [{ key: "next_rung_tips", value: "off" }, { tipId: "next_rung.agent_checks" }]);
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

await browser.close();
server.close();
const failed = results.filter((r) => r[1] !== "pass").length;
console.log(`\n${results.length - failed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
