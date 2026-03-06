use crate as pallet_eterra;
use frame_support::{
    parameter_types,
    traits::{ConstU16, ConstU32, Currency, Get},
};
use frame_system as system;
use pallet_assets;
use pallet_balances;
use pallet_eterra_gamer;
use pallet_eterra_monte_carlo_ai as mc_ai;
use pallet_eterra_tcg;
use parity_scale_codec::{Decode, Encode}; 
use scale_info::TypeInfo;
use sp_core::H256; 
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
}; 

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Assets: pallet_assets,
        Gamer: pallet_eterra_gamer,
        Cards: pallet_eterra_tcg,
        Eterra: pallet_eterra,
        EterraMonteCarloAi: pallet_eterra_monte_carlo_ai,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaximumBlockWeight: u64 = 1024;
    pub const MaximumBlockLength: u32 = 2 * 1024;
    pub const AvailableBlockRatio: sp_runtime::Perbill = sp_runtime::Perbill::from_percent(75);
    pub const ExistentialDeposit: u128 = 1;
}

const UNIT: u128 = 1_000_000_000_000;

parameter_types! {
    pub const FaucetAccountId: u64 = 999; // arbitrary faucet for tests
}

// --- TCG mint configuration (tests) ---
parameter_types! {
    pub const PackPriceConst: u128 = 0;
    pub const ProPriceConst: u128 = 0;
    pub const MintCardPriceConst: u128 = 0;
    pub const PackPriceReceiverConst: u64 = 999;
    pub const ProPriceReceiverConst: u64 = 999;
    pub const MintCardPriceReceiverConst: u64 = 999;
    pub const MaxProSpinsConst: u8 = 5;
    pub const MaxAttemptsConst: u8 = 3;
    pub const CardsPerPackConst: u8 = 6;
    pub const MaxPacksConst: u32 = 10;
    pub const MaxOwnedCardsConst: u32 = 1024;
}

pub struct MockHandChecker;

impl pallet_eterra_tcg::HandChecker<u64> for MockHandChecker {
    fn is_card_in_current_hand(owner: &u64, card_id: u32) -> bool {
        pallet_eterra::CurrentHandOf::<Test>::get(owner)
            .map(|hand| hand.iter().any(|&id| id == card_id))
            .unwrap_or(false)
    }
}

// --- Multi-currency + XP reward configuration (tests) ---
parameter_types! {
    pub const DevCoinAssetIdConst: u32 = 1;
    pub const BetaCoinAssetIdConst: u32 = 2;
    pub const WinRewardCoinConst: u128 = 10 * UNIT;
    pub const WinRewardDevCoinConst: u128 = 100 * UNIT;
    pub const WinRewardBetaCoinConst: u128 = 100 * UNIT;
    pub const WinRewardExperienceConst: u128 = 100;
}

// --- Assets pallet test config ---
parameter_types! {
    pub const AssetDeposit: u128 = 0;
    pub const AssetAccountDeposit: u128 = 0;
    pub const MetadataDepositBase: u128 = 0;
    pub const MetadataDepositPerByte: u128 = 0;
    pub const ApprovalDeposit: u128 = 0;
    pub const AssetsStringLimit: u32 = 64;
}

// --- Gamer pallet test config ---
parameter_types! {
    pub const GamerChangeFee: u128 = 0;
    pub const GamerTagMaxLen: u32 = 32;
    pub const GamerAvatarCidMaxLen: u32 = 96;
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

impl pallet_assets::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = u128;
    type AssetId = u32;
    type AssetIdParameter = u32;
    type Currency = Balances;
    // Allow signed accounts to `create` in tests; we mainly use `force_create` via Root.
    type CreateOrigin =
        frame_support::traits::AsEnsureOriginWithArg<frame_system::EnsureSigned<u64>>;
    type ForceOrigin = frame_system::EnsureRoot<u64>;
    type AssetDeposit = AssetDeposit;
    type AssetAccountDeposit = AssetAccountDeposit;
    type MetadataDepositBase = MetadataDepositBase;
    type MetadataDepositPerByte = MetadataDepositPerByte;
    type ApprovalDeposit = ApprovalDeposit;
    type StringLimit = AssetsStringLimit;
    type Freezer = ();
    type Extra = ();
    type CallbackHandle = ();
    type WeightInfo = ();
    type RemoveItemsLimit = ConstU32<1_000>;
}

impl pallet_eterra_gamer::Config for Test {
    type Currency = Balances;
    type ExpIssuerOrigin = frame_system::EnsureRoot<u64>;
    type FaucetAccount = FaucetAccountId;
    type ChangeFee = GamerChangeFee;
    type MaxTagLen = GamerTagMaxLen;
    type MaxAvatarCidLen = GamerAvatarCidMaxLen;
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
}

impl pallet_eterra_tcg::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type HandChecker = MockHandChecker;
    type PackPrice = PackPriceConst;
    type PackPriceReceiver = PackPriceReceiverConst;
    type ProPrice = ProPriceConst;
    type ProPriceReceiver = ProPriceReceiverConst;
    type MintCardPrice = MintCardPriceConst;
    type MintCardPriceReceiver = MintCardPriceReceiverConst;
    type MaxProSpins = MaxProSpinsConst;
    type MaxAttempts = MaxAttemptsConst;
    type CardsPerPack = CardsPerPackConst;
    type MaxPacks = MaxPacksConst;
    type MaxOwnedCards = MaxOwnedCardsConst;
    type WeightInfo = ();
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
    // Keep these small for unit tests.
    pub const BlocksPerHourConst: u64 = 10;
    pub const BlocksPerDayConst: u64 = 240;
    pub const BlocksPerWeekConst: u64 = 7 * 240;
    pub const BlocksPerMonthConst: u64 = 30 * 240;
}

parameter_types! {
    pub const HandSizeConst: u32 = 5;
}

parameter_types! {
    pub const AiDifficultyConst: u8 = 20;
}

parameter_types! {
    // Disable Gridlock by default in the shared test runtime so existing game tests remain stable.
    pub const GridlockMinLocksConst: u8 = 0;
    pub const GridlockMaxLocksConst: u8 = 0;
}

impl pallet_eterra::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type NumPlayers = MockNumPlayers;
    type MaxRounds = MockMaxRounds;
    type BlocksToPlayLimit = MockBlocksToPlayLimit;
    type HandSize = HandSizeConst;
    type AiAccount = FaucetAccountId;
    type AiDifficulty = AiDifficultyConst;
    type AdminOrigin = frame_system::EnsureRoot<u64>;
    type BlocksPerHour = BlocksPerHourConst;
    type BlocksPerDay = BlocksPerDayConst;
    type BlocksPerWeek = BlocksPerWeekConst;
    type BlocksPerMonth = BlocksPerMonthConst;
    type GridlockMinLocks = GridlockMinLocksConst;
    type GridlockMaxLocks = GridlockMaxLocksConst;
    type Assets = Assets;
    type ExperienceManager = Gamer;
    type DevCoinAssetId = DevCoinAssetIdConst;
    type BetaCoinAssetId = BetaCoinAssetIdConst;
    type WinRewardCoin = WinRewardCoinConst;
    type WinRewardDevCoin = WinRewardDevCoinConst;
    type WinRewardBetaCoin = WinRewardBetaCoinConst;
    type WinRewardExperience = WinRewardExperienceConst;
    type WeightInfo = ();
}

impl mc_ai::pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Adapter = eterra_card_ai_adapter::eterra_adapter::Adapter;
    type MaxActions = ConstU32<64>;
    type BaseIterations = ConstU32<100>;
    type MaxPlayoutDepth = ConstU16<16>;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = frame_system::GenesisConfig::<Test>::default() // Explicit type annotation
        .build_storage()
        .unwrap();

    let mut ext = sp_io::TestExternalities::from(t);
    ext.execute_with(|| {
        System::set_block_number(1); // Reset block number
                                     // fund some accounts
        let _ = <Balances as Currency<u64>>::deposit_creating(&1u64, 1 * UNIT);
        let _ = <Balances as Currency<u64>>::deposit_creating(&2u64, 1 * UNIT);
        let _ = <Balances as Currency<u64>>::deposit_creating(&999u64, 1 * UNIT);

        // Create devCOIN (asset 1) and betaCOIN (asset 2) for reward tests.
        let _ = Assets::force_create(
            frame_system::RawOrigin::Root.into(),
            DevCoinAssetIdConst::get(),
            FaucetAccountId::get(),
            false,
            1u128,
        );
        let _ = Assets::force_create(
            frame_system::RawOrigin::Root.into(),
            BetaCoinAssetIdConst::get(),
            FaucetAccountId::get(),
            false,
            1u128,
        );
    });
    ext
}
