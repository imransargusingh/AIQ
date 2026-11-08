// Screenshots for the README (docs/images/readme), taken from the same synthetic snapshots the UI tests use.
// No real data: folder paths and project names in the fixtures are swapped for neutral example names here.
// Run: npm run build && npm run readme-shots
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync, existsSync, mkdirSync } from "node:fs";
import { join, extname, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "..", "dist");
const fixtures = join(here, "fixtures");
const out = join(here, "..", "..", "..", "docs", "images", "readme");
mkdirSync(out, { recursive: true });

const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css" };
const server = createServer((req, res) => {
  if (req.url === "/favicon.ico") return res.writeHead(204).end();
  const p = join(dist, decodeURIComponent(new URL(req.url, "http://x").pathname).replace(/^\/$/, "/index.html"));
  if (!p.startsWith(dist) || !existsSync(p)) return res.writeHead(404).end();
  res.writeHead(200, { "content-type": types[extname(p)] ?? "application/octet-stream" }).end(readFileSync(p));
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;
const browser = await chromium.launch(process.env.PW_CHROMIUM ? { executablePath: process.env.PW_CHROMIUM } : {});

/** The fixtures were cut from a developer machine; README images show neutral example names instead. */
function neutral(json) {
  return json
    .replace(/c:\\\\users\\\\[a-z]+\\\\appdata\\\\local\\\\temp\\\\[^"]*/gi, "c:\\\\users\\\\you\\\\source\\\\repos\\\\mobile-app")
    .replace(/c:\\\\users\\\\[a-z]+\\\\/gi, "c:\\\\users\\\\you\\\\")
    .replace(/repos\\\\aiq\b/gi, "repos\\\\shop-api")
    .replace(/repos\\\\foresight\b/gi, "repos\\\\docs-site")
    .replace(/repos\\\\pipeline\b/gi, "repos\\\\mobile-app")
    .replace(/"name": "aiq"/g, '"name": "shop-api"')
    .replace(/"name": "foresight"/g, '"name": "docs-site"')
    .replace(/"name": "pipeline"/g, '"name": "mobile-app"')
    // task.json carries markup to prove the app shows it as text; not something a README reader needs to see.
    .replace(/ <b>bold<\/b>/g, "");
}

/** Two tools connected, recent activity, tour already seen: what a settled install looks like. */
function settled(s) {
  s.setup.tour_seen = true;
  s.update.running = JSON.parse(readFileSync(join(here, "..", "package.json"), "utf8")).version;
  s.setup.tools = ["claude-code", "copilot"].map((tool) => ({ tool, hooked: true, present: true, install: "" }));
  s.seen = [
    { surface: "copilot-cli", last_ms: s.now_ms - 12 * 60_000 },
    { surface: "claude-code", last_ms: s.now_ms - 40 * 60_000 },
  ];
}

async function open(fixture, { theme, hash = "", width = 1280, height = 900, edit } = {}) {
  const ctx = await browser.newContext({ viewport: { width, height }, deviceScaleFactor: 2 });
  const page = await ctx.newPage();
  const snapshot = JSON.parse(neutral(readFileSync(join(fixtures, `${fixture}.json`), "utf8")));
  const task = JSON.parse(neutral(readFileSync(join(fixtures, "task.json"), "utf8")));
  settled(snapshot);
  if (edit) edit(snapshot);
  await page.addInitScript(([s, t]) => {
    window.__AIQ_FIXTURE__ = { snapshot: s, task: t, calls: [], fail: [] };
  }, [snapshot, task]);
  await page.goto(base + hash);
  if (theme) await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
  await page.waitForSelector("main h1");
  await page.evaluate(() => Promise.all(document.getAnimations().map((a) => a.finished.catch(() => {}))));
  return page;
}

async function shot(name, page, locator) {
  const path = join(out, `${name}.png`);
  if (locator) {
    // A little room around the part, so it reads as a cut from the window rather than a bare element.
    const b = await page.locator(locator).first().boundingBox();
    await page.setViewportSize({ width: 1280, height: Math.ceil(b.y + b.height + 40) });
    const c = await page.locator(locator).first().boundingBox();
    await page.screenshot({ path, clip: { x: c.x - 16, y: c.y - 16, width: c.width + 32, height: c.height + 32 } });
  }
  else await page.screenshot({ path });
  console.log(`wrote ${path}`);
  await page.context().close();
}

await shot("home", await open("coaching"));
await shot("home-dark", await open("coaching", { theme: "dark" }));
await shot("right-now", await open("live"), "[data-testid=right-now]");
await shot(
  "right-now-not-counted",
  await open("full", {
    edit: (s) => {
      s.starting = [];
      s.aside = [{ surface: "claude-code", last_ms: s.now_ms - 2 * 60_000, reason: "projects" }];
    },
  }),
  "[data-testid=right-now]",
);
await shot("progress", await open("progress"), "section[aria-labelledby=pg]");
await shot("habits-and-tips", await open("coaching", { hash: "#tips", height: 1100 }));
await shot("task", await open("full", { hash: "#task/019000000001-0000-7000-8000-000000000000", height: 1000 }));
await shot("settings-tips", await open("full", { hash: "#settings", edit: (s) => (s.settings.daily_cap = 5) }), "section[aria-labelledby=st1]");

await browser.close();
server.close();
