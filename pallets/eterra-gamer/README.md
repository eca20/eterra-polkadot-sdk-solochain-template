# pallet-eterra-gamer

A minimal Substrate pallet that handles:
- GamerTag (UTF-8 bytes, bounded). First set free; later changes cost a fee.
- Avatar CID (IPFS/multibase as ASCII bytes, bounded). First set free; later changes cost a fee.
- Experience + Leveling (0..=99). `redeem_levels` consumes unredeemed EXP using a deterministic curve
  whose total from level 1..99 is ≈ 1,000,000,000 EXP (L1=250; L≥2 uses `250 + round(k*(L^2-1))`).

## Storage
- `GamerTag: map AccountId -> BoundedVec<u8, MaxTagLen>`
- `AvatarCid: map AccountId -> BoundedVec<u8, MaxAvatarCidLen>` (validated as printable ASCII)
- `Experience: map AccountId -> u128`
- `Level: map AccountId -> u8`

## Calls
- `set_gamer_tag(Vec<u8>)` — first set free; later changes transfer `ChangeFee` to `FeePalletId` account.
- `set_avatar(Vec<u8>)` — CID must be printable ASCII; first set free; later changes transfer `ChangeFee`.
- `grant_experience(AccountId, u128)` — privileged; mints EXP to `Experience`.
- `redeem_levels()` — converts available EXP to level(s) until 99 or XP exhausted.

## Events
- `TagSet { who, tag, charged }`
- `AvatarSet { who, cid, charged }`
- `ExperienceGranted { to, amount }`
- `LevelUp { who, new_level }`

## Errors
- `TagTooShort`, `TagTooLong`
- `AvatarCidTooLong`, `AvatarCidInvalidAscii`
- `AlreadyMaxLevel`, `NotEnoughExperience`, `InvalidLevelRequest`
- `InsufficientBalanceForChange`

## Runtime wiring (snippet)
```rust
use pallet_eterra_gamer;

parameter_types! {
    pub const GamerTagMaxLen: u32 = 32;
    pub const AvatarCidMaxLen: u32 = 96;
    pub const GamerChangeFee: Balance = 100u128.saturating_into();
    pub const GamerFeePalletId: PalletId = PalletId(*b"etr:gmer");
}

impl pallet_eterra_gamer::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ExpIssuerOrigin = frame_system::EnsureRoot<AccountId>;
    type FeePalletId = GamerFeePalletId;
    type ChangeFee = GamerChangeFee;
    type MaxTagLen = GamerTagMaxLen;
    type MaxAvatarCidLen = AvatarCidMaxLen;
}
```
Add to `construct_runtime!`:
```rust
EterraGamer: pallet_eterra_gamer,
```

## Notes on CIDs
- We store **ASCII bytes** only and reject spaces/control characters to prevent malformed inputs.
- Typical CIDs (v0 `Qm...`, v1 multibase like `bafy...`) are supported as-is.
- Frontend should send bytes of the CID string (`TextEncoder().encode(cid)`).

## License
Apache-2.0
