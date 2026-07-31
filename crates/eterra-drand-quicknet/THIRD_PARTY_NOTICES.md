# Third-party notices

This verifier implements the public `bls-unchained-g1-rfc9380` verification
contract documented by the drand project.

The interoperability test vector for Quicknet round 123 is published by
`noislabs/drand-verify`, Apache-2.0:

- https://github.com/noislabs/drand-verify
- public key and round-123 signature retrieved from the drand Quicknet API

Cryptographic arithmetic is provided by:

- `zkcrypto/bls12_381` 0.8.0, licensed MIT OR Apache-2.0
- `zkcrypto/pairing` 0.23.0, licensed MIT OR Apache-2.0
- RustCrypto `sha2` 0.9.9, licensed MIT OR Apache-2.0

This internal candidate is fail-closed behind the runtime's external
cryptography-review flag. Inclusion and passing vectors do not constitute an
external security review.
