{
  description = "Portail — unified proxy/gateway: AI Gateway + MCP Gateway + CDN cache";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = inputs@{ flake-parts, nixpkgs, rust-overlay, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];

      perSystem = { config, pkgs, system, ... }: let
        rust' = pkgs.rust-bin.stable.latest.minimal.override {
          extensions = [ "rustc" "cargo" "clippy" "rustfmt" "rust-src" ];
        };
      in {
        packages = {
          default = pkgs.callPackage ./nix/package.nix { };
          portail = pkgs.callPackage ./nix/package.nix { };
          portail-mcp = pkgs.callPackage ./nix/mcp-plugin.nix { };
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rust'
            cargo-watch
            cargo-nextest
            cargo-expand
            cargo-audit
            cargo-outdated
            cargo-deny
            tokio-console
            hyperfine
            just
            upx
            cosign
          ] ++ lib.optionals pkgs.stdenv.isLinux [
            mold-wrapped
            gdb
          ] ++ lib.optionals pkgs.stdenv.isDarwin [
            darwin.apple_sdk.frameworks.SystemConfiguration
          ];

          buildInputs = with pkgs; [ openssl pkg-config zlib ];

          shellHook = ''
            echo "🛞  Portail dev shell"
            echo "   cargo build     — debug build"
            echo "   cargo test      — run all tests"
            echo "   cargo nextest   — faster test runner"
            echo "   cargo watch     — auto-build on changes"
            echo "   just bench      — criterion benchmarks"
            echo "   just release    — LTO release build"
          '';
        };

        checks = {
          build = config.packages.default;
          clippy = pkgs.runCommand "clippy-check" {
            buildInputs = [ rust' pkgs.openssl pkgs.pkg-config ];
          } ''
            cd ${../.}
            cargo clippy --locked --all-targets -- -D warnings
            touch $out
          '';
        };
      };

      flake.nixosModules = {
        default = ./nix/module.nix;
        portail = ./nix/module.nix;
      };
    };
}
