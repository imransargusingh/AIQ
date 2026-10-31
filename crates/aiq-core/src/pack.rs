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
