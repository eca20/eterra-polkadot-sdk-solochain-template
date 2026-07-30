# Chain Agent Instructions

Use the active Nexus V2 snapshot at:

`/Users/edmundanderson/eterra_projects/docs/nexus_v2_spec_snapshot_20260730_runtime_freeze_v3/`

The superseded V2 snapshots and
`/Users/edmundanderson/eterra_projects/docs/nexus_spec_snapshot/` historical
LegacyV1/Season 1 authority are read-only. Legacy SCALE/state compatibility is
preserved only where the active V2 contract requires it.

Primary specs for this repo:

- `nexus_v2_core_rules.md`
- `nexus_v2_migration_baseline_addendum.md`
- `superseded_nexus_assumptions.md`
- `nexus_product_increment_plan.md`
- `approved_design/nexus_v2_card_entity_cross_game_loop_refactor.md`
- `content/nexus_v2/manifests/content_manifest.json`

## Scope

The runtime is authoritative for inventory, match, forge, seal, salvage, reward, lock, pause, and payment-gated state transitions. Do not make frontend-only assumptions for mechanics that affect player assets or match outcomes.

Emit events for meaningful state changes and keep event names aligned with `nexus_event_schema.md`.

For each PI, inspect existing pallets before adding new ones. Prefer evolving local Eterra pallets over creating parallel systems unless the plan documents why a split is safer.

## Commands

Run the narrowest relevant checks first, then broaden when touching shared runtime behavior:

- `cargo fmt`
- `cargo test -p <crate-or-pallet>`
- `cargo test`
- `cargo clippy --all-targets --all-features`, if available and practical

For runtime upgrade or chain spec behavior, also review the commands in `README.md` and `scripts/deploy.sh`.
