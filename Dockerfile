# -------------------------
# Builder stage
# -------------------------
FROM rust:1.85-bookworm AS builder

# Build deps for Substrate (kept minimal; extend if your pallets need more)
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang pkg-config libssl-dev cmake protobuf-compiler git \
 && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown

WORKDIR /code

# Cache deps
COPY Cargo.toml Cargo.lock ./
# if you have a Cargo workspace, copy the workspace Cargo.toml and members
COPY node/Cargo.toml node/Cargo.toml
COPY runtime/Cargo.toml runtime/Cargo.toml
# (optional) COPY other pallet crates' Cargo.toml if present

# Dummy build to warm the cache
RUN mkdir -p node/src runtime/src \
 && echo 'fn main(){}' > node/src/main.rs \
 && echo 'pub fn dummy(){}' > runtime/src/lib.rs \
 && cargo build -p solochain-eterra-node --release || true

# Real source
COPY . .

# Build the actual binary
RUN cargo build -p solochain-eterra-node --release

# -------------------------
# Runtime stage
# -------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates jq tini \
 && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN useradd -m -u 1000 eterra
USER eterra

# Folders
ENV BASE_PATH=/data \
    CHAIN_SPEC_DIR=/etc/eterra/chain-specs \
    RAW_SPEC=${CHAIN_SPEC_DIR}/testnet-raw.json \
    HUMAN_SPEC=${CHAIN_SPEC_DIR}/testnet.json

WORKDIR /home/eterra

# Copy binary and default chain-specs
COPY --from=builder /code/target/release/solochain-eterra-node /usr/local/bin/solochain-eterra-node
# If you keep chain-specs in repo, copy them in; you can also mount them at runtime
COPY --chown=eterra:eterra chain-specs ${CHAIN_SPEC_DIR}

# Entrypoint script
COPY --chown=eterra:eterra ./docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

EXPOSE 30333 9944
ENTRYPOINT ["/usr/bin/tini","--"]
CMD ["/usr/local/bin/entrypoint.sh"]
