//! `aiq.card.v1`: one coaching card. JSON Schema: `contracts/aiq.card.v1.json`.
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "aiq.card.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardV1 {
    pub schema: String,
    pub card_id: String,
    pub opportunity_id: String,
    pub tip_id: String,
    /// One "What we noticed" sentence citing the person's own activity (US-3). Never quotes prompt text.
    pub noticed: String,
    pub why: String,
    pub try_this: String,
    /// The evidence link: `aiq show <card_id>`.
    pub evidence_ref: String,
    pub pack_version: String,
}

/// Claude Code renderer: the text that follows Claude Code's own "Stop says: " prefix.
/// Plain text, no colour, short lines (design: UI and interaction design).
pub fn render_claude_code(card: &CardV1) -> String {
    format!(
        "AIQ tip: {}\nWhy: {}\nTry this: {}\nDetails: {}  ·  Turn off tips for now: aiq pause",
        card.noticed, card.why, card.try_this, card.evidence_ref
    )
}

/// Claude Code draws hook messages dim, which made the card easy to miss (Imran, laptop check). Each line is
/// yellow at normal intensity (`22` cancels the dim), and the "AIQ tip:" label is bold. Basic 16-colour
/// codes, so Claude Code's "ANSI colors only" themes work too. Codes are per line because the terminal UI
/// lays each line out separately.
const LINE_ON: &str = "\u{1b}[22;33m";
const LINE_OFF: &str = "\u{1b}[39m";
const BOLD_ON: &str = "\u{1b}[1m";
const BOLD_OFF: &str = "\u{1b}[22m";

/// The only stdout a Claude Code hook may produce: `{"systemMessage": "<text>"}`.
/// Confirmed on the Windows laptop 2026-11-01: Claude Code renders the codes. `color: false` (the
/// person set `NO_COLOR`, see no-color.org) sends plain text.
pub fn claude_code_system_message(card: &CardV1, color: bool) -> String {
    let plain = render_claude_code(card);
    let text = if color {
        plain
            .replacen("AIQ tip:", &format!("{BOLD_ON}AIQ tip:{BOLD_OFF}"), 1)
            .lines()
            .map(|l| format!("{LINE_ON}{l}{LINE_OFF}"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        plain
    };
    serde_json::json!({ "systemMessage": text }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> CardV1 {
        CardV1 {
            schema: SCHEMA.into(),
            card_id: "01J".into(),
            opportunity_id: "reprompt".into(),
            tip_id: "reprompt.add-what-changed".into(),
            noticed: "you've reworded this request 3 times without adding new details.".into(),
            why: "w".into(),
            try_this: "t".into(),
            evidence_ref: "aiq show 01J".into(),
            pack_version: "bundled-1".into(),
        }
    }

    #[test]
    fn render_card_has_one_line_per_part_in_order() {
        let r = render_claude_code(&card());
        let lines: Vec<_> = r.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("AIQ tip: you've reworded"));
        assert!(lines[1].starts_with("Why: "));
        assert!(lines[2].starts_with("Try this: "));
        assert!(lines[3].contains("aiq show 01J") && lines[3].contains("aiq pause"));
    }

    #[test]
    fn system_message_is_exactly_one_key() {
        let v: serde_json::Value = serde_json::from_str(&claude_code_system_message(&card(), true)).unwrap();
        let o = v.as_object().unwrap();
        assert_eq!(o.len(), 1);
        assert!(o["systemMessage"].is_string());
    }

    fn strip_sgr(t: &str) -> String {
        let mut out = String::new();
        let mut it = t.chars().peekable();
        while let Some(c) = it.next() {
            if c == '\u{1b}' {
                for d in it.by_ref() {
                    if d == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn system_message_colours_every_line_and_bolds_the_label() {
        let v: serde_json::Value = serde_json::from_str(&claude_code_system_message(&card(), true)).unwrap();
        let t = v["systemMessage"].as_str().unwrap();
        assert!(t.starts_with("\u{1b}[22;33m\u{1b}[1mAIQ tip:\u{1b}[22m you've reworded"), "{t}");
        for l in t.lines() {
            assert!(l.starts_with(LINE_ON) && l.ends_with(LINE_OFF), "{l:?}");
        }
        assert_eq!(strip_sgr(t), render_claude_code(&card()), "same words as the plain card");
    }

    #[test]
    fn system_message_without_colour_is_plain_text() {
        let v: serde_json::Value = serde_json::from_str(&claude_code_system_message(&card(), false)).unwrap();
        assert_eq!(v["systemMessage"].as_str().unwrap(), render_claude_code(&card()));
    }
}
