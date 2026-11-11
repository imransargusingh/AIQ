// Your data (spec US-H6): everything captured, newest first; Pause; delete one prompt's text (design
// aiq-home-deferred-pages E1–E4); Delete everything (typed `forget`). Older-than waits for a later phase.
import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { bytes, clock, plural, toolName, when } from "../format";
import type { Diagnostics, Item } from "../types";
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
  const [diag, setDiag] = useState(false);
  const [saved, setSaved] = useState<{ name: string; folder: string } | null>(null);

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

  const diagBtn = (
    <button className="btn" onClick={() => setDiag(true)}>
      Save a diagnostics file…
    </button>
  );
  const savedLine = saved && (
    <p className="savedline" role="status">
      Saved <b>{saved.name}</b> in {saved.folder}. AIQ sent it nowhere.{" "}
      <button className="btnlink" onClick={() => api.showDiagnostics()}>
        Show in folder
      </button>
    </p>
  );
  const diagDialog = diag && (
    <DiagnosticsDialog
      onClose={() => setDiag(false)}
      onSaved={(name, folder) => {
        setDiag(false);
        setSaved({ name, folder });
        announce(`Saved ${name} in ${folder}. AIQ sent it nowhere.`);
      }}
    />
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
            {snap.status.paused ? "AIQ is paused, so it saves nothing new." : "New work will show here as you go."}
          </span>
          <div className="row">
            {pauseBtn}
            {diagBtn}
          </div>
        </div>
        {savedLine}
        <p className="meta">Stored in {d.location}</p>
        {diagDialog}
      </div>
    );
  }

  return (
    <div className="page wide">
      <Banners {...ctx} />
      <header className="group">
        <Title>Your data</Title>
        <p className="mute">Everything AIQ has saved, only on this computer, in a file only your account can read. It stays until you delete it.</p>
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
          {diagBtn}
          <button className="btn danger" onClick={() => setConfirm(true)}>Delete everything…</button>
        </div>
      </div>
      {savedLine}
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
                    {i.text ? <Content text={i.text} /> : i.deleted ? "Text deleted" : i.kind === "prompt" ? "No text saved" : "—"}
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

      {diagDialog}

      {confirm && (
        <Dialog label="Delete everything AIQ stored?" onClose={close}>
          <div className="dialog">
            <h2>Delete everything AIQ stored?</h2>
            <p>
              This deletes {plural(d.events, "event")}
              {d.imported_sessions > 0 && <>, {plural(d.imported_sessions, "imported session")}</>}, {plural(snap.tips.length, "tip")} and the prompts behind them. It can't be undone. Your tools and your work aren't affected.
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

/** Your data's "Save a diagnostics file" (design diagnostics-file): what is in the file and what is left out, one
 * "Include my prompts" choice (off), the file itself to read, and Save to Downloads. AIQ sends it nowhere. */
function DiagnosticsDialog({ onClose, onSaved }: { onClose: () => void; onSaved: (name: string, folder: string) => void }) {
  const [prompts, setPrompts] = useState(false);
  const [file, setFile] = useState<Diagnostics | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    setFile(null);
    setErr(null);
    api.diagnostics(prompts).then(
      (f) => live && setFile(f),
      (e: unknown) => live && setErr(`Couldn't make the file. (${e instanceof Error ? e.message : String(e)})`),
    );
    return () => {
      live = false;
    };
  }, [prompts]);

  const save = async () => {
    setBusy(true);
    const r = await api.saveDiagnostics(prompts);
    setBusy(false);
    if (r.ok) onSaved(r.name, r.folder);
    else setErr(r.message ?? "Couldn't save the file.");
  };

  return (
    <Dialog label="Save a diagnostics file" onClose={onClose}>
      <div className="dialog diag">
        <h2>Save a diagnostics file</h2>
        <p>How AIQ ran on this computer over the last 14 days, so a problem can be found.</p>
        <div className="diagcols">
          <div>
            <h3 className="meta">In the file</h3>
            <ul>
              <li>AIQ and tool versions, settings</li>
              <li>Each task's kind, rung and confidence</li>
              <li>Why each tip showed or didn't</li>
              <li>Label and analysis call errors</li>
              <li>AIQ's own log warnings</li>
              {prompts && <li>Your prompts, secrets removed</li>}
            </ul>
          </div>
          <div>
            <h3 className="meta">Left out</h3>
            <ul>
              {!prompts && <li>Your prompts</li>}
              <li>The agent's replies</li>
              <li>File and folder names</li>
              <li>Your name, account, computer name</li>
              <li>What the analysis wrote about a task</li>
              <li>Tip and team tip wording</li>
            </ul>
          </div>
        </div>
        <label className="row check">
          <input type="checkbox" checked={prompts} onChange={(e) => setPrompts(e.target.checked)} /> Include my prompts
        </label>
        {prompts && <p className="meta warnline">Prompts can name projects, people or customers. Read them below before you share.</p>}
        {file ? (
          <details>
            <summary>
              See the file{" "}
              <span className="mute">
                ({plural(file.tasks, "task")}
                {prompts && <>, {plural(file.prompts, "prompt")}</>}, {bytes(file.bytes)})
              </span>
            </summary>
            <pre className="preview" tabIndex={0} aria-label="The diagnostics file">
              {file.text}
            </pre>
          </details>
        ) : (
          !err && <p className="meta" role="status">Reading…</p>
        )}
        {err && (
          <p className="errtext" role="alert">
            {err}
          </p>
        )}
        <p className="meta">AIQ sends this file nowhere. Sharing it is up to you.</p>
        <div className="row" style={{ justifyContent: "flex-end" }}>
          <button className="btn" onClick={onClose}>Cancel</button>
          <button className="btn pri" data-autofocus disabled={!file || busy} onClick={save}>
            Save to Downloads
          </button>
        </div>
      </div>
    </Dialog>
  );
}
