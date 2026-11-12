//! Live side of the import spec's automated rule (docs/specs/history-import.md, US-12, H4): a session
//! that an app or script started is not the person's work, so AIQ records nothing for it and never
//! coaches it. Pure: time and the persisted set-aside folders come in as arguments.
//!
//! Live, AIQ sees one prompt at a time, so the rule is applied at a session's first prompt:
//! - **B, burst**: `burst_sessions`+ sessions started in the folder within `burst_min` minutes.
//! - **T, template**: the normalized first prompt matches `template_sessions`+ sessions in the folder
//!   within `template_window_h` hours.
//! - **F, folder prior**: the folder is set aside (persisted by the caller) only on folder-level evidence: a
//!   burst, or a template whose sessions are at least `FOLDER_SHARE` of the folder's recent sessions. Then every
//!   later session there is held at its first prompt. A template in a folder a person also works in sets aside
//!   only the matching sessions.
//! - **H ≤ 1**: a set-aside session that sends a second prompt is a person, so AIQ records it from then on,
//!   first prompt included (`Verdict::Rescued`: the caller kept the held first prompt in memory).
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
/// A template sets its folder aside only when its sessions are at least this share of the folder's sessions in
/// the window, the same bar as the import's folder prior. Below it, a person works there too.
/// assume: an app folder is over 80% app runs. Measured on Imran's laptop 2026-11-03: executive_presence 99%,
/// foresight 91%, AIQ's temp checks 100%; tacit 59% (9 of 22 sessions a person's). If false for some folder:
/// only its matching sessions are set aside, which is the safe side.
pub const FOLDER_SHARE: f64 = 0.8;

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
    /// Record this prompt, after the session's held first prompt: a set-aside session sent a second prompt.
    Rescued,
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

/// A short, stable tag for a folder (FNV-1a 32-bit of `folder_key`, 8 hex digits) for the hook's timing log,
/// so `aiq status` can leave out runs from set-aside folders without the log holding folder paths.
/// None when there is no folder. Two folders sharing a tag is about 1 in 4 billion per pair; the cost of one
/// is that a person's own runs drop out of the hook figures, and the left-out count shows it.
pub fn folder_tag(cwd: &str) -> Option<String> {
    let k = folder_key(cwd);
    if k.is_empty() {
        return None;
    }
    Some(short_tag(&k))
}

/// A session's tag for the hook's timing log (same hash as `folder_tag`), so `aiq status` can apply the rule's
/// per-session verdict (H ≥ 2) to hook runs without the log holding session ids. None for an empty id.
pub fn session_tag(session: &str) -> Option<String> {
    (!session.is_empty()).then(|| short_tag(session))
}

/// FNV-1a 32-bit, 8 hex digits.
fn short_tag(s: &str) -> String {
    let h = s.bytes().fold(0x811c_9dc5_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
    format!("{h:08x}")
}

/// Rule AIQ: a folder AIQ's own checks and probes run in, `<temp>\aiq-*` (or below it), e.g.
/// `%TEMP%\aiq-phase2-check` or `%TEMP%\aiq-copilot-probe-3glkf9fj\run0`. Those sessions are tests of AIQ, not
/// the person's work, and carry no other marker (Copilot CLI 1.0.104 `session.start` has none; the probes
/// were run by hand or by Remote Control, not by AIQ, so `AIQ_INTERNAL` was not set). Measured: 8 of the
/// imported Copilot sessions on Imran's laptop (2026-11-02), all in such folders.
/// assume: nobody does their own work in a temp folder whose name starts with `aiq-`. If false: those
/// sessions are set aside, and `aiq include <folder>` brings them back.
pub fn is_aiq_check_folder(cwd: &str) -> bool {
    let k = folder_key(cwd).to_lowercase();
    let parts: Vec<&str> = k.split(['\\', '/']).collect();
    parts.windows(2).any(|w| matches!(w[0], "temp" | "tmp") && w[1].starts_with("aiq-"))
}

/// Rule FAST: a prompt that follows the same session's turn end within `FAST_AFTER_STOP_MS` and is at least
/// `FAST_MIN_CHARS` long was not typed: a turn-end hook sent it to keep the agent going. Measured on the laptop
/// 2026-11-12 over 217 live prompts: 28 matched (24 from one Stop hook that asks for a list before the agent
/// stops, 1.3 to 4.6 s after the Stop, 394 characters each; 2 more hook texts; 2 relayed messages), and none was
/// typed. A pasted prompt (`text::Features::pasted`) is never matched: a person may paste an error the moment a
/// turn ends. The Stop carries no sign of a hook (`stop_hook_active` was false on all 159 VS Code Stops).
/// assume: nobody types 150 characters within 5 seconds of a turn ending, and a message queued while the agent
/// works reaches the hook when it is typed, not when the turn ends. Unmeasured: VS Code and Copilot CLI can queue
/// a message. If false: that turn is left out as "not typed" (Home's left-out line shows the count), and
/// `aiq include <folder>` brings its folder back.
pub const FAST_AFTER_STOP_MS: i64 = 5_000;
pub const FAST_MIN_CHARS: usize = 150;

/// Rule FAST for one prompt: `gap_ms` since the session's last turn end (None when there was none).
pub fn sent_by_turn_end_hook(gap_ms: Option<i64>, chars: usize, pasted: bool) -> bool {
    !pasted && chars >= FAST_MIN_CHARS && gap_ms.is_some_and(|g| (0..FAST_AFTER_STOP_MS).contains(&g))
}

/// Rule TOOL: openings the tool itself writes into a session as a prompt. Copilot CLI sends a background agent's or
/// shell's end as `<system_notification> Agent "…" has finished…` and a scheduled run as `[Scheduled prompt #N] …`.
/// Measured 2026-11-12 over the laptop store with the Devbox's synced rows: 31 notifications and 26 scheduled prompts,
/// none typed (each followed a background run or a schedule the person had set).
/// assume: nobody types a prompt that opens with these exact marks. If false: that prompt is left out as "not typed"
/// (Home's left-out line shows the count).
pub const TOOL_OPENINGS: [&str; 2] = ["<system_notification>", "[scheduled prompt #"];

/// Rule TOOL for one prompt: it opens with one of `TOOL_OPENINGS` (case and leading space ignored).
pub fn written_by_tool(prompt: &str) -> bool {
    let head: String = prompt.trim_start().chars().take(24).collect::<String>().to_lowercase();
    TOOL_OPENINGS.iter().any(|o| head.starts_with(o))
}

/// Rule WRAP: openings of a message envelope that tooling around an agent writes as the prompt (a project message
/// delivered to a Claude Code session, a relay between Claude sessions, a task notification, a scheduled task).
/// Measured 2026-11-12 over the laptop store: 38 Claude Code prompts opened with one of these. 25 `<wake …>` carried
/// the person's own message (`<message … from="human">`); the other 13 carried none.
pub const ENVELOPE_OPENINGS: [&str; 6] =
    ["<wake ", "<relay ", "<coordinator-relay", "<task-notification", "<scheduled-task", "<project_claude_message"];

/// What a prompt says the person typed: the prompt itself; for a `<wake …>` envelope, the person's own message in it
/// (`text::own_words`, the one definition); None for an envelope with no message from a person (not typed).
pub fn person_words(prompt: &str) -> Option<std::borrow::Cow<'_, str>> {
    let t = prompt.trim_start();
    if !ENVELOPE_OPENINGS.iter().any(|o| t.starts_with(o)) {
        return Some(std::borrow::Cow::Borrowed(prompt));
    }
    match crate::text::own_words(prompt) {
        w @ std::borrow::Cow::Owned(_) => Some(w),
        std::borrow::Cow::Borrowed(_) => None,
    }
}

/// Rule AGENT: the session ids a first prompt names, other than its own. A sub-agent's brief names the session that
/// started it ("Parent session a1b2c3d4-… owns integration", or a work folder under that session's state folder).
/// Measured 2026-11-12, every live session's first prompt in the laptop store (Devbox rows synced in): of 118 Devbox
/// Copilot CLI sessions, all 104 sub-agent briefs named a session AIQ already had on the same tool, and none of the
/// 14 the person started did; on the laptop the 2 that did were Copilot's own autopilot checks ("Independently verify
/// whether this autopilot objective has been fully satisfied"), also not typed. The caller keeps only ids it knows.
/// assume: a person rarely opens a new session by pasting another session's id. If false: that session is left out
/// as an agent's (Home's left-out line shows the count).
pub fn named_sessions(prompt: &str, own: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let b = prompt.as_bytes();
    let mut i = 0;
    while i + 36 <= b.len() {
        let boundary = i == 0 || !b[i - 1].is_ascii_hexdigit();
        let cand = &prompt.get(i..i + 36);
        let after_ok = b.get(i + 36).is_none_or(|c| !c.is_ascii_hexdigit() && *c != b'-');
        if let (true, Some(c), true) = (boundary, cand, after_ok)
            && is_guid(c)
        {
            let id = c.to_ascii_lowercase();
            if !id.eq_ignore_ascii_case(own) && !out.contains(&id) {
                out.push(id);
            }
            i += 36;
            continue;
        }
        i += 1;
    }
    out
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

/// The share of the window's sessions whose first-prompt template has `min`+ sessions in the window.
fn template_share(q: &VecDeque<(i64, Option<u64>)>, min: usize) -> f64 {
    if q.is_empty() {
        return 0.0;
    }
    let mut n: HashMap<u64, usize> = HashMap::new();
    for h in q.iter().filter_map(|(_, h)| *h) {
        *n.entry(h).or_default() += 1;
    }
    let in_template = q.iter().filter(|(_, h)| h.is_some_and(|h| n[&h] >= min)).count();
    in_template as f64 / q.len() as f64
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
            // A second prompt: H ≥ 2, so a person is driving this session. Record from now on, first prompt too.
            self.seen.insert(key);
            return Verdict::Rescued;
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
        // Folder-level evidence: a burst is app volume by itself; a template must be most of the folder's sessions.
        let folder_level = burst || template_share(&starts.q, p.template_sessions as usize) >= FOLDER_SHARE;
        self.ignore(key);
        Verdict::Ignore { reason, new_folder: folder_level.then_some(folder) }
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

    #[test]
    fn a_long_prompt_just_after_a_turn_end_was_sent_by_a_hook() {
        // The measured hook: 394 characters, 1.3 to 4.6 s after the Stop.
        assert!(sent_by_turn_end_hook(Some(1_346), 394, false));
        assert!(sent_by_turn_end_hook(Some(4_572), 394, false));
        assert!(!sent_by_turn_end_hook(Some(5_000), 394, false), "5 s or more: a person may have typed it");
        assert!(!sent_by_turn_end_hook(Some(1_000), 149, false), "short: a person may type it that fast");
        assert!(!sent_by_turn_end_hook(Some(1_000), 1_285, true), "pasted: a person may paste the moment a turn ends");
        assert!(!sent_by_turn_end_hook(None, 394, false), "no turn end yet: a first prompt");
        assert!(!sent_by_turn_end_hook(Some(-10), 394, false), "a prompt stamped before the turn end");
    }

    #[test]
    fn tool_written_openings_are_not_typed() {
        assert!(written_by_tool("<system_notification> Agent \"x\" has finished processing and is now idle."));
        assert!(written_by_tool("  [Scheduled prompt #3] Continue the implementation"));
        assert!(written_by_tool("[scheduled prompt #1] continue"));
        assert!(!written_by_tool("keep going"));
        assert!(!written_by_tool("why did the scheduled prompt #3 fail?"), "only the opening counts");
        assert!(!written_by_tool("what does <system_notification> mean here?"));
    }

    #[test]
    fn an_envelope_counts_only_the_persons_own_words() {
        let wake = "<wake reason=\"mention\" current-time=\"2026-11-12T14:32:26Z\">\n  <project id=\"p\">\n    <thread ts=\"t\">\n      <message trigger=\"true\" from=\"human\" trust=\"principal\" id=\"m1\">why does it say &#34;Sign in&#34;?</message>\n    </thread>\n  </project>\n</wake>";
        assert_eq!(person_words(wake).as_deref(), Some("why does it say \"Sign in\"?"));
        let system = "<wake reason=\"spawn\"><message trigger=\"true\" from=\"system\">You are a thread session.</message></wake>";
        assert_eq!(person_words(system), None, "a message from the system is not typed");
        assert_eq!(person_words("<relay from=\"coordinator\"><note>Do the thing.</note></relay>"), None);
        assert_eq!(person_words("<task-notification> <task-id>b5</task-id></task-notification>"), None);
        assert_eq!(person_words("<scheduled-task name=\"x\">run it</scheduled-task>"), None);
        let two = "<wake reason=\"mention\"><message from=\"human\">one</message><message from=\"human\">two</message></wake>";
        assert_eq!(person_words(two).as_deref(), Some("one\ntwo"));
        assert_eq!(person_words("fix the <b> tag in index.html").as_deref(), Some("fix the <b> tag in index.html"));
        assert_eq!(
            person_words("<div> is wrapping wrong").as_deref(),
            Some("<div> is wrapping wrong"),
            "not an envelope"
        );
    }

    #[test]
    fn a_brief_naming_another_session_lists_that_id_once() {
        let own = "11111111-2222-3333-4444-555555555555";
        let parent = "a1b2c3d4-4040-446f-ab83-9bed2f4ecfb0";
        let brief = format!(
            "Implement one bounded slice in C:\\Users\\me\\.copilot\\session-state\\{parent}\\files\\x. Parent session {} owns integration. You are {own}.",
            parent.to_uppercase()
        );
        assert_eq!(named_sessions(&brief, own), vec![parent.to_owned()]);
        assert!(named_sessions("keep going", own).is_empty());
        assert!(named_sessions(&format!("my session {own}"), own).is_empty(), "its own id is not another session");
        assert!(named_sessions("hash 0a1b2c3d4-4040-446f-ab83-9bed2f4ecfb0", own).is_empty(), "longer hex run");
        assert!(named_sessions("a1b2c3d4-4040-446f-ab83-9bed2f4ecfb0-77", own).is_empty(), "longer dashed run");
        assert!(named_sessions("c0474ae7-4040-446f-ab83-9bed2f4ecfbz", own).is_empty(), "not hex");
    }

    #[test]
    fn folder_tag_matches_folder_key_not_spelling() {
        let t = folder_tag(r"C:\Repos\Tacit\").unwrap();
        assert_eq!(t.len(), 8);
        assert_eq!(folder_tag(r"c:/repos/tacit"), Some(t.clone()), "same folder, other spelling");
        assert_eq!(folder_tag(&folder_key(r"C:\Repos\Tacit")), Some(t.clone()), "a stored folder key tags the same");
        assert_ne!(folder_tag(r"c:\repos\tacit2"), Some(t));
        assert_eq!(folder_tag("  "), None);
    }

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
    fn aiq_check_folders_are_temp_aiq_dash_only() {
        for yes in [
            r"C:\Users\imrans\AppData\Local\Temp\aiq-phase2-check",
            r"c:\users\imrans\appdata\local\temp\aiq-copilot-probe-3glkf9fj\run0",
            r"C:\Users\imrans\AppData\Local\Temp\aiq-copilot-judge-probe\",
            "/tmp/aiq-judge-check/src",
        ] {
            assert!(is_aiq_check_folder(yes), "{yes}");
        }
        for no in [
            r"C:\Users\imrans\source\repos\AIQ",
            r"C:\Users\imrans\source\repos\aiq-tools",
            r"C:\Users\imrans\AppData\Local\Temp\other\aiq-x",
            r"C:\Users\imrans\AppData\Local\Temp\myaiq-x",
            "/home/x/tmpaiq-y",
            "",
        ] {
            assert!(!is_aiq_check_folder(no), "{no}");
        }
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
    fn template_in_a_folder_a_person_also_works_in_sets_aside_only_its_sessions() {
        // tacit's shape (measured 2026-11-03): a person's sessions next to an app's repeated prompt.
        let mut g = Guard::new();
        for i in 0..5 {
            let t = format!("my own question {}", "q".repeat(i + 1));
            let v = g.on_prompt(&p(), "copilot-cli", &format!("me{i}"), Some("/tacit"), Some(&t), false, false, 0);
            assert_eq!(v, Verdict::Record);
        }
        for i in 0..11 {
            let v = g.on_prompt(
                &p(),
                "copilot-cli",
                &format!("app{i}"),
                Some("/tacit"),
                Some(&format!("distil session {i}")),
                false,
                false,
                (i + 1) * MIN,
            );
            if i < 10 {
                assert_eq!(v, Verdict::Record, "app {i}: before the template shows");
            } else {
                // 11 of 16 window sessions (69%) are the template: below the folder bar.
                assert_eq!(v, Verdict::Ignore { reason: Reason::Template, new_folder: None });
            }
        }
        // Enough app runs to pass 80% of the window: now the folder is set aside.
        let mut folder = None;
        for i in 11..30 {
            if let Verdict::Ignore { new_folder: Some(f), .. } = g.on_prompt(
                &p(),
                "copilot-cli",
                &format!("app{i}"),
                Some("/tacit"),
                Some(&format!("distil session {i}")),
                false,
                false,
                (i + 1) * MIN,
            ) {
                folder.get_or_insert((i, f));
            }
        }
        // 20 template of 25 = 80%: the 20th app run (i = 19) is the first at the bar.
        assert_eq!(folder, Some((19, "/tacit".to_owned())));
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
            Verdict::Rescued,
            "the caller records the held first prompt, then this one"
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
