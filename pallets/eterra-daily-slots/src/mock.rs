#![cfg(test)]

//! Mock runtime and helpers for pallet-eterra-daily-slots

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

// =====================================================
// 🕰️ Mock Time Provider
// =====================================================

/// Global mutable variable for mock time
static mut MOCK_NOW: u64 = 90_000; // default baseline time

/// Mock implementation of UnixTime
pub struct MockTime;

impl UnixTime for MockTime {
    fn now() -> Duration {
        Duration::from_secs(unsafe { MOCK_NOW })
    }
}

/// Utility to manipulate MockTime safely in tests
pub struct MockTimeState;

impl MockTimeState {
    /// Set the mock current time
    pub fn set_now(new_now: u64) {
        unsafe { MOCK_NOW = new_now; }
    }

    /// Get the current mock time
    pub fn now() -> u64 {
        unsafe { MOCK_NOW }
    }
}

// =====================================================
// 🛠️ Mock Runtime Setup
// =====================================================

/// Define UncheckedExtrinsic and Block
type UncheckedExtrinsic = system::mocking::MockUncheckedExtrinsic<Test>;
type Block = system::mocking::MockBlock<Test>;

/// Create a mock runtime
construct_runtime!(
    pub enum Test {
        System: system,
        EterraDailySlots: pallet_eterra_daily_slots,
    }
);

/// Alias to avoid confusion in test files
pub type TestRuntime = Test;

// =====================================================
// ⚙️ Runtime Config Implementations
// =====================================================

parameter_types! {
    pub const BlockHashCount: u64 = 250;
}

impl system::Config for Test {
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

    // Newer Substrate associated types
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

parameter_types! {
    pub const MaxSlotLength: u32 = 3;
    pub const MaxOptionsPerSlot: u32 = 5;
    pub const MaxRollsPerRound: u32 = 2;
}

impl pallet_eterra_daily_slots::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type TimeProvider = MockTime;
    type MaxSlotLength = MaxSlotLength;
    type MaxOptionsPerSlot = MaxOptionsPerSlot;
    type MaxRollsPerRound = MaxRollsPerRound;
}

// =====================================================
// 🧪 Externalities Setup
// =====================================================

/// Reset mock time to baseline (90_000) before each test
fn reset_mock_time() {
    MockTimeState::set_now(90_000);
}

/// Build externalities for tests
pub fn new_test_ext() -> sp_io::TestExternalities {
    reset_mock_time(); // Always reset mock clock

    let storage = system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("Failed to build storage");

    let mut ext = sp_io::TestExternalities::from(storage);
    ext.execute_with(|| {
        // Reset system
        frame_system::Pallet::<Test>::set_block_number(1);

        // 🧹 Reset custom storage values
        crate::LastRollTime::<TestRuntime>::remove_all(None);
        crate::RollsThisBlock::<TestRuntime>::remove_all(None);
        crate::SlotMachineConfig::<TestRuntime>::kill(); // if needed
        crate::TicketsPerUser::<TestRuntime>::remove_all(None);
        crate::TotalTickets::<TestRuntime>::kill();
        crate::LastDrawingTime::<TestRuntime>::kill();
    });
    ext
}