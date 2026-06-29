{
  description = "Portail — Hyper-Optimized Rust + Nix Development Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    devshell.url = "github:numtide/devshell";
    devshell.inputs.nixpkgs.follows = "nixpkgs";
    git-hooks.url = "github:cachix/git-hooks.nix";
    git-hooks.inputs.nixpkgs.follows = "nixpkgs";
    treefmt-nix.url = "github:numtide/treefmt-nix";
    treefmt-nix.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = inputs@{ self, nixpkgs, flake-parts, devshell, git-hooks, treefmt-nix, rust-overlay, crane, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        devshell.flakeModule
        git-hooks.flakeModule
        treefmt-nix.flakeModule
      ];

      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

      perSystem = { config, self', inputs', pkgs, system, ... }:
        let
          overlays = [ (import inputs.rust-overlay) ];
          pkgsRust = import nixpkgs { inherit system overlays; };

          rustToolchain = pkgsRust.rust-bin.nightly.latest.default.override {
            extensions = [
              "rust-src" "rustfmt" "clippy" "llvm-tools"
              "rustc-codegen-cranelift-preview"
            ];
            targets = [
              "wasm32-unknown-unknown"
              "aarch64-unknown-linux-gnu"
              "x86_64-unknown-linux-gnu"
            ];
          };

          linkerFlags = if pkgs.stdenv.isLinux then [
            "-C" "link-arg=-fuse-ld=mold"
          ] else if pkgs.stdenv.isDarwin then [
            "-C" "link-arg=-fuse-ld=/opt/homebrew/bin/zld"
          ] else [];

          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
          src = craneLib.cleanCargoSource (craneLib.path ./.);

          commonBuildArgs = {
            nativeBuildInputs = with pkgs; [ pkg-config installShellFiles ];
            buildInputs = with pkgs; [ aws-lc openssl zlib zstd ]
              ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ mold-wrapped ]
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
                Security SystemConfiguration CoreFoundation
              ]);
            RUSTFLAGS = "-C linker=mold -C link-arg=-Wl,--threads=all --remap-path-prefix=${toString ./..}=/portail-src";
          };

          cargoArtifacts = craneLib.buildDepsOnly {
            inherit src;
            nativeBuildInputs = with pkgs; [ pkg-config ];
            buildInputs = with pkgs; [ aws-lc openssl zlib zstd ]
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin
                [ Security SystemConfiguration CoreFoundation ];
          };

          opencodeMux = import ./nix/opencode-mux.nix {
            inherit pkgs;
          };
        in
        {
          treefmt = {
            projectRootFile = "flake.nix";
            programs = {
              nixpkgs-fmt.enable = true;
              rustfmt.enable = true;
              taplo.enable = true;
            };
          };

          pre-commit.settings = {
            hooks = {
              treefmt.enable = true;
              cargo-check = {
                enable = true;
                entry = "cargo check --all-targets --workspace";
                pass_filenames = false;
              };
            };
          };

          packages = {
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
              '';
            });

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

            portail = self'.packages.default;

            portail-mcp = pkgs.callPackage ./nix/mcp-plugin.nix { };
          };

          checks = {
            clippy-pass = craneLib.cargoClippy {
              inherit src cargoArtifacts;
              cargoClippyExtraArgs = "-- --deny warnings";
            };
            fmt-pass = craneLib.cargoFmt { inherit src; };
          };

          devShells.default = pkgs.mkShell {
            name = "portail-dev-hyper";

            env = {
              RUST_LOG = "info";
              RUST_BACKTRACE = "1";
              CARGO_BUILD_JOBS = "0";
              CARGO_INCREMENTAL = "0";
              SCCACHE_DIR = "${builtins.getEnv "HOME"}/.cache/sccache";
              RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";
              PKG_CONFIG_PATH = "${pkgs.aws-lc.dev}/lib/pkgconfig:${pkgs.openssl.dev}/lib/pkgconfig";
              RUSTFLAGS = pkgs.lib.concatStringsSep " " (
                linkerFlags ++ [ "-Zshare-generics=y" "-Zthreads=0" ]
              );
            };

            packages = with pkgs; [
              rustToolchain
              pkg-config mold-wrapped zld sccache
              cargo-nextest cargo-watch cargo-expand cargo-audit cargo-outdated cargo-deny cargo-llvm-cov
              hyperfine just upx cosign
              python312 nodejs_22 bun nushell parallel
              ripgrep jq sd bat eza fd dua dust
              bottom delta doggo gping websocat httpie zellij mosh
            ];

            shellHook = ''
              echo "  check: cargo check --all-targets --workspace"
              echo "  test: cargo test --workspace"
              echo "  watch: cargo watch -x check -x test"
              echo "  fmt: nix fmt"
              echo "  lint: cargo clippy --all-targets -- --deny warnings"
              echo "  validate: nix flake check"
              echo "  audit: cargo audit"
              echo "  build: nix build .#portail"
              echo "  release: nix build .#portail-max"
              echo "  clean: cargo clean"
              echo "  coverage: cargo llvm-cov --workspace --lcov --output-path lcov.info"
              ${config.pre-commit.installationScript}
              echo "   Aliases: check, test, build, lint, fmt, validate, audit"
            '';
          };

          devShells.light = pkgs.mkShell {
            name = "portail-dev-light";
            nativeBuildInputs = with pkgs; [
              rustToolchain pkg-config mold-wrapped zld sccache cargo-nextest just
            ];
            buildInputs = with pkgs; [ aws-lc openssl zlib zstd ]
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin
                [ Security SystemConfiguration CoreFoundation ];
            RUST_BACKTRACE = "1";
            shellHook = ''
              echo "portail-dev-light — minimal dev shell"
            '';
          };

          apps = {
            ohmy-mux = opencodeMux.appMux;
            opencode-mux = opencodeMux.appNoMux;
          };
          # Ops shell: nushell + zellij + opencode (no rust toolchain).
          # `nix develop .#opencode-mux` enters a fast shell ready for
          # `ohmy-slim mux-launch` or `nix run .#ohmy-mux`.
          devShells.opencode-mux = opencodeMux.shell;
        };

        # NixOS module for services.portail — used by nix-base dev-cx53 config.
        flake.nixosModules.default = ./nix/module.nix;
    };
}
