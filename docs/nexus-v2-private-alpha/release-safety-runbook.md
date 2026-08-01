# Nexus V2 private-alpha release safety runbook

Status: **guarded local-only release tooling implemented; no backup, restore,
reset, rollback, deploy, live connection, or runtime upgrade has been executed
by this change**.

This packet prepares a future private-alpha reset while preserving the Nexus
authority model. Runtime state remains authoritative; IPFS and indexers are
backed up as media/read-model systems, not treated as the source of live game
state.

The reviews this autonomous private-alpha run cannot self-approve are recorded
in `external-reviews.pending.json`. Every listed public, production, paid,
marketplace, wagering, package-publication, and final-art capability remains
blocked.

## Hard gates

A reset is not ready until one packet contains all of the following:

1. A hash-complete private backup covering node, media, IPFS, config, service,
   and indexer artifacts.
2. A successful full restore in a sentinel directory, with every service bound
   to loopback ports that are unique and disjoint from declared live ports.
3. Pinned copied-state V14-to-V16 try-runtime evidence plus a completion
   verifier proving the bounded `on_idle` migration resumes safely and
   completes.
4. One finalized-block observation proving either that every V16 economic
   surface is disabled, or that the exact pre-V16 source is frozen behind a
   complete write-ingress stop for a fresh-genesis replacement only.
5. An acceptance inventory from that same finalized block with every current
   and lifetime V2 and legacy game-authority acceptance count equal to zero.

`prepare-reset` only emits a readiness document. A separate operator approval
and separately reviewed live procedure remain required.

## Capture packet

The old helper remains useful only for a preliminary, recoverable capture:

```bash
./deploy/alpha/macmini2010/backup-alpha-state.sh \
  nexus-v2-pre-reset-YYYYMMDDTHHMMSSZ
```

Do not represent that preliminary capture as the final frozen state. The final
capture uses the SHA-256-pinned `final_freeze.py` plan described below. Do not
use `restore-alpha-state.sh`, `reset-node.sh`, `reset-media.sh`,
`deploy-all.sh --fresh`, or any purge option at this stage.

### Linux-authoritative runtime bundle

The macOS-built spec-106 bundle is superseded because absolute host source
paths made its compact Wasm target-specific. It remains input evidence only:
its V14 recovery Wasm, TCG V14 observation, host try-runtime CLI, host migration
verifier, pending-review record, and Metadata V15 compatibility baseline are
preserved with their original source commit and hashes. Its production Wasm is
never copied into the replacement bundle or accepted as a release target.

Assemble the replacement from the closed, checksum-verified Linux build root.
The assembler rechecks the build attestation against its source commit, builds
the try-runtime Wasm and Linux migration verifier from an exact Git archive in
the digest-pinned Linux image, and runs the ELF64/x86-64 node only through a
network-disabled container with a read-only root filesystem and a private,
ephemeral writable evidence workspace. The derived dev spec, embedded
`:code`, genesis, runtime version, Metadata V15 SCALE, and decoded metadata JSON
are checksummed. Both metadata files must be byte-identical to the prior
baseline; a structural-only comparison is insufficient.

```bash
./scripts/release/assemble-nexus-v2-linux-runtime-bundle.py \
  --linux-build-root /private/path/to/linux-amd64-node-SOURCE \
  --prior-runtime-bundle /private/path/to/superseded-macos-runtime-bundle \
  --source-commit 40_HEX_LINUX_BUILD_SOURCE \
  --expected-production-wasm-sha256 64_HEX_LINUX_WASM \
  --expected-superseded-wasm-sha256 64_HEX_MACOS_WASM \
  --expected-metadata-scale-sha256 64_HEX_METADATA_SCALE \
  --expected-metadata-json-sha256 64_HEX_METADATA_JSON \
  --try-runtime-revision 40_HEX_REVISION \
  --subxt-bin /reviewed/path/subxt \
  --subxt-sha256 64_HEX_SUBXT \
  --output /private/path/to/runtime-release-spec106-linux
```

The command has no live RPC input and refuses a dirty assembly-tool worktree,
an unpinned image/tool, a non-x86-64 build, a changed runtime source surface,
the superseded production Wasm, or any metadata difference.

The frozen result is runtime manifest
`79359a961d065bd189f9b585cd57d339b6f58d8855b4d1d156c03b3e0b3feb5c`
with closed `SHA256SUMS`
`e983dc09310e737c0e9e5cc3ba067a336da877c72db42c335e9b39316be2aace`.
It binds runtime source `7338beff0c99ef72db43a6908f7bee07a181b50b`,
production Wasm `0b0c7c52b38ea880fa626784846164752aa256b9f30d83ed0b45d25277f38243`,
Metadata V15 SCALE `26ed50d186a0cb134cb8ef6b9f619cd04195b52cf4d06fb3f2c31050b103ee68`,
and the Linux probe/spec-builder node. That node is bundle evidence only; it is
not the final deployment node, which must be rebuilt and attested from the
eventual final chain commit.

### Immutable node candidate and target identity

Build the replacement candidate from the Linux-authoritative runtime bundle in
a clean exact deployment worktree. The private override file is a closed,
address-only JSON document; seed phrases and secret URIs are rejected and are
never copied into either output:

```bash
CHAIN_COMMIT="$(git rev-parse HEAD)"
./scripts/release/build-linux-amd64-node.sh \
  --source-commit "${CHAIN_COMMIT}" \
  --expected-runtime-wasm-sha256 PINNED_PRODUCTION_WASM_SHA256 \
  --output /private/path/to/linux-amd64-node
python3 scripts/nexus-v2-private-alpha/node_candidate.py build \
  --runtime-bundle /private/path/to/runtime-release-spec106-linux-7338beff0c99 \
  --deployment-node /private/path/to/linux-amd64-node/solochain-eterra-node \
  --deployment-node-attestation /private/path/to/linux-amd64-node/deployment-node-attestation.json \
  --deployment-node-source-commit "${CHAIN_COMMIT}" \
  --public-overrides /private/path/to/alpha-overrides.json \
  --release-id nexus-v2-private-alpha-RELEASE \
  --deployment-source-commit "${CHAIN_COMMIT}" \
  --output /private/path/to/node-candidate-spec106 \
  --target-identity-output /private/path/to/eterra-spec106-target-identity.v2.json
```

The deployment build uses the digest-pinned Rust 1.89 bookworm image, BuildKit
`linux/amd64`, locked dependencies, and the source commit's epoch. The candidate
builder verifies every runtime-bundle checksum and the ELF64/x86-64 header,
then executes the ELF under the pinned network-disabled runner to regenerate
the raw Alpha spec. Its full raw state and embedded `:code` must equal the
native builder output and pinned production Wasm. The separate
`eterra-spec106-target-identity.v2` binds that build attestation, runner,
Ubuntu 24.04 x86-64 host contract, release, genesis, runtime code,
runtime/deployment source commits, metadata V15, spec `106`, TCG storage `16`,
and candidate hash with paid/public activation false. Deployment rechecks
`uname`, Ubuntu version, ELF machine, and candidate `--version` before install.

### Atomic final freeze

Populate `final-freeze-plan.example.json` with exact clean component commits and
SHA-256-pinned drivers. Its web value remains a deliberate placeholder until
the final reviewed site-driver commit exists. The site driver must implement
both `site-ingress` and `site-indexer-mongo`; the bundled chain driver implements
`authority`, `chain`, and `media-ipfs`. The coordinator passes the global chain
deployment source plus a separate closed component source: authority uses
SDKGen, chain uses chain, media/IPFS uses media, and both site roles use web.
Every driver validates that component pin against its protected environment.

```bash
PLAN=/private/path/to/final-freeze-plan.json
PLAN_SHA256="$(shasum -a 256 "${PLAN}" | awk '{print $1}')"

python3 scripts/nexus-v2-private-alpha/final_freeze.py validate \
  --plan "${PLAN}" --expected-plan-sha256 "${PLAN_SHA256}"
python3 scripts/nexus-v2-private-alpha/final_freeze.py dry-run \
  --plan "${PLAN}" --expected-plan-sha256 "${PLAN_SHA256}" \
  --bundle-root /private/path/to/final-freeze-dry-bundle \
  --state-root /private/path/to/final-freeze-dry-state \
  --evidence /private/path/to/final-freeze-dry-run.json
```

Only after reviewing the dry-run, use `execute` with new empty bundle/state
paths. It stops Caddy/public ingress, site/indexer/Mongo, authority, node/RPC/P2P
and block production, then media/IPFS. After a stable 30-second minimum barrier,
it snapshots every role, creates same-block pre-V16 gates and zero inventory,
and writes the complete backup manifest. It never resumes a component on
failure. The stopped state is then used for isolated restore and copied-state
migration rehearsal before any reset.

Copy the current backup into an access-controlled bundle and add the required
supplemental artifacts listed in
`scripts/nexus-v2-private-alpha/README.md`. Dumps must be produced by the
owning service's consistent snapshot mechanism. Record the indexer's finalized
chain checkpoint alongside its database dump.

Create the manifest by supplying every exact artifact role:

```bash
./scripts/nexus-v2-private-alpha/alpha_v2_release.py backup-manifest \
  --bundle-root /private/path/to/bundle \
  --release-id nexus-v2-private-alpha-YYYYMMDD \
  --source-commit 40_LOWERCASE_HEX_COMMIT \
  --artifact node:node-data:node/node-data.tar.gz \
  ...all required roles... \
  --output /private/path/to/bundle/backup-manifest.json
```

The manifest refuses symlinks, traversal, missing roles, extra roles, and
artifact drift. It also pins the current Alpha helper scripts so an operator
cannot silently mix procedures from different revisions.

## Full isolated restore

Create a fresh temporary directory whose basename begins with
`nexus-v2-isolated-restore-`, then initialize its sentinel:

```bash
./scripts/nexus-v2-private-alpha/alpha_v2_release.py init-isolation-root \
  --root /private/tmp/nexus-v2-isolated-restore-RELEASE \
  --release-id nexus-v2-private-alpha-YYYYMMDD
```

Copy and review `isolated-ports.example.json`. The bind host must be
`127.0.0.1` or `::1`; node RPC/P2P, media, IPFS API/gateway, and indexer ports
must be unique and must not overlap any listed live port.

The independently reviewed restore driver must:

- extract/restore only below the sentinel root;
- load copied node, media, IPFS, configuration, service, and indexer state;
- verify the input hashes before use;
- start all copied services only on the supplied loopback ports;
- validate node RPC/state, media state/health, IPFS repository/gateway, config
  and service definitions, indexer state/checkpoint/health, and one
  cross-service finalized read;
- stop the isolated services and report teardown completion;
- report `liveAlphaTouched: false`.

Run:

```bash
./scripts/nexus-v2-private-alpha/alpha_v2_release.py rehearse-restore \
  --manifest /private/path/to/bundle/backup-manifest.json \
  --bundle-root /private/path/to/bundle \
  --isolation-root /private/tmp/nexus-v2-isolated-restore-RELEASE \
  --ports /private/path/to/isolated-ports.json \
  --driver /reviewed/path/isolated-restore-driver \
  --evidence /private/path/to/evidence/restore.json
```

The tool records the manifest, driver, port plan, log, and result hashes.

## V14-to-V16 copied-state rehearsal

The live Alpha TCG storage-version key is V14 at the pinned finalized backup
block. The V15 source bump added only new Prize Pool storage/call definitions
and had no migration hook, so it never changed that on-chain key. The V16
runtime accepts both V14 and V15 sources, but this copied-state packet must
truthfully prove V14-to-V16.

The existing `scripts/release/rehearse-runtime-upgrade.sh` remains the
spec-103-to-104 current release procedure. It is hash-pinned in the packet but
does not replace V2 migration evidence.

The rehearsal command never accepts a live RPC URI. It uses the
`node:legacy-source-inventory`, `node:try-runtime-snapshot`,
`node:try-runtime-snapshot-proof`, and `node:runtime-v16-try-runtime-wasm`
files already pinned in the backup manifest. The final-freeze driver creates
the source inventory through its loopback tunnel to the isolated stopped-state
copy; the inventory proves V14 storage, `NextCardId`, and the complete legacy
`Cards` key set/count/maximum ID at the snapshot's exact finalized block. The
proof must bind the snapshot to that block and stopped node-data archive. The separate
`node:runtime-v16-production-wasm` role pins the deployable Wasm and must not
be substituted for the migration-test build:

```bash
./scripts/nexus-v2-private-alpha/alpha_v2_release.py rehearse-migration \
  --manifest /private/path/to/bundle/backup-manifest.json \
  --bundle-root /private/path/to/bundle \
  --try-runtime-bin /reviewed/path/try-runtime \
  --try-runtime-revision PINNED_GIT_REVISION \
  --try-runtime-sha256 64_HEX_SHA256 \
  --migration-verifier /reviewed/path/v16-completion-verifier \
  --migration-verifier-sha256 64_HEX_SHA256 \
  --evidence /private/path/to/evidence/v14-v16.json
```

The first phase uses try-runtime `fast-forward --run-migrations` to execute
`on_runtime_upgrade`. By default the tool supplies
`max(1, ceil(NextCardId / 100))` empty blocks, derived from the hash-pinned
source inventory and runtime batch limit. `--migration-blocks` may add margin,
but a value below that deterministic minimum hard-stops before try-runtime is
invoked. The runtime completion marker is mandatory. Because the TCG repair is bounded and
multi-block, the second verifier independently drives `on_idle` through
completion, interrupts and resumes one run, and emits the exact structured
result in `migration-result.example.json`.

The result reconciles all ordinary, NFT-wrapped, known-escrow, and anomalous
cards; proves no loss, duplication, or silent reclassification; verifies
ownership/subject indexes and `NextCardId`; preserves safe legacy exits; seals
legacy creation; and keeps `Packs`, `Conversion`, and `Ranked` false.

### GameResults V16 storage assumption

`EterraGameResults` is introduced at pallet index `38` with pallet storage
version `1`. Its V16 session-authority counters and replay-receipt indexes do
not include a backfill migration because the approved source states do not
contain this pallet:

- the copied pre-V16 Alpha state structurally lacks sidecar pallets `35`
  through `38`; and
- the replacement V16 Alpha starts from fresh genesis with no sessions,
  authorization receipts, result epochs, locks, or reserved reward liability.

The copied-state verifier must therefore confirm that the complete
`EterraGameResults` storage prefix is absent from the pre-V16 snapshot. The
fresh-genesis inspection may observe FRAME's expected pallet storage-version
marker, but must find no session, receipt, result-epoch, lock, liability, or
active-counter state. Any source state containing one of those GameResults
records is a hard stop. It requires a separately reviewed storage-version bump
and migration/backfill; the empty-state assumption must never be applied to an
already-used GameResults deployment.

## Pre-V16 fresh-reset freeze

The current Alpha source is runtime spec `1` with V14 metadata and
`EterraTCG` storage version `14`. It does not contain the V16 seal or V2
sidecar pallets, so it is incorrect to claim that
`LegacyCreationSealedV16` is true or that V2 storage flags were read as false.

For a fresh-genesis replacement, populate
`pre-v16-fresh-reset-gates.example.json` only after all write paths are
stopped. This is a separate, mutually exclusive gate kind. It must record:

- the deployed source commit, spec `1`, metadata V14, TCG index `9`, TCG
  storage V14, Flow index `29`, and the pinned runtime/metadata/TCG-observation
  hashes;
- structural absence of the V2 TCG interfaces and sidecar pallets at indices
  `35` through `38`;
- the truthful presence of legacy paid mint, marketplace, faucet, economy,
  and pay-continue dispatchables;
- `AllIngressStopped`, including stopped node, authority, public RPC, P2P, and
  block production, plus an offline finalized-head check and a minimum
  30-second stability window; and
- fresh-genesis-only scope with in-place upgrade, V2 activation, and
  paid/public activation all forbidden.

Capture a truthful acceptance inventory from that frozen state. Every V2 field
must be zero because those interfaces are structurally absent; legacy
game-authority counters may be nonzero and are preserved as source-Alpha
history. Those legacy values do not cancel this backup-protected pre-V16
fresh-genesis replacement, but the same fields are restore-blocking after V2
cutover. The gate, inventory, and pinned TCG storage-version observation must
identify the same finalized block. If Alpha advanced after an earlier backup, take a final
recoverable backup and copied-state snapshot at this frozen boundary and
repeat restore/migration evidence before reset. Do not represent an older
backup as the frozen current state.

The pre-V16 gate can authorize only `prepare-reset`. It cannot authorize an
in-place runtime upgrade, catalog activation, or automatic rollback. The
readiness packet records `resetMode: fresh-genesis-replacement` and
`inPlaceRuntimeActivationAuthorized: false`.

## Post-V16 economic gate observation

Start from `economic-gates.example.json` and populate it from one finalized
block. Required runtime observations are:

- `pallet_eterra_tcg::V2Feature::{Packs, Conversion, Ranked,
  MythicalAscension}` all false;
- `LegacyCreationSealedV16` true;
- randomness `Disabled`, or `DeterministicPrivateAlpha` only after seed evidence;
- `CryptographyReviewApproved` false, Drand disabled, and no production
  economic use;
- no active Production result policy; Alpha extraction/BR policies are
  practice-only or valueless Training;
- Training pack-credit issuance rejects Production and no paid V2 issuance
  call exists;
- reforge has no dispatchable; magic seed is Training-only;
- legacy marketplace, purchase, faucet, and economic writes disabled;
- Arcade Ticket earning, transfer, redemption, random vending, and featured
  vending all disabled;
- every additional observed economic flag false.

Do not infer these values from UI configuration.

## Reset-readiness packet

Populate `acceptance-inventory.example.json` from the same finalized block as
the selected pre-V16 fresh-reset freeze or post-V16 economic observation. The
`lifetime*` fields are monotonic finalized-event counts from the chain/indexer,
not current-balance counts. They prevent a card or entitlement that was later
consumed, burned, or transferred away from making automatic rollback appear
safe again. Then run:

```bash
./scripts/nexus-v2-private-alpha/alpha_v2_release.py prepare-reset \
  --manifest /private/path/to/bundle/backup-manifest.json \
  --bundle-root /private/path/to/bundle \
  --restore-evidence /private/path/to/evidence/restore.json \
  --migration-evidence /private/path/to/evidence/v14-v16.json \
  --economic-gates /private/path/to/economic-gates.json \
  --acceptance-inventory /private/path/to/acceptance-inventory.json \
  --output /private/path/to/evidence/reset-readiness.json
```

The output says only that the evidence is ready for a separate operator reset
authorization. It explicitly records `resetExecuted: false` and
`deployExecuted: false`.

## Guarded local-only fresh replacement

The ordinary release prohibitions remain in force. A release `--purge-state`
or `--fresh` is accepted only when all of the following are true:

- the source worktree is clean and its `HEAD` equals the configured expected
  commit;
- `NEXUS_V2_LOCAL_ONLY_RELEASE=1` explicitly selects local-only private-alpha
  provenance, so no release branch, tag, remote lookup, or push is required;
- `NEXUS_V2_RESET_READINESS_SHA256` is the exact lowercase SHA-256 of the
  regular, non-symlink readiness packet;
- `--fresh-reset-readiness` names that packet and its closed schema proves
  `pre-v16-fresh-reset-frozen`, fresh-genesis-only scope, disabled economic
  flags, zero V2 acceptance assets, and `resetExecuted: false` /
  `deployExecuted: false`;
- the packet `sourceCommit` equals the exact chain commit being deployed.
  Media and site retain their own independently pinned commits, and archive
  evidence records the frozen chain, replacement chain, replacement media,
  and replacement site identities distinctly; and
- release media and site resets promote previously built immutable candidate
  image IDs. They never build during destructive cutover.

Set the local-only flag, exact expected commits, readiness hash, spec `106`,
runtime code hash, image digests, and service-specific pins in the protected
Mac mini deployment environment files. Build the non-mutating candidates
before the frozen reset:

```bash
READINESS=/private/path/to/evidence/reset-readiness.json
READINESS_SHA256="$(shasum -a 256 "${READINESS}" | awk '{print $1}')"
test "${#READINESS_SHA256}" -eq 64

# macmini2010.env and macmini2014.env must both contain:
# NEXUS_V2_LOCAL_ONLY_RELEASE=1
# NEXUS_V2_RESET_READINESS_SHA256=${READINESS_SHA256}

./deploy/alpha/macmini2010/deploy-media.sh \
  --build-candidate /private/path/to/evidence/media-candidate.json

cd /absolute/path/to/clean/exact-commit/tcg
./deploy/alpha/macmini2014/deploy-site.sh \
  --build-candidate /private/path/to/evidence/site-candidate.json
```

After the final backup, restore rehearsal, copied-state migration rehearsal,
write freeze, zero inventory, and readiness packet are all current, run the
local-only dry-runs. These validate locally and exit before SSH:

```bash
cd /absolute/path/to/clean/exact-commit/chain
./deploy/alpha/macmini2010/deploy-node.sh \
  --purge-state \
  --fresh-reset-readiness "${READINESS}" \
  --promote-candidate /private/path/to/node-candidate-spec106/node-candidate.json \
  --target-identity /private/path/to/eterra-spec106-target-identity.v2.json \
  --dry-run
./deploy/alpha/macmini2010/deploy-media.sh \
  --fresh \
  --fresh-reset-readiness "${READINESS}" \
  --promote-candidate /private/path/to/evidence/media-candidate.json \
  --evidence /private/path/to/evidence/media-deploy.json \
  --dry-run

cd /absolute/path/to/clean/exact-commit/tcg
./deploy/alpha/macmini2014/deploy-site.sh \
  --fresh \
  --fresh-reset-readiness "${READINESS}" \
  --promote-candidate /private/path/to/evidence/site-candidate.json \
  --dry-run
```

The separately authorized real sequence removes `--dry-run` and preserves this
order:

```bash
cd /absolute/path/to/clean/exact-commit/chain
./deploy/alpha/macmini2010/deploy-node.sh \
  --purge-state \
  --fresh-reset-readiness "${READINESS}" \
  --promote-candidate /private/path/to/node-candidate-spec106/node-candidate.json \
  --target-identity /private/path/to/eterra-spec106-target-identity.v2.json \
  --evidence /private/path/to/evidence/node-promotion.json
./deploy/alpha/macmini2010/deploy-media.sh \
  --fresh \
  --fresh-reset-readiness "${READINESS}" \
  --promote-candidate /private/path/to/evidence/media-candidate.json \
  --evidence /private/path/to/evidence/media-deploy.json

cd /absolute/path/to/clean/exact-commit/tcg
./deploy/alpha/macmini2014/deploy-site.sh \
  --fresh \
  --fresh-reset-readiness "${READINESS}" \
  --promote-candidate /private/path/to/evidence/site-candidate.json
```

Before destructive state removal, each host archives the readiness packet,
deployment-root/component identifiers, prior state fingerprints, service or
compose identity, and persistent-volume identity below:

```text
${DEPLOY_ROOT}/archive/nexus-v2-fresh-reset/${READINESS_SHA256}/
```

Node, media, and site write separate `reset-applied.marker` files. The media
path removes only its IPFS project volumes. The site path removes only
`${REMOTE_PROJECT_NAME}_mongo_data`; it verifies the Caddy data/config volumes
before and after and never removes them. The readiness JSON itself is not
rewritten after use because that would invalidate its pin.

Direct `reset-node.sh` and `reset-media.sh` remain development-only. After a
component marker exists, do not reuse `--fresh`; assess the evidence and use a
non-fresh immutable promotion for a forward repair if appropriate.

### Two closed-access post-reset phases

Keep Alpha access closed throughout both phases. The runtime-bundle spec
builder (`7338beff...`, node hash `2dd95dcd...`) is not the deployment node.
Build and attest the deployment node from the final chain commit after all
freeze hardening; its source, binary hash, attestation, candidate, and target-v2
identity remain replacement values until that build finishes.

Phase 1 is the recoverable base-stack window. Start only the fresh node, media,
IPFS, indexer/API, and site required for read-only checks. Do not register the
game authority and do not create a gameplay session or result. Capture fresh
post-V16 disabled-economic gates, zero current and lifetime V2/legacy
inventory, the all-write-ingress barrier, and base-stack health/readback. Use
`acceptance_boundary.py collect`; it verifies the exact frozen Linux runtime
bytes and metadata, captures every relevant storage key at one finalized
block, and deterministically writes all three artifacts. Do not hand-author the
gate or inventory. If this base smoke fails while every count is still zero,
the pinned post-cutover coordinator may select automatic restoration of the
verified archive. Complete the coordinator's actual `--execute` decision
before any actual-chain FPS proof.

Phase 2 begins only after Phase 1 passes, coordinator execute evidence says
`keep-v2`, all external write ingress is still closed, and
`acceptance_boundary.py create-receipt` has written its hash-pinned immutable
receipt. Receipt issuance itself permanently disables automatic restore before
the first operator write. The receipt authorizes only bounded authority
registration, one `ManualAdmin` AlphaAccess grant while mode stays `Enforced`,
and a proof-only game `1005`/version `1`/mode `1` Ability Deathmatch
Training/practice policy with zero XP, rewards, budget liability, and
persistent loadout. `Open` access, valued rewards, paid/public activation, and
canonical policy seeding remain forbidden.

Run the actual-chain FPS proof/candidate and AI seal, then deactivate its
temporary policy. Its exact session/result permanently moves recovery to
`post-acceptance-pause-and-forward-fix`; `NextSessionId` and durable result
history keep that true even after live maps are pruned. Legacy
`EterraGameAuthority` game allocation/end/elimination writes have the same
effect through `NextGameId` and durable command/event receipts. Record the
post-proof inventory and authoritative readback immediately. The canonical
seeder must require those exact nonzero proof session/result IDs and hashes;
zero, omitted, wildcard, or arbitrary baselines are invalid. Reopen Alpha
access only after later authority/FPS/Legends/full-loop checks pass, and never
switch AlphaAccess to `Open`. Never precompute or submit the proof in Phase 1.

### Post-cutover coordinator

The component reset scripts are not themselves a cross-host transaction.
`deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py` now supplies
the missing restart-safe decision and ordering layer without owning
credentials or embedding SSH/RPC/Docker commands.

It accepts only a SHA-256-pinned, unexpired plan and revalidates:

- the exact complete final backup and its isolated restore evidence;
- an at-most-900-second-old post-V16 disabled-economics observation;
- a deterministic current/lifetime V2 and legacy inventory rederived from the
  same finalized-block RPC capture as the disabled gates;
- the exact frozen production Wasm, SCALE metadata, fresh genesis, and closed
  external-write-ingress evidence;
- a stable all-V2-write barrier established before inventory capture;
- distinct clean exact commits for chain, media, and site/indexer;
- exact hashes for the coordinator, component adapters, and existing
  restore/deploy/status scripts; and
- the node, media, and site reset archives under the original readiness hash.

Both hosts dry-run every possible action. On a pre-acceptance smoke failure,
both hosts reassert the write pause and archive their failed V2 roots before
either restore starts. Each phase receives an immutable result and marker, so
an interrupted run validates and skips completed phases. An adapter retry must
return its remote idempotency marker without repeating the mutation.

After any current or lifetime V2 or legacy acceptance count is nonzero, archive
and restore actions are excluded. The coordinator only performs the idempotent
cross-host pause and records `post-acceptance-pause-and-forward-fix`.

The exact schemas, action protocol, and validate/dry-run/execute commands are
in `deploy/alpha/macmini2010/nexus-v2-post-cutover-rollback.md`. That runbook
also documents the explicit-confirmation protected chain/media backend, exact
host paths/ports, immutable failed-root archive, closed final-backup staging
layout, and read-only restored-artifact verification modes. Fixture remains
the default. The site/indexer adapter now provides the equivalent protected
backend while importing the exact pinned shared chain contract. Full protected
cross-host execute remains a hard stop until final clean chain/site commits,
helper hashes, the closed site-indexer service lock/checkpoint, and retained
image IDs are pinned and verified. Protected and fixture receipts must never
be mixed. No live host was contacted and no coordinator action was executed
while implementing this path.

## Rollback boundary

Automatic rollback is a one-way safety boundary:

- A pre-V16 fresh-reset gate is never valid rollback evidence. Rollback
  requires a new post-V16 `economic-gates.example.json` observation from the
  replacement chain.
- Before any post-reset V2 or legacy acceptance write exists, a fresh finalized
  zero inventory, disabled economic gates, unexpired approval, readiness hash,
  and rollback driver hash may authorize the external driver.
- Receipt issuance permanently closes automatic rollback before the first
  Phase-2 operator write. Independently, the instant any V2 card, entity,
  credit, opening, commitment, magic balance, session/result, entitlement,
  ranked team, progression record, or legacy game-authority game write exists,
  every inventory-based rollback check also remains closed.
- After acceptance, pause new V2 writes and preserve cards, conversion
  commitments, entities, and result history. Recovery must move forward under
  a separately reviewed plan.

The repository-owned coordinator delegates remote work to clean,
SHA-256-pinned component adapters so credentials stay inside the existing
deployment scripts. Tests use temporary mock adapters only; no live operation
is exercised.
