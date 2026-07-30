# Provenance

## Extraction baseline

The Flow v0 compatibility contract was inspected from the isolated Eterra
runtime baseline:

- repository: `polkadot-sdk-solochain-template`
- baseline commit: `faae2ab5c1721d27946ef2f0f76370f31e209666`
- source path: `pallets/eterra-flow`
- source workspace license declaration: `MIT-0`
- pallet `src/lib.rs` SHA-256:
  `4defb80db438a4b4863f65e7900ad13d27f57e6cc4378c5192d36820b6bf0b0d`
- pallet `src/weights.rs` SHA-256:
  `983fef4b99a028afd823b943450b5143bc3fb8f516181428618618a5a1564b3c`

The FRAME pallet is an extraction and generic-brand adaptation of that MIT-0
source. The Eterra adapter preserves its historical runtime identity.

## Compiler/core clean-room boundary

An ignored local Eterra MVP package was also inspected as a behavioral and wire
reference. Its workspace declared `Apache-2.0`, and no evidence authorizing
relicensing that implementation to MIT-0 was found.

Accordingly, the Blockchainia manifest compiler, engine-neutral core, WASM
facade, TypeScript SDK, and builder were newly implemented against:

- the documented v0 JSON/SCALE contract;
- the runtime pallet's public type ordering and validation behavior;
- locked SCALE outputs generated from five known manifests;
- independent cross-language tests.

The Apache-2.0 compiler/core source files were not copied into this repository.
Their reference SHA-256 values are recorded in
`fixtures/wire/v0/source-lock.json` for audit reproducibility.

## Release posture

This report records engineering provenance; it does not self-approve a legal
conclusion. Public source/package release remains blocked until an independent
review confirms the source history, ownership, notices, dependency licenses,
and intended MIT-0 distribution.

