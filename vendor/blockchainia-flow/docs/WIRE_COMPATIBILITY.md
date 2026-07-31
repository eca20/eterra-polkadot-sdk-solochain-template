# Flow v0 wire compatibility

## Locked identifiers

- Preferred authoring label: `blockchainia.flow.v0`
- Permanent alias: `eterra.flow.v0`
- Runtime manifest version: `0`
- Eterra runtime alias: `EterraFlow`
- Eterra pallet index: `29`
- Storage version: `2`
- Dispatch call indices: `create_game=0`, `upload_version_chunk=1`,
  `finalize_version=2`, `activate_version=3`, `create_instance=4`,
  `submit_action=5`, `submit_attested_event=6`

The label is authoring metadata and is not part of SCALE. Both accepted labels
therefore compile to identical bytes.

## Canonical hash

The canonical manifest hash is the runtime Blake2-256 hash of the exact
SCALE-encoded `Manifest V0` bytes. A decoder must consume the entire byte slice;
trailing data is invalid.

## Compatibility rules

1. Never reorder fields or enum variants in v0.
2. Never reuse a call, event, error, or enum discriminant.
3. Never rename Eterra storage aliases or move the runtime alias/index.
4. Existing finalized manifests and instances remain pinned.
5. A future wire shape uses a new manifest version and new locked fixtures.
6. The `eterra.flow.v0` authoring alias is permanent.

`fixtures/wire/v0/contract.json` locks storage names/hashers, calls, events,
errors, and Manifest v0 enum discriminants. The template `.scale.hex` files
lock full Manifest bytes. `scripts/verify-runtime-contract.mjs` checks the
runtime source against the ABI contract, and the Eterra adapter additionally
checks its vendored Flow tree against the exact local Flow commit.

## Zero-write upgrade proof

The extracted pallet and Eterra adapter have no migration hook. Their
`runtime_upgrade_hook_is_zero_write` and
`adapter_runtime_upgrade_hook_is_zero_write` tests snapshot the externalities
storage root, run `on_runtime_upgrade`, and prove both an unchanged root and
zero returned weight. The Eterra runtime gate runs its adapter test with
`try-runtime` enabled before accepting a vendored commit. A copied-state
try-runtime rehearsal remains a deployment gate; these unit proofs do not
replace it.

The fixture inputs under `fixtures/wire/v0/inputs` intentionally pin
`game_id=1` so the historical wire captures remain immutable. The reusable
examples keep distinct game namespaces for integration tests; identity fields
are part of SCALE and therefore produce different bytes.
