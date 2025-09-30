// pallets/matchmaker/src/mock.rs
#![cfg(test)]

use crate as pallet_matchmaker;

use frame_support::{
    construct_runtime, parameter_types,
    traits::{Everything, OnFinalize, OnInitialize},
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage, DispatchError,
};

// --- Base types for the mock runtime ---
pub type AccountId = u64;
pub type BlockNumber = u64;

parameter_types! {
    pub const BlockHashCount: u64 = 240;
    pub const ExistentialDeposit: u64 = 0;
    pub const PlayersPerMatchConst: u8 = 2;      // For 1v1 matching
    pub const QueueCapacityConst: u32 = 64;      // Circular buffer capacity for tests
}

impl system::Config for Test {
    type BaseCallFilter = Everything;
    type Block = frame_system::mocking::MockBlock<Test>;
    type BlockHashCount = BlockHashCount;
    type DbWeight = ();
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type BlockLength = ();
    type BlockWeights = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type AccountData = ();
    type Version = ();
    type PalletInfo = PalletInfo;
    type SS58Prefix = ();
    type SystemWeightInfo = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
    type OnSetCode = ();
    // Additional associated types for new frame_system::Config requirements
    type RuntimeTask = (); // no background tasks in tests
    type Nonce = u64; // account nonce type used in tests
    type SingleBlockMigrations = (); // none for tests
    type MultiBlockMigrator = (); // none for tests
    type PreInherents = (); // no pre-inherents hooks in mock
    type PostInherents = (); // no post-inherents hooks in mock
    type PostTransactions = (); // no post-transactions hooks in mock
}

// --- Controllable HandProvider for tests ---
// We keep a per-test thread-local set of accounts that "have" a hand.
use std::cell::RefCell;
use std::collections::BTreeSet;

thread_local! {
    static TL_HAND_SET: RefCell<BTreeSet<AccountId>> = RefCell::new(BTreeSet::new());
}

thread_local! {
    pub static CREATED_GAMES: std::cell::RefCell<Vec<(AccountId, AccountId)>> =
        std::cell::RefCell::new(Vec::new());
    pub static NEXT_GAME_ID: std::cell::Cell<u64> = std::cell::Cell::new(1);
}

pub fn created_games() -> Vec<(AccountId, AccountId)> {
    CREATED_GAMES.with(|v| v.borrow().clone())
}

/// Test-only provider: consults a thread-local set to determine if an account has a hand.
pub struct MockHandProvider;
impl pallet_matchmaker::CurrentHandProvider<AccountId> for MockHandProvider {
    fn has_current_hand(who: &AccountId) -> bool {
        TL_HAND_SET.with(|s| s.borrow().contains(who))
    }
}

/// Helper: mark/unmark an account as having a hand in this test thread.
pub fn set_has_hand(who: AccountId, has: bool) {
    TL_HAND_SET.with(|s| {
        let mut s = s.borrow_mut();
        if has {
            s.insert(who);
        } else {
            s.remove(&who);
        }
    });
}

/// Helper: clear all "has hand" flags (called by new_test_ext).
pub fn clear_all_hands() {
    TL_HAND_SET.with(|s| s.borrow_mut().clear());
}

// --- Test-only GameCreator implementation for () ---
impl pallet_matchmaker::GameCreator<AccountId> for () {
    type GameId = u32;

    fn create_from_matchmaking(
        a: &AccountId,
        b: &AccountId,
    ) -> Result<Self::GameId, DispatchError> {
        // Record the created game pair for assertions.
        CREATED_GAMES.with(|v| v.borrow_mut().push((*a, *b)));
        // Bump a simple counter for the returned GameId.
        let id = NEXT_GAME_ID.with(|c| {
            let id = c.get();
            c.set(id + 1);
            id
        });
        Ok(id as u32)
    }
}

impl pallet_matchmaker::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type PlayersPerMatch = PlayersPerMatchConst;
    type QueueCapacity = QueueCapacityConst;
    type HandProvider = MockHandProvider;
    type GameCreator = ();
}

construct_runtime!(
    pub enum Test where
        Block = frame_system::mocking::MockBlock<Test>,
        NodeBlock = frame_system::mocking::MockBlock<Test>,
        UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>,
    {
        System: frame_system,
        Matchmaker: pallet_matchmaker,
    }
);

/// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| {
        clear_all_hands();
        CREATED_GAMES.with(|v| v.borrow_mut().clear());
        NEXT_GAME_ID.with(|c| c.set(1));
        System::set_block_number(1);
    });
    ext
}

/// Progress to the next block (handy for event ordering if needed).
pub fn run_to_block(n: BlockNumber) {
    while System::block_number() < n {
        let b = System::block_number() + 1;
        System::on_finalize(System::block_number());
        System::set_block_number(b);
        System::on_initialize(b);
    }
}
