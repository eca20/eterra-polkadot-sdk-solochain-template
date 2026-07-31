use crate as pallet_eterra_slots;
use frame_support::{
    dispatch::DispatchResult,
    parameter_types,
    traits::{ConstU128, ConstU32, ConstU64, ConstU8, Everything},
    BoundedVec,
};
use frame_system as system;
use parity_scale_codec::Encode;
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage, DispatchError,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub struct Test {
        System: frame_system,
        Balances: pallet_balances,
        EterraMedia: pallet_eterra_media,
        EterraSeasons: pallet_eterra_seasons,
        Nfts: pallet_nfts,
        EterraSlots: pallet_eterra_slots,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const ExistentialDeposit: u128 = 1;
    pub const MaxAttempts: u8 = 3;
    pub const CardsPerPack: u8 = 6;
    pub const MaxOwnedCards: u32 = 5_000;
    pub const BaseCardCapacity: u32 = 500;
    pub const CardCapacityUpgradeAmount: u32 = 100;
    pub const CardCapacityUpgradePrice: u128 = 100;
    pub const CardCapacityUpgradePriceReceiver: u64 = 999;
    pub const PackPrice: u128 = 500;
    pub const PackPriceReceiver: u64 = 999;
    pub const ProPrice: u128 = 200;
    pub const ProPriceReceiver: u64 = 999;
    pub const MintCardPrice: u128 = 100;
    pub const MintCardPriceReceiver: u64 = 999;
    pub const MaxProSpins: u8 = 5;
    pub const MaxBorders: u32 = 32;
    pub const MaxBackgrounds: u32 = 32;
    pub const MaxSubjects: u32 = 128;
    pub const MaxBacks: u32 = 32;
    pub const MaxPackagingFronts: u32 = 16;
    pub const MaxPackagingBacks: u32 = 16;
    pub const MaxSeasonCollections: u32 = 32;
    pub const MaxSeasonCollectionNameLen: u32 = 64;
    pub const NexusTeamSize: u32 = 5;
    pub const NexusSubjectCopyCap: u32 = 5;
    pub const NexusOverflowTotalCapacity: u32 = 30;
    pub const NexusOverflowPerSubjectCapacity: u32 = 2;
    pub const NexusBaseVaultCapacity: u32 = 20;
    pub const MaxNexusMetadataUriLen: u32 = 256;
    pub const MaxNexusReasonLen: u32 = 128;
    pub const MaxNexusSpellSlotsPerCard: u32 = 3;
    pub const MaxNexusMatchPlayers: u32 = 2;
    pub const MaxProgressionNodesPerTree: u32 = 16;
    pub const MaxProgressionNodesPerCard: u32 = 16;
    pub const MaxMagicSlotsPerCard: u32 = 3;
    pub const MaxProgressionTrees: u32 = 64;
    pub const CardXpPerLevel: u32 = 100;
    pub const MaxCardXpGrantAmount: u32 = 500;
    pub const MaxV2PoolProfiles: u32 = 400;
    pub const MaxV2PoolPoses: u32 = 256;
    pub const MaxV2PoolBackgrounds: u32 = 32;
    pub const MaxV2CreditsPerAccountSku: u32 = 16;
    pub const MaxV2ProtectionBytes: u32 = 600;
    pub const MaxV2TeamSize: u32 = 6;
    pub const V2OperationalCardWarningThreshold: u64 = 9_000;
    pub const V2OperationalCardLimit: u64 = 10_000;
    pub const V16MigrationBatchSize: u32 = 50;
    pub const MinimumActiveCardsAfterConversion: u32 = 1;
    pub const MaxPendingConversionsPerAccount: u32 = 2;
    pub const MythicalAscensionSeasonDurationBlocks: u64 = 90;
    pub const MythicalAscensionWeekDurationBlocks: u64 = 7;

    pub const MaxMediaUriLen: u32 = 256;
    pub const MaxMediaContentTypeLen: u32 = 64;
    pub const MaxMediaNameLen: u32 = 64;
    pub const MaxMediaDescriptionLen: u32 = 256;
    pub const MaxMediaRolesPerAccount: u32 = 8;
    pub const DefaultMediaCollectionId: u32 = 0;

    pub const MaxSeasonNameLen: u32 = 64;
    pub const MaxSeasonDescLen: u32 = 256;
}

pub struct TcgSeasonActivationValidator;

impl pallet_eterra_seasons::SeasonActivationValidator<u32> for TcgSeasonActivationValidator {
    fn ensure_can_activate(season_id: u32) -> DispatchResult {
        pallet_eterra_slots::Pallet::<Test>::ensure_season_ready_for_activation(season_id)
    }
}

pub struct MockProgressionAuthorityProvider;

impl pallet_eterra_slots::ProgressionAuthorityProvider<u64> for MockProgressionAuthorityProvider {
    fn resolve_authority(
        _account: &u64,
        game_id: pallet_eterra_slots::ProgressionGameId,
        version_id: Option<pallet_eterra_slots::ProgressionVersionId>,
        event_type: pallet_eterra_slots::ProgressionEventTypeId,
    ) -> Option<pallet_eterra_slots::ProgressionAuthorityId> {
        if game_id == 10 && version_id == Some(7) && event_type == 8 {
            Some(99)
        } else {
            None
        }
    }
}

std::thread_local! {
    static MOCK_CURRENT_HAND: RefCell<BTreeSet<(u64, u32)>> = const { RefCell::new(BTreeSet::new()) };
    static MOCK_RANDOM_OUTPUTS: RefCell<BTreeMap<[u8; 32], [u8; 32]>> = const { RefCell::new(BTreeMap::new()) };
    static MOCK_RANDOM_TIMEOUTS: RefCell<BTreeSet<[u8; 32]>> = const { RefCell::new(BTreeSet::new()) };
    static MOCK_RANDOM_CONTEXTS: RefCell<BTreeMap<[u8; 32], (eterra_nexus_primitives::EconomicRealm, pallet_eterra_randomness::RandomnessMode)>> = const { RefCell::new(BTreeMap::new()) };
    static MOCK_RANDOM_MODE: RefCell<pallet_eterra_randomness::RandomnessMode> = const { RefCell::new(pallet_eterra_randomness::RandomnessMode::DeterministicPrivateAlpha) };
    static MOCK_LAST_RANDOM_REQUEST: RefCell<Option<[u8; 32]>> = const { RefCell::new(None) };
    static MOCK_NEXT_ENTITY_ID: RefCell<u64> = const { RefCell::new(1) };
    static MOCK_CREATED_ENTITIES: RefCell<Vec<pallet_eterra_creatures::ConversionEntityInput<u64>>> = const { RefCell::new(Vec::new()) };
    static MOCK_CONVERSION_PROFILE_ACTIVE: RefCell<bool> = const { RefCell::new(true) };
    static MOCK_LEGACY_ESCROW_OWNERS: RefCell<BTreeMap<u32, u64>> = const { RefCell::new(BTreeMap::new()) };
    static MOCK_V2_GENESIS_HASH: RefCell<[u8; 32]> = const { RefCell::new([0xA5; 32]) };
}

pub fn set_mock_current_hand(owner: u64, card_id: u32) {
    MOCK_CURRENT_HAND.with(|hand| {
        hand.borrow_mut().insert((owner, card_id));
    });
}

pub fn clear_mock_current_hands() {
    MOCK_CURRENT_HAND.with(|hand| hand.borrow_mut().clear());
}

pub fn last_random_request() -> [u8; 32] {
    MOCK_LAST_RANDOM_REQUEST.with(|request| request.borrow().expect("random request exists"))
}

pub fn has_random_request() -> bool {
    MOCK_LAST_RANDOM_REQUEST.with(|request| request.borrow().is_some())
}

pub fn finalize_random_request(request_id: [u8; 32], output: [u8; 32]) {
    MOCK_RANDOM_OUTPUTS.with(|outputs| {
        outputs.borrow_mut().insert(request_id, output);
    });
}

pub fn set_mock_randomness_mode(mode: pallet_eterra_randomness::RandomnessMode) {
    MOCK_RANDOM_MODE.with(|current| *current.borrow_mut() = mode);
}

pub fn timeout_random_request(request_id: [u8; 32]) {
    MOCK_RANDOM_TIMEOUTS.with(|timeouts| {
        timeouts.borrow_mut().insert(request_id);
    });
}

pub fn created_entities() -> Vec<pallet_eterra_creatures::ConversionEntityInput<u64>> {
    MOCK_CREATED_ENTITIES.with(|entities| entities.borrow().clone())
}

pub fn set_mock_conversion_profile_active(active: bool) {
    MOCK_CONVERSION_PROFILE_ACTIVE.with(|value| *value.borrow_mut() = active);
}

pub fn set_mock_v2_genesis_hash(genesis_hash: [u8; 32]) {
    MOCK_V2_GENESIS_HASH.with(|value| *value.borrow_mut() = genesis_hash);
}

pub fn set_mock_legacy_escrow_owner(card_id: u32, owner: u64) {
    MOCK_LEGACY_ESCROW_OWNERS.with(|owners| {
        owners.borrow_mut().insert(card_id, owner);
    });
}

pub const MOCK_LEGACY_ESCROW_CUSTODIAN: u64 = 900_001;

pub struct MockHandChecker;

impl pallet_eterra_slots::HandChecker<u64> for MockHandChecker {
    fn is_card_in_current_hand(owner: &u64, card_id: u32) -> bool {
        MOCK_CURRENT_HAND.with(|hand| hand.borrow().contains(&(*owner, card_id)))
    }
}

pub struct MockV2Randomness;

pub struct MockV2ChainDomain;

impl pallet_eterra_slots::V2ChainDomainProvider for MockV2ChainDomain {
    fn genesis_hash() -> [u8; 32] {
        MOCK_V2_GENESIS_HASH.with(|value| *value.borrow())
    }
}

impl pallet_eterra_randomness::VerifiableRandomness for MockV2Randomness {
    fn request(
        domain: [u8; 32],
        commitment: [u8; 32],
        immutable_config_hash: [u8; 32],
        min_epoch: u64,
    ) -> Result<[u8; 32], DispatchError> {
        Self::request_for(
            eterra_nexus_primitives::EconomicRealm::Training,
            Self::current_mode(),
            domain,
            commitment,
            immutable_config_hash,
            min_epoch,
        )
    }

    fn output(request_id: [u8; 32]) -> Option<(u64, [u8; 32], [u8; 32])> {
        MOCK_RANDOM_OUTPUTS.with(|outputs| {
            outputs
                .borrow()
                .get(&request_id)
                .copied()
                .map(|output| (1, output, [7; 32]))
        })
    }

    fn timed_out(request_id: [u8; 32]) -> bool {
        MOCK_RANDOM_TIMEOUTS.with(|timeouts| timeouts.borrow().contains(&request_id))
    }

    fn current_mode() -> pallet_eterra_randomness::RandomnessMode {
        MOCK_RANDOM_MODE.with(|mode| *mode.borrow())
    }

    fn request_for(
        economic_realm: eterra_nexus_primitives::EconomicRealm,
        expected_provenance: pallet_eterra_randomness::RandomnessMode,
        domain: [u8; 32],
        commitment: [u8; 32],
        immutable_config_hash: [u8; 32],
        min_epoch: u64,
    ) -> Result<[u8; 32], DispatchError> {
        let mode = Self::current_mode();
        if expected_provenance == pallet_eterra_randomness::RandomnessMode::Disabled
            || expected_provenance != mode
            || (economic_realm == eterra_nexus_primitives::EconomicRealm::Production
                && expected_provenance != pallet_eterra_randomness::RandomnessMode::DrandQuicknet)
        {
            return Err(DispatchError::Other(
                "mock randomness realm or provenance mismatch",
            ));
        }
        let request_id = sp_io::hashing::blake2_256(
            &(
                economic_realm,
                expected_provenance,
                domain,
                commitment,
                immutable_config_hash,
                min_epoch,
            )
                .encode(),
        );
        MOCK_RANDOM_CONTEXTS.with(|contexts| {
            contexts
                .borrow_mut()
                .insert(request_id, (economic_realm, expected_provenance));
        });
        MOCK_LAST_RANDOM_REQUEST.with(|request| *request.borrow_mut() = Some(request_id));
        Ok(request_id)
    }

    fn output_for(
        request_id: [u8; 32],
        expected_realm: eterra_nexus_primitives::EconomicRealm,
        expected_provenance: pallet_eterra_randomness::RandomnessMode,
    ) -> Option<pallet_eterra_randomness::RealmBoundRandomnessOutput> {
        let context =
            MOCK_RANDOM_CONTEXTS.with(|contexts| contexts.borrow().get(&request_id).copied())?;
        if context != (expected_realm, expected_provenance) {
            return None;
        }
        Self::output(request_id).map(|(epoch, output, proof_hash)| {
            pallet_eterra_randomness::RealmBoundRandomnessOutput {
                epoch,
                output,
                proof_hash,
                economic_realm: expected_realm,
                provenance: expected_provenance,
                provider_chain_hash: [0; 32],
            }
        })
    }

    fn production_ready() -> bool {
        Self::current_mode() == pallet_eterra_randomness::RandomnessMode::DrandQuicknet
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub struct MockV2BenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_eterra_slots::V2BenchmarkHelper for MockV2BenchmarkHelper {
    fn prepare_randomness() {
        set_mock_randomness_mode(pallet_eterra_randomness::RandomnessMode::DrandQuicknet);
    }

    fn seed_finalized_randomness(request_id: [u8; 32], output: [u8; 32]) {
        finalize_random_request(request_id, output);
    }

    fn seed_timed_out_randomness(request_id: [u8; 32]) {
        timeout_random_request(request_id);
    }

    fn prepare_conversion_entity_profile(
        _subject_id: u32,
        _subject_version: u32,
        _rarity: eterra_nexus_primitives::CardRarity,
    ) {
    }
}

pub struct MockV2Entities;

impl pallet_eterra_creatures::EntityManager<u64, u64> for MockV2Entities {
    fn reserve_entity_id() -> Result<u64, DispatchError> {
        MOCK_NEXT_ENTITY_ID.with(|next| {
            let entity_id = *next.borrow();
            *next.borrow_mut() = entity_id.saturating_add(1);
            Ok(entity_id)
        })
    }

    fn ensure_conversion_profile_active(
        _subject_id: u32,
        _subject_version: u32,
        _rarity: eterra_nexus_primitives::CardRarity,
    ) -> DispatchResult {
        MOCK_CONVERSION_PROFILE_ACTIVE.with(|active| {
            if *active.borrow() {
                Ok(())
            } else {
                Err(DispatchError::Other("mock conversion profile inactive"))
            }
        })
    }

    fn create_from_conversion(
        input: pallet_eterra_creatures::ConversionEntityInput<u64>,
    ) -> DispatchResult {
        MOCK_CREATED_ENTITIES.with(|entities| entities.borrow_mut().push(input));
        Ok(())
    }

    fn validate_session_entity(
        _owner: &u64,
        _economic_realm: eterra_nexus_primitives::EconomicRealm,
        _entity_id: u64,
        _revision: u32,
        _format_id: u32,
        _format_version: u32,
        _allowed_roles_mask: u8,
    ) -> DispatchResult {
        Ok(())
    }

    fn lock_entity(
        _owner: &u64,
        _entity_id: u64,
        _lock: eterra_nexus_primitives::AssetLock<u64>,
    ) -> DispatchResult {
        Ok(())
    }

    fn unlock_entity(_session_id: u64, _entity_id: u64) -> DispatchResult {
        Ok(())
    }

    fn force_unlock_entity(_session_id: u64, _entity_id: u64) -> DispatchResult {
        Ok(())
    }

    fn grant_experience(
        _owner: &u64,
        _entity_id: u64,
        _amount: u64,
        _result_id: [u8; 32],
    ) -> DispatchResult {
        Ok(())
    }
}

pub struct MockLegacyEscrowOwnerProvider;

impl pallet_eterra_slots::LegacyEscrowOwnerProvider<u64> for MockLegacyEscrowOwnerProvider {
    fn beneficial_owner(card_id: u32) -> Option<u64> {
        MOCK_LEGACY_ESCROW_OWNERS.with(|owners| owners.borrow().get(&card_id).copied())
    }

    fn custodian_account() -> Option<u64> {
        Some(MOCK_LEGACY_ESCROW_CUSTODIAN)
    }
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

parameter_types! {
    pub NftsFeatures: pallet_nfts::PalletFeatures = pallet_nfts::PalletFeatures::all_enabled();
}

#[cfg(feature = "runtime-benchmarks")]
pub struct NftsBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl
    pallet_nfts::BenchmarkHelper<
        u32,
        u32,
        sp_runtime::testing::UintAuthorityId,
        u64,
        sp_runtime::testing::TestSignature,
    > for NftsBenchmarkHelper
{
    fn collection(i: u16) -> u32 {
        u32::from(i)
    }

    fn item(i: u16) -> u32 {
        u32::from(i)
    }

    fn signer() -> (sp_runtime::testing::UintAuthorityId, u64) {
        (sp_runtime::testing::UintAuthorityId(1), 1)
    }

    fn sign(
        signer: &sp_runtime::testing::UintAuthorityId,
        message: &[u8],
    ) -> sp_runtime::testing::TestSignature {
        sp_runtime::testing::TestSignature(signer.0, message.to_vec())
    }
}

impl pallet_nfts::Config for Test {
    type RuntimeEvent = RuntimeEvent;

    type CollectionId = u32;
    type ItemId = u32;

    type Currency = Balances;
    type ForceOrigin = frame_system::EnsureRoot<u64>;
    type CreateOrigin =
        frame_support::traits::AsEnsureOriginWithArg<frame_system::EnsureSigned<u64>>;
    type Locker = ();

    type CollectionDeposit = ConstU128<0>;
    type ItemDeposit = ConstU128<0>;
    type MetadataDepositBase = ConstU128<0>;
    type AttributeDepositBase = ConstU128<0>;
    type DepositPerByte = ConstU128<0>;

    type StringLimit = ConstU32<256>;
    type KeyLimit = ConstU32<64>;
    type ValueLimit = ConstU32<256>;

    type ApprovalsLimit = ConstU32<20>;
    type ItemAttributesApprovalsLimit = ConstU32<20>;
    type MaxTips = ConstU32<10>;
    type MaxDeadlineDuration = ConstU64<100_000>;
    type MaxAttributesPerCall = ConstU32<10>;

    type Features = NftsFeatures;

    type OffchainSignature = sp_runtime::testing::TestSignature;
    type OffchainPublic = sp_runtime::testing::UintAuthorityId;

    #[cfg(feature = "runtime-benchmarks")]
    type Helper = NftsBenchmarkHelper;

    type WeightInfo = ();
}

impl pallet_eterra_seasons::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = frame_system::EnsureRoot<u64>;
    type MaxSeasonNameLen = MaxSeasonNameLen;
    type MaxSeasonDescLen = MaxSeasonDescLen;
    type SeasonActivationValidator = TcgSeasonActivationValidator;
    type WeightInfo = ();
}

impl pallet_eterra_media::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxUriLen = MaxMediaUriLen;
    type MaxContentTypeLen = MaxMediaContentTypeLen;
    type MaxNameLen = MaxMediaNameLen;
    type MaxDescriptionLen = MaxMediaDescriptionLen;
    type MaxRolesPerAccount = MaxMediaRolesPerAccount;
    type DefaultCollectionId = DefaultMediaCollectionId;
    type DefaultCollectionOwner = MintCardPriceReceiver;
    type WeightInfo = ();
}

impl pallet_eterra_slots::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type PaymentCurrency = Balances;
    type AccessControl = ();
    type HandChecker = MockHandChecker;
    type ProgressionAuthorityProvider = MockProgressionAuthorityProvider;
    type V2Randomness = MockV2Randomness;
    type V2ChainDomain = MockV2ChainDomain;
    #[cfg(feature = "runtime-benchmarks")]
    type V2BenchmarkHelper = MockV2BenchmarkHelper;
    type V2Entities = MockV2Entities;
    type LegacyEscrowOwnerProvider = MockLegacyEscrowOwnerProvider;
    type PackPrice = PackPrice;
    type PackPriceReceiver = PackPriceReceiver;
    type ProPrice = ProPrice;
    type ProPriceReceiver = ProPriceReceiver;
    type MintCardPrice = MintCardPrice;
    type MintCardPriceReceiver = MintCardPriceReceiver;
    type MaxProSpins = MaxProSpins;
    type MaxAttempts = ConstU8<3>;
    type CardsPerPack = ConstU8<6>;
    type MaxOwnedCards = MaxOwnedCards;
    type BaseCardCapacity = BaseCardCapacity;
    type CardCapacityUpgradeAmount = CardCapacityUpgradeAmount;
    type CardCapacityUpgradePrice = CardCapacityUpgradePrice;
    type CardCapacityUpgradePriceReceiver = CardCapacityUpgradePriceReceiver;
    type MaxBorders = MaxBorders;
    type MaxBackgrounds = MaxBackgrounds;
    type MaxSubjects = MaxSubjects;
    type MaxBacks = MaxBacks;
    type MaxPackagingFronts = MaxPackagingFronts;
    type MaxPackagingBacks = MaxPackagingBacks;
    type MaxSeasonCollections = MaxSeasonCollections;
    type MaxSeasonCollectionNameLen = MaxSeasonCollectionNameLen;
    type NexusTeamSize = NexusTeamSize;
    type NexusSubjectCopyCap = NexusSubjectCopyCap;
    type NexusOverflowTotalCapacity = NexusOverflowTotalCapacity;
    type NexusOverflowPerSubjectCapacity = NexusOverflowPerSubjectCapacity;
    type NexusBaseVaultCapacity = NexusBaseVaultCapacity;
    type MaxNexusMetadataUriLen = MaxNexusMetadataUriLen;
    type MaxNexusReasonLen = MaxNexusReasonLen;
    type MaxNexusSpellSlotsPerCard = MaxNexusSpellSlotsPerCard;
    type MaxProgressionNodesPerTree = MaxProgressionNodesPerTree;
    type MaxProgressionNodesPerCard = MaxProgressionNodesPerCard;
    type MaxMagicSlotsPerCard = MaxMagicSlotsPerCard;
    type MaxProgressionTrees = MaxProgressionTrees;
    type CardXpPerLevel = CardXpPerLevel;
    type MaxCardXpGrantAmount = MaxCardXpGrantAmount;
    type MaxNexusMatchPlayers = MaxNexusMatchPlayers;
    type MaxV2PoolProfiles = MaxV2PoolProfiles;
    type MaxV2PoolPoses = MaxV2PoolPoses;
    type MaxV2PoolBackgrounds = MaxV2PoolBackgrounds;
    type MaxV2CreditsPerAccountSku = MaxV2CreditsPerAccountSku;
    type MaxV2ProtectionBytes = MaxV2ProtectionBytes;
    type MaxV2TeamSize = MaxV2TeamSize;
    type V2OperationalCardWarningThreshold = V2OperationalCardWarningThreshold;
    type V2OperationalCardLimit = V2OperationalCardLimit;
    type V16MigrationBatchSize = V16MigrationBatchSize;
    type MinimumActiveCardsAfterConversion = MinimumActiveCardsAfterConversion;
    type MaxPendingConversionsPerAccount = MaxPendingConversionsPerAccount;
    type MythicalAscensionSeasonDurationBlocks = MythicalAscensionSeasonDurationBlocks;
    type MythicalAscensionWeekDurationBlocks = MythicalAscensionWeekDurationBlocks;

    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    clear_mock_current_hands();
    MOCK_RANDOM_OUTPUTS.with(|outputs| outputs.borrow_mut().clear());
    MOCK_RANDOM_TIMEOUTS.with(|timeouts| timeouts.borrow_mut().clear());
    MOCK_RANDOM_CONTEXTS.with(|contexts| contexts.borrow_mut().clear());
    MOCK_RANDOM_MODE.with(|mode| {
        *mode.borrow_mut() = pallet_eterra_randomness::RandomnessMode::DeterministicPrivateAlpha
    });
    MOCK_LAST_RANDOM_REQUEST.with(|request| *request.borrow_mut() = None);
    MOCK_NEXT_ENTITY_ID.with(|next| *next.borrow_mut() = 1);
    MOCK_CREATED_ENTITIES.with(|entities| entities.borrow_mut().clear());
    MOCK_CONVERSION_PROFILE_ACTIVE.with(|active| *active.borrow_mut() = true);
    MOCK_LEGACY_ESCROW_OWNERS.with(|owners| owners.borrow_mut().clear());
    MOCK_V2_GENESIS_HASH.with(|value| *value.borrow_mut() = [0xA5; 32]);
    let mut storage = system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("system genesis builds");

    // Fund common test accounts so they can mint packs (and pay transaction fees if enabled).
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 1_000_000), (2, 1_000_000), (3, 1_000_000)],
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis assimilates");

    pallet_eterra_seasons::GenesisConfig::<Test> {
        admins: vec![1],
        initial_draft_season: None,
        initial_active_season: None,
    }
    .assimilate_storage(&mut storage)
    .expect("seasons genesis assimilates");

    pallet_eterra_media::GenesisConfig::<Test> {
        create_default_collection: true,
        default_collection_name: b"Default".to_vec(),
        default_collection_description: b"Default".to_vec(),
        default_collection_owner: Some(1),
    }
    .assimilate_storage(&mut storage)
    .expect("media genesis assimilates");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        // The production on-empty default seals LegacyV1 creation. Legacy unit
        // tests explicitly exercise the pre-V16 behavior.
        pallet_eterra_slots::LegacyCreationSealedV16::<Test>::put(false);

        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S1".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D1".to_vec().try_into().unwrap();
        pallet_eterra_seasons::Pallet::<Test>::create_season(RuntimeOrigin::signed(1), name, desc)
            .expect("create season");

        let uri: BoundedVec<u8, MaxMediaUriLen> = b"ipfs://b".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri.clone(),
            ct.clone(),
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register border");
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri.clone(),
            ct.clone(),
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register background");
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri.clone(),
            ct.clone(),
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register subject");
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri,
            ct,
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register back");
        let uri: BoundedVec<u8, MaxMediaUriLen> = b"ipfs://pf".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri.clone(),
            ct.clone(),
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register packaging front");
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri,
            ct,
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register packaging back");

        let collection_name: frame_support::BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core Set".to_vec().try_into().unwrap();
        pallet_eterra_slots::Pallet::<Test>::create_season_collection(
            RuntimeOrigin::signed(1),
            1,
            collection_name,
        )
        .expect("create season collection");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::Border,
            0,
        )
        .expect("add border asset");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::Background,
            1,
        )
        .expect("add background asset");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::Subject,
            2,
        )
        .expect("add subject asset");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::Back,
            3,
        )
        .expect("add back asset");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::PackagingFront,
            4,
        )
        .expect("add packaging front asset");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::PackagingBack,
            5,
        )
        .expect("add packaging back asset");
        pallet_eterra_slots::Pallet::<Test>::publish_season_collection(
            RuntimeOrigin::signed(1),
            1,
            0,
        )
        .expect("publish season collection");

        pallet_eterra_seasons::Pallet::<Test>::activate_season(RuntimeOrigin::signed(1), 1)
            .expect("activate season");

        System::reset_events();
    });
    ext
}
