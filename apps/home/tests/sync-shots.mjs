// Screenshots of the sync screens (design multi-machine-sync) from the built app over the test fixtures, for review.
// No real data: computer names are examples. Run: npm run build && node tests/sync-shots.mjs <out folder>
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync, existsSync, mkdirSync } from "node:fs";
import { join, extname, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "..", "dist");
const out = process.argv[2] ?? join(here, "..", "test-results", "sync");
mkdirSync(out, { recursive: true });
const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css" };
const server = createServer((req, res) => {
  const p = join(dist, decodeURIComponent(new URL(req.url, "http://x").pathname).replace(/^\/$/, "/index.html"));
  if (!p.startsWith(dist) || !existsSync(p)) return res.writeHead(404).end();
  res.writeHead(200, { "content-type": types[extname(p)] ?? "application/octet-stream" }).end(readFileSync(p));
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;
const browser = await chromium.launch();
const fixture = (name) => JSON.parse(readFileSync(join(here, "fixtures", `${name}.json`), "utf8"));
const sync = (s, over) => (s.sync = { id: "a1", name: "DEVBOX-07", setting: "unset", offer: null, via: null, peers: [], pending: 0, utc_ms: s.now_ms, errors: [], ...over });

const shots = [
  ["offer-fresh-devbox", "empty", "", (s) => sync(s, { offer: { name: "LAPTOP-9" }, via: "LAPTOP-9" })],
  ["offer-home", "full", "", (s) => sync(s, { offer: { name: "LAPTOP-9" }, via: "LAPTOP-9" })],
  ["settings-in-sync", "full", "#settings", (s) => sync(s, { setting: "on", via: "LAPTOP-9", peers: [{ id: "b2", name: "LAPTOP-9", version: "0.1.44", state: "ok", detail: null, last_ms: s.now_ms - 60_000, rows: 812 }] })],
  ["settings-paused", "full", "#settings", (s) => sync(s, { setting: "on", peers: [{ id: "b2", name: "LAPTOP-9", version: "0.1.44", state: "paused", detail: null, last_ms: s.now_ms - 3_600_000, rows: 812 }] })],
  ["settings-away", "full", "#settings", (s) => sync(s, { setting: "on", peers: [{ id: "b2", name: "LAPTOP-9", version: "0.1.44", state: "away", detail: null, last_ms: s.now_ms - 3 * 3_600_000, rows: 812 }] })],
  ["tasks-tag", "full", "#tasks", (s) => { sync(s, { setting: "on", peers: [{ id: "b2", name: "LAPTOP-9", version: "0.1.44", state: "ok", detail: null, last_ms: s.now_ms, rows: 812 }] }); s.tasks[0].computer = "b2"; s.tasks[2].computer = "b2"; }],
];
for (const theme of ["light", "dark"]) {
  for (const [name, fx, hash, edit] of shots) {
    const ctx = await browser.newContext({ viewport: { width: 1180, height: 760 }, deviceScaleFactor: 2 });
    const page = await ctx.newPage();
    const s = fixture(fx);
    s.setup.tour_seen = true;
    edit(s);
    await page.addInitScript(([snap, task]) => (window.__AIQ_FIXTURE__ = { snapshot: snap, task, calls: [] }), [s, fixture("task")]);
    await page.goto(base + hash);
    await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
    await page.waitForSelector("main h1");
    if (hash === "#settings") await page.getByRole("heading", { name: "Your computers" }).scrollIntoViewIfNeeded();
    await page.waitForTimeout(250);
    await page.screenshot({ path: join(out, `${name}-${theme}.png`) });
    await ctx.close();
  }
}
await browser.close();
server.close();
