
# Suggested Tests for Pallet Eterra Gameplay

This document outlines additional tests that could enhance the coverage and robustness of the `pallet-eterra` test suite. These tests address various edge cases and scenarios to ensure fully validated gameplay.

---

## **2. Capturing Cards in All Directions**
**Description**:  
Test that a card correctly captures opponent cards in all four directions when its ranks are higher.

**Functionality**:
- Place opponent cards in positions above, below, left, and right of the creator’s move.
- Place a creator card with ranks higher in all directions.
- Verify that all adjacent opponent cards are captured.

**Rationale**:  
Validates the full functionality of the card capture mechanics.

---

## **3. Invalid Game ID**
**Description**:  
Test that attempting to play or interact with a non-existent game results in an appropriate error.

**Functionality**:
- Attempt to make a move on a game ID that does not exist.
- Ensure the call fails with the `GameNotFound` error.

**Rationale**:  
Ensures robust handling of invalid game IDs, preventing unintended behavior or crashes.

---

## **4. Turn Timeout (If Applicable)**
**Description**:  
Test that a player loses their turn (or the game) if they exceed a time limit for making a move.

**Functionality**:
- Simulate a scenario where a player exceeds the allowed turn duration.
- Verify that the appropriate penalty or action (e.g., forfeiting the turn) is applied.

**Rationale**:  
Ensures time-based constraints are respected if such a feature exists.

---

## **5. Score Calculation**
**Description**:  
Test that the game calculates the final score correctly based on captured cards or other rules.

**Functionality**:
- Simulate a completed game with various captured cards for each player.
- Verify that the calculated scores match the expected results.

**Rationale**:  
Validates the scoring logic to ensure accurate game outcomes.

---

## **6. Invalid Card Placement**
**Description**:  
Test that players cannot place cards in positions that violate specific placement rules (e.g., placing a card not adjacent to any other card).

**Functionality**:
- Attempt to place a card in an invalid position based on game rules.
- Ensure the call fails with the appropriate error.

**Rationale**:  
Ensures spatial placement rules are consistently enforced.

---

## **7. Edge and Corner Capture Scenarios**
**Description**:  
Test card capture mechanics specifically at the edges and corners of the board.

**Functionality**:
- Simulate scenarios where cards are placed and captured at board edges or corners.
- Verify that the capture logic handles these cases correctly.

**Rationale**:  
Edge and corner cases may behave differently due to fewer neighbors, so they require explicit validation.

---

## **8. Replay Functionality**
**Description**:  
If the game supports replay or restarting, test that players can replay or restart a game properly.

**Functionality**:
- Create a game and simulate some moves.
- Restart the game.
- Verify that the board resets and the game state is cleared.

**Rationale**:  
Ensures proper handling of replays or resets without residual state issues.

---

## **9. Player Disconnect (If Applicable)**
**Description**:  
Test the handling of a player leaving or disconnecting mid-game.

**Functionality**:
- Simulate a player leaving during the game.
- Verify that the game transitions to an appropriate state (e.g., forfeit, pause, or AI takeover).

**Rationale**:  
Validates game behavior under unexpected player disconnections.

---

## **10. Invalid Card Attributes**
**Description**:  
Test that only valid cards with proper attributes (e.g., ranks within a specific range) can be played.

**Functionality**:
- Attempt to play cards with invalid attributes (e.g., negative ranks or ranks exceeding the allowed maximum).
- Verify that the call fails with an appropriate error.

**Rationale**:  
Ensures input validation for card attributes.

---

## **11. Concurrent Games**
**Description**:  
Test that multiple games can run concurrently without interfering with each other.

**Functionality**:
- Create two or more games with different players.
- Simulate moves in all games.
- Ensure that game states are isolated and do not affect one another.

**Rationale**:  
Validates multi-game handling and state isolation.

---

## **12. Game Cancellation**
**Description**:  
Test that a player can cancel a game before it starts (if supported).

**Functionality**:
- Create a game and cancel it before any moves are made.
- Verify that the game state is cleared and no moves can be made.

**Rationale**:  
Ensures proper handling of game cancellations.

---

## **13. Error Resilience**
**Description**:  
Test that invalid actions (e.g., duplicate moves, invalid inputs) do not crash the system or corrupt the game state.

**Functionality**:
- Perform sequences of invalid actions.
- Verify that the system gracefully handles errors and maintains a valid state.

**Rationale**:  
Validates overall system robustness and error handling.

---

By adding these tests, the gameplay mechanics of `pallet-eterra` can be validated under a broader range of scenarios, ensuring robustness and correctness.
