# Release Process

## Prerequisites

- [ ] `CARGO_REGISTRY_TOKEN` set as GitHub secret (Settings → Secrets and variables → Actions)
- [ ] `CODECOV_TOKEN` set for coverage upload (optional)
- [ ] GitHub Actions permissions: "Read and write" + "Allow GitHub Actions to create releases"
- [ ] Container registry: ghcr.io package visibility set to public (Settings → Packages)

## One-time: crates.io credentials

Generate a token at https://crates.io/settings/tokens, then:

```bash
just login
```
Paste the token when prompted. Cargo stores it in `~/.cargo/credentials`.

Optional — keychain-backed storage:
```bash
cargo install cargo-credential-pass
# then add to ~/.cargo/config.toml:
# [registry]
# global-credential-providers = ["cargo-credential-pass"]
```

Verify:
```bash
just publish-dry
```

## Release steps

1. **Ensure main is clean**
   ```bash
   git checkout main && git pull
   just ci
   ```

2. **Bump version** (if not a patch release)
   - Update `version` in `Cargo.toml`
   - Update `CHANGELOG.md` date

3. **Tag and push**
   ```bash
   git tag v0.1.0
   git push --tags
   ```

4. **Verify workflows** (5-10 min)
   - [GitHub Actions](https://github.com/peterlodri-sec/portail/actions): `release.yml` + `docker.yml`
   - [GitHub Release](https://github.com/peterlodri-sec/portail/releases): binary artifacts + checksums + signatures
   - [ghcr.io](https://github.com/peterlodri-sec/portail/pkgs/container/portail): multi-arch image + tags
   - [crates.io](https://crates.io/crates/portail): package published

5. **Verify signatures**
   ```bash
   gh release download v0.1.0
   cosign verify-blob \
     --cert portail-x86_64-unknown-linux-gnu.pem \
     --signature portail-x86_64-unknown-linux-gnu.sig \
     portail-x86_64-unknown-linux-gnu
   ```

6. **Switch nix-base to GitHub source** (after v0.1.0)
   ```nix
   # in nix-base/flake.nix
   portail = {
     url = "github:peterlodri-sec/portail/v0.1.0";
     inputs.nixpkgs.follows = "nixpkgs";
   };
   ```

## Rollback

```bash
git tag -d v0.1.0               # delete local tag
git push --delete origin v0.1.0 # delete remote tag
# GitHub Release: edit → unpublish
# crates.io: yank
cargo yank --version 0.1.0
```
