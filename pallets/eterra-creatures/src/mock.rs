use crate as pallet_eterra_creatures;
use eterra_nexus_primitives::EconomicRealm;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU32, Everything},
};
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage, DispatchError,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
    pub struct Test {
        System: frame_system,
        Creatures: pallet_eterra_creatures,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaxLearnedMoves: u32 = 12;
    pub const MaxEquippedMoves: u32 = 4;
    pub const MaxProfileMoves: u32 = 24;
    pub const MaxLeagueAllowedMoves: u32 = 48;
    pub const MaxExperienceGrant: u64 = 100_000;
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
    type AccountData = ();
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

thread_local! {
    static ACCESS_ALLOWED: Cell<bool> = const { Cell::new(true) };
    static ESSENCE: RefCell<BTreeMap<(u64, EconomicRealm, u8), u32>> =
        const { RefCell::new(BTreeMap::new()) };
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

pub struct MockEssence;
impl crate::EssenceManager<u64> for MockEssence {
    fn consume(
        owner: &u64,
        realm: EconomicRealm,
        element_id: u8,
        amount: u32,
    ) -> frame_support::dispatch::DispatchResult {
        ESSENCE.with(|balances| {
            let mut balances = balances.borrow_mut();
            let balance = balances.entry((*owner, realm, element_id)).or_default();
            if *balance < amount {
                return Err(DispatchError::Other("insufficient essence"));
            }
            *balance -= amount;
            Ok(())
        })
    }
}

pub fn seed_essence(owner: u64, realm: EconomicRealm, element: u8, amount: u32) {
    ESSENCE.with(|balances| {
        balances
            .borrow_mut()
            .insert((owner, realm, element), amount);
    });
}

impl crate::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = frame_system::EnsureRoot<u64>;
    type ResultOrigin = frame_system::EnsureRoot<u64>;
    type AccessControl = MockAccessControl;
    type Essence = MockEssence;
    type MaxLearnedMoves = MaxLearnedMoves;
    type MaxEquippedMoves = MaxEquippedMoves;
    type MaxProfileMoves = MaxProfileMoves;
    type MaxLeagueAllowedMoves = MaxLeagueAllowedMoves;
    type MaxExperienceGrant = MaxExperienceGrant;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        set_access_allowed(true);
        ESSENCE.with(|balances| balances.borrow_mut().clear());
    });
    ext
}
