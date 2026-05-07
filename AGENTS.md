# Chain Agent Instructions

Use the Nexus Season 1 snapshot at:

`/Users/edmundanderson/eterra_projects/docs/nexus_spec_snapshot/`

Primary specs for this repo:

- `nexus_core_rules_1_39.md`
- `superseded_nexus_assumptions.md`
- `nexus_chain_runtime_requirements.md`
- `nexus_event_schema.md`
- `tasks/PI_01_runtime_state_model.md`
- `tasks/PI_05_match_engine.md`
- `tasks/PI_06_workshop_forge_trials.md`
- `tasks/PI_07_admin_locks_pauses.md`
- `tasks/PI_09_payments_vault_expansion.md`

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

