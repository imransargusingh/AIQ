// The task page's "Tip for this task" card (design task-tips, Imran 2026-11-12): the tip shown for the task, with
// where, when and the person's answer; else the tip AIQ held back and why; else why there was no tip. Every line
// comes from the store (TaskDetail.tips). A team tip's "Tool to try" line is read from the team file by tip id, as
// Tip history reads it; its text is never stored.
import { plural, sentence, toolName, when } from "./format";
import { TeamToolLine, teamTool } from "./team";
import type { HeldTip, Snapshot, TaskAnalysis, TaskRow, TaskTips, TipRow } from "./types";
import { Chip } from "./ui";

/** What the card shows: one view per task, chosen in this order. */
export type TipView =
  | { kind: "shown"; tips: TipRow[] }
  | { kind: "held"; tip: HeldTip }
  | { kind: "none"; why: string; help: string };

export function tipView(t: TaskTips, task: Pick<TaskRow, "origin">, analysis: Pick<TaskAnalysis, "state">): TipView {
  if (t.shown.length > 0) return { kind: "shown", tips: t.shown };
  if (t.held) return { kind: "held", tip: t.held };
  if (task.origin === "imported") return { kind: "none", why: "AIQ gives tips while you work, not for tasks it imported.", help: "" };
  // A label still on its way while the task is open is not "never came back" yet.
  if ((t.step === "label_asked" || t.step === "label_retry") && analysis.state === "open")
    return { kind: "none", why: "AIQ is still working out what kind of task this is.", help: "" };
  if (t.step && t.step !== "decided") return { kind: "none", why: `${sentence(t.step_words)}.`, help: t.step_help };
  return { kind: "none", why: "AIQ didn't record why for this task.", help: "" };
}

/** The person's answer, as the tip's own buttons say it. */
export function answerWords(a: TipRow["answer"]): string | null {
  switch (a) {
    case "helpful":
      return "You said: Helpful";
    case "not_helpful":
      return "You said: Not helpful";
    case "snooze":
      return "You snoozed it";
    case "off":
      return "You turned it off";
    default:
      return null;
  }
}

export function TaskTipsCard({ tips, task, analysis, snap }: { tips: TaskTips; task: TaskRow; analysis: TaskAnalysis; snap: Snapshot }) {
  const v = tipView(tips, task, analysis);
  return (
    <section className="card group" aria-labelledby="tt">
      {v.kind === "shown" &&
        v.tips.map((t, i) => {
          const answer = answerWords(t.answer);
          return (
            <div key={t.card_id} className="group">
              <div className="row" style={{ justifyContent: "space-between" }}>
                {i === 0 ? <h2 id="tt">Tip for this task</h2> : <span />}
                <span className="meta">
                  Shown in {toolName(t.tool)} · {when(t.delivered_ms, snap.now_ms, snap.offset_min)}
                </span>
              </div>
              <p>{sentence(t.noticed)}</p>
              <p className="mute">Try this: {t.try_this}</p>
              <TeamToolLine t={teamTool(snap, t.tip_id)} />
              <div className="row">
                <Chip>{t.opened ? "You opened it" : "Not opened"}</Chip>
                {answer && <Chip tone={t.answer === "helpful" ? "ok" : undefined}>{answer}</Chip>}
                {t.after && t.after.checked > 0 && (
                  <Chip tone={t.after.seen * 2 > t.after.checked ? "ok" : undefined}>
                    Did it in {t.after.seen} of the next {plural(t.after.checked, "task")}
                  </Chip>
                )}
              </div>
            </div>
          );
        })}
      {v.kind === "held" && (
        <>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <h2 id="tt">Tip for this task</h2>
            <Chip tone="warn">{v.tip.outcome === "show_ready" ? "Ready, not shown yet" : "Ready, not shown"}</Chip>
          </div>
          {v.tip.noticed && <p className="mute">{sentence(v.tip.noticed)}</p>}
          {v.tip.try_this && <p className="mute">Try this: {v.tip.try_this}</p>}
          {v.tip.outcome !== "show_ready" && (
            <p className="meta">
              Not shown: {v.tip.reason_words}. {v.tip.reason_help}
            </p>
          )}
        </>
      )}
      {v.kind === "none" && (
        <>
          <h2 id="tt">Tip for this task</h2>
          <p>
            <b>No tip:</b> {v.why}
          </p>
          {v.help && <p className="mute">{v.help}</p>}
        </>
      )}
    </section>
  );
}
