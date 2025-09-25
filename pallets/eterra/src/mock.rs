use crate as pallet_eterra;
use pallet_balances;
use pallet_eterra_simple_tcg;
use frame_support::{
    parameter_types,
    traits::{ConstU16, ConstU32, Get, Currency},
};
use frame_system as system;
use parity_scale_codec::{Decode, Encode}; // Ensure Encode and Decode are imported
use scale_info::TypeInfo;
use sp_core::H256; // Ensure H256 is imported
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
}; // Import TypeInfo

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Cards: pallet_eterra_simple_tcg,
        Eterra: pallet_eterra,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaximumBlockWeight: u64 = 1024;
    pub const MaximumBlockLength: u32 = 2 * 1024;
    pub const AvailableBlockRatio: sp_runtime::Perbill = sp_runtime::Perbill::from_percent(75);
    pub const ExistentialDeposit: u128 = 1;
}

parameter_types! {
    pub const FaucetAccountId: u64 = 999; // arbitrary faucet for tests
    pub const RandomnessSeedConst: u64 = 42;
    pub const MintFeeConst: u128 = 0; // zero-fee minting in tests to avoid funding hassle
}

impl system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
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
    type AccountData = pallet_balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ConstU16<42>;
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeTask = ();
    type RuntimeEvent = RuntimeEvent;
    // Add missing associated types
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

impl pallet_balances::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = u128;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
    type MaxLocks = ();
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = ();
    type MaxFreezes = ();
    type RuntimeHoldReason = (); 
    type RuntimeFreezeReason = ();
}

impl pallet_eterra_simple_tcg::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RandomnessSeed = RandomnessSeedConst;
    type Currency = Balances;
    type MintFee = MintFeeConst;
    type FaucetAccount = FaucetAccountId;
}

#[derive(Encode, Decode, TypeInfo, Clone, Copy, PartialEq, Eq, Debug)]
pub struct MockNumPlayers;

impl Get<u32> for MockNumPlayers {
    fn get() -> u32 {
        2 // The number of players in the mock setup
    }
}

#[derive(Encode, Decode, TypeInfo, Clone, Copy, PartialEq, Eq, Debug)]
pub struct MockMaxRounds;

impl Get<u8> for MockMaxRounds {
    fn get() -> u8 {
        5 // The number of players in the mock setup
    }
}

#[derive(Encode, Decode, TypeInfo, Clone, Copy, PartialEq, Eq, Debug)]
pub struct MockBlocksToPlayLimit;

impl Get<u8> for MockBlocksToPlayLimit {
    fn get() -> u8 {
        5 // The number of players in the mock setup
    }
}

parameter_types! {
    pub const HandSizeConst: u32 = 5;
}

impl pallet_eterra::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type NumPlayers = MockNumPlayers;
    type MaxRounds = MockMaxRounds;
    type BlocksToPlayLimit = MockBlocksToPlayLimit;
    type HandSize = HandSizeConst;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = frame_system::GenesisConfig::<Test>::default() // Explicit type annotation
        .build_storage()
        .unwrap();

    let mut ext = sp_io::TestExternalities::from(t);
    ext.execute_with(|| {
        System::set_block_number(1); // Reset block number
        // fund some accounts
        let _ = <Balances as Currency<u64>>::deposit_creating(&1u64, 1_000_000_000_000);
        let _ = <Balances as Currency<u64>>::deposit_creating(&2u64, 1_000_000_000_000);
        let _ = <Balances as Currency<u64>>::deposit_creating(&999u64, 1_000_000_000_000); // faucet
    });
    ext
}
