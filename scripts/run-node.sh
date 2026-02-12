#!/usr/bin/env bash
set -euo pipefail

# Build runtime + node
cargo build -r -p solochain-eterra-runtime -p solochain-eterra-node

# Regenerate chain specs
rm -f chain-specs/testnet-plain.json chain-specs/testnet-raw.json

./target/release/solochain-eterra-node build-spec \
  --chain testnet > chain-specs/testnet-plain.json

./target/release/solochain-eterra-node build-spec \
  --chain chain-specs/testnet-plain.json --raw > chain-specs/testnet-raw.json

# Fresh local base path
BASE=./data/alice
rm -rf "$BASE"
mkdir -p "$BASE/chains/eterra_testnet/network"

# Generate libp2p key
./target/release/solochain-eterra-node key generate-node-key \
  --chain chain-specs/testnet-raw.json \
  --file "$BASE/chains/eterra_testnet/network/secret_ed25519"

# Insert AURA and GRANDPA keys
./target/release/solochain-eterra-node key insert \
  --base-path "$BASE" \
  --chain chain-specs/testnet-raw.json \
  --key-type aura \
  --scheme Sr25519 \
  --suri //Alice

./target/release/solochain-eterra-node key insert \
  --base-path "$BASE" \
  --chain chain-specs/testnet-raw.json \
  --key-type gran \
  --scheme Ed25519 \
  --suri //Alice

# Run the node
./target/release/solochain-eterra-node \
  --chain chain-specs/testnet-raw.json \
  --base-path "$BASE" \
  --validator --alice \
  --force-authoring \
  --port 30333 --rpc-port 9944 \
  --public-addr /ip4/127.0.0.1/tcp/30333 \
  --unsafe-rpc-external --rpc-cors all
