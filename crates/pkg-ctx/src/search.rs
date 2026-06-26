use crate::storage::{PackageDb, SearchResult};
use anyhow::Result;
use std::path::Path;

const TOKEN_BUDGET: usize = 2000;

pub struct DocSearch {
    pkg_dir: Box<Path>,
}

impl DocSearch {
    pub fn new(pkg_dir: &Path) -> Self {
        Self {
            pkg_dir: pkg_dir.to_path_buf().into_boxed_path(),
        }
    }

    pub fn search_package(
        &self,
        package: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let db_path = self.find_package_db(package)?;
        let db = PackageDb::open(db_path.to_str().unwrap_or(""))?;
        let results = db.search(query, limit)?;
        Ok(apply_token_budget(results))
    }

    pub fn search_all(&self, query: &str, limit: usize) -> Result<Vec<(String, Vec<SearchResult>)>> {
        let mut all = Vec::new();
        for entry in std::fs::read_dir(&*self.pkg_dir)? {
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
            let db = PackageDb::open(path.to_str().unwrap_or(""))?;
            let results = db.search(query, limit)?;
            if !results.is_empty() {
                all.push((filename, apply_token_budget(results)));
            }
        }
        Ok(all)
    }

    pub fn list_installed(&self) -> Result<Vec<String>> {
        let mut packages = Vec::new();
        if !self.pkg_dir.exists() {
            return Ok(packages);
        }
        for entry in std::fs::read_dir(&*self.pkg_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("db") {
                continue;
            }
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                packages.push(name.to_string());
            }
        }
        packages.sort();
        Ok(packages)
    }

    fn find_package_db(&self, package: &str) -> Result<std::path::PathBuf> {
        for entry in std::fs::read_dir(&*self.pkg_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("db") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem == package || stem.starts_with(&format!("{package}@")) {
                    return Ok(path);
                }
            }
        }
        anyhow::bail!("Package not found: {package}")
    }
}

fn apply_token_budget(results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut total = 0;
    let mut filtered = Vec::new();
    for r in results {
        total += r.tokens;
        if total > TOKEN_BUDGET as i64 {
            break;
        }
        filtered.push(r);
    }
    filtered
}

pub fn format_search_results(
    results: &[SearchResult],
    package: &str,
    query: &str,
) -> String {
    if results.is_empty() {
        return format!("No results found for '{query}' in {package}.");
    }
    let mut out = format!("# Docs: {package} — \"{query}\"\n\n");
    for r in results {
        let section = if r.section_title.is_empty() {
            String::new()
        } else {
            format!(" §{}", r.section_title)
        };
        out.push_str(&format!("## {}{}\n", r.doc_title, section));
        out.push_str(&r.content);
        out.push('\n');
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_budget_empty() {
        assert!(apply_token_budget(vec![]).is_empty());
    }
}
