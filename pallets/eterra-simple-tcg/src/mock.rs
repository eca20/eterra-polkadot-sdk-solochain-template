use crate as pallet_eterra_simple_tcg;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU128, ConstU32, ConstU64, Everything, GenesisBuild},
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};

pub const ALICE: u64 = 1;
pub const BOB: u64 = 2;

construct_runtime!(
    pub struct Test {
        System: frame_system,
        Balances: pallet_balances,
        EterraSimpleTCGConfig: pallet_eterra_simple_tcg,
    }
);

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

parameter_types! {
    pub const RandomnessSeed: u64 = 42;
    pub const ExistentialDeposit: u128 = 0; // keep accounts alive at 0 for tests
    pub const MintFeeConst: u128 = 100;     // 100 whole tokens in tests
    pub FaucetAccountParam: u64 = ALICE;    // faucet is Alice for tests
}

impl system::Config for Test {
    type BaseCallFilter = Everything;
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
    type AccountData = pallet_balances::AccountData<u128>;
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

impl pallet_balances::Config for Test {
    type Balance = u128;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ConstU128<1>;
    type AccountStore = System;
    type WeightInfo = ();
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ConstU32<0>;
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = [u8; 8];

    type RuntimeHoldReason = ();
    type RuntimeFreezeReason = ();
    type MaxFreezes = ConstU32<0>;
}

impl pallet_eterra_simple_tcg::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RandomnessSeed = RandomnessSeed;

    // Currency integration for mint fee & marketplace
    type Currency = Balances;
    type MintFee = ConstU128<100>;
    type FaucetAccount = FaucetAccountParam;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    // Seed some balances so we can mint/buy/sell in tests
    let mut t = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ALICE, 1_000_000), (BOB, 1_000_000)],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    t.into()
}
