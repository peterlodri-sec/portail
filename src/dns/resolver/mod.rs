//! DNS resolver trait — pluggable DNS backends.
//!
//! # v2.x — SOTA Abstraction

use std::net::IpAddr;

/// DNS record type for resolution.
#[derive(Debug, Clone, PartialEq)]
pub enum DnsRecordType {
    A,
    AAAA,
    PTR,
}

/// Core DNS resolution interface. All backends implement this.
#[async_trait::async_trait]
pub trait DnsResolver: Send + Sync + 'static {
    async fn resolve(&self, name: &str, record_type: DnsRecordType) -> Result<Vec<IpAddr>, String>;
    fn name(&self) -> &'static str;
}

pub mod blocky;
pub mod doh;
pub mod system;
