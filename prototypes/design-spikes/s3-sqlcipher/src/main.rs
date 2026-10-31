//! Spike S3b: SQLCipher through rusqlite `bundled-sqlcipher-vendored-openssl` (build check + behaviour).
use rusqlite::Connection;
use std::time::Instant;

fn main() {
    let path = std::env::temp_dir().join(format!("aiq-spike-{}.db", std::process::id()));
    let canary = "canary-prompt-7f3a";
    let t = Instant::now();
    {
        let c = Connection::open(&path).expect("open");
        c.pragma_update(None, "key", "spike-key-1").expect("key");
        c.execute_batch("CREATE TABLE content(event_id TEXT PRIMARY KEY, text TEXT);").expect("create");
        c.execute("INSERT INTO content VALUES ('e1', ?1)", [canary]).expect("insert");
    }
    let raw = std::fs::read(&path).expect("read file");
    let plaintext_found = raw.windows(canary.len()).any(|w| w == canary.as_bytes());
    let wrong = Connection::open(&path).and_then(|c| {
        c.pragma_update(None, "key", "wrong-key")?;
        c.query_row("SELECT count(*) FROM content", [], |r| r.get::<_, i64>(0))
    });
    let right = Connection::open(&path).and_then(|c| {
        c.pragma_update(None, "key", "spike-key-1")?;
        c.query_row("SELECT text FROM content WHERE event_id='e1'", [], |r| r.get::<_, String>(0))
    });
    let version = Connection::open_in_memory()
        .and_then(|c| c.query_row("PRAGMA cipher_version", [], |r| r.get::<_, String>(0)));
    println!("cipher_version: {version:?}");
    println!("plaintext_in_file: {plaintext_found}");
    println!("wrong_key_read: {}", if wrong.is_err() { "fails (expected)" } else { "SUCCEEDED (bad)" });
    println!("right_key_read: {:?}", right.map(|s| s == canary));
    println!("elapsed_ms: {}", t.elapsed().as_millis());
    let _ = std::fs::remove_file(&path);
}
