//! D4: real SQLite store in a temp dir.
use aiq_contracts::codes;
use aiq_domain::clock::Now;
use aiq_store::*;
use tempfile::TempDir;

fn open() -> (TempDir, Store) {
    let d = TempDir::new().unwrap();
    let s = Store::open(&d.path().join("aiq.db")).unwrap();
    (d, s)
}

fn ev<'a>(id: &'a str, session: &'a str, text: Option<&'a str>) -> NewEvent<'a> {
    NewEvent {
        event_id: id,
        surface: "claude-code",
        session,
        kind: "prompt",
        ts_utc_ms: 1,
        prompt_id: Some("p"),
        trace_id: None,
        collector_version: "0.1.0",
        prompt_chars: text.map(|t| t.len() as i64),
        payload_meta: "{}",
        text,
    }
}

fn seed(s: &mut Store) {
    s.insert_event(&ev("e1", "s", Some("hello world")), 10).unwrap();
    s.insert_signal(
        &NewSignal {
            signal_id: "sig1",
            task_id: "e1",
            surface: "claude-code",
            session: "s",
            detector_id: "reprompt",
            detector_version: 1,
            pack_version: "p",
            opportunity_id: "reprompt",
            reprompt_count: 3,
            score: 1.0,
            evidence: &["e1".to_string()],
        },
        10,
    )
    .unwrap();
    decide(s, "d1", "c1", "show_ready", "ok", 10);
    s.insert_folder_rule("/ep", "set_aside", "template", "live", 10).unwrap();
    s.set_setting("daily_cap", "5", 10).unwrap();
    s.insert_delivery("c1", "claude-code", "s", Now { utc_ms: 20, offset_min: 0 }).unwrap();
    s.insert_open("c1", "cli", 30).unwrap();
}

fn decide(s: &Store, did: &str, cid: &str, outcome: &str, reason: &str, at: i64) {
    s.insert_decision(
        &NewDecision {
            decision_id: did,
            card_id: cid,
            signal_id: "sig1",
            surface: "claude-code",
            session: "s",
            outcome,
            reason_code: reason,
            rule_id: "in_moment.v1",
            pack_version: "p",
            card_json: "{}",
        },
        at,
    )
    .unwrap();
}

#[test]
fn open_creates_latest_schema_version_4() {
    let (_d, s) = open();
    assert_eq!(s.user_version().unwrap(), 4);
}

#[test]
fn v1_store_migrates_to_latest_keeping_data() {
    let d = TempDir::new().unwrap();
    let p = d.path().join("aiq.db");
    {
        let mut s = Store::open_with(&p, &[SCHEMA_V1]).unwrap();
        s.insert_event(&ev("e1", "s", Some("keep")), 1).unwrap();
        assert_eq!(s.user_version().unwrap(), 1);
    }
    let s = Store::open(&p).unwrap();
    assert_eq!(s.user_version().unwrap(), 4);
    assert_eq!(s.events_by_kind().unwrap(), vec![("prompt".to_string(), 1)]);
    assert!(s.import_counts().unwrap().is_empty());
    s.insert_open("c1", "cli", 5).unwrap();
    assert!(s.opened("c1").unwrap() && !s.opened("c2").unwrap());
}

#[test]
fn fact_tables_reject_update_and_delete() {
    let (_d, mut s) = open();
    seed(&mut s);
    for (table, col) in [
        ("surface_session", "first_seen_utc_ms"),
        ("event", "ts_utc_ms"),
        ("signal", "score"),
        ("card_decision", "utc_ms"),
        ("card_delivery", "delivered_utc_ms"),
        ("setting_change", "utc_ms"),
        ("card_open", "utc_ms"),
        ("folder_rule", "utc_ms"),
    ] {
        let u = s.raw().execute(&format!("UPDATE {table} SET {col} = {col} + 1"), []);
        assert!(u.unwrap_err().to_string().contains(codes::STORE_APPEND_ONLY), "update {table}");
        let d = s.raw().execute(&format!("DELETE FROM {table}"), []);
        assert!(d.unwrap_err().to_string().contains(codes::STORE_APPEND_ONLY), "delete {table}");
    }
}

#[test]
fn content_allows_delete_forbids_update() {
    let (_d, mut s) = open();
    s.insert_event(&ev("e1", "s", Some("secret")), 1).unwrap();
    let u = s.raw().execute("UPDATE content SET text = 'x'", []);
    assert!(u.unwrap_err().to_string().contains(codes::STORE_APPEND_ONLY));
    assert_eq!(s.raw().execute("DELETE FROM content", []).unwrap(), 1);
}

#[test]
fn insert_event_replay_stores_one_row() {
    let (_d, mut s) = open();
    assert!(s.insert_event(&ev("e1", "s", Some("a")), 1).unwrap());
    assert!(!s.insert_event(&ev("e1", "s", Some("a")), 2).unwrap());
    let n: i64 = s.raw().query_row("SELECT count(*) FROM content", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1);
    assert_eq!(s.events_by_kind().unwrap(), vec![("prompt".to_string(), 1)]);
}

#[test]
fn insert_event_missing_prompt_stores_no_content() {
    let (_d, mut s) = open();
    s.insert_event(&ev("e1", "s", None), 1).unwrap();
    let n: i64 = s.raw().query_row("SELECT count(*) FROM content", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn second_delivery_same_session_conflicts_even_bypassing_loop() {
    let (_d, mut s) = open();
    seed(&mut s);
    let e = s.insert_delivery("c2", "claude-code", "s", Now { utc_ms: 30, offset_min: 0 }).unwrap_err();
    assert_eq!(e.code, codes::DELIVERY_CONFLICT);
    s.insert_delivery("c3", "claude-code", "other", Now { utc_ms: 30, offset_min: 0 }).unwrap();
}

#[test]
fn ready_card_excludes_delivered_and_expired() {
    let (_d, mut s) = open();
    seed(&mut s); // c1 delivered
    assert!(s.ready_card("claude-code", "s").unwrap().is_none());
    decide(&s, "d2", "c2", "show_ready", "ok", 40);
    assert_eq!(s.ready_card("claude-code", "s").unwrap().unwrap().card_id, "c2");
    assert_eq!(s.stale_ready(41).unwrap().len(), 1);
    assert!(s.stale_ready(40).unwrap().is_empty());
    decide(&s, "d3", "c2", "drop", "expired", 50);
    assert!(s.ready_card("claude-code", "s").unwrap().is_none());
    let rec = s.card("c2").unwrap().unwrap();
    assert_eq!((rec.outcome.as_str(), rec.reason_code.as_str()), ("drop", "expired"));
}

#[test]
fn setting_latest_row_wins() {
    let (_d, s) = open();
    assert_eq!(s.setting("paused").unwrap(), None);
    s.set_setting("paused", "true", 1).unwrap();
    s.set_setting("paused", "false", 2).unwrap();
    assert_eq!(s.setting("paused").unwrap().as_deref(), Some("false"));
}

#[test]
fn delivery_days_use_stored_offset() {
    let (_d, s) = open();
    let t = 1_793_489_400_000; // 2026-10-31T23:30Z
    s.insert_delivery("a", "claude-code", "x", Now { utc_ms: t, offset_min: 60 }).unwrap();
    s.insert_delivery("b", "claude-code", "y", Now { utc_ms: t, offset_min: 0 }).unwrap();
    let mut d = s.delivery_days().unwrap();
    d.sort();
    assert_eq!(d[1] - d[0], 1);
}

#[test]
fn card_evidence_deleted_content_reports_none() {
    let (_d, mut s) = open();
    seed(&mut s);
    assert_eq!(s.card("c1").unwrap().unwrap().evidence, vec![(1, Some("hello world".to_string()))]);
    s.raw().execute("DELETE FROM content", []).unwrap();
    let rec = s.card("c1").unwrap().unwrap();
    assert_eq!(rec.evidence, vec![(1, None)]);
    assert!(rec.delivered);
    assert!(s.card("nope").unwrap().is_none());
}

#[test]
fn failing_migration_leaves_version_and_data_unchanged() {
    let d = TempDir::new().unwrap();
    let p = d.path().join("aiq.db");
    {
        let mut s = Store::open_with(&p, &[SCHEMA_V1]).unwrap();
        s.insert_event(&ev("e1", "s", Some("keep")), 1).unwrap();
    }
    let bad = "CREATE TABLE new_thing (x INTEGER); INSERT INTO no_such_table VALUES (1);";
    let e = Store::open_with(&p, &[SCHEMA_V1, bad]).err().unwrap();
    assert_eq!(e.code, codes::STORE_MIGRATION);
    let s = Store::open_with(&p, &[SCHEMA_V1]).unwrap();
    assert_eq!(s.user_version().unwrap(), 1);
    let t: i64 =
        s.raw().query_row("SELECT count(*) FROM sqlite_master WHERE name = 'new_thing'", [], |r| r.get(0)).unwrap();
    assert_eq!(t, 0);
    assert_eq!(s.events_by_kind().unwrap(), vec![("prompt".to_string(), 1)]);
}

#[cfg(unix)]
#[test]
fn store_file_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, mut s) = open();
    s.insert_event(&ev("e1", "s", Some("x")), 1).unwrap();
    for suffix in ["", "-wal"] {
        let p = format!("{}{suffix}", s.path().display());
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{p}");
    }
}

#[test]
fn delete_store_files_removes_all_and_tolerates_missing() {
    let (d, mut s) = open();
    s.insert_event(&ev("e1", "s", Some("x")), 1).unwrap();
    let p = s.path().to_owned();
    drop(s);
    // Security review S10: side files hold prompt text too. SQLite removes them on a clean close, so make
    // them exist, as after a crash.
    for side in ["-wal", "-shm", "-journal"] {
        std::fs::write(format!("{}{side}", p.display()), b"x").unwrap();
    }
    delete_store_files(&p).unwrap();
    delete_store_files(&p).unwrap();
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
}

#[test]
fn last_delivery_and_session_queries() {
    let (_d, mut s) = open();
    assert_eq!(s.last_delivery_ms().unwrap(), None);
    seed(&mut s);
    assert_eq!(s.last_delivery_ms().unwrap(), Some(20));
    assert_eq!(s.last_event_kind("claude-code", "s").unwrap().as_deref(), Some("prompt"));
    assert_eq!(s.latest_session("claude-code").unwrap().as_deref(), Some("s"));
    assert_eq!(s.latest_session("copilot-cli").unwrap(), None);
    let (id, _, at) = s.delivered_card("claude-code", "s").unwrap().unwrap();
    assert_eq!((id.as_str(), at), ("c1", 20));
    assert_eq!(s.prompts_after("claude-code", "s", 0).unwrap(), 1);
    assert_eq!(s.prompts_after("claude-code", "s", i64::MAX).unwrap(), 0);
}

#[test]
fn folder_rule_newest_action_wins() {
    let (_d, s) = open();
    assert!(s.set_aside_folders().unwrap().is_empty());
    s.insert_folder_rule("/ep", "set_aside", "template", "live", 1).unwrap();
    s.insert_folder_rule("/b", "set_aside", "burst", "live", 2).unwrap();
    assert_eq!(
        s.set_aside_folders().unwrap(),
        vec![("/b".to_string(), "burst".to_string(), 2), ("/ep".to_string(), "template".to_string(), 1)]
    );
    s.insert_folder_rule("/ep", "include", "person", "person", 3).unwrap();
    assert_eq!(s.set_aside_folders().unwrap().len(), 1);
    let now = s.folder_rules_now().unwrap();
    assert_eq!((now["/ep"].as_str(), now["/b"].as_str()), ("include", "set_aside"));
    s.insert_folder_rule("/ep", "set_aside", "burst", "live", 4).unwrap();
    assert_eq!(s.set_aside_folders().unwrap()[0].0, "/ep");
    assert!(s.insert_folder_rule("/x", "maybe", "r", "live", 5).is_err(), "action is checked");
}

#[test]
fn session_known_after_first_event() {
    let (_d, mut s) = open();
    assert!(!s.session_known("claude-code", "s").unwrap());
    s.insert_event(&ev("e1", "s", Some("x")), 1).unwrap();
    assert!(s.session_known("claude-code", "s").unwrap() && !s.session_known("copilot-cli", "s").unwrap());
}
