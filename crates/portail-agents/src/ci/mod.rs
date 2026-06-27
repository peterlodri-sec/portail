//! CI loop agents — ported from portail's built-in CI agents to ADK-Rust.
//!
//! Each agent wraps one CI check as an ADK-Rust `Agent`:
//! - `DriftDetect`:   capture/replay traffic regression
//! - `SpecVerify`:    compare route spec against golden file
//! - `FuzzRoute`:     crash-test all routes
//! - `ChoreBot`:      mechanical cleanup automation

pub mod chore;
pub mod drift;
pub mod fuzz_route;
pub mod research;
pub mod spec_verify;
