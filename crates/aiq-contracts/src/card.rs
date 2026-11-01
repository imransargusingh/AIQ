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

/// Basic 16-colour codes, so they also work in Claude Code's "ANSI colors only" themes. Claude Code draws
/// hook messages dim; line 1 is lifted out of it (label bold yellow, tip sentence bright white) while the
/// Why / Try this / Details lines stay dim. Every code opens and closes on line 1, so nothing leaks.
/// `22` ends bold and dim, which is what puts the tip sentence at normal intensity.
const TIP_LABEL_ON: &str = "\u{1b}[1;33m";
const TIP_TEXT_ON: &str = "\u{1b}[22;97m";
const TIP_TEXT_OFF: &str = "\u{1b}[39m";

/// The only stdout a Claude Code hook may produce: `{"systemMessage": "<text>"}`.
/// Checked on the Windows laptop (2026-11-01 screenshot): Claude Code renders ANSI colour in `systemMessage`.
pub fn claude_code_system_message(card: &CardV1) -> String {
    let plain = render_claude_code(card);
    let (first, rest) = plain.split_once('\n').unwrap_or((&plain, ""));
    let tip = first.strip_prefix("AIQ tip:").unwrap_or(first);
    let text = format!("{TIP_LABEL_ON}AIQ tip:{TIP_TEXT_ON}{tip}{TIP_TEXT_OFF}\n{rest}");
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
        let v: serde_json::Value = serde_json::from_str(&claude_code_system_message(&card())).unwrap();
        let o = v.as_object().unwrap();
        assert_eq!(o.len(), 1);
        assert!(o["systemMessage"].is_string());
    }

    #[test]
    fn system_message_colours_only_the_first_line() {
        let v: serde_json::Value = serde_json::from_str(&claude_code_system_message(&card())).unwrap();
        let t = v["systemMessage"].as_str().unwrap();
        let first = t.lines().next().unwrap();
        assert_eq!(first, "\u{1b}[1;33mAIQ tip:\u{1b}[22;97m you've reworded this request 3 times without adding new details.\u{1b}[39m");
        assert_eq!(t.matches('\u{1b}').count(), 3, "no other escape codes");
        assert!(t.lines().skip(1).all(|l| !l.contains('\u{1b}')), "Why / Try this / Details stay plain");
        let stripped = t.replace(TIP_LABEL_ON, "").replace(TIP_TEXT_ON, "").replace(TIP_TEXT_OFF, "");
        assert_eq!(stripped, render_claude_code(&card()));
    }
}
