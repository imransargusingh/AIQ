//! Prompt features for the re-prompt detector. Words follow the Phase 0 spike: lower case, start with a
//! letter, at least `min_word_len` long, stop words removed.
use aiq_contracts::pack::RepromptParams;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct Features {
    pub words: BTreeSet<String>,
    /// File paths, URLs and `@`-mentions named in the prompt.
    pub mentions: BTreeSet<String>,
    /// A code fence or three or more line breaks: the person pasted something.
    pub pasted: bool,
    /// Lower case, whitespace collapsed, leading punctuation removed; used for cue matching.
    pub normalized: String,
}

pub fn features(text: &str, p: &RepromptParams) -> Features {
    let lower = text.to_lowercase();
    Features {
        words: content_words(&lower, p),
        mentions: mentions(text),
        pasted: text.contains("```") || text.matches('\n').count() >= 3,
        normalized: lower
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .to_owned(),
    }
}

fn content_words(lower: &str, p: &RepromptParams) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut cur = String::new();
    let mut flush = |cur: &mut String| {
        if cur.chars().count() >= p.min_word_len && !p.stop_words.iter().any(|s| s == cur) {
            out.insert(stem(cur));
            cur.clear();
        } else {
            cur.clear();
        }
    };
    for c in lower.chars() {
        let starts = cur.is_empty();
        if (starts && c.is_alphabetic()) || (!starts && (c.is_alphanumeric() || c == '_')) {
            cur.push(c);
        } else {
            flush(&mut cur);
        }
    }
    flush(&mut cur);
    out
}

fn mentions(text: &str) -> BTreeSet<String> {
    text.split_whitespace()
        .map(|t| t.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '(' | ')' | ',' | ';' | ':' | '!' | '?')))
        .map(|t| t.trim_end_matches('.'))
        .filter(|t| is_mention(t))
        .map(str::to_owned)
        .collect()
}

fn is_mention(t: &str) -> bool {
    if t.len() > 1 && t.starts_with('@') {
        return true;
    }
    if t.len() > 1 && (t.contains('/') || t.contains('\\')) {
        return true;
    }
    // name.ext: a dot with a word before it and a short extension containing a letter after it.
    match t.rsplit_once('.') {
        Some((name, ext)) => {
            !name.is_empty()
                && name.chars().any(|c| c.is_alphanumeric())
                && (1..=5).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
                && ext.chars().any(|c| c.is_ascii_alphabetic())
                && !matches!(t.to_ascii_lowercase().as_str(), "e.g" | "i.e" | "etc")
        }
        None => false,
    }
}

/// Light suffix stripping, so inflections of one word compare equal: "errors"/"error",
/// "refactoring"/"refactor", "configured"/"configure", "settings"/"setting", "queries"/"query".
/// Not a full stemmer (no "-ation", "-er"); a word must keep a vowel before "-ing"/"-ed" comes off, so
/// "string" and "red" stay whole. Laptop finding 2026-11-01: inflected rewords missed the 0.6 bar.
pub fn stem(w: &str) -> String {
    let vowel = |s: &str| s.chars().any(|c| "aeiouy".contains(c));
    let mut s = w.to_owned();
    let n = s.chars().count();
    if n > 4 && s.ends_with("ies") {
        s.truncate(s.len() - 3);
        s.push('y');
    } else if n > 4 && ["sses", "xes", "ches", "shes"].iter().any(|e| s.ends_with(e)) {
        s.truncate(s.len() - 2);
    } else if n > 3 && s.ends_with('s') && !["ss", "us", "is"].iter().any(|e| s.ends_with(e)) {
        s.pop();
    }
    for suffix in ["ing", "ed"] {
        if let Some(base) = s.strip_suffix(suffix)
            && base.chars().count() >= 2
            && vowel(base)
        {
            s = base.to_owned();
            // "running" -> "runn" -> "run"; keep "ll", "ss", "zz" ("filling" -> "fill").
            let b = s.as_bytes();
            if b.len() >= 2 && b[b.len() - 1] == b[b.len() - 2] && !b"aeiouylsz".contains(&b[b.len() - 1]) {
                s.pop();
            }
            break;
        }
    }
    if s.chars().count() > 3 && s.ends_with('e') {
        s.pop();
    }
    s
}

/// The person's own words in a prompt. A claude.ai Projects / Remote Control turn reaches Claude Code wrapped in a
/// `<wake …>` envelope (thread ids, times, system notes) around the person's `<message from="human" …>`. Measured on
/// the laptop 2026-11-10: all 7 Claude Code first prompts since 11-08 were such envelopes of ~470 characters, and the
/// kind label read the XML and answered `other` at 0.3–0.5 on every one. This returns the triggering human message
/// (else every human message, in order), XML entities decoded; any other prompt comes back unchanged.
/// assume: the envelope keeps this shape (`<wake`, `<message … from="human" …>text</message>`). Confirm: the label
/// of a new Projects task is no longer `other`. If false: the prompt is passed on whole, as before this.
pub fn own_words(prompt: &str) -> std::borrow::Cow<'_, str> {
    let t = prompt.trim_start();
    if !t.starts_with("<wake") {
        return prompt.into();
    }
    let mut trigger = None;
    let mut all = Vec::new();
    for part in t.split("<message ").skip(1) {
        let Some((attrs, rest)) = part.split_once('>') else { continue };
        if !attrs.split_whitespace().any(|a| a == "from=\"human\"") {
            continue;
        }
        let Some((body, _)) = rest.split_once("</message>") else { continue };
        let body = unescape(body.trim());
        if attrs.split_whitespace().any(|a| a == "trigger=\"true\"") {
            trigger.get_or_insert(body.clone());
        }
        all.push(body);
    }
    match trigger {
        Some(b) => b.into(),
        None if !all.is_empty() => all.join("\n").into(),
        None => prompt.into(),
    }
}

fn unescape(s: &str) -> String {
    s.replace("&#34;", "\"")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// |a ∩ b| / |a ∪ b|; two empty sets have similarity 0 (nothing to compare).
pub fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f64 / union as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reprompt::tests::params;

    fn set(ws: &[&str]) -> BTreeSet<String> {
        ws.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn features_words_lowercase_min_len_no_stop_words() {
        let f = features("Please FIX the login Bug in it", &params());
        assert_eq!(f.words, set(&["bug", "login"]));
    }

    #[test]
    fn features_mentions_paths_files_at_and_urls() {
        let f = features("look at src/main.rs, @db and README.md or https://x.io/a (e.g. this).", &params());
        assert_eq!(f.mentions, set(&["@db", "README.md", "https://x.io/a", "src/main.rs"]));
    }

    #[test]
    fn own_words_takes_the_persons_message_out_of_a_projects_envelope() {
        // Shaped like the laptop's first prompts (2026-11-10): attributes in the measured order, a system note.
        let real = "<wake reason=\"mention\" current-time=\"2026-11-10T14:42:18Z\">\n  <project id=\"chan_x\" type=\"project\">\n    <thread ts=\"cmsg_a\">\n      <message trigger=\"true\" from=\"human\" trust=\"principal\" author=\"A\" author-id=\"user_1\" id=\"cmsg_b\" sent-at=\"2026-11-10T14:42:18Z\" mention=\"true\">do the measurement and share you findings. It&#39;s &#34;broken&#34; &amp; &lt;odd&gt;</message>\n    </thread>\n  </project>\n  <system-note>The person last wrote 0m ago.</system-note>\n</wake>";
        assert_eq!(own_words(real), "do the measurement and share you findings. It's \"broken\" & <odd>");
        let two = "<wake><message from=\"human\" id=\"cmsg_1\">first</message><message from=\"human\" trigger=\"true\" id=\"cmsg_2\">second</message></wake>";
        assert_eq!(own_words(two), "second", "the triggering message wins");
        let no_trigger = "<wake><message from=\"human\" id=\"cmsg_1\">one</message><message from=\"system\">x</message><message from=\"human\">two</message></wake>";
        assert_eq!(own_words(no_trigger), "one\ntwo", "else every human message, in order");
        let relay_only = "<wake><message from=\"system\" trust=\"principal\">thread created</message></wake>";
        assert_eq!(own_words(relay_only), relay_only, "no human message: unchanged");
        assert_eq!(
            own_words("fix the <message from=\"human\">x</message> parser"),
            "fix the <message from=\"human\">x</message> parser",
            "a typed prompt about XML is not an envelope"
        );
        assert_eq!(own_words("fix the login test"), "fix the login test");
    }

    #[test]
    fn features_bare_at_or_slash_not_a_mention() {
        assert!(features("@ / \\ and x", &params()).mentions.is_empty());
    }

    #[test]
    fn features_version_number_not_a_file() {
        assert!(features("bump to 1.2", &params()).mentions.is_empty());
    }

    #[test]
    fn features_pasted_fence_or_three_newlines() {
        assert!(features("see ```x```", &params()).pasted);
        assert!(features("a\nb\nc\nd", &params()).pasted);
        assert!(!features("a\nb\nc", &params()).pasted);
    }

    #[test]
    fn stem_merges_inflections() {
        for group in [
            &["error", "errors"][..],
            &["refactor", "refactoring", "refactored", "refactors"],
            &["configure", "configured", "configuring", "configures"],
            &["setting", "settings"],
            &["query", "queries"],
            &["handle", "handled", "handling", "handles"],
            &["run", "running", "runs"],
            &["fill", "filling", "filled"],
            &["type", "typed", "types"],
            &["process", "processes"],
        ] {
            let stems: BTreeSet<String> = group.iter().map(|w| stem(w)).collect();
            assert_eq!(stems.len(), 1, "{group:?} -> {stems:?}");
        }
    }

    #[test]
    fn stem_keeps_words_that_only_look_inflected() {
        for w in ["string", "red", "bus", "status", "class", "analysis", "thing"] {
            assert_eq!(stem(w), w.trim_end_matches('e'), "{w}");
        }
    }

    #[test]
    fn inflected_reword_now_clears_the_similarity_bar() {
        let p = params();
        let a = features("summarize the error handling in the store crate", &p).words;
        let b = features("summarise how errors are handled in the store crate", &p).words;
        assert!(jaccard(&a, &b) >= p.similarity, "{a:?} {b:?}");
    }

    #[test]
    fn jaccard_bounds() {
        assert_eq!(jaccard(&set(&[]), &set(&[])), 0.0);
        assert_eq!(jaccard(&set(&["a", "b"]), &set(&["a", "b"])), 1.0);
        assert_eq!(jaccard(&set(&["a", "b", "c"]), &set(&["a", "b", "d"])), 0.5);
    }
}
