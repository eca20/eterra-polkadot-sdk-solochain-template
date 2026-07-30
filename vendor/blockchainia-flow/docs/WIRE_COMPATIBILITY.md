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

`fixtures/wire/v0/contract.json` and the template `.scale.hex` files are the
machine-checked contract. The Eterra adapter additionally checks its vendored
Flow tree against the exact local Flow commit.

The fixture inputs under `fixtures/wire/v0/inputs` intentionally pin
`game_id=1` so the historical wire captures remain immutable. The reusable
examples keep distinct game namespaces for integration tests; identity fields
are part of SCALE and therefore produce different bytes.
