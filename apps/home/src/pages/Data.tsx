// Your data (spec US-H6): everything captured, newest first; Pause; delete one prompt's text (design
// aiq-home-deferred-pages E1–E4); Delete everything (typed `forget`). Older-than waits for a later phase.
import { useRef, useState } from "react";
import { api } from "../api";
import { bytes, clock, plural, toolName, when } from "../format";
import type { Item } from "../types";
import { Banners, Content, Dialog, Title, preview, type Ctx } from "../ui";

export function Data(ctx: Ctx) {
  const { snap, announce, togglePause, refresh } = ctx;
  const d = snap.data;
  const [confirm, setConfirm] = useState(false);
  const [typed, setTyped] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [deletedAt, setDeletedAt] = useState<number | null>(null);
  const [one, setOne] = useState<Item | null>(null);
  const [oneErr, setOneErr] = useState<string | null>(null);
  const table = useRef<HTMLTableElement>(null);

  const closeOne = () => {
    setOne(null);
    setOneErr(null);
  };
  const delOne = async () => {
    if (!one) return;
    const r = await api.forgetItem(one.event_id);
    if (r.ok) {
      closeOne();
      announce("Prompt deleted. Its text is gone from this computer.");
      refresh();
      // The Delete button that opened the dialog goes away with the text; keep focus on the table.
      requestAnimationFrame(() => table.current?.focus());
    } else setOneErr(r.message);
  };

  const close = () => {
    setConfirm(false);
    setTyped("");
    setErr(null);
  };
  const del = async () => {
    const r = await api.forgetAll(typed.trim());
    if (r.ok) {
      setDeletedAt(Date.now());
      close();
      announce("Deleted. AIQ has no stored history. Capture is still on.");
      refresh();
    } else setErr(r.message);
  };

  const pauseBtn = (
    <button className="btn" onClick={togglePause}>
      {snap.status.paused ? "Resume AIQ" : "Pause AIQ"} <kbd>P</kbd>
    </button>
  );

  if (d.events === 0 && d.imported_sessions === 0) {
    return (
      <div className="page">
        <Banners {...ctx} />
        <Title>Your data</Title>
        <div className="empty">
          <h2>AIQ has no stored history.</h2>
          <span className="mute">
            {deletedAt ? `Deleted at ${clock(deletedAt, snap.offset_min)}. ` : ""}
            {snap.status.paused ? "AIQ is paused, so nothing new is captured." : "Capture is still on, so new activity will show here."}
          </span>
          {pauseBtn}
        </div>
        <p className="meta">Stored in {d.location}</p>
      </div>
    );
  }

  return (
    <div className="page wide">
      <Banners {...ctx} />
      <header className="group">
        <Title>Your data</Title>
        <p className="mute">Everything AIQ captured, stored only on this computer in a file only your account can read. It stays until you delete it.</p>
      </header>
      <div className="head">
        <p>
          <b className="num">{plural(d.events, "event")}</b>{" "}
          <span className="mute">
            ({plural(d.prompts, "prompt")}, {plural(d.turn_ends, "turn end")})
            {d.imported_sessions > 0 && <> · {plural(d.imported_sessions, "imported session")}</>} · {bytes(d.store_bytes)}
          </span>
        </p>
        <div className="row">
          {pauseBtn}
          <button className="btn danger" onClick={() => setConfirm(true)}>Delete everything…</button>
        </div>
      </div>
      {d.items.length > 0 && (
        <div className="tablewrap">
          <table aria-label="Captured events, newest first" ref={table} tabIndex={-1}>
            <thead>
              <tr>
                <th>When</th>
                <th>Tool</th>
                <th>Kind</th>
                <th>Content</th>
                <th>
                  <span className="sr">Delete</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {d.items.map((i) => (
                <tr key={i.event_id}>
                  <td className="num" style={{ whiteSpace: "nowrap" }}>{when(i.ts_ms, snap.now_ms, snap.offset_min)}</td>
                  <td style={{ whiteSpace: "nowrap" }}>{toolName(i.tool)}</td>
                  <td>{i.kind === "prompt" ? "Prompt" : "Turn ended"}</td>
                  <td className={i.text ? "wrap" : "mute"}>
                    {i.text ? <Content text={i.text} /> : i.deleted ? "Text deleted" : i.kind === "prompt" ? "No text captured" : "—"}
                  </td>
                  <td>
                    {i.kind === "prompt" && i.text != null && (
                      <button className="btn small" aria-label={`Delete prompt from ${when(i.ts_ms, snap.now_ms, snap.offset_min)}`} onClick={() => setOne(i)}>
                        Delete
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="meta">
        {d.items_capped && <>Showing the newest {d.items.length.toLocaleString("en-GB")}. </>}
        {d.imported_sessions > 0 && <>Imported sessions ({plural(d.imported_events, "turn")}) are counted above, not listed. </>}
        Stored in {d.location}. Read in {snap.status.read_ms} ms.
      </p>

      {one && one.text != null && (
        <Dialog label="Delete this prompt?" onClose={closeOne}>
          <div className="dialog">
            <h2>Delete this prompt?</h2>
            <p className="quote">{preview(one.text, 160)}</p>
            <p className="meta">
              {toolName(one.tool)} · {when(one.ts_ms, snap.now_ms, snap.offset_min)}
            </p>
            <p>
              AIQ deletes this prompt's text from this computer. It keeps only that a prompt was sent, when, and from which tool and folder, so
              your task counts stay right. This can't be undone.
            </p>
            {oneErr && (
              <p className="errtext" role="alert">
                {oneErr}
              </p>
            )}
            <div className="row" style={{ justifyContent: "flex-end" }}>
              <button className="btn" data-autofocus onClick={closeOne}>Cancel</button>
              <button className="btn danger" onClick={delOne}>Delete prompt</button>
            </div>
          </div>
        </Dialog>
      )}

      {confirm && (
        <Dialog label="Delete everything AIQ stored?" onClose={close}>
          <div className="dialog">
            <h2>Delete everything AIQ stored?</h2>
            <p>
              This deletes {plural(d.events, "event")}
              {d.imported_sessions > 0 && <>, {plural(d.imported_sessions, "imported session")}</>}, {plural(snap.tips.length, "tip")} and their evidence. It can't be undone. Your tools and your work aren't affected.
            </p>
            <div className="field">
              <label htmlFor="forget">Type forget to confirm</label>
              <input
                id="forget"
                className={`input${err ? " err" : ""}`}
                value={typed}
                autoComplete="off"
                spellCheck={false}
                aria-describedby={err ? "forgeterr" : undefined}
                onChange={(e) => setTyped(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && typed.trim() === "forget" && del()}
              />
              {err && <span className="errtext" id="forgeterr">{err}</span>}
            </div>
            <div className="row" style={{ justifyContent: "flex-end" }}>
              <button className="btn" data-autofocus onClick={close}>Cancel</button>
              <button className="btn danger" disabled={typed.trim() !== "forget"} onClick={del}>Delete everything</button>
            </div>
          </div>
        </Dialog>
      )}
    </div>
  );
}
