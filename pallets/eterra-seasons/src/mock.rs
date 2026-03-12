use crate as pallet_eterra_seasons;
use frame_support::derive_impl;

use frame_support::parameter_types;
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test
    {
        System: frame_system,
        Seasons: pallet_eterra_seasons,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
}

parameter_types! {
    pub const MaxSeasonNameLen: u32 = 64;
    pub const MaxSeasonDescLen: u32 = 256;
}

impl pallet_eterra_seasons::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = frame_system::EnsureRoot<u64>;
    type MaxSeasonNameLen = MaxSeasonNameLen;
    type MaxSeasonDescLen = MaxSeasonDescLen;
    type SeasonActivationValidator = ();
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_eterra_seasons::GenesisConfig::<Test>::default()
        .assimilate_storage(&mut storage)
        .unwrap();
    storage.into()
}
