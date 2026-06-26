//! Blocky resolver — self-hosted DoH via Tailscale/Headscale.
//!
//! Stub for v2.1+. When deployed with NixOS + Blocky, this backend
//! resolves via the local Tailscale-hosted DoH server for private
//! DNS (nas.internal, etc.) and falls back to public DoH for external.

use std::net::IpAddr;

use super::{DnsRecordType, DnsResolver};
use super::doh::DohResolver;

pub struct BlockyResolver {
    doh: DohResolver,
    local_url: String,
}

impl BlockyResolver {
    pub fn new(local_url: String) -> Self {
        Self { doh: DohResolver::default(), local_url }
    }
}

#[async_trait::async_trait]
impl DnsResolver for BlockyResolver {
    async fn resolve(&self, name: &str, record_type: DnsRecordType) -> Result<Vec<IpAddr>, String> {
        // Try local Blocky first, fall back to public DoH
        let doh = DohResolver::new(vec![self.local_url.clone()]);
        match doh.resolve(name, record_type.clone()).await {
            Ok(ips) if !ips.is_empty() => return Ok(ips),
            _ => {}
        }
        self.doh.resolve(name, record_type).await
    }

    fn name(&self) -> &'static str { "blocky" }
}
