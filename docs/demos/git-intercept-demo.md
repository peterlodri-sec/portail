# Git/Command Interceptor — Force Every Process in the Box to Use Optimized Tools

**Goal:** Any binary, script, or app running in our environment that calls `git`, `gh`, or
other CLI tools gets transparently intercepted and routed through native Rust
implementations (gix, octocrab, etc.) via execve-syscall hooks.

No subprocess overhead. No dependency on system-installed tools. Single-binary
deployment. Cross-process introspection via `process_vm_readv`/`writev`.

---

## Architecture

```
                    ┌─────────────────────────┐
                    │  Any Process in our Box  │
                    │  (editor, CI, script,    │
                    │   IDE, build tool, etc.) │
                    └──────────┬──────────────┘
                               │
                    execve("git", ...)
                    execve("gh", ...)
                    execve("make", ...)
                               │
                               ▼
            ┌──────────────────────────────────┐
            │     libintercept.so              │
            │  (LD_PRELOAD / sysext / ebpf)    │
            │                                  │
            │  hooks execve/execvp/execl       │
            │  matches argv[0] against:        │
            │    • git     → gix (Rust)        │
            │    • gh      → octocrab (Rust)   │
            │    • make    → native make-rs    │
            │    • *       → pass-through      │
            └────────────────┬─────────────────┘
                             │
                    ┌────────┴────────┐
                    │                 │
                    ▼                 ▼
            ┌──────────────┐  ┌──────────────┐
            │  Native Rust  │  │ Real system  │
            │  Handler     │  │ binary       │
            │  (gix, etc.) │  │ (passthrough)│
            └──────────────┘  └──────────────┘
```

## Layer 1: LD_PRELOAD execve Hook

The core intercept mechanism: a shared library that replaces `execve`, `execvp`,
and `execl` with versions that check if the target command has a native handler.

```rust
// lib.rs — compile as cdylib
use libc::{execve, c_char, c_int};
use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::collections::HashMap;

// Registry of commands and their native Rust handlers
lazy_static! {
    static ref HANDLERS: HashMap<&'static str, fn(&[&str]) -> i32> = {
        let mut m = HashMap::new();
        m.insert("git", handle_git as fn(&[&str]) -> i32);
        m.insert("gh",  handle_gh  as fn(&[&str]) -> i32);
        m
    };
}

#[no_mangle]
pub unsafe extern "C" fn execve(
    pathname: *const c_char,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    // Extract command name from argv[0]
    let cmd = CStr::from_ptr(*argv).to_bytes();
    let cmd_str = std::str::from_utf8(cmd).unwrap_or("");

    // Check if we have a handler
    if let Some(handler) = HANDLERS.get(cmd_str) {
        let args: Vec<&str> = /* collect argv into &str slice */;
        return handler(&args);
    }

    // Passthrough to real execve
    execve(pathname, argv, envp)
}
```

## Layer 2: Native Handlers

### git → gix

```rust
use gix::Repository;

fn handle_git(args: &[&str]) -> i32 {
    let repo = match Repository::open(".") {
        Ok(r) => r,
        Err(_) => return call_real_git(args),
    };

    match args {
        ["git", "worktree", "add", branch] => {
            let path = std::path::PathBuf::from("../worktrees").join(branch);
            match repo.worktrees()?.add(branch, &path) {
                Ok(_) => { println!("worktree at {}", path.display()); 0 }
                Err(e) => { eprintln!("{e}"); 1 }
            }
        }
        ["git", "worktree", "list"] => {
            for wt in repo.worktrees()?.iter() {
                if let Ok(wt) = wt {
                    println!("{}  {}", wt.name(), wt.path().display());
                }
            }
            0
        }
        ["git", "status", ..] => {
            let status = repo.status(Default::default());
            // render status...
            0
        }
        ["git", "log", ..] => {
            for c in repo.head_commit().unwrap().ancestors().all().unwrap() {
                if let Ok(c) = c {
                    println!("{} {}", c.id(), c.message().summary());
                }
            }
            0
        }
        _ => call_real_git(args),
    }
}
```

### gh → octocrab

```rust
use octocrab::Octocrab;

fn handle_gh(args: &[&str]) -> i32 {
    let octo = Octocrab::builder()
        .personal_token(std::env::var("GITHUB_TOKEN").unwrap_or_default())
        .build()
        .unwrap();

    match args {
        ["gh", "pr", "list"] => {
            let prs = octo.current().list_prs().send().await.unwrap();
            for pr in prs {
                println!("#{} {}", pr.number, pr.title);
            }
            0
        }
        ["gh", "pr", "view", n] => {
            let n: u64 = n.parse().unwrap();
            let pr = octo.current().get_pr(n).await.unwrap();
            println!("{} — {} ({})", pr.title, pr.state, pr.html_url);
            0
        }
        ["gh", "issue", "list"] => {
            let issues = octo.current().list_issues().send().await.unwrap();
            for issue in issues {
                println!("#{} {}", issue.number, issue.title);
            }
            0
        }
        _ => call_real_gh(args),
    }
}
```

## Layer 3: Cross-Process Introspection

Use `process_vm_readv`/`writev` to inspect memory of any intercepted process.
Combined with `/proc/<pid>/maps` to discover valid address ranges.

```rust
use libc::{process_vm_readv, process_vm_writev, iovec, pid_t};
use std::fs;
use std::mem;

/// Discover memory regions of a process from /proc/<pid>/maps
fn read_maps(pid: pid_t) -> Vec<(usize, usize, String)> {
    let maps = fs::read_to_string(format!("/proc/{pid}/maps")).unwrap_or_default();
    let mut regions = Vec::new();
    for line in maps.lines() {
        let parts: Vec<&str> = line.splitn(5, ' ').collect();
        if parts.len() < 2 { continue; }
        let addrs: Vec<&str> = parts[0].split('-').collect();
        if addrs.len() != 2 { continue; }
        let start = usize::from_str_radix(addrs[0], 16).unwrap_or(0);
        let end   = usize::from_str_radix(addrs[1], 16).unwrap_or(0);
        let perms = parts[1].to_string();
        regions.push((start, end, perms));
    }
    regions
}

/// Generic read of T from remote process at given address
fn read_mem<T: Copy>(pid: pid_t, addr: usize) -> Option<T> {
    let mut val: T = unsafe { mem::zeroed() };
    let local = iovec {
        iov_base: &mut val as *mut _ as *mut _,
        iov_len: mem::size_of::<T>(),
    };
    let remote = iovec {
        iov_base: addr as *mut _,
        iov_len: mem::size_of::<T>(),
    };
    if unsafe { process_vm_readv(pid, &local, 1, &remote, 1, 0) } >= 0 {
        Some(val)
    } else {
        None
    }
}
```

## Deployment: The "Intercept Box"

```
┌─────────────────────────────────────────────────┐
│              The Box (container/VM/chroot)        │
│                                                   │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  │
│  │  App A     │  │  App B     │  │  Agent C   │  │
│  │  (calls    │  │  (calls    │  │  (calls    │  │
│  │   git/gh)  │  │   git)    │  │   gh)      │  │
│  └──────┬─────┘  └──────┬─────┘  └──────┬─────┘  │
│         │               │               │          │
│         └───────────────┼───────────────┘          │
│                         │                          │
│                         ▼                          │
│              ┌────────────────────┐                │
│              │  libintercept.so   │                │
│              │  (LD_PRELOAD)      │                │
│              └──────┬─────────────┘                │
│                     │                               │
│          ┌──────────┴──────────┐                   │
│          ▼                     ▼                    │
│  ┌──────────────┐   ┌─────────────────┐           │
│  │  gix handler │   │  octocrab handler│           │
│  │  (git work-  │   │  (gh pr, issue)  │           │
│  │   tree, log, │   └─────────────────┘           │
│  │   status)    │                                  │
│  └──────────────┘                                  │
│                                                   │
│  ┌──────────────────────────────────────────┐     │
│  │  Process Memory Inspector                │     │
│  │  (read_mem<T>, write_mem<T>, maps scan)  │     │
│  └──────────────────────────────────────────┘     │
└─────────────────────────────────────────────────┘
```

## Quick Start Demo

```bash
# 1. Create a standalone project
cargo new intercept-box --lib
cd intercept-box

# 2. Dependencies
cargo add gix
cargo add octocrab
cargo add libc
cargo add anyhow
cargo add clap --features derive

# 3. Build the interceptor shared library
cargo build --release --lib  # produces libintercept_box.so

# 4. Use it
LD_PRELOAD=./target/release/libintercept_box.so git status
LD_PRELOAD=./target/release/libintercept_box.so gh pr list

# 5. Or system-wide (container only!)
echo "LD_PRELOAD=/opt/intercept/libintercept_box.so" > /etc/ld.so.preload
```

## Implementation Plan

| Phase | What | How |
|-------|------|-----|
| 1 | `execve` hook | LD_PRELOAD shared library, intercepts execve/execvp |
| 2 | `git`→`gix` mapper | gix worktree, status, log, branch |
| 3 | `gh`→`octocrab` mapper | gh pr, issue, release via octocrab |
| 4 | Process memory inspector | `process_vm_readv`/`writev` + `/proc/<pid>/maps` |
| 5 | `make`→`make-rs` | Native Rust make replacement |
| 6 | `curl`→`rustls` | curl → reqwest with rustls |
| 7 | Systemd sysext | Immutable-root deployment via sysext images |
| 8 | eBPF fallback | eBPF-based execve tracing (when LD_PRELOAD not possible) |

## Files

| File | Role |
|------|------|
| `src/lib.rs` | LD_PRELOAD shared library — execve hook + dispatch |
| `src/git_ops.rs` | gix-native git command handlers |
| `src/gh_ops.rs` | octocrab-native gh command handlers |
| `src/memory.rs` | Cross-process memory read/write (`process_vm_*`) |
| `src/binary_cache.rs` | SHA256-pinned binary cache for passthrough commands |
| `Cargo.toml` | Dependencies: gix, octocrab, libc |
