# Nexus V2 post-cutover smoke and rollback coordinator

Status: coordinator, fixture adapters, and a protected chain/media host backend
are implemented and covered by offline mock-host safety tests. Fixture remains
the default. The protected backend is unreachable unless an operator selects
it and supplies the exact confirmation token. This implementation change has
not opened an SSH connection, queried Alpha RPC, invoked Docker, paused a
service, restored a backup, or deployed a component.

The coordinator is:

`deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py`

The chain/media offline adapter is:

`deploy/alpha/macmini2010/nexus-v2-rollback-component-driver`

Its protected host helper is:

`deploy/alpha/macmini2010/nexus-v2-rollback-protected-host-action.sh`

The site/indexer entrypoint is stored in the independently pinned site source:

`deploy/alpha/macmini2014/nexus-v2-rollback-component-driver`

It imports the shared closed receipt protocol only after verifying the exact
clean chain commit already required by the site component. The site restore
surface is `deploy/alpha/macmini2014/restore-alpha-state.sh`.

The coordinator deliberately owns no credentials and contains no
remote-operation implementation. It invokes only action adapters that are:

- tracked in clean, exact-commit component worktrees;
- pinned by SHA-256 in the coordinator plan;
- passed no secret values by the coordinator; and
- responsible for loading the existing protected deployment environment and
  using the plan-pinned restore, deploy, and status scripts.

The chain, media, and site commits are independent identities. The plan and
observation record all three. Only the chain pin must equal the top-level
`sourceCommit`.

## Required evidence

The coordinator revalidates all of these before invoking a component driver:

1. The complete final backup manifest and every artifact in its bundle.
2. The exact isolated restore-rehearsal evidence for that manifest.
3. The exact frozen spec-106 Linux runtime bundle, including production Wasm
   SHA-256 `0b0c7c52...8243`, Metadata SCALE SHA-256 `26ed50d1...ee68`, and
   bundle-manifest SHA-256 `79359a96...feb5c`.
4. A canonical RPC capture made at one non-genesis finalized block and pinned
   to the fresh Alpha genesis, exact `:code`, and exact SCALE metadata.
5. Economic gates and the complete current/lifetime V2 and legacy acceptance
   inventory byte-for-byte rederived from that capture. Hand-authored evidence
   is rejected.
6. A closed post-cutover observation envelope no more than 900 seconds old.
7. Separate hash-pinned evidence that chain/media and site/indexer external
   write ingress is closed while block production remains available for
   finalized readback.
8. A stable `AllV2WritesPaused` barrier established before the inventory was
   captured and bound to that ingress evidence.
9. A SHA-256-pinned, unexpired coordinator plan with
   `automaticRestoreApproved: true` and
   `paidOrPublicActivationAuthorized: false`.
10. The original node, media, and site reset archives keyed by the exact fresh
   reset-readiness hash.

The example schemas are:

- `docs/nexus-v2-private-alpha/post-cutover-observation.example.json`
- `docs/nexus-v2-private-alpha/post-cutover-coordinator-plan.example.json`
- `docs/nexus-v2-private-alpha/ingress-closed-evidence.example.json`
- `docs/nexus-v2-private-alpha/acceptance-boundary-receipt.example.json`

Examples contain placeholders and are never valid authorization.

## Component action protocol

The plan contains exactly two drivers:

- `chain-media`, covering the independently pinned chain and media roots;
- `site-indexer`, covering the pinned chain coordinator and site/indexer root.

The coordinator calls each driver with the component, action, mode, operation
ID, plan and evidence paths, and an exclusive result path. It never passes
credential values. Drivers must support:

- `post-cutover-smoke`
- `pause-v2-writes`
- `archive-failed-v2`
- `restore-final-backup`
- `restored-smoke`

and modes `dry-run` and `execute`.

Both implementations keep a fixture contract whose root ends in
`.NONDEPLOYABLE` as their default backend. The fixture tests verify the exact
existing restore/deploy/status paths and hashes, all final-backup artifact
hashes, restore-rehearsal identity, source commits, reset archive identities,
action ordering, and immutable idempotency markers. Without either the fixture
root or the explicit protected-backend selection, they fail before writing a
result or invoking an operational command.

Protected operation for both chain/media and site/indexer additionally
requires both:

```text
NEXUS_V2_ROLLBACK_BACKEND=protected-alpha
NEXUS_V2_ROLLBACK_PROTECTED_CONFIRMATION=PRIVATE_ALPHA_ROLLBACK_ONLY
```

The site/indexer adapter imports the shared contract from the exact pinned
clean chain commit and delegates protected actions to its pinned
`deploy/alpha/macmini2014/nexus-v2-rollback-protected-host-action.sh`. It
restores only its closed site/indexer final-backup staging contract, starts
only retained image IDs with `--no-build --pull never`, and never archives,
removes, or restores Caddy TLS volumes. Full protected cross-host execution
remains fail-closed until both final clean source commits, helper hashes,
final-backup artifacts, and retained image IDs are pinned in the rollback
plan. Protected and fixture receipts must never be mixed.

It loads credentials only through the existing protected environment file and
never puts them in the coordinator context, result, logs, or remote markers.
Its host contract is intentionally exact:

```text
DEPLOY_ROOT=/opt/eterra-alpha
REMOTE_NODE_DATA_DIR=/var/lib/eterra-alpha-node
chain RPC/P2P=9944/30333
media/authority=4000/8787
IPFS API/gateway=5001/8080
```

Any path, port, release ID, source commit, helper path, reset-archive path, or
script hash mismatch is a hard stop. Dry-run resolves the existing SSH
credential and performs only read-only reset-archive checks. It does not pause,
archive, restore, restart, or deploy.

Every action is dry-run for both hosts before any execute action. Closed result
schemas reject extra fields, missing reset archives, source drift, remote work
during dry-run, missing remote idempotency markers, and incomplete checks.
Driver output is captured in private evidence logs and is not echoed.

Before any restoration:

1. both hosts must report V2 writes paused;
2. both failed V2 deployment roots must be archived;
3. both immutable archive receipts must exist; and
4. the restore action must bind to the same archive hash.

The restore receipts must prove the exact backup hashes and restore evidence
were used, and that the pinned existing restore/deploy scripts were invoked.

## Protected chain/media execution

The host helper implements the five actions as follows:

- `post-cutover-smoke` invokes the pinned status script, then strictly checks
  release/source readback, node runtime RPC, media readiness, authority
  authorization, IPFS, and blocked public media upload. A failed V2 smoke is a
  valid observed outcome, not permission to skip the acceptance inventory.
- `pause-v2-writes` stops the node, authority, and media service while
  preserving node data and IPFS volumes. After any current or lifetime
  acceptance count becomes nonzero, this is the only mutating action allowed.
- `archive-failed-v2` requires the immutable pause marker, stops IPFS, archives
  the failed node/media/authority roots, shared environment/state, node data,
  IPFS volumes, service units, and image identity, then makes the archive
  read-only and binds later actions to its manifest SHA-256.
- `restore-final-backup` is permitted only while every current and lifetime
  acceptance count is zero. It invokes the pinned
  `restore-alpha-state.sh --verified-final-backup`, followed by the pinned
  node and media scripts in their new read-only
  `--verify-restored-final-backup` modes.
- `restored-smoke` checks the backup manifest identity, exact installed
  artifacts and image locks, node/media/IPFS health, stopped authority,
  blocked upload, and disabled paid/public gate evidence.

Each completed execute action gets an immutable marker below:

```text
/opt/eterra-alpha/shared/rollback/nexus-v2-post-cutover/
  OPERATION_ID/chain-media/actions/ACTION.json
```

A retry validates that marker and reports `alreadyApplied: true` without
repeating the mutation. A partial failed-V2 archive has no valid marker and is
a manual-review hard stop. An interrupted exact restore may be retried from
the same immutable staging contract; it cannot select different backup bytes.

### Final-backup restore layout

The driver copies only these manifest-bound files into a mode-0700 temporary
directory:

```text
node-data.tar.gz
node-binary
media-state.tar.gz
media-image-lock.json
ipfs-data.tar.gz
ipfs-staging.tar.gz
node.env
media.env
backup-economic-gates.json
chain-spec.json
node-service.service
media-service.json
staging-contract.json
```

The staging contract pins every file hash, the backup manifest, release,
chain/media commits, and a closed filename set. Archives reject traversal,
links, devices, and other special files. `media-service.json` pins both compose
paths and their hashes inside `media-state.tar.gz`.
`media-image-lock.json` pins the exact media and Kubo digest references and
local image IDs. Restore never builds or pulls. Missing retained images are a
hard stop, not permission to substitute a newer image.

Before mutation, the restore script rehashes the uploaded copy and preflights
the compose files and image IDs. It restores the node binary/spec/service,
chain data, media definitions, and IPFS volumes, preserves the backed
environments, starts only node/media/IPFS, and records the exact backup
identity. The authority remains stopped after rollback. Stale V2 deploy hashes
are removed so they cannot masquerade as restored provenance.

## Decision boundary

If post-cutover smoke passes, the coordinator records `keep-v2`.

If smoke fails and every current and lifetime V2 and legacy acceptance count
remains zero, it:

1. reasserts the write pause on both hosts;
2. archives both failed V2 roots;
3. restores both hosts from the exact final backup;
4. runs restored-state smoke on both hosts; and
5. records `pre-acceptance-automatic-restore`.

If any count is nonzero, archive and restore actions are never invoked. This
includes a legacy `EterraGameAuthority` game allocation/end/elimination write,
a V2 GameResults session allocation, or an accepted V2 result even after its
live maps are later pruned. Both hosts receive only the idempotent pause action,
and the evidence records `post-acceptance-pause-and-forward-fix`.

## Invocation

Populate the closed plan and observation from current reviewed evidence, then
pin the exact plan bytes:

```bash
export NEXUS_V2_ROLLBACK_PLAN_SHA256="$(
  shasum -a 256 /private/evidence/post-cutover-plan.json | awk '{print $1}'
)"
```

Local validation invokes no driver:

```bash
./deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py \
  --plan /private/evidence/post-cutover-plan.json \
  --manifest /private/backup/backup-manifest.json \
  --bundle-root /private/backup \
  --runtime-bundle-root /private/runtime/runtime-release-spec106-linux \
  --restore-evidence /private/evidence/restore.json \
  --observation /private/evidence/post-cutover-observation.json \
  --acceptance-boundary-capture /private/evidence/acceptance-boundary-rpc-capture.json \
  --ingress-closed-evidence /private/evidence/ingress-closed-evidence.json \
  --economic-gates /private/evidence/post-v16-economic-gates.json \
  --acceptance-inventory /private/evidence/post-v16-inventory.json \
  --state-dir /private/evidence/coordinator/validate \
  --evidence /private/evidence/coordinator/validate.json \
  --validate-only
```

An explicit dry-run uses a separate state directory and runs every component
adapter in dry-run mode:

```bash
./deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py \
  --plan /private/evidence/post-cutover-plan.json \
  --manifest /private/backup/backup-manifest.json \
  --bundle-root /private/backup \
  --runtime-bundle-root /private/runtime/runtime-release-spec106-linux \
  --restore-evidence /private/evidence/restore.json \
  --observation /private/evidence/post-cutover-observation.json \
  --acceptance-boundary-capture /private/evidence/acceptance-boundary-rpc-capture.json \
  --ingress-closed-evidence /private/evidence/ingress-closed-evidence.json \
  --economic-gates /private/evidence/post-v16-economic-gates.json \
  --acceptance-inventory /private/evidence/post-v16-inventory.json \
  --state-dir /private/evidence/coordinator/dry-run \
  --evidence /private/evidence/coordinator/dry-run.json \
  --dry-run
```

The two protected values above select the chain/media backend. Do not use them
for a cross-host coordinator run until the site/indexer protected backend is
also present and pinned; the present site adapter will fail closed. Do not put
confirmation values into the plan JSON.

The separately authorized execution uses a new state directory. It repeats and
pins every dry-run before executing anything:

```bash
./deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py \
  --plan /private/evidence/post-cutover-plan.json \
  --manifest /private/backup/backup-manifest.json \
  --bundle-root /private/backup \
  --runtime-bundle-root /private/runtime/runtime-release-spec106-linux \
  --restore-evidence /private/evidence/restore.json \
  --observation /private/evidence/post-cutover-observation.json \
  --acceptance-boundary-capture /private/evidence/acceptance-boundary-rpc-capture.json \
  --ingress-closed-evidence /private/evidence/ingress-closed-evidence.json \
  --economic-gates /private/evidence/post-v16-economic-gates.json \
  --acceptance-inventory /private/evidence/post-v16-inventory.json \
  --state-dir /private/evidence/coordinator/execute \
  --evidence /private/evidence/coordinator/execute.json \
  --execute
```

Protected execute uses the same environment selection as protected dry-run.
Never reuse the dry-run state directory for execute, and never reuse an
operation ID with different evidence.

## One-way Phase-2 receipt

The coordinator never invents the economic gates or inventory. With every
external write path already closed, collect them from the replacement chain:

```bash
./scripts/nexus-v2-private-alpha/acceptance_boundary.py collect \
  --rpc-url http://127.0.0.1:9944 \
  --runtime-bundle-root /private/runtime/runtime-release-spec106-linux \
  --runtime-bundle-manifest-sha256 79359a961d065bd189f9b585cd57d339b6f58d8855b4d1d156c03b3e0b3feb5c \
  --release-id RELEASE_ID \
  --source-commit 40_LOWERCASE_HEX_DEPLOYMENT_COMMIT \
  --genesis-hash 0x67556081fc3400e10591f6857e89ce37014b8ba39c539f559f6959b4afa00e5b \
  --capture /private/evidence/acceptance-boundary-rpc-capture.json \
  --economic-gates /private/evidence/post-v16-economic-gates.json \
  --acceptance-inventory /private/evidence/post-v16-inventory.json
```

All three outputs are exclusive-create, canonical, mode `0440`, and tied to
one finalized block. The collector verifies deployed `:code` SHA-256
`0b0c7c52b38ea880fa626784846164752aa256b9f30d83ed0b45d25277f38243`
and Metadata SCALE SHA-256
`26ed50d186a0cb134cb8ef6b9f619cd04195b52cf4d06fb3f2c31050b103ee68`.
The first is the SHA-256 of the production Wasm bytes, not the chain's Blake2
runtime-code hash.

After coordinator `--execute` produces `decision: keep-v2`, issue exactly one
receipt. The final marker is normally
`STATE_DIR/final-evidence.marker.json`:

```bash
./scripts/nexus-v2-private-alpha/acceptance_boundary.py create-receipt \
  --runtime-bundle-root /private/runtime/runtime-release-spec106-linux \
  --runtime-bundle-manifest-sha256 79359a961d065bd189f9b585cd57d339b6f58d8855b4d1d156c03b3e0b3feb5c \
  --release-id RELEASE_ID \
  --source-commit 40_LOWERCASE_HEX_DEPLOYMENT_COMMIT \
  --genesis-hash 0x67556081fc3400e10591f6857e89ce37014b8ba39c539f559f6959b4afa00e5b \
  --capture /private/evidence/acceptance-boundary-rpc-capture.json \
  --economic-gates /private/evidence/post-v16-economic-gates.json \
  --acceptance-inventory /private/evidence/post-v16-inventory.json \
  --observation /private/evidence/post-cutover-observation.json \
  --ingress-closed-evidence /private/evidence/ingress-closed-evidence.json \
  --ingress-closed-evidence-sha256 64_LOWERCASE_HEX \
  --coordinator-evidence /private/evidence/coordinator/execute.json \
  --coordinator-evidence-sha256 64_LOWERCASE_HEX \
  --coordinator-final-marker /private/evidence/coordinator/execute/final-evidence.marker.json \
  --coordinator-final-marker-sha256 64_LOWERCASE_HEX \
  --output /private/evidence/acceptance-boundary-receipt.json
```

The returned receipt SHA-256 is a required independent input to every proof or
bootstrap verifier. Issuance permanently retires automatic archive restoration
before any operator write. The exact scope permits authority registration, one
bounded `ManualAdmin` AlphaAccess grant while access remains `Enforced`, and a
proof-only Ability Deathmatch Training/practice policy with zero XP, rewards,
budget liability, or persistent assets. It forbids `Open` access, economically
valued rewards, paid/public activation, and canonical policy seeding before the
proof; the proof-only policy must be deactivated afterward.

The actual proof creates the first exact session/result and therefore makes the
boundary independently observable in `NextSessionId` and durable result/epoch
counters. Capture a separate post-proof inventory/readback and bind canonical
seeding to those exact nonzero proof IDs and result hashes. A zero, missing,
wildcard, or arbitrary proof baseline must fail closed.

## Restart behavior

Each component/mode/action receipt is hash-pinned by an immutable local marker.
Completed phases are skipped after validation. A driver retry must return its
remote idempotency marker and `alreadyApplied: true` without repeating the
remote mutation. All host archives finish before the first restore begins.

The final evidence and final marker are also immutable. If interruption occurs
between writing them, the next identical invocation validates the evidence and
reconstructs only the missing marker. Reusing the state directory with a
different plan, backup, restore, observation, gate, or inventory hash is
rejected.

## Offline adapter tests

```bash
python3 -m unittest \
  deploy/alpha/macmini2010/test_nexus_v2_rollback_component_driver.py

NEXUS_V2_CHAIN_ROLLBACK_DRIVER_SOURCE="$PWD/deploy/alpha/macmini2010/nexus-v2-rollback-component-driver" \
  python3 -m unittest \
  /absolute/path/to/site/deploy/alpha/macmini2014/test_nexus_v2_rollback_component_driver.py
```

The tests create clean temporary chain/media/site repositories, a complete
hash-pinned final-backup fixture, and local host-state fixtures. They cover all
five dry-run receipts, ordered execute receipts, protected-backend confirmation,
closed restore staging, post-acceptance restore rejection, failed precondition
handling, and no-repeat idempotency. The protected tests substitute a local
mock helper before committing their temporary source repository. They never
load a deployment environment or make a network connection. A separate test
runs the actual protected host helper against stubbed read-only remote
functions, covering its closed dry-run receipt, pause receipt, immutable marker
readback, and no-repeat retry behavior.
