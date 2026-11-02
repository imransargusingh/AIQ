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
    s.status.read_ms = 12; // fixed, so screenshots and tests do not churn
    write(&out, "full", &s);

    // Task detail of the newest Claude Code task (three prompts, a tip fired on it).
    let id = &s.tasks.iter().find(|t| t.tool == "claude-code" && t.prompts == 3).unwrap().task_id;
    let mut det = aiq_home::task_detail(&full, &pack, now, id).unwrap();
    det.read_ms = 9;
    std::fs::write(out.join("task.json"), serde_json::to_string_pretty(&det).unwrap()).unwrap();

    s.status.core = false;
    write(&out, "down", &s);

    aiq_core::control::set_paused(&full, true, std::time::Duration::from_millis(20)).unwrap();
    let mut p = aiq_home::snapshot(&full, &pack, now);
    p.status.core = true;
    p.status.read_ms = 12;
    write(&out, "paused", &p);

    // Coaching from rung results: kinds of task with next steps, habits, the split (design coaching-from-rungs).
    let coached = Paths::at(&tmp.join("coaching"));
    seed::coaching(&coached);
    aiq_core::control::finish_setup(&coached).unwrap();
    let mut c = aiq_home::snapshot(&coached, &pack, now);
    c.status.core = true;
    c.status.read_ms = 12;
    write(&out, "coaching", &c);

    let empty = Paths::at(&tmp.join("empty"));
    let mut e = aiq_home::snapshot(&empty, &pack, now);
    e.status.core = true;
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
    std::fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(&s).unwrap()).unwrap();
}
