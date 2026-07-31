# Nexus V2 private-alpha safety tooling

`alpha_v2_release.py` builds evidence for a future private-alpha V2 reset. It
does not reset, deploy, connect to live RPC, or ship a live-operation driver.
`verify_reset_readiness.py` is the dependency-free closed-schema verifier used
by the separately guarded deployment scripts; it accepts only the exact
SHA-256-pinned pre-V16 frozen/fresh-genesis packet.

## Relationship to current Alpha scripts

The final capture is coordinated by `final_freeze.py`; the older
`backup-alpha-state.sh` remains a preliminary chain/media helper and is not a
complete or frozen V2 backup on its own. The coordinator hash-pins five drivers
and stops Caddy/public write ingress, site/indexer/Mongo, authority, node/block
production, and media/IPFS in that order. A partial failure never restarts a
service.

Copy its output into a private bundle and supplement it with:

- node binary, exact stopped node-data archive, live V14 Wasm/metadata, V16
  runtime Wasm, finalized TCG storage-version observation, and the newly
  created exact-block try-runtime snapshot plus its provenance proof;
- media state and immutable image lock/digest;
- site/indexer/Mongo state and finalized checkpoint;
- authority state and Caddy state/configuration;
- all node, media, authority, site, and indexer configs and service definitions;
- chain spec, write-barrier evidence, exact release identifiers, the generated
  same-block pre-V16 economic gates, and zero V2 acceptance inventory.

The manifest records the SHA-256 of the current backup, restore, reset, and
runtime-rehearsal scripts. This is coordination by identity only: the tool
never invokes those live helpers.

The local runtime-bundle builder starts its staged production node, reads the
node's actual embedded `:code` with
`state_getStorage(0x3a636f6465)`, decodes it, and refuses the bundle unless its
SHA-256 equals the staged compact-compressed production Wasm. The bundle
manifest records both hashes. The try-runtime Wasm remains a separate artifact
and role; it is never accepted as the deployable production Wasm.

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
  V14 snapshot, then a pinned verifier that drives the bounded migration to
  completion and emits the structured V14-to-V16 result. The V16 runtime also
  accepts an undeployed V15 source for compatibility, but evidence for the
  current Alpha must record its observed V14 source truth.
- `prepare-reset` requires passing restore/migration evidence, exact disabled
  economy gates, and a zero-asset inventory from one finalized block. For the
  current pre-V16 Alpha it also accepts the distinct, fresh-reset-only
  `nexus-v2-private-alpha-pre-v16-fresh-reset-gates` contract after every write
  ingress and block production path is stopped. It emits readiness evidence
  and performs no reset or deploy.
- The Mac mini node/media deployment scripts retain their ordinary release
  reset prohibitions. Their sole exception is an explicit
  `--fresh-reset-readiness` packet whose SHA-256 matches
  `NEXUS_V2_RESET_READINESS_SHA256`; release media reset also requires
  immutable candidate promotion. `--dry-run` validates this local plan and
  exits before SSH.
- `build-linux-amd64-node.sh` builds the deployment ELF with Rust 1.89, locked
  dependencies, a digest-pinned bookworm image, BuildKit `linux/amd64`, and the
  source commit's `SOURCE_DATE_EPOCH`; it emits a closed build attestation and
  never publishes an image or package.
- `node_candidate.py build` consumes the runtime bundle, address-only Alpha
  overrides, and attested Linux/x86-64 node. It repeats spec generation, then
  executes that ELF under the network-disabled digest-pinned runner to prove
  identical raw state and embedded runtime. It emits a closed candidate and
  `eterra-spec106-target-identity.v2` with an Ubuntu 24.04 x86-64 contract.
- `frozen_snapshot_proof.py` creates/verifies the exact stopped-block provenance
  required beside every final-freeze try-runtime snapshot.
- `final_freeze.py validate|dry-run|execute` owns the cross-host stop/snapshot
  order. Every component driver and plan is SHA-256 pinned. Execute creates the
  same-block pre-V16 gates and zero inventory only after the stable write
  barrier, then emits the complete backup manifest.
- `automatic-rollback` invokes a separately reviewed, approval-hash-pinned
  driver only while a fresh finalized inventory still contains no V2
  acceptance asset. Monotonic lifetime event counters keep this gate closed
  after an asset is later consumed or burned. Any nonzero count writes a
  blocked decision and never invokes the driver. Pre-V16 fresh-reset gates are
  never accepted for rollback; rollback requires post-V16 disabled-state
  gates from the replacement chain.
- `deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py` closes the
  cross-host decision/orchestration gap. It revalidates the exact final
  backup and restore rehearsal, requires a fresh post-V16 same-block gate and
  inventory observation behind a stable all-V2-write barrier, pins distinct
  clean chain/media/site commits, dry-runs every action adapter, archives both
  failed V2 roots before restoring either host, and writes restart-safe
  immutable phase markers. See
  `deploy/alpha/macmini2010/nexus-v2-post-cutover-rollback.md`.

Required artifact roles:

```text
node:node-data
node:node-binary
node:runtime-v14-wasm
node:runtime-v14-metadata
node:runtime-v16-production-wasm
node:runtime-v16-try-runtime-wasm
node:tcg-storage-version-observation
node:try-runtime-snapshot
node:try-runtime-snapshot-proof
media:media-state
media:media-image-lock
ipfs:ipfs-data
ipfs:ipfs-staging
config:node-env
config:media-env
config:indexer-env
config:site-env
config:authority-env
config:chain-spec
config:economic-gates
config:acceptance-inventory
config:deployment-fingerprints
config:release-identifiers
config:write-barrier-evidence
service:node-service
service:media-service
service:indexer-service
service:site-service
service:authority-service
service:caddy-service
indexer:indexer-state
indexer:indexer-checkpoint
indexer:mongo-state
site:site-state
site:site-image-lock
authority:authority-state
ingress:caddy-state
ingress:caddy-config
```

Each `backup-manifest --artifact` value is
`GROUP:NAME:PATH_RELATIVE_TO_BUNDLE`.

## Driver boundaries

No restore, migration-completion, or rollback driver is bundled. Final-freeze
drivers are a separate, explicit exception. A restore/migration/rollback driver:

- must be a regular executable outside the existing `deploy/` tree;
- is invoked without a shell;
- is SHA-256 recorded, and rollback additionally requires an unexpired approval
  that pins the driver and readiness hashes;
- must write the corresponding result JSON contract shown in
  `docs/nexus-v2-private-alpha/`;
- must never reuse live ports for an isolated rehearsal.

That legacy statement applies to actions directly invoked by
`alpha_v2_release.py`. The post-cutover coordinator is separately bundled
under `deploy/`; it contains no SSH/RPC/Docker implementation and owns no
credential material. Its component adapters are exact hash-pinned release
artifacts in clean worktrees. Fixture is their default backend. Both adapters
also have explicit-confirmation protected backends. Chain/media delegates to
the fixed helper in the pinned chain commit; site/indexer imports that exact
shared receipt contract and delegates to the fixed helper in the pinned site
commit. Both must prove through closed receipts that the pinned existing
restore/deploy/status scripts were used. See
`deploy/alpha/macmini2010/nexus-v2-post-cutover-rollback.md` for the exact
selection token, paths, ports, staging contract, post-acceptance stop, and the
final clean-commit/helper/image pins required before protected cross-host
execution. Protected and fixture receipts must never be mixed.

Configuration archives can contain secrets. Keep the bundle and all evidence
private with restrictive filesystem access.
