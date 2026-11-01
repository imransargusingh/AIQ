//! How long a Copilot CLI status-line tip stays: the person's setting "Copilot CLI tip stays"
//! (AIQ home spec, Settings > Tips; ADR-0005 rule 5). Derived from facts on every refresh; nothing
//! about the tip line is stored.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipStays {
    /// Default (Imran, 2026-11-01): the tip clears on the next prompt.
    NextPrompt,
    /// Full tip for 2 prompts or 5 minutes, then the "1 tip waiting" marker until opened.
    TwoPromptsOrFiveMin,
    /// Full tip until the person opens it.
    UntilOpened,
}

impl TipStays {
    pub const ALL: [TipStays; 3] = [TipStays::NextPrompt, TipStays::TwoPromptsOrFiveMin, TipStays::UntilOpened];

    /// The stored setting value (persisted in `setting_change`; changing one rewrites history's meaning).
    pub fn as_str(self) -> &'static str {
        match self {
            TipStays::NextPrompt => "next-prompt",
            TipStays::TwoPromptsOrFiveMin => "two-prompts-or-5-min",
            TipStays::UntilOpened => "until-opened",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipLine {
    Full,
    Waiting,
    Gone,
}

pub const FULL_FOR_PROMPTS: u32 = 2;
pub const FULL_FOR_MS: i64 = 5 * 60_000;

/// `prompts_after`: prompts in the session after delivery; `opened`: the person opened the card's details.
pub fn tip_line_state(
    stays: TipStays,
    delivered_utc_ms: i64,
    prompts_after: u32,
    opened: bool,
    now_utc_ms: i64,
) -> TipLine {
    if opened {
        return TipLine::Gone;
    }
    match stays {
        TipStays::NextPrompt if prompts_after == 0 => TipLine::Full,
        TipStays::NextPrompt => TipLine::Gone,
        TipStays::TwoPromptsOrFiveMin
            if prompts_after < FULL_FOR_PROMPTS && now_utc_ms - delivered_utc_ms < FULL_FOR_MS =>
        {
            TipLine::Full
        }
        TipStays::TwoPromptsOrFiveMin => TipLine::Waiting,
        TipStays::UntilOpened => TipLine::Full,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const T: i64 = 1_000_000;

    #[test]
    fn next_prompt_full_until_first_prompt_then_gone() {
        assert_eq!(tip_line_state(TipStays::NextPrompt, T, 0, false, T + 3_600_000), TipLine::Full);
        assert_eq!(tip_line_state(TipStays::NextPrompt, T, 1, false, T + 1), TipLine::Gone);
    }

    #[test]
    fn two_prompts_or_5_min_boundaries() {
        let s = TipStays::TwoPromptsOrFiveMin;
        assert_eq!(tip_line_state(s, T, 1, false, T + FULL_FOR_MS - 1), TipLine::Full);
        assert_eq!(tip_line_state(s, T, 1, false, T + FULL_FOR_MS), TipLine::Waiting);
        assert_eq!(tip_line_state(s, T, 2, false, T + 1), TipLine::Waiting);
        assert_eq!(tip_line_state(s, T, 9, true, T + 1), TipLine::Gone);
    }

    #[test]
    fn until_opened_stays_through_prompts_and_time() {
        assert_eq!(tip_line_state(TipStays::UntilOpened, T, 50, false, T + 86_400_000), TipLine::Full);
        assert_eq!(tip_line_state(TipStays::UntilOpened, T, 0, true, T), TipLine::Gone);
    }

    #[test]
    fn opened_clears_every_setting() {
        for s in TipStays::ALL {
            assert_eq!(tip_line_state(s, T, 0, true, T), TipLine::Gone);
        }
    }

    #[test]
    fn setting_strings_round_trip_and_are_stable() {
        let v: Vec<_> = TipStays::ALL.map(TipStays::as_str).into();
        assert_eq!(v, ["next-prompt", "two-prompts-or-5-min", "until-opened"]);
        for s in TipStays::ALL {
            assert_eq!(TipStays::parse(s.as_str()), Some(s));
        }
        assert_eq!(TipStays::parse("forever"), None);
    }
}
