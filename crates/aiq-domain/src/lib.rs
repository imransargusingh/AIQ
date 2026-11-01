//! Pure coaching logic for the Phase 1 walking skeleton. No I/O, no clock, no randomness:
//! ids and time come in as arguments, so every rule is testable with plain values.
pub mod card;
pub mod clock;
pub mod job;
pub mod reprompt;
pub mod text;
pub mod tipline;
pub mod trigger;

use aiq_contracts::pack::PackV1;

/// Seam: Phase 1 = the bundled pack; Phase 4 = the signed-pack loader. Both return a validated `PackV1`.
pub trait PackSource {
    fn load(&self) -> Result<PackV1, String>;
}
