# Pre-reset automatic restore contract

This contract is the only authorized fresh-replacement entry point for the
current private Alpha. It does not authorize public, paid, wagering, transfer,
or economically destructive behavior.

## Foreground ownership

`pre_reset_rollback_supervisor.py run` must remain in the foreground from
archive preparation until it verifies the canonical zero-current/zero-lifetime
acceptance-boundary receipt. Before publishing an arm it installs
SIGINT/SIGTERM/SIGHUP handlers, verifies every clean source commit and exact
driver/helper/script hash, creates read-only detached source clones, prepares
the readiness-bound rollback archives without changing the current Alpha, and
runs both protected recovery preflights.

The immutable arm is never rewritten or removed. Its issuance must be no more
than 300 seconds old when `pre_reset_closure.py` creates the stopped-state
handoff. Its live workflow lease may last no more than 3,600 seconds. Liveness
requires the exact PID, OS process-start token, owner-only lease, nonce binding,
and active lease state. The supervisor checks those identities, all evidence
files, and all immutable executable pins throughout the replacement workflow.

After verifying the canonical zero-asset acceptance-start receipt, the
supervisor atomically changes only the mutable lease from `active` to `retired`
and publishes immutable retirement evidence. Bootstrap tools must require both
the acceptance receipt and:

```text
pre_reset_rollback_supervisor.py verify-retirement \
  --evidence EVIDENCE.json \
  --expected-evidence-sha256 SHA256 \
  --arm ARM.json \
  --expected-arm-sha256 SHA256 \
  --release-id CHAIN_RELEASE_ID \
  --site-release-version vSEMVER \
  --source-commit COMMIT
```

The chain `releaseId` and v-prefixed site `siteReleaseVersion` are distinct and
are bound independently in the plan, arm, lease, component receipts, workflow
receipts, and retirement evidence. Plan validation also enforces the canonical
source ID and relative path for every workflow driver, stage helper, workflow
tool, component driver, host helper, and restore/deploy/status script. A valid
hash attached to a substituted executable is not accepted.

## Pre-arm archive preparation

Rollback archives must exist before a recovery preflight can truthfully prove
that they are usable. The supervisor therefore owns this exact sequence:

1. reconstruct and verify immutable detached source views;
2. invoke both components with `--mode prepare --action prepare-reset-archives`;
3. invoke both component recovery preflights;
4. revalidate the original and detached plan inputs;
5. publish the immutable arm and active lease;
6. run the replacement workflow.

Preparation may copy and seal current Alpha state only. It must not stop,
restart, reset, restore, or rewrite the current deployment. Its component
result has the same closed outer schema as every other component call and
exactly these true checks:

```text
archivePreparationNonDestructive
archivesPreparedAndReadOnly
currentAlphaStatePreserved
noResetApplied
readinessIdentityBound
restoreInputsVerified
sourcePinsVerified
```

The immutable arm pins both preparation result files and both preflight result
files. Supervisor evidence and retirement evidence retain each preparation
result SHA-256. A preparation or preflight error before arm publication emits
`pre-arm-archive-preparation-or-preflight-failed`; no destructive workflow or
automatic restore is attempted because no reset was allowed to occur.

## Recovery component interface

The plan pins exactly `chain-media` and `site-indexer`. Each driver is invoked
without secrets on its command line:

```text
DRIVER \
  --plan PLAN.json \
  --plan-sha256 SHA256 \
  --component chain-media|site-indexer \
  --mode prepare|preflight|execute \
  --action prepare-reset-archives|preflight|pause-v2-writes|archive-failed-v2|restore-final-backup|restored-smoke \
  --result RESULT.json
```

Preflight must be read-only and prove credentials, the already prepared
readiness-bound reset archives, final-backup restore inputs, and every exact
executable pin. Each component preflight dry-runs all four recovery actions;
the site lane keeps action-scoped context and result evidence so one dry-run
cannot overwrite another. Execute actions run in this global order, with both
components attempted at every step:

```text
pause-v2-writes
archive-failed-v2
restore-final-backup
restored-smoke
```

Recovery does not depend on a V2 RPC or acceptance capture. The failed V2 roots
must be archived immutably before restoring the final backup. The same failed
archive SHA-256 must be returned by archive, restore, and restored-smoke. A
failure in either component never prevents attempts in the other lane.

The canonical component result fields are `COMPONENT_RESULT_KEYS` in
`pre_reset_rollback_supervisor.py`. Checks must exactly match
`PREFLIGHT_CHECKS` or `ACTION_CHECKS[action]`. Production children require:

```text
NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION=PRIVATE_ALPHA_ROLLBACK_ONLY
```

`nexus-v2-pre-reset-chain-media-component-driver` is the protected chain/media
adapter. It uses the pinned existing host action and the production
`deploy/alpha/macmini2010/nexus_v2_rollback_staging.py` library. That library
rehashes the final backup manifest and exact chain/media restore subset, copies
each item with exclusive creation into an owner-only staging directory, and
writes a canonical staging contract before the protected restore helper can
run. Its restored smoke probes the restored final backup, not a V2 dependency.
The site lane supplies the equivalent `site-indexer` adapter under the same
frozen interface. Fixture roots end in `.NONDEPLOYABLE` and can never contact
a protected host.

## Replacement workflow interface

The supervisor executes exactly one pinned
`pre_reset_replacement_workflow.py`. It owns this immutable stage order:

1. `createPreResetClosure`
2. `deployChainMediaAuthority`
3. `deploySiteIndexer`
4. `closeIngressAndObserve`
5. `createZeroAssetAcceptanceFence`

Each stage helper receives:

```text
HELPER \
  --plan PLAN.json --plan-sha256 SHA256 \
  --workflow-contract CONTRACT.json --workflow-contract-sha256 SHA256 \
  --automatic-restore-arm ARM.json --automatic-restore-arm-sha256 SHA256 \
  --stage STAGE \
  --workflow-state-root WORKFLOW_ROOT \
  --stage-state-root WORKFLOW_ROOT/stages/STAGE \
  --result WORKFLOW_ROOT/stages/STAGE/result.json
```

The workflow contract has the exact `CONTRACT_KEYS` declared by
`pre_reset_replacement_workflow.py`. `artifactSha256` equals all supervisor plan
artifact pins. `toolPins` has exactly these `sourceId/path/sha256` roles:

- `preResetClosure`
- `chainDeployAll`
- `siteDeploy`
- `phase1IngressClosure`
- `acceptanceBoundary`
- `postCutoverCoordinator`

`stageInputs` contains exactly the five stage names. Each pinned adapter owns a
closed sub-schema:

```text
createPreResetClosure: {}
deployChainMediaAuthority: node/media candidate and target-identity path+hash pins
deploySiteIndexer: site candidate and Phase-1 Caddyfile path+hash pins
closeIngressAndObserve: stability window and runtime-bundle identity
createZeroAssetAcceptanceFence: runtime/site drivers, reset archives, and observation age
```

Generated outputs never become unpinned input paths. Site deployment writes a
canonical mode-0400 receipt only on its execute path at:

```text
WORKFLOW_ROOT/stages/deploySiteIndexer/site-post-deploy-identity.json
```

The deploy adapter verifies its release, chain/site commits, readiness,
pre-reset closure, restore arm, compose identity, Phase-1 Caddy identity,
container/image/publication facts, authority documents, and disabled safety
flags. Dry-run must not create it. The close/observe stage rehashes and
revalidates the exact same receipt before and after ingress closure so a
different deployment cannot be observed than the one just installed.
Authority may be explicitly unavailable during this pre-acceptance phase; if
it is reported available, both FPS and Legends documents must match their
reviewed private-alpha configuration hashes and exact disabled-economic facts.
This is a Phase-1 pre-acceptance receipt only. It cannot authorize reopening;
that boundary requires a separate immutable read-only capture taken after the
bounded Phase-2 proof and bound to its nonzero proof inventory.

The close/observe adapter writes canonical Phase-1 evidence under:

```text
WORKFLOW_ROOT/stages/closeIngressAndObserve/phase1-output/
```

including `execute-evidence.json`. The final adapter consumes that exact tree,
derives the keep-V2 coordinator evidence, proves zero current and lifetime
acceptance counts, and exclusively creates the handoff path named by
`acceptanceStartFence.handoffPath`.

Every stage emits exactly `STAGE_RESULT_KEYS`. Only the two deployments and
ingress close report `mutationPerformed: true`; only the last stage reports
`acceptanceStartFenceWritten: true`. No stage may perform a bootstrap or other
acceptance write.

## Single recovery owner during post-cutover smoke

The zero-asset fence stage launches the post-cutover coordinator with the
hash-pinned foreground supervisor, immutable arm, and site release version.
The coordinator verifies the live arm with `verify-arm --full-binding` and
publishes immutable `external-recovery-ownership.json` evidence. That receipt
names `pre-reset-rollback-supervisor` as the sole recovery owner and sets
`nestedRecoveryActionsAllowed` to false.

In this mode the coordinator may run the post-cutover smoke and retain V2 on
success. On failure it exits without invoking its own pause, failed-root
archive, restore, or reopen actions. The error propagates to the still-live
supervisor, which alone executes the four cross-component recovery phases.
This prevents two independent rollback engines from racing on the same Alpha.

## Failure boundary

Before the arm exists, an archive-preparation or preflight failure is a
non-mutating stop. After the arm exists and before the zero-asset receipt is
verified, any workflow error, exception, timeout, or termination signal
triggers both recovery lanes. Once that receipt is verified, automatic
restoration is permanently retired before any bootstrap write can be
authorized. Subsequent failures are pause-and-forward-fix only.
