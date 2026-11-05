//! Write the UI test fixtures from a seeded store: the contract test between this crate and the React
//! types (design aiq-home-app, Test plan: Contract). `cargo run -p aiq-home --example fixture -- <dir>`
//! With `AIQ_SEED_DIR=<dir>` it writes only the seeded store, for running the app against it.
#[path = "../tests/support/seed.rs"]
mod seed;

use aiq_core::pack::BundledPack;
use aiq_core::paths::Paths;
use aiq_domain::PackSource;
use aiq_domain::clock::Now;

fn main() {
    // AIQ_SEED_DIR=<dir>: only seed a real store there (for running the desktop app against it), then stop.
    if let Some(dir) = std::env::var_os("AIQ_SEED_DIR") {
        seed::standard(&Paths::at(std::path::Path::new(&dir)), true);
        return;
    }
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: fixture <out dir>"));
    std::fs::create_dir_all(&out).unwrap();
    let pack = BundledPack.load().unwrap();
    let now = Now { utc_ms: seed::NOW, offset_min: 0 };
    let tmp = std::env::temp_dir().join(format!("aiq-home-fixture-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);

    let full = Paths::at(&tmp.join("full"));
    seed::standard(&full, true);
    aiq_core::control::finish_setup(&full).unwrap();
    let mut s = aiq_home::snapshot(&full, &pack, now);
    s.status.core = true;
    s.status.core_state = aiq_home::CoreState::Running;
    s.status.read_ms = 12; // fixed, so screenshots and tests do not churn
    write(&out, "full", &s);

    // A task open right now: the Right now card on Home (design live-rung-trend C).
    let live = Paths::at(&tmp.join("live"));
    seed::coaching(&live);
    seed::open_task(&live);
    aiq_core::control::finish_setup(&live).unwrap();
    let mut l = aiq_home::snapshot(&live, &pack, now);
    l.status.core = true;
    l.status.core_state = aiq_home::CoreState::Running;
    l.status.read_ms = 12;
    write(&out, "live", &l);

    // Task detail of the newest Claude Code task (three prompts, a tip fired on it).
    let id = &s.tasks.iter().find(|t| t.tool == "claude-code" && t.prompts == 3).unwrap().task_id;
    let mut det = aiq_home::task_detail(&full, &pack, now, id).unwrap();
    det.read_ms = 9;
    std::fs::write(out.join("task.json"), serde_json::to_string_pretty(&det).unwrap()).unwrap();

    s.status.core = false;
    s.status.core_state = aiq_home::CoreState::Failed;
    s.status.core_error = Some("AIQ-STORE-OPEN".into());
    write(&out, "down", &s);

    aiq_core::control::set_paused(&full, true, std::time::Duration::from_millis(20)).unwrap();
    let mut p = aiq_home::snapshot(&full, &pack, now);
    p.status.core = true;
    p.status.core_state = aiq_home::CoreState::Running;
    p.status.read_ms = 12;
    write(&out, "paused", &p);

    // Coaching from rung results: kinds of task with next steps, habits, the split (design coaching-from-rungs).
    let coached = Paths::at(&tmp.join("coaching"));
    seed::coaching(&coached);
    aiq_core::control::finish_setup(&coached).unwrap();
    let mut c = aiq_home::snapshot(&coached, &pack, now);
    c.status.core = true;
    c.status.core_state = aiq_home::CoreState::Running;
    c.status.read_ms = 12;
    write(&out, "coaching", &c);

    // Rung progress per kind of task: one up, one down, one the same, one too few (the success measure).
    let moving = Paths::at(&tmp.join("progress"));
    seed::progress(&moving);
    aiq_core::control::finish_setup(&moving).unwrap();
    let mut m = aiq_home::snapshot(&moving, &pack, now);
    m.status.core = true;
    m.status.core_state = aiq_home::CoreState::Running;
    m.status.read_ms = 12;
    write(&out, "progress", &m);

    // The tour's example data (design home-tour T2), shipped inside AIQ home: every view filled at once, with
    // neutral folder names in place of the seed's.
    let tour = Paths::at(&tmp.join("tour"));
    seed::tour(&tour);
    aiq_core::control::finish_setup(&tour).unwrap();
    let mut t = aiq_home::snapshot(&tour, &pack, now);
    t.status.core = true;
    t.status.read_ms = 12;
    // Fixed: four seeds open the store in turn, so its file size depends on when SQLite checkpoints.
    t.data.store_bytes = 336_000;
    write(&out, "tour", &t);
    let path = out.join("tour.json");
    let json = std::fs::read_to_string(&path).unwrap();
    // Folder paths are JSON-escaped in the file (one backslash becomes two); project names are bare strings.
    let esc = |p: &str| p.replace('\\', r"\\");
    let neutral = json
        .replace(&esc(seed::AIQ), &esc(r"c:\users\you\source\repos\shop-api"))
        .replace(&esc(seed::FORESIGHT), &esc(r"c:\users\you\source\repos\docs-site"))
        .replace("\"aiq\"", "\"shop-api\"")
        .replace("\"foresight\"", "\"docs-site\"");
    assert!(!neutral.to_lowercase().contains("imran"), "the shipped example names no real person");
    std::fs::write(&path, neutral).unwrap();

    let empty = Paths::at(&tmp.join("empty"));
    let mut e = aiq_home::snapshot(&empty, &pack, now);
    e.status.core = true;
    e.status.core_state = aiq_home::CoreState::Running;
    write(&out, "first-run", &e);
    e.setup.done = true;
    write(&out, "empty", &e);
    let _ = std::fs::remove_dir_all(&tmp);
}

fn write(dir: &std::path::Path, name: &str, s: &aiq_home::Snapshot) {
    use aiq_core::tools::ToolFound;
    let mut s = s.clone();
    s.data.location = r"C:\Users\you\.aiq".into();
    // Fixed, not this machine's: Claude Code hooked, Copilot present but not hooked.
    s.setup.tools = vec![
        ToolFound { tool: "claude-code", hooked: true, present: true, install: "aiq install claude-code" },
        ToolFound { tool: "copilot", hooked: false, present: true, install: "aiq install copilot" },
    ];
    // Fixed, not this machine's: an installed copy, checked an hour before the seed's now, nothing waiting.
    s.update.supported = true;
    s.update.running = "0.1.0".into();
    s.update.checked_ms = Some(s.now_ms - 3_600_000);
    s.update.can_rollback = true;
    std::fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(&s).unwrap()).unwrap();
}
