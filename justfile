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

# ── Help ───────────────────────────────────────────────────────────────
help:
    @echo "  build          cargo build (debug)"
    @echo "  release        cargo build --release (LTO fat)"
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
    @echo "  nix-check      nix flake check --impulse"
