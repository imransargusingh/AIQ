//! Secret redaction at ingest (security review S2). People paste API keys, tokens, private keys and
//! connection strings into prompts. Every prompt passes through [`redact`] before it is stored, spooled
//! or put into a judge call, and each secret is replaced with [`MARKER`].
//!
//! A deterministic, tested list of well-known shapes, not a guess at entropy: a missed shape is a
//! residual risk (the store and spool are owner-only), a false hit only blanks a few characters of a
//! prompt. The canary set in the tests must never survive.
//! simplify: hand-written byte scanner instead of a regex crate (the domain crate is std-only, D3, and the
//! hook has a 100 ms p95 budget). Upgrade trigger: more than about 20 shapes, or shapes that need
//! alternation inside a token.

/// What a removed secret is replaced with.
pub const MARKER: &str = "[secret removed]";

/// Token prefixes with the shortest believable tail. The tail is `[A-Za-z0-9_-]` (plus `.` for none).
const PREFIXES: &[(&str, usize)] = &[
    ("sk-", 20),         // Anthropic (sk-ant-…), OpenAI (sk-…, sk-proj-…)
    ("ghp_", 30),        // GitHub personal access token
    ("gho_", 30),        // GitHub OAuth
    ("ghu_", 30),        // GitHub user-to-server
    ("ghs_", 30),        // GitHub server-to-server
    ("ghr_", 30),        // GitHub refresh
    ("github_pat_", 30), // GitHub fine-grained PAT
    ("glpat-", 20),      // GitLab
    ("xoxb-", 10),       // Slack bot
    ("xoxa-", 10),       // Slack app
    ("xoxp-", 10),       // Slack user
    ("xoxr-", 10),       // Slack refresh
    ("xoxs-", 10),       // Slack session
    ("AIza", 35),        // Google API key
    ("npm_", 36),        // npm
    ("AKIA", 16),        // AWS access key id
    ("ASIA", 16),        // AWS temporary access key id
];

/// `key=value` / `key: value` names whose value is a secret (matched without case).
const KEYS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "secret",
    "client_secret",
    "api_key",
    "apikey",
    "api-key",
    "access_token",
    "auth_token",
    "token",
    "accountkey",
    "sharedaccesskey",
    "aws_secret_access_key",
];

/// Longest value, URL password or key-block header looked at. Bounds the scan so a hostile 256 KB prompt
/// stays linear (hook budget); a longer value is still cut at this length.
const MAX_SCAN: usize = 512;

fn tok(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Replace every secret in `s` with [`MARKER`]. Returns the clean text and how many were removed.
pub fn redact(s: &str) -> (String, usize) {
    let b = s.as_bytes();
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if let Some(end) = secret_at(b, i) {
            cuts.push((i, end));
            i = end;
        } else if let Some(end) = url_password_at(b, i) {
            cuts.push((i + 1, end)); // keep the `:`
            i = end;
        } else if let Some((vs, ve)) = keyed_value_at(b, i) {
            cuts.push((vs, ve));
            i = ve;
        } else {
            i += 1;
        }
    }
    if cuts.is_empty() {
        return (s.to_owned(), 0);
    }
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for &(a, z) in &cuts {
        // Every cut starts and ends next to an ASCII byte, so both ends are char boundaries.
        out.push_str(&s[last..a]);
        out.push_str(MARKER);
        last = z;
    }
    out.push_str(&s[last..]);
    (out, cuts.len())
}

/// A whole secret starting at `i` (prefix tokens, private-key blocks, JWTs, bearer tokens):
/// returns its end.
fn secret_at(b: &[u8], i: usize) -> Option<usize> {
    let at_word_start = i == 0 || !tok(b[i - 1]);
    let rest = &b[i..];
    if rest.starts_with(b"-----BEGIN ") {
        let head_end = find(&b[..b.len().min(i + 11 + 64)], i + 11, b"-----")?;
        if b[i + 11..head_end].ends_with(b"PRIVATE KEY") {
            // Unterminated block (cut-off paste): remove to the end.
            return Some(
                find(b, head_end + 5, b"-----END ").and_then(|e| find(b, e + 9, b"-----")).map_or(b.len(), |e| e + 5),
            );
        }
        return None;
    }
    if !at_word_start {
        return None;
    }
    for (p, min) in PREFIXES {
        if rest.starts_with(p.as_bytes()) {
            let n = rest[p.len()..].iter().take_while(|c| tok(**c)).count();
            if n >= *min {
                return Some(i + p.len() + n);
            }
        }
    }
    if rest.starts_with(b"eyJ") {
        // JWT: three base64url segments, the first two starting `eyJ` ({" encoded).
        let seg = |from: usize| from + b[from..].iter().take_while(|c| tok(**c)).count();
        let a = seg(i);
        if a - i >= 10 && b.get(a) == Some(&b'.') && b[a + 1..].starts_with(b"eyJ") {
            let c = seg(a + 1);
            if c - a > 10 && b.get(c) == Some(&b'.') {
                let d = seg(c + 1);
                if d - c > 10 {
                    return Some(d);
                }
            }
        }
    }
    if rest.len() > 7 && rest[..7].eq_ignore_ascii_case(b"bearer ") {
        let n = rest[7..]
            .iter()
            .take_while(|c| tok(**c) || **c == b'.' || **c == b'~' || **c == b'+' || **c == b'/')
            .count();
        if n >= 20 {
            return Some(i + 7 + n);
        }
    }
    None
}

/// `scheme://user:PASSWORD@host`: when `i` is the `:` before the password, the password's end.
fn url_password_at(b: &[u8], i: usize) -> Option<usize> {
    let _ = b.get(i).filter(|c| **c == b':')?;
    // Walk back over the user name to `//`.
    let mut u = i;
    while u > 0 && (tok(b[u - 1]) || b[u - 1] == b'.' || b[u - 1] == b'%') {
        u -= 1;
    }
    if u == i || u < 3 || &b[u - 3..u] != b"://" {
        return None;
    }
    // The password ends at the last `@` before the host (a stray `@` in it is common even though URLs
    // should encode it).
    let span = b[i + 1..].iter().take(MAX_SCAN).take_while(|c| !c.is_ascii_whitespace() && **c != b'/').count();
    let at = b[i + 1..i + 1 + span].iter().rposition(|c| *c == b'@')?;
    (at > 0 && !b[i + 1..].starts_with(MARKER.as_bytes())).then_some(i + 1 + at)
}

/// `password=VALUE`, `"api_key": "VALUE"`, `Password=VALUE;`: when a key name starts at `i`, the value's
/// span (the key stays, so the prompt still reads).
fn keyed_value_at(b: &[u8], i: usize) -> Option<(usize, usize)> {
    if i > 0 && (b[i - 1].is_ascii_alphanumeric()) {
        return None;
    }
    let key = KEYS.iter().find(|k| b.len() >= i + k.len() && b[i..i + k.len()].eq_ignore_ascii_case(k.as_bytes()))?;
    let mut j = i + key.len();
    if b.get(j).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
        return None; // part of a longer word: "tokens", "secretary"
    }
    if b.get(j).is_some_and(|c| *c == b'"' || *c == b'\'') {
        j += 1; // closing quote of a JSON / YAML key
    }
    while b.get(j) == Some(&b' ') {
        j += 1;
    }
    let sep = *b.get(j)?;
    if sep != b'=' && sep != b':' {
        return None;
    }
    j += 1;
    while b.get(j) == Some(&b' ') {
        j += 1;
    }
    let quote = b.get(j).copied().filter(|c| *c == b'"' || *c == b'\'');
    if quote.is_some() {
        j += 1;
    }
    let start = j;
    if b[start..].starts_with(MARKER.as_bytes()) {
        return None; // already removed (a second pass)
    }
    let end = match quote {
        Some(q) => start + b[start..].iter().take(MAX_SCAN).take_while(|c| **c != q && **c != b'\n').count(),
        None => {
            start
                + b[start..]
                    .iter()
                    .take(MAX_SCAN)
                    .take_while(|c| {
                        !c.is_ascii_whitespace() && !matches!(c, b';' | b',' | b'&' | b'"' | b'\'' | b'}' | b')')
                    })
                    .count()
        }
    };
    let v = &b[start..end];
    // Prose guard: "password: please reset it" is not a secret. An unquoted `:` value must look like a
    // credential (a digit or a symbol in it); `=` and quoted values need only 4 characters.
    let plain_word = v.iter().all(|c| c.is_ascii_alphabetic());
    let ok = v.len() >= 4 && (sep == b'=' || quote.is_some() || !plain_word);
    ok.then_some((start, end))
}

fn find(b: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    (from..=b.len().checked_sub(pat.len())?).find(|&k| &b[k..k + pat.len()] == pat)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canary secrets (fake, generated shapes). None may survive redaction.
    pub(crate) const CANARIES: &[&str] = &[
        "sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-AbCdEf",
        "sk-proj-AbCdEfGhIjKlMnOpQrStUvWx1234",
        "ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
        "gho_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
        "github_pat_11ABCDEFG0123456789_abcdefghijklmnopqrstuvwxyz",
        "glpat-AbCdEfGhIjKlMnOpQrSt",
        "xoxb-123456789012-123456789012-AbCdEfGhIjKl",
        "AIzaSyAbCdEfGhIjKlMnOpQrStUvWxYz0123456",
        "npm_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
        "AKIAIOSFODNN7EXAMPLE",
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        "Hunter2Secret!",
        "p@ss:w0rd",
        "AbCdEfGhIjKlMnOpQrStUvWxYz0123456789==",
    ];

    fn clean(s: &str) -> String {
        redact(s).0
    }

    #[test]
    fn canaries_never_survive_in_their_usual_settings() {
        let settings = [
            format!("use this key {} please", CANARIES[0]),
            format!("export OPENAI_API_KEY={}", CANARIES[1]),
            format!("git remote set-url origin https://{}@github.com/x/y", CANARIES[2]),
            format!("token: {}", CANARIES[3]),
            format!("\"{}\"", CANARIES[4]),
            format!("{}\n", CANARIES[5]),
            format!("SLACK_TOKEN={}", CANARIES[6]),
            format!("key=({})", CANARIES[7]),
            format!("//registry.npmjs.org/:_authToken={}", CANARIES[8]),
            format!("aws_access_key_id = {}", CANARIES[9]),
            format!("Authorization: Bearer {}", CANARIES[10]),
            format!("aws_secret_access_key = {}", CANARIES[11]),
            format!("Server=db;User Id=sa;Password={};Encrypt=true", CANARIES[12]),
            format!("postgres://admin:{}@db.local:5432/app", CANARIES[13]),
            format!("DefaultEndpointsProtocol=https;AccountName=x;AccountKey={};EndpointSuffix=core", CANARIES[14]),
        ];
        for (s, canary) in settings.iter().zip(CANARIES) {
            let (out, n) = redact(s);
            assert!(!out.contains(canary), "canary survived: {out}");
            assert!(n >= 1 && out.contains(MARKER), "{out}");
        }
    }

    #[test]
    fn private_key_block_removed_whole_even_when_cut_off() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIEow\nIBAAK\n-----END RSA PRIVATE KEY-----";
        assert_eq!(clean(&format!("here: {pem} done")), format!("here: {MARKER} done"));
        assert_eq!(clean("a -----BEGIN OPENSSH PRIVATE KEY-----\nb3Blbn"), format!("a {MARKER}"));
        let cert = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----";
        assert_eq!(clean(cert), cert, "public material stays");
    }

    #[test]
    fn keeps_the_key_name_and_the_rest_of_the_prompt() {
        assert_eq!(clean("set password=Tr0ub4dor and retry"), format!("set password={MARKER} and retry"));
        assert_eq!(clean(r#"{"api_key": "abcd1234efgh"}"#), format!(r#"{{"api_key": "{MARKER}"}}"#));
        assert_eq!(clean("https://user:hunter22@example.com/x"), format!("https://user:{MARKER}@example.com/x"));
    }

    #[test]
    fn ordinary_prompts_are_untouched() {
        for s in [
            "fix the failing test in task-runner and ask the user for the password: they know it",
            "count tokens: the model has 200k tokens",
            "the secretary role has a password field",
            "rename sk-learn to scikit-learn",
            "see https://example.com:8080/path and http://localhost:3000",
            "the token=5 limit in config.toml",
            "use the eyJ prefix check",
            "Bearer tokens expire",
            "café ☕ résumé",
            "",
        ] {
            assert_eq!(redact(s), (s.to_owned(), 0), "{s}");
        }
    }

    #[test]
    fn prefix_needs_a_word_start_and_a_long_tail() {
        assert_eq!(clean("task-AbCdEfGhIjKlMnOpQrStUvWx"), "task-AbCdEfGhIjKlMnOpQrStUvWx");
        assert_eq!(clean("ghp_short"), "ghp_short");
    }

    #[test]
    fn counts_each_secret_and_is_idempotent() {
        let s = format!("{} and {}", CANARIES[0], CANARIES[2]);
        let (once, n) = redact(&s);
        assert_eq!(n, 2);
        assert_eq!(redact(&once), (once.clone(), 0), "a second pass changes nothing");
    }

    /// Hook budget (100 ms p95): hostile 256 KB inputs (the hook's stdin cap) must stay linear. Generous
    /// bound for debug builds on slow CI; a quadratic scan takes seconds here.
    #[test]
    fn hostile_input_stays_fast() {
        let n = 256 * 1024;
        for unit in ["-----BEGIN X ", "_pwd:aaaaaaaaaaaaaaaa", "://u:", "a://u:p", "eyJ", "sk-", "password=\"", "x"] {
            let s = unit.repeat(n / unit.len());
            let t = std::time::Instant::now();
            let _ = redact(&s);
            assert!(t.elapsed().as_millis() < 500, "{unit}: {:?}", t.elapsed());
        }
    }

    #[test]
    fn multibyte_neighbours_stay_valid_utf8() {
        let s = format!("clé→{}←fin", CANARIES[2]);
        assert_eq!(clean(&s), format!("clé→{MARKER}←fin"));
    }
}
