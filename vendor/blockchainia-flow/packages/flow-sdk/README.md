# `@blockchainia/flow-sdk`

Typed, keyless authoring and transaction-preparation helpers for Flow v0.
Compilation is delegated to the Rust/WASM compiler so JavaScript does not
reimplement the locked SCALE codec.

The SDK never accepts a seed phrase or private key, never signs, and never
submits an extrinsic. Consumers must review and pass prepared calls to their own
wallet or operator integration.
