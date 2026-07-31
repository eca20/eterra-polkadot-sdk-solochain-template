use crate as pallet_eterra_magic;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU32, Everything},
};
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage, DispatchError,
};
use std::cell::Cell;

type Block = frame_system::mocking::MockBlock<Test>;
pub type Balance = u128;

construct_runtime!(
    pub struct Test {
        System: frame_system,
        Balances: pallet_balances,
        Magic: pallet_eterra_magic,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const ExistentialDeposit: Balance = 1;
    pub CraftingFeeDestination: u64 = 99;
    pub const MaxChargeDefinitionsPerSession: u32 = 8;
    pub const MaxCraftBatch: u32 = 10;
    pub const MaxPrismXpGrant: u64 = 100_000;
}

thread_local! {
    static ACCESS_ALLOWED: Cell<bool> = const { Cell::new(true) };
    static PRODUCTION_CRAFTING_ENABLED: Cell<bool> = const { Cell::new(false) };
}

pub struct MockAccessControl;

impl pallet_alpha_access::AccessControl<u64> for MockAccessControl {
    fn ensure_whitelisted(_: &u64) -> frame_support::dispatch::DispatchResult {
        if ACCESS_ALLOWED.with(Cell::get) {
            Ok(())
        } else {
            Err(DispatchError::Other("not whitelisted"))
        }
    }
}

pub fn set_access_allowed(allowed: bool) {
    ACCESS_ALLOWED.with(|value| value.set(allowed));
}

pub struct ProductionCraftingEnabled;
impl frame_support::traits::Get<bool> for ProductionCraftingEnabled {
    fn get() -> bool {
        PRODUCTION_CRAFTING_ENABLED.with(Cell::get)
    }
}

pub fn set_production_crafting_enabled(enabled: bool) {
    PRODUCTION_CRAFTING_ENABLED.with(|value| value.set(enabled));
}

impl frame_system::Config for Test {
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
    type AccountData = pallet_balances::AccountData<Balance>;
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

impl crate::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = frame_system::EnsureRoot<u64>;
    type AccessControl = MockAccessControl;
    type Currency = Balances;
    type CraftingFeeDestination = CraftingFeeDestination;
    type ProductionCraftingEnabled = ProductionCraftingEnabled;
    type MaxChargeDefinitionsPerSession = MaxChargeDefinitionsPerSession;
    type MaxCraftBatch = MaxCraftBatch;
    type MaxPrismXpGrant = MaxPrismXpGrant;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 10_000), (2, 10_000), (99, 1)],
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        set_access_allowed(true);
        set_production_crafting_enabled(false);
    });
    ext
}
