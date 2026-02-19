# pallet-eterra-media

Path: `pallets/eterra-media`

Immutable media registry with collections, roles, and delivery metadata.
Designed to pair with gameplay and NFT-style pallets using stable on-chain media references.

## Core Calls

- `create_collection(name, description)`
- `set_collection_role(collection_id, account, role, granted)`
- `register_media(maybe_collection_id, uri, content_type, class, delivery, size_bytes)`
- `freeze_collection(collection_id)`
- `deprecate_media(media_id)`

All calls are signed and enforce collection ownership/role checks where required.

## Runtime Wiring (current shape)

```rust
parameter_types! {
    pub const MaxMediaUriLen: u32 = 256;
    pub const MaxMediaContentTypeLen: u32 = 64;
    pub const MaxMediaNameLen: u32 = 64;
    pub const MaxMediaDescriptionLen: u32 = 256;
    pub const MaxMediaRolesPerAccount: u32 = 8;
    pub const DefaultMediaCollectionId: u32 = 0;
}

impl pallet_eterra_media::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxUriLen = MaxMediaUriLen;
    type MaxContentTypeLen = MaxMediaContentTypeLen;
    type MaxNameLen = MaxMediaNameLen;
    type MaxDescriptionLen = MaxMediaDescriptionLen;
    type MaxRolesPerAccount = MaxMediaRolesPerAccount;
    type DefaultCollectionId = DefaultMediaCollectionId;
    type DefaultCollectionOwner = TreasuryAccount;
    type WeightInfo = pallet_eterra_media::weights::SubstrateWeight<Runtime>;
}
```

## Notes

- `register_media` accepts `None` for collection id and then falls back to `DefaultCollectionId`.
- A default collection can also be created at genesis via the pallet `GenesisConfig`.
