//! Scrubbing for the diagnostics file (design docs/design/diagnostics-file.md). Every free-text line that goes
//! into a file a person may share (log lines, a tool's error message, and prompts when the person chose to add
//! them) passes through [`Scrub::line`]: secrets first, then the person's own names and paths, then any web
//! address that is not this project's public repository.
//!
//! simplify: hand-written byte scanner, as in `secrets` (std-only crate, D3). Upgrade trigger: a shape that needs
//! alternation or look-around, or more than about 10 rules.
use crate::secrets;

/// What replaces each kind of removed text.
pub const PROFILE: &str = "%USERPROFILE%";
pub const PATH: &str = "<path>";
pub const EMAIL: &str = "<email>";
pub const URL: &str = "<url>";
pub const USER: &str = "<user>";
pub const COMPUTER: &str = "<computer>";

/// The one web address kept as is: the project's public repository (release and update links).
const KEPT_URL: &str = "https://github.com/imransargusingh/";

/// The person's own names, read once by the caller (environment, never guessed here).
#[derive(Debug, Clone, Default)]
pub struct Scrub {
    /// The profile folder (`%USERPROFILE%` or `$HOME`); every spelling of it becomes [`PROFILE`].
    pub home: Option<String>,
    /// The sign-in name (`%USERNAME%` / `$USER`).
    pub user: Option<String>,
    /// The computer's name (`%COMPUTERNAME%` / `$HOSTNAME`).
    pub computer: Option<String>,
    /// Exact strings that must never appear (the team tips address); each becomes [`URL`].
    pub never: Vec<String>,
}

impl Scrub {
    /// One line or message, scrubbed and cut to `max` characters (an ellipsis marks the cut).
    pub fn line(&self, s: &str, max: usize) -> String {
        let (s, _) = secrets::redact(s);
        let mut s = s;
        for n in self.never.iter().filter(|n| !n.trim().is_empty()) {
            s = replace_nocase(&s, n, URL);
        }
        if let Some(h) = self.home.as_deref().map(|h| h.trim_end_matches(['\\', '/'])).filter(|h| h.len() > 3) {
            // As typed, with the other slash, and as it reads inside a JSON log line (backslashes doubled).
            for v in [h.to_owned(), h.replace('\\', "/"), h.replace('/', "\\"), h.replace('\\', "\\\\")] {
                s = replace_nocase(&s, &v, PROFILE);
            }
        }
        let s = urls(&s);
        let s = emails(&s);
        let s = paths(&s);
        let mut s = s;
        // The computer first: its name often holds the user name ("DANA-PC7").
        for (name, mark) in [(&self.computer, COMPUTER), (&self.user, USER)] {
            if let Some(n) = name.as_deref().map(str::trim).filter(|n| n.chars().count() >= 3) {
                s = replace_word_nocase(&s, n, mark);
            }
        }
        cut(&s, max)
    }
}

fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// Replace every case-insensitive (ASCII) occurrence of `needle`.
fn replace_nocase(s: &str, needle: &str, with: &str) -> String {
    if needle.is_empty() {
        return s.to_owned();
    }
    let (hay, pat) = (s.to_ascii_lowercase(), needle.to_ascii_lowercase());
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    let mut from = 0;
    while let Some(i) = hay[from..].find(&pat).map(|i| i + from) {
        out.push_str(&s[last..i]);
        out.push_str(with);
        last = i + pat.len();
        from = last;
    }
    out.push_str(&s[last..]);
    out
}

fn word(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// As [`replace_nocase`], only where the match is a whole word (so the user "dan" leaves "dangerous" alone).
fn replace_word_nocase(s: &str, needle: &str, with: &str) -> String {
    let (hay, pat) = (s.to_ascii_lowercase(), needle.to_ascii_lowercase());
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    let mut from = 0;
    while let Some(i) = hay[from..].find(&pat).map(|i| i + from) {
        let end = i + pat.len();
        if !word(s[..i].chars().next_back()) && !word(s[end..].chars().next()) {
            out.push_str(&s[last..i]);
            out.push_str(with);
            last = end;
        }
        from = end;
    }
    out.push_str(&s[last..]);
    out
}

/// Where a path, address or email ends: whitespace, a quote, or closing punctuation.
fn stop(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | ')' | ']' | '}' | ',' | ';' | '|')
}

/// Rewrite each token that starts at a boundary and that `take` claims (returns its byte length and the
/// replacement). `boundary` is the previous char (None at the start).
fn rewrite(s: &str, take: impl Fn(&str, Option<char>) -> Option<(usize, String)>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev: Option<char> = None;
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        if let Some((n, with)) = take(rest, prev) {
            out.push_str(&with);
            prev = with.chars().next_back();
            i += n.max(1);
            continue;
        }
        let c = rest.chars().next().expect("non-empty");
        out.push(c);
        prev = Some(c);
        i += c.len_utf8();
    }
    out
}

fn token_len(s: &str) -> usize {
    s.char_indices().find(|(_, c)| stop(*c)).map_or(s.len(), |(i, _)| i)
}

fn urls(s: &str) -> String {
    rewrite(s, |rest, prev| {
        if word(prev) {
            return None;
        }
        let lower = rest.get(..8).map(str::to_ascii_lowercase).unwrap_or_default();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return None;
        }
        let n = token_len(rest);
        let t = &rest[..n];
        Some((n, if t.to_ascii_lowercase().starts_with(KEPT_URL) { t.to_owned() } else { URL.to_owned() }))
    })
}

fn email_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
}

fn emails(s: &str) -> String {
    rewrite(s, |rest, prev| {
        if prev.is_some_and(email_char) {
            return None;
        }
        let local = rest.char_indices().find(|(_, c)| !email_char(*c)).map_or(rest.len(), |(i, _)| i);
        if local == 0 || !rest[local..].starts_with('@') {
            return None;
        }
        let dom = &rest[local + 1..];
        let dlen = dom
            .char_indices()
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-')))
            .map_or(dom.len(), |(i, _)| i);
        let d = dom[..dlen].trim_end_matches('.');
        let tld = d.rsplit('.').next().unwrap_or("");
        if !d.contains('.') || tld.len() < 2 || !tld.chars().all(|c| c.is_ascii_alphabetic()) {
            return None;
        }
        Some((local + 1 + d.len(), EMAIL.to_owned()))
    })
}

fn paths(s: &str) -> String {
    rewrite(s, |rest, prev| {
        if prev.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '%' | '/' | '\\' | ':' | '.' | '-')) {
            return None;
        }
        let b = rest.as_bytes();
        // C:\…  C:/…  C:\\… (as in JSON)
        let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/');
        // \\server\share  (or \\\\server in JSON)
        let unc = b.len() >= 3 && b[0] == b'\\' && b[1] == b'\\' && (b[2].is_ascii_alphanumeric() || b[2] == b'\\');
        // /a/b: at least two segments, so "and/or" or a lone "/" stays.
        let unix = b.first() == Some(&b'/') && {
            let t = &rest[..token_len(rest)];
            t.len() > 1
                && t[1..].contains('/')
                && t.as_bytes().get(1).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'.')
        };
        if !(drive || unc || unix) {
            return None;
        }
        Some((token_len(rest), PATH.to_owned()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> Scrub {
        Scrub {
            home: Some(r"C:\Users\dana".into()),
            user: Some("dana".into()),
            computer: Some("DANA-PC7".into()),
            never: vec!["https://example.internal/team/tips.git".into()],
        }
    }

    #[test]
    fn the_profile_folder_in_every_spelling_becomes_userprofile() {
        let s = me();
        assert_eq!(s.line(r"read C:\Users\dana\.aiq\label.log", 200), r"read %USERPROFILE%\.aiq\label.log");
        assert_eq!(s.line("read c:/users/DANA/.copilot/x", 200), "read %USERPROFILE%/.copilot/x");
        assert_eq!(s.line(r#"{"file":"C:\\Users\\dana\\a.txt"}"#, 200), r#"{"file":"%USERPROFILE%\\a.txt"}"#);
    }

    #[test]
    fn other_absolute_paths_become_path() {
        let s = me();
        assert_eq!(s.line(r"cd D:\work\secret-project\src", 200), "cd <path>");
        assert_eq!(s.line(r"open \\fileserver\team\plan.docx now", 200), "open <path> now");
        assert_eq!(s.line("cat /home/someone/proj/x.rs", 200), "cat <path>");
        assert_eq!(s.line(r#"{"cwd":"C:\\src\\contoso"}"#, 200), r#"{"cwd":"<path>"}"#);
    }

    #[test]
    fn ordinary_slashes_stay() {
        let s = me();
        assert_eq!(s.line("label t1 copilot-cli error -/- ms=10", 200), "label t1 copilot-cli error -/- ms=10");
        assert_eq!(s.line("code_change/fix_bug and/or 11/10 a / b", 200), "code_change/fix_bug and/or 11/10 a / b");
    }

    #[test]
    fn emails_and_web_addresses_go_but_the_public_repo_stays() {
        let s = me();
        assert_eq!(s.line("signed in as dana.k@contoso.com.", 200), "signed in as <email>.");
        assert_eq!(s.line("see https://dev.azure.com/org/x?y=1 now", 200), "see <url> now");
        assert_eq!(
            s.line("get https://github.com/imransargusingh/AIQ/releases/latest", 200),
            "get https://github.com/imransargusingh/AIQ/releases/latest"
        );
        assert_eq!(s.line("fetch HTTPS://EXAMPLE.internal/team/tips.git failed", 200), "fetch <url> failed");
        assert_eq!(s.line("no @ here, a@b and x@y.z9", 200), "no @ here, a@b and x@y.z9");
    }

    #[test]
    fn the_team_tips_address_never_survives_in_any_form() {
        let s = me();
        let out = s.line("clone https://example.internal/team/tips.git: 403", 200);
        assert!(!out.to_lowercase().contains("example.internal"), "{out}");
    }

    #[test]
    fn user_and_computer_names_go_as_whole_words_only() {
        let s = me();
        assert_eq!(s.line("account dana on DANA-PC7 is dangerous", 200), "account <user> on <computer> is dangerous");
    }

    #[test]
    fn secrets_go_first() {
        let s = me();
        let out = s.line("token=ghp_abcdefghijklmnopqrstuvwxyz0123456789", 200);
        assert!(out.contains(secrets::MARKER) && !out.contains("ghp_"), "{out}");
    }

    #[test]
    fn long_lines_are_cut_with_an_ellipsis() {
        let s = Scrub::default();
        assert_eq!(s.line(&"x".repeat(10), 4), "xxxx…");
        assert_eq!(s.line("abc", 4), "abc");
    }

    #[test]
    fn non_ascii_text_is_kept_whole() {
        let s = me();
        assert_eq!(s.line("état — ok ✓ C:\\Données\\x", 200), "état — ok ✓ <path>");
    }
}
