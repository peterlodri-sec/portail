# ── Portail dev workflows ──────────────────────────────────────────────
default: build

# ── Build ──────────────────────────────────────────────────────────────
build:
    cargo build

release:
    cargo build --release

check:
    cargo check --locked

# ── Test ───────────────────────────────────────────────────────────────
test:
    cargo test --locked

test-fast: nextest

nextest:
    cargo nextest run --locked

bench:
    cargo bench

# ── Lint ───────────────────────────────────────────────────────────────
lint: clippy format

clippy:
    cargo clippy --locked --all-targets -- -D warnings

format:
    cargo fmt --check

format-fix:
    cargo fmt

# ── Watch ──────────────────────────────────────────────────────────────
watch:
    cargo watch -x check -x test

# ── Security ───────────────────────────────────────────────────────────
audit:
    cargo audit

deny:
    cargo deny check

outdated:
    cargo outdated

# ── Clean ──────────────────────────────────────────────────────────────
clean:
    cargo clean

clean-release:
    cargo clean --release

# ── Full CI pipeline ───────────────────────────────────────────────────
ci: check lint test

# ── Nix ────────────────────────────────────────────────────────────────
nix-check:
    nix flake check --impulse

nix-build:
    nix build .#portail

# ── Docker ─────────────────────────────────────────────────────────────
docker-build:
    docker build -t portail:latest .

docker-run: docker-build
    docker run --rm -p 8787:8787 portail:latest

docker-slim:
    docker build -t portail:slim --target build -f Dockerfile .
    @echo "Binary size:"
    docker run --rm portail:slim ls -lh /build/target/release/portail

# ── Credentials ────────────────────────────────────────────────────────
login:
    cargo login --credential-provider cargo-credential-pass

publish-dry:
    cargo publish --dry-run

publish:
    cargo publish

# ── Release ────────────────────────────────────────────────────────────
release-dist: release
    upx --best --lzma target/release/portail
    sha256sum target/release/portail | tee target/release/portail.sha256

release-sign: release-dist
    cosign sign-blob --yes \
      --output-signature target/release/portail.sig \
      --output-certificate target/release/portail.pem \
      target/release/portail
    cosign sign-blob --yes \
      --output-signature target/release/portail.sha256.sig \
      --output-certificate target/release/portail.sha256.pem \
      target/release/portail.sha256

release-verify:
    cosign verify-blob \
      --cert target/release/portail.pem \
      --signature target/release/portail.sig \
      target/release/portail

# ── Help ───────────────────────────────────────────────────────────────
help:
    @echo "  build          cargo build (debug)"
    @echo "  release        cargo build --release (LTO fat)"
    @echo "  release-dist   build + UPX compress"
    @echo "  release-sign   build + UPX + cosign sign"
    @echo "  release-verify verify cosign signature"
    @echo "  test           cargo test"
    @echo "  test-fast      cargo nextest run"
    @echo "  bench          cargo bench (criterion)"
    @echo "  lint           clippy + fmt check"
    @echo "  clippy         cargo clippy -- -D warnings"
    @echo "  check          cargo check"
    @echo "  watch          cargo watch -x check -x test"
    @echo "  audit          cargo audit"
    @echo "  deny           cargo deny check"
    @echo "  ci             check + lint + test"
    @echo "  docker-build   build Docker image"
    @echo "  docker-run     build + run on :8787"
    @echo "  docker-slim    check compressed binary size"
    @echo "  login          setup crates.io credentials (cargo-credential-pass)"
    @echo "  publish-dry    cargo publish --dry-run"
    @echo "  publish        cargo publish to crates.io"
    @echo "  nix-check      nix flake check --impulse"
