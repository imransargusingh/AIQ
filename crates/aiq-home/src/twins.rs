//! One session stored twice: once live (the hooks, table `event`) and once by `aiq import` (`import_event`).
//!
//! Both copies name the session by the same id: Claude Code's hook `session_id` is its transcript's file name,
//! and Copilot CLI and VS Code agent mode both write `session-state/<session>/events.jsonl`, whose folder name
//! is the live session id. Neither copy holds every prompt (the hooks miss some, the import reads a file that
//! may hold more or fewer), so the task list keeps the live copy and adds only the imported prompts that have
//! no live twin. A twin is a prompt of the same session, at most `TWIN_MS` away, with the same text (when both
//! copies kept it). Measured on Imran's store 2026-11-03: the two copies of one prompt are 2–3 s apart.

/// Furthest apart the live and imported copies of one prompt can be.
pub const TWIN_MS: i64 = 10_000;

/// The tool that wrote a live surface's session file: VS Code agent mode writes Copilot's.
pub fn family(surface: &str) -> &str {
    match surface {
        "vscode" => "copilot-cli",
        s => s,
    }
}

/// One prompt of either copy.
pub struct Copy<'a> {
    pub tool: &'a str,
    pub session: &'a str,
    pub utc_ms: i64,
    pub text: Option<&'a str>,
}

/// What to do with one imported prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    /// The session has no live copy: an imported task as before.
    Alone,
    /// A live prompt is its twin: leave it out.
    Twin,
    /// The session has a live copy but this prompt is not in it: add it to the live stream, under the surface
    /// of this live prompt (index into `live`).
    Extra(usize),
}

fn same_text(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.split_whitespace().eq(b.split_whitespace()),
        _ => true,
    }
}

/// The fate of each imported prompt, in order. Each live prompt is the twin of at most one imported prompt,
/// the nearest in time. `live` and `imported` may come in any order.
pub fn fates(live: &[Copy<'_>], imported: &[Copy<'_>]) -> Vec<Fate> {
    use std::collections::HashMap;
    let mut by_session: HashMap<(&str, &str), Vec<usize>> = HashMap::new();
    for (i, l) in live.iter().enumerate() {
        by_session.entry((family(l.tool), l.session)).or_default().push(i);
    }
    let mut out = vec![Fate::Alone; imported.len()];
    // Candidate pairs, nearest first; each side is taken once.
    let mut pairs: Vec<(i64, usize, usize)> = Vec::new();
    for (i, p) in imported.iter().enumerate() {
        let Some(same) = by_session.get(&(p.tool, p.session)) else { continue };
        out[i] = Fate::Extra(same[0]);
        for &j in same {
            let d = (live[j].utc_ms - p.utc_ms).abs();
            if d <= TWIN_MS && same_text(live[j].text, p.text) {
                pairs.push((d, i, j));
            }
        }
    }
    pairs.sort_unstable();
    let mut used = vec![false; live.len()];
    for (_, i, j) in pairs {
        if !used[j] && out[i] != Fate::Twin {
            used[j] = true;
            out[i] = Fate::Twin;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c<'a>(tool: &'a str, session: &'a str, utc_ms: i64, text: Option<&'a str>) -> Copy<'a> {
        Copy { tool, session, utc_ms, text }
    }

    #[test]
    fn twins_need_the_same_session_time_and_text() {
        let live = [
            c("vscode", "S", 1_000, Some("fix the build")),
            c("vscode", "S", 60_000, Some("now the tests")),
            c("claude-code", "C", 5_000, None),
        ];
        let imported = [
            c("copilot-cli", "S", 3_500, Some("fix  the build")), // VS Code's file, 2.5 s later, spacing differs
            c("copilot-cli", "S", 30_000, Some("steer")),         // typed while the agent worked: hook missed it
            c("copilot-cli", "S", 61_000, Some("something else")), // near a live prompt, other text
            c("claude-code", "C", 7_000, Some("text the live copy deleted")),
            c("claude-code", "D", 7_000, Some("only imported")),
            c("copilot-cli", "S", 1_000 + TWIN_MS + 1, Some("fix the build")), // too far from its twin
        ];
        assert_eq!(
            fates(&live, &imported),
            [Fate::Twin, Fate::Extra(0), Fate::Extra(0), Fate::Twin, Fate::Alone, Fate::Extra(0)]
        );
    }

    #[test]
    fn each_live_prompt_is_one_twin_the_nearest() {
        let live = [c("claude-code", "C", 10_000, Some("go"))];
        let imported = [c("claude-code", "C", 4_000, Some("go")), c("claude-code", "C", 11_000, Some("go"))];
        assert_eq!(fates(&live, &imported), [Fate::Extra(0), Fate::Twin]);
    }
}
