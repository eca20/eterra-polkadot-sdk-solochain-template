use crate as pallet_eterra_simple_tcg; // or whatever your pallet module is
use frame_support::{
    parameter_types,
    traits::{ConstU32, ConstU64, ConstU8},
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

frame_support::construct_runtime!(
    pub struct Test {
        System: frame_system,
        EterraSimpleTCGConfig: pallet_eterra_simple_tcg,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaxAttempts: u8 = 3;
    pub const CardsPerPack: u8 = 6;
    pub const MaxPacks: u32 = 10;
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

    type Nonce = u64;
    type RuntimeTask = ();
    type MaxConsumers = ConstU32<16>;
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
    type BlockHashCount = ConstU64<250>;
}

impl pallet_eterra_simple_tcg::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RandomnessSeed = RandomnessSeed;
    type MaxAttempts = ConstU8<3>;
    type CardsPerPack = ConstU8<6>;
    type MaxPacks = ConstU32<10>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    storage.into()
}
