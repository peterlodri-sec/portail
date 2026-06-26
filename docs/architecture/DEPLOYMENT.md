# Deployment Architecture — Hyper-Parallelized Builds & OS Strategy

**Reference:** Hyper-parallelized Rust+Nix build blueprint, NixOS ephemeral-root
configuration, and cloud-native OS decision matrix for the Portail AI proxy stack.

---

## Build Pipeline: Crane + Mold + Thin LTO

The Nix flake uses `crane` with a decoupled derivation strategy:

```
Source Tree
    │
    ▼
craneLib.cleanCargoSource  (filters non-Rust changes from cache-busting)
    │
    ▼
craneLib.buildDepsOnly     (LAYER 1: compile all 3rd-party deps once)
    │                        cached until Cargo.lock changes
    ▼
craneLib.buildPackage       (LAYER 2: compile only local workspace crates)
    │                        pulls cargoArtifacts from layer 1
    ▼
postInstall                 (LAYER 3: genesis seal — sha256sum, SBOM)
```

### Optimal build flags

```bash
# Saturation: Nix evaluates in parallel, Cargo uses all cores
nix build .#portail --max-jobs auto --cores 0

# Thin LTO parallelizes codegen (fat LTO is single-threaded)
CARGO_PROFILE_RELEASE_LTO=thin
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16

# Fast linker
RUSTFLAGS="-C linker=mold -C link-arg=-Wl,--threads=all"

# Deterministic binary — strip absolute host paths
RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=$PWD=/portail-src"
```

### Genesis Seal (postInstall)

```bash
mkdir -p $out/var/portail
sha256sum $out/bin/portail > $out/var/portail/GENESIS_SEAL.hash
sha256sum $out/bin/portail-mon > $out/var/portail/GENESIS_SEAL.mon.hash
```

---

## NixOS Configuration — Ephemeral Root + Hardened Kernel

For nodes running Portail in production:

```nix
{
  # Ephemeral root — every boot is a clean slate
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
    options = [ "defaults" "size=4G" "mode=755" ];
  };
  fileSystems."/persist" = { device = "/dev/disk/by-uuid/..."; neededForBoot = true; };

  # Kernel tuning for high-concurrency proxy loops
  boot.kernel.sysctl = {
    "fs.file-max" = 2097152;
    "net.core.somaxconn" = 4096;
    "net.ipv4.tcp_fastopen" = 3;
  };

  # Minimal — strip bloat
  documentation.enable = false;
  sound.enable = false;
}
```

---

## OS Decision Matrix

| Criteria | Talos Linux | Flatcar Container Linux | Custom NixOS JeOS |
|----------|-------------|------------------------|-------------------|
| Shell access | ❌ None — gRPC API only | ✅ systemd + bash | ✅ Full Nix |
| Root filesystem | Read-only squashfs + tmpfs | Read-only A/B partitions | Ephemeral tmpfs |
| Init system | Custom Go binary | systemd | systemd or direct binary |
| Kubernetes | Native (designed for it) | Optional | Manual |
| Container runtime | containerd | Docker / containerd | Any |
| Update model | Talos API | Atomic A/B flip | nixos-rebuild switch |
| Best for | Enterprise K8s fabric | Bare-metal container host | Custom hardened appliance |

**Portail recommendation:** Custom NixOS JeOS for development + edge nodes where
full control over kernel tuning and binary sealing is required. Talos Linux for
large-scale Kubernetes deployments where the OS must be an invisible appliance.

---

## CI Pipeline Integration

```
Git push
    │
    ▼
GitHub Actions + self-hosted Nix builder
    │
    ├── nix build .#portail --max-jobs auto --cores 0
    │   └── crane: deps cached → app compiles in seconds
    │
    ├── nix build .#docker
    │   └── distroless image with genesis seal
    │
    ├── portail release-audit  (SBOM + report + stamp)
    │
    └── cosign sign-blob       (keyless signing)
        └── GitHub Release
```
