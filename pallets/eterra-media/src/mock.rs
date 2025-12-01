use crate as pallet_eterra_media;

use frame_support::{
    construct_runtime, parameter_types,
    traits::{Everything, Get, BuildGenesisConfig},
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};

pub type AccountId = u64;
pub type BlockNumber = u64;

// Basic mock types.
type UncheckedExtrinsic = system::mocking::MockUncheckedExtrinsic<Test>;
type Block = system::mocking::MockBlock<Test>;

construct_runtime!(
    pub enum Test
    where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: system,
        EterraMedia: pallet_eterra_media,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;

    pub const MaxUriLen: u32 = 256;
    pub const MaxContentTypeLen: u32 = 64;
    pub const MaxNameLen: u32 = 64;
    pub const MaxDescriptionLen: u32 = 256;
    pub const MaxRolesPerAccount: u32 = 8;
    pub const DefaultCollectionId: u32 = 0;
}

// Default collection owner used in tests.
pub struct DefaultCollectionOwnerForMock;
impl Get<AccountId> for DefaultCollectionOwnerForMock {
    fn get() -> AccountId {
        1 // arbitrary "admin" account in tests
    }
}

impl system::Config for Test {
    type BaseCallFilter = Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type Block = Block;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    // Use ConstU32 here so it implements ConsumerLimits in this SDK version.
    type MaxConsumers = frame_support::traits::ConstU32<16>;
    // Additional required associated types in this frame-system version:
    type Nonce = u64;
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
    type RuntimeTask = ();
}

impl pallet_eterra_media::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxUriLen = MaxUriLen;
    type MaxContentTypeLen = MaxContentTypeLen;
    type MaxNameLen = MaxNameLen;
    type MaxDescriptionLen = MaxDescriptionLen;
    type MaxRolesPerAccount = MaxRolesPerAccount;
    type DefaultCollectionId = DefaultCollectionId;
    type DefaultCollectionOwner = DefaultCollectionOwnerForMock;
}

// Helper to build a fresh Ext for each test.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let storage = system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame-system storage build should not fail");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        // If you later want to create the default collection at genesis,
        // you can call EterraMedia::something here.
    });
    ext
}

/// Helper that builds a TestExternalities where the media pallet
/// creates its default collection at genesis.
pub fn new_test_ext_with_default_collection() -> sp_io::TestExternalities {
    let mut storage = system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame-system storage build should not fail");

    pallet_eterra_media::GenesisConfig::<Test> {
        create_default_collection: true,
        default_collection_name: b"Default Media".to_vec(),
        default_collection_description: b"Default media collection".to_vec(),
        default_collection_owner: None, // falls back to DefaultCollectionOwnerForMock (account 1)
    }
    .assimilate_storage(&mut storage)
    .expect("pallet_eterra_media genesis config assimilation should not fail");

    sp_io::TestExternalities::new(storage)
}