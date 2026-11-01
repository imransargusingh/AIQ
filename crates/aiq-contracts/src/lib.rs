//! Wire contracts shared by the hook client, the core and the CLI.
//!
//! One crate, one binary: the hook and the core can never disagree on a frame shape.
pub mod card;
pub mod codes;
pub mod event;
pub mod frames;
pub mod pack;
