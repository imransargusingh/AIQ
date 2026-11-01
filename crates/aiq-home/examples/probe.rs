//! Measure snapshot read times against a real store, read-only: `probe [reads] [interval_ms]`.
//! Uses the same data folder as the app (AIQ_HOME or the default). Prints timings only, never data.
use aiq_core::pack::BundledPack;
use aiq_core::paths::Paths;
use aiq_core::sysclock::SystemClock;
use aiq_domain::PackSource;
use aiq_domain::clock::Clock;

fn main() {
    let mut args = std::env::args().skip(1);
    let reads: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(60);
    let every = std::time::Duration::from_millis(args.next().and_then(|a| a.parse().ok()).unwrap_or(1000));
    let paths = Paths::from_env();
    let pack = BundledPack.load().unwrap();
    let (mut total, mut open) = (Vec::new(), Vec::new());
    for i in 0..reads {
        let s = aiq_home::snapshot(&paths, &pack, SystemClock.now());
        if let Some(code) = &s.status.error {
            println!("read {i}: error {}", code.split(':').next().unwrap_or(""));
        }
        println!("read {i}: read_ms={} open_ms={}", s.status.read_ms, s.status.open_ms);
        total.push(s.status.read_ms);
        open.push(s.status.open_ms);
        std::thread::sleep(every);
    }
    for (name, mut v) in [("read_ms", total), ("open_ms", open)] {
        v.sort_unstable();
        let at = |p: f64| v[((v.len() as f64 - 1.0) * p).round() as usize];
        println!("{name}: min {} median {} p95 {} max {} (n={})", v[0], at(0.5), at(0.95), v[v.len() - 1], v.len());
    }
}
