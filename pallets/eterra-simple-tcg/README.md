

# Eterra Simple TCG Pallet

The **eterra-simple-tcg** pallet is a minimal Substrate FRAME pallet that implements the basic mechanics required for a trading card game (TCG) on-chain. It provides functionality for minting, owning, and transferring individual cards, as well as querying cards owned by accounts.

---

## Features

### Card Minting
- Users can mint new cards one at a time.
- Each card receives a unique, incrementing `CardId`.
- The minting account becomes the owner of the card.
- Emits the `CardMinted` event upon success.

### Ownership Tracking
- Each card is associated with exactly one owner account.
- Owners have a bounded list of their owned cards.
- The maximum number of cards per owner is limited by the constant `OwnedLimit`.

### Transfer of Cards
- Owners can transfer cards to other accounts.
- Transfers update both sender and receiver ownership lists.
- Emits the `CardTransferred` event upon success.
- Prevents transfers if:
  - The sender is not the card owner.
  - The recipient’s ownership list is already at capacity.

### Querying
- The pallet provides storage access to:
  - `Cards`: mapping of `CardId` to owner account.
  - `OwnedCards`: mapping of `AccountId` to a bounded vector of `CardId`.
- Off-chain logic or RPCs can query all cards owned by an address efficiently.

---

## Events

- **CardMinted**: Emitted when a new card is minted.
  ```rust
  CardMinted { player: AccountId, card_id: CardId }
  ```

- **CardTransferred**: Emitted when a card is transferred between accounts.
  ```rust
  CardTransferred { from: AccountId, to: AccountId, card_id: CardId }
  ```

---

## Errors

- **CardNotFound**: Attempted to transfer or access a non-existent card.
- **NotCardOwner**: Attempted to transfer a card without being the owner.
- **OwnedListFull**: Attempted to mint or receive a card when already at the ownership limit.

---

## Constants

- **OwnedLimit**: Maximum number of cards an account can own at once (currently set to 600).

---

## Limitations

- No support for card packs (minting is strictly one card at a time).
- No card metadata or attributes (only unique identifiers).
- No battle, deck, or game logic included — this pallet only manages ownership.
- Hard-coded maximum ownership limit (`OwnedLimit`) — not configurable at runtime.
- Does not implement economic mechanisms (e.g., costs for minting or transferring).

---

## Testing

The pallet includes unit tests that cover:
- Minting a single card.
- Ensuring `CardMinted` event is emitted.
- Transferring a card between two accounts.
- Preventing invalid transfers (wrong owner, full ownership list, or non-existent card).
- Querying multiple owners for correctness.

---

## Future Improvements

- Add metadata (e.g., name, rarity, element, stats).
- Support minting multiple cards in a batch (packs).
- Add runtime configuration for ownership limits.
- Integrate with additional pallets for gameplay mechanics.
- Add benchmarking for accurate weight calculation.