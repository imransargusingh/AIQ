//! `BundledPack`: the Phase 1 `PackSource`, a TOML file compiled into the binary.
use aiq_contracts::pack::PackV1;
use aiq_domain::PackSource;

pub const BUNDLED_TOML: &str = include_str!("../packs/bundled.toml");

#[derive(Debug, Default)]
pub struct BundledPack;

impl PackSource for BundledPack {
    fn load(&self) -> Result<PackV1, String> {
        parse(BUNDLED_TOML)
    }
}

/// Parse and validate a pack; the Phase 4 signed loader must call the same `validate`.
pub fn parse(toml_text: &str) -> Result<PackV1, String> {
    let p: PackV1 = toml::from_str(toml_text).map_err(|e| e.to_string())?;
    p.validate()?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_pack_validates_against_schema() {
        let p = BundledPack.load().unwrap();
        assert_eq!(p.reprompt.threshold, 3);
        assert_eq!(p.trigger.default_daily_cap, 5);
        assert!(
            p.tips.iter().all(|t| !t.noticed.contains('\n') && !t.why.contains('\n') && !t.try_this.contains('\n'))
        );
    }

    #[test]
    fn bundled_moves_carry_no_rung_name_number_or_model_text_and_fit_the_status_line() {
        let p = BundledPack.load().unwrap();
        assert_eq!(p.moves.len(), 28);
        for m in &p.moves {
            for t in [&m.noticed, &m.why, &m.try_this, &m.habit, &m.counts] {
                assert!(!aiq_domain::rung::RUNG_NAMES.iter().any(|r| t.contains(r)), "{t}");
                assert!(!t.chars().any(|c| ('1'..='5').contains(&c)), "{t}");
            }
            let longest = aiq_contracts::judge::TASK_TYPES
                .iter()
                .map(|(j, t, _)| aiq_domain::coach::card_label(j, t))
                .max_by_key(|l| l.chars().count())
                .unwrap_or_default();
            let line = format!("AIQ tip: {} · aiq show", m.noticed.trim_end_matches('.').replace("{type}", &longest));
            // Measured 122 at most with the longest label (2026-11-08); the id-free line keeps it near a terminal's width.
            assert!(line.chars().count() <= 125, "{} chars: {line}", line.chars().count());
        }
        // Every step the rung guardrails need for each job has a move.
        for job in aiq_domain::rung::PACK_JOBS {
            for step in 1..=3u8 {
                assert!(p.moves.iter().any(|m| m.step == step && (m.job == job || m.job == "any")), "{job} {step}");
            }
        }
        // Goal, context and limits (F4) is coached in each job's own words, and only when the analysis is sure.
        for job in aiq_domain::rung::PACK_JOBS {
            let f4: Vec<_> = p.moves.iter().filter(|m| m.facts == ["F4"] && m.job == job).collect();
            assert_eq!(f4.len(), 1, "{job}");
            assert_eq!(f4[0].min_confidence, Some(0.8), "{job}");
        }
    }

    /// Every fact a job's next rung can confidently fail, at a rung the task can really sit at, has a move of
    /// that step and job (Imran 2026-11-05: tips apply to every kind of work). Before this, a doc or a run
    /// driven one step at a time (F12) had no next step, so its kind read "AIQ isn't sure yet".
    #[test]
    fn every_guardrail_fact_a_job_can_fail_has_a_move() {
        use aiq_domain::rung::{self, Gate};
        let p = BundledPack.load().unwrap();
        let ids = ["F1", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F12", "F13", "F14", "F15"];
        let mut gaps = Vec::new();
        for job in rung::PACK_JOBS {
            for step in 1..=4u8 {
                for bad in ids {
                    let fact = |id: &str| {
                        let good = id != "F12";
                        let v = if id == bad { !good } else { good };
                        if id == "F3" {
                            let who = if id == bad { "person" } else { "agent" };
                            format!(r#""F3":{{"value":"{who}","confidence":1.0,"source":"log"}}"#)
                        } else {
                            format!(r#""{id}":{{"value":{v},"confidence":0.9}}"#)
                        }
                    };
                    let json = format!("{{{}}}", ids.iter().map(|i| fact(i)).collect::<Vec<_>>().join(","));
                    let f = rung::facts_from_json(&json, 2, job);
                    // The task must be able to sit at `step` with this fact failing.
                    if !matches!(rung::gate(&f, step), Gate::Holds) {
                        continue;
                    }
                    let fails = rung::missing(&f, step + 1).contains(&Gate::Fails(bad));
                    let covered = p.moves.iter().any(|m| {
                        m.step == step && (m.job == job || m.job == "any") && m.facts.iter().any(|x| x == bad)
                    });
                    if fails && !covered {
                        gaps.push(format!("{job} step {step} {bad}"));
                    }
                }
            }
        }
        assert!(gaps.is_empty(), "no move for: {gaps:?}");
    }

    #[test]
    fn parse_unknown_field_rejected() {
        let bad = BUNDLED_TOML.replace("idle_gap_min = 30", "idle_gap_min = 30\nsurprise = 1");
        assert!(parse(&bad).is_err());
    }

    #[test]
    fn parse_out_of_range_threshold_rejected() {
        let bad = BUNDLED_TOML.replace("in_moment_threshold = 0.75", "in_moment_threshold = 1.5");
        assert!(parse(&bad).is_err());
    }

    #[test]
    fn bundled_card_first_line_fits_80_columns_after_prefix() {
        let p = BundledPack.load().unwrap();
        let line = format!("  ⎿  Stop says: AIQ tip: {}", p.tips[0].noticed.replace("{count}", "3"));
        assert!(line.chars().count() <= 100, "{}", line.chars().count());
    }
}
