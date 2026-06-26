//! System resolver — uses OS DNS (e.g., /etc/resolv.conf).
//!
//! Wraps std::net::lookup_host. No external dependencies.
//! Falls back to DoH if system resolution fails.

use std::net::IpAddr;

use super::{DnsRecordType, DnsResolver};

pub struct SystemResolver;

#[async_trait::async_trait]
impl DnsResolver for SystemResolver {
    async fn resolve(&self, name: &str, _record_type: DnsRecordType) -> Result<Vec<IpAddr>, String> {
        tokio::net::lookup_host(name)
            .await
            .map(|addrs| addrs.map(|a| a.ip()).collect())
            .map_err(|e| format!("System DNS failed: {}", e))
    }

    fn name(&self) -> &'static str { "system" }
}
