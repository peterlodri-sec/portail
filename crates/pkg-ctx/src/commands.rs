use crate::chunker::{self, DocSection};
use crate::storage::{DocChunk, PackageDb};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use walkdir::WalkDir;

pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub db_path: PathBuf,
    pub chunk_count: usize,
    pub repo_url: String,
}

pub async fn add_package(
    repo_url: &str,
    name: Option<&str>,
    version: Option<&str>,
    pkg_dir: &Path,
) -> Result<PackageInfo> {
    let pkg_name = name
        .map(|n| n.to_string())
        .unwrap_or_else(|| infer_name(repo_url));
    let pkg_version = version.unwrap_or("latest").to_string();
    let db_filename = format!("{}@{}.db", pkg_name, pkg_version);
    let db_path = pkg_dir.join(&db_filename);

    if db_path.exists() {
        anyhow::bail!("Package already exists at {}", db_path.display());
    }

    let tmp_dir = pkg_dir.join(".tmp").join(&pkg_name);
    if tmp_dir.exists() {
        tokio::fs::remove_dir_all(&tmp_dir).await?;
    }
    tokio::fs::create_dir_all(&tmp_dir).await?;

    clone_repo(repo_url, &tmp_dir).await?;

    let docs_dir = find_docs_dir(&tmp_dir);
    let chunks = index_docs(&docs_dir)?;
    let db = PackageDb::open(
        db_path.to_str().context("invalid db path")?,
    )?;

    let mut chunk_count = 0;
    for section in &chunks {
        db.insert_chunk(&DocChunk {
            id: 0,
            doc_path: section.doc_path.clone(),
            doc_title: section.doc_title.clone(),
            section_title: section.section_title.clone(),
            content: section.content.clone(),
            tokens: section.tokens as i64,
            has_code: section.has_code,
        })?;
        chunk_count += 1;
    }

    db.set_meta("name", &pkg_name)?;
    db.set_meta("version", &pkg_version)?;
    db.set_meta("repo", repo_url)?;
    db.set_meta("built_at", &chrono::Utc::now().to_rfc3339())?;
    db.set_meta("chunk_count", &chunk_count.to_string())?;

    tokio::fs::remove_dir_all(&tmp_dir).await?;

    Ok(PackageInfo {
        name: pkg_name,
        version: pkg_version,
        db_path,
        chunk_count,
        repo_url: repo_url.to_string(),
    })
}

async fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(dest)
        .output()
        .await
        .context("failed to run git clone")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git clone failed: {stderr}");
    }
    Ok(())
}

fn find_docs_dir(repo_dir: &Path) -> PathBuf {
    for name in &["docs", "documentation", "doc"] {
        let candidate = repo_dir.join(name);
        if candidate.is_dir() {
            return candidate;
        }
    }
    repo_dir.to_path_buf()
}

fn index_docs(docs_dir: &Path) -> Result<Vec<DocSection>> {
    let mut all_chunks = Vec::new();

    for entry in WalkDir::new(docs_dir)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            !name.starts_with('.') && name != "node_modules" && name != "target"
        })
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "md" | "mdx" | "markdown") {
            continue;
        }

        let rel_path = path
            .strip_prefix(docs_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {path:?}"))?;

        let cleaned = chunker::strip_mdx_tags(&content);
        let chunks = chunker::chunk_markdown(&rel_path, &cleaned);
        all_chunks.extend(chunks);
    }

    Ok(all_chunks)
}

fn infer_name(repo_url: &str) -> String {
    let name = repo_url
        .trim_end_matches(".git")
        .split('/')
        .next_back()
        .unwrap_or("unknown")
        .to_string();
    name.strip_suffix("-docs")
        .or_else(|| name.strip_suffix("-doc"))
        .unwrap_or(&name)
        .to_string()
}

pub fn list_packages(pkg_dir: &Path) -> Result<Vec<PackageInfo>> {
    let mut packages = Vec::new();
    if !pkg_dir.exists() {
        return Ok(packages);
    }
    for entry in std::fs::read_dir(pkg_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("db") {
            continue;
        }
        let filename = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if let Some((name, version)) = filename.split_once('@') {
            let db = PackageDb::open(path.to_str().unwrap_or(""))?;
            let chunk_count: usize = db
                .get_meta("chunk_count")?
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            let repo_url = db.get_meta("repo")?.unwrap_or_default();
            packages.push(PackageInfo {
                name: name.to_string(),
                version: version.to_string(),
                db_path: path,
                chunk_count,
                repo_url,
            });
        }
    }
    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_name_from_url() {
        assert_eq!(infer_name("https://github.com/vercel/next.js"), "next.js");
        assert_eq!(infer_name("https://github.com/vercel/next.js.git"), "next.js");
        assert_eq!(infer_name("git@github.com:user/repo-docs.git"), "repo");
    }
}
