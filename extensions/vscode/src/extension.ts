// AIQ for VS Code (Phase 2 design, Exposed contract 6): status-bar item, a poll that shows a tip after a
// Copilot agent turn ends, and Pause / Resume / Show tip. Notifications are non-modal and never take focus.
import * as vscode from "vscode";
import { Card, CoreClient, FEEDBACK_BUTTONS, State, coreClient, detailsText, findExecutable, loadEndpoint, notificationText, pollOnce, runAiq, statusText } from "./core";

let timer: NodeJS.Timeout | undefined;

export async function activate(ctx: vscode.ExtensionContext): Promise<void> {
  const item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 50);
  item.command = "aiq.menu";
  ctx.subscriptions.push(item);
  const output = vscode.window.createOutputChannel("AIQ");
  ctx.subscriptions.push(output);
  let waiting: Card | undefined;
  let client: CoreClient | undefined;
  const exe = findExecutable(vscode.workspace.getConfiguration("aiq").get<string>("executable", ""));

  const show = (s: State) => {
    const t = statusText(s);
    item.text = t.text;
    item.tooltip = t.tooltip;
    item.accessibilityInformation = { label: t.text };
    item.show();
  };

  if (!exe) {
    item.text = "AIQ not installed";
    item.tooltip = "Set aiq.executable to the aiq program, or put it on PATH.";
    item.show();
    return;
  }
  try {
    client = coreClient(await loadEndpoint(exe));
  } catch {
    show("down");
  }

  const openDetails = async () => {
    if (!client || !waiting) {
      void vscode.window.showInformationMessage("AIQ: no tip waiting.");
      return;
    }
    const r = await client.show(waiting.card_id);
    output.clear();
    output.append(r ? detailsText(r) : "AIQ can't open its data right now. Run `aiq status`.\n");
    output.show(true); // preserveFocus: never take focus from the editor
    waiting = undefined;
    show("on");
  };
  const setPaused = async (on: boolean) => {
    await runAiq(exe, [on ? "pause" : "resume"]).catch(() => undefined);
    show(on ? "paused" : "on");
  };

  ctx.subscriptions.push(
    vscode.commands.registerCommand("aiq.showTip", openDetails),
    vscode.commands.registerCommand("aiq.pause", () => setPaused(true)),
    vscode.commands.registerCommand("aiq.resume", () => setPaused(false)),
    vscode.commands.registerCommand("aiq.menu", async () => {
      const paused = client?.paused() ?? false;
      const pick = await vscode.window.showQuickPick(
        [...(waiting ? ["Show tip"] : []), paused ? "Resume AIQ" : "Pause AIQ"],
        { placeHolder: "AIQ" },
      );
      if (pick === "Show tip") await openDetails();
      else if (pick === "Pause AIQ") await setPaused(true);
      else if (pick === "Resume AIQ") await setPaused(false);
    }),
  );

  const tick = async () => {
    if (!client) {
      try {
        client = coreClient(await loadEndpoint(exe));
      } catch {
        show("down");
        timer = setTimeout(tick, 10_000);
        return;
      }
    }
    const r = await pollOnce(client, vscode.window.state.focused, waiting);
    show(r.state);
    if (r.card) {
      waiting = r.card;
      const card = r.card;
      const labels = FEEDBACK_BUTTONS.map(([label]) => label);
      void vscode.window.showInformationMessage(notificationText(card), "Try this", ...labels).then(async (a) => {
        if (a === "Try this") return openDetails();
        const answer = FEEDBACK_BUTTONS.find(([label]) => label === a)?.[1];
        if (!answer || !client) return; // dismissed: no answer recorded
        const ok = await client.feedback(card.card_id, answer);
        if (!ok) void vscode.window.showInformationMessage("AIQ isn't running, so your answer wasn't saved.");
      });
    }
    timer = setTimeout(tick, r.nextMs);
  };
  void tick();
}

export function deactivate(): void {
  if (timer) clearTimeout(timer);
}
