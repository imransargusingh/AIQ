//! Sync between one person's computers (schema v12): change files cut on one store and applied on another.
use aiq_domain::clock::Now;
use aiq_store::sync::{Applied, Segment};
use aiq_store::*;
use tempfile::TempDir;

fn open() -> (TempDir, Store) {
    let d = TempDir::new().unwrap();
    let s = Store::open(&d.path().join("aiq.db")).unwrap();
    (d, s)
}

fn ev<'a>(id: &'a str, session: &'a str, text: Option<&'a str>, ts: i64) -> NewEvent<'a> {
    NewEvent {
        event_id: id,
        surface: "copilot-cli",
        session,
        kind: "prompt",
        ts_utc_ms: ts,
        prompt_id: None,
        trace_id: None,
        collector_version: "0.1.0",
        prompt_chars: text.map(|t| t.len() as i64),
        payload_meta: "{}",
        text,
    }
}

/// Cut the next change file, as the sync folder would hold it (through the JSON lines text, like a real file).
fn cut(s: &mut Store) -> Option<Segment> {
    let mut text = String::new();
    s.sync_export(
        10_000,
        |seg| {
            text = seg.to_jsonl("test");
            Ok(())
        },
        100,
    )
    .unwrap()
    .map(|_| Segment::from_jsonl(&text).unwrap())
}

fn count(s: &Store, sql: &str) -> i64 {
    s.raw().query_row(sql, [], |r| r.get(0)).unwrap()
}

fn feedback(s: &Store, tip: &str) {
    s.insert_tip_feedback(&TipFeedbackRow {
        card_id: None,
        tip_id: tip.into(),
        answer: "helpful".into(),
        surface: "cli".into(),
        utc_ms: 5,
    })
    .unwrap();
}

#[test]
fn each_store_has_its_own_computer_id() {
    let (_a, a) = open();
    let (_b, b) = open();
    let (ia, ib) = (a.device_id().unwrap(), b.device_id().unwrap());
    assert_eq!(ia.len(), 32);
    assert_ne!(ia, ib);
}

#[test]
fn rows_recorded_on_one_computer_arrive_on_the_other_tagged_with_where_they_came_from() {
    let (_a, mut a) = open();
    let (_b, mut b) = open();
    a.insert_event(&ev("e1", "s1", Some("fix the export test"), 1000), 1000).unwrap();
    a.insert_event(&ev("e2", "s1", None, 2000), 2000).unwrap();
    let seg = cut(&mut a).expect("new rows make a change file");
    assert_eq!(seg.seq, 1);
    assert_eq!(seg.device, a.device_id().unwrap());

    assert_eq!(b.sync_apply(&seg, 50).unwrap(), Applied::Done { rows: 4, deletes: 0 }); // 1 session, 2 events, 1 text
    let who: String = b.raw().query_row("SELECT computer FROM event WHERE event_id = 'e1'", [], |r| r.get(0)).unwrap();
    assert_eq!(who, a.device_id().unwrap());
    let text: String = b.raw().query_row("SELECT text FROM content WHERE event_id = 'e1'", [], |r| r.get(0)).unwrap();
    assert_eq!(text, "fix the export test");
    // What arrived is never sent on: B has nothing of its own to send.
    assert!(cut(&mut b).is_none());
    assert_eq!(b.sync_applied_upto(&a.device_id().unwrap()).unwrap(), 1);
}

#[test]
fn a_change_file_applies_once_in_order_and_only_new_rows_are_cut_next_time() {
    let (_a, mut a) = open();
    let (_b, mut b) = open();
    a.insert_event(&ev("e1", "s1", Some("one"), 1), 1).unwrap();
    let s1 = cut(&mut a).unwrap();
    a.insert_event(&ev("e2", "s1", Some("two"), 2), 2).unwrap();
    let s2 = cut(&mut a).unwrap();
    assert_eq!(s2.seq, 2);
    assert_eq!(s2.lines.len(), 2, "only e2 and its text, not e1 again");
    assert!(cut(&mut a).is_none(), "nothing new, no file");

    assert_eq!(b.sync_apply(&s2, 1).unwrap(), Applied::Gap { want: 1 });
    assert_eq!(count(&b, "SELECT count(*) FROM event"), 0, "a gap applies nothing");
    assert!(matches!(b.sync_apply(&s1, 1).unwrap(), Applied::Done { .. }));
    assert!(matches!(b.sync_apply(&s2, 1).unwrap(), Applied::Done { .. }));
    assert_eq!(b.sync_apply(&s2, 1).unwrap(), Applied::Already);
    assert_eq!(count(&b, "SELECT count(*) FROM event"), 2);
}

#[test]
fn a_file_from_a_newer_aiq_is_held_not_guessed_at() {
    let (_a, mut a) = open();
    let (_b, mut b) = open();
    a.insert_event(&ev("e1", "s1", Some("one"), 1), 1).unwrap();
    let mut seg = cut(&mut a).unwrap();
    seg.schema = 99;
    assert_eq!(b.sync_apply(&seg, 1).unwrap(), Applied::Held { schema: 99 });
    assert_eq!(count(&b, "SELECT count(*) FROM event"), 0);
    assert_eq!(b.sync_applied_upto(&a.device_id().unwrap()).unwrap(), 0, "tried again after the update");
}

#[test]
fn a_failed_write_marks_nothing_and_the_same_file_is_cut_again() {
    let (_a, mut a) = open();
    a.insert_event(&ev("e1", "s1", Some("one"), 1), 1).unwrap();
    let r = a.sync_export(100, |_| Err(std::io::Error::other("drive gone")), 1);
    assert_eq!(r.unwrap_err().code, aiq_contracts::codes::SYNC_WRITE);
    let seg = cut(&mut a).unwrap();
    assert_eq!(seg.seq, 1);
    assert_eq!(seg.lines.len(), 3);
}

#[test]
fn a_cut_short_file_is_an_error_not_a_partial_apply() {
    let (_a, mut a) = open();
    a.insert_event(&ev("e1", "s1", Some("one"), 1), 1).unwrap();
    let mut text = String::new();
    a.sync_export(
        100,
        |seg| {
            text = seg.to_jsonl("t");
            Ok(())
        },
        1,
    )
    .unwrap();
    let short: String = text.lines().take(2).map(|l| format!("{l}\n")).collect();
    assert!(Segment::from_jsonl(&short).unwrap_err().contains("incomplete"));
    assert!(Segment::from_jsonl("").is_err());
    assert!(Segment::from_jsonl("{\"aiq_sync\": 9, \"rows\": 0}\n").unwrap_err().contains("newer"));
}

#[test]
fn deleting_a_prompt_on_one_computer_deletes_it_on_the_other() {
    let (_a, mut a) = open();
    let (_b, mut b) = open();
    a.insert_event(&ev("e1", "s1", Some("secret words"), 1), 1).unwrap();
    b.sync_apply(&cut(&mut a).unwrap(), 1).unwrap();
    assert!(a.forget_content("e1").unwrap());
    let seg = cut(&mut a).unwrap();
    assert_eq!(b.sync_apply(&seg, 2).unwrap(), Applied::Done { rows: 0, deletes: 1 });
    assert_eq!(count(&b, "SELECT count(*) FROM content"), 0);
    assert_eq!(count(&b, "SELECT count(*) FROM event"), 1, "the prompt happened; only its words go");

    // The other way too: a delete on B reaches A, and B's own copy of the delete is not sent back.
    a.insert_event(&ev("e2", "s1", Some("more words"), 2), 2).unwrap();
    b.sync_apply(&cut(&mut a).unwrap(), 3).unwrap();
    assert!(b.forget_content("e2").unwrap());
    let back = cut(&mut b).unwrap();
    assert_eq!(back.lines.len(), 1);
    a.sync_apply(&back, 4).unwrap();
    assert_eq!(count(&a, "SELECT count(*) FROM content"), 0);
    assert!(cut(&mut a).is_none(), "a delete that arrived is not echoed");
}

fn session(id: &str, mtime: i64, prompts: &[&str]) -> ImportedSession {
    ImportedSession {
        tool: "copilot-cli".into(),
        source_session_id: id.into(),
        source_mtime_ms: mtime,
        folder: "C:\\src\\shop".into(),
        started_utc_ms: Some(10),
        human_turns: prompts.len() as u32,
        non_interactive: None,
        template_key: None,
        outcome: "imported".into(),
        why: String::new(),
        events: prompts
            .iter()
            .enumerate()
            .map(|(i, p)| ImportedEvent {
                kind: "prompt".into(),
                ts_utc_ms: Some(10 + i as i64),
                text: Some((*p).into()),
                actions: None,
            })
            .collect(),
    }
}

#[test]
fn imported_history_travels_a_session_that_grew_replaces_the_old_copy_and_forgetting_it_forgets_everywhere() {
    let (_a, mut a) = open();
    let (_b, mut b) = open();
    a.put_import_session("r1", &session("x", 1, &["one"])).unwrap();
    b.sync_apply(&cut(&mut a).unwrap(), 1).unwrap();
    assert_eq!(count(&b, "SELECT count(*) FROM import_event"), 1);

    a.put_import_session("r2", &session("x", 2, &["one", "two"])).unwrap();
    b.sync_apply(&cut(&mut a).unwrap(), 2).unwrap();
    assert_eq!(count(&b, "SELECT count(*) FROM import_event"), 2);
    assert_eq!(count(&b, "SELECT count(*) FROM import_session"), 1);
    assert_eq!(count(&b, "SELECT source_mtime_ms FROM import_session"), 2);

    a.forget_imported().unwrap();
    b.sync_apply(&cut(&mut a).unwrap(), 3).unwrap();
    assert_eq!(count(&b, "SELECT count(*) FROM import_session"), 0);
    assert_eq!(count(&b, "SELECT count(*) FROM import_event"), 0);
}

#[test]
fn numbered_rows_from_both_computers_keep_their_own_numbering() {
    let (_a, mut a) = open();
    let (_b, mut b) = open();
    feedback(&a, "tip.a");
    feedback(&b, "tip.b");
    b.sync_apply(&cut(&mut a).unwrap(), 1).unwrap();
    a.sync_apply(&cut(&mut b).unwrap(), 1).unwrap();
    assert_eq!(count(&a, "SELECT count(*) FROM tip_feedback"), 2);
    assert_eq!(count(&b, "SELECT count(*) FROM tip_feedback"), 2);
}

#[test]
fn another_computers_ready_tip_and_sessions_are_never_offered_or_shown_here() {
    let (_a, mut a) = open();
    let (_b, mut b) = open();
    a.insert_event(&ev("e1", "s1", Some("one"), 1), 1).unwrap();
    a.insert_signal(
        &NewSignal {
            signal_id: "sig1",
            task_id: "e1",
            surface: "copilot-cli",
            session: "s1",
            detector_id: "reprompt",
            detector_version: 1,
            pack_version: "p",
            opportunity_id: "reprompt",
            reprompt_count: 3,
            score: 1.0,
            evidence: &["e1".to_string()],
        },
        1,
    )
    .unwrap();
    a.insert_decision(
        &NewDecision {
            decision_id: "d1",
            card_id: "c1",
            signal_id: "sig1",
            surface: "copilot-cli",
            session: "s1",
            outcome: "show_ready",
            reason_code: "ok",
            rule_id: "r",
            pack_version: "p",
            card_json: "{}",
        },
        1,
    )
    .unwrap();
    assert_eq!(a.ready_cards_on("copilot-cli").unwrap().len(), 1);
    b.sync_apply(&cut(&mut a).unwrap(), 1).unwrap();
    assert!(b.ready_cards_on("copilot-cli").unwrap().is_empty());
    assert!(b.stale_ready(i64::MAX, i64::MAX).unwrap().is_empty());
    assert_eq!(b.latest_session("copilot-cli").unwrap(), None);
    assert!(b.prompts_for_rebuild(0).unwrap().is_empty(), "the detectors never rerun on another computer's prompts");
}

#[test]
fn tips_shown_on_either_computer_count_toward_the_quiet_period() {
    let (_a, mut a) = open();
    let (_b, mut b) = open();
    a.insert_event(&ev("e1", "s1", Some("one"), 1), 1).unwrap();
    a.insert_delivery("c1", "copilot-cli", "s1", Now { utc_ms: 500, offset_min: 0 }).unwrap();
    b.sync_apply(&cut(&mut a).unwrap(), 1).unwrap();
    assert_eq!(b.last_delivery_ms().unwrap(), Some(500));
    assert_eq!(b.last_delivered_card_id().unwrap(), None, "`aiq show` opens this computer's last tip");
}

#[test]
fn a_store_from_before_sync_sends_everything_it_already_had_once() {
    let d = TempDir::new().unwrap();
    let path = d.path().join("aiq.db");
    {
        let mut old = Store::open_with(&path, &MIGRATIONS[..11]).unwrap();
        old.insert_event(&ev("e1", "s1", Some("before"), 1), 1).unwrap();
        old.raw()
            .execute("INSERT INTO tip_feedback (tip_id, answer, surface, utc_ms) VALUES ('t', 'helpful', 'cli', 1)", [])
            .unwrap();
    }
    let mut a = Store::open(&path).unwrap();
    assert_eq!(a.user_version().unwrap(), 12);
    let seg = cut(&mut a).unwrap();
    let tables: Vec<&str> = seg.lines.iter().map(|l| l["t"].as_str().unwrap()).collect();
    assert_eq!(tables, ["surface_session", "event", "content", "tip_feedback"]);
    assert!(cut(&mut a).is_none());
}

#[test]
fn a_computer_that_never_syncs_keeps_no_queue_and_turning_sync_on_still_sends_everything() {
    // 2026-11-12, the laptop on 0.1.46: sync_outbox held 17,206 rows (about 20 bytes each) with sync never turned on,
    // and rows already in a sent file were never removed either.
    let (_d, mut a) = open();
    let (_e, mut b) = open();
    a.insert_event(&ev("e1", "s1", Some("first"), 1), 1).unwrap();
    a.sync_drop_queue_if_never_sent().unwrap();
    assert_eq!(count(&a, "SELECT count(*) FROM sync_outbox"), 0, "no queue while sync was never on");
    a.insert_event(&ev("e2", "s1", Some("second"), 2), 2).unwrap();
    a.sync_drop_queue_if_never_sent().unwrap();
    // Sync turns on: everything recorded here is queued again and sent.
    a.sync_queue_all_if_never_sent().unwrap();
    let seg = cut(&mut a).unwrap();
    assert!(matches!(b.sync_apply(&seg, 3).unwrap(), Applied::Done { .. }));
    assert_eq!(count(&b, "SELECT count(*) FROM event"), 2);
    assert_eq!(count(&a, "SELECT count(*) FROM sync_outbox"), 0, "rows in a written file leave the queue");
    // Once a file was sent the queue is never dropped or rebuilt again, and new rows still go.
    a.sync_drop_queue_if_never_sent().unwrap();
    a.insert_event(&ev("e3", "s1", Some("third"), 4), 4).unwrap();
    a.sync_queue_all_if_never_sent().unwrap();
    assert_eq!(count(&a, "SELECT count(*) FROM sync_outbox"), 2, "the new prompt and its text only");
    let seg = cut(&mut a).unwrap();
    assert_eq!(seg.seq, 2);
    assert!(matches!(b.sync_apply(&seg, 5).unwrap(), Applied::Done { .. }));
    assert_eq!(count(&b, "SELECT count(*) FROM event"), 3);
    assert_eq!(count(&a, "SELECT count(*) FROM sync_outbox"), 0);
}
