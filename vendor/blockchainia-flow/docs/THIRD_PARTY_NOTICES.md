# Third-party notices

This alpha uses third-party packages under their own licenses. The dependency
lockfiles are the authoritative version inventory.

| Ecosystem | Package families | Expected license family |
|---|---|---|
| Rust | parity-scale-codec, scale-info | Apache-2.0 / MIT |
| Rust | FRAME, Substrate `sp-*` crates | Apache-2.0 |
| Rust | serde, serde_json, hex, base64, blake2, wasm-bindgen | MIT or Apache-2.0/MIT |
| JavaScript tooling | TypeScript, Node type declarations, Vite | Apache-2.0 / MIT |

No dependency is relicensed as MIT-0. Its upstream license and notices continue
to apply. Before public distribution, generate a complete transitive report
from `Cargo.lock` and `package-lock.json`, review every `UNKNOWN` or nonpermissive
entry, and include required notices.
