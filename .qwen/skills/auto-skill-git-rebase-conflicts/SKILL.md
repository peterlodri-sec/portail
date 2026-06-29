---
name: git-rebase-conflicts
description: Systematic approach to resolving complex git rebase conflicts across many files with multiple conflict types.
source: auto-skill
extracted_at: '2026-06-27T13:00:00.000Z'
---

# Git Rebase Conflict Resolution

When rebasing a branch with many commits onto a diverged main branch, conflicts are inevitable. This skill provides a systematic approach to resolving them without losing work.

## Step 1: Start the rebase and identify conflict scope

```bash
git pull --rebase origin main
```

When conflicts occur, git will stop and list them. Get an overview:

```bash
git status
```

Look for:
- **Content conflicts**: Both sides modified the same lines
- **Modify/delete conflicts**: One side deleted a file, the other modified it
- **Add/add conflicts**: Both sides added the same file with different content

## Step 2: Handle modify/delete conflicts first

These are easiest to resolve. Ask: "Should this file exist?"

**Keep deleted** (file was intentionally removed):
```bash
git rm <file>
```

**Keep modified** (file deletion was accidental):
```bash
git add <file>
```

Example: If `src/cli/dashboard.rs` was deleted in HEAD but modified in the rebased commit, and the deletion was intentional (e.g., feature moved elsewhere), keep it deleted.

## Step 3: Read all content conflicts

For each conflicted file, read the full content to understand both sides:

```bash
grep -n '<<<<<<<\|=======\|>>>>>>>' <file>
```

This shows conflict marker locations. Read the file to see:
- `<<<<<<< HEAD` — your local changes
- `=======` — separator
- `>>>>>>> <commit>` — incoming changes from rebased commit

## Step 4: Resolve content conflicts systematically

For each conflict block, decide based on context:

**Option A: Keep local (HEAD)**
- When your local version is more recent/complete
- When the incoming change is outdated

**Option B: Keep incoming (rebased commit)**
- When the incoming change has newer features
- When local changes were superseded

**Option C: Merge both**
- When both sides add complementary features
- Combine the changes manually

**Option D: Use constant/reference**
- When one side uses a magic string and the other uses a constant
- Prefer the constant (cleaner, more maintainable)

Example resolution:
```rust
// Conflict:
<<<<<<< HEAD
    golden_path: GOLDEN_FILE.into(),
=======
    golden_path: "spec.routes.toml".into(),
>>>>>>> ef9ef9b

// Resolution: Keep the constant reference
    golden_path: GOLDEN_FILE.into(),
```

## Step 5: Stage resolved files and continue

After resolving all conflicts in a commit:

```bash
git add <resolved-files>
GIT_EDITOR=true git rebase --continue
```

`GIT_EDITOR=true` auto-accepts the commit message. If you want to edit it, omit `GIT_EDITOR=true`.

## Step 6: Handle subsequent conflicts

Rebase will continue to the next commit. If more conflicts occur:

1. Repeat Steps 2-5
2. If a conflict is in the same file as before, the conflict markers may be nested or shifted
3. For merge commits (commits that merged another branch), consider `git rebase --skip` if all changes are already incorporated

## Step 7: Skip merge commits when appropriate

Merge commits often cause conflicts because they combine two branches. If the merge commit's changes are already in the rebased commits:

```bash
git rebase --skip
```

This is safe when:
- The merge commit was just combining branches
- The actual changes are in the individual commits
- You're rebasing onto a branch that already has those changes

## Step 8: Verify the final result

After rebase completes:

```bash
git status
git log --oneline -10
git diff HEAD~5 --stat  # Check recent changes
```

Ensure:
- Working tree is clean (except untracked files)
- Commit history is linear
- No unexpected file changes

## Key commands

| Purpose | Command |
|---------|---------|
| Find conflict markers | `grep -n '<<<<<<<\|=======\|>>>>>>>' <file>` |
| Keep file deleted | `git rm <file>` |
| Keep file modified | `git add <file>` |
| Continue rebase | `GIT_EDITOR=true git rebase --continue` |
| Skip merge commit | `git rebase --skip` |
| Abort rebase | `git rebase --abort` |

## Pitfalls to avoid

- **Don't** blindly accept "ours" or "theirs" — read both sides
- **Don't** skip conflict resolution — unresolved markers will break compilation
- **Don't** panic on nested conflicts — they're just multiple conflict blocks in one file
- **Do** resolve conflicts incrementally — one commit at a time
- **Do** verify compilation after rebase completes
- **Do** use `git rebase --abort` if you make a mistake — you can start over

## Example workflow

```bash
# Start rebase
git pull --rebase origin main

# Conflict on commit 1/20
git status  # See conflicted files
git rm src/cli/dashboard.rs  # Keep deleted
read_file src/proxy.rs  # Read conflict
edit src/proxy.rs  # Resolve conflict
git add src/proxy.rs
GIT_EDITOR=true git rebase --continue

# Conflict on commit 3/20
# ... repeat resolution process

# Conflict on merge commit 20/20
git rebase --skip  # Changes already incorporated

# Verify
git status
git log --oneline -5
```
