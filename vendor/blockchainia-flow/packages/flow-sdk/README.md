# `@blockchainia/flow-sdk`

Typed, keyless authoring and transaction-preparation helpers for Flow v0.
Compilation is delegated to the Rust/WASM compiler so JavaScript does not
reimplement the locked SCALE codec.

The SDK never accepts a seed phrase or private key, never signs, and never
submits an extrinsic. Consumers must review and pass prepared calls to their own
wallet or operator integration.

It also exposes typed storage-read descriptors through `flowState` and
normalizes exact-metadata-decoded runtime events through `decodeFlowEvent`.
`readFlowState` accepts an application-owned adapter, so the package does not
force a specific Substrate RPC library or retain credentials.
