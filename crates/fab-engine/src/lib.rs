//! fabcraft engine: the Part model, smart fabrication drawing commands, automatic dimensioning
//! and sheet layout — built on top of cadcraft-engine, never forking or copying it. See
//! `docs/superpowers/specs/2026-10-12-fabcraft-v1-design.md` in cadcraft-main for the design.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![forbid(unsafe_code)]
// Test code never ships and never runs against a real drawing — a test that panics on a bad
// `.expect()`/`.unwrap()` is just a normal, correct test failure, not the production crash this
// crate's `deny` above exists to prevent. Scoped to `cfg(test)` only: production code keeps the
// full deny.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod catalog;
pub mod commands;
pub mod dispatch;
pub mod fabdim;
pub mod jsonutil;
pub mod layout_fit;
pub mod part;

pub use part::{PartData, PartKind};
