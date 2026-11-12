//! Sync over a folder only the person's computers hold: a laptop's AIQ home seen from a Devbox (as through Remote
//! Desktop's `\\tsclient`), here two temp folders.
use aiq_core::paths::Paths;
use aiq_core::sync::{SETTING_SYNC, pass, read_status};
use aiq_store::{NewEvent, Store};
use tempfile::TempDir;

struct Computer {
    _dir: TempDir,
    paths: Paths,
    store: Store,
}

fn computer() -> Computer {
    let dir = TempDir::new().unwrap();
    let paths = Paths::at(dir.path());
    let store = Store::open(&paths.db()).unwrap();
    Computer { _dir: dir, paths, store }
}

fn prompt(c: &mut Computer, id: &str, text: &str, ts: i64) {
    c.store
        .insert_event(
            &NewEvent {
                event_id: id,
                surface: "copilot-cli",
                session: id,
                kind: "prompt",
                ts_utc_ms: ts,
                prompt_id: None,
                trace_id: None,
                collector_version: "t",
                prompt_chars: Some(text.len() as i64),
                payload_meta: "{}",
                text: Some(text),
            },
            ts,
        )
        .unwrap();
}

fn events(c: &Computer) -> Vec<String> {
    let mut st = c.store.raw().prepare("SELECT event_id FROM event ORDER BY event_id").unwrap();
    st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn the_devbox_offers_once_it_sees_the_laptop_and_writes_nothing_before_the_person_chooses() {
    let laptop = computer();
    let mut devbox = computer();
    let st = pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&laptop.paths.data), 1);
    assert_eq!(st.setting, "unset");
    assert!(st.offer.is_some());
    assert!(!laptop.paths.data.join("sync").exists(), "nothing written on the laptop before the person chooses");
    assert!(!devbox.paths.data.join("sync").exists());
    // Without the laptop in reach there is nothing to offer.
    assert!(pass(&mut devbox.store, &devbox.paths, &[], 2).offer.is_none());
}

#[test]
fn keep_in_sync_on_the_devbox_brings_both_computers_everything_and_the_laptop_joins_by_itself() {
    let mut laptop = computer();
    let mut devbox = computer();
    prompt(&mut laptop, "lap-1", "draft the release notes", 10);
    prompt(&mut devbox, "dev-1", "fix the failing export test", 20);
    let lap_home = laptop.paths.data.clone();

    devbox.store.set_setting(SETTING_SYNC, "on", 30).unwrap();
    let st = pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 40);
    assert!(st.errors.is_empty(), "{:?}", st.errors);
    assert!(st.offer.is_none());
    let dev_id = devbox.store.device_id().unwrap();
    assert!(lap_home.join("sync").join(&dev_id).join("00000001.jsonl").exists(), "the devbox's file is on the laptop");

    // The laptop, on its own next pass, finds the devbox's folder: it turns on and takes the devbox's rows.
    let st = pass(&mut laptop.store, &laptop.paths, &[], 50);
    assert_eq!(st.setting, "on");
    assert_eq!(laptop.store.setting(SETTING_SYNC).unwrap().as_deref(), Some("on"));
    assert_eq!(events(&laptop), ["dev-1", "lap-1"]);
    assert_eq!(st.peers.len(), 1);
    assert_eq!(st.peers[0].id, dev_id);
    assert_eq!(st.peers[0].state, "ok");

    // The devbox's next pass takes the laptop's rows from the laptop's folder.
    let st = pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 60);
    assert_eq!(events(&devbox), ["dev-1", "lap-1"]);
    assert_eq!(st.peers.len(), 1);
    assert!(st.peers[0].last_ms.is_some());
    assert_eq!(read_status(&devbox.paths).unwrap(), st, "Home reads the same status");

    // New work keeps flowing, both ways, and nothing is applied twice.
    prompt(&mut laptop, "lap-2", "plan the sprint", 70);
    pass(&mut laptop.store, &laptop.paths, &[], 80);
    pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 90);
    pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 95);
    assert_eq!(events(&devbox), ["dev-1", "lap-1", "lap-2"]);
}

#[test]
fn turning_sync_off_on_one_computer_pauses_it_on_the_other_and_a_computer_out_of_reach_stays_listed() {
    let mut laptop = computer();
    let mut devbox = computer();
    let lap_home = laptop.paths.data.clone();
    prompt(&mut laptop, "lap-1", "one", 10);
    devbox.store.set_setting(SETTING_SYNC, "on", 1).unwrap();
    pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 2);
    pass(&mut laptop.store, &laptop.paths, &[], 3);
    pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 4);

    laptop.store.set_setting(SETTING_SYNC, "off", 5).unwrap();
    let st = pass(&mut laptop.store, &laptop.paths, &[], 6);
    assert_eq!(st.setting, "off");
    prompt(&mut laptop, "lap-2", "two", 7);
    pass(&mut laptop.store, &laptop.paths, &[], 8);
    let st = pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 9);
    assert_eq!(st.peers[0].state, "paused");
    assert_eq!(events(&devbox), ["lap-1"], "nothing new from a computer that turned sync off");

    // The Remote Desktop session ends: the laptop is out of reach but still listed, with when it last synced.
    let st = pass(&mut devbox.store, &devbox.paths, &[], 10);
    assert_eq!(st.peers.len(), 1);
    assert!(st.peers[0].last_ms.is_some());
}

#[test]
fn files_from_a_newer_aiq_wait_for_the_update_and_a_cut_short_file_is_retried() {
    let mut laptop = computer();
    let mut devbox = computer();
    let lap_home = laptop.paths.data.clone();
    devbox.store.set_setting(SETTING_SYNC, "on", 1).unwrap();
    pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 2);
    prompt(&mut laptop, "lap-1", "one", 3);
    pass(&mut laptop.store, &laptop.paths, &[], 4);
    let lap_id = laptop.store.device_id().unwrap();
    let file = lap_home.join("sync").join(&lap_id).join("00000001.jsonl");
    let good = std::fs::read_to_string(&file).unwrap();

    std::fs::write(&file, good.replacen("\"schema\":12", "\"schema\":99", 1)).unwrap();
    let st = pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 5);
    assert_eq!(st.peers[0].state, "update");
    assert!(events(&devbox).is_empty());

    std::fs::write(&file, good.lines().next().unwrap()).unwrap();
    let st = pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 6);
    assert_eq!(st.peers[0].state, "error");
    assert!(events(&devbox).is_empty());

    std::fs::write(&file, &good).unwrap();
    let st = pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 7);
    assert_eq!(st.peers[0].state, "ok");
    assert_eq!(events(&devbox), ["lap-1"]);
}

#[test]
fn once_the_core_deletes_everything_the_sync_thread_never_opens_the_store_again() {
    // DC-060: the sync thread used to keep its store connection open, so on Windows `forget --all` could not delete
    // the store. It now opens the store only for a check or a pass, and after `stop` it never opens it again.
    let dir = TempDir::new().unwrap();
    let paths = Paths::at(dir.path());
    aiq_core::sync::stop();
    aiq_core::sync::run(paths.clone());
    assert!(!paths.db().exists(), "a stopped sync thread must not create the store again");
}

#[test]
fn a_file_that_will_not_copy_to_the_laptop_still_lets_the_laptop_see_the_devbox() {
    // 2026-11-12, Imran's laptop and Devbox: the Devbox copied two files, then its third stayed a `.part` on the
    // laptop pass after pass, and the laptop never saw the Devbox because the Devbox wrote its own entry only after
    // every file had copied. A file that won't copy must not hide the computer.
    let laptop = computer();
    let mut devbox = computer();
    prompt(&mut devbox, "dev-1", "fix the failing export test", 10);
    let lap_home = laptop.paths.data.clone();
    let dev_id = devbox.store.device_id().unwrap();
    // Block the first file's copy: its temp name on the laptop is a folder with something in it.
    let blocked = lap_home.join("sync").join(&dev_id).join("00000001.part");
    std::fs::create_dir_all(blocked.join("x")).unwrap();
    devbox.store.set_setting(SETTING_SYNC, "on", 20).unwrap();
    let st = pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&lap_home), 30);
    assert!(st.errors.iter().any(|e| e == "AIQ-SYNC-WRITE"), "{:?}", st.errors);
    assert!(
        lap_home.join("sync").join(&dev_id).join("computer.json").exists(),
        "the laptop can see the Devbox even while a file won't copy"
    );
}

#[test]
fn the_offer_names_the_other_computer_not_its_windows_user_folder() {
    // 2026-11-12, Imran: how does the second computer learn the first one's name? Before anyone chooses, the laptop
    // has no folder under sync/, so the offer used the user folder's name ("imrans"). The laptop's AIQ writes its
    // own name in its status file on every pass; the offer reads it there.
    let mut laptop = computer();
    let mut devbox = computer();
    pass(&mut laptop.store, &laptop.paths, &[], 1);
    let lap_name = read_status(&laptop.paths).unwrap().name;
    let st = pass(&mut devbox.store, &devbox.paths, std::slice::from_ref(&laptop.paths.data), 2);
    assert_eq!(st.offer.map(|o| o.name), Some(lap_name.clone()));
    assert_eq!(st.via, Some(lap_name));
}
