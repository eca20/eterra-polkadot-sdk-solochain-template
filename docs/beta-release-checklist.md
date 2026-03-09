# Eterra Controlled-Beta Release Checklist

This checklist is the release gate for the current feature set. It assumes owner-controlled beta operation with a multisig root address.

## Chain Spec

- Generate production overrides with real validator keys and bootnodes.
- Use `sudo_address` for the owner multisig.
- Set explicit `season_admin_suris` / `season_admin_addresses`.
- Set `media_collection_owner_suri` / `media_collection_owner_address`.
- Run:

```bash
./scripts/generate-production-overrides.py \
  --in chain-specs/production-keys.json \
  --out chain-specs/production-overrides.json

./scripts/deploy.sh finalize-production-spec production chain-specs/production-overrides.json
./scripts/deploy.sh verify-production chain-specs/finalized/production/production-plain.json
```

## Runtime / Node

- Confirm `scripts/run-node.sh` keeps production RPC local-only by default.
- Confirm no Alice/Bob placeholder authorities, sudo, balances, season admins, or media owner remain.
- Confirm the production owner account is multisig-backed.

## Media Service

- Set `ADMIN_API_KEY`, `CHAIN_WS`, `IPFS_API`, `PUBLIC_BASE_URL`, and CORS/rate-limit envs.
- Check `GET /health/live`.
- Check `GET /health/ready`.
- Confirm `ffmpeg` is present in the runtime/container.
- Confirm public image/metadata endpoints are reachable through the reverse proxy.

## Web

- Set `PUBLIC_CHAIN_ENDPOINT`, `PUBLIC_MEDIA_BASE_URL`, `PUBLIC_IPFS_GATEWAY`.
- Set server-only admin envs:
  - `ADMIN_USERNAME`
  - `ADMIN_PASSWORD`
  - `ADMIN_SESSION_SECRET`
  - `MEDIA_ADMIN_API_KEY`
  - `INTERNAL_MEDIA_SERVICE_URL`
  - `INTERNAL_IPFS_API`
- Confirm `/admin/seasons` uses same-origin `/admin/api/*` calls and does not expose the media admin key in the browser.
- Confirm avatar upload works through `/api/avatar/upload`.

## End-to-End Functional Smoke

- Run `./scripts/smoke-controlled-beta.sh` against the live beta stack.
- Optionally run the manual GitHub Actions workflow `Controlled Beta Smoke` with the live stack URLs.
- Create or verify an active season.
- Create or verify a media collection.
- Upload at least one border, background, and subject.
- Mint a card and verify `CardArtwork` exists.
- Load the rendered card image from the media service.
- Convert a card to an NFT.
- Set on-chain NFT metadata through the admin proxy.
- Transfer and unwrap an NFT.
- Verify PvE and PvP still work.

## Release Artifacts

- Record finalized `production-plain.json` and `production-raw.json`.
- Record node binary checksum and runtime wasm checksum.
- Store bootstrap/admin secrets and chain-spec artifacts in backed-up operator storage.
