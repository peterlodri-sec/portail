{
  description = "Portail — Hyper-Optimized Rust Development Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, crane, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        # Rust toolchain: nightly with wasm + cranelift support
        rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [
            "rust-src"
            "rustfmt"
            "clippy"
            "llvm-tools"
            "rustc-codegen-cranelift-preview"
          ];
          targets = [
            "wasm32-unknown-unknown"
            "aarch64-unknown-linux-gnu"
            "x86_64-unknown-linux-gnu"
          ];
        };

        # Linker: mold (Linux) or zld (macOS) — 5x faster linking
        linkerFlags = if pkgs.stdenv.isLinux then [
          "-C" "link-arg=-fuse-ld=mold"
        ] else if pkgs.stdenv.isDarwin then [
          "-C" "link-arg=-fuse-ld=/opt/homebrew/bin/zld"
        ] else [];

        # ═══════════════════════════════════════════════════════
        # Crane Setup: split deps from app for cached builds
        # ═══════════════════════════════════════════════════════
        craneLib = crane.mkLib pkgs;
        src = craneLib.cleanCargoSource (craneLib.path ./.);

        # Common build args shared across profiles
        commonBuildArgs = {
          nativeBuildInputs = with pkgs; [ pkg-config installShellFiles ];
          buildInputs = with pkgs; [ aws-lc openssl zlib zstd ]
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ mold-wrapped ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.darwin.apple_sdk.frameworks.Security
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
              pkgs.darwin.apple_sdk.frameworks.CoreFoundation
            ];
          RUSTFLAGS = "-C linker=mold -C link-arg=-Wl,--threads=all --remap-path-prefix=${toString ./..}=/portail-src";
        };

        cargoArtifacts = craneLib.buildDepsOnly {
          inherit src;
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ aws-lc openssl zlib zstd ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.darwin.apple_sdk.frameworks.Security
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
              pkgs.darwin.apple_sdk.frameworks.CoreFoundation
            ];
        };
      in {
        # ── Packages ────────────────────────────────────────
        packages = {
          # ── Thin LTO profile (parallelized, fast) ──────────
          default = craneLib.buildPackage (commonBuildArgs // {
            inherit src cargoArtifacts;
            CARGO_PROFILE_RELEASE_LTO = "thin";
            CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "16";
            CARGO_PROFILE_RELEASE_OPT_LEVEL = "3";
            CARGO_PROFILE_RELEASE_STRIP = "symbols";
            CARGO_PROFILE_RELEASE_DEBUG = "false";

            postInstall = ''
              echo "=== Genesis Seal ==="
              mkdir -p $out/var/portail
              sha256sum $out/bin/portail > $out/var/portail/GENESIS_SEAL.hash
              echo "Seal: $(cat $out/var/portail/GENESIS_SEAL.hash)"
            '';
          });

          # ── Fat LTO profile (max optimization, slower) ─────
          portail-max = craneLib.buildPackage (commonBuildArgs // {
            inherit src cargoArtifacts;
            CARGO_PROFILE_RELEASE_LTO = "fat";
            CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1";
            CARGO_PROFILE_RELEASE_OPT_LEVEL = "3";
            CARGO_PROFILE_RELEASE_STRIP = "symbols";
            CARGO_PROFILE_RELEASE_DEBUG = "false";

            postInstall = ''
              echo "=== Genesis Seal (max) ==="
              mkdir -p $out/var/portail
              sha256sum $out/bin/portail > $out/var/portail/GENESIS_SEAL.hash
            '';
          });

          portail = self.packages.${system}.default;
          portail-mcp = pkgs.callPackage ./nix/mcp-plugin.nix { };
        };

        # ── Dev Shell (native cargo, direnv-optimized) ─────
        devShells.default = pkgs.mkShell {
          name = "portail-dev-hyper";

          nativeBuildInputs = with pkgs; [
            rustToolchain
            pkg-config
            mold-wrapped
            zld
            sccache
            cargo-nextest
            cargo-watch
            cargo-expand
            cargo-audit
            cargo-outdated
            cargo-deny
            cargo-llvm-cov
            hyperfine
            just
            upx
            cosign
            python312
            nodejs_22
            bun
            nushell
            parallel
            # Modern CLI layer — no legacy grep/awk/sed/cat/ls/find
            ripgrep         # rg — replaces grep
            jq              # structured JSON queries
            sd              # replaces sed
            bat             # replaces cat
            eza             # replaces ls
            fd              # replaces find
            dua             # replaces du
            dust            # visual disk usage
            bottom          # btm — replaces top
            delta           # diff viewer
            doggo           # replaces dig
            gping           # replaces ping
            websocat        # replaces netcat/curl for WS
            httpie          # replaces curl
            zellij          # replaces tmux
            mosh            # replaces ssh roaming
          ];

          buildInputs = with pkgs; [
            aws-lc
            openssl
            zlib
            zstd
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            pkgs.darwin.apple_sdk.frameworks.CoreFoundation
          ];

          # ── Environment variables for native cargo speed ──
          RUSTFLAGS = pkgs.lib.concatStringsSep " " (
            linkerFlags ++ [ "-Zshare-generics=y" "-Zthreads=0" ]
          );
          RUST_BACKTRACE = "1";
          CARGO_BUILD_JOBS = "0";
          CARGO_INCREMENTAL = "0";
          SCCACHE_DIR = "${builtins.getEnv "HOME"}/.cache/sccache";
          RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";
          CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER = "lld";

          # Dynamic library paths for Nix-provided deps
          PKG_CONFIG_PATH = "${pkgs.aws-lc.dev}/lib/pkgconfig:${pkgs.openssl.dev}/lib/pkgconfig";

          shellHook = ''
            echo ""
            echo "🛞  Portail Hyper Dev Shell (Nix-Native)"
            echo "   rustc  — ${rustToolchain.version}"
            echo "   linker — ${if pkgs.stdenv.isLinux then "mold" else "zld"} (5x fast)"
            echo "   test   — cargo nextest (parallel)"
            echo "   cache  — sccache (shared)"
            echo ""
            echo "   task c        — fast check"
            echo "   task t        — test affected modules"
            echo "   task bench    — benchmarks"
            echo "   task release  — production build"
            echo "   task e2e      — end-to-end suite"
            echo ""
            echo "   Build: nix build     (cached, production)"
            echo "   Dev:   cargo build   (native, instant)"
          '';
        };

        # ── Light dev shell (no AI runtimes) ───────────────────
        devShells.light = pkgs.mkShell {
          name = "portail-dev-light";
          nativeBuildInputs = with pkgs; [ rustToolchain pkg-config mold-wrapped zld sccache cargo-nextest just ];
          buildInputs = with pkgs; [ aws-lc openssl zlib ];
          RUST_BACKTRACE = "1";
          shellHook = ''
            echo "🛞  Portail Light Dev Shell"
          '';
        };
      });
}
