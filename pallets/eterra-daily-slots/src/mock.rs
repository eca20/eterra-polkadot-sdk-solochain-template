#![cfg(test)]

use crate as pallet_eterra_daily_slots;

use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU16, ConstU32, Everything, UnixTime},
};
use frame_system as system;

use sp_core::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};
use std::time::Duration;

// 1. Define a MockTime struct implementing UnixTime
pub struct MockTime;
impl UnixTime for MockTime {
    fn now() -> Duration {
        // e.g. 90000 seconds is ~1 day + 1 hour
        Duration::from_secs(90_000)
    }
}

// 2. Types for Extrinsic and Block
type UncheckedExtrinsic = system::mocking::MockUncheckedExtrinsic<Test>;
type Block = system::mocking::MockBlock<Test>;

// 3. Construct a runtime named `Test`
construct_runtime!(
    pub enum Test {
        System: system,
        EterraDailySlots: pallet_eterra_daily_slots,
    }
);

// -----------------------------------------------------------------------------
//  IMPORTANT: Provide an alias for `TestRuntime` so that tests referencing
//  `TestRuntime` still compile without error (E0412).
// -----------------------------------------------------------------------------
pub type TestRuntime = Test;

// 4. System config
parameter_types! {
    pub const BlockHashCount: u64 = 250;
    // If needed, define other constants here
}

impl system::Config for Test {
    // The same approach as in your `pallet_eterra` mock
    type BaseCallFilter = Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type AccountId = u64;
    type RuntimeCall = RuntimeCall;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Nonce = u64;
    type Block = Block;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type BlockHashCount = BlockHashCount;
    type DbWeight = ();
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ConstU16<42>;
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeTask = ();
    type RuntimeEvent = RuntimeEvent;

    // Additional associated types for newer Substrate versions
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

// 5. Pallet config
parameter_types! {
    pub const MaxSlotLength: u32 = 3;
    pub const MaxOptionsPerSlot: u32 = 5;
    pub const MaxRollsPerRound: u32 = 2;
}

impl pallet_eterra_daily_slots::Config for Test {
    // Same event alias from above
    type RuntimeEvent = RuntimeEvent;
    type TimeProvider = MockTime;
    type MaxSlotLength = MaxSlotLength;
    type MaxOptionsPerSlot = MaxOptionsPerSlot;
    type MaxRollsPerRound = MaxRollsPerRound;
}

// 6. Build test externalities
pub fn new_test_ext() -> sp_io::TestExternalities {
    let storage = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    let mut ext = sp_io::TestExternalities::from(storage);
    ext.execute_with(|| {
        // Optionally set an initial block number or do other setup
        frame_system::Pallet::<Test>::set_block_number(1);
    });
    ext
}
