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
    /// Next-rung tips: a starter prompt with `<placeholders>` that `aiq show` prints for the person to copy
    /// (never sent). Absent on other cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starter: Option<String>,
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
/// yellow at normal intensity (`22` cancels the dim). The "AIQ tip:" label is a badge: bold black on a
/// yellow background, so the tip stands out from the agent's own yellow or bold text (Imran 2026-11-03,
/// "make tips stand out"). Basic 16-colour codes, so Claude Code's "ANSI colors only" themes work too.
/// Codes are per line because the terminal UI lays each line out separately.
/// assume: Claude Code and Copilot CLI draw SGR background 43 as they draw foreground 33 (foreground was
/// seen on the laptop 2026-11-01; background is not yet seen). Confirm: the next laptop check shows a
/// yellow badge. If false: the label shows as bold text on the yellow line, as before (no text is lost).
const LINE_ON: &str = "\u{1b}[22;33m";
const LINE_OFF: &str = "\u{1b}[39m";
const BADGE_ON: &str = "\u{1b}[1;30;43m";
/// Ends the badge and goes back to the yellow line.
const BADGE_OFF: &str = "\u{1b}[22;49;33m";
const LABEL: &str = "AIQ tip:";

fn badge(plain: &str) -> String {
    plain.replacen(LABEL, &format!("{BADGE_ON}{LABEL}{BADGE_OFF}"), 1)
}

/// Copilot CLI status line (ADR-0005 rule 5): one line, the tip and where to read it in full. Never quotes
/// prompt text. With colour it looks like the Claude Code card: the whole line yellow, "AIQ tip:" a badge
/// (Imran, laptop check 2026-11-01); `NO_COLOR` gives plain text.
pub fn render_status_line(card: &CardV1, color: bool) -> String {
    let noticed = card.noticed.trim_end_matches('.');
    let plain = format!("AIQ tip: {noticed} · {}", card.evidence_ref);
    if !color {
        return plain;
    }
    format!("{LINE_ON}{}{LINE_OFF}", badge(&plain))
}

/// The shrunk status line once the full tip has had its time ("two prompts or 5 minutes" setting). Yellow
/// like the tip, so it still reads as AIQ's; no badge, because the tip itself has already been seen.
pub fn render_waiting_marker(card_id: &str, color: bool) -> String {
    let plain = format!("1 tip waiting · aiq show {card_id}");
    if color { format!("{LINE_ON}{plain}{LINE_OFF}") } else { plain }
}

/// The only stdout a Claude Code hook may produce: `{"systemMessage": "<text>"}`.
/// Confirmed on the Windows laptop 2026-11-01: Claude Code renders the codes. `color: false` (the
/// person set `NO_COLOR`, see no-color.org) sends plain text.
pub fn claude_code_system_message(card: &CardV1, color: bool) -> String {
    serde_json::json!({ "systemMessage": card_text(card, color) }).to_string()
}

/// The four-line card, every line yellow and the label a badge when `color`.
fn card_text(card: &CardV1, color: bool) -> String {
    let plain = render_claude_code(card);
    if color {
        badge(&plain).lines().map(|l| format!("{LINE_ON}{l}{LINE_OFF}")).collect::<Vec<_>>().join("\n")
    } else {
        plain
    }
}

/// Copilot CLI `agentStop` stdout: one progress line, which Copilot CLI shows in the conversation and
/// documents as display-only (hooks reference, "progress messages"), so the agent is unchanged. Not
/// `temporary`, so it stays after the agent's next reply. Same card as Claude Code's.
/// assume: Copilot CLI 1.0.106 shows the line and draws its colour codes, as Claude Code does. Confirm:
/// the next laptop upgrade shows the yellow card in the conversation. If false: the person sees the tip in
/// the status line only (no line), or with raw codes (then turn the highlight off by default here).
pub fn copilot_progress_message(card: &CardV1, color: bool) -> String {
    serde_json::json!({ "type": "progress", "message": card_text(card, color) }).to_string()
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
            starter: None,
        }
    }

    #[test]
    fn render_status_line_is_one_line_with_details_command() {
        let plain = render_status_line(&card(), false);
        assert_eq!(plain, "AIQ tip: you've reworded this request 3 times without adding new details · aiq show 01J");
        assert!(!plain.contains('\n') && plain.chars().count() <= 100);
        let colored = render_status_line(&card(), true);
        assert!(
            colored.starts_with("\u{1b}[22;33m\u{1b}[1;30;43mAIQ tip:\u{1b}[22;49;33m ") && !colored.contains('\n')
        );
        assert!(colored.ends_with(" · aiq show 01J\u{1b}[39m"), "whole line yellow, as the Claude Code card");
        assert_eq!(strip_sgr(&colored), plain, "same words as the plain line");
        assert_eq!(render_waiting_marker("01J", false), "1 tip waiting · aiq show 01J");
        assert_eq!(render_waiting_marker("01J", true), "\u{1b}[22;33m1 tip waiting · aiq show 01J\u{1b}[39m");
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
        assert!(t.starts_with("\u{1b}[22;33m\u{1b}[1;30;43mAIQ tip:\u{1b}[22;49;33m you've reworded"), "{t}");
        for l in t.lines() {
            assert!(l.starts_with(LINE_ON) && l.ends_with(LINE_OFF), "{l:?}");
        }
        assert_eq!(strip_sgr(t), render_claude_code(&card()), "same words as the plain card");
    }

    #[test]
    fn copilot_progress_message_is_one_display_only_line_with_the_card() {
        let line = copilot_progress_message(&card(), true);
        assert!(!line.contains('\n'), "one JSON line");
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v.as_object().unwrap().len(), 2);
        assert_eq!(v["type"], "progress");
        assert_eq!(strip_sgr(v["message"].as_str().unwrap()), render_claude_code(&card()));
        let plain: serde_json::Value = serde_json::from_str(&copilot_progress_message(&card(), false)).unwrap();
        assert_eq!(plain["message"].as_str().unwrap(), render_claude_code(&card()));
    }

    #[test]
    fn system_message_without_colour_is_plain_text() {
        let v: serde_json::Value = serde_json::from_str(&claude_code_system_message(&card(), false)).unwrap();
        assert_eq!(v["systemMessage"].as_str().unwrap(), render_claude_code(&card()));
    }
}
