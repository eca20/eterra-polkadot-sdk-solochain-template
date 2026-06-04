# pallet-cryptostrike

This crate is the PI-01 runtime contract scaffold for Crypto-Strike.

It defines the planned FRAME pallet boundary for:

- Crypto-Strike settlement and pending SteamHash Guap claims
- server registry and staking
- active server/session roster
- wallet-granted server allowances
- round settlement batches
- same-server Guap transfers
- pending Guap claims
- season ladder stats

Implemented runtime calls through PI-09:

- `claim_pending_guap`
- `register_server`
- `increase_server_stake`
- `request_unstake`
- `finalize_unstake`
- `heartbeat`
- `set_server_status`
- `slash_server`
- `authorize_server_allowance`
- `revoke_server_allowance`
- `set_session_roster_root`
- `upsert_active_player`
- `remove_active_player`
- `submit_round_settlement`
- `start_season`
- `end_season`

The `NotImplemented` error is retained for future expansion, but the scaffolded dispatchables now have concrete behavior.

Note: SteamHash to wallet/profile linking is owned by `pallet-eterra-gamer`.
CryptoStrike resolves linked accounts and frozen state through its configured
identity provider and keeps `PendingGuapClaims<SteamHash>` for reward escrow.

Note: server stake is reserved, released, and slashed through the `StakeLedger` trait. Unit tests use a mock ledger; a later node/runtime integration PI should back this with the canonical stake asset implementation.

Note: server allowances are recorded as wallet-granted spend ceilings for active servers. Accepted settlements consume allowance for weapon spends and same-session transfers, but allowance does not reserve Guap before settlement.

Note: active session roster entries are server-owner updated and support linked accounts plus unlinked SteamHashes. Roster roots are stored for later settlement auditability but are not cryptographically verified yet.

Note: round settlement currently validates server ownership/status/stake, server signature through `ServerSignatureVerifier`, duplicate round IDs, roster root consistency, active participants, duplicate transfer nonces, allowance, and Guap balance effects. It records settled rounds/nonces, burns weapon spends, transfers linked Guap, mints linked rewards, creates pending claims for unlinked SteamHashes, emits settlement economy events, and updates active season stats for linked accounts. Spends and transfers draw from the pre-settlement ledger; new rewards become usable after settlement.

Note: Guap is accessed through the `GuapLedger` trait. Unit tests use a mock ledger; a later node/runtime integration PI should back this with the canonical Guap asset implementation.

Note: admin freeze controls are implemented by `pallet-eterra-gamer`. Frozen
accounts cannot authorize new server allowances or participate in accepted
settlement paths through the CryptoStrike identity provider.
