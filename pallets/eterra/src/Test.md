# Pallet Eterra Test Suite

This README describes the purpose, functionality, and validity of each test in the `pallet-eterra` test suite. These tests verify the functionality and robustness of the game mechanics implemented in the pallet.

---

## **Test: `create_game_works`**
**Description**: 
Verifies that a new game can be successfully created with a unique game ID.

**Functionality**:
1. Calls `setup_new_game` to create a game between two players.
2. Checks that the game creator and opponent IDs match the expected players.
3. Ensures the board is empty at initialization and the turn order is set correctly.

**Validity**:
This test is valid because it ensures that the core function of creating a game works as intended and initializes the game state correctly.

---

## **Test: `create_game_with_same_players_fails`**
**Description**: 
Ensures that a game cannot be created where the creator and opponent are the same player.

**Functionality**:
1. Attempts to create a game with the same player as both creator and opponent.
2. Expects the call to fail with the `InvalidMove` error.

**Validity**:
This test validates that the game logic enforces a rule requiring two distinct players to start a game.

---

## **Test: `play_turn_works`**
**Description**: 
Confirms that players can take turns placing cards on the board.

**Functionality**:
1. Plays one turn for the creator and one for the opponent.
2. Validates that the cards are placed on the board in the correct positions.
3. Ensures that the turn alternates between players.

**Validity**:
This test ensures that basic turn-taking mechanics and card placement functionality operate as expected.

---

## **Test: `invalid_move_on_occupied_cell`**
**Description**: 
Checks that a player cannot place a card on an already occupied cell.

**Functionality**:
1. The creator places a card in a specific cell.
2. The opponent attempts to place another card in the same cell.
3. Verifies that the call fails with the `CellOccupied` error.

**Validity**:
This test validates the rule preventing overlapping moves on the board.

---

## **Test: `play_turn_out_of_bounds_fails`**
**Description**: 
Ensures that a card cannot be placed outside the boundaries of the board.

**Functionality**:
1. The creator attempts to place a card at an invalid board coordinate.
2. Expects the call to fail with the `InvalidMove` error.

**Validity**:
This test enforces spatial constraints, ensuring that all moves are within the 4x4 board.

---

## **Test: `card_capture_multiple_directions`**
**Description**: 
Confirms that a placed card can capture adjacent cards in multiple directions when its ranks are higher.

**Functionality**:
1. Simulates a scenario where the creator captures cards placed by the opponent in two directions.
2. Validates the board state to ensure that the capturing card replaces the opponent’s cards.

**Validity**:
This test ensures the correctness of the card capture logic, a core mechanic of the game.

---

## **Test: `full_game_simulation`**
**Description**: 
Simulates a complete game between two players, alternating moves until the board is partially filled.

**Functionality**:
1. Simulates a sequence of valid moves by both players.
2. Ensures that the moves are processed correctly and the board reflects the expected state after each turn.
3. Validates that the number of non-empty cells matches the total number of moves.

**Validity**:
This test verifies end-to-end functionality, ensuring that a game can proceed without errors under normal gameplay.

---

## **Test: `play_out_of_turn_fails`**
**Description**:  
Ensures that a player cannot make a move when it is not their turn.

**Functionality**:  
1. The creator makes a valid move.  
2. The creator attempts to play again immediately, skipping the opponent's turn.  
3. Verifies that the call fails with the `NotYourTurn` error.

**Validity**:  
This test enforces the game's turn-based structure, ensuring that each player alternates turns and that turn order is strictly followed.

---
## **Logging**
All tests use a custom logger to provide detailed information about the game state and events, such as:
- Game creation details.
- Player moves and their outcomes.
- Board state after each move.

This logging enhances test traceability and debugging capabilities.

---

## **Conclusion**
These tests collectively validate the fundamental game mechanics of `pallet-eterra`, including game creation, turn-taking, spatial rules, and card capturing logic. They ensure that the game operates correctly under various scenarios, maintaining robustness and preventing invalid actions.
