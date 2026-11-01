//! In-process error and event counters, keyed by stable code. Labels are bounded (codes only, O13).
use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct Counters(Mutex<BTreeMap<&'static str, u64>>);

impl Counters {
    pub fn inc(&self, code: &'static str) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()).entry(code).or_insert(0) += 1;
    }
    pub fn get(&self, code: &str) -> u64 {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).get(code).copied().unwrap_or(0)
    }
    pub fn snapshot(&self) -> Vec<(String, u64)> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }
}
