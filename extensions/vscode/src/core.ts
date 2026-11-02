// The AIQ core client and the poll logic, with no `vscode` import so they are unit-tested with plain Node.
// Wire contract: one newline-terminated JSON frame per connection over the core's user-only socket / pipe,
// with the per-boot token from a user-only file (Phase 1 design, Exposed contract 3). The endpoint comes
// from `aiq endpoint`, so the socket path and pipe name are defined only in the Rust core.
import { execFile } from "node:child_process";
import * as fs from "node:fs";
import * as net from "node:net";
import * as os from "node:os";
import * as path from "node:path";

export interface Endpoint {
  socket?: string;
  pipe?: string;
  token: string;
  paused_flag: string;
}

/** `aiq.card.v1` (contracts/aiq.card.v1.json). */
export interface Card {
  schema: string;
  card_id: string;
  opportunity_id: string;
  tip_id: string;
  noticed: string;
  why: string;
  try_this: string;
  evidence_ref: string;
  pack_version: string;
  /** Next-rung tips: a starter prompt to copy (never sent). */
  starter?: string;
}

/** A judged task a next-rung guess leaned on: time, tool and kind, never its text. */
export interface LeanedOn {
  ts: string;
  tool: string;
  task_type: string;
}

export interface EvidencePrompt {
  ts: string;
  text: string | null;
}

export interface ShowReply {
  card: Card | null;
  outcome: string | null;
  reason_code: string | null;
  delivered: boolean;
  evidence: EvidencePrompt[];
  leaned_on?: LeanedOn[];
}

/** The person's answers to a tip (coaching-from-rungs D8), as the notification's buttons name them. */
export const FEEDBACK_BUTTONS: [string, string][] = [
  ["Helpful", "helpful"],
  ["Not helpful", "not_helpful"],
  ["Snooze 7 days", "snooze"],
  ["Don't show again", "off"],
];

export const SURFACE = "vscode";
export const FRAME_TIMEOUT_MS = 500;

/** Candidates for the aiq executable: the setting, PATH, then the usual install folders. */
export function executableCandidates(setting: string, env: NodeJS.ProcessEnv, platform: string): string[] {
  const exe = platform === "win32" ? "aiq.exe" : "aiq";
  const out: string[] = [];
  if (setting.trim()) out.push(setting.trim());
  const sep = platform === "win32" ? ";" : ":";
  for (const dir of (env.PATH ?? env.Path ?? "").split(sep).filter(Boolean)) out.push(path.join(dir, exe));
  if (platform === "win32" && env.LOCALAPPDATA) out.push(path.join(env.LOCALAPPDATA, "AIQ", "bin", exe));
  out.push(path.join(env.HOME ?? env.USERPROFILE ?? os.homedir(), ".local", "bin", exe));
  return out;
}

export function findExecutable(setting: string): string | undefined {
  return executableCandidates(setting, process.env, process.platform).find((p) => {
    try {
      return fs.statSync(p).isFile();
    } catch {
      return false;
    }
  });
}

export function runAiq(exe: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) =>
    execFile(exe, args, { timeout: 10_000, windowsHide: true }, (err, stdout) =>
      err ? reject(err) : resolve(stdout),
    ),
  );
}

export async function loadEndpoint(exe: string): Promise<Endpoint> {
  const e = JSON.parse(await runAiq(exe, ["endpoint"])) as Endpoint;
  if (!(e.socket || e.pipe) || !e.token) throw new Error("aiq endpoint: unexpected reply");
  return e;
}

/** Send one frame (token added here) and return the parsed reply line, or null on any failure. */
export function request<T>(ep: Endpoint, frame: Record<string, unknown>, timeoutMs = FRAME_TIMEOUT_MS): Promise<T | null> {
  let token: string;
  try {
    token = fs.readFileSync(ep.token, "utf8").trim();
  } catch {
    return Promise.resolve(null);
  }
  return new Promise((resolve) => {
    const sock = net.connect(ep.pipe ?? ep.socket ?? "");
    let buf = "";
    let done = false;
    const finish = (v: T | null) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      sock.destroy();
      resolve(v);
    };
    const timer = setTimeout(() => finish(null), timeoutMs);
    sock.setEncoding("utf8");
    sock.on("connect", () => sock.write(JSON.stringify({ ...frame, token }) + "\n"));
    sock.on("data", (d: string) => {
      buf += d;
      const nl = buf.indexOf("\n");
      if (nl < 0) return;
      try {
        finish(JSON.parse(buf.slice(0, nl)) as T);
      } catch {
        finish(null);
      }
    });
    sock.on("error", () => finish(null));
    sock.on("close", () => finish(null));
  });
}

export interface CoreClient {
  takeCard(): Promise<Card | null | undefined>; // undefined: core not reachable
  show(cardId: string): Promise<ShowReply | null>;
  /** Record an answer; false when the core refused or is not reachable ("AIQ isn't running"). */
  feedback(cardId: string, answer: string): Promise<boolean>;
  paused(): boolean;
}

export function coreClient(ep: Endpoint): CoreClient {
  return {
    async takeCard() {
      const r = await request<{ card: Card | null }>(ep, { op: "take_card_any", surface: SURFACE });
      return r === null ? undefined : r.card;
    },
    show: (cardId) => request<ShowReply>(ep, { op: "show", card_id: cardId, opened_on: SURFACE }),
    async feedback(cardId, answer) {
      const r = await request<{ ok: boolean }>(ep, { op: "tip_feedback", card_id: cardId, answer, surface: SURFACE });
      return r?.ok === true;
    },
    paused: () => fs.existsSync(ep.paused_flag),
  };
}

/** Card fields are shown as plain text: control characters removed (STRIDE B6: no markup, no command links). */
export function plain(s: string): string {
  return s.replace(/[\u0000-\u001f\u007f-\u009f]/g, " ").trim();
}

export function notificationText(c: Card): string {
  return `AIQ tip: ${plain(c.noticed)} Try this: ${plain(c.try_this)}`;
}

export function detailsText(r: ShowReply): string {
  if (!r.card) return "No card with that id.\n";
  const c = r.card;
  let out = `AIQ tip: ${plain(c.noticed)}\nWhy: ${plain(c.why)}\nTry this: ${plain(c.try_this)}\n\n`;
  if (c.opportunity_id === "next_rung") {
    if (c.starter) out += `Starter prompt (copy and fill in; AIQ never sends it):\n  ${plain(c.starter)}\n\n`;
    out += "AIQ guessed the kind of task from these earlier tasks of yours:\n";
    const l = r.leaned_on ?? [];
    if (l.length === 0) out += "  (none left: they were deleted)\n";
    l.forEach((x, i) => {
      out += `  ${i + 1}. ${x.ts}  ${plain(x.tool)}  ${plain(x.task_type)}\n`;
    });
    return out + "\nTurn off tips for now: AIQ: Pause\n";
  }
  const texts = r.evidence.map((e) => e.text);
  if (texts.every((t) => t === null)) return out + "The evidence for this card was deleted.\n";
  out += "The prompts it counted:\n";
  r.evidence.forEach((e, i) => {
    out += `  ${i + 1}. ${e.ts}  ${e.text === null ? "(deleted)" : plain(e.text)}\n`;
  });
  return out + "\nTurn off tips for now: AIQ: Pause\n";
}

export type State = "on" | "paused" | "down" | "tip";

export const POLL_MS = 2_000;
export const DOWN_POLL_MS = 10_000;

/**
 * One poll: never asks while paused; takes at most one card. Returns the state to show, the card to
 * notify (if any), and when to poll next. simplify: polling, not push. Ceiling: 0.5 frames/s per focused
 * window. Upgrade trigger: a second VS Code consumer of core events (Phase 3 feedback).
 */
export async function pollOnce(
  client: CoreClient,
  focused: boolean,
  waiting: Card | undefined,
): Promise<{ state: State; card?: Card; nextMs: number }> {
  if (client.paused()) return { state: "paused", nextMs: POLL_MS };
  if (!focused) return { state: waiting ? "tip" : "on", nextMs: POLL_MS };
  const card = await client.takeCard();
  if (card === undefined) return { state: waiting ? "tip" : "down", nextMs: DOWN_POLL_MS };
  if (card) return { state: "tip", card, nextMs: POLL_MS };
  return { state: waiting ? "tip" : "on", nextMs: POLL_MS };
}

export function statusText(s: State): { text: string; tooltip: string } {
  switch (s) {
    case "on":
      return { text: "AIQ on", tooltip: "AIQ watches quietly. Click for tips, pause or resume." };
    case "paused":
      return { text: "AIQ paused", tooltip: "Nothing is captured and no tips show. Click to resume." };
    case "down":
      return { text: "AIQ not running", tooltip: "Starts with your next agent prompt." };
    case "tip":
      return { text: "AIQ: 1 tip waiting", tooltip: "Click to read the tip." };
  }
}
