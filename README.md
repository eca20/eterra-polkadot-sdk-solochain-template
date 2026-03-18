# Runtime Upgrade Runbook

When runtime code changes (pallet logic, config constants, weights, `spec_version`, etc.), rebuild runtime/node and regenerate chain specs before restarting nodes.

## Hardened Deployment Commands

Use the deployment helper to keep local and CI command paths aligned:

```bash
# Full validation (build + specs + verification + smoke) for dev/testnet/prod chains
./scripts/deploy.sh pipeline-check default

# Same validation with production origin policy enabled
./scripts/deploy.sh pipeline-check production

# Try-runtime gate (compile + on_runtime_upgrade checks)
./scripts/deploy.sh try-runtime-check default
./scripts/deploy.sh try-runtime-check production

# Generate specs only (output: chain-specs/generated/<mode>)
./scripts/deploy.sh specs default
./scripts/deploy.sh specs production

# Strict validation for a finalized production plain spec (expects real keys + bootnodes + sudo key)
./scripts/deploy.sh verify-production chain-specs/production-plain.json

# Cross-stack smoke for a running local beta stack
CHAIN_RPC_URL=http://127.0.0.1:9944 \
MEDIA_BASE_URL=http://127.0.0.1:4000 \
WEB_BASE_URL=http://127.0.0.1:3000 \
CARD_ID=1 \
./scripts/smoke-controlled-beta.sh

# Finalize production plain/raw specs from generated baseline + overrides config
cp chain-specs/production-keys.example.json chain-specs/production-keys.json
# fills addresses from suris (supports "@/path/to/secret" entries)
./scripts/generate-production-overrides.py \
  --in chain-specs/production-keys.json \
  --out chain-specs/production-overrides.json
# accepts either sudo_suri or sudo_address (use sudo_address for multisig owner control)
./scripts/deploy.sh finalize-production-spec production chain-specs/production-overrides.json
# outputs: chain-specs/finalized/production/production-{plain,raw}.json

# Start local validator/full nodes (safe RPC defaults)
./scripts/run-node.sh default testnet release validator
./scripts/run-node.sh production production release full

# Reset local dev state (chain base-path + media/IPFS docker volumes)
./scripts/reset-local-dev-stack.sh --yes
./scripts/reset-local-dev-stack.sh --yes --media-only --restart-media

# Start a production validator (requires non-dev key suris)
AURA_SURI="<sr25519_suri>" GRAN_SURI="<ed25519_suri>" \
  ./scripts/run-node.sh production production release validator
```

Equivalent `make` shortcuts:

```bash
make deploy-check-default
make deploy-check-production
make deploy-try-runtime-default
make deploy-try-runtime-production
make deploy-verify-production SPEC=chain-specs/production-plain.json
make deploy-generate-production-overrides-production
make deploy-finalize-production-production PROD_CONFIG=chain-specs/production-overrides.json
make run-default-testnet
make run-production
make reset-local-dev RESET_ARGS="--yes --restart-media"
make help
```

## Rapid Dev Deploy To The 2010 Mac mini

The `deploy/macmini2010/` toolkit treats this MacBook as the source of truth and the 2010 mini as a runtime target for the dev node plus the IPFS media stack.

1. Copy `deploy/macmini2010.env.example` to `deploy/macmini2010.env` and fill in the LAN/SSH values.
2. Run `deploy/macmini2010/bootstrap.sh` once to prepare Docker, `systemd`, `/opt/eterra/...`, `/var/lib/eterra-node-dev`, and the LAN-only UFW rules on the mini.
3. Run `deploy/macmini2010/deploy-node.sh` to build the Linux `amd64` node artifact locally, ship `solochain-eterra-node` plus `dev-raw.json`, and restart `eterra-node.service`.
4. Run `deploy/macmini2010/deploy-media.sh` to `rsync` the sibling `eterra-ipfs-media-service` repo to `/opt/eterra/media/current` and rebuild the remote Docker Compose stack.
5. Use `deploy/macmini2010/deploy-all.sh`, `status.sh`, `logs.sh`, `reset-node.sh --yes`, and `reset-media.sh --yes` for the everyday loop.

`scripts/run-node.sh` defaults to exposed RPC on `dev`/`testnet` (so dockerized services can connect; validator uses `--unsafe-rpc-external` per Substrate CLI rules) and local-only RPC on `production` unless explicitly overridden.

`scripts/reset-local-dev-stack.sh` is intentionally destructive and requires `--yes`. By default it removes the local node base-path for `MODE=default CHAIN=dev ROLE=validator` and runs `docker compose down --volumes` for the sibling `eterra-ipfs-media-service` repo. Use `--media-only`, `--chain-only`, or `--restart-media` to narrow or streamline the reset flow.

Production override support now includes:

- `sudo_address` for multisig owner control
- `season_admin_suris` or `season_admin_addresses`
- `media_collection_owner_suri` or `media_collection_owner_address`

Strict production validation also checks that:

- `eterraSeasons.admins` is non-empty
- season admins are funded and not Alice/Bob placeholders
- `eterraMedia.defaultCollectionOwner` is funded and not a dev placeholder

For the operational release checklist, see `docs/beta-release-checklist.md`.

Faucet policy: faucet claim sponsorship is only for zero-balance accounts (`dest == signer`), capped per on-chain window; claims are also cooldown-limited on-chain.

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
