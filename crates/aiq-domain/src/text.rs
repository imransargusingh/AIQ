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
            out.insert(std::mem::take(cur));
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
    fn jaccard_bounds() {
        assert_eq!(jaccard(&set(&[]), &set(&[])), 0.0);
        assert_eq!(jaccard(&set(&["a", "b"]), &set(&["a", "b"])), 1.0);
        assert_eq!(jaccard(&set(&["a", "b", "c"]), &set(&["a", "b", "d"])), 0.5);
    }
}
