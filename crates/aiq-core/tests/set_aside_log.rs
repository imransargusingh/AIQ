//! Its own test binary: tracing caches each callsite's interest process-wide, so a test that captures logs
//! races every other test that hits the same callsite with no subscriber.
use aiq_contracts::event::{EventBody, EventV1, PromptPayload, SCHEMA};
use aiq_core::counters::Counters;
use aiq_core::pack::BundledPack;
use aiq_core::service::Service;
use aiq_core::sysclock::format_rfc3339_ms;
use aiq_domain::PackSource;
use aiq_domain::clock::FakeClock;
use aiq_store::Store;
use std::sync::{Arc, Mutex};

const T0: i64 = 1_793_448_000_000; // 2026-10-31T12:00:00Z

#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for Buf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A dropped event leaves one log line naming its rule (2026-11-03: six first prompts of hand-typed VS Code
/// sessions were dropped in a set-aside folder and nothing said why). The line never holds text or a path.
#[test]
fn a_set_aside_prompt_logs_its_reason_without_text_or_path() {
    let buf = Buf::default();
    let w = buf.clone();
    let sub = tracing_subscriber::fmt().with_ansi(false).with_writer(move || w.clone()).finish();
    tracing::subscriber::set_global_default(sub).unwrap();
    let d = tempfile::TempDir::new().unwrap();
    let store = Store::open(&d.path().join("aiq.db")).unwrap();
    store.insert_folder_rule("/ep", "set_aside", "template", "live", T0).unwrap();
    let mut svc =
        Service::new(store, BundledPack.load().unwrap(), FakeClock::new(T0, 0), Arc::new(Counters::default())).unwrap();
    let ev = EventV1 {
        schema: SCHEMA.into(),
        event_id: "e1".into(),
        surface: "vscode".into(),
        surface_session_id: "s1".into(),
        seq: None,
        ts: format_rfc3339_ms(T0),
        collector_version: "test".into(),
        cwd: Some("/ep".into()),
        projects: false,
        transcript: None,
        body: EventBody::Prompt(PromptPayload {
            text: Some("a private first prompt".into()),
            chars: 22,
            prompt_id: None,
            truncated: false,
        }),
    };
    svc.ingest(&ev, Some("t1"));
    let log = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
    let line = log.lines().find(|l| l.contains("event set aside")).unwrap_or_else(|| panic!("no line in:\n{log}"));
    for want in ["aiq.reason=\"folder\"", "aiq.kind=\"prompt\"", "aiq.surface=\"vscode\"", "trace_id=\"t1\""] {
        assert!(line.contains(want), "{want} missing: {line}");
    }
    assert!(!line.contains("private first prompt") && !line.contains("/ep"), "{line}");
}
