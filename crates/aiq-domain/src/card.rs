//! Card composer: one Tip + one Signal → one `aiq.card.v1`. Never quotes prompt text.
use aiq_contracts::card::{CardV1, SCHEMA};
use aiq_contracts::pack::Tip;

pub fn compose(tip: &Tip, reprompt_count: u32, card_id: &str, pack_version: &str) -> CardV1 {
    CardV1 {
        schema: SCHEMA.into(),
        card_id: card_id.to_owned(),
        opportunity_id: tip.opportunity_id.clone(),
        tip_id: tip.tip_id.clone(),
        noticed: tip.noticed.replace("{count}", &reprompt_count.to_string()),
        why: tip.why.clone(),
        try_this: tip.try_this.clone(),
        evidence_ref: format!("aiq show {card_id}"),
        pack_version: pack_version.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compose_fills_count_and_evidence_ref() {
        let tip = Tip {
            tip_id: "t".into(),
            opportunity_id: "o".into(),
            noticed: "reworded {count} times".into(),
            why: "w".into(),
            try_this: "x".into(),
        };
        let c = compose(&tip, 3, "c1", "p1");
        assert_eq!(c.noticed, "reworded 3 times");
        assert_eq!(c.evidence_ref, "aiq show c1");
        assert_eq!(c.pack_version, "p1");
    }
}
