use anyhow::Result;
use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct SetupConfig {
    pub domain: Option<String>,
    pub self_signed: bool,
    pub headscale: bool,
    pub cert_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl Default for SetupConfig {
    fn default() -> Self {
        Self {
            domain: None,
            self_signed: false,
            headscale: false,
            cert_dir: PathBuf::from("/etc/portail/certs"),
            config_dir: PathBuf::from("/etc/portail"),
        }
    }
}

pub fn run_setup(config: SetupConfig) -> Result<()> {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║              Portail Setup Wizard                         ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Step 1: Domain configuration
    let domain = if let Some(ref d) = config.domain {
        println!("✓ Using domain: {}", d);
        Some(d.clone())
    } else if config.self_signed {
        println!("✓ Using self-signed certificates (no domain required)");
        None
    } else {
        println!("Step 1: Domain Configuration");
        println!("─────────────────────────────");
        println!("Do you have a domain name? (e.g., portail.example.com)");
        println!("  [y] Yes, I have a domain");
        println!("  [n] No, use self-signed certificates");
        println!();
        print!("Choice [y/n]: ");
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();
        
        if input == "y" || input == "yes" {
            print!("Enter your domain (e.g., portail.example.com): ");
            let mut domain = String::new();
            std::io::stdin().read_line(&mut domain)?;
            let domain = domain.trim().to_string();
            if domain.is_empty() {
                println!("⚠ No domain provided, falling back to self-signed");
                None
            } else {
                println!("✓ Domain: {}", domain);
                Some(domain)
            }
        } else {
            println!("✓ Using self-signed certificates");
            None
        }
    };
    println!();

    // Step 2: Certificate setup
    println!("Step 2: TLS Certificates");
    println!("─────────────────────────");
    
    if let Some(ref domain) = domain {
        println!("Setting up Let's Encrypt for: {}", domain);
        setup_letsencrypt(domain, &config.cert_dir)?;
    } else {
        println!("Generating self-signed certificates...");
        setup_self_signed(&config.cert_dir)?;
    }
    println!();

    // Step 3: Headscale setup (optional)
    println!("Step 3: Mesh Networking (Headscale)");
    println!("───────────────────────────────────");
    
    if config.headscale {
        println!("Setting up Headscale for mesh networking...");
        setup_headscale(&config)?;
    } else {
        println!("Skipping Headscale setup (use --headscale to enable)");
    }
    println!();

    // Step 4: Generate configuration
    println!("Step 4: Configuration");
    println!("─────────────────────");
    generate_config(&config, domain.as_deref())?;
    println!();

    // Step 5: Summary
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║                    Setup Complete!                        ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
    println!("Next steps:");
    println!("  1. Start portail:  portail serve");
    println!("  2. Check health:   portail health");
    println!("  3. View config:    portail config show");
    println!();
    
    if let Some(ref domain) = domain {
        println!("Your portail is available at: https://{}", domain);
    } else {
        println!("Your portail is available at: https://localhost:8787");
    }
    
    Ok(())
}

fn setup_letsencrypt(domain: &str, cert_dir: &Path) -> Result<()> {
    fs::create_dir_all(cert_dir)?;
    
    // Check if certbot is installed
    if !command_exists("certbot") {
        println!("⚠ certbot not found. Installing...");
        install_certbot()?;
    }
    
    // Check if nginx/caddy is available for reverse proxy
    if command_exists("caddy") {
        println!("✓ Caddy detected - will auto-provision certificates");
        setup_caddy(domain, cert_dir)?;
    } else if command_exists("nginx") {
        println!("✓ Nginx detected - configuring reverse proxy...");
        setup_nginx(domain, cert_dir)?;
    } else {
        println!("⚠ No reverse proxy detected. Using standalone certbot...");
        println!("  Run: sudo certbot certonly --standalone -d {}", domain);
    }
    
    println!("✓ Certificate setup complete");
    Ok(())
}

fn setup_self_signed(cert_dir: &Path) -> Result<()> {
    fs::create_dir_all(cert_dir)?;
    
    let key_path = cert_dir.join("portail.key");
    let cert_path = cert_dir.join("portail.crt");
    
    // Generate self-signed certificate using openssl
    let output = Command::new("openssl")
        .args([
            "req", "-x509", "-newkey", "rsa:4096",
            "-keyout", key_path.to_str().unwrap(),
            "-out", cert_path.to_str().unwrap(),
            "-days", "365",
            "-nodes",
            "-subj", "/CN=localhost/O=Portail/C=US",
        ])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to generate certificate: {}", stderr);
    }
    
    // Set permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))?;
    }
    
    println!("✓ Self-signed certificate generated");
    println!("  Key:  {}", key_path.display());
    println!("  Cert: {}", cert_path.display());
    
    Ok(())
}

fn setup_headscale(_config: &SetupConfig) -> Result<()> {
    // Check if headscale is installed
    if !command_exists("headscale") {
        println!("⚠ Headscale not found.");
        println!("  Install: https://headscale.net/quick-setup/");
        return Ok(());
    }
    
    // Create headscale namespace for portail
    let output = Command::new("headscale")
        .args(["namespaces", "create", "portail"])
        .output()?;
    
    if output.status.success() {
        println!("✓ Headscale namespace 'portail' created");
    } else {
        println!("⚠ Headscale namespace may already exist");
    }
    
    // Generate pre-auth key
    let output = Command::new("headscale")
        .args(["preauthkeys", "create", "-n", "portail", "--expiration", "24h"])
        .output()?;
    
    if output.status.success() {
        let key = String::from_utf8_lossy(&output.stdout);
        println!("✓ Pre-auth key generated: {}", key.trim());
        println!("  Use this key to register nodes with: tailscale up --login-server <headscale-url> --authkey <key>");
    }
    
    Ok(())
}

fn generate_config(config: &SetupConfig, domain: Option<&str>) -> Result<()> {
    fs::create_dir_all(&config.config_dir)?;
    
    let config_path = config.config_dir.join("portail.toml");
    
    let listen_addr = if domain.is_some() {
        "0.0.0.0:443"
    } else {
        "0.0.0.0:8787"
    };
    
    let config_content = format!(r#"# Portail configuration
# Generated by: portail setup

listen = "{}"

[ai_gateway]
enabled = true
upstream = "http://127.0.0.1:4000"

[mcp]
enabled = true
socket_path = "/run/portail/mcp.sock"

[cdn]
enabled = false
origin = "http://127.0.0.1:9000"

[tls]
enabled = {}
cert_path = "{}"
key_path = "{}"

[domain]
name = "{}"
"#,
        listen_addr,
        domain.is_some() || config.self_signed,
        config.cert_dir.join("portail.crt").display(),
        config.cert_dir.join("portail.key").display(),
        domain.unwrap_or("localhost"),
    );
    
    fs::write(&config_path, config_content)?;
    println!("✓ Configuration written to: {}", config_path.display());
    
    Ok(())
}

fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

fn install_certbot() -> Result<()> {
    if cfg!(target_os = "linux") {
        // Try apt
        if command_exists("apt-get") {
            Command::new("apt-get")
                .args(["update", "-qq"])
                .status()?;
            Command::new("apt-get")
                .args(["install", "-y", "certbot"])
                .status()?;
            return Ok(());
        }
        // Try dnf
        if command_exists("dnf") {
            Command::new("dnf")
                .args(["install", "-y", "certbot"])
                .status()?;
            return Ok(());
        }
    } else if cfg!(target_os = "macos") && command_exists("brew") {
        Command::new("brew")
            .args(["install", "certbot"])
            .status()?;
        return Ok(());
    }
    
    anyhow::bail!("Could not install certbot. Please install manually.")
}

fn setup_caddy(domain: &str, _cert_dir: &Path) -> Result<()> {
    let caddyfile = format!(r#"{} {{
    reverse_proxy localhost:8787
    tls internal
}}
"#, domain);
    
    let caddy_config = Path::new("/etc/caddy/Caddyfile.portail");
    fs::write(caddy_config, caddyfile)?;
    println!("✓ Caddy config written to: {}", caddy_config.display());
    println!("  Reload Caddy: sudo systemctl reload caddy");
    
    Ok(())
}

fn setup_nginx(domain: &str, cert_dir: &Path) -> Result<()> {
    let nginx_config = format!(r#"server {{
    listen 443 ssl;
    server_name {};

    ssl_certificate {};
    ssl_certificate_key {};

    location / {{
        proxy_pass http://localhost:8787;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }}
}}
"#,
        domain,
        cert_dir.join("portail.crt").display(),
        cert_dir.join("portail.key").display(),
    );
    
    let nginx_config_path = Path::new("/etc/nginx/sites-available/portail");
    fs::write(nginx_config_path, nginx_config)?;
    
    // Enable site
    let enabled_path = Path::new("/etc/nginx/sites-enabled/portail");
    if !enabled_path.exists() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(nginx_config_path, enabled_path)?;
    }
    
    println!("✓ Nginx config written to: {}", nginx_config_path.display());
    println!("  Test: sudo nginx -t");
    println!("  Reload: sudo systemctl reload nginx");
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_setup_config_default() {
        let config = SetupConfig::default();
        assert!(config.domain.is_none());
        assert!(!config.self_signed);
        assert!(!config.headscale);
    }
}
