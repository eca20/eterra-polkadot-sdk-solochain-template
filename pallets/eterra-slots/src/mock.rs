use crate as pallet_eterra_slots;
use frame_support::{
    parameter_types,
    traits::{ConstU32, ConstU64, ConstU8},
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage, // ✅ Fixes `build_storage` error
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub struct Test {
        System: frame_system,
        EterraSlots: pallet_eterra_slots,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250; // ✅ Changed to `u64` instead of `ConstU32`
    pub const MaxAttempts: u8 = 3; // Max attempts per card
    pub const CardsPerPack: u8 = 5; // Number of cards per pack
    pub const MaxPacks: u32 = 10; // Maximum packs a player can have
    pub const RandomnessSeed: u64 = 42;
}

impl system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type RuntimeEvent = RuntimeEvent;
    type Block = Block;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();

    // ✅ Fixed missing trait items:
    type Nonce = u64;
    type RuntimeTask = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();

    // ✅ Corrected BlockHashCount
    type BlockHashCount = ConstU64<250>;
}

impl pallet_eterra_slots::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RandomnessSeed = RandomnessSeed;
    type MaxAttempts = ConstU8<3>;
    type CardsPerPack = ConstU8<5>;
    type MaxPacks = ConstU32<10>;
}

// ✅ Explicitly specify `Test` in `GenesisConfig`
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    storage.into()
}
