# Nexus V2 private-alpha safety tooling

`alpha_v2_release.py` builds evidence for a future private-alpha V2 reset. It
does not reset, deploy, connect to live RPC, or ship a live-operation driver.

## Relationship to current Alpha scripts

Start a future capture with
`deploy/alpha/macmini2010/backup-alpha-state.sh`. That existing helper supplies
the node database, IPFS data/staging, and node/media environment files. It is
not a complete V2 backup on its own.

Copy its output into a private bundle and supplement it with:

- node binary, V15/V16 runtime Wasm, and copied try-runtime snapshot;
- media state and immutable image lock/digest;
- indexer state and finalized checkpoint;
- node, media, and indexer configs;
- chain spec and the finalized economic-gate observation;
- node, media, and indexer service definitions.

The manifest records the SHA-256 of the current backup, restore, reset, and
runtime-rehearsal scripts. This is coordination by identity only: the tool
never invokes those live helpers.

## Commands

Run `./scripts/nexus-v2-private-alpha/alpha_v2_release.py COMMAND --help` for
the complete arguments.

- `backup-manifest` requires the exact closed artifact-role set printed below
  and hashes every file.
- `verify-backup` rehashes the complete private bundle and rejects tool drift.
- `init-isolation-root` creates a sentinel root whose name starts with
  `nexus-v2-isolated-restore-`.
- `rehearse-restore` invokes a separately reviewed restore driver on loopback
  ports disjoint from all declared live ports.
- `rehearse-migration` invokes a pinned try-runtime binary against the copied
  snapshot, then a pinned verifier that drives the bounded migration to
  completion and emits the structured V15-to-V16 result.
- `prepare-reset` requires passing restore/migration evidence, exact disabled
  economy gates, and a zero-asset inventory from one finalized block. It emits
  readiness evidence and performs no reset or deploy.
- `automatic-rollback` invokes a separately reviewed, approval-hash-pinned
  driver only while a fresh finalized inventory still contains no V2
  acceptance asset. Monotonic lifetime event counters keep this gate closed
  after an asset is later consumed or burned. Any nonzero count writes a
  blocked decision and never invokes the driver.

Required artifact roles:

```text
node:node-data
node:node-binary
node:runtime-v15-wasm
node:runtime-v16-wasm
node:try-runtime-snapshot
media:media-state
media:media-image-lock
ipfs:ipfs-data
ipfs:ipfs-staging
config:node-env
config:media-env
config:indexer-env
config:chain-spec
config:economic-gates
service:node-service
service:media-service
service:indexer-service
indexer:indexer-state
indexer:indexer-checkpoint
```

Each `backup-manifest --artifact` value is
`GROUP:NAME:PATH_RELATIVE_TO_BUNDLE`.

## Driver boundary

No restore, migration-completion, or rollback driver is bundled. A future
driver:

- must be a regular executable outside the existing `deploy/` tree;
- is invoked without a shell;
- is SHA-256 recorded, and rollback additionally requires an unexpired approval
  that pins the driver and readiness hashes;
- must write the corresponding result JSON contract shown in
  `docs/nexus-v2-private-alpha/`;
- must never reuse live ports for an isolated rehearsal.

Configuration archives can contain secrets. Keep the bundle and all evidence
private with restrictive filesystem access.
