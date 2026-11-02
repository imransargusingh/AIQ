//! `aiq core`: single instance per OS user. One listener thread hands parsed frames to one writer thread
//! over a bounded channel; the writer owns the store.
use crate::counters::Counters;
use crate::ipc;
use crate::pack::BundledPack;
use crate::paths::{Paths, write_user_only};
use crate::service::{SETTING_PAUSED, Service, TipLineOut};
use crate::spool;
use crate::sysclock::SystemClock;
use aiq_contracts::card::CardV1;
use aiq_contracts::codes;
use aiq_contracts::event::EventV1;
use aiq_contracts::frames::{MAX_FRAME_BYTES, OkReply, Request, ShowReply, StatusReply, TakeCardReply, TipLineReply};
use aiq_domain::PackSource;
use aiq_store::Store;
use interprocess::local_socket::traits::ListenerExt as _;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

pub const CHANNEL_CAPACITY: usize = 1024;
/// simplify: a thread per connection, capped. Ceiling: a same-user process holding 64 idle connections
/// makes hooks fail open (accepted threat: same-user software). Upgrade trigger: Phase 2 VS Code streams.
pub const MAX_CONNECTIONS: usize = 64;
const TICK: Duration = Duration::from_secs(30);
const CLI_TIMEOUT: Duration = Duration::from_secs(5);
/// Renderers (status line, VS Code poll) wait at most this long; they are never queued behind a full channel.
pub const RENDER_WAIT: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq)]
pub struct CoreError {
    pub code: &'static str,
    pub detail: String,
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

pub enum Cmd {
    TipFeedback {
        card_id: Option<String>,
        tip_id: Option<String>,
        answer: String,
        surface: String,
        reply: mpsc::Sender<Result<(), String>>,
    },
    GuidanceFlag {
        job: String,
        task_type: String,
        move_id: String,
        action: String,
        reason: Option<String>,
        note: Option<String>,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Ingest {
        event: EventV1,
        trace_id: Option<String>,
    },
    TakeCard {
        event: EventV1,
        trace_id: Option<String>,
        received: Instant,
        wait_ms: u64,
        reply: mpsc::Sender<Option<CardV1>>,
    },
    Status {
        reply: mpsc::Sender<StatusReply>,
    },
    Show {
        card_id: String,
        opened_on: String,
        reply: mpsc::Sender<ShowReply>,
    },
    TipLine {
        surface: String,
        session: Option<String>,
        reply: mpsc::Sender<TipLineReply>,
    },
    TakeCardAny {
        surface: String,
        received: Instant,
        reply: mpsc::Sender<Option<CardV1>>,
    },
    Setting {
        key: String,
        value: String,
        reply: mpsc::Sender<Result<(), String>>,
    },
    IncludeFolder {
        folder: String,
        reply: mpsc::Sender<Result<(), String>>,
    },
    ForgetItem {
        event_id: String,
        reply: mpsc::Sender<Result<(), String>>,
    },
    DrainSpool,
    Shutdown {
        reply: mpsc::Sender<()>,
    },
}

/// Run the core until it is killed or told to forget everything.
pub fn run(paths: &Paths) -> Result<(), CoreError> {
    let fail = |code: &'static str| move |e: std::io::Error| CoreError { code, detail: e.to_string() };
    paths.ensure_dirs().map_err(fail(codes::STORE_OPEN))?;
    let mut lock_opts = std::fs::OpenOptions::new();
    lock_opts.create(true).truncate(false).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        lock_opts.mode(0o600);
    }
    let lock = lock_opts.open(paths.core_lock()).map_err(fail(codes::CORE_ALREADY_RUNNING))?;
    if lock.try_lock().is_err() {
        return Err(CoreError { code: codes::CORE_ALREADY_RUNNING, detail: "another aiq core holds the lock".into() });
    }
    crate::logging::init(&paths.log());
    let counters = Arc::new(Counters::default());
    let started = (|| {
        let pack = BundledPack.load().map_err(|d| CoreError { code: codes::STORE_OPEN, detail: d })?;
        let store = Store::open(&paths.db()).map_err(|e| CoreError { code: e.code, detail: e.detail })?;
        let svc = Service::new(store, pack, SystemClock, counters.clone())
            .map_err(|e| CoreError { code: e.code, detail: e.detail })?;
        Ok::<_, CoreError>(svc)
    })();
    let mut svc = match started {
        Ok(s) => s,
        Err(e) => {
            error!(error.code = e.code, "core failed to start");
            let _ = write_user_only(&paths.core_error(), format!("{e}\n").as_bytes());
            return Err(e);
        }
    };
    let _ = std::fs::remove_file(paths.core_error());
    reconcile_pause(paths, &mut svc);

    let token = ipc::new_token();
    write_user_only(&paths.token(), token.as_bytes()).map_err(fail(codes::STORE_OPEN))?;
    let listener = ipc::listen(paths).map_err(fail(codes::CORE_ALREADY_RUNNING))?;
    info!(pid = std::process::id(), "core started");

    let (tx, rx) = mpsc::sync_channel::<Cmd>(CHANNEL_CAPACITY);
    let writer_paths = paths.clone();
    let writer_counters = counters.clone();
    let writer = std::thread::spawn(move || writer_loop(svc, rx, &writer_paths, &writer_counters));
    // Drain what hooks spooled while the core was down before serving new frames.
    let _ = tx.send(Cmd::DrainSpool);

    let active = Arc::new(AtomicUsize::new(0));
    for conn in listener.incoming() {
        let Ok(stream) = conn else { continue };
        if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
            drop(stream);
            continue;
        }
        active.fetch_add(1, Ordering::SeqCst);
        let (tx, token, counters, active, paths) =
            (tx.clone(), token.clone(), counters.clone(), active.clone(), paths.clone());
        std::thread::spawn(move || {
            handle(stream, &token, &tx, &counters, &paths);
            active.fetch_sub(1, Ordering::SeqCst);
        });
        if writer.is_finished() {
            break;
        }
    }
    Ok(())
}

/// The hook honours the pause flag file even with the core down; make the stored setting agree with it.
fn reconcile_pause(paths: &Paths, svc: &mut Service<SystemClock>) {
    let flag = paths.pause_flag().exists();
    if flag != svc.paused() {
        let _ = svc.set(SETTING_PAUSED, if flag { "true" } else { "false" });
    }
}

fn writer_loop(mut svc: Service<SystemClock>, rx: Receiver<Cmd>, paths: &Paths, counters: &Counters) {
    let shutdown = loop {
        let cmd = match rx.recv_timeout(TICK) {
            Ok(c) => c,
            Err(mpsc::RecvTimeoutError::Timeout) => Cmd::DrainSpool,
            Err(mpsc::RecvTimeoutError::Disconnected) => break None,
        };
        match cmd {
            Cmd::Ingest { event, trace_id } => svc.ingest(&event, trace_id.as_deref()),
            Cmd::TakeCard { event, trace_id, received, wait_ms, reply } => {
                let fresh = received.elapsed() <= Duration::from_millis(wait_ms);
                let card = svc.take_card(&event, trace_id.as_deref(), fresh);
                let _ = reply.send(card);
            }
            Cmd::Status { reply } => {
                let _ = reply.send(svc.status());
            }
            Cmd::Show { card_id, opened_on, reply } => {
                let _ = reply.send(svc.show(&card_id, &opened_on));
            }
            Cmd::TipLine { surface, session, reply } => {
                let r = match svc.tip_line(&surface, session.as_deref()) {
                    Some(TipLineOut::Full(c)) => TipLineReply { card: Some(c), waiting_card_id: None },
                    Some(TipLineOut::Waiting(id)) => TipLineReply { card: None, waiting_card_id: Some(id) },
                    None => TipLineReply::default(),
                };
                let _ = reply.send(r);
            }
            Cmd::TakeCardAny { surface, received, reply } => {
                // A card handed to a poller that already gave up would be lost; deliver only while it waits.
                let card = if received.elapsed() <= RENDER_WAIT { svc.take_card_any(&surface) } else { None };
                let _ = reply.send(card);
            }
            Cmd::Setting { key, value, reply } => {
                let _ = reply.send(svc.set(&key, &value));
            }
            Cmd::IncludeFolder { folder, reply } => {
                let r = match svc.include_folder(&folder) {
                    Ok(true) => Ok(()),
                    Ok(false) => Err("that folder is not set aside".into()),
                    Err(e) => Err(e.code.into()),
                };
                let _ = reply.send(r);
            }
            Cmd::ForgetItem { event_id, reply } => {
                let _ = reply.send(svc.forget_item(&event_id));
            }
            Cmd::TipFeedback { card_id, tip_id, answer, surface, reply } => {
                let _ = reply.send(svc.tip_feedback(card_id.as_deref(), tip_id.as_deref(), &answer, &surface));
            }
            Cmd::GuidanceFlag { job, task_type, move_id, action, reason, note, reply } => {
                let _ = reply.send(svc.guidance_flag(
                    &job,
                    &task_type,
                    &move_id,
                    &action,
                    reason.as_deref(),
                    note.as_deref(),
                ));
            }
            Cmd::DrainSpool => {
                drain_spool(paths, &mut svc, counters);
                svc.prune();
                if let Err(e) = svc.expire_stale() {
                    counters.inc(e.code);
                }
            }
            Cmd::Shutdown { reply } => break Some(reply),
        }
    };
    drop(svc); // closes the store before anyone deletes it
    if let Some(r) = shutdown {
        let _ = r.send(());
    }
}

pub fn drain_spool<C: aiq_domain::clock::Clock>(paths: &Paths, svc: &mut Service<C>, counters: &Counters) {
    let lines = match spool::take(&paths.spool(), &paths.spool_draining()) {
        Ok(l) => l,
        Err(e) => {
            warn!(error = %e, "spool drain failed");
            return;
        }
    };
    if lines.is_empty() {
        return;
    }
    let (frames, bad) = spool::parse(&lines);
    for _ in 0..bad {
        counters.inc(codes::SPOOL_BAD_LINE);
    }
    info!(lines = frames.len(), bad, "spool drained");
    for f in frames {
        svc.ingest(&f.event, f.trace_id.as_deref());
    }
}

fn to_line(v: &impl serde::Serialize) -> String {
    serde_json::to_string(v).map(|s| format!("{s}\n")).unwrap_or_default()
}

/// Parse a frame and check its token. A bad frame or token is counted and gets no reply.
pub fn authorize(line: &str, token: &str, counters: &Counters) -> Option<Request> {
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(_) => {
            counters.inc(codes::INGEST_SHAPE);
            return None;
        }
    };
    if !ipc::token_eq(req.token(), token) {
        counters.inc(codes::API_AUTH);
        return None; // closed without reply, nothing stored
    }
    Some(req)
}

#[derive(Debug, PartialEq)]
pub enum Dispatch {
    NoReply,
    Reply(String),
    Forget,
}

/// Route one authorized request to the writer and build the reply line.
pub fn dispatch(req: Request, tx: &SyncSender<Cmd>, counters: &Counters, received: Instant) -> Dispatch {
    fn ask<T>(tx: &SyncSender<Cmd>, mk: impl FnOnce(mpsc::Sender<T>) -> Cmd) -> Option<T> {
        let (rtx, rrx) = mpsc::channel();
        tx.send(mk(rtx)).ok()?;
        rrx.recv_timeout(CLI_TIMEOUT).ok()
    }
    match req {
        Request::Ingest { event, trace_id, .. } => {
            if let Err(TrySendError::Full(_)) = tx.try_send(Cmd::Ingest { event, trace_id }) {
                counters.inc(codes::INGEST_BACKPRESSURE);
            }
            Dispatch::NoReply
        }
        Request::TakeCard { event, trace_id, wait_ms, .. } => {
            let (rtx, rrx) = mpsc::channel();
            let card = match tx.try_send(Cmd::TakeCard { event, trace_id, received, wait_ms, reply: rtx }) {
                Ok(()) => rrx.recv_timeout(Duration::from_millis(wait_ms.max(1))).ok().flatten(),
                Err(_) => None, // never queued behind a full channel
            };
            Dispatch::Reply(to_line(&TakeCardReply { card }))
        }
        Request::Status { .. } => {
            ask(tx, |reply| Cmd::Status { reply }).map_or(Dispatch::NoReply, |s| Dispatch::Reply(to_line(&s)))
        }
        Request::Show { card_id, opened_on, .. } => {
            let opened_on = opened_on.filter(|s| s == "vscode").unwrap_or_else(|| "cli".into());
            ask(tx, |reply| Cmd::Show { card_id, opened_on, reply })
                .map_or(Dispatch::NoReply, |s| Dispatch::Reply(to_line(&s)))
        }
        Request::TipLine { surface, session, .. } => {
            let (rtx, rrx) = mpsc::channel();
            let r = match tx.try_send(Cmd::TipLine { surface, session, reply: rtx }) {
                Ok(()) => rrx.recv_timeout(RENDER_WAIT).unwrap_or_default(),
                Err(_) => TipLineReply::default(),
            };
            Dispatch::Reply(to_line(&r))
        }
        Request::TakeCardAny { surface, .. } => {
            let (rtx, rrx) = mpsc::channel();
            let card = match tx.try_send(Cmd::TakeCardAny { surface, received, reply: rtx }) {
                Ok(()) => rrx.recv_timeout(RENDER_WAIT).ok().flatten(),
                Err(_) => None,
            };
            Dispatch::Reply(to_line(&TakeCardReply { card }))
        }
        Request::Setting { key, value, .. } => {
            let r = ask(tx, |reply| Cmd::Setting { key, value, reply })
                .unwrap_or_else(|| Err("core did not answer".into()));
            Dispatch::Reply(to_line(&OkReply { ok: r.is_ok(), message: r.err() }))
        }
        Request::IncludeFolder { folder, .. } => {
            let r = ask(tx, |reply| Cmd::IncludeFolder { folder, reply })
                .unwrap_or_else(|| Err("core did not answer".into()));
            Dispatch::Reply(to_line(&OkReply { ok: r.is_ok(), message: r.err() }))
        }
        Request::ForgetItem { event_id, .. } => {
            let r = ask(tx, |reply| Cmd::ForgetItem { event_id, reply })
                .unwrap_or_else(|| Err("core did not answer".into()));
            Dispatch::Reply(to_line(&OkReply { ok: r.is_ok(), message: r.err() }))
        }
        Request::TipFeedback { card_id, tip_id, answer, surface, .. } => {
            let r = ask(tx, |reply| Cmd::TipFeedback { card_id, tip_id, answer, surface, reply })
                .unwrap_or_else(|| Err("core did not answer".into()));
            Dispatch::Reply(to_line(&OkReply { ok: r.is_ok(), message: r.err() }))
        }
        Request::GuidanceFlag { job, task_type, move_id, action, reason, note, .. } => {
            let r = ask(tx, |reply| Cmd::GuidanceFlag { job, task_type, move_id, action, reason, note, reply })
                .unwrap_or_else(|| Err("core did not answer".into()));
            Dispatch::Reply(to_line(&OkReply { ok: r.is_ok(), message: r.err() }))
        }
        Request::Forget { .. } => Dispatch::Forget,
    }
}

fn handle(
    mut stream: interprocess::local_socket::Stream,
    token: &str,
    tx: &SyncSender<Cmd>,
    counters: &Counters,
    paths: &Paths,
) {
    let received = Instant::now();
    let Ok(Some(line)) = ipc::read_line_capped(&mut stream, MAX_FRAME_BYTES) else { return };
    let Some(req) = authorize(&line, token, counters) else { return };
    match dispatch(req, tx, counters, received) {
        Dispatch::NoReply => {}
        Dispatch::Reply(l) => {
            let _ = stream.write_all(l.as_bytes());
            let _ = stream.flush();
        }
        Dispatch::Forget => {
            let (rtx, rrx) = mpsc::channel();
            let stopped = tx.send(Cmd::Shutdown { reply: rtx }).is_ok() && rrx.recv_timeout(CLI_TIMEOUT).is_ok();
            let r =
                if stopped { forget_files(paths).map_err(|e| e.to_string()) } else { Err("core did not stop".into()) };
            let _ = stream.write_all(to_line(&OkReply { ok: r.is_ok(), message: r.err() }).as_bytes());
            let _ = stream.flush();
            info!("forget --all done; core exiting");
            std::process::exit(0);
        }
    }
}

/// `forget --all`: delete the store and the spool. Logs hold no prompt text and are kept.
pub fn forget_files(paths: &Paths) -> std::io::Result<()> {
    aiq_store::delete_store_files(&paths.db())?;
    for p in [paths.spool(), paths.spool_draining(), paths.token()] {
        match std::fs::remove_file(&p) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}
