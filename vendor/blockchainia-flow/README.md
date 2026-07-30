# Blockchainia Flow

Blockchainia Flow is a bounded, deterministic state-machine toolkit for game and
interactive-product workflows. It includes a FRAME pallet, an engine-neutral Rust
core, a manifest compiler, a browser WASM facade, a typed TypeScript SDK, and a
visual builder.

Current release target: `0.1.0-alpha.1`.

## Product boundary

Flow coordinates authored states, conditions, economy gates, effects, and
authority-attested events. It does not own critical game assets, private keys,
randomness, competitive legality, or reward calculations. A runtime validates
and applies every authoritative state transition.

The builder is intentionally keyless. It compiles and validates manifests and
prepares unsigned transaction arguments; wallets or operator tooling remain
responsible for review and signing.

## Workspace

- `crates/pallet-blockchainia-flow` — bounded FRAME runtime pallet.
- `crates/blockchainia-flow-core` — deterministic engine-neutral interpreter.
- `crates/blockchainia-flow-manifest` — JSON-to-SCALE compiler and diagnostics.
- `crates/blockchainia-flow-manifest-wasm` — string-based browser bridge.
- `packages/flow-sdk` — typed TypeScript compiler and transaction preparation.
- `apps/builder` — local visual authoring application.
- `fixtures/wire/v0` — immutable compatibility fixtures.
- `examples` and `docs` — authoring examples and integration guidance.

The authoring schema is available at
[`docs/schema/blockchainia.flow.v0.schema.json`](docs/schema/blockchainia.flow.v0.schema.json).

## Compatibility

The preferred authoring label is `blockchainia.flow.v0`. The permanent
`eterra.flow.v0` alias maps to the same runtime manifest version (`0`) and must
compile to byte-identical SCALE.

The Eterra compatibility lane retains runtime alias `EterraFlow`, pallet index
`29`, storage version `2`, calls `0..=6`, and existing storage/wire encodings.
See [docs/WIRE_COMPATIBILITY.md](docs/WIRE_COMPATIBILITY.md).

## Verification

```bash
cargo fmt --check
cargo test --workspace
cargo build -p blockchainia-flow-manifest-wasm --target wasm32-unknown-unknown --release
npm install
npm run check
./scripts/verify-wire-fixtures.sh
```

This local alpha is not approval for package publication, public deployment, or
production economic use. The release gate remains blocked until the provenance
and third-party license review in [docs/LICENSE_REPORT.md](docs/LICENSE_REPORT.md)
is independently approved.
