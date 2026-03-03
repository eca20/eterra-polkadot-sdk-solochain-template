# pallet-eterra-simple-tcg

A compact card ownership + marketplace pallet.

## What It Does

- Mints cards with deterministic directional stats (`north/east/south/west`).
- Tracks card ownership and per-account owned-card index.
- Supports direct transfers.
- Supports simple listing + buying flow.
- Charges a mint fee into a configured faucet/treasury account.

## Calls

- `mint_card()`
  - Signed.
  - Charges `MintFee` from caller to `FaucetAccount`.
- `transfer_card(card_id, to)`
  - Signed owner-only transfer.
  - Automatically unlists the card if it was listed.
- `set_price(card_id, price)`
  - Signed owner-only list/update listing.
- `remove_price(card_id)`
  - Signed owner-only unlist.
- `buy_card(card_id)`
  - Signed buyer call.
  - Transfers payment from buyer to seller, unlists, then transfers ownership.

## Key Storage

- `NextCardId`
- `Cards: CardId -> CardInfo`
- `OwnedCards: AccountId -> BoundedVec<CardId, OwnedLimit>`
- `CardPrices: CardId -> Balance` (optional)
- `ListedByOwner: AccountId -> BoundedVec<CardId, OwnedLimit>`

## Events

- `CardMinted`
- `CardTransferred`
- `CardListed`
- `CardUnlisted`
- `CardBought`

## Runtime Wiring (current shape)

```rust
impl pallet_eterra_simple_tcg::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type MintFee = ConstU128<{ 100 * UNIT }>;
    type FaucetAccount = TreasuryAccount;
    type WeightInfo = pallet_eterra_simple_tcg::weights::SubstrateWeight<Runtime>;
}
```
