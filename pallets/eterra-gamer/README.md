# pallet-eterra-gamer

Gamer profile + progression pallet:
- Gamer tag (bounded bytes)
- Avatar CID (bounded ASCII bytes)
- Experience minting (privileged origin)
- Level redemption (0..=99)
- SteamHash to AccountId linking with short-lived authority signatures
- Player freeze controls for linked accounts

## Storage

- `GamerTag: AccountId -> BoundedVec<u8, MaxTagLen>`
- `AvatarCid: AccountId -> BoundedVec<u8, MaxAvatarCidLen>`
- `Experience: AccountId -> u128`
- `Level: AccountId -> u8`
- `SteamToAccount: SteamHash -> AccountId`
- `AccountToSteam: AccountId -> SteamHash`
- `GamerProfiles: AccountId -> GamerProfile`
- `UsedSteamLinkNonces: SteamLinkNonce -> ()`
- `SteamLinkAuthority: Option<sr25519::Public>`

## Calls

- `set_steam_link_authority(sr25519::Public)`
  - Privileged (`AdminOrigin`).
- `link_steam(steam_hash, nonce, expires_at, authority_signature)`
  - Signed by the player wallet account.
  - Verifies a bridge-issued sr25519 authorization over
    `eterra:gamer:steam-link:v1`, account, SteamHash, nonce, and expiry.
- `unlink_steam()`
  - Removes the caller's SteamHash link and profile link metadata.
- `freeze_player(AccountId, ReasonHash)`
  - Privileged (`AdminOrigin`).
- `unfreeze_player(AccountId)`
  - Privileged (`AdminOrigin`).
- `set_gamer_tag(BoundedVec<u8, MaxTagLen>)`
  - First set is free.
  - Later changes charge `ChangeFee` and transfer it to `FaucetAccount`.
  - Linked Steam accounts may set the first tag even without AlphaAccess.
- `set_avatar(BoundedVec<u8, MaxAvatarCidLen>)`
  - Must be printable ASCII CID bytes.
  - First set is free.
  - Later changes charge `ChangeFee` and transfer it to `FaucetAccount`.
  - Linked Steam accounts may set the first avatar even without AlphaAccess.
- `grant_experience(AccountId, u128)`
  - Privileged (`ExpIssuerOrigin`).
- `redeem_levels()`
  - Converts available XP to levels until XP is insufficient or level 99 is reached.

## Events

- `SteamLinkAuthoritySet { authority }`
- `SteamLinked { steam_hash, account }`
- `SteamUnlinked { steam_hash, account }`
- `PlayerFrozen { account, reason_hash }`
- `PlayerUnfrozen { account }`
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
    type AdminOrigin = PrivilegedControlOrigin;
    type FaucetAccount = TreasuryAccount;
    type ChangeFee = GamerChangeFee;
    type MaxTagLen = GamerTagMaxLen;
    type MaxAvatarCidLen = AvatarCidMaxLen;
    type MaxSteamLinkSignatureLen = SteamLinkSignatureMaxLen;
    type WeightInfo = pallet_eterra_gamer::weights::SubstrateWeight<Runtime>;
}
```

`PrivilegedControlOrigin` is currently runtime-mode dependent:
- default/testnet: `EnsureRoot<AccountId>`
- `runtime-production` feature: `EnsureNever<AccountId>`
