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

/// Bold yellow (basic 16-colour, so it also works in Claude Code's "ANSI colors only" themes), then
/// bold off and default colour. Claude Code draws hook messages dim; this makes the card's label stand out.
const TIP_LABEL_ON: &str = "\u{1b}[1;33m";
const TIP_LABEL_OFF: &str = "\u{1b}[22;39m";

/// The only stdout a Claude Code hook may produce: `{"systemMessage": "<text>"}`.
/// assume: Claude Code passes ANSI colour codes in `systemMessage` through to the terminal. Confirm on
/// the laptop screenshot; if it shows raw "[1;33m", drop the codes (the plain text is unchanged).
pub fn claude_code_system_message(card: &CardV1) -> String {
    let text = render_claude_code(card).replacen("AIQ tip:", &format!("{TIP_LABEL_ON}AIQ tip:{TIP_LABEL_OFF}"), 1);
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
    fn system_message_colours_only_the_label() {
        let v: serde_json::Value = serde_json::from_str(&claude_code_system_message(&card())).unwrap();
        let t = v["systemMessage"].as_str().unwrap();
        assert!(t.starts_with("\u{1b}[1;33mAIQ tip:\u{1b}[22;39m you've reworded"), "{t}");
        assert_eq!(t.matches('\u{1b}').count(), 2, "no other escape codes");
        assert_eq!(t.replace(TIP_LABEL_ON, "").replace(TIP_LABEL_OFF, ""), render_claude_code(&card()));
    }
}
