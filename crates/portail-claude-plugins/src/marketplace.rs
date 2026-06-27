//! Marketplace manifest parser — `.claude-plugin/marketplace.json`.
//!
//! Mirrors the Python SDK's `MarketplaceManifest`, `PluginEntry`,
//! and source types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceManifest {
    #[serde(default, alias = "$schema")]
    pub schema_url: Option<String>,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub owner: MarketplaceOwner,
    #[serde(default)]
    pub metadata: Option<MarketplaceMetadata>,
    pub plugins: Vec<MarketplaceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceOwner {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarketplaceMetadata {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default, alias = "pluginRoot")]
    pub plugin_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceEntry {
    pub name: String,
    pub source: PluginSource,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<Author>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Strict mode: if true, plugin.json is authority.
    /// If false, marketplace entry is the entire definition.
    #[serde(default = "default_true")]
    pub strict: bool,

    // Component overrides
    #[serde(default)]
    pub commands: Option<serde_json::Value>,
    #[serde(default)]
    pub agents: Option<serde_json::Value>,
    #[serde(default)]
    pub skills: Option<serde_json::Value>,
    #[serde(default)]
    pub hooks: Option<serde_json::Value>,
    #[serde(default, alias = "mcpServers")]
    pub mcp_servers: Option<serde_json::Value>,
    #[serde(default, alias = "lspServers")]
    pub lsp_servers: Option<serde_json::Value>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginSource {
    /// Relative path string.
    RelativePath(String),
    /// Typed source object.
    Typed(TypedSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source")]
pub enum TypedSource {
    #[serde(rename = "github")]
    GitHub(GitHubSource),
    #[serde(rename = "url")]
    URL(URLSource),
    #[serde(rename = "npm")]
    NPM(NPMSource),
    #[serde(rename = "pip")]
    PIP(PIPSource),
    #[serde(rename = "http")]
    HTTP(HTTPSource),
    #[serde(rename = "git-subdir")]
    GitSubdir(GitSubdirSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSource {
    pub repo: String,
    #[serde(default)]
    pub ref_: Option<String>,
    #[serde(default)]
    pub sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct URLSource {
    pub url: String,
    #[serde(default)]
    pub ref_: Option<String>,
    #[serde(default)]
    pub sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NPMSource {
    pub package: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub registry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PIPSource {
    pub package: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HTTPSource {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitSubdirSource {
    pub url: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub ref_: Option<String>,
    #[serde(default)]
    pub sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Author {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

// ─── Loading ──────────────────────────────────────────────────────

/// Load a marketplace manifest from a file path.
///
/// If `path` is a directory, looks for `.claude-plugin/marketplace.json` inside.
pub fn load_marketplace(path: &std::path::Path) -> anyhow::Result<MarketplaceManifest> {
    let file_path = if path.is_file() {
        path.to_path_buf()
    } else {
        path.join(".claude-plugin").join("marketplace.json")
    };

    let content = std::fs::read_to_string(&file_path)?;
    let manifest: MarketplaceManifest = serde_json::from_str(&content)?;
    Ok(manifest)
}

/// Validate a marketplace manifest (mirrors Python SDK validation).
pub fn validate_marketplace(manifest: &MarketplaceManifest) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    if manifest.name.is_empty() {
        issues.push(ValidationIssue {
            level: IssueLevel::Error,
            message: "marketplace name is required".into(),
        });
    }

    // Check for reserved names
    let reserved = ["official", "anthropic", "claude"];
    if reserved.contains(&manifest.name.as_str()) {
        issues.push(ValidationIssue {
            level: IssueLevel::Warning,
            message: format!("'{}' is a reserved marketplace name", manifest.name),
        });
    }

    if manifest.owner.name.is_empty() {
        issues.push(ValidationIssue {
            level: IssueLevel::Error,
            message: "owner.name is required".into(),
        });
    }

    if manifest
        .metadata
        .as_ref()
        .and_then(|m| m.description.as_deref())
        == Some("")
    {
        issues.push(ValidationIssue {
            level: IssueLevel::Warning,
            message: "metadata.description is empty".into(),
        });
    }

    // Check for duplicate plugin names
    let mut seen = std::collections::HashSet::new();
    for plugin in &manifest.plugins {
        if !seen.insert(&plugin.name) {
            issues.push(ValidationIssue {
                level: IssueLevel::Error,
                message: format!("duplicate plugin name: '{}'", plugin.name),
            });
        }
    }

    // Check for path traversal in source strings
    for plugin in &manifest.plugins {
        if let PluginSource::RelativePath(path) = &plugin.source
            && path.contains("..")
        {
            issues.push(ValidationIssue {
                level: IssueLevel::Error,
                message: format!(
                    "plugin '{}' source contains path traversal: '{}'",
                    plugin.name, path
                ),
            });
        }
    }

    issues
}

#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub level: IssueLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueLevel {
    Error,
    Warning,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_marketplace_manifest() {
        let json = r#"{
            "name": "community",
            "owner": { "name": "Community" },
            "plugins": [
                {
                    "name": "my-plugin",
                    "source": "./plugins/my-plugin",
                    "description": "A cool plugin"
                },
                {
                    "name": "remote-plugin",
                    "source": { "source": "github", "repo": "owner/repo" },
                    "description": "From GitHub"
                }
            ]
        }"#;
        let manifest: MarketplaceManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "community");
        assert_eq!(manifest.plugins.len(), 2);

        // Relative path source
        match &manifest.plugins[0].source {
            PluginSource::RelativePath(p) => assert_eq!(p, "./plugins/my-plugin"),
            _ => panic!("expected RelativePath"),
        }

        // GitHub source
        match &manifest.plugins[1].source {
            PluginSource::Typed(TypedSource::GitHub(g)) => assert_eq!(g.repo, "owner/repo"),
            _ => panic!("expected GitHub"),
        }
    }

    #[test]
    fn validate_marketplace_catches_issues() {
        let manifest = MarketplaceManifest {
            schema_url: None,
            name: "official".into(), // reserved
            version: None,
            description: None,
            owner: MarketplaceOwner {
                name: "".into(), // missing
                email: None,
            },
            metadata: None,
            plugins: vec![
                MarketplaceEntry {
                    name: "p1".into(),
                    source: PluginSource::RelativePath(".".into()),
                    description: None,
                    version: None,
                    author: None,
                    homepage: None,
                    repository: None,
                    license: None,
                    keywords: vec![],
                    category: None,
                    tags: vec![],
                    strict: true,
                    commands: None,
                    agents: None,
                    skills: None,
                    hooks: None,
                    mcp_servers: None,
                    lsp_servers: None,
                },
                MarketplaceEntry {
                    name: "p1".into(), // duplicate
                    source: PluginSource::RelativePath(".".into()),
                    description: None,
                    version: None,
                    author: None,
                    homepage: None,
                    repository: None,
                    license: None,
                    keywords: vec![],
                    category: None,
                    tags: vec![],
                    strict: true,
                    commands: None,
                    agents: None,
                    skills: None,
                    hooks: None,
                    mcp_servers: None,
                    lsp_servers: None,
                },
            ],
        };

        let issues = validate_marketplace(&manifest);
        assert!(issues.iter().any(|i| i.message.contains("reserved")));
        assert!(issues.iter().any(|i| i.message.contains("owner.name")));
        assert!(issues.iter().any(|i| i.message.contains("duplicate")));
    }
}
