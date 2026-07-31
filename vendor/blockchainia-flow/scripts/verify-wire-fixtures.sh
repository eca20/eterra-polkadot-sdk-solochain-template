#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

test "$(wc -c < fixtures/wire/v0/zelda-door.scale.hex | tr -d ' ')" = "305"
test "$(wc -c < fixtures/wire/v0/arcade-credit-run.scale.hex | tr -d ' ')" = "245"
test "$(wc -c < fixtures/wire/v0/season-pass-reward.scale.hex | tr -d ' ')" = "269"
test "$(wc -c < fixtures/wire/v0/dungeon-run.scale.hex | tr -d ' ')" = "411"
test "$(wc -c < fixtures/wire/v0/fps-attested-result.scale.hex | tr -d ' ')" = "175"

node ./scripts/verify-runtime-contract.mjs
cargo test -p blockchainia-flow-manifest locked_wire_fixtures_compile_byte_for_byte
cargo test -p blockchainia-flow-manifest permanent_eterra_alias_compiles_identically
