# Nexus V2 private-alpha release safety runbook

Status: **tooling implemented; no backup, restore, reset, rollback, deploy, or
runtime upgrade has been executed by this change**.

This packet prepares a future private-alpha reset while preserving the Nexus
authority model. Runtime state remains authoritative; IPFS and indexers are
backed up as media/read-model systems, not treated as the source of live game
state.

## Hard gates

A reset is not ready until one packet contains all of the following:

1. A hash-complete private backup covering node, media, IPFS, config, service,
   and indexer artifacts.
2. A successful full restore in a sentinel directory, with every service bound
   to loopback ports that are unique and disjoint from declared live ports.
3. Pinned copied-state V15-to-V16 try-runtime evidence plus a completion
   verifier proving the bounded `on_idle` migration resumes safely and
   completes.
4. One finalized-block observation proving every economic surface is disabled.
5. An acceptance inventory from that same finalized block with every V2
   acceptance-asset count equal to zero.

`prepare-reset` only emits a readiness document. A separate operator approval
and separately reviewed live procedure remain required.

## Capture packet

First use the existing current-Alpha helper with a unique name:

```bash
./deploy/alpha/macmini2010/backup-alpha-state.sh \
  nexus-v2-pre-reset-YYYYMMDDTHHMMSSZ
```

Do not use `restore-alpha-state.sh`, `reset-node.sh`, `reset-media.sh`,
`deploy-all.sh --fresh`, or any purge option at this stage.

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

## V15-to-V16 copied-state rehearsal

The existing `scripts/release/rehearse-runtime-upgrade.sh` remains the
spec-103-to-104 current release procedure. It is hash-pinned in the packet but
does not replace V2 migration evidence.

The V2 command never accepts a live RPC URI. It uses the
`node:try-runtime-snapshot` and `node:runtime-v16-wasm` files already pinned in
the backup manifest:

```bash
./scripts/nexus-v2-private-alpha/alpha_v2_release.py rehearse-migration \
  --manifest /private/path/to/bundle/backup-manifest.json \
  --bundle-root /private/path/to/bundle \
  --try-runtime-bin /reviewed/path/try-runtime \
  --try-runtime-revision PINNED_GIT_REVISION \
  --try-runtime-sha256 64_HEX_SHA256 \
  --migration-verifier /reviewed/path/v16-completion-verifier \
  --migration-verifier-sha256 64_HEX_SHA256 \
  --evidence /private/path/to/evidence/v15-v16.json
```

The first phase runs `on-runtime-upgrade` on the copied snapshot. Because the
TCG repair is bounded and multi-block, the second verifier must drive `on_idle`
through completion, interrupt and resume one run, and emit the exact structured
result in `migration-result.example.json`.

The result reconciles all ordinary, NFT-wrapped, known-escrow, and anomalous
cards; proves no loss, duplication, or silent reclassification; verifies
ownership/subject indexes and `NextCardId`; preserves safe legacy exits; seals
legacy creation; and keeps `Packs`, `Conversion`, and `Ranked` false.

## Economic gate observation

Start from `economic-gates.example.json` and populate it from one finalized
block. Required runtime observations are:

- `pallet_eterra_tcg::V2Feature::{Packs, Conversion, Ranked}` all false;
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
the economic observation. The `lifetime*` fields are monotonic finalized-event
counts from the chain/indexer, not current-balance counts. They prevent a card
or entitlement that was later consumed, burned, or transferred away from
making automatic rollback appear safe again. Then run:

```bash
./scripts/nexus-v2-private-alpha/alpha_v2_release.py prepare-reset \
  --manifest /private/path/to/bundle/backup-manifest.json \
  --bundle-root /private/path/to/bundle \
  --restore-evidence /private/path/to/evidence/restore.json \
  --migration-evidence /private/path/to/evidence/v15-v16.json \
  --economic-gates /private/path/to/economic-gates.json \
  --acceptance-inventory /private/path/to/acceptance-inventory.json \
  --output /private/path/to/evidence/reset-readiness.json
```

The output says only that the evidence is ready for a separate operator reset
authorization. It explicitly records `resetExecuted: false` and
`deployExecuted: false`.

## Rollback boundary

Automatic rollback is a one-way safety boundary:

- Before any V2 acceptance asset exists, a fresh finalized zero inventory,
  disabled economic gates, unexpired approval, readiness hash, and rollback
  driver hash may authorize the external driver.
- The instant any V2 card, entity, credit, opening, commitment, magic balance,
  session/result, entitlement, ranked team, or progression record exists,
  automatic rollback is permanently forbidden for that state.
- After acceptance, pause new V2 writes and preserve cards, conversion
  commitments, entities, and result history. Recovery must move forward under
  a separately reviewed plan.

The repository intentionally does not include the rollback driver. Tests use
temporary mock executables only; no live operation is exercised.
