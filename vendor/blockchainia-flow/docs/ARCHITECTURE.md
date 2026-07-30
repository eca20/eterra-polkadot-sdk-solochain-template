# Architecture

## Trust boundary

```text
visual builder / TypeScript SDK / Rust compiler
        │ deterministic JSON → SCALE
        ▼
unsigned publish plan
        │ external wallet or operator signs
        ▼
FRAME pallet validates bytes, ownership, bounds, state, nonce, providers
        │
        ├── authority provider
        ├── economy provider
        └── profile provider
```

The manifest compiler improves authoring feedback. It is never consensus.
Finalization hashes the exact SCALE bytes and the runtime decodes and validates
them again.

## Crate roles

`blockchainia-flow-manifest` owns the stable authoring model and the unbounded
off-chain mirror of the v0 SCALE wire contract. `pallet-blockchainia-flow` owns
bounded on-chain storage and execution. Locked fixtures prove the two encodings
remain equal.

`blockchainia-flow-core` is an in-memory deterministic interpreter for fast
behavior tests and integrations that need previews without a chain. It follows
the same priority and rollback rules, but it is not an authorization source.

`blockchainia-flow-manifest-wasm` exposes a string-only bridge so browser
applications do not duplicate Rust validation logic. The TypeScript SDK also
implements the locked v0 codec for typed clients and cross-language fixtures.

## Runtime state

The pallet owns game namespaces, immutable finalized versions, pinned
instances, actor nonces, attested-event sequences, replay hashes, variable
values, machine state, and bounded per-instance inventory.

Manifest/IPFS metadata is descriptive. Critical live state remains runtime
authoritative.

