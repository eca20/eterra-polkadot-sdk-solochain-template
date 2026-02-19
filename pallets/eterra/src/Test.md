# pallet-eterra Test Coverage Overview

This file summarizes the current intent of the `pallet-eterra` unit tests.
Source of truth is always `pallets/eterra/src/tests.rs`.

## Covered Areas

- Game creation
  - Valid PvP/PvE creation paths
  - Invalid player counts and duplicate-player rejection
  - Requirement that players have preset/current hands
  - Active-game constraints
- Turn and move logic
  - Valid move placement and turn rotation
  - Out-of-turn and out-of-bounds rejection
  - Occupied-cell rejection
  - Capture logic in multiple directions
- Hand-based gameplay path
  - `submit_hand` validation
  - Hand index range checks
  - Used-card prevention
  - Card ownership and existence checks
- Force-finish flow
  - Block-based timeout validation
  - Caller eligibility checks
  - Turn advancement behavior
- PvE behavior
  - AI hand generation
  - AI suggestion and move execution path
  - Multiple PvE game isolation
- Game lifecycle
  - Winner detection
  - End-state cleanup and event checks
  - Unknown game ID error handling

## How To Run

```bash
cargo test -p pallet-eterra
```
