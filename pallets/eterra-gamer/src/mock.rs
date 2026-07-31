//! Mock runtime for pallet-eterra-gamer tests.
#![cfg(test)]

use crate as pallet_eterra_gamer;
use eterra_nexus_primitives::{EconomicRealm, PackCreditSource};
use frame_support::{construct_runtime, parameter_types};
use frame_system as system;
use sp_core::H256;
#[cfg(feature = "runtime-benchmarks")]
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};
use sp_runtime::BuildStorage;
use std::cell::{Cell, RefCell};

pub type Balance = u128;
pub type AccountId = u64;
pub type BlockNumber = u32;
pub type IssuedCredit = (AccountId, u32, u32, EconomicRealm, PackCreditSource);

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const FAUCET: AccountId = 99;

parameter_types! {
    pub const BlockHashCount: BlockNumber = 250;
    pub const SS58Prefix: u16 = 42;
    pub const ExistentialDeposit: Balance = 1;
    pub const MaxTagLen: u32 = 32;
    pub const MaxInitialsLen: u32 = 4;
    pub const MaxAvatarCidLen: u32 = 96;
    pub const MaxRegionCodeLen: u32 = 2;
    pub const MaxSteamLinkSignatureLen: u32 = 64;
    pub const ChangeFee: Balance = 100;
    pub const MaxV2XpGrant: u128 = 100_000;
    pub const MaxPackCreditsPerAllocation: u32 = 8;
    pub FaucetAccountParam: AccountId = FAUCET;
}

thread_local! {
    pub static ACCESS_ALLOWED: Cell<bool> = const { Cell::new(true) };
    pub static ISSUED_CREDITS: RefCell<Vec<IssuedCredit>> =
        const { RefCell::new(Vec::new()) };
    pub static CREDIT_ISSUER_FAILS: RefCell<bool> = const { RefCell::new(false) };
}

pub struct MockAccessControl;

impl pallet_alpha_access::AccessControl<AccountId> for MockAccessControl {
    fn ensure_whitelisted(_: &AccountId) -> frame_support::dispatch::DispatchResult {
        if ACCESS_ALLOWED.with(Cell::get) {
            Ok(())
        } else {
            Err(sp_runtime::DispatchError::Other("not whitelisted"))
        }
    }
}

pub fn set_access_allowed(allowed: bool) {
    ACCESS_ALLOWED.with(|value| value.set(allowed));
}

pub struct MockPackCreditIssuer;
impl crate::PackCreditIssuer<AccountId> for MockPackCreditIssuer {
    fn issue_pack_credit(
        owner: &AccountId,
        pack_sku: u32,
        sku_version: u32,
        realm: EconomicRealm,
        source: PackCreditSource,
    ) -> frame_support::dispatch::DispatchResult {
        if CREDIT_ISSUER_FAILS.with(|fails| *fails.borrow()) {
            return Err(sp_runtime::DispatchError::Other("mock issuer failure"));
        }
        ISSUED_CREDITS.with(|issued| {
            issued
                .borrow_mut()
                .push((*owner, pack_sku, sku_version, realm, source));
        });
        Ok(())
    }
}

pub struct MockPackTrackCatalogPolicy;

impl crate::PackTrackCatalogPolicy for MockPackTrackCatalogPolicy {
    fn ensure_earned_pack_sku(
        pack_sku: u32,
        sku_version: u32,
    ) -> frame_support::dispatch::DispatchResult {
        if (pack_sku == 10 && sku_version == 1) || pack_sku == 2 {
            Ok(())
        } else {
            Err(sp_runtime::DispatchError::Other("pack SKU is not Earned"))
        }
    }
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
    type AccessControl = MockAccessControl;
    type ExpIssuerOrigin = frame_system::EnsureRoot<AccountId>;
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type FaucetAccount = FaucetAccountParam;
    type ChangeFee = ChangeFee;
    type MaxTagLen = MaxTagLen;
    type MaxInitialsLen = MaxInitialsLen;
    type MaxAvatarCidLen = MaxAvatarCidLen;
    type MaxRegionCodeLen = MaxRegionCodeLen;
    type MaxSteamLinkSignatureLen = MaxSteamLinkSignatureLen;
    type PackCreditIssuer = MockPackCreditIssuer;
    type PackTrackCatalogPolicy = MockPackTrackCatalogPolicy;
    type MaxV2XpGrant = MaxV2XpGrant;
    type MaxPackCreditsPerAllocation = MaxPackCreditsPerAllocation;
    type WeightInfo = ();
}

// Build a mock runtime
type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        EterraGamer: pallet_eterra_gamer,
    }
);

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ALICE, 1_000_000), (BOB, 1_000), (FAUCET, 1)],
    }
    .assimilate_storage(&mut t)
    .unwrap();
    let mut ext: sp_io::TestExternalities = t.into();
    #[cfg(feature = "runtime-benchmarks")]
    ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
    ext.execute_with(|| {
        set_access_allowed(true);
        ISSUED_CREDITS.with(|issued| issued.borrow_mut().clear());
        CREDIT_ISSUER_FAILS.with(|fails| *fails.borrow_mut() = false);
    });
    ext
}
