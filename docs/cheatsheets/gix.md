# gitoxide (gix) — Pure Rust Git Cheatsheet

**Crate:** `gix` (high-level), `gix-*` (plumbing crates)
**Repo:** https://github.com/GitoxideLabs/gitoxide
**Docs:** https://docs.rs/gix

---

## Quick Start

```toml
[dependencies]
gix = "0.70"
```

```rust
use gix::Repository;

// Open existing repo
let repo = Repository::open(".")?;
println!("{:?}", repo.git_dir());
```

---

## Repository Operations

| Operation | Code |
|-----------|------|
| **Open** | `Repository::open(path)?` |
| **Open from env** | `Repository::discover(path)?` (walks up to find `.git`) |
| **Init** | `Repository::init(path, gix::create::Kind::WithWorktree)?` |
| **Init bare** | `Repository::init(path, gix::create::Kind::Bare)?` |
| **Clone** | `gix::clone(repo_url, dir, gix::clone::Kind::Full)?` |
| **Open submodule** | `repo.submodule(name)?.open()?` |

### Worktree operations (like wtp)

```rust
use gix::Repository;

let repo = Repository::open(".")?;

// List worktrees
for wt in repo.worktrees()?.iter() {
    let wt = wt?;
    println!("{} @ {}", wt.name(), wt.path().display());
}

// Add worktree
let worktree_path = std::path::Path::new("../worktrees/my-feature");
let wt = repo.opts().worktrees().add("my-feature", worktree_path)?;

// Remove worktree
repo.worktrees()?.remove("my-feature")?;
```

---

## Object Database

```rust
// Find object by SHA
let obj = repo.find_object(gix::ObjectId::from_hex(b"abc123def...")?)?;

// Read blob
let blob = obj.into_blob();
println!("Blob: {} bytes", blob.data.len());

// Read tree
let tree = repo.find_object(id)?.into_tree();
for entry in tree.iter() {
    println!("{} {} {}", entry.mode(), entry.filename(), entry.oid());
}

// Read commit
let commit = repo.find_object(id)?.into_commit();
let commit_ref = commit.decode()?;
println!("{}: {}", commit_ref.author().name, commit_ref.message());
```

---

## References (Branches, Tags)

```rust
// List branches
for branch in repo.main_branch()? {
    println!("{}", branch.name());
}

// Iterate all references
let mut refs = repo.references()?.all()?.peeled()?;
while let Some(ref_) = refs.next()? {
    println!("{} → {}", ref_.name(), ref_.target().id());
}

// Find branch
let branch = repo.find_branch("feature/auth")?;
println!("{}", branch.reference.target().id());

// Create branch
let commit = repo.head_commit()?;
repo.branch("feature/abc", commit, gix::branch::Create::Default)?;

// Delete branch
repo.references()?.delete(branch)?;
```

---

## Status & Index

```rust
// Read index
let index = repo.index()?;
println!("Index has {} entries", index.entries().len());

// Status
let status = repo.status(gix::status::Options::default())?;
for entry in status.into_iter() {
    match entry {
        gix::status::Entry::Modified(_, _) => println!("modified"),
        gix::status::Entry::Untracked(path) => println!("untracked: {}", path),
        _ => {}
    }
}

// Diff
let diff = repo.diff().tree_to_workdir()?;
for entry in diff.into_iter() {
    println!("diff: {}", entry.path());
}
```

---

## Commits

```rust
// HEAD commit
let head = repo.head_commit()?;
println!("{}", head.message());

// Create commit
let tree = repo.index_to_tree()?;
let signature = gix::actor::Signature::now("Author", "email@example.com")?;
repo.commit("refs/heads/main", &signature, &signature, "message", tree)?;

// Walk ancestors
for commit in repo.head_commit()?.ancestors().all()? {
    let commit = commit?;
    println!("{}", commit.id());
}
```

---

## Remotes & Fetch/Push

```rust
// List remotes
for remote in repo.remotes()? {
    let remote = remote?;
    println!("{} → {}", remote.name(), remote.url(gix::remote::Direction::Fetch));
}

// Fetch
let mut remote = repo.find_remote("origin")?;
let outcome = remote.fetch(gix::progress::Discard, Default::default())?;

// Push
remote.push(gix::remote::PushOptions::default())?;
```

---

## Configuration

```rust
// Read config
let config = repo.config_snapshot();
let user_name: String = config.string("user.name")?.unwrap();

// Set config
repo.config_snapshot_mut()?.set("user.name", "Author")?;
```

---

## Intercepting git/gh Commands

Replace subprocess `git`/`gh` calls with native gix:

```rust
use gix::Repository;

/// Git operation enum — covers common CLI patterns
pub enum GitOp {
    WorktreeAdd { branch: String, path: std::path::PathBuf },
    WorktreeRemove { name: String },
    BranchList,
    Status { porcelain: bool },
    Fetch { remote: String },
    Log { max_count: usize },
}

pub fn execute_git_op(repo: &Repository, op: GitOp) -> Result<(), Box<dyn std::error::Error>> {
    match op {
        GitOp::WorktreeAdd { branch, path } => {
            repo.worktrees()?.add(&branch, &path)?;
        }
        GitOp::WorktreeRemove { name } => {
            repo.worktrees()?.remove(&name)?;
        }
        GitOp::BranchList => {
            for b in repo.references()?.all()?? {
                println!("  {}", b.name());
            }
        }
        GitOp::Status { porcelain: true } => {
            let status = repo.status(Default::default())?;
            for s in &status {
                println!("{:?}", s);
            }
        }
        GitOp::Fetch { remote: _ } => {
            // gix fetch is still developing
            println!("fetch via gix (experimental)");
        }
        GitOp::Log { max_count } => {
            for c in repo.head_commit()?.ancestors().all()?.take(max_count) {
                let c = c?;
                println!("{} {}", c.id().to_hex(), c.message().summary());
            }
        }
    }
    Ok(())
}
```

---

## Cross-Process Memory (process_vm_readv/writev)

Linux 3.2+ syscalls for reading/writing another process's memory.

```rust
use libc::{process_vm_readv, process_vm_writev, iovec, pid_t};
use std::mem;

pub fn read_mem<T: Copy>(pid: pid_t, addr: usize) -> Option<T> {
    let mut val: T = unsafe { mem::zeroed() };
    let local = iovec {
        iov_base: &mut val as *mut _ as *mut _,
        iov_len: mem::size_of::<T>(),
    };
    let remote = iovec {
        iov_base: addr as *mut _,
        iov_len: mem::size_of::<T>(),
    };
    let n = unsafe {
        process_vm_readv(pid, &local, 1, &remote, 1, 0)
    };
    if n >= 0 { Some(val) } else { None }
}

pub fn write_mem<T: Copy>(pid: pid_t, addr: usize, val: &T) -> bool {
    let local = iovec {
        iov_base: &val as *const _ as *mut _,
        iov_len: mem::size_of::<T>(),
    };
    let remote = iovec {
        iov_base: addr as *mut _,
        iov_len: mem::size_of::<T>(),
    };
    unsafe {
        process_vm_writev(pid, &local, 1, &remote, 1, 0) >= 0
    }
}
```

**Permissions:** Set `kernel.yama.ptrace_scope = 0` or run as root.
**Verify maps:** `cat /proc/<pid>/maps` to find valid address ranges.

---

## Architecture Decision: gix vs Command-Line git

| Aspect | `std::process::Command("git")` | `gix` crate |
|--------|-------------------------------|-------------|
| Dependencies | None (OS provides git) | 80+ transitive crates |
| Speed | ~10-50ms per invocation | ~0.1ms (in-process) |
| Memory | Separate process | In-process library |
| Portability | Requires git installed | Pure Rust, single binary |
| API safety | String parsing | Typed API, compiler-checked |
| Feature coverage | Complete | Growing (~70% of common ops) |
