# Multi-stage build for ps12image_operator (Rust).
# Much lighter than the pamsoft_grid build: no OpenCV — the operator only
# reads TIFF *metadata* (pure-Rust `tiff` crate), never pixel data.

# ============================================================================
# Builder stage
# ============================================================================
FROM rust:1-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        git \
        pkg-config \
        protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Prime the dependency cache: dummy source stubs for every declared target
# let the first `cargo build` fetch and compile the heavy transitive stacks
# (tonic/polars via tercen-rs) into a Docker layer that only invalidates
# when Cargo.{toml,lock} change.
COPY Cargo.toml ./
RUN mkdir -p src/bin && \
    : > src/lib.rs && \
    echo 'fn main() {}' > src/main.rs && \
    echo 'fn main() {}' > src/bin/dev.rs && \
    cargo build --release && \
    rm -rf src && \
    cargo clean -p ps12image_operator --release
# `cargo clean -p` wipes all artifacts for the local crate so the second
# build re-derives them from the real sources (Cargo caches the empty-lib
# metadata otherwise).

COPY src ./src
RUN cargo build --release --bin ps12image_operator

# ============================================================================
# Runtime stage
# ============================================================================
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/ps12image_operator /usr/local/bin/ps12image_operator

WORKDIR /operator
ENV RUST_BACKTRACE=1

ENTRYPOINT ["/usr/local/bin/ps12image_operator"]
