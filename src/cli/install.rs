use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub enum InstallMethod {
    Auto,
    Cargo,
    Nix,
    Binary,
}

pub fn install(method: InstallMethod, install_dir: Option<&Path>) -> Result<()> {
    match method {
        InstallMethod::Auto => {
            if try_cargo_install()? {
                return Ok(());
            }
            if try_nix_install()? {
                return Ok(());
            }
            try_binary_install(install_dir)?;
        }
        InstallMethod::Cargo => {
            if !try_cargo_install()? {
                anyhow::bail!("Cargo not found. Install Rust first: https://rustup.rs");
            }
        }
        InstallMethod::Nix => {
            if !try_nix_install()? {
                anyhow::bail!("Nix not found. Install Nix first: https://nixos.org/download");
            }
        }
        InstallMethod::Binary => {
            try_binary_install(install_dir)?;
        }
    }
    
    println!("Installation complete!");
    Ok(())
}

fn try_cargo_install() -> Result<bool> {
    if !command_exists("cargo") {
        return Ok(false);
    }
    
    println!("Installing via cargo...");
    let status = Command::new("cargo")
        .args(["install", "portail"])
        .status()?;
    
    Ok(status.success())
}

fn try_nix_install() -> Result<bool> {
    if !command_exists("nix") {
        return Ok(false);
    }
    
    println!("Installing via Nix...");
    let status = Command::new("nix")
        .args(["profile", "install", "github:peterlodri-sec/portail"])
        .status()?;
    
    Ok(status.success())
}

fn try_binary_install(install_dir: Option<&Path>) -> Result<()> {
    let platform = detect_platform()?;
    let version = get_latest_version()?;
    
    println!("Downloading portail v{} for {}...", version, platform);
    
    let url = format!(
        "https://github.com/peterlodri-sec/portail/releases/download/v{}/portail-{}",
        version, platform
    );
    
    let install_path = if let Some(dir) = install_dir {
        dir.join("portail")
    } else if let Ok(path) = which::which("portail") {
        path.parent().unwrap_or(Path::new("/usr/local/bin")).join("portail")
    } else {
        PathBuf::from("/usr/local/bin/portail")
    };
    
    // Download
    let client = reqwest::blocking::Client::new();
    let response = client.get(&url).send()?;
    
    if !response.status().is_success() {
        anyhow::bail!("Failed to download: {}", response.status());
    }
    
    let bytes = response.bytes()?;
    std::fs::write(&install_path, bytes)?;
    
    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&install_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&install_path, perms)?;
    }
    
    println!("Installed to {}", install_path.display());
    Ok(())
}

fn detect_platform() -> Result<String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    
    let os_str = match os {
        "linux" => "unknown-linux-gnu",
        "macos" => "apple-darwin",
        _ => anyhow::bail!("Unsupported OS: {}", os),
    };
    
    let arch_str = match arch {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        _ => anyhow::bail!("Unsupported architecture: {}", arch),
    };
    
    Ok(format!("{}-{}", arch_str, os_str))
}

fn get_latest_version() -> Result<String> {
    let url = "https://api.github.com/repos/peterlodri-sec/portail/releases/latest";
    let client = reqwest::blocking::Client::new();
    let response = client.get(url)
        .header("User-Agent", "portail-installer")
        .send()?;
    
    let json: serde_json::Value = response.json()?;
    let tag = json["tag_name"].as_str()
        .ok_or_else(|| anyhow::anyhow!("Failed to get version"))?;
    
    Ok(tag.trim_start_matches('v').to_string())
}

fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_platform() {
        let platform = detect_platform().unwrap();
        assert!(platform.contains("linux") || platform.contains("darwin"));
    }
    
    #[test]
    fn test_command_exists() {
        assert!(command_exists("cargo"));
        assert!(!command_exists("nonexistent_command_12345"));
    }
}
