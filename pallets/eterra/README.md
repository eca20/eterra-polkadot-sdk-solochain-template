# Eterra Pallet Integration Guide

This document reflects the current `pallet-eterra` interface used by the runtime.

## Gameplay Flow

1. Player configures hand:
   - `set_current_hand(card_ids)`
   - `set_preset_hand(card_ids)` (deprecated alias)
2. Player creates game:
   - `create_game(players, game_mode)`
3. Player submits in-game hand snapshot:
   - `submit_hand(game_id, card_ids_compat)`
4. Player takes turns:
   - `play_from_hand(game_id, hand_index, x, y)`
   - optional legacy move path: `play(game_id, move)`
5. If opponent stalls beyond block limit:
   - `force_finish_turn(game_id)`

## Call Signatures (Current)

- `create_game(origin, players: BoundedVec<AccountId, NumPlayers>, game_mode: GameMode)`
  - `GameMode::PvP`: caller must be in `players`, exactly two distinct players.
  - `GameMode::PvE`: players input is normalized to `[caller, AiAccount]`.
- `set_current_hand(origin, card_ids: BoundedVec<u32, HandLimit>)`
  - Requires exact `HandSize` and unique cards owned by caller.
- `submit_hand(origin, game_id, _card_ids: BoundedVec<u32, HandLimit>)`
  - Compatibility argument is ignored.
  - Hand is loaded from `CurrentHandOf`.

## Integration Notes

- Ensure each human player has `set_current_hand` completed before calling `create_game`.
- For PvP, both players need active hand setups.
- For PvE, AI hand generation and AI turns are handled by pallet internals.
- Frontends should treat `set_preset_hand` as backward-compatible alias and prefer `set_current_hand`.

## Runtime Wiring (current shape)

```rust
impl pallet_eterra::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type NumPlayers = EterraNumPlayers; // currently 2
    type MaxRounds = EterraMaxRounds;
    type BlocksToPlayLimit = EterraBlocksToPlayLimit;
    type HandSize = ConstU32<5>;
    type AiAccount = AiBotAccountParam;
    type AiDifficulty = ConstU8<60>;
    type WeightInfo = pallet_eterra::weights::SubstrateWeight<Runtime>;
}
```
