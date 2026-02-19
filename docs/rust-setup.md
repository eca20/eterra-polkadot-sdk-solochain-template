# Rust Setup For This Repository

This repository is configured for the **stable** Rust toolchain.

Source of truth:
- `env-setup/rust-toolchain.toml`

## Required toolchain

```bash
rustup toolchain install stable
rustup default stable
rustup target add wasm32-unknown-unknown --toolchain stable
```

## Verify

```bash
rustup show
rustc --version
cargo --version
```

You should see stable as the active/default toolchain, and `wasm32-unknown-unknown` installed.

## System dependencies

### Ubuntu/Debian

```bash
sudo apt update
sudo apt install -y git clang curl libssl-dev llvm libudev-dev pkg-config cmake protobuf-compiler
```

### macOS

```bash
brew update
brew install openssl llvm cmake protobuf
```

## Build checks

### Runtime + node (default/testnet mode)

```bash
cargo check -p solochain-eterra-runtime --features runtime-benchmarks
cargo check -p solochain-eterra-node
```

### Runtime + node (production origin policy)

```bash
cargo check -p solochain-eterra-runtime --features "runtime-benchmarks,runtime-production"
cargo check -p solochain-eterra-node --features runtime-production
```

## Notes

- Nightly is **not required** for normal development in this repository.
- If you are developing against upstream SDK internals outside this workspace, nightly requirements may differ.
