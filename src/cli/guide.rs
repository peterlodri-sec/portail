use std::io::{self, Write};

const BLOCKS: &[(&str, &str)] = &[
    (
        "Overview",
        r#"
This guide sets up branch protection for `peterlodri-sec/portail` to ensure:
- Only maintainers and agents can merge to `main`
- Self-hosted runners are protected from fork PRs
- All changes require review and passing CI
"#,
    ),
    (
        "Step 1: Enable Branch Protection Rules",
        r#"
Go to: Settings → Branches → Add branch protection rule

Branch name pattern: main

✅ Require a pull request before merging
  ✅ Require approvals (1)
  ✅ Dismiss stale pull request approvals when new commits are pushed
  ✅ Require review from Code Owners

✅ Require status checks to pass before merging
  ✅ Require branches to be up to date before merging
  Status checks:
    - check-linux
    - check-macos
    - nix
    - python

✅ Require conversation resolution before merging
✅ Require signed commits
✅ Require linear history
✅ Do not allow bypassing the above settings

✅ Restrict who can push to matching branches
  ✅ Specify actors: peterlodri-sec

❌ Allow force pushes (disabled)
❌ Allow deletions (disabled)
"#,
    ),
    (
        "Step 2: Enable Tag Protection",
        r#"
Go to: Settings → Tags → Add tag protection rule

Tag name pattern: v*

This prevents unauthorized users from pushing version tags.
Only maintainers can create releases.
"#,
    ),
    (
        "Step 3: Configure CODEOWNERS",
        r#"
Create file: .github/CODEOWNERS

Contents:
```
# Default owners for everything
* @peterlodri-sec

# CI/CD workflows
.github/ @peterlodri-sec

# Nix configuration
nix/ @peterlodri-sec

# Security-sensitive files
SECURITY.md @peterlodri-sec
```
"#,
    ),
    (
        "Step 4: Configure Merge Options",
        r#"
Go to: Settings → General → Pull Requests

✅ Allow merge commits (default to merge commit)
✅ Allow squash merging (default to squash merge)
✅ Allow rebase merging (default to rebase merge)
✅ Always suggest updating pull request branches
✅ Allow auto-merge
✅ Automatically delete head branches
"#,
    ),
    (
        "Step 5: Enable Security Features",
        r#"
Go to: Settings → Security

Code security and analysis:
  ✅ Dependabot alerts
  ✅ Dependabot security updates
  ✅ Dependabot version updates
  ✅ Code scanning (CodeQL)
  ✅ Secret scanning
  ✅ Secret scanning push protection
"#,
    ),
    (
        "Step 6: Configure Runner Permissions",
        r#"
Go to: Settings → Actions → General

Actions permissions:
  ● Allow select actions and reusable workflows
    ✅ Allow actions created by GitHub
    ✅ Allow actions by Marketplace verified creators

Workflow permissions:
  ● Read repository contents and packages permissions

Fork pull request workflows:
  ✅ Require approval for first-time contributors
  ✅ Require approval for all outside collaborators

Run workflows from fork pull requests:
  ✅ (enabled - protected by workflow logic)
"#,
    ),
    (
        "Step 7: Security Model",
        r#"
┌─────────────────────────────────────────────────────────────┐
│                    CI Access Control                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   Push to main ──────────▶ Self-hosted runner              │
│   (maintainers/agents)      (trusted)                       │
│                                                             │
│   PR from same repo ─────▶ Self-hosted runner              │
│   (collaborators)           (trusted)                       │
│                                                             │
│   PR from fork ──────────▶ GitHub-hosted runner            │
│   (anyone)                  (safe, isolated)                │
│                                                             │
│   Tag push (v*) ─────────▶ Self-hosted runner              │
│   (maintainers only)        (release builds)                │
│                                                             │
└─────────────────────────────────────────────────────────────┘
"#,
    ),
    (
        "Step 8: Verify Protection",
        r#"
Test with a fork PR:
1. Fork the repo to another account
2. Create a PR from the fork
3. Verify: PR uses GitHub-hosted runners (not self-hosted)

Test with a same-repo PR:
1. Create a branch in the main repo
2. Create a PR
3. Verify: PR uses self-hosted runners

Commands to verify:
```bash
gh run list --workflow ci.yml --limit 10
gh run view <run-id> --log | grep "runs-on"
```
"#,
    ),
    (
        "Quick Reference",
        r#"
┌─────────────────────────────────────────────────────────────┐
│                    Settings Summary                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   Branch pattern:     main                                  │
│   Required approvals: 1                                     │
│   Status checks:      check-linux, check-macos, nix, python │
│   Tag pattern:        v*                                    │
│   Force push:         Disabled                              │
│   Delete branch:      Disabled                              │
│   Signed commits:     Required                              │
│   Linear history:     Required                              │
│                                                             │
└─────────────────────────────────────────────────────────────┘

Commands:
```bash
gh api repos/peterlodri-sec/portail/branches/main/protection
gh api repos/peterlodri-sec/portail/branches --jq '.[].name'
gh api repos/peterlodri-sec/portail/tags/protection
```
"#,
    ),
];

pub fn run_guide() -> io::Result<()> {
    let total = BLOCKS.len();
    let mut current = 0;

    println!("\n╔════════════════════════════════════════════════════════════╗");
    println!("║          Branch Protection E2E Guide                      ║");
    println!(
        "║          {} blocks — type 'help' for commands              ║",
        total
    );
    println!("╚════════════════════════════════════════════════════════════╝\n");

    print_block(current, total);

    loop {
        print!("\n> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        match input.as_str() {
            "n" | "next" | "" => {
                if current < total - 1 {
                    current += 1;
                    print_block(current, total);
                } else {
                    println!("\n✓ End of guide. Type 'exit' to quit.\n");
                }
            }
            "p" | "prev" | "back" => {
                if current > 0 {
                    current -= 1;
                    print_block(current, total);
                } else {
                    println!("\n← Already at the beginning.\n");
                }
            }
            "f" | "first" => {
                current = 0;
                print_block(current, total);
            }
            "l" | "last" => {
                current = total - 1;
                print_block(current, total);
            }
            "j" | "jump" => {
                print!("Jump to block (1-{}): ", total);
                io::stdout().flush()?;
                let mut num = String::new();
                io::stdin().read_line(&mut num)?;
                if let Ok(n) = num.trim().parse::<usize>() {
                    if n >= 1 && n <= total {
                        current = n - 1;
                        print_block(current, total);
                    } else {
                        println!("\n⚠ Invalid block number.\n");
                    }
                } else {
                    println!("\n⚠ Enter a number.\n");
                }
            }
            "list" => {
                println!("\nBlocks:");
                for (i, (title, _)) in BLOCKS.iter().enumerate() {
                    let marker = if i == current { "▶" } else { " " };
                    println!("  {} {:2}. {}", marker, i + 1, title);
                }
                println!();
            }
            "h" | "help" | "?" => {
                print_help();
            }
            "q" | "exit" | "quit" => {
                println!("\n✓ Done. Run 'portail guide' anytime.\n");
                break;
            }
            _ => {
                println!("\n⚠ Unknown command. Type 'help' for options.\n");
            }
        }
    }

    Ok(())
}

fn print_block(current: usize, total: usize) {
    let (title, content) = BLOCKS[current];

    println!("\n┌─────────────────────────────────────────────────────────────┐");
    println!("│ {:2}/{}  {:<53} │", current + 1, total, title);
    println!("└─────────────────────────────────────────────────────────────┘");
    println!("{}", content);
    println!("─────────────────────────────────────────────────────────────");
    if current < total - 1 {
        println!("  [Enter] next  [p] prev  [j] jump  [l] list  [h] help  [q] exit");
    } else {
        println!("  [p] prev  [j] jump  [l] list  [h] help  [q] exit");
    }
}

fn print_help() {
    println!("\n┌─────────────────────────────────────────────────────────────┐");
    println!("│                    Commands                                 │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!("│                                                             │");
    println!("│  n, next, Enter  — Next block                               │");
    println!("│  p, prev, back   — Previous block                           │");
    println!("│  f, first        — First block                              │");
    println!("│  l, last         — Last block                               │");
    println!("│  j, jump         — Jump to block number                     │");
    println!("│  l, list         — List all blocks                          │");
    println!("│  h, help, ?      — Show this help                           │");
    println!("│  q, exit, quit   — Exit guide                               │");
    println!("│                                                             │");
    println!("└─────────────────────────────────────────────────────────────┘\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_not_empty() {
        assert!(!BLOCKS.is_empty());
    }

    #[test]
    fn blocks_have_content() {
        for (title, content) in BLOCKS {
            assert!(!title.is_empty());
            assert!(!content.is_empty());
        }
    }
}
