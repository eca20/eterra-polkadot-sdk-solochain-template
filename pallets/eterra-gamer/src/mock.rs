//! Mock runtime for pallet-eterra-gamer tests.
#![cfg(test)]

use crate as pallet_eterra_gamer;
use frame_support::{
    construct_runtime, parameter_types,
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
};
use sp_runtime::BuildStorage;

pub type Balance = u128;
pub type AccountId = u64;
pub type BlockNumber = u32;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const FAUCET: AccountId = 99;

parameter_types! {
    pub const BlockHashCount: BlockNumber = 250;
    pub const SS58Prefix: u16 = 42;
    pub const ExistentialDeposit: Balance = 1;
    pub const MaxTagLen: u32 = 32;
    pub const MaxAvatarCidLen: u32 = 96;
    pub const ChangeFee: Balance = 100;
    pub FaucetAccountParam: AccountId = FAUCET;
}

impl system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type Block = Block;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<AccountId>;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = SS58Prefix;
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
    type RuntimeTask = ();
    type Nonce = u32;
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

impl pallet_balances::Config for Test {
    type Balance = Balance;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type MaxLocks = ();
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type RuntimeHoldReason = ();
    type RuntimeFreezeReason = ();
    type FreezeIdentifier = ();
    type MaxFreezes = ();
}

impl pallet_eterra_gamer::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ExpIssuerOrigin = frame_system::EnsureRoot<AccountId>;
    type FaucetAccount = FaucetAccountParam;
    type ChangeFee = ChangeFee;
    type MaxTagLen = MaxTagLen;
    type MaxAvatarCidLen = MaxAvatarCidLen;
}

// Build a mock runtime
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system,
        Balances: pallet_balances,
        EterraGamer: pallet_eterra_gamer,
    }
);

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ALICE, 1_000_000), (BOB, 1_000), (FAUCET, 1)],
    }
    .assimilate_storage(&mut t)
    .unwrap();
    t.into()
}
