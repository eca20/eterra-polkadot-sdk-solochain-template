use crate as pallet_eterra_slots;
use frame_support::{
    construct_runtime,
    parameter_types,
    traits::{ConstU16, ConstU32}, // Removed ConstU64
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

// Updated construct_runtime to remove the deprecated `where` clause
construct_runtime!(
    pub enum Test {
        System: frame_system,
        SlotMachine: pallet_eterra_slots,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const RandomnessSeed: u64 = 12345; // Example seed for pseudo-randomness
}

impl system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type Nonce = u64;
    type Block = Block; // Updated to use `Block` instead of `Header`
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ConstU16<42>;
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>; // Corrected type for MaxConsumers
    type RuntimeTask = ();
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

impl pallet_eterra_slots::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RandomnessSeed = RandomnessSeed;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    // Explicitly specify the Test runtime for `build_storage`
    let t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    t.into()
}
