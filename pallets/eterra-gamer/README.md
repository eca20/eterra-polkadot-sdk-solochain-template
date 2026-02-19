# pallet-eterra-gamer

Gamer profile + progression pallet:
- Gamer tag (bounded bytes)
- Avatar CID (bounded ASCII bytes)
- Experience minting (privileged origin)
- Level redemption (0..=99)

## Storage

- `GamerTag: AccountId -> BoundedVec<u8, MaxTagLen>`
- `AvatarCid: AccountId -> BoundedVec<u8, MaxAvatarCidLen>`
- `Experience: AccountId -> u128`
- `Level: AccountId -> u8`

## Calls

- `set_gamer_tag(BoundedVec<u8, MaxTagLen>)`
  - First set is free.
  - Later changes charge `ChangeFee` and transfer it to `FaucetAccount`.
- `set_avatar(BoundedVec<u8, MaxAvatarCidLen>)`
  - Must be printable ASCII CID bytes.
  - First set is free.
  - Later changes charge `ChangeFee` and transfer it to `FaucetAccount`.
- `grant_experience(AccountId, u128)`
  - Privileged (`ExpIssuerOrigin`).
- `redeem_levels()`
  - Converts available XP to levels until XP is insufficient or level 99 is reached.

## Events

- `TagSet { who, tag, charged }`
- `AvatarSet { who, cid, charged }`
- `ExperienceGranted { to, amount }`
- `LevelUp { who, new_level }`

## Runtime Wiring (current shape)

```rust
parameter_types! {
    pub const GamerTagMaxLen: u32 = 32;
    pub const AvatarCidMaxLen: u32 = 96;
    pub const GamerChangeFee: Balance = 100u128;
}

impl pallet_eterra_gamer::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ExpIssuerOrigin = PrivilegedControlOrigin;
    type FaucetAccount = TreasuryAccount;
    type ChangeFee = GamerChangeFee;
    type MaxTagLen = GamerTagMaxLen;
    type MaxAvatarCidLen = AvatarCidMaxLen;
    type WeightInfo = pallet_eterra_gamer::weights::SubstrateWeight<Runtime>;
}
```

`PrivilegedControlOrigin` is currently runtime-mode dependent:
- default/testnet: `EnsureRoot<AccountId>`
- `runtime-production` feature: `EnsureNever<AccountId>`
