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
    let mut s = aiq_home::snapshot(&full, &pack, now);
    s.status.core = true;
    s.status.read_ms = 12; // fixed, so screenshots and tests do not churn
    write(&out, "full", &s);

    s.status.core = false;
    write(&out, "down", &s);

    aiq_core::control::set_paused(&full, true, std::time::Duration::from_millis(20)).unwrap();
    let mut p = aiq_home::snapshot(&full, &pack, now);
    p.status.core = true;
    p.status.read_ms = 12;
    write(&out, "paused", &p);

    let empty = Paths::at(&tmp.join("empty"));
    let mut e = aiq_home::snapshot(&empty, &pack, now);
    e.status.core = true;
    e.data.location = r"C:\Users\you\.aiq".into();
    write(&out, "empty", &e);
    let _ = std::fs::remove_dir_all(&tmp);
}

fn write(dir: &std::path::Path, name: &str, s: &aiq_home::Snapshot) {
    let mut s = s.clone();
    s.data.location = r"C:\Users\you\.aiq".into();
    std::fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(&s).unwrap()).unwrap();
}
