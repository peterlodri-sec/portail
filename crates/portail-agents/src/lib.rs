//! Portail Internal Agents.
//!
//! NullClaw: network-native heartbeat agent
//! CI agents: drift-detect, spec-verify, fuzz-route, chore-bot
//! PIT: Process Interception Tracker

pub mod ci;
pub mod nullclaw;
pub mod pit;
