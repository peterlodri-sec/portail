//! Release audit pipeline: verify binaries, stamp artifacts,
//! generate CycloneDX SBOM, and produce a human-readable audit report.
//!
//! Run as:
//!   portail release-audit ./dist/ --version 2.5.0
//!
//! All checks are deterministic, offline-safe, and use zero LLM calls.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

// ── Public types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct BinaryAudit {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub file_type: String,
    pub arch: Option<String>,
    pub is_stripped: Option<bool>,
    pub checks: Vec<CheckResult>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stamp {
    pub algorithm: String,
    pub manifest_hash: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditReport {
    pub version: String,
    pub created_at: String,
    pub artifacts: Vec<BinaryAudit>,
    pub summary: ReportSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportSummary {
    pub total_artifacts: usize,
    pub passed: usize,
    pub failed: usize,
    pub checks_passed: usize,
    pub checks_total: usize,
    pub warnings: usize,
}

// ── Constants ──────────────────────────────────────────────────────────────

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const MACHO_32_MAGIC: [u8; 4] = [0xfe, 0xed, 0xfa, 0xce];
const MACHO_64_MAGIC: [u8; 4] = [0xfe, 0xed, 0xfa, 0xcf];
const MACHO_32_REV: [u8; 4] = [0xce, 0xfa, 0xed, 0xfe];
const MACHO_64_REV: [u8; 4] = [0xcf, 0xfa, 0xed, 0xfe];
const PE_MAGIC: [u8; 2] = *b"MZ";

// ── Core logic ─────────────────────────────────────────────────────────────

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn read_head(path: &Path, n: usize) -> Vec<u8> {
    fs::read(path)
        .ok()
        .map(|b| b.into_iter().take(n).collect())
        .unwrap_or_default()
}

fn detect_file_type(path: &Path) -> String {
    let head = read_head(path, 64);
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    if head.starts_with(&ELF_MAGIC) {
        let class = match head.get(4) {
            Some(1) => "32-bit",
            Some(2) => "64-bit",
            _ => "unknown",
        };
        let data = match head.get(5) {
            Some(1) => "LSB",
            Some(2) => "MSB",
            _ => "unknown",
        };
        let e_type = if head.len() >= 18 {
            let et = u16::from_le_bytes([head[16], head[17]]);
            match et {
                2 => "EXEC",
                3 => "DYN (shared obj)",
                _ => "unknown",
            }
        } else {
            "unknown"
        };
        return format!("ELF {class} {data} {e_type}");
    }
    if head.starts_with(&MACHO_64_MAGIC) || head.starts_with(&MACHO_64_REV) {
        return "Mach-O 64-bit".into();
    }
    if head.starts_with(&MACHO_32_MAGIC) || head.starts_with(&MACHO_32_REV) {
        return "Mach-O 32-bit".into();
    }
    if head.starts_with(&PE_MAGIC) {
        return "PE (Windows executable)".into();
    }
    match ext {
        "whl" => "Python wheel (ZIP archive)".into(),
        "gz" | "tgz" => "GZip tarball".into(),
        "so" | "dylib" => "Shared library".into(),
        "dll" => "Windows DLL".into(),
        "exe" => "Windows executable".into(),
        "sig" | "pem" => "Signature / certificate".into(),
        "sha256" | "sums" => "Checksum file".into(),
        _ => format!("Unknown (magic={:02x?})", &head[..8.min(head.len())]),
    }
}

fn detect_arch(path: &Path) -> Option<String> {
    let head = read_head(path, 20);
    if !head.starts_with(&ELF_MAGIC) {
        return None;
    }
    if head.len() < 20 {
        return None;
    }
    let machine = u16::from_le_bytes([head[18], head[19]]);
    let name = match machine {
        0x03 => "i386",
        0x08 => "MIPS",
        0x14 => "PowerPC",
        0x28 => "ARM",
        0x2A => "SuperH",
        0x32 => "IA-64",
        0x3E => "x86_64",
        0xB7 => "AArch64",
        0xF3 => "RISC-V",
        _ => return None,
    };
    Some(name.into())
}

fn check_stripped(path: &Path) -> Option<bool> {
    let head = read_head(path, 64);
    if !head.starts_with(&ELF_MAGIC) {
        return None;
    }
    // Read ELF section header string table index from e_shstrndx
    // For ELF64: e_shstrndx is at offset 62 (2 bytes)
    let is_64bit = head.get(4) == Some(&2);
    let shstrndx_offset = if is_64bit { 62 } else { 50 };
    let full = fs::read(path).ok()?;
    if full.len() <= shstrndx_offset + 2 {
        return None;
    }
    let shstrndx =
        u16::from_le_bytes([full[shstrndx_offset], full[shstrndx_offset + 1]]) as usize;

    // Parse section header table
    let shoff_offset: usize = if is_64bit { 0x28 } else { 0x20 };
    let shent_size: usize = if is_64bit { 64 } else { 40 };
    let shnum_offset: usize = if is_64bit { 60 } else { 48 };
    if full.len() <= shoff_offset + 8 {
        return None;
    }
    let shoff = u64::from_le_bytes([
        full[shoff_offset],
        full[shoff_offset + 1],
        full[shoff_offset + 2],
        full[shoff_offset + 3],
        full[shoff_offset + 4],
        full[shoff_offset + 5],
        full[shoff_offset + 6],
        full[shoff_offset + 7],
    ]) as usize;
    let shnum = u16::from_le_bytes([full[shnum_offset], full[shnum_offset + 1]]) as usize;

    if shoff == 0 || shnum == 0 {
        return None;
    }

    // Get section name string table header
    let strtab_hdr_off = shoff + shstrndx * shent_size;
    if strtab_hdr_off + shent_size > full.len() {
        return None;
    }
    // Read sh_offset (file offset of string data) from section header
    let strtab_data_off = if is_64bit {
        u64::from_le_bytes([
            full[strtab_hdr_off + 24],
            full[strtab_hdr_off + 25],
            full[strtab_hdr_off + 26],
            full[strtab_hdr_off + 27],
            full[strtab_hdr_off + 28],
            full[strtab_hdr_off + 29],
            full[strtab_hdr_off + 30],
            full[strtab_hdr_off + 31],
        ]) as usize
    } else {
        u32::from_le_bytes([
            full[strtab_hdr_off + 16],
            full[strtab_hdr_off + 17],
            full[strtab_hdr_off + 18],
            full[strtab_hdr_off + 19],
        ]) as usize
    };
    // Read sh_size (size of string data)
    let strtab_size = if is_64bit {
        u64::from_le_bytes([
            full[strtab_hdr_off + 32],
            full[strtab_hdr_off + 33],
            full[strtab_hdr_off + 34],
            full[strtab_hdr_off + 35],
            full[strtab_hdr_off + 36],
            full[strtab_hdr_off + 37],
            full[strtab_hdr_off + 38],
            full[strtab_hdr_off + 39],
        ]) as usize
    } else {
        u32::from_le_bytes([
            full[strtab_hdr_off + 20],
            full[strtab_hdr_off + 21],
            full[strtab_hdr_off + 22],
            full[strtab_hdr_off + 23],
        ]) as usize
    };

    if strtab_data_off + strtab_size > full.len() || strtab_data_off == 0 {
        return None;
    }
    let strtab = &full[strtab_data_off..strtab_data_off + strtab_size];

    // Scan section headers for .debug_ or .symtab
    for i in 0..shnum {
        let sh_off = shoff + i * shent_size;
        if sh_off + shent_size > full.len() {
            break;
        }
        let name_off = u32::from_le_bytes([full[sh_off], full[sh_off + 1], full[sh_off + 2], full[sh_off + 3]]) as usize;
        if name_off < strtab_size {
            let name_end = strtab[name_off..]
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(0);
            let name = std::str::from_utf8(&strtab[name_off..name_off + name_end]).unwrap_or("");
            if name.starts_with(".debug_") || name == ".symtab" || name == ".strtab" {
                return Some(false);
            }
        }
    }
    Some(true)
}

fn scan_suspicious_strings(path: &Path) -> Vec<String> {
    let mut warnings = Vec::new();

    // Try `strings` command
    let output = std::process::Command::new("strings")
        .arg(path)
        .output();
    let stdout = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return warnings,
    };

    // Check for absolute build paths
    let build_paths: Vec<&str> = stdout
        .lines()
        .filter(|l| l.starts_with("/build/") || l.starts_with("/Users/") || l.contains("/home/runner/"))
        .collect();
    if !build_paths.is_empty() {
        warnings.push(format!(
            "embedded {} absolute build paths (leak info)",
            build_paths.len()
        ));
    }

    // Check for debug section references
    let debug_refs: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains(".debug_") || l.contains(".symtab"))
        .collect();
    if !debug_refs.is_empty() {
        warnings.push(format!(
            "embedded {} debug section references",
            debug_refs.len()
        ));
    }

    // Check for credential-like patterns (simple heuristics)
    let cred_count = stdout
        .lines()
        .filter(|l| {
            let lower = l.to_lowercase();
            (lower.contains("api_key") || lower.contains("apikey") || lower.contains("secret")
                || lower.contains("token") || lower.contains("password"))
                && (lower.contains('=') || lower.contains(':'))
        })
        .count();
    if cred_count > 0 {
        warnings.push(format!("found {} credential-like strings", cred_count));
    }

    warnings
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Run the full release audit on a directory of artifacts.
pub fn audit_directory(dir: &Path, version: &str) -> Result<AuditReport> {
    let mut artifacts = Vec::new();

    // Phase 1: Walk directory and audit every file
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .context("cannot read artifacts directory")?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();

    for path in &entries {
        // Skip our own output files
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with("release-audit-") {
            continue;
        }

        let sha256 = sha256_file(path)?;
        let size = fs::metadata(path)?.len();
        let file_type = detect_file_type(path);
        let arch = detect_arch(path);
        let is_stripped = if file_type.starts_with("ELF") {
            check_stripped(path)
        } else {
            None
        };

        let mut checks = Vec::new();

        // Check 1: non-empty
        checks.push(CheckResult {
            name: "non-empty".into(),
            passed: size > 0,
            detail: format!("{} bytes", size),
        });

        // Check 2: sha256 computed
        checks.push(CheckResult {
            name: "sha256".into(),
            passed: true,
            detail: sha256[..16].to_string(),
        });

        // Check 3: recognized file type
        checks.push(CheckResult {
            name: "recognized-type".into(),
            passed: !file_type.starts_with("Unknown"),
            detail: file_type.clone(),
        });

        // Check 4: stripped (ELF only)
        if let Some(stripped) = is_stripped {
            checks.push(CheckResult {
                name: "stripped".into(),
                passed: stripped,
                detail: if stripped {
                    "no debug sections found".into()
                } else {
                    "debug sections present".into()
                },
            });
        }

        // Check 5: architecture detected (ELF only)
        if let Some(ref a) = arch {
            checks.push(CheckResult {
                name: "arch-detected".into(),
                passed: true,
                detail: a.clone(),
            });
        }

        let warnings = scan_suspicious_strings(path);

        artifacts.push(BinaryAudit {
            path: name.to_string(),
            size_bytes: size,
            sha256,
            file_type,
            arch,
            is_stripped,
            checks,
            warnings,
        });
    }

    // Phase 2: Compute summary
    let total = artifacts.len();
    let passed = artifacts.iter().filter(|a| a.checks.iter().all(|c| c.passed)).count();
    let failed = total - passed;
    let checks_passed: usize = artifacts.iter().map(|a| a.checks.iter().filter(|c| c.passed).count()).sum();
    let checks_total: usize = artifacts.iter().map(|a| a.checks.len()).sum();
    let warnings: usize = artifacts.iter().map(|a| a.warnings.len()).sum();

    let created_at = Utc::now().to_rfc3339();

    Ok(AuditReport {
        version: version.to_string(),
        created_at: created_at.clone(),
        artifacts,
        summary: ReportSummary {
            total_artifacts: total,
            passed,
            failed,
            checks_passed,
            checks_total,
            warnings,
        },
    })
}

/// Generate a SHA256 manifest (JSON).
pub fn generate_manifest(report: &AuditReport) -> serde_json::Value {
    let mut artifacts_map = BTreeMap::new();
    for art in &report.artifacts {
        let entry = serde_json::json!({
            "sha256": art.sha256,
            "size_bytes": art.size_bytes,
            "file_type": art.file_type,
        });
        artifacts_map.insert(art.path.clone(), entry);
    }

    serde_json::json!({
        "version": report.version,
        "created_at": report.created_at,
        "algorithm": "sha256",
        "artifacts": artifacts_map,
    })
}

/// Generate a CycloneDX 1.5 SBOM (JSON).
pub fn generate_sbom(report: &AuditReport, version: &str) -> serde_json::Value {
    let components: Vec<serde_json::Value> = report
        .artifacts
        .iter()
        .map(|art| {
            let comp_type = if art.file_type.starts_with("ELF") {
                "application"
            } else {
                "library"
            };
            serde_json::json!({
                "type": comp_type,
                "name": art.path,
                "version": version,
                "hashes": [{"alg": "SHA-256", "content": art.sha256}],
                "evidence": {
                    "fileType": art.file_type,
                },
                "properties": [
                    {"name": "portail:size_bytes", "value": art.size_bytes.to_string()},
                    {"name": "portail:arch", "value": art.arch.as_deref().unwrap_or("unknown")},
                    {"name": "portail:stripped", "value": art.is_stripped.map(|s| s.to_string()).unwrap_or_else(|| "unknown".into())},
                ],
            })
        })
        .collect();

    serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "timestamp": Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "tools": [{"vendor": "portail", "name": "release-audit", "version": version}],
            "properties": [{"name": "portail:audit-version", "value": version}],
        },
        "components": components,
    })
}

/// Generate a Markdown audit report.
pub fn generate_markdown_report(report: &AuditReport, manifest_hash: &str) -> String {
    let mut md = String::new();

    md.push_str(&format!("# Release Audit Report — v{}\n\n", report.version));
    md.push_str(&format!("**Generated:** {}\n", report.created_at));
    md.push_str(&format!("**Artifacts audited:** {}\n\n", report.summary.total_artifacts));

    // Summary table
    md.push_str("## Summary\n\n");
    md.push_str("| Metric | Value |\n");
    md.push_str("|--------|-------|\n");
    md.push_str(&format!("| Artifacts | {} |\n", report.summary.total_artifacts));
    md.push_str(&format!("| Passed | {} |\n", report.summary.passed));
    md.push_str(&format!("| Failed | {} |\n", report.summary.failed));
    md.push_str(&format!("| Checks | {}/{} passed |\n", report.summary.checks_passed, report.summary.checks_total));
    md.push_str(&format!("| Warnings | {} |\n", report.summary.warnings));
    md.push('\n');
    md.push_str(&format!("**Manifest hash:** `{}`\n\n", manifest_hash));

    // Failed artifacts
    let failed_artifacts: Vec<&BinaryAudit> = report
        .artifacts
        .iter()
        .filter(|a| !a.checks.iter().all(|c| c.passed))
        .collect();
    if !failed_artifacts.is_empty() {
        md.push_str("## Failed Artifacts\n\n");
        for art in &failed_artifacts {
            md.push_str(&format!("### {}\n\n", art.path));
            for check in &art.checks {
                if !check.passed {
                    md.push_str(&format!("- ❌ **{}**: {}\n", check.name, check.detail));
                }
            }
            md.push('\n');
        }
    }

    // Per-artifact table
    md.push_str("## Per-Artifact Details\n\n");
    md.push_str("| Path | Type | Size | SHA256 | Checks | Warnings |\n");
    md.push_str("|------|------|------|--------|--------|----------|\n");
    for art in &report.artifacts {
        let status = if art.checks.iter().all(|c| c.passed) { "✅" } else { "❌" };
        let size_str = if art.size_bytes < 1_000_000 {
            format!("{:.1} KiB", art.size_bytes as f64 / 1024.0)
        } else {
            format!("{:.1} MiB", art.size_bytes as f64 / 1_000_000.0)
        };
        let c = format!("{}/{}",
            art.checks.iter().filter(|c| c.passed).count(),
            art.checks.len()
        );
        md.push_str(&format!(
            "| {} {} | {} | {} | {} | {} | {} |\n",
            status, art.path, art.file_type, size_str, &art.sha256[..12], c, art.warnings.len()
        ));
    }
    md.push('\n');

    // Warnings detail
    for art in &report.artifacts {
        if !art.warnings.is_empty() {
            md.push_str(&format!("### {} — Warnings\n\n", art.path));
            for w in &art.warnings {
                md.push_str(&format!("- ⚠️ {}\n", w));
            }
            md.push('\n');
        }
    }

    // Verification instructions
    md.push_str("## Verification\n\n");
    md.push_str("```bash\n");
    md.push_str(&"# Verify SHA256 of any artifact\n".to_string());
    md.push_str("sha256sum <artifact>\n");
    md.push('\n');
    md.push_str("# Compare against manifest\n");
    md.push_str("cat release-audit-manifest.json | jq '.artifacts[\"<artifact>\"].sha256'\n");
    md.push_str("```\n");

    md
}

/// Run the full pipeline: audit → manifest → SBOM → report.
/// Returns the manifest hash.
pub fn run_pipeline(dir: &Path, version: &str, out_dir: &Path) -> Result<String> {
    fs::create_dir_all(out_dir)?;

    // Phase 1: Audit
    let report = audit_directory(dir, version)?;

    // Phase 2: Manifest
    let manifest = generate_manifest(&report);
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    let manifest_hash = hex::encode(Sha256::digest(manifest_json.as_bytes()));
    let manifest_path = out_dir.join("release-audit-manifest.json");
    fs::write(&manifest_path, &manifest_json)?;

    // Phase 3: SBOM
    let sbom = generate_sbom(&report, version);
    let sbom_json = serde_json::to_string_pretty(&sbom)?;
    let sbom_path = out_dir.join("release-audit-sbom.cdx.json");
    fs::write(&sbom_path, &sbom_json)?;

    // Phase 4: Markdown report
    let md = generate_markdown_report(&report, &manifest_hash);
    let report_path = out_dir.join("release-audit-report.md");
    fs::write(&report_path, &md)?;

    let passed = report.summary.passed;
    let failed = report.summary.failed;
    println!(
        "release-audit: {} artifacts ({} passed, {} failed) — {} checks, {} warnings",
        report.summary.total_artifacts,
        passed,
        failed,
        report.summary.checks_total,
        report.summary.warnings,
    );
    println!("  manifest: {}", manifest_path.display());
    println!("  sbom:     {}", sbom_path.display());
    println!("  report:   {}", report_path.display());

    Ok(manifest_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_file_type_elf() {
        // Create a minimal ELF header
        let mut elf = Vec::new();
        elf.extend_from_slice(&[0x7f, b'E', b'L', b'F']); // magic
        elf.extend_from_slice(&[2, 1, 1, 0]); // 64-bit, LSB, EI_VERSION, OS/ABI
        elf.extend_from_slice(&[0u8; 8]); // padding
        elf.extend_from_slice(&[3, 0]); // e_type = ET_DYN (shared obj)
        let dir = std::env::temp_dir();
        let path = dir.join("test_elf_audit");
        fs::write(&path, &elf).unwrap();
        assert_eq!(detect_file_type(&path), "ELF 64-bit LSB DYN (shared obj)");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_detect_file_type_pe() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_pe_audit.exe");
        fs::write(&path, &[b'M', b'Z', 0x90, 0x00]).unwrap();
        assert!(detect_file_type(&path).contains("PE"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_sha256_consistency() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_sha_audit");
        fs::write(&path, b"hello world").unwrap();
        let h1 = sha256_file(&path).unwrap();
        let h2 = sha256_file(&path).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_detect_arch_elf() {
        let mut elf = Vec::new();
        elf.extend_from_slice(&[0x7f, b'E', b'L', b'F']); // magic
        elf.extend_from_slice(&[2, 1, 1, 0]); // 64-bit, LSB
        elf.extend_from_slice(&[0u8; 8]); // padding
        elf.extend_from_slice(&[2, 0]); // e_type = ET_EXEC
        elf.extend_from_slice(&[0x3E, 0x00]); // e_machine = x86_64
        let dir = std::env::temp_dir();
        let path = dir.join("test_arch_audit");
        fs::write(&path, &elf).unwrap();
        assert_eq!(detect_arch(&path).as_deref(), Some("x86_64"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_audit_directory_empty() {
        let dir = std::env::temp_dir().join("audit_test_empty");
        let _ = fs::create_dir_all(&dir);
        let report = audit_directory(&dir, "0.0.0").unwrap();
        assert_eq!(report.summary.total_artifacts, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_audit_single_file() {
        let dir = std::env::temp_dir().join("audit_test_single");
        let _ = fs::create_dir_all(&dir);
        let file_path = dir.join("test.bin");
        fs::write(&file_path, b"test content").unwrap();

        let report = audit_directory(&dir, "1.0.0").unwrap();
        assert_eq!(report.summary.total_artifacts, 1);
        assert_eq!(report.artifacts[0].path, "test.bin");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_generate_manifest_roundtrip() {
        let report = AuditReport {
            version: "1.0.0".into(),
            created_at: "2026-06-26T12:00:00Z".into(),
            artifacts: vec![BinaryAudit {
                path: "portail-x86_64-unknown-linux-gnu".into(),
                size_bytes: 12345,
                sha256: "abcd".repeat(16),
                file_type: "ELF 64-bit LSB EXEC".into(),
                arch: Some("x86_64".into()),
                is_stripped: Some(true),
                checks: vec![],
                warnings: vec![],
            }],
            summary: ReportSummary {
                total_artifacts: 1,
                passed: 1,
                failed: 0,
                checks_passed: 0,
                checks_total: 0,
                warnings: 0,
            },
        };
        let manifest = generate_manifest(&report);
        assert_eq!(manifest["version"], "1.0.0");
        assert_eq!(manifest["algorithm"], "sha256");
        let artifacts = manifest["artifacts"].as_object().unwrap();
        assert!(artifacts.contains_key("portail-x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn test_generate_sbom() {
        let report = AuditReport {
            version: "2.5.0".into(),
            created_at: "2026-06-26T12:00:00Z".into(),
            artifacts: vec![BinaryAudit {
                path: "portail".into(),
                size_bytes: 456,
                sha256: "ff".repeat(32),
                file_type: "ELF 64-bit LSB EXEC".into(),
                arch: Some("AArch64".into()),
                is_stripped: Some(true),
                checks: vec![],
                warnings: vec![],
            }],
            summary: ReportSummary {
                total_artifacts: 1,
                passed: 1,
                failed: 0,
                checks_passed: 0,
                checks_total: 0,
                warnings: 0,
            },
        };
        let sbom = generate_sbom(&report, "2.5.0");
        assert_eq!(sbom["bomFormat"], "CycloneDX");
        assert_eq!(sbom["specVersion"], "1.5");
        assert_eq!(sbom["components"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_generate_markdown_report() {
        let report = AuditReport {
            version: "1.0.0".into(),
            created_at: "2026-06-26T12:00:00Z".into(),
            artifacts: vec![BinaryAudit {
                path: "portail".into(),
                size_bytes: 1024,
                sha256: "ab".repeat(32),
                file_type: "ELF 64-bit LSB EXEC".into(),
                arch: Some("x86_64".into()),
                is_stripped: Some(true),
                checks: vec![
                    CheckResult { name: "non-empty".into(), passed: true, detail: "1024 bytes".into() },
                    CheckResult { name: "sha256".into(), passed: true, detail: "abababababababab".into() },
                ],
                warnings: vec![],
            }],
            summary: ReportSummary {
                total_artifacts: 1,
                passed: 1,
                failed: 0,
                checks_passed: 2,
                checks_total: 2,
                warnings: 0,
            },
        };
        let md = generate_markdown_report(&report, "manifest_hash_xyz");
        assert!(md.contains("Release Audit Report"));
        assert!(md.contains("1.0.0"));
        assert!(md.contains("manifest_hash_xyz"));
        assert!(md.contains("✅"));
    }

    #[test]
    fn test_run_pipeline_creates_outputs() {
        let dir = std::env::temp_dir().join("audit_pipeline_test");
        let out = dir.join("out");
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join("artifact.bin"), b"some bytes").unwrap();

        let hash = run_pipeline(&dir, "1.0.0", &out).unwrap();
        assert!(!hash.is_empty());
        assert!(out.join("release-audit-manifest.json").exists());
        assert!(out.join("release-audit-sbom.cdx.json").exists());
        assert!(out.join("release-audit-report.md").exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
