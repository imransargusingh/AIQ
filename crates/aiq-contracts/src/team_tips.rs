//! `aiq.team-tips.v1`: a team's own "Tool to try" line per habit (design: docs/team-tips/README.md).
//!
//! The file lives outside this repo, in a folder the person's team keeps (an internal repo clone or a
//! share), and AIQ reads it in place. It never changes when or whether a tip shows: it only adds one line
//! to a tip that already shows. Its text is never stored, logged, exported or sent anywhere, and never
//! shown on Claude Code (Imran 2026-11-08: Copilot surfaces may name internal tools, Claude may not).
use serde::Deserialize;

pub const SCHEMA: &str = "aiq.team-tips.v1";
/// The file AIQ reads inside the chosen folder.
pub const FILE_NAME: &str = "team-tips.toml";
/// A team tips file is a few dozen short lines; anything bigger is not one.
pub const MAX_BYTES: u64 = 256 * 1024;
/// Short enough that the Copilot status line can still name it.
pub const MAX_TOOL_CHARS: usize = 32;
pub const MAX_HOW_CHARS: usize = 160;
pub const MAX_LINK_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamTipsV1 {
    pub schema: String,
    #[serde(default)]
    pub tools: Vec<TeamTool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct TeamTool {
    /// The habit's id in the bundled pack (`[[moves]] move_id`). Ids stay the same when a habit's words
    /// change, so an entry survives rewording.
    pub move_id: String,
    /// The tool's name, as people know it.
    pub tool: String,
    /// One sentence: how the tool helps with this habit.
    pub how: String,
    /// Optional `https://` link to the tool's guide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

impl TeamTool {
    /// The line added to a tip: "Tool to try: <tool>: <how>".
    pub fn line(&self) -> String {
        format!("Tool to try: {}: {}", self.tool, self.how.trim())
    }
}

fn one_line(field: &str, v: &str, max: usize, at: usize) -> Result<(), String> {
    if v.trim().is_empty() {
        return Err(format!("entry {at}: {field} is empty"));
    }
    if v.chars().any(char::is_control) {
        return Err(format!("entry {at}: {field} has a line break or control character"));
    }
    if v.chars().count() > max {
        return Err(format!("entry {at}: {field} is longer than {max} characters"));
    }
    Ok(())
}

impl TeamTipsV1 {
    /// Checks the shape only. Entries naming a habit the pack doesn't have are kept here and skipped at
    /// display time, so one stale entry never hides the rest (`aiq team-tips check` lists them).
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SCHEMA {
            return Err(format!("schema must be \"{SCHEMA}\""));
        }
        let mut seen = std::collections::HashSet::new();
        for (i, t) in self.tools.iter().enumerate() {
            let at = i + 1;
            one_line("move_id", &t.move_id, 64, at)?;
            one_line("tool", &t.tool, MAX_TOOL_CHARS, at)?;
            one_line("how", &t.how, MAX_HOW_CHARS, at)?;
            if let Some(l) = &t.link {
                one_line("link", l, MAX_LINK_CHARS, at)?;
                if !l.starts_with("https://") {
                    return Err(format!("entry {at}: link must start with https://"));
                }
            }
            if !seen.insert(t.move_id.as_str()) {
                return Err(format!("entry {at}: a second entry for habit {}", t.move_id));
            }
        }
        Ok(())
    }

    pub fn for_move(&self, move_id: &str) -> Option<&TeamTool> {
        self.tools.iter().find(|t| t.move_id == move_id)
    }
}
