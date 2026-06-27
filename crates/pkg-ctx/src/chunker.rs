use std::collections::HashSet;

const TOKEN_TARGET: usize = 800;
const TOKEN_HARD_MAX: usize = 1200;

#[derive(Debug, Clone)]
pub struct DocSection {
    pub doc_path: String,
    pub doc_title: String,
    pub section_title: String,
    pub content: String,
    pub tokens: usize,
    pub has_code: bool,
}

pub fn estimate_tokens(text: &str) -> usize {
    text.split_whitespace().count() + text.len() / 10
}

pub fn extract_title(markdown: &str) -> String {
    let mut in_frontmatter = false;
    let mut frontmatter_lines = 0;

    for line in markdown.lines().take(30) {
        if line.trim() == "---" {
            in_frontmatter = !in_frontmatter;
            frontmatter_lines += 1;
            if frontmatter_lines == 2 {
                in_frontmatter = false;
                continue;
            }
            continue;
        }
        if in_frontmatter {
            if let Some(val) = line
                .strip_prefix("title:")
                .or_else(|| line.strip_prefix("title: "))
            {
                return val.trim().trim_matches('"').to_string();
            }
            continue;
        }
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            return title.to_string();
        }
    }
    String::new()
}

pub fn chunk_markdown(doc_path: &str, markdown: &str) -> Vec<DocSection> {
    let doc_title = extract_title(markdown);
    let cleaned = strip_frontmatter(markdown);
    let tocs = detect_toc_boundaries(&cleaned);
    let lines: Vec<&str> = cleaned.lines().collect();

    let sections = split_by_heading(&lines, &tocs);
    let merged = merge_small_sections(&sections);
    let mut result = Vec::new();

    for sec in &merged {
        let content = sec.trim();
        if content.is_empty() {
            continue;
        }
        let tokens = estimate_tokens(content);
        let has_code = content.contains("```") || content.contains('`');
        let section_title = extract_section_title(content);

        result.push(DocSection {
            doc_path: doc_path.to_string(),
            doc_title: doc_title.clone(),
            section_title,
            content: content.to_string(),
            tokens,
            has_code,
        });
    }

    result
}

fn strip_frontmatter(markdown: &str) -> String {
    let lines: Vec<&str> = markdown.lines().collect();
    if lines.first().is_some_and(|l| l.trim() == "---") {
        let mut idx = 1;
        while idx < lines.len() && lines[idx].trim() != "---" {
            idx += 1;
        }
        lines
            .iter()
            .skip(idx + 1)
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        markdown.to_string()
    }
}

fn detect_toc_boundaries(markdown: &str) -> HashSet<usize> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut toc_lines = HashSet::new();
    let mut i = 0;
    while i < lines.len() {
        let link_count = lines[i..]
            .iter()
            .take(15)
            .filter(|l| {
                let t = l.trim();
                t.starts_with("- [")
                    || t.starts_with("* [")
                    || (t.starts_with('[') && t.contains("](/"))
            })
            .count();
        if link_count >= 3 {
            let window = 15.min(lines.len().saturating_sub(i));
            if link_count as f64 / window as f64 > 0.5 {
                for j in i..(i + link_count).min(lines.len()) {
                    toc_lines.insert(j);
                }
                i += link_count;
                continue;
            }
        }
        i += 1;
    }
    toc_lines
}

fn split_by_heading(lines: &[&str], toc_lines: &HashSet<usize>) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut in_code_block = false;

    for (i, line) in lines.iter().enumerate() {
        if toc_lines.contains(&i) {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            current.push(line);
            continue;
        }
        if !in_code_block && trimmed.starts_with("## ") {
            if !current.is_empty() {
                sections.push(current.join("\n"));
            }
            current.clear();
            current.push(line);
            continue;
        }
        current.push(line);
    }
    if !current.is_empty() {
        sections.push(current.join("\n"));
    }
    sections
}

fn merge_small_sections(sections: &[String]) -> Vec<String> {
    let mut merged = Vec::new();
    let mut buffer = String::new();

    for section in sections {
        let tokens = estimate_tokens(section);
        if tokens >= TOKEN_TARGET {
            if !buffer.is_empty() {
                merged.push(buffer.clone());
                buffer.clear();
            }
            merged.push(section.clone());
        } else if tokens < TOKEN_TARGET / 3 && !buffer.is_empty() {
            buffer.push_str("\n\n");
            buffer.push_str(section);
        } else {
            if !buffer.is_empty() {
                let buf_tokens = estimate_tokens(&buffer);
                if buf_tokens + tokens <= TOKEN_HARD_MAX {
                    buffer.push_str("\n\n");
                    buffer.push_str(section);
                } else {
                    merged.push(buffer.clone());
                    buffer.clear();
                    buffer.push_str(section);
                }
            } else {
                buffer.push_str(section);
            }
        }
    }
    if !buffer.is_empty() {
        merged.push(buffer);
    }
    merged
}

fn extract_section_title(content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("## ") {
            return title.to_string();
        }
        if let Some(title) = trimmed.strip_prefix("# ") {
            return title.to_string();
        }
    }
    String::new()
}

pub fn strip_mdx_tags(content: &str) -> String {
    let mut result = String::new();
    let mut in_tag = 0;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('<')
            && trimmed.ends_with('>')
            && !trimmed.starts_with("```")
            && !trimmed.starts_with("</")
            && !trimmed.contains('=')
            && !trimmed.starts_with("<!--")
        {
            in_tag += 1;
            continue;
        }
        if in_tag > 0 {
            if trimmed.starts_with("</") {
                in_tag -= 1;
                continue;
            }
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn large_section(content: &str, count: usize) -> String {
        let mut s = String::with_capacity(count * 10);
        for _ in 0..count {
            s.push_str(content);
            s.push(' ');
        }
        s
    }

    #[test]
    fn test_chunk_simple_doc() {
        let sec1 = large_section("word", 900);
        let sec2 = large_section("content", 400);
        let sec3 = large_section("more", 400);
        let md =
            format!("# Test\n\n{sec1}\n\n## Section One\n\n{sec2}\n\n## Section Two\n\n{sec3}\n");
        let chunks = chunk_markdown("test.md", &md);
        assert!(
            chunks.len() >= 2,
            "expected >=2 chunks, got {}",
            chunks.len()
        );
        assert_eq!(chunks[0].doc_title, "Test");
    }

    #[test]
    fn test_strip_frontmatter() {
        let md = "---\ntitle: My Doc\n---\n\n# Actual Title\n\nContent\n";
        let title = extract_title(md);
        assert_eq!(title, "My Doc");
    }

    #[test]
    fn test_estimate_tokens() {
        let t = estimate_tokens("hello world");
        assert!(t >= 2);
    }

    #[test]
    fn test_mdx_stripping() {
        let html = "<AppOnly>\n\nhello\n\n</AppOnly>\n\nreal content\n";
        let clean = strip_mdx_tags(html);
        assert!(!clean.contains("AppOnly"));
        assert!(clean.contains("real content"));
    }

    #[test]
    fn test_chunk_handles_code_blocks() {
        let intro = large_section("word", 900);
        let md = format!(
            "# Code\n\n{intro}\n\n## Setup\n\n```rust\nfn main() {{}}\n```\n\n## More\n\ntext\n"
        );
        let chunks = chunk_markdown("code.md", &md);
        assert!(
            chunks.len() >= 2,
            "expected >=2 chunks, got {}",
            chunks.len()
        );
        assert!(chunks.iter().any(|c| c.has_code));
    }

    #[test]
    fn test_extract_section_title() {
        let content = "## My Section\n\nsome text\n";
        assert_eq!(extract_section_title(content), "My Section");
    }

    #[test]
    fn test_merge_respects_target_size() {
        let big = "A".repeat(4000);
        let small = "B".repeat(50);
        let sections = vec![big, small];
        let merged = merge_small_sections(&sections);
        assert_eq!(merged.len(), 1, "small should merge with big");
    }

    #[test]
    fn test_split_by_heading_empty() {
        let lines: Vec<&str> = vec![];
        let tocs = HashSet::new();
        let sections = split_by_heading(&lines, &tocs);
        assert!(sections.is_empty());
    }
}
