# ── Stage 1: Build with full toolchain ──────────────────────────────────
FROM rust:1.85-slim-bookworm AS build

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev upx-ucl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .

ENV CARGO_PROFILE_RELEASE_LTO=fat \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_PROFILE_RELEASE_STRIP=symbols \
    CARGO_PROFILE_RELEASE_DEBUG=false \
    PKG_CONFIG_ALLOW_CROSS=1

RUN cargo build --release --bin portail \
    && upx --best --lzma target/release/portail

# ── Stage 2: Distroless runtime ────────────────────────────────────────
FROM gcr.io/distroless/cc-debian12

COPY --from=build /build/target/release/portail /portail

EXPOSE 8787
USER 1000:1000

ENTRYPOINT ["/portail"]
