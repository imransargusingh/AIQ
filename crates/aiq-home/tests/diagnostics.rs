//! The diagnostics file over a seeded store (design diagnostics-file, Test plan). The load-bearing promise: without
//! "Include my prompts" no prompt text, analysis sentence, folder name or personal name reaches the file.
#[path = "support/seed.rs"]
mod seed;

use aiq_core::pack::BundledPack;
use aiq_core::paths::Paths;
use aiq_domain::PackSource;
use aiq_domain::clock::Now;
use aiq_domain::scrub::Scrub;
use aiq_home::diagnostics::{FORMAT, Options, build, file_name, save};
use seed::*;

fn now() -> Now {
    Now { utc_ms: NOW, offset_min: 0 }
}

fn me() -> Scrub {
    Scrub {
        home: Some(r"c:\users\imran".into()),
        user: Some("imran".into()),
        computer: Some("IMRAN-LAPTOP".into()),
        never: vec!["https://example.internal/team/tips.git".into(), "example.internal".into()],
    }
}

fn seeded() -> (tempfile::TempDir, Paths) {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    standard(&p, true);
    let mut s = Seed::new(&p);
    s.setting(r"project_name.c:\users\imran\source\repos\foresight", "Foresight Secret Codename", NOW - DAY);
    drop(s);
    std::fs::write(
        p.data.join("label.log"),
        "label t1 copilot-cli error -/- ms=1790 wait_ms=2011\n\
         why t1 error: cannot read C:\\Users\\imran\\.copilot\\config.json for imran@contoso.com via https://example.internal/team/tips.git\n",
    )
    .unwrap();
    std::fs::write(
        p.data.join("aiq.log"),
        "{\"level\":\"INFO\",\"fields\":{\"message\":\"noise\"}}\n\
         {\"level\":\"WARN\",\"fields\":{\"message\":\"spool at C:\\\\Users\\\\imran\\\\.aiq\\\\spool.d on IMRAN-LAPTOP\"}}\n",
    )
    .unwrap();
    (d, p)
}

fn make(p: &Paths, prompts: bool) -> (String, serde_json::Value) {
    let pack = BundledPack.load().unwrap();
    let d = build(p, &pack, now(), Options { prompts }, &me(), Some(true)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&d.text).unwrap();
    assert_eq!(d.bytes, d.text.len());
    (d.text, v)
}

/// Prompts the seed wrote inside the 14-day window: never in the file unless the person chose prompts.
const PROMPTS: &[&str] = &[
    "refactor the login handler to return typed errors",
    "write a spec for the export button",
    "draft the release notes",
];

/// Text that must never reach the file, whatever the person chose.
const NEVER: &[&str] = &[
    // the analysis's sentences
    "You gave the agent the goal",
    "Hand the agent the whole change",
    "You handed the agent the whole plan",
    // folder names and project names
    "repos\\\\aiq",
    "foresight",
    "pipeline",
    "Executive_Presence",
    "Secret Codename",
    "aiq-copilot-probe",
    // personal names and addresses
    "imran",
    "IMRAN-LAPTOP",
    "contoso.com",
    "example.internal",
];

/// The project's public repository address is the one kept on purpose; it holds the seed's user name.
fn lower_has(text: &str, needle: &str) -> bool {
    text.to_lowercase().replace("github.com/imransargusingh", "").contains(&needle.to_lowercase())
}

#[test]
fn without_prompts_the_file_holds_no_prompt_text_sentence_folder_or_name() {
    let (_d, p) = seeded();
    let (text, v) = make(&p, false);
    for n in PROMPTS.iter().chain(NEVER) {
        assert!(!lower_has(&text, n), "{n:?} reached the file:\n{text}");
    }
    assert_eq!(v["about"]["format"], FORMAT);
    assert_eq!(v["about"]["prompts"], false);
    assert!(v["about"]["left_out"].as_array().unwrap().iter().any(|x| x == "your prompts"));
    assert!(v["tasks"].as_array().unwrap().iter().all(|t| t.get("prompt_text").is_none()));
}

#[test]
fn with_prompts_the_task_prompts_go_in_scrubbed_and_the_rest_stays_out() {
    let (_d, p) = seeded();
    let (text, v) = make(&p, true);
    for n in PROMPTS {
        assert!(lower_has(&text, n), "{n:?} missing with prompts chosen");
    }
    for n in NEVER {
        assert!(!lower_has(&text, n), "{n:?} reached the file:\n{text}");
    }
    assert_eq!(v["about"]["prompts"], true);
    // The task whose text the person deleted shows a gap, not old text.
    assert!(!lower_has(&text, "secret project notes"));
}

#[test]
fn tasks_carry_kind_analysis_numbers_tip_path_and_folder_letters() {
    let (_d, p) = seeded();
    let (_, v) = make(&p, false);
    let tasks = v["tasks"].as_array().unwrap();
    let judged = tasks.iter().find(|t| t["analysis"]["job"] == "code_change").expect("judged task");
    assert_eq!(judged["analysis"]["rung"], 2);
    assert!(judged["analysis"].get("evidence").is_none() && judged["analysis"].get("gap").is_none());
    // Each tip as the person saw it, with what matched, so "was this the right tip?" can be answered from the file.
    let tip = &judged["tips"][0];
    assert_eq!(tip["noticed"], "This request went round 3 times. The agent may need one new detail.");
    assert_eq!(tip["try_this"], "Say what is wrong with the last answer and add the one fact it lacked.");
    assert!(tip["matched"]["detector"].is_string() && tip["matched"]["score"].is_number(), "{tip}");
    // Folders are letters only.
    for t in tasks {
        let f = &t["folder"];
        assert!(f.is_null() || f.as_str().is_some_and(|s| s.chars().all(|c| c.is_ascii_uppercase())), "{f}");
    }
    // Tip path steps from tasks outside the task list are kept (labelled as such).
    assert!(tasks.iter().any(|t| t["task"] == "t1" && t["tip_path"][0]["step"] == "label_asked"));
    assert_eq!(v["counts"]["set_aside_sessions"], 14);
    assert_eq!(v["settings"]["project names set"], 1);
}

#[test]
fn a_failed_label_carries_the_tools_words_scrubbed() {
    let (_d, p) = seeded();
    let (_, v) = make(&p, false);
    let log: Vec<&str> = v["logs"]["label.log"].as_array().unwrap().iter().filter_map(|x| x.as_str()).collect();
    assert_eq!(
        log[1], r"why t1 error: cannot read %USERPROFILE%\.copilot\config.json for <email> via <url>",
        "{log:?}"
    );
    let warn: Vec<&str> = v["logs"]["aiq.log"].as_array().unwrap().iter().filter_map(|x| x.as_str()).collect();
    assert_eq!(warn.len(), 1, "only warnings and errors: {warn:?}");
    assert!(warn[0].contains(r"%USERPROFILE%\\.aiq") && warn[0].contains("<computer>"), "{warn:?}");
}

#[test]
fn a_missing_store_still_makes_a_file() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::at(d.path());
    let (_, v) = make(&p, false);
    assert_eq!(v["store"], "AIQ has stored nothing yet");
    assert!(!p.db().exists(), "making the file never creates the store");
}

#[test]
fn saving_never_overwrites() {
    let d = tempfile::tempdir().unwrap();
    let name = file_name(now());
    assert!(name.starts_with("aiq-diagnostics-") && name.ends_with(".json"), "{name}");
    let a = save(d.path(), &name, "one").unwrap();
    let b = save(d.path(), &name, "two").unwrap();
    assert_ne!(a, b);
    assert_eq!(std::fs::read_to_string(a).unwrap(), "one");
    assert!(b.to_string_lossy().ends_with("-2.json"));
}
