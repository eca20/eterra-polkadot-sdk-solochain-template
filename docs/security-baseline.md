# Security Baseline (Phase 1)

Date: 2026-02-18  
Scope: custom pallets in this repo and runtime wiring under `runtime/`.

## Goal

Establish a minimum production-grade baseline for:

1. Origin/access control correctness.
2. Spam/DoS resistance at extrinsic decode and execution layers.
3. A repeatable checklist for testnet and production preflight.

## Hardening Applied In This Baseline

1. Enforced signed origin for `process_queue` in matchmaker.
2. Bounded work per matchmaker processing call to one pair per extrinsic.
3. Replaced unbounded signed extrinsic inputs with bounded input types:
   - `pallet-eterra-gamer`:
     - `set_gamer_tag(BoundedVec<u8, MaxTagLen>)`
     - `set_avatar(BoundedVec<u8, MaxAvatarCidLen>)`
   - `pallet-eterra`:
     - `create_game(..., BoundedVec<AccountId, NumPlayers>, ...)`
     - `submit_hand(..., BoundedVec<u32, HandLimit>)`
     - `set_current_hand(BoundedVec<u32, HandLimit>)`
     - `set_preset_hand(BoundedVec<u32, HandLimit>)`
   - `pallet-eterra-daily-slots`:
     - `set_reel_weights(..., BoundedVec<(u32, u32), MaxWeightEntries>)`
     - `set_all_reel_weights(..., BoundedVec<(u32, BoundedVec<(u32, u32), MaxWeightEntries>), MaxSlotLength>)`
   - `pallet-eterra-media`:
     - `create_collection(..., BoundedVec name, BoundedVec description)`
     - `register_media(..., BoundedVec uri, BoundedVec content_type, ...)`
4. Updated impacted benchmarks and tests to compile and pass with bounded args.

## Runtime Origin Matrix (Testnet vs Production)

1. Runtime origin selector
   - Default mode (testnet/dev): `PrivilegedControlOrigin = EnsureRoot<AccountId>`.
   - Production mode (`runtime-production` feature): `PrivilegedControlOrigin = EnsureNever<AccountId>`.
2. Pallets wired to `PrivilegedControlOrigin`
   - `pallet-eterra-game-authority::AdminOrigin`
   - `pallet-eterra-gamer::ExpIssuerOrigin`
   - `pallet-node-authorization::{AddOrigin, RemoveOrigin, SwapOrigin, ResetOrigin}`
3. Chain spec policy
   - `dev` / `local_testnet` / `testnet`: Sudo key is set.
   - `production` (`eterra_production`): Sudo key is `None`.
4. Effect
   - In production mode, privileged maintenance paths are intentionally disabled at origin level until governance origins are introduced.
   - In testnet mode, root-controlled operations remain available for iteration speed.

### Build And Run Modes

1. Testnet/default runtime behavior
   - `cargo build -p solochain-eterra-runtime --release --features runtime-benchmarks`
   - `cargo run -p solochain-eterra-node -- --chain testnet`
2. Production runtime behavior (privileged origins disabled)
   - `cargo build -p solochain-eterra-runtime --release --features \"runtime-benchmarks,runtime-production\"`
   - `cargo run -p solochain-eterra-node --features runtime-production -- --chain production`

## Origin/Access Audit (By Extrinsic)

1. `pallet-eterra`
   - Signed: `create_game`, `play`, `submit_hand`, `play_from_hand`, `force_finish_turn`, `set_current_hand`, `set_preset_hand`.
   - Runtime checks: game membership, turn ownership, active game gating.
2. `pallet-eterra-daily-slots`
   - Signed: `roll`.
   - Root: `set_reel_weights`, `set_all_reel_weights`.
3. `pallet-eterra-faucet`
   - Signed: `claim`.
   - Runtime checks: `dest == caller`, per-block throttling, faucet balance checks.
4. `pallet-eterra-game-authority`
   - `AdminOrigin`: `add_server`, `remove_server`.
   - Signed: `create_game`, `add_player`, `add_players_batch`, `create_game_with_batch_add`, `record_eliminations`, `end_game`.
   - Runtime checks: whitelist + server ownership + active-game constraints.
5. `pallet-eterra-gamer`
   - Signed: `set_gamer_tag`, `set_avatar`, `redeem_levels`.
   - `ExpIssuerOrigin`: `grant_experience`.
6. `pallet-eterra-media`
   - Signed: `create_collection`, `set_collection_role`, `register_media`, `freeze_collection`, `deprecate_media`.
   - Runtime checks: collection ownership/admin role checks for sensitive operations.
7. `pallet-eterra-monte-carlo-ai`
   - Signed: `suggest_move`.
8. `pallet-eterra-simple-matchmaker`
   - Signed: `join_queue`, `leave_queue`, `process_queue`.
   - Runtime checks: one queued entry per account and bounded per-call processing work.
9. `pallet-eterra-simple-tcg`
   - Signed: `mint_card`, `transfer_card`, `set_price`, `remove_price`, `buy_card`.
10. `pallet-eterra-tcg`
   - Signed: `mint_pack`, `generate_slot`, `accept_slot`, `transfer_card`.

## Spam/DoS Audit Notes

1. Input decoding bounds
   - High-traffic signed calls now decode bounded vectors, reducing memory/CPU abuse risk.
2. Execution bounds
   - Matchmaker processing now has explicit per-call work cap (one pair).
3. Storage iteration patterns
   - Existing queue/game loops rely on configured upper bounds and now avoid unbounded drain in one dispatch.

## Fee/Spam Baseline

1. Most extrinsics are normal signed transactions and therefore fee-metered by transaction-payment.
2. Additional anti-spam checks:
   - Faucet: one claim per account per block.
   - Gamer: repeat profile updates charge `ChangeFee`.
   - Matchmaker: per-call processing work is capped.

## Residual Risks / Next Hardening Items

1. Faucet anti-spam is per-account; sybil resistance depends on chain-level fees and account economics.
2. Root/admin-call monitoring should be wired into indexer/alerting (for operational visibility, not runtime correctness).
3. Add denylist/allowlist or circuit-breaker playbooks for emergency response in testnet/prod runbooks.

## Verification Performed

1. `cargo check -p solochain-eterra-runtime --features runtime-benchmarks`
2. `cargo build -p solochain-eterra-runtime --release --features runtime-benchmarks`
3. `cargo test -p pallet-eterra`
4. `cargo test -p pallet-eterra-daily-slots`
5. `cargo test -p pallet-eterra-gamer`
6. `cargo test -p pallet-eterra-media`
7. `cargo test -p pallet-eterra-simple-matchmaker`

All commands above passed after the applied changes.
