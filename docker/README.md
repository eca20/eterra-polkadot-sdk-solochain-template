# Eterra Node Docker Guide

This guide matches the current Docker assets in this repository:
- `Dockerfile` (at repository root)
- `docker/entrypoint.sh`

## Build Image

Run from repository root:

```bash
docker build -t solochain-eterra-node -f Dockerfile .
```

## Default Run (testnet raw spec in image)

```bash
docker run -d \
  --name eterra-node \
  -p 30333:30333 \
  -p 9944:9944 \
  -v eterra-node-data:/data \
  solochain-eterra-node
```

The entrypoint starts:
- `solochain-eterra-node --chain <RAW_SPEC> --base-path <BASE_PATH> ...`
- validator mode enabled by default (`--validator --force-authoring`)

## Chain Spec Behavior

Environment defaults:
- `BASE_PATH=/data`
- `CHAIN_SPEC_DIR=/etc/eterra/chain-specs`
- `RAW_SPEC=/etc/eterra/chain-specs/testnet-raw.json`
- `HUMAN_SPEC=/etc/eterra/chain-specs/testnet.json`

If `RAW_SPEC` is missing and `HUMAN_SPEC` exists, entrypoint auto-generates RAW from HUMAN.

## Run With Custom Specs

Mount custom spec files and point env vars explicitly:

```bash
docker run -d \
  --name eterra-node \
  -p 30333:30333 \
  -p 9944:9944 \
  -v eterra-node-data:/data \
  -v $(pwd)/chain-specs:/etc/eterra/chain-specs:ro \
  -e RAW_SPEC=/etc/eterra/chain-specs/production-raw.json \
  -e HUMAN_SPEC=/etc/eterra/chain-specs/production.json \
  solochain-eterra-node
```

## Optional Environment Variables

- `P2P_PORT` (default `30333`)
- `RPC_PORT` (default `9944`)
- `PUBLIC_ADDR` (default `/ip4/0.0.0.0/tcp/${P2P_PORT}`)
- `VALIDATOR` (`true`/`false`, default `true`)
- `INSERT_KEYS` (`true`/`false`, default `false`)
- `AURA_SURI` (used when `INSERT_KEYS=true`, default `//Alice`)
- `GRANDPA_SURI` (used when `INSERT_KEYS=true`, default `//Alice`)
- `NODE_KEY_HEX` (optional libp2p key)
- `NODE_KEY_FILE` (optional libp2p key file)
- `EXTRA_ARGS` (extra node CLI args)

## Insert Dev Keys On Startup

```bash
docker run -d \
  --name eterra-node \
  -p 30333:30333 \
  -p 9944:9944 \
  -v eterra-node-data:/data \
  -e INSERT_KEYS=true \
  -e AURA_SURI=//Alice \
  -e GRANDPA_SURI=//Alice \
  solochain-eterra-node
```

## Backup / Restore

Backup:

```bash
docker run --rm \
  -v eterra-node-data:/data \
  -v $(pwd):/backup \
  alpine \
  tar czf /backup/eterra-node-data-backup.tar.gz -C /data .
```

Restore:

```bash
docker run --rm \
  -v eterra-node-data:/data \
  -v $(pwd):/backup \
  alpine \
  sh -c "rm -rf /data/* && tar xzf /backup/eterra-node-data-backup.tar.gz -C /data"
```
