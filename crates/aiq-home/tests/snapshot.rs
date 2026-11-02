//! Read-model tests over a seeded store (design aiq-home-app, Test plan: Integration).
#[path = "support/seed.rs"]
mod seed;

use aiq_core::pack::BundledPack;
use aiq_core::paths::Paths;
use aiq_domain::PackSource;
use aiq_domain::clock::Now;
use aiq_home::{MAX_ITEMS, snapshot};
use seed::*;

fn now() -> Now {
    Now { utc_ms: NOW, offset_min: 0 }
}

fn snap(paths: &Paths) -> aiq_home::Snapshot {
    snapshot(paths, &BundledPack.load().unwrap(), now())
}

#[test]
fn missing_store_is_empty_and_creates_nothing() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    let s = snap(&p);
    assert!(!s.status.store_found);
    assert!(s.status.error.is_none());
    assert_eq!((s.home.tasks_in_window, s.data.events, s.tasks.len()), (0, 0, 0));
    assert_eq!(s.home.weeks.len(), 4);
    assert_eq!(s.settings.daily_cap, 5, "pack default");
    assert!(!p.db().exists(), "a read must never create the store");
}

#[test]
fn tasks_follow_the_task_rule_window_and_set_aside() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    standard(&p, false);
    let s = snap(&p);
    assert!(s.status.error.is_none(), "{:?}", s.status.error);
    // cc1 (3 prompts), cp1 x2 (gap split), vscode v1+v2 (one), cc2 (deleted text), cp2 = 6. Old and pipeline excluded.
    let shape: Vec<(&str, u32)> = s.tasks.iter().map(|t| (t.tool.as_str(), t.prompts)).collect();
    assert_eq!(
        shape,
        vec![
            ("copilot-cli", 1),
            ("claude-code", 3),
            ("copilot-cli", 1),
            ("copilot-cli", 1),
            ("vscode", 2),
            ("claude-code", 1)
        ]
    );
    assert_eq!(s.home.tasks_in_window, 6);
    assert!(s.tasks.iter().all(|t| t.rung.is_none() && t.job.is_none() && t.origin == "live"));
    assert_eq!(s.home.judged, 0, "no judge: nothing is judged, nothing guessed");
    assert_eq!(s.tasks[1].folder.as_deref(), Some(AIQ));
    assert_eq!(s.tasks[4].folder, None, "vscode prompts carried no folder");
    assert!(s.tasks.iter().all(|t| t.folder.as_deref() != Some(PIPELINE)));
    assert_eq!(s.home.set_aside.len(), 1);
    // Weeks: oldest first, last ends now: [-28d,-21d) [-21d,-14d) [-14d,-7d) [-7d,now). Tasks at -20d, -16d | -9d x2 | -2d, -30min.
    let w: Vec<u32> = s.home.weeks.iter().map(|w| w.tasks).collect();
    assert_eq!(w, vec![0, 2, 2, 2]);
    assert_eq!(s.home.by_tool[0], ("copilot-cli".to_string(), 3));
}

#[test]
fn tips_settings_and_data() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    standard(&p, false);
    let s = snap(&p);
    assert_eq!(s.tips.len(), 2);
    assert_eq!(s.tips[0].tool, "copilot-cli");
    assert!(!s.tips[0].opened);
    assert!(s.tips[1].opened);
    assert!(s.tips[1].noticed.starts_with("You reworded"));
    assert_eq!(s.home.tips_today, 1);
    assert_eq!(s.settings.daily_cap, 3);
    assert_eq!(s.settings.copilot_tip_stays, "next-prompt");
    assert_eq!((s.data.events, s.data.prompts, s.data.turn_ends), (13, 11, 2));
    assert_eq!(s.data.items.len(), 13);
    let deleted: Vec<_> = s.data.items.iter().filter(|i| i.deleted).collect();
    assert_eq!(deleted.len(), 1);
    assert!(deleted[0].text.is_none());
    assert!(s.data.items.iter().filter(|i| i.kind == "stop").all(|i| !i.deleted && i.text.is_none()));
    assert!(s.data.items.windows(2).all(|w| w[0].ts_ms >= w[1].ts_ms), "newest first");
    assert_eq!(s.status.last_event_ms, Some(NOW - 30 * MIN));
    assert!(s.data.store_bytes > 0);
}

#[test]
fn imported_tasks_are_tasks_and_automated_ones_are_not() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    standard(&p, true);
    let s = snap(&p);
    assert!(s.status.error.is_none(), "{:?}", s.status.error);
    let imported: Vec<_> = s.tasks.iter().filter(|t| t.origin == "imported").collect();
    assert_eq!(imported.len(), 1, "automated imported session is set aside");
    assert_eq!(imported[0].prompts, 2);
    assert_eq!(imported[0].folder.as_deref(), Some(FORESIGHT), "folder normalised like live folders");
    assert_eq!(s.home.imported_in_window, 1);
    assert_eq!((s.data.imported_sessions, s.data.imported_events), (1, 6));
}

#[test]
fn paused_flag_is_read() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    standard(&p, false);
    aiq_core::control::set_paused(&p, true, std::time::Duration::from_millis(20)).unwrap();
    assert!(snap(&p).status.paused);
}

#[test]
fn unreadable_store_reports_code_not_a_panic() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    p.ensure_dirs().unwrap();
    std::fs::write(p.db(), b"this is not a sqlite database, it is just text padded out to a page").unwrap();
    let s = snap(&p);
    assert!(s.status.store_found);
    assert!(s.status.error.as_deref().unwrap().starts_with("AIQ-HOME-STORE-READ"), "{:?}", s.status.error);
    assert_eq!(s.tasks.len(), 0);
}

#[test]
fn read_never_writes_the_store() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    standard(&p, false);
    let before = std::fs::read(p.db()).unwrap();
    let st = aiq_store::Store::open_read_only(&p.db()).unwrap();
    assert!(st.raw().execute("INSERT INTO setting_change (key, value, utc_ms) VALUES ('x', 'y', 1)", []).is_err());
    drop(st);
    let _ = snap(&p);
    assert_eq!(before, std::fs::read(p.db()).unwrap());
}

#[test]
fn items_are_capped_counts_exact_and_read_is_fast() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    let mut s = Seed::new(&p);
    // 5,000 prompts over 30 days in 250 sessions: the design's performance case.
    for i in 0..5_000i64 {
        let at = NOW - 29 * DAY + i * (29 * DAY / 5_000);
        s.prompt("claude-code", &format!("s{}", i / 20), at, Some(AIQ), "write tests for the parser module");
    }
    drop(s);
    let mut times = Vec::new();
    let mut last = None;
    for _ in 0..10 {
        let snap = snap(&p);
        times.push(snap.status.read_ms);
        last = Some(snap);
    }
    let snap = last.unwrap();
    assert_eq!(snap.data.events, 5_000);
    assert_eq!(snap.data.items.len(), MAX_ITEMS);
    assert!(snap.data.items_capped);
    assert_eq!(snap.home.tasks_in_window, 250);
    times.sort_unstable();
    let p95 = times[9];
    eprintln!("snapshot read_ms over 10 runs (5,000 prompts): {times:?}");
    // Debug builds are slower than release; the design's 150 ms budget is checked with headroom.
    assert!(p95 <= 600, "snapshot p95 {p95} ms");
}

/// Task detail (design aiq-home-deferred-pages E8, E9): the same task as the list, with its prompts in order.
#[test]
fn task_detail_lists_the_tasks_prompts_live_and_imported() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    standard(&p, true);
    let pack = BundledPack.load().unwrap();
    let s = snap(&p);
    let cc1 = s.tasks.iter().find(|t| t.tool == "claude-code" && t.prompts == 3).unwrap();
    let det = aiq_home::task_detail(&p, &pack, now(), &cc1.task_id).unwrap();
    assert_eq!(&det.task, cc1);
    let texts: Vec<_> = det.prompts.iter().map(|x| x.text.as_deref().unwrap()).collect();
    assert_eq!(
        texts,
        vec![
            "refactor the login handler to return typed errors",
            "refactor login handler so it returns typed errors",
            "please refactor the login handler, typed errors returned"
        ]
    );
    assert!(det.prompts.windows(2).all(|w| w[0].ts_ms < w[1].ts_ms));

    // Deleted text: the prompt stays, marked deleted.
    let gone = s.tasks.iter().find(|t| t.first_ms == NOW - 20 * DAY).unwrap();
    let det = aiq_home::task_detail(&p, &pack, now(), &gone.task_id).unwrap();
    assert_eq!((det.prompts.len(), det.prompts[0].text.is_none(), det.prompts[0].deleted), (1, true, true));

    // Imported: text from the import tables.
    let imp = s.tasks.iter().find(|t| t.origin == "imported").unwrap();
    let det = aiq_home::task_detail(&p, &pack, now(), &imp.task_id).unwrap();
    let texts: Vec<_> = det.prompts.iter().map(|x| x.text.as_deref().unwrap()).collect();
    assert_eq!(texts, vec!["plan the migration", "go"]);

    assert!(aiq_home::task_detail(&p, &pack, now(), "no-such-task").is_none());
    // The snapshot never carries prompt ids or text per task.
    let json = serde_json::to_string(&s).unwrap();
    assert!(!json.contains("members"));
}

#[test]
fn setup_done_follows_the_flag() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    assert!(!snap(&p).setup.done);
    aiq_core::control::finish_setup(&p).unwrap();
    let s = snap(&p);
    assert!(s.setup.done);
    assert_eq!(s.setup.tools.iter().map(|t| t.tool).collect::<Vec<_>>(), vec!["claude-code", "copilot"]);
}
