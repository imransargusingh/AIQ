//! Live side of the import spec's automated rule (docs/specs/history-import.md, US-12, H4): a session
//! that an app or script started is not the person's work, so AIQ records nothing for it and never
//! coaches it. Pure: time and the persisted set-aside folders come in as arguments.
//!
//! Live, AIQ sees one prompt at a time, so the rule is applied at a session's first prompt:
//! - **B, burst**: `burst_sessions`+ sessions started in the folder within `burst_min` minutes.
//! - **T, template**: the normalized first prompt matches `template_sessions`+ sessions in the folder
//!   within `template_window_h` hours.
//! - **F, folder prior**: once B or T fires, the folder is set aside (persisted by the caller), and every
//!   later session there is judged automated at its first prompt.
//! - **H ≤ 1**: a set-aside session that sends a second prompt is a person, so AIQ records it from then on.
//!
//! The windows hold only start times and an in-memory hash of the normalized first prompt; no prompt text.
//! `simplify:` sessions before the rule fires in a new folder (at most `template_sessions - 1` for T,
//! `burst_sessions - 1` for B) are recorded. They have one prompt each, so the per-session detector never
//! fires on them; ceiling: those few rows in a folder's history. Upgrade trigger: task views (Phase 3)
//! or the judge queue read them, then mark them set aside retroactively.
use aiq_contracts::pack::AutomatedParams;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

const MAX_STARTS_PER_FOLDER: usize = 4096;
const MAX_TRACKED_SESSIONS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Burst,
    Template,
    FolderSetAside,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Burst => "burst",
            Reason::Template => "template",
            Reason::FolderSetAside => "folder",
        }
    }
}

/// What to do with one prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Record,
    /// Record nothing. `new_folder` is Some when this prompt made the folder set aside (persist it).
    Ignore {
        reason: Reason,
        new_folder: Option<String>,
    },
}

/// The folder key: separators unified, trailing separator dropped, lowercase when it is a Windows path
/// (Windows paths are case-insensitive). Empty when there is no folder.
pub fn folder_key(cwd: &str) -> String {
    let t = cwd.trim();
    let windows = t.contains('\\') || t.as_bytes().get(1) == Some(&b':');
    let mut k = if windows { t.replace('/', "\\").to_lowercase() } else { t.to_owned() };
    while k.len() > 1 && (k.ends_with('\\') || k.ends_with('/')) && !k.ends_with(":\\") {
        k.pop();
    }
    k
}

/// The import spec's template normalization: URLs, GUIDs, paths, quoted strings and digit runs replaced,
/// whitespace collapsed, lowercase, first 200 chars. None for an empty prompt (empty prompts never group).
pub fn template_of(prompt: &str, max_chars: usize) -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in prompt.split_whitespace() {
        let w = raw.to_lowercase();
        let core = w.trim_matches(|c: char| matches!(c, ',' | ';' | ':' | '.' | '!' | '?' | '(' | ')'));
        let token = if core.starts_with("http://") || core.starts_with("https://") || core.starts_with("www.") {
            "<url>".to_owned()
        } else if is_guid(core) {
            "<guid>".to_owned()
        } else if core.len() > 1 && (core.contains('/') || core.contains('\\')) {
            "<path>".to_owned()
        } else if core.len() >= 2 && matches!(core.chars().next(), Some('"' | '\'' | '`')) {
            "<str>".to_owned()
        } else {
            digits_replaced(&w)
        };
        out.push(token);
    }
    let s: String = out.join(" ").chars().take(max_chars).collect();
    (!s.is_empty()).then_some(s)
}

fn is_guid(s: &str) -> bool {
    let parts: Vec<&str> = s.trim_matches(|c| c == '{' || c == '}').split('-').collect();
    parts.len() == 5
        && parts.iter().zip([8, 4, 4, 4, 12]).all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn digits_replaced(w: &str) -> String {
    let mut s = String::with_capacity(w.len());
    let mut in_digits = false;
    for c in w.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                s.push('#');
            }
            in_digits = true;
        } else {
            s.push(c);
            in_digits = false;
        }
    }
    s
}

fn hash(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[derive(Default)]
struct Starts {
    /// (start utc ms, template hash) of recent sessions in one folder, oldest first.
    q: VecDeque<(i64, Option<u64>)>,
}

#[derive(Default)]
pub struct Guard {
    starts: HashMap<String, Starts>,
    /// Sessions whose prompts are being ignored, and sessions already judged a person's.
    ignored: HashSet<(String, String)>,
    seen: HashSet<(String, String)>,
}

impl Guard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decide for one prompt. `known_session`: the store already holds events for this session (so this is
    /// not its first prompt). `set_aside`: the folder is set aside now (persisted rule, newest action wins).
    #[allow(clippy::too_many_arguments)]
    pub fn on_prompt(
        &mut self,
        p: &AutomatedParams,
        surface: &str,
        session: &str,
        cwd: Option<&str>,
        text: Option<&str>,
        known_session: bool,
        set_aside: bool,
        now_ms: i64,
    ) -> Verdict {
        let key = (surface.to_owned(), session.to_owned());
        if self.ignored.remove(&key) {
            // A second prompt: H ≥ 2, so a person is driving this session. Record from now on.
            self.seen.insert(key);
            return Verdict::Record;
        }
        if known_session || !self.seen.insert(key.clone()) {
            return Verdict::Record;
        }
        if self.seen.len() > MAX_TRACKED_SESSIONS {
            self.seen.clear(); // bounded; a cleared session is re-judged only if its folder is set aside
        }
        let Some(folder) = cwd.map(folder_key).filter(|k| !k.is_empty()) else { return Verdict::Record };
        if set_aside {
            self.ignore(key);
            return Verdict::Ignore { reason: Reason::FolderSetAside, new_folder: None };
        }
        let tpl = text.and_then(|t| template_of(t, p.template_chars)).map(|t| hash(&t));
        let starts = self.starts.entry(folder.clone()).or_default();
        let keep_ms = i64::from(p.template_window_h.max(1)) * 3_600_000;
        while starts.q.front().is_some_and(|(t, _)| *t < now_ms - keep_ms) || starts.q.len() >= MAX_STARTS_PER_FOLDER {
            starts.q.pop_front();
        }
        starts.q.push_back((now_ms, tpl));
        let burst_from = now_ms - i64::from(p.burst_min) * 60_000;
        let burst = starts.q.iter().filter(|(t, _)| *t >= burst_from).count() >= p.burst_sessions as usize;
        let template =
            tpl.is_some() && starts.q.iter().filter(|(_, h)| *h == tpl).count() >= p.template_sessions as usize;
        let reason = match (template, burst) {
            (true, _) => Reason::Template,
            (false, true) => Reason::Burst,
            _ => return Verdict::Record,
        };
        self.ignore(key);
        Verdict::Ignore { reason, new_folder: Some(folder) }
    }

    /// A stop (or any non-prompt event) of a session whose prompts are being ignored is ignored too.
    pub fn is_ignored(&self, surface: &str, session: &str) -> bool {
        self.ignored.contains(&(surface.to_owned(), session.to_owned()))
    }

    fn ignore(&mut self, key: (String, String)) {
        if self.ignored.len() >= MAX_TRACKED_SESSIONS {
            self.ignored.clear();
        }
        self.ignored.insert(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> AutomatedParams {
        AutomatedParams {
            burst_sessions: 50,
            burst_min: 30,
            template_sessions: 11,
            template_window_h: 24,
            template_chars: 200,
        }
    }

    const MIN: i64 = 60_000;

    #[test]
    fn folder_key_normalizes() {
        assert_eq!(folder_key(r"C:\Users\Imran\Executive_Presence\"), r"c:\users\imran\executive_presence");
        assert_eq!(folder_key("C:/Users/Imran/EP"), r"c:\users\imran\ep");
        assert_eq!(folder_key(r"C:\"), r"c:\");
        assert_eq!(folder_key("/home/x/Repo/"), "/home/x/Repo");
        assert_eq!(folder_key("  "), "");
    }

    #[test]
    fn template_replaces_variable_parts() {
        let a = template_of(
            "Fetch https://a.b/c?id=1 for user 4411 into C:\\data\\x.json, id 3f2504e0-4f89-11d3-9a0c-0305e82c3301 \"Acme\"",
            200,
        );
        let b = template_of(
            "fetch https://z.y/q for user 7 into C:\\other\\y.json, id 6ba7b810-9dad-11d1-80b4-00c04fd430c8 'Beta'",
            200,
        );
        assert_eq!(a, b);
        assert_eq!(a.unwrap(), "fetch <url> for user # into <path> id <guid> <str>");
        assert_eq!(template_of("   ", 200), None);
        assert_eq!(template_of("abcdef", 3).unwrap(), "abc");
    }

    #[test]
    fn template_sessions_set_folder_aside_on_the_eleventh() {
        let mut g = Guard::new();
        for i in 0..10 {
            let v = g.on_prompt(
                &p(),
                "copilot-cli",
                &format!("s{i}"),
                Some("C:\\EP"),
                Some(&format!("collect profile {i}")),
                false,
                false,
                i * 3 * MIN,
            );
            assert_eq!(v, Verdict::Record, "session {i}");
        }
        let v =
            g.on_prompt(&p(), "copilot-cli", "s10", Some("C:\\EP"), Some("collect profile 99"), false, false, 31 * MIN);
        assert_eq!(v, Verdict::Ignore { reason: Reason::Template, new_folder: Some("c:\\ep".into()) });
        assert!(g.is_ignored("copilot-cli", "s10"));
    }

    #[test]
    fn burst_of_unique_prompts_sets_folder_aside_on_the_fiftieth() {
        let mut g = Guard::new();
        for i in 0..49 {
            let t = format!("unique {}", "x".repeat(i as usize + 1));
            assert_eq!(
                g.on_prompt(&p(), "copilot-cli", &format!("s{i}"), Some("/ep"), Some(&t), false, false, i * 10_000),
                Verdict::Record
            );
        }
        let v = g.on_prompt(&p(), "copilot-cli", "s49", Some("/ep"), Some("zzz"), false, false, 49 * 10_000);
        assert!(matches!(v, Verdict::Ignore { reason: Reason::Burst, .. }));
    }

    #[test]
    fn slow_unique_sessions_and_other_folders_are_recorded() {
        let mut g = Guard::new();
        for i in 0..60 {
            let folder = if i % 2 == 0 { "/a" } else { "/b" };
            let t = format!("question number {}", "y".repeat(i as usize + 1));
            assert_eq!(
                g.on_prompt(&p(), "copilot-cli", &format!("s{i}"), Some(folder), Some(&t), false, false, i * 2 * MIN),
                Verdict::Record
            );
        }
    }

    #[test]
    fn set_aside_folder_ignores_first_prompt_and_second_prompt_is_a_person() {
        let mut g = Guard::new();
        let v = g.on_prompt(&p(), "copilot-cli", "h1", Some("/ep"), Some("hello"), false, true, 0);
        assert_eq!(v, Verdict::Ignore { reason: Reason::FolderSetAside, new_folder: None });
        assert!(g.is_ignored("copilot-cli", "h1"));
        assert_eq!(
            g.on_prompt(&p(), "copilot-cli", "h1", Some("/ep"), Some("and more"), false, true, MIN),
            Verdict::Record
        );
        assert!(!g.is_ignored("copilot-cli", "h1"));
        assert_eq!(
            g.on_prompt(&p(), "copilot-cli", "h1", Some("/ep"), Some("third"), false, true, 2 * MIN),
            Verdict::Record
        );
    }

    #[test]
    fn known_sessions_and_missing_folder_are_recorded() {
        let mut g = Guard::new();
        assert_eq!(g.on_prompt(&p(), "claude-code", "k", Some("/ep"), Some("x"), true, true, 0), Verdict::Record);
        assert_eq!(g.on_prompt(&p(), "claude-code", "n", None, Some("x"), false, true, 0), Verdict::Record);
    }

    #[test]
    fn empty_first_prompts_never_form_a_template() {
        let mut g = Guard::new();
        for i in 0..20 {
            assert_eq!(
                g.on_prompt(&p(), "copilot-cli", &format!("s{i}"), Some("/ep"), Some(""), false, false, i * 5 * MIN),
                Verdict::Record
            );
        }
    }
}
