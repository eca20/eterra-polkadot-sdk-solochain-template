# Final-freeze component driver contract

`scripts/nexus-v2-private-alpha/final_freeze.py` owns ordering and evidence but
contains no SSH, Docker, systemd, Caddy, or database commands. Each plan entry
names an absolute regular executable, its SHA-256, a non-secret target ID, and
optional fixed arguments. The coordinator re-hashes the executable before every
action and invokes it directly without a shell.

The example deliberately retains a replacement placeholder for the final clean
web/site commit. The final web commit must expose both site roles below and
parse the component-source binding before final-freeze execution. Reusing a
generic deploy script or an unpinned shell command is not accepted.

## Invocation

Every driver accepts:

```text
--action preflight|freeze|verify-frozen|snapshot|verify-snapshot
--transaction-id ID
--release-id ID
--source-commit 40_HEX                    # global chain deployment identity
--component-source-commit 40_HEX          # role-mapped repository identity
--role ROLE
--target TARGET_ID
--bundle-root ABSOLUTE_PRIVATE_DIRECTORY
--result ABSOLUTE_NEW_JSON
--artifact GROUP:NAME                 # repeated exact closed set
--frozen-block-number NUMBER          # verify/snapshot actions
--frozen-block-hash 0x64_HEX          # verify/snapshot actions
--dry-run                             # no connection or mutation allowed
```

It must create `--result` with `O_EXCL` semantics. The JSON keys are exactly:

```text
schemaVersion kind transactionId releaseId sourceCommit role action target
dryRun liveMutationPerformed planned frozenAtUtc frozenFinalizedBlock checks
artifacts
```

`kind` is `nexus-v2-private-alpha-final-freeze-component-result`.
Dry-run reports `planned: true`, `liveMutationPerformed: false`, null freeze
time/block, no artifacts, and still validates every requested action. A live
preflight is read-only. A role freeze records an immutable remote transaction
marker and is idempotent only for the same transaction/release/global-source
and protected component-source identity.
No action may restart or unpause a service.

The coordinator owns the closed mapping: `authority -> sdkgen`, `chain ->
chain`, `media-ipfs -> media`, and both site roles `-> web`. It passes both
source flags and forbids plan arguments from overriding either. Each pinned
driver validates the component commit against its protected deployment
environment; the global source remains the target-v2 chain deployment commit.

Snapshot/verify receipts list exact regular files below `bundle-root` with keys
`group`, `name`, `path`, `bytes`, and `sha256`. Verify-snapshot must return the
identical sorted receipt as snapshot and independently re-read archives/hashes.

## Required checks

All preflights:

```text
credentialsAvailable driverPinned restoreInputsIdentified
snapshotDestinationWritable targetResolved
```

`site-ingress` freeze:

```text
caddyStopped publicHttpIngressStopped publicRpcWriteIngressStopped
```

Its verify-frozen result adds `remainsStopped`. The driver must stop or replace
the live Caddy route with a fail-closed maintenance route that exposes neither
site mutations nor RPC writes, verify public and LAN write paths are closed,
and keep Caddy data/config volumes intact for backup and rollback. It owns:

```text
ingress:caddy-config ingress:caddy-state service:caddy-service
```

`site-indexer-mongo` freeze:

```text
indexerStopped mongoWritesQuiescent siteStopped
```

Its verify-frozen result adds `remainsStopped`. The driver must stop the web/API
mutation process first, stop the indexer at a finalized checkpoint, quiesce
Mongo, capture a consistent database dump, and prove the checkpoint equals the
coordinator-supplied frozen chain block. It owns:

```text
config:indexer-env config:site-env
indexer:indexer-checkpoint indexer:indexer-state indexer:mongo-state
service:indexer-service service:site-service
site:site-image-lock site:site-state
```

`authority`, `chain`, and `media-ipfs` are implemented by the pinned
`deploy/alpha/macmini2010/nexus-v2-final-freeze-chain-driver`. The chain plan
entry also supplies the exact Linux runtime bundle manifest
`79359a961d065bd189f9b585cd57d339b6f58d8855b4d1d156c03b3e0b3feb5c`
and live V14 metadata. The driver additionally pins the bundle's closed
`SHA256SUMS`, Linux source/tree, ELF64/x86-64 host contract, runners,
attestations, exact metadata compatibility proof, production Wasm
`0b0c7c52b38ea880fa626784846164752aa256b9f30d83ed0b45d25277f38243`,
and superseded-macOS-not-copied proof. It
never accepts a pre-existing try-runtime snapshot. After the node stops, the
driver archives its exact base path, extracts that archive into a disposable
isolated base path, and proves the copy's finalized head/number/hash equal the
frozen marker. It invokes the pinned CLI with explicit `--at` and emits
`node:try-runtime-snapshot-proof`, binding the snapshot to the stopped archive,
node, chain spec, frozen marker, RPC observations, CLI, and creation log. The
coordinator independently verifies that proof before producing the backup
manifest. The coordinator itself derives and validates the write-barrier evidence, pre-V16 gate, zero acceptance inventory,
release identifiers, and deployment fingerprints; component drivers may not
invent those values.

Snapshot checks are exactly:

```text
artifactHashesComputed artifactRolesComplete consistentSnapshotCaptured
privateBundlePermissionsRestricted
```

Verify-snapshot checks are exactly:

```text
archivesReadable artifactHashesVerified noServiceResumed restoreContractReady
```

Any nonzero exit, missing/extra key, false check, driver drift, block mismatch,
artifact drift, or role mismatch stops the coordinator. Its failure evidence
sets `automaticResumeAttempted: false`; operations must keep every stopped
component frozen while correcting the affected lane.
