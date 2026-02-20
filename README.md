# Runtime Upgrade Runbook

When runtime code changes (pallet logic, config constants, weights, `spec_version`, etc.), rebuild runtime/node and regenerate chain specs before restarting nodes.

## Hardened Deployment Commands

Use the deployment helper to keep local and CI command paths aligned:

```bash
# Full validation (build + specs + verification + smoke) for dev/testnet/prod chains
./scripts/deploy.sh pipeline-check default

# Same validation with production origin policy enabled
./scripts/deploy.sh pipeline-check production

# Generate specs only (output: chain-specs/generated/<mode>)
./scripts/deploy.sh specs default
./scripts/deploy.sh specs production

# Strict validation for a finalized production plain spec (expects real keys + bootnodes + sudo key)
./scripts/deploy.sh verify-production chain-specs/production-plain.json

# Finalize production plain/raw specs from generated baseline + overrides config
cp chain-specs/production-keys.example.json chain-specs/production-keys.json
# fills addresses from suris (supports "@/path/to/secret" entries)
./scripts/generate-production-overrides.py \
  --in chain-specs/production-keys.json \
  --out chain-specs/production-overrides.json
./scripts/deploy.sh finalize-production-spec production chain-specs/production-overrides.json
# outputs: chain-specs/finalized/production/production-{plain,raw}.json

# Start local validator/full nodes (safe RPC defaults)
./scripts/run-node.sh default testnet release validator
./scripts/run-node.sh production production release full

# Start a production validator (requires non-dev key suris)
AURA_SURI="<sr25519_suri>" GRAN_SURI="<ed25519_suri>" \
  ./scripts/run-node.sh production production release validator
```

Equivalent `make` shortcuts:

```bash
make deploy-check-default
make deploy-check-production
make deploy-verify-production SPEC=chain-specs/production-plain.json
make deploy-generate-production-overrides-production
make deploy-finalize-production-production PROD_CONFIG=chain-specs/production-overrides.json
make run-default-testnet
make run-production
make help
```

`scripts/run-node.sh` defaults to local-only RPC (`EXPOSE_RPC=0`) and blocks unsafe RPC on production unless explicitly overridden.

## 1) Build

### Testnet/default mode

```bash
cargo build -r -p solochain-eterra-runtime -p solochain-eterra-node
```

### Production origin-policy mode

```bash
cargo build -r -p solochain-eterra-runtime -p solochain-eterra-node \
  --features runtime-production
```

## 2) Regenerate Chain Specs

```bash
mkdir -p chain-specs
rm -f chain-specs/testnet-plain.json chain-specs/testnet-raw.json
rm -f chain-specs/production-plain.json chain-specs/production-raw.json
```

### Testnet

```bash
./target/release/solochain-eterra-node build-spec \
  --chain testnet > chain-specs/testnet-plain.json

./target/release/solochain-eterra-node build-spec \
  --chain chain-specs/testnet-plain.json --raw > chain-specs/testnet-raw.json
```

### Production

```bash
./target/release/solochain-eterra-node build-spec \
  --chain production > chain-specs/production-plain.json

./target/release/solochain-eterra-node build-spec \
  --chain chain-specs/production-plain.json --raw > chain-specs/production-raw.json
```

Note: built-in `production` config is a baseline template. Replace authority keys, balances, bootnodes, and the sudo owner key before real deployment.

## 3) Optional: Purge Local Testnet DB

```bash
BASE=/var/lib/eterra-testnet/alice
rm -rf "$BASE"
sudo rm -rf /var/lib/eterra-testnet
sudo mkdir -p /var/lib/eterra-testnet/alice
sudo chown -R "$USER":staff /var/lib/eterra-testnet
```

## 4) Optional: Insert Dev Keys (Alice)

```bash
BASE=/var/lib/eterra-testnet/alice

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
```

## 5) Start Node

### Testnet

```bash
BASE=/var/lib/eterra-testnet/alice

./target/release/solochain-eterra-node \
  --chain chain-specs/testnet-raw.json \
  --base-path "$BASE" \
  --validator --alice \
  --force-authoring \
  --port 30333 --rpc-port 9944 \
  --public-addr /ip4/127.0.0.1/tcp/30333 \
  --unsafe-rpc-external --rpc-cors all
```

### Production policy mode

```bash
./target/release/solochain-eterra-node \
  --chain chain-specs/production-raw.json \
  --base-path /var/lib/eterra-production/node1 \
  --validator \
  --port 30333 --rpc-port 9944
```

## 6) Verify Runtime Version

```bash
curl -H "Content-Type: application/json" \
  -d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[]}' \
  http://127.0.0.1:9944
```

Expected response includes:

```text
"specName":"solochain-eterra-runtime"
"specVersion":<new version>
```

## Troubleshooting

- Rebuild before regenerating specs.
- Ensure the node uses the correct binary (`./target/release/solochain-eterra-node`).
- If version does not update, verify old process/database/spec artifacts are not being reused.
