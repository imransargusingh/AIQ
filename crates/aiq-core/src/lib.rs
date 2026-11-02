//! AIQ core: the long-lived, per-user process behind the hook client and the CLI.
pub mod coaching;
pub mod control;
pub mod counters;
pub mod ipc;
pub mod logging;
pub mod pack;
pub mod paths;
pub mod server;
pub mod service;
pub mod spool;
pub mod sysclock;
pub mod tools;
