# pallet-eterra-media

Path: `pallet/eterra-media`

Immutable media registry pallet for Eterra and other indie games. Designed to
pair cleanly with `pallet-nfts` and game pallets that reference `MediaId` for
artwork, audio, skins, etc.

## Wiring into your runtime

In `runtime/Cargo.toml` add:

```toml
[dependencies.pallet-eterra-media]
default-features = false
path = "../pallet/eterra-media"
```

And in `runtime/src/lib.rs`:

```rust
impl pallet_eterra_media::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxUriLen = ConstU32<256>;
    type MaxContentTypeLen = ConstU32<64>;
    type MaxNameLen = ConstU32<64>;
    type MaxDescriptionLen = ConstU32<256>;
    type MaxRolesPerAccount = ConstU32<8>;
    type DefaultCollectionId = ConstU32<0>;
}

construct_runtime!(
    pub enum Runtime where
        // ...
    {
        // ...
        EterraMedia: pallet_eterra_media,
    }
);
```

You can then:

1. Create a collection via `create_collection`.
2. Or enable genesis creation of collection `DefaultCollectionId` (0) using the
   pallet's `GenesisConfig`.
3. Register immutable media via `register_media`, passing `None` for the
   collection ID to use the default (0), or a specific collection ID.
