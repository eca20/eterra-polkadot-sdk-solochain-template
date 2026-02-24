use crate as pallet_eterra_faucet;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU32, Everything},
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};

pub const FAUCET: u64 = 10;
pub const BOB: u64 = 20;
pub const CHARLIE: u64 = 30;
pub const PAYOUT: u128 = 50;

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
    pub struct Test {
        System: frame_system,
        Balances: pallet_balances,
        EterraFaucet: pallet_eterra_faucet,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const ExistentialDeposit: u128 = 1;
    pub const ClaimCooldownBlocks: u64 = 10;
    pub const SponsoredClaimMaxCount: u32 = 2;
    pub const SponsoredClaimWindowBlocks: u64 = 20;
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
    type BlockHashCount = BlockHashCount;
}

impl pallet_balances::Config for Test {
    type Balance = u128;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ExistentialDeposit;
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

impl pallet_eterra_faucet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ClaimCooldownBlocks = ClaimCooldownBlocks;
    type SponsoredClaimMaxCount = SponsoredClaimMaxCount;
    type SponsoredClaimWindowBlocks = SponsoredClaimWindowBlocks;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("system genesis builds");

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(FAUCET, 1_000_000), (CHARLIE, 1_000)],
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis assimilates");

    pallet_eterra_faucet::GenesisConfig::<Test> {
        faucet_account: Some(FAUCET),
        payout_amount: PAYOUT,
    }
    .assimilate_storage(&mut storage)
    .expect("faucet genesis assimilates");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
    });
    ext
}
