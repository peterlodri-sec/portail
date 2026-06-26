use anyhow::Result;

// ── Learn Module: Zero-to-Hero Network Security Guide ────────────

const ASCII_HEADER: &str = r#"
 _____           _    __                    
|  __ \         | |  / _|                   
| |__) |__  _ __| |_| |_ ___  _ __   ___   
|  ___/ _ \| '__| __|  _/ _ \| '_ \ / _ \  
| |  | (_) | |  | |_| || (_) | | | |  __/  
|_|   \___/|_|   \__|_| \___/|_| |_|\___|  
                                            
 Network & Security Guide
"#;

const SEPARATOR: &str = "════════════════════════════════════════════════════════════════════";

const TOPICS: &[(&str, &str)] = &[
    ("dns", "Domain Name System"),
    ("tcp", "TCP/IP Fundamentals"),
    ("tls", "TLS/SSL Certificates"),
    ("doh", "DNS over HTTPS"),
    ("vpn", "VPN & Mesh Networks"),
    ("proxy", "Proxy & Reverse Proxy"),
    ("firewall", "Firewall Basics"),
    ("dnssec", "DNS Security Extensions"),
    ("zero-trust", "Zero Trust Architecture"),
    ("headscale", "Headscale & Tailscale"),
];

pub fn run_learn(topic: Option<&str>) -> Result<()> {
    println!("{}", ASCII_HEADER);
    println!("{}", SEPARATOR);
    println!();
    
    match topic {
        Some(t) => show_topic(t),
        None => show_menu(),
    }
    
    Ok(())
}

fn show_menu() {
    println!("Available topics:");
    println!();
    
    for (i, (code, title)) in TOPICS.iter().enumerate() {
        println!("  {:2}. [{:12}] {}", i + 1, code, title);
    }
    
    println!();
    println!("Usage: portail learn <topic>");
    println!("Example: portail learn dns");
    println!();
}

fn show_topic(topic: &str) {
    match topic.to_lowercase().as_str() {
        "dns" => show_dns(),
        "tcp" => show_tcp(),
        "tls" => show_tls(),
        "doh" => show_doh(),
        "vpn" => show_vpn(),
        "proxy" => show_proxy(),
        "firewall" => show_firewall(),
        "dnssec" => show_dnssec(),
        "zero-trust" => show_zero_trust(),
        "headscale" => show_headscale(),
        _ => {
            println!("Unknown topic: {}", topic);
            println!();
            show_menu();
        }
    }
}

fn show_dns() {
    println!("DNS - Domain Name System");
    println!("{}", SEPARATOR);
    println!();
    println!("What is DNS?");
    println!("  DNS translates human-readable domain names (example.com)");
    println!("  into IP addresses (93.184.216.34) that computers use.");
    println!();
    println!("How it works:");
    println!("  1. You type 'example.com' in your browser");
    println!("  2. Your computer asks a DNS resolver");
    println!("  3. Resolver asks root servers -> TLD servers -> authoritative");
    println!("  4. Response (IP address) is cached for future use");
    println!();
    println!("Record types:");
    println!("  A     - Maps domain to IPv4 address");
    println!("  AAAA  - Maps domain to IPv6 address");
    println!("  CNAME - Alias pointing to another domain");
    println!("  MX    - Mail server for the domain");
    println!("  TXT   - Text records (SPF, DKIM, verification)");
    println!("  NS    - Nameservers for the domain");
    println!();
    println!("Portail DNS features:");
    println!("  - DNS over HTTPS (DoH) for privacy");
    println!("  - DNS hooks for filtering and routing");
    println!("  - Network isolation via DNS policies");
    println!();
    println!("Try: portail setup --domain your-domain.com");
    println!();
}

fn show_tcp() {
    println!("TCP/IP Fundamentals");
    println!("{}", SEPARATOR);
    println!();
    println!("TCP (Transmission Control Protocol):");
    println!("  - Connection-oriented protocol");
    println!("  - Guarantees delivery order");
    println!("  - Error checking and retransmission");
    println!();
    println!("Three-way handshake:");
    println!("  Client -> SYN -> Server");
    println!("  Client <- SYN-ACK <- Server");
    println!("  Client -> ACK -> Server");
    println!();
    println!("Ports:");
    println!("  80   - HTTP");
    println!("  443  - HTTPS");
    println!("  53   - DNS");
    println!("  22   - SSH");
    println!("  8787 - Portail default");
    println!();
    println!("Headers Portail adds:");
    println!("  x-request-id     - Unique request identifier");
    println!("  x-forwarded-for  - Client IP chain");
    println!("  x-portail-proxy  - Identifies proxied requests");
    println!();
}

fn show_tls() {
    println!("TLS/SSL Certificates");
    println!("{}", SEPARATOR);
    println!();
    println!("TLS (Transport Layer Security):");
    println!("  Encrypts communication between client and server");
    println!("  Replaced SSL (Secure Sockets Layer)");
    println!();
    println!("Certificate types:");
    println!("  Self-signed  - You create it yourself (dev only)");
    println!("  Let's Encrypt - Free, automated certificates");
    println!("  CA-signed    - From a Certificate Authority (paid)");
    println!();
    println!("Portail TLS setup:");
    println!("  portail setup --self-signed    # For development");
    println!("  portail setup --domain x.com   # For production");
    println!();
    println!("Files:");
    println!("  /etc/portail/certs/portail.crt - Certificate");
    println!("  /etc/portail/certs/portail.key - Private key");
    println!();
}

fn show_doh() {
    println!("DNS over HTTPS (DoH)");
    println!("{}", SEPARATOR);
    println!();
    println!("Why DoH?");
    println!("  Regular DNS queries are sent in plain text.");
    println!("  Anyone on the network can see what you look up.");
    println!("  DoH encrypts DNS queries inside HTTPS.");
    println!();
    println!("How it works:");
    println!("  1. Your app sends DNS query as HTTPS request");
    println!("  2. Query goes to a DoH server (e.g., Cloudflare)");
    println!("  3. Response comes back encrypted");
    println!();
    println!("Portail DoH:");
    println!("  Built-in DoH client for secure DNS resolution");
    println!("  Supports Cloudflare and Google DoH endpoints");
    println!();
    println!("Endpoints:");
    println!("  https://cloudflare-dns.com/dns-query");
    println!("  https://dns.google/dns-query");
    println!();
}

fn show_vpn() {
    println!("VPN & Mesh Networks");
    println!("{}", SEPARATOR);
    println!();
    println!("Traditional VPN:");
    println!("  - Client connects to VPN server");
    println!("  - All traffic routed through server");
    println!("  - Single point of failure");
    println!();
    println!("Mesh Network (WireGuard/Tailscale):");
    println!("  - Every node connects to every other node");
    println!("  - No single point of failure");
    println!("  - Automatic NAT traversal");
    println!();
    println!("Tailscale/Headscale:");
    println!("  - Built on WireGuard");
    println!("  - Zero-config mesh VPN");
    println!("  - Headscale = self-hosted control server");
    println!();
    println!("Portail + Headscale:");
    println!("  portail setup --headscale");
    println!("  - Creates isolated network for agents");
    println!("  - Automatic certificate management");
    println!("  - DNS integration for service discovery");
    println!();
}

fn show_proxy() {
    println!("Proxy & Reverse Proxy");
    println!("{}", SEPARATOR);
    println!();
    println!("Forward Proxy:");
    println!("  Client -> Proxy -> Internet");
    println!("  Hides client identity from server");
    println!();
    println!("Reverse Proxy:");
    println!("  Client -> Proxy -> Backend servers");
    println!("  Hides server topology from client");
    println!();
    println!("Portail as reverse proxy:");
    println!("  - AI Gateway: Routes to LLM providers");
    println!("  - MCP Gateway: Routes to tool servers");
    println!("  - CDN Cache: Caches responses");
    println!();
    println!("Benefits:");
    println!("  - Single entry point");
    println!("  - Load balancing");
    println!("  - SSL termination");
    println!("  - Request/response transformation");
    println!("  - Authentication");
    println!();
}

fn show_firewall() {
    println!("Firewall Basics");
    println!("{}", SEPARATOR);
    println!();
    println!("What is a firewall?");
    println!("  Controls network traffic based on rules");
    println!("  Can be hardware or software");
    println!();
    println!("Types:");
    println!("  Packet filter - Inspects headers only");
    println!("  Stateful      - Tracks connections");
    println!("  Application   - Deep packet inspection");
    println!();
    println!("Linux firewalls:");
    println!("  iptables  - Traditional");
    println!("  nftables  - Modern replacement");
    println!("  ufw       - User-friendly frontend");
    println!();
    println!("Portail network isolation:");
    println!("  - DNS-level filtering");
    println!("  - Domain allowlists/blocklists");
    println!("  - IP-based restrictions");
    println!();
}

fn show_dnssec() {
    println!("DNS Security Extensions (DNSSEC)");
    println!("{}", SEPARATOR);
    println!();
    println!("Problem:");
    println!("  DNS responses can be forged (DNS spoofing)");
    println!("  Attackers can redirect you to malicious sites");
    println!();
    println!("Solution:");
    println!("  DNSSEC signs DNS records with cryptography");
    println!("  You can verify responses are authentic");
    println!();
    println!("How it works:");
    println!("  1. Domain owner creates cryptographic keys");
    println!("  2. DNS records are signed");
    println!("  3. Resolver verifies signatures");
    println!();
    println!("Chain of trust:");
    println!("  Root -> TLD -> Domain -> Records");
    println!();
}

fn show_zero_trust() {
    println!("Zero Trust Architecture");
    println!("{}", SEPARATOR);
    println!();
    println!("Principle:");
    println!("  Never trust, always verify");
    println!("  Every request must be authenticated");
    println!();
    println!("Traditional vs Zero Trust:");
    println!("  Traditional: Trust everything inside the network");
    println!("  Zero Trust:  Verify every request, everywhere");
    println!();
    println!("Components:");
    println!("  1. Identity verification");
    println!("  2. Device verification");
    println!("  3. Least privilege access");
    println!("  4. Microsegmentation");
    println!("  5. Continuous monitoring");
    println!();
    println!("Portail implements:");
    println!("  - Request ID tracking");
    println!("  - Per-request authentication");
    println!("  - Network isolation");
    println!("  - Hook-based policy enforcement");
    println!();
}

fn show_headscale() {
    println!("Headscale & Tailscale");
    println!("{}", SEPARATOR);
    println!();
    println!("Tailscale:");
    println!("  - Mesh VPN built on WireGuard");
    println!("  - Zero configuration");
    println!("  - MagicDNS for service discovery");
    println!();
    println!("Headscale:");
    println!("  - Open-source Tailscale control server");
    println!("  - Self-hosted");
    println!("  - Compatible with Tailscale clients");
    println!();
    println!("Portail + Headscale:");
    println!("  1. Portail runs on a node");
    println!("  2. Agents connect via Headscale mesh");
    println!("  3. DNS resolves .ts.net domains");
    println!("  4. All traffic encrypted via WireGuard");
    println!();
    println!("Setup:");
    println!("  portail setup --headscale");
    println!();
    println!("Benefits:");
    println!("  - No public exposure needed");
    println!("  - Automatic NAT traversal");
    println!("  - End-to-end encryption");
    println!("  - Self-hosted, self-managed");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_topics_count() {
        assert_eq!(TOPICS.len(), 10);
    }
    
    #[test]
    fn test_show_topic_unknown() {
        // Should not panic
        show_topic("nonexistent");
    }
}
