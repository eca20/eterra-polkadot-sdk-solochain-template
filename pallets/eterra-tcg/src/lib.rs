#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "512"]
#![allow(clippy::needless_borrows_for_generic_args, clippy::useless_conversion)]

pub use pallet::*;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use eterra_nexus_primitives::{
    CardIdV2, CardInstanceV2, CardOriginV2, CardRarity, CardStateV2, ConversionPolicy,
    DiscoveryPolicy, EconomicRealm, Hash32, MediaDefinitionV2, PackCredit, PackCreditSource,
    PackSkuVersion, SubjectActivationState, SubjectDefinitionV2, SubjectRarityProfile,
};
#[cfg(feature = "try-runtime")]
use frame_support::traits::PalletInfoAccess;
use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ExistenceRequirement, Get},
    BoundedBTreeSet, BoundedVec, PalletId,
};
use frame_system::{ensure_root, ensure_signed, pallet_prelude::OriginFor};
use pallet_eterra_randomness::RandomnessMode;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::traits::{AccountIdConversion, Hash, SaturatedConversion};
use sp_std::prelude::*;

pub type MediaId = pallet_eterra_media::MediaId;
pub type SeasonId = pallet_eterra_seasons::SeasonId;
pub type SeasonCollectionId = u32;
pub type CardGenomeHash = [u8; 32];
pub type SubjectId = u32;
pub type VaultVariantId = u32;
pub type GearId = u32;
pub type SpellId = u32;
pub type TeamId = u32;
pub type MatchId = u32;
pub type TrialId = u32;
pub type BoardId = u32;
pub type NexusConfigVersion = u32;
pub type ProgressionTreeId = u32;
pub type ProgressionNodeId = u32;
pub type ItemTemplateId = u32;
pub type ProgressionGameId = u64;
pub type ProgressionVersionId = u32;
pub type ProgressionEventTypeId = u32;
pub type ProgressionAuthorityId = u64;
pub type NexusPrizePoolId = u32;

const V2_RARITY_PROTECTION_SLOTS: usize = 5;
const V2_MAX_POSE_SLOTS_PER_SUBJECT: u8 = 3;
const V2_MAX_BACKGROUND_SLOTS_PER_SET: u8 = 5;
const V2_COSMETIC_PROTECTION_SLOTS_PER_RARITY: usize =
    V2_MAX_POSE_SLOTS_PER_SUBJECT as usize * V2_MAX_BACKGROUND_SLOTS_PER_SET as usize;
const V2_COSMETIC_PROTECTION_SLOTS_PER_SUBJECT: usize =
    V2_RARITY_PROTECTION_SLOTS * V2_COSMETIC_PROTECTION_SLOTS_PER_RARITY;
const V2_BRING_FIVE_TEAM_SIZE: u8 = 5;
const V2_TUTORIAL_CONVERSION_SLOT: u8 = 5;
const V2_MAX_REJECTION_ATTEMPTS: u32 = 256;

/// Provides a runtime-defined view of whether a given `card_id` is currently included
/// in `owner`'s configured "current hand".
///
/// This is used to prevent listing/selling/transferring a card that is actively in use
/// by gameplay, avoiding dangling card IDs in the player's current hand.
pub trait HandChecker<AccountId> {
    /// Returns `true` if `card_id` is present in `owner`'s current hand.
    fn is_card_in_current_hand(owner: &AccountId, card_id: u32) -> bool;
}

impl<AccountId> HandChecker<AccountId> for () {
    fn is_card_in_current_hand(_owner: &AccountId, _card_id: u32) -> bool {
        false
    }
}

/// Runtime-provided authority resolver for game/reward systems that can grant
/// card progression XP.
pub trait ProgressionAuthorityProvider<AccountId> {
    fn resolve_authority(
        account: &AccountId,
        game_id: ProgressionGameId,
        version_id: Option<ProgressionVersionId>,
        event_type: ProgressionEventTypeId,
    ) -> Option<ProgressionAuthorityId>;
}

pub trait LegacyEscrowOwnerProvider<AccountId> {
    fn beneficial_owner(card_id: u32) -> Option<AccountId>;
    fn custodian_account() -> Option<AccountId>;
}

impl<AccountId> LegacyEscrowOwnerProvider<AccountId> for () {
    fn beneficial_owner(_card_id: u32) -> Option<AccountId> {
        None
    }

    fn custodian_account() -> Option<AccountId> {
        None
    }
}

pub trait V2PackCreditManager<AccountId> {
    fn issue_credit(
        owner: &AccountId,
        pack_sku: u32,
        sku_version: u32,
        realm: EconomicRealm,
        source: PackCreditSource,
    ) -> frame_support::dispatch::DispatchResult;
}

impl<AccountId> V2PackCreditManager<AccountId> for () {
    fn issue_credit(
        _owner: &AccountId,
        _pack_sku: u32,
        _sku_version: u32,
        _realm: EconomicRealm,
        _source: PackCreditSource,
    ) -> frame_support::dispatch::DispatchResult {
        Err(sp_runtime::DispatchError::Other(
            "V2 pack credit provider unavailable",
        ))
    }
}

/// Supplies the Eterra genesis identity included in every Nexus V2 draw and
/// conversion transcript. Runtime wiring must use the live chain's block-zero
/// hash; tests deliberately vary it to prove cross-chain separation.
pub trait V2ChainDomainProvider {
    fn genesis_hash() -> Hash32;
}

#[derive(Clone, Copy)]
pub(crate) struct V2DrawTranscript {
    pub request_id: Hash32,
    pub immutable_config_hash: Hash32,
    pub account_commitment: Hash32,
    pub verified_randomness_output: Hash32,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct PoolProfileEntry {
    pub profile_id: u32,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct AcquisitionPoolVersion<Profiles, Poses, Backgrounds> {
    pub pool_id: u32,
    pub version: u32,
    pub set_id: u32,
    pub profiles: Profiles,
    pub poses: Poses,
    pub backgrounds: Backgrounds,
    pub immutable_config_hash: Hash32,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct PendingPackOpening<AccountId, BlockNumber> {
    pub opening_id: Hash32,
    pub owner: AccountId,
    pub credit_id: u64,
    pub pack_sku: u32,
    pub sku_version: u32,
    pub economic_realm: EconomicRealm,
    pub randomness_request_id: Hash32,
    pub commitment: Hash32,
    pub immutable_config_hash: Hash32,
    pub requested_at: BlockNumber,
    /// Immutable provenance expectation consumed by `output_for`; appended so
    /// callers never infer economic safety from mutable provider mode.
    pub expected_randomness_provenance: pallet_eterra_randomness::RandomnessMode,
}

/// Permanent replay receipt for a caller-chosen pack commitment. It is keyed
/// by `(owner, commitment)`, so an uncertain exact retry resolves before
/// another credit can be dequeued, while conflicting commitment reuse fails.
#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct PackOpeningRequestReceipt {
    pub opening_id: Hash32,
    pub pack_sku: u32,
    pub sku_version: u32,
    pub economic_realm: EconomicRealm,
}

/// Permanent receipt for the root-only private-alpha tutorial grant. The
/// tutorial identifier is globally single-use: exact retries are no-ops and
/// any attempt to redirect the entitlement to another owner or SKU fails.
#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct TutorialPackCreditGrantReceipt<AccountId> {
    pub owner: AccountId,
    pub credit_id: u64,
    pub pack_sku: u32,
    pub sku_version: u32,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct CompetitiveFormatV2 {
    pub format_id: u32,
    pub version: u32,
    pub set_id: u32,
    pub team_size: u8,
    pub rarity_load_budget: u8,
    pub max_mythical: u8,
    pub max_legendary_or_better: u8,
    pub rules_hash: Hash32,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct CompetitiveTeamV2<AccountId, Cards> {
    pub owner: AccountId,
    pub team_id: u32,
    pub format_id: u32,
    pub format_version: u32,
    pub cards: Cards,
    pub rarity_load: u8,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum V2Feature {
    Packs,
    Conversion,
    Ranked,
    MythicalAscension,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct MythicalAscensionSeasonConfig<BlockNumber> {
    pub season_id: u32,
    pub set_id: u32,
    pub pool_id: u32,
    pub pool_version: u32,
    pub starts_at: BlockNumber,
    pub ends_at: BlockNumber,
    pub required_mastery: u8,
    pub required_marks: u8,
    pub available_weeks: u8,
    pub config_hash: Hash32,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct MythicalAscensionSubjectConfig {
    pub season_id: u32,
    pub subject_id: SubjectId,
    pub subject_version: u32,
    pub foundation_pose_definition_id: u32,
    pub foundation_background_definition_id: u32,
    pub config_hash: Hash32,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum MythicalAscensionInput {
    LegendaryCard { card_id: CardIdV2 },
    LegendaryFoundation,
}

#[derive(
    Clone, Copy, Default, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug,
)]
pub struct ConvergenceProgress {
    pub marks_earned: u8,
    pub credited_week_bitmap: u16,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct MythicalAscensionReceipt<AccountId, BlockNumber> {
    pub ascension_id: Hash32,
    pub season_eligibility_id: Hash32,
    pub owner: AccountId,
    pub season_id: u32,
    pub subject_id: SubjectId,
    pub subject_version: u32,
    pub input: MythicalAscensionInput,
    pub output_card_id: CardIdV2,
    pub pose_definition_id: u32,
    pub background_definition_id: u32,
    pub config_hash: Hash32,
    pub ascended_at: BlockNumber,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum ConversionResolution {
    Pending,
    Created,
    StasisTimeout,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct ConversionTombstone<AccountId, BlockNumber> {
    pub request_id: Hash32,
    pub owner: AccountId,
    pub source_card_id: CardIdV2,
    pub source_card_snapshot_hash: Hash32,
    pub source_rarity: CardRarity,
    pub subject_id: SubjectId,
    pub subject_version: u32,
    pub reserved_entity_id: u64,
    pub randomness_request_id: Hash32,
    pub commitment: Hash32,
    pub committed_at: BlockNumber,
    pub resolution: ConversionResolution,
    /// Immutable provenance expectation consumed by `output_for`.
    pub expected_randomness_provenance: pallet_eterra_randomness::RandomnessMode,
    /// Caller-supplied catalog precondition retained for exact retry matching.
    pub expected_catalog_version: u32,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum LegacyCustodyKind {
    Ordinary,
    NftWrapped,
    KnownEscrow,
    UnknownFrozen,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct LegacyCardClassification<AccountId> {
    pub beneficial_owner: Option<AccountId>,
    pub custody: LegacyCustodyKind,
    pub frozen: bool,
    pub record_hash: Hash32,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum MigrationPhaseV16 {
    Running,
    Completed,
    UnsupportedSource,
    AwaitingVerification,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct TcgMigrationStateV16 {
    pub phase: MigrationPhaseV16,
    pub from_storage_version: u16,
    pub cursor: u32,
    pub upper_bound: u32,
    pub cards_seen: u32,
    pub ordinary: u32,
    pub nft_wrapped: u32,
    pub known_escrow: u32,
    pub anomalies: u32,
    pub max_card_id_seen: Option<u32>,
}

#[cfg(feature = "try-runtime")]
#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug)]
struct TcgMigrationPreUpgradeEvidenceV16 {
    from_storage_version: u16,
    pallet_storage_entry_count: u32,
    pallet_storage_hash: Hash32,
    card_count: u32,
    cards_hash: Hash32,
    nexus_card_count: u32,
    nexus_cards_hash: Hash32,
    converted_count: u32,
    converted_hash: Hash32,
    owner_index_count: u32,
    owner_index_hash: Hash32,
    vault_variant_count: u32,
    vault_variants_hash: Hash32,
    nexus_subject_index_count: u32,
    nexus_subject_indexes_hash: Hash32,
    overflow_owner_index_count: u32,
    overflow_owner_indexes_hash: Hash32,
    overflow_subject_index_count: u32,
    overflow_subject_indexes_hash: Hash32,
    next_card_id: u32,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct TcgMigrationAnomalyV16<AccountId> {
    pub card_id: u32,
    pub card_owner: AccountId,
    pub nexus_owner: Option<AccountId>,
    pub reason_hash: Hash32,
}

impl<AccountId> ProgressionAuthorityProvider<AccountId> for () {
    fn resolve_authority(
        _account: &AccountId,
        _game_id: ProgressionGameId,
        _version_id: Option<ProgressionVersionId>,
        _event_type: ProgressionEventTypeId,
    ) -> Option<ProgressionAuthorityId> {
        None
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub trait V2BenchmarkHelper {
    fn prepare_randomness();
    fn seed_finalized_randomness(request_id: Hash32, output: Hash32);
    fn seed_timed_out_randomness(request_id: Hash32);
    fn prepare_conversion_entity_profile(
        subject_id: SubjectId,
        subject_version: u32,
        rarity: CardRarity,
    );
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum AssetKind {
    Border,
    Background,
    Subject,
    Back,
    PackagingFront,
    PackagingBack,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum AssetWeightKind {
    Border,
    Background,
    Subject,
    Back,
    Packaging,
}

pub type WeightPercentage = u8;
pub type WeightMultiplier = u16;

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct AssetWeightConfig<BWeights, BMultipliers> {
    pub weights: BWeights,
    pub multipliers: BMultipliers,
}

impl<BWeights: Default, BMultipliers: Default> Default
    for AssetWeightConfig<BWeights, BMultipliers>
{
    fn default() -> Self {
        Self {
            weights: Default::default(),
            multipliers: Default::default(),
        }
    }
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct SeasonAssetsInfo<
    BBorders,
    BBackgrounds,
    BSubjects,
    BBacks,
    BPackagingFronts,
    BPackagingBacks,
    BBorderWeights,
    BBackgroundWeights,
    BSubjectWeights,
    BBackWeights,
    BPackagingWeights,
> {
    pub borders: BBorders,
    pub backgrounds: BBackgrounds,
    pub subjects: BSubjects,
    pub backs: BBacks,
    pub packaging_fronts: BPackagingFronts,
    pub packaging_backs: BPackagingBacks,
    pub border_weights: BBorderWeights,
    pub background_weights: BBackgroundWeights,
    pub subject_weights: BSubjectWeights,
    pub back_weights: BBackWeights,
    pub packaging_weights: BPackagingWeights,
}

impl<
        BBorders: Default,
        BBackgrounds: Default,
        BSubjects: Default,
        BBacks: Default,
        BPackagingFronts: Default,
        BPackagingBacks: Default,
        BBorderWeights: Default,
        BBackgroundWeights: Default,
        BSubjectWeights: Default,
        BBackWeights: Default,
        BPackagingWeights: Default,
    > Default
    for SeasonAssetsInfo<
        BBorders,
        BBackgrounds,
        BSubjects,
        BBacks,
        BPackagingFronts,
        BPackagingBacks,
        BBorderWeights,
        BBackgroundWeights,
        BSubjectWeights,
        BBackWeights,
        BPackagingWeights,
    >
{
    fn default() -> Self {
        Self {
            borders: Default::default(),
            backgrounds: Default::default(),
            subjects: Default::default(),
            backs: Default::default(),
            packaging_fronts: Default::default(),
            packaging_backs: Default::default(),
            border_weights: Default::default(),
            background_weights: Default::default(),
            subject_weights: Default::default(),
            back_weights: Default::default(),
            packaging_weights: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct CardArtworkInfo {
    pub season_id: SeasonId,
    pub border_media_id: MediaId,
    pub background_media_id: MediaId,
    pub subject_media_id: MediaId,
    pub back_media_id: MediaId,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct CardMintInfo<AccountId, BlockNumber> {
    pub minter: AccountId,
    pub minted_at: BlockNumber,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum SeasonCollectionStatus {
    Draft,
    Published,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct SeasonCollectionInfo<BName, BlockNumber> {
    pub name: BName,
    pub status: SeasonCollectionStatus,
    pub created_at: BlockNumber,
    pub published_at: Option<BlockNumber>,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum StarterPath {
    Fire,
    Earth,
    Water,
    Wind,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum NexusCardKind {
    Echo,
    Monster,
    Boss,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum NexusCardOrigin {
    StarterGrant,
    Claim,
    Pull,
    Event,
    Trial,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum NexusStorageLocation {
    Collection,
    Vault,
    Overflow,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum OverflowReason {
    SubjectCopyCapExceeded,
    VaultCapacityUnavailable,
    CollectionCapacityUnavailable,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum OverflowResolutionAction {
    MoveToCollection,
    SealToVault,
    Salvage,
}

#[derive(
    Clone,
    Copy,
    Encode,
    Decode,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    TypeInfo,
    MaxEncodedLen,
    RuntimeDebug,
)]
pub enum ResourceKind {
    EonCoins,
    GearParts,
    ElementShards,
    EchoCoreFragments,
    EchoCores,
    #[deprecated(
        note = "Seasonal forge-star progression is superseded by card progression trees."
    )]
    ForgeStars,
    MakeUpStamps,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum Element {
    Fire,
    Earth,
    Water,
    Wind,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum RankValue {
    Number(u8),
    Apex,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum ApexSide {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum RankStyleLabel {
    Balanced,
    Sharp,
    Guarded,
    Apex,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum GearSlotType {
    Weapon,
    Armor,
    Accessory,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum GearTier {
    Basic,
    Improved,
    Refined,
    Common,
    Rare,
    Epic,
    Legendary,
    Mythical,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum ProgressionNodeKind {
    Weapon,
    Armor,
    Accessory,
    Relic,
    Mastery,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum ProgressionNodeStatus {
    Locked,
    Unlocked,
    Completed,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct ProgressionNode {
    pub node_id: ProgressionNodeId,
    pub node_kind: ProgressionNodeKind,
    pub required_level: u16,
    pub required_item_template_id: ItemTemplateId,
    pub gear_slot_type: Option<GearSlotType>,
    pub power_delta: u16,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct ProgressionTree<BNodes> {
    pub tree_id: ProgressionTreeId,
    pub subject_id: SubjectId,
    pub rarity: Option<u8>,
    pub nodes: BNodes,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct CardProgression<BCompletedNodes> {
    pub card_id: u32,
    pub tree_id: ProgressionTreeId,
    pub level: u16,
    pub experience: u32,
    pub completed_nodes: BCompletedNodes,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct CardEquipmentAttachment<BlockNumber> {
    pub card_id: u32,
    pub node_id: ProgressionNodeId,
    pub gear_id: GearId,
    pub item_template_id: ItemTemplateId,
    pub attached_at: BlockNumber,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct CardMagicLoadout<BSpells> {
    pub card_id: u32,
    pub spells: BSpells,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum SpellSlotKind {
    Open,
    Element(Element),
    Locked,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum MatchMode {
    Tutorial,
    Quick,
    Ranked,
    DailyPuzzle,
    Trial,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum MatchStatus {
    Pending,
    Active,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum TrialType {
    Weapon,
    Element,
    Mana,
    Boss,
    Season,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum TrialStatus {
    Started,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum SystemKey {
    Claims,
    Pulls,
    Seal,
    Salvage,
    #[deprecated(note = "Seasonal forge paths are superseded by card progression trees.")]
    Forge,
    Progression,
    RankedRewards,
    VaultExpansion,
    Trading,
}

#[derive(
    Clone, Copy, Encode, Decode, Default, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug,
)]
pub struct GeneProfile {
    pub strength: u8,
    pub agility: u8,
    pub vitality: u8,
    pub defense: u8,
    pub magic: u8,
    pub resist: u8,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct ElementProfile {
    pub main: Element,
    pub minor: Option<Element>,
    pub resistance: Option<Element>,
    pub weakness: Option<Element>,
}

#[derive(
    Clone, Copy, Encode, Decode, Default, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug,
)]
pub struct ResourceBundle {
    pub eon_coins: u32,
    pub gear_parts: u32,
    pub element_shards: u32,
    pub echo_core_fragments: u32,
    pub echo_cores: u32,
    pub forge_stars: u32,
    pub make_up_stamps: u32,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusAccountState<BlockNumber> {
    pub starter_claimed: bool,
    pub starter_path: Option<StarterPath>,
    pub vault_capacity: u32,
    pub created_at: BlockNumber,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct StarterGrantState<BlockNumber> {
    pub path: StarterPath,
    pub grant_id: u32,
    pub claimed_at: BlockNumber,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct StarterCardTemplate {
    pub subject_id: SubjectId,
    pub base_ranks: [RankValue; 4],
    pub apex_side: Option<ApexSide>,
    pub style_label: RankStyleLabel,
    pub genes: GeneProfile,
    pub element_profile: ElementProfile,
    pub card_power: u16,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum NexusPrizeKind {
    RandomSingle,
    RandomPack,
    FeaturedSubject,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusPrizeTemplate {
    pub kind: NexusCardKind,
    pub card: StarterCardTemplate,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusPrizePool<BTemplates> {
    pub templates: BTemplates,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct CollectionCard<AccountId, BlockNumber> {
    pub owner: AccountId,
    pub subject_id: SubjectId,
    pub kind: NexusCardKind,
    pub origin: NexusCardOrigin,
    pub base_ranks: [RankValue; 4],
    pub apex_side: Option<ApexSide>,
    pub genes: GeneProfile,
    pub element_profile: ElementProfile,
    pub card_power: u16,
    pub location: NexusStorageLocation,
    pub account_bound: bool,
    pub acquired_at: BlockNumber,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct VaultVariant<BlockNumber, BMetadataUri> {
    pub variant_id: VaultVariantId,
    pub card_record_id: u32,
    pub subject_id: SubjectId,
    pub sealed_at: BlockNumber,
    pub metadata_uri: BMetadataUri,
    pub trade_eligible: bool,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct SpellSlotEntry {
    pub slot_kind: SpellSlotKind,
    pub spell_id: Option<SpellId>,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct GearItem<AccountId, BSpellSlots> {
    pub owner: AccountId,
    pub gear_id: GearId,
    pub slot_type: GearSlotType,
    pub tier: GearTier,
    pub power: u16,
    /// Deprecated compatibility field. Active Nexus magic is stored in
    /// `CardMagicLoadouts`, not embedded permanently in gear.
    pub spell_slots: BSpellSlots,
    pub equipped_card_id: Option<u32>,
    pub season_id: SeasonId,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct SpellEntry<AccountId> {
    pub owner: AccountId,
    pub spell_id: SpellId,
    pub element: Element,
    pub power: u16,
    pub slotted_to: Option<(GearId, u8)>,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct Team<AccountId, BCardIds> {
    pub owner: AccountId,
    pub team_id: TeamId,
    pub card_ids: BCardIds,
    pub team_power: u16,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct MatchState<AccountId, BPlayers> {
    pub match_id: MatchId,
    pub mode: MatchMode,
    pub board_id: BoardId,
    pub players: BPlayers,
    pub first_player: Option<AccountId>,
    pub status: MatchStatus,
    pub turn_index: u8,
    pub winner: Option<AccountId>,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct TrialState<AccountId> {
    pub account_id: AccountId,
    pub trial_id: TrialId,
    pub trial_type: TrialType,
    pub board_id: BoardId,
    pub status: TrialStatus,
    pub config_version: NexusConfigVersion,
}

#[derive(Clone, Encode, Decode, Default, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusConfigState<BlockNumber> {
    pub config_version: NexusConfigVersion,
    pub subject_copy_cap: u32,
    pub overflow_total_capacity: u32,
    pub overflow_per_subject_capacity: u32,
    pub base_vault_capacity: u32,
    pub team_size: u32,
    pub updated_at: BlockNumber,
}

#[allow(clippy::too_many_arguments)]
#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use frame_support::traits::ConstU32;
    use frame_support::transactional;
    use frame_system::pallet_prelude::BlockNumberFor;
    use pallet_alpha_access::AccessControl;
    use pallet_eterra_creatures::EntityManager;
    use pallet_eterra_randomness::VerifiableRandomness;
    use sp_runtime::traits::StaticLookup;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(16);
    const ESCROW_PALLET_ID: PalletId = PalletId(*b"et/tcgsc");
    const WEIGHT_TOTAL_PERCENT: u32 = 100;
    const DEFAULT_WEIGHT_MULTIPLIER: WeightMultiplier = 100;
    const NORMALIZED_WEIGHT_POINTS: u32 = 10_000;

    /// Balance type bound to the runtime currency.
    pub type BalanceOf<T> = <<T as Config>::PaymentCurrency as Currency<
        <T as frame_system::Config>::AccountId,
    >>::Balance;

    type BoundedBorders<T> = BoundedVec<MediaId, <T as Config>::MaxBorders>;
    type BoundedBackgrounds<T> = BoundedVec<MediaId, <T as Config>::MaxBackgrounds>;
    type BoundedSubjects<T> = BoundedVec<MediaId, <T as Config>::MaxSubjects>;
    type BoundedBacks<T> = BoundedVec<MediaId, <T as Config>::MaxBacks>;
    type BoundedPackagingFronts<T> = BoundedVec<MediaId, <T as Config>::MaxPackagingFronts>;
    type BoundedPackagingBacks<T> = BoundedVec<MediaId, <T as Config>::MaxPackagingBacks>;
    type BoundedBorderWeightConfig<T> = AssetWeightConfig<
        BoundedVec<WeightPercentage, <T as Config>::MaxBorders>,
        BoundedVec<WeightMultiplier, <T as Config>::MaxBorders>,
    >;
    type BoundedBackgroundWeightConfig<T> = AssetWeightConfig<
        BoundedVec<WeightPercentage, <T as Config>::MaxBackgrounds>,
        BoundedVec<WeightMultiplier, <T as Config>::MaxBackgrounds>,
    >;
    type BoundedSubjectWeightConfig<T> = AssetWeightConfig<
        BoundedVec<WeightPercentage, <T as Config>::MaxSubjects>,
        BoundedVec<WeightMultiplier, <T as Config>::MaxSubjects>,
    >;
    type BoundedBackWeightConfig<T> = AssetWeightConfig<
        BoundedVec<WeightPercentage, <T as Config>::MaxBacks>,
        BoundedVec<WeightMultiplier, <T as Config>::MaxBacks>,
    >;
    type BoundedPackagingWeightConfig<T> = AssetWeightConfig<
        BoundedVec<WeightPercentage, <T as Config>::MaxPackagingFronts>,
        BoundedVec<WeightMultiplier, <T as Config>::MaxPackagingFronts>,
    >;
    type BoundedSeasonCollectionName<T> = BoundedVec<u8, <T as Config>::MaxSeasonCollectionNameLen>;
    type BoundedSeasonCollectionIds<T> =
        BoundedVec<SeasonCollectionId, <T as Config>::MaxSeasonCollections>;
    pub type BoundedNexusMetadataUri<T> = BoundedVec<u8, <T as Config>::MaxNexusMetadataUriLen>;
    pub type BoundedNexusReason<T> = BoundedVec<u8, <T as Config>::MaxNexusReasonLen>;
    type BoundedNexusTeamCardIds<T> = BoundedVec<u32, <T as Config>::NexusTeamSize>;
    type BoundedNexusOverflowCards<T> = BoundedVec<u32, <T as Config>::NexusOverflowTotalCapacity>;
    type BoundedNexusSpellSlots<T> =
        BoundedVec<SpellSlotEntry, <T as Config>::MaxNexusSpellSlotsPerCard>;
    type BoundedProgressionNodes<T> =
        BoundedVec<ProgressionNode, <T as Config>::MaxProgressionNodesPerTree>;
    type BoundedProgressionNodeIds<T> =
        BoundedVec<ProgressionNodeId, <T as Config>::MaxProgressionNodesPerCard>;
    type BoundedProgressionTreeIds<T> =
        BoundedVec<ProgressionTreeId, <T as Config>::MaxProgressionTrees>;
    type BoundedMagicSpells<T> = BoundedVec<SpellId, <T as Config>::MaxMagicSlotsPerCard>;
    type BoundedMagicSpellSet<T> = BoundedBTreeSet<SpellId, <T as Config>::MaxMagicSlotsPerCard>;
    type BoundedNexusMatchPlayers<T> =
        BoundedVec<<T as frame_system::Config>::AccountId, <T as Config>::MaxNexusMatchPlayers>;
    type SeasonAssetsInfoOf<T> = SeasonAssetsInfo<
        BoundedBorders<T>,
        BoundedBackgrounds<T>,
        BoundedSubjects<T>,
        BoundedBacks<T>,
        BoundedPackagingFronts<T>,
        BoundedPackagingBacks<T>,
        BoundedBorderWeightConfig<T>,
        BoundedBackgroundWeightConfig<T>,
        BoundedSubjectWeightConfig<T>,
        BoundedBackWeightConfig<T>,
        BoundedPackagingWeightConfig<T>,
    >;
    type SeasonCollectionInfoOf<T> =
        SeasonCollectionInfo<BoundedSeasonCollectionName<T>, BlockNumberFor<T>>;
    type NexusAccountStateOf<T> = NexusAccountState<BlockNumberFor<T>>;
    type StarterGrantStateOf<T> = StarterGrantState<BlockNumberFor<T>>;
    type BoundedStarterTeamCards<T> = BoundedVec<StarterCardTemplate, <T as Config>::NexusTeamSize>;
    type BoundedNexusPrizeTemplates<T> = BoundedVec<NexusPrizeTemplate, <T as Config>::MaxSubjects>;
    type NexusPrizePoolOf<T> = NexusPrizePool<BoundedNexusPrizeTemplates<T>>;
    type CollectionCardOf<T> =
        CollectionCard<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>;
    type VaultVariantOf<T> = VaultVariant<BlockNumberFor<T>, BoundedNexusMetadataUri<T>>;
    type GearItemOf<T> =
        GearItem<<T as frame_system::Config>::AccountId, BoundedNexusSpellSlots<T>>;
    type SpellEntryOf<T> = SpellEntry<<T as frame_system::Config>::AccountId>;
    type ProgressionTreeOf<T> = ProgressionTree<BoundedProgressionNodes<T>>;
    type CardProgressionOf<T> = CardProgression<BoundedProgressionNodeIds<T>>;
    type CardEquipmentAttachmentOf<T> = CardEquipmentAttachment<BlockNumberFor<T>>;
    type CardMagicLoadoutOf<T> = CardMagicLoadout<BoundedMagicSpells<T>>;
    type TeamOf<T> = Team<<T as frame_system::Config>::AccountId, BoundedNexusTeamCardIds<T>>;
    type MatchStateOf<T> =
        MatchState<<T as frame_system::Config>::AccountId, BoundedNexusMatchPlayers<T>>;
    type TrialStateOf<T> = TrialState<<T as frame_system::Config>::AccountId>;
    type NexusConfigStateOf<T> = NexusConfigState<BlockNumberFor<T>>;
    type CardInstanceV2Of<T> =
        CardInstanceV2<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>;
    type PackSkuVersionOf<T> = PackSkuVersion<BlockNumberFor<T>>;
    type PackCreditOf<T> = PackCredit<<T as frame_system::Config>::AccountId>;
    type PoolProfileEntriesOf<T> = BoundedVec<PoolProfileEntry, <T as Config>::MaxV2PoolProfiles>;
    type PoolPoseIdsOf<T> = BoundedVec<u32, <T as Config>::MaxV2PoolPoses>;
    type PoolBackgroundIdsOf<T> = BoundedVec<u32, <T as Config>::MaxV2PoolBackgrounds>;
    type AcquisitionPoolVersionOf<T> =
        AcquisitionPoolVersion<PoolProfileEntriesOf<T>, PoolPoseIdsOf<T>, PoolBackgroundIdsOf<T>>;
    type PendingPackOpeningOf<T> =
        PendingPackOpening<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>;
    type TutorialConversionProfileIdsOf<T> = BoundedVec<u32, <T as Config>::MaxV2PoolProfiles>;
    type AvailableCreditIdsOf<T> = BoundedVec<u64, <T as Config>::MaxV2CreditsPerAccountSku>;
    type ProtectionBitmapOf<T> = BoundedVec<u8, <T as Config>::MaxV2ProtectionBytes>;
    type V2TeamCardsOf<T> = BoundedVec<CardIdV2, <T as Config>::MaxV2TeamSize>;
    type CompetitiveTeamV2Of<T> =
        CompetitiveTeamV2<<T as frame_system::Config>::AccountId, V2TeamCardsOf<T>>;
    type ConversionTombstoneOf<T> =
        ConversionTombstone<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>;
    type MythicalAscensionSeasonConfigOf<T> = MythicalAscensionSeasonConfig<BlockNumberFor<T>>;
    type MythicalAscensionReceiptOf<T> =
        MythicalAscensionReceipt<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>;
    type LegacyClassificationOf<T> =
        LegacyCardClassification<<T as frame_system::Config>::AccountId>;
    type MigrationAnomalyOf<T> = TcgMigrationAnomalyV16<<T as frame_system::Config>::AccountId>;

    #[derive(Clone, Copy)]
    struct SelectedSeasonAsset {
        collection_id: SeasonCollectionId,
        media_id: MediaId,
        selection_weight: u32,
    }

    #[allow(dead_code)]
    #[derive(Clone, Copy)]
    struct SelectedPackagingAsset {
        collection_id: SeasonCollectionId,
        front_media_id: MediaId,
        back_media_id: MediaId,
        selection_weight: u32,
    }

    #[derive(Default)]
    struct PublishedSeasonAssetPools {
        borders: Vec<SelectedSeasonAsset>,
        backgrounds: Vec<SelectedSeasonAsset>,
        subjects: Vec<SelectedSeasonAsset>,
        backs: Vec<SelectedSeasonAsset>,
        packagings: Vec<SelectedPackagingAsset>,
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // ------------------
    // Pallet Config
    // ------------------

    #[pallet::config]
    pub trait Config:
        frame_system::Config
        + pallet_eterra_seasons::Config
        + pallet_eterra_media::Config
        + pallet_nfts::Config<CollectionId = u32, ItemId = u32>
    {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Currency used to charge for minting packs.
        type PaymentCurrency: Currency<Self::AccountId>;

        /// Canonical Alpha access gate for player-facing calls.
        type AccessControl: pallet_alpha_access::AccessControl<Self::AccountId>;

        /// A runtime-provided hook for checking whether a card is currently part of the owner's
        /// gameplay "current hand".
        type HandChecker: crate::HandChecker<Self::AccountId>;

        /// Runtime-provided authority resolver for game/reward progression XP issuers.
        type ProgressionAuthorityProvider: crate::ProgressionAuthorityProvider<Self::AccountId>;

        /// Delayed domain-separated randomness used by V2 packs and conversion.
        type V2Randomness: pallet_eterra_randomness::VerifiableRandomness;

        /// Chain identity used by the locked V2 draw and conversion transcript.
        type V2ChainDomain: crate::V2ChainDomainProvider;

        #[cfg(feature = "runtime-benchmarks")]
        type V2BenchmarkHelper: crate::V2BenchmarkHelper;

        /// Embodied entity provider. Conversion reserves an ID before randomness.
        type V2Entities: pallet_eterra_creatures::EntityManager<
            Self::AccountId,
            BlockNumberFor<Self>,
        >;

        /// Known legacy escrow lookup used by the V14/V15→V16 custody repair.
        type LegacyEscrowOwnerProvider: crate::LegacyEscrowOwnerProvider<Self::AccountId>;

        /// Fixed pack mint price (in native `COIN` base units).
        #[pallet::constant]
        type PackPrice: Get<BalanceOf<Self>>;

        /// Account that receives pack mint payments.
        #[pallet::constant]
        type PackPriceReceiver: Get<Self::AccountId>;

        /// Fixed "pro" mint price (in native `COIN` base units).
        #[pallet::constant]
        type ProPrice: Get<BalanceOf<Self>>;

        /// Account that receives "pro" mint payments.
        #[pallet::constant]
        type ProPriceReceiver: Get<Self::AccountId>;

        /// Fixed single-card mint price (in native `COIN` base units).
        #[pallet::constant]
        type MintCardPrice: Get<BalanceOf<Self>>;

        /// Account that receives single-card mint payments.
        #[pallet::constant]
        type MintCardPriceReceiver: Get<Self::AccountId>;

        /// Maximum number of spins allowed for a "pro" card mint.
        #[pallet::constant]
        type MaxProSpins: Get<u8>;

        /// The maximum times a card can generate slots before it is forced to finalize.
        #[pallet::constant]
        type MaxAttempts: Get<u8>;

        /// How many cards are in each newly minted pack.
        #[pallet::constant]
        type CardsPerPack: Get<u8>;

        /// The maximum number of cards a single account can own.
        ///
        /// This bounds storage reads for dashboards that list cards by owner.
        #[pallet::constant]
        type MaxOwnedCards: Get<u32>;

        /// Base card capacity available to every account before buying extra storage.
        #[pallet::constant]
        type BaseCardCapacity: Get<u32>;

        /// Slots added per storage upgrade purchase.
        #[pallet::constant]
        type CardCapacityUpgradeAmount: Get<u32>;

        /// Price charged for each storage upgrade purchase.
        #[pallet::constant]
        type CardCapacityUpgradePrice: Get<BalanceOf<Self>>;

        /// Account that receives storage upgrade payments.
        #[pallet::constant]
        type CardCapacityUpgradePriceReceiver: Get<Self::AccountId>;

        /// Maximum number of border layers per season.
        #[pallet::constant]
        type MaxBorders: Get<u32>;

        /// Maximum number of background layers per season.
        #[pallet::constant]
        type MaxBackgrounds: Get<u32>;

        /// Maximum number of subject layers per season.
        #[pallet::constant]
        type MaxSubjects: Get<u32>;

        /// Maximum number of back layers per season.
        #[pallet::constant]
        type MaxBacks: Get<u32>;

        /// Maximum number of packaging front images per season.
        #[pallet::constant]
        type MaxPackagingFronts: Get<u32>;

        /// Maximum number of packaging back images per season.
        #[pallet::constant]
        type MaxPackagingBacks: Get<u32>;

        /// Maximum number of art collections per season.
        #[pallet::constant]
        type MaxSeasonCollections: Get<u32>;

        /// Maximum byte length of a season art collection name.
        #[pallet::constant]
        type MaxSeasonCollectionNameLen: Get<u32>;

        /// Number of cards in a legal Nexus Season 1 team.
        #[pallet::constant]
        type NexusTeamSize: Get<u32>;

        /// Maximum Collection + Vault copies of the same subject.
        #[pallet::constant]
        type NexusSubjectCopyCap: Get<u32>;

        /// Maximum total cards waiting in Overflow.
        #[pallet::constant]
        type NexusOverflowTotalCapacity: Get<u32>;

        /// Maximum Overflow copies for one subject.
        #[pallet::constant]
        type NexusOverflowPerSubjectCapacity: Get<u32>;

        /// Base capacity for sealed Vault Variants.
        #[pallet::constant]
        type NexusBaseVaultCapacity: Get<u32>;

        /// Maximum metadata URI length for sealed Vault Variant display metadata.
        #[pallet::constant]
        type MaxNexusMetadataUriLen: Get<u32>;

        /// Maximum byte length of runtime-facing reasons in Nexus events.
        #[pallet::constant]
        type MaxNexusReasonLen: Get<u32>;

        /// Maximum number of spell slots a card build can use in Season 1.
        #[pallet::constant]
        type MaxNexusSpellSlotsPerCard: Get<u32>;

        /// Maximum nodes in a configured card progression tree.
        #[pallet::constant]
        type MaxProgressionNodesPerTree: Get<u32>;

        /// Maximum completed progression nodes tracked per card.
        #[pallet::constant]
        type MaxProgressionNodesPerCard: Get<u32>;

        /// Maximum removable magic spells in one card loadout.
        #[pallet::constant]
        type MaxMagicSlotsPerCard: Get<u32>;

        /// Maximum configured progression trees tracked by this pallet.
        #[pallet::constant]
        type MaxProgressionTrees: Get<u32>;

        /// XP required for each deterministic card level step.
        #[pallet::constant]
        type CardXpPerLevel: Get<u32>;

        /// Maximum XP that can be granted to one card in a single authorized call.
        #[pallet::constant]
        type MaxCardXpGrantAmount: Get<u32>;

        /// Maximum players tracked by a Nexus match state.
        #[pallet::constant]
        type MaxNexusMatchPlayers: Get<u32>;

        #[pallet::constant]
        type MaxV2PoolProfiles: Get<u32>;
        #[pallet::constant]
        type MaxV2PoolPoses: Get<u32>;
        #[pallet::constant]
        type MaxV2PoolBackgrounds: Get<u32>;
        #[pallet::constant]
        type MaxV2CreditsPerAccountSku: Get<u32>;
        #[pallet::constant]
        type MaxV2ProtectionBytes: Get<u32>;
        #[pallet::constant]
        type MaxV2TeamSize: Get<u32>;
        /// Emit an operational warning when the lifetime stored-card count
        /// first crosses this threshold.
        #[pallet::constant]
        type V2OperationalCardWarningThreshold: Get<u64>;
        /// Maximum lifetime V2 card records permitted for one account before
        /// pending pack entitlements must remain unopened for review.
        #[pallet::constant]
        type V2OperationalCardLimit: Get<u64>;
        #[pallet::constant]
        type V16MigrationBatchSize: Get<u32>;
        #[pallet::constant]
        type MinimumActiveCardsAfterConversion: Get<u32>;
        /// Maximum non-terminal conversion commitments owned by one account.
        #[pallet::constant]
        type MaxPendingConversionsPerAccount: Get<u32>;
        /// Exact 90-day season duration in runtime blocks.
        #[pallet::constant]
        type MythicalAscensionSeasonDurationBlocks: Get<BlockNumberFor<Self>>;
        /// Exact seven-day Convergence Mark epoch in runtime blocks.
        #[pallet::constant]
        type MythicalAscensionWeekDurationBlocks: Get<BlockNumberFor<Self>>;

        /// Weight information for this pallet's extrinsics.
        type WeightInfo: WeightInfo;
    }

    // ------------------
    // Data Structures
    // ------------------

    /// The info stored about each card.
    #[derive(Clone, Encode, Decode, Default, PartialEq, TypeInfo, MaxEncodedLen, Debug)]
    pub struct CardInfo<AccountId> {
        owner: AccountId,
        finalized: bool,
        /// Directional ranks in `[north, east, south, west]` order.
        slot_values: Option<[u8; 4]>,
    }

    impl<AccountId> CardInfo<AccountId> {
        pub fn get_owner(&self) -> &AccountId {
            &self.owner
        }

        pub fn is_finalized(&self) -> bool {
            self.finalized
        }

        pub fn get_slot_values(&self) -> Option<[u8; 4]> {
            self.slot_values
        }
    }

    /// A "Pack" just references existing cards by their IDs, rather than embedding them.
    #[derive(Clone, Encode, Decode, Default, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct Pack {
        id: u32,
        // Store the IDs of the cards that were originally minted in this pack
        card_ids: BoundedVec<u32, ConstU32<16>>,
        active_card_index: u8,
        completed: bool,
    }

    impl Pack {
        pub fn get_id(&self) -> u32 {
            self.id
        }

        pub fn get_card_ids(&self) -> &BoundedVec<u32, ConstU32<16>> {
            &self.card_ids
        }

        pub fn get_active_card_index(&self) -> u8 {
            self.active_card_index
        }

        pub fn get_completed(&self) -> bool {
            self.completed
        }
    }

    // ------------------
    // Storage
    // ------------------

    /// A global counter to assign unique IDs to cards.
    #[pallet::storage]
    #[pallet::getter(fn next_card_id)]
    pub type NextCardId<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// All cards in the system, by global `card_id` => `CardInfo`.
    #[pallet::storage]
    #[pallet::getter(fn cards)]
    pub type Cards<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CardInfo<T::AccountId>, OptionQuery>;

    /// Ordered collection ids for each season.
    #[pallet::storage]
    #[pallet::getter(fn season_collection_ids)]
    pub type SeasonCollectionIds<T: Config> =
        StorageMap<_, Blake2_128Concat, SeasonId, BoundedSeasonCollectionIds<T>, ValueQuery>;

    /// Next collection id to use for a given season.
    #[pallet::storage]
    #[pallet::getter(fn next_season_collection_id)]
    pub type NextSeasonCollectionId<T: Config> =
        StorageMap<_, Blake2_128Concat, SeasonId, SeasonCollectionId, ValueQuery>;

    /// Season-scoped collection metadata.
    #[pallet::storage]
    #[pallet::getter(fn season_collections)]
    pub type SeasonCollections<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        SeasonId,
        Blake2_128Concat,
        SeasonCollectionId,
        SeasonCollectionInfoOf<T>,
        OptionQuery,
    >;

    /// Artwork assets contained within a season collection.
    #[pallet::storage]
    #[pallet::getter(fn season_collection_assets)]
    pub type SeasonCollectionAssets<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        SeasonId,
        Blake2_128Concat,
        SeasonCollectionId,
        SeasonAssetsInfoOf<T>,
        ValueQuery,
    >;

    /// Immutable assigned artwork for each card: `card_id => CardArtworkInfo`.
    #[pallet::storage]
    #[pallet::getter(fn card_artwork)]
    pub type CardArtwork<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CardArtworkInfo, OptionQuery>;

    /// The season collection used to assign artwork for a card, when applicable.
    #[pallet::storage]
    #[pallet::getter(fn card_artwork_collection_id)]
    pub type CardArtworkCollectionId<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, SeasonCollectionId, OptionQuery>;

    /// Original mint provenance for each card.
    #[pallet::storage]
    #[pallet::getter(fn card_mint_info)]
    pub type CardMintInfoByCard<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u32,
        CardMintInfo<T::AccountId, BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// Deterministic card genome used by escrow-driven enemy spawning.
    #[pallet::storage]
    #[pallet::getter(fn card_genome)]
    pub type CardGenome<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CardGenomeHash, OptionQuery>;

    /// The NFT collection ID used for converted cards (single collection).
    #[pallet::storage]
    #[pallet::getter(fn card_nft_collection_id)]
    pub type CardNftCollectionId<T: Config> = StorageValue<_, u32, OptionQuery>;

    /// Tracks cards that have been converted to NFTs: `card_id => ()`.
    #[pallet::storage]
    #[pallet::getter(fn converted)]
    pub type Converted<T: Config> = StorageMap<_, Blake2_128Concat, u32, (), OptionQuery>;

    /// Additional card capacity purchased by each account.
    #[pallet::storage]
    #[pallet::getter(fn card_capacity_bonus)]
    pub type CardCapacityBonus<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    /// A map from account => list of currently in-progress packs.
    #[pallet::storage]
    #[pallet::getter(fn player_packs)]
    pub type PlayerPacks<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<Pack, T::MaxOwnedCards>,
        ValueQuery,
    >;

    /// A map from account => set of owned card IDs.
    ///
    /// This is a secondary index to support efficient front-end queries like
    /// "show me all cards owned by this account", including cards minted via pro minting.
    #[pallet::storage]
    #[pallet::getter(fn cards_by_owner)]
    pub type CardsByOwner<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedBTreeSet<u32, T::MaxOwnedCards>,
        ValueQuery,
    >;

    /// A map of cards that are up for sale: `card_id => price`.
    #[pallet::storage]
    #[pallet::getter(fn card_prices)]
    pub type CardPrices<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, BalanceOf<T>, OptionQuery>;

    /// Index of cards a given owner has listed for sale.
    #[pallet::storage]
    #[pallet::getter(fn listed_by_owner)]
    pub type ListedByOwner<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedBTreeSet<u32, T::MaxOwnedCards>,
        ValueQuery,
    >;

    /// Tracks whether an account has ever minted at least one card or pack.
    #[pallet::storage]
    #[pallet::getter(fn has_minted)]
    pub type HasMinted<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, (), OptionQuery>;

    /// Total number of distinct accounts that have minted at least one card or pack.
    #[pallet::storage]
    #[pallet::getter(fn unique_minter_count)]
    pub type UniqueMinterCount<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// Tracks the currently “active” card index (within a pack) for each account
    #[pallet::storage]
    #[pallet::getter(fn active_card)]
    pub type ActiveCard<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, Option<u8>, ValueQuery>;

    /// Tracks the caller's currently in-progress pack mint, if any.
    ///
    /// This makes it easy for the front end to resume a minting flow after refresh.
    #[pallet::storage]
    #[pallet::getter(fn pack_in_progress)]
    pub type PackInProgress<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, OptionQuery>;

    /// Tracks the caller's currently active card ID within the pack mint in progress, if any.
    #[pallet::storage]
    #[pallet::getter(fn pack_card_in_progress)]
    pub type PackCardInProgress<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, OptionQuery>;

    /// Stores the attempt count for each card: `card_id => current attempts`.
    /// We omit the account ID here because the card can be traded to another owner.
    #[pallet::storage]
    #[pallet::getter(fn card_attempts)]
    pub type CardAttempts<T: Config> = StorageMap<_, Blake2_128Concat, u32, u8, ValueQuery>;

    /// Tracks the caller's currently in-progress "pro mint" card ID, if any.
    #[pallet::storage]
    #[pallet::getter(fn pro_in_progress)]
    pub type ProInProgress<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, OptionQuery>;

    /// Account-level Nexus Season 1 state.
    #[pallet::storage]
    #[pallet::getter(fn nexus_account_state)]
    pub type NexusAccountStates<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, NexusAccountStateOf<T>, OptionQuery>;

    /// Starter Grant state by account.
    #[pallet::storage]
    #[pallet::getter(fn starter_grant)]
    pub type StarterGrants<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, StarterGrantStateOf<T>, OptionQuery>;

    /// Next Starter Grant id used by the Nexus state skeleton.
    #[pallet::storage]
    #[pallet::getter(fn next_starter_grant_id)]
    pub type NextStarterGrantId<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// Root-configured internal-alpha starter team templates by path.
    #[pallet::storage]
    #[pallet::getter(fn starter_team_config)]
    pub type StarterTeamConfigs<T: Config> =
        StorageMap<_, Blake2_128Concat, StarterPath, BoundedStarterTeamCards<T>, OptionQuery>;

    /// Versioned subject/result templates used by the shared Prize Counter and
    /// Vending Machine acquisition path.
    #[pallet::storage]
    #[pallet::getter(fn nexus_prize_pool)]
    pub type NexusPrizePools<T: Config> =
        StorageMap<_, Blake2_128Concat, NexusPrizePoolId, NexusPrizePoolOf<T>, OptionQuery>;

    #[pallet::storage]
    pub type NextNexusPullId<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// Nexus Collection card records keyed by runtime card id.
    #[pallet::storage]
    #[pallet::getter(fn nexus_collection_card)]
    pub type NexusCollectionCards<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CollectionCardOf<T>, OptionQuery>;

    /// Sealed Vault Variant records keyed by variant id.
    #[pallet::storage]
    #[pallet::getter(fn vault_variant)]
    pub type VaultVariants<T: Config> =
        StorageMap<_, Blake2_128Concat, VaultVariantId, VaultVariantOf<T>, OptionQuery>;

    /// Next sealed Vault Variant id.
    #[pallet::storage]
    #[pallet::getter(fn next_vault_variant_id)]
    pub type NextVaultVariantId<T: Config> = StorageValue<_, VaultVariantId, ValueQuery>;

    /// Collection + Vault subject copy counts. Overflow is tracked separately.
    #[pallet::storage]
    #[pallet::getter(fn nexus_subject_copy_count)]
    pub type NexusSubjectCopyCounts<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        SubjectId,
        u32,
        ValueQuery,
    >;

    /// Cards waiting in Overflow for an account.
    #[pallet::storage]
    #[pallet::getter(fn nexus_overflow_cards)]
    pub type NexusOverflowCards<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BoundedNexusOverflowCards<T>, ValueQuery>;

    /// Per-subject Overflow counts by account.
    #[pallet::storage]
    #[pallet::getter(fn nexus_overflow_subject_count)]
    pub type NexusOverflowSubjectCounts<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        SubjectId,
        u32,
        ValueQuery,
    >;

    /// Account resource balances for Nexus-only non-tradable resources.
    #[pallet::storage]
    #[pallet::getter(fn nexus_resource_balance)]
    pub type NexusResources<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        ResourceKind,
        u32,
        ValueQuery,
    >;

    /// Nexus gear inventory records.
    #[pallet::storage]
    #[pallet::getter(fn nexus_gear_item)]
    pub type NexusGearItems<T: Config> =
        StorageMap<_, Blake2_128Concat, GearId, GearItemOf<T>, OptionQuery>;

    /// Nexus spellbook records.
    #[pallet::storage]
    #[pallet::getter(fn nexus_spell_entry)]
    pub type NexusSpellbook<T: Config> =
        StorageMap<_, Blake2_128Concat, SpellId, SpellEntryOf<T>, OptionQuery>;

    /// Configured card-specific progression trees.
    #[pallet::storage]
    #[pallet::getter(fn progression_tree)]
    pub type ProgressionTrees<T: Config> =
        StorageMap<_, Blake2_128Concat, ProgressionTreeId, ProgressionTreeOf<T>, OptionQuery>;

    /// Bounded index of configured progression tree ids.
    #[pallet::storage]
    #[pallet::getter(fn progression_tree_ids)]
    pub type ProgressionTreeIds<T: Config> =
        StorageValue<_, BoundedProgressionTreeIds<T>, ValueQuery>;

    /// Number of cards initialized against a progression tree.
    #[pallet::storage]
    #[pallet::getter(fn progression_tree_use_count)]
    pub type ProgressionTreeUseCounts<T: Config> =
        StorageMap<_, Blake2_128Concat, ProgressionTreeId, u32, ValueQuery>;

    /// Subject/rarity lookup for automatic card progression-tree assignment.
    #[pallet::storage]
    #[pallet::getter(fn progression_tree_by_subject)]
    pub type ProgressionTreeBySubject<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        SubjectId,
        Blake2_128Concat,
        Option<u8>,
        ProgressionTreeId,
        OptionQuery,
    >;

    /// Per-card progression state.
    #[pallet::storage]
    #[pallet::getter(fn card_progression)]
    pub type CardProgressions<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CardProgressionOf<T>, OptionQuery>;

    /// Permanent equipment attachments by card and progression node.
    #[pallet::storage]
    #[pallet::getter(fn card_equipment_attachment)]
    pub type CardEquipmentAttachments<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u32,
        Blake2_128Concat,
        ProgressionNodeId,
        CardEquipmentAttachmentOf<T>,
        OptionQuery,
    >;

    /// Removable magic loadout by card.
    #[pallet::storage]
    #[pallet::getter(fn card_magic_loadout)]
    pub type CardMagicLoadouts<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CardMagicLoadoutOf<T>, OptionQuery>;

    /// Item-template sidecar for gear records, preserving existing gear encoding.
    #[pallet::storage]
    #[pallet::getter(fn gear_item_template)]
    pub type GearItemTemplates<T: Config> =
        StorageMap<_, Blake2_128Concat, GearId, ItemTemplateId, OptionQuery>;

    /// Saved Nexus teams by owner and team id.
    #[pallet::storage]
    #[pallet::getter(fn nexus_team)]
    pub type NexusTeams<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        TeamId,
        TeamOf<T>,
        OptionQuery,
    >;

    /// Nexus match state records.
    #[pallet::storage]
    #[pallet::getter(fn nexus_match)]
    pub type NexusMatches<T: Config> =
        StorageMap<_, Blake2_128Concat, MatchId, MatchStateOf<T>, OptionQuery>;

    /// Nexus trial state by account and trial id.
    #[pallet::storage]
    #[pallet::getter(fn nexus_trial)]
    pub type NexusTrials<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        TrialId,
        TrialStateOf<T>,
        OptionQuery,
    >;

    /// Optional persisted Nexus config snapshot. When absent, runtime constants are canonical.
    #[pallet::storage]
    #[pallet::getter(fn nexus_config)]
    pub type NexusConfig<T: Config> = StorageValue<_, NexusConfigStateOf<T>, OptionQuery>;

    // ------------------
    // Nexus V2 parallel state (legacy encodings above remain unchanged)
    // ------------------

    #[pallet::storage]
    #[pallet::getter(fn subject_definition_v2)]
    pub type SubjectDefinitionsV2<T> =
        StorageMap<_, Blake2_128Concat, u32, SubjectDefinitionV2, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn subject_definition_v2_by_key)]
    pub type SubjectDefinitionByKeyV2<T> =
        StorageMap<_, Blake2_128Concat, (SubjectId, u32), u32, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn subject_activation_v2)]
    pub type SubjectActivationStatesV2<T> =
        StorageMap<_, Blake2_128Concat, u32, SubjectActivationState, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn subject_rarity_profile_v2)]
    pub type SubjectRarityProfilesV2<T> =
        StorageMap<_, Blake2_128Concat, u32, SubjectRarityProfile, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn subject_rarity_profile_v2_by_key)]
    pub type SubjectRarityProfileByKeyV2<T> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        (SubjectId, u32),
        Blake2_128Concat,
        CardRarity,
        u32,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn pose_definition_v2)]
    pub type PoseDefinitionsV2<T> =
        StorageMap<_, Blake2_128Concat, u32, MediaDefinitionV2, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn background_definition_v2)]
    pub type BackgroundDefinitionsV2<T> =
        StorageMap<_, Blake2_128Concat, u32, MediaDefinitionV2, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn acquisition_pool_version_v2)]
    pub type AcquisitionPoolVersionsV2<T: Config> =
        StorageMap<_, Blake2_128Concat, (u32, u32), AcquisitionPoolVersionOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pack_sku_version_v2)]
    pub type PackSkuVersionsV2<T: Config> =
        StorageMap<_, Blake2_128Concat, (u32, u32), PackSkuVersionOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_card_id_v2)]
    pub type NextCardIdV2<T> = StorageValue<_, CardIdV2, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn card_v2)]
    pub type CardsV2<T: Config> =
        StorageMap<_, Blake2_128Concat, CardIdV2, CardInstanceV2Of<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn v2_owner_card_count)]
    pub type V2OwnerCardCount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn v2_owner_active_card_count)]
    pub type V2OwnerActiveCardCount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    /// Card slots reserved by committed pack openings. This closes the race
    /// between multiple pending openings at the operational account limit.
    #[pallet::storage]
    #[pallet::getter(fn reserved_v2_pack_card_count)]
    pub type ReservedV2PackCardCount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_serial_v2)]
    pub type NextSerialV2<T> =
        StorageMap<_, Blake2_128Concat, (SubjectId, CardRarity), u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_pack_credit_id_v2)]
    pub type NextPackCreditIdV2<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pack_credit_v2)]
    pub type PackCreditsV2<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, PackCreditOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn available_pack_credit_ids_v2)]
    pub type AvailablePackCreditIdsV2<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        (u32, u32, EconomicRealm),
        AvailableCreditIdsOf<T>,
        ValueQuery,
    >;

    /// Includes available and currently locked credits so a timed-out opening
    /// always has a reserved queue slot for restoring its exact entitlement.
    #[pallet::storage]
    #[pallet::getter(fn outstanding_pack_credit_count_v2)]
    pub type OutstandingPackCreditCountV2<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        (u32, u32, EconomicRealm),
        u32,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn pending_pack_opening_v2)]
    pub type PendingPackOpeningsV2<T: Config> =
        StorageMap<_, Blake2_128Concat, Hash32, PendingPackOpeningOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pack_opening_request_receipt_v2)]
    pub type PackOpeningRequestReceiptsV2<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        Hash32,
        PackOpeningRequestReceipt,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn tutorial_pack_credit_grant_receipt_v2)]
    pub type TutorialPackCreditGrantReceiptsV2<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        Hash32,
        TutorialPackCreditGrantReceipt<T::AccountId>,
        OptionQuery,
    >;

    /// Permanent marker used to make permissionless pack-timeout retries
    /// idempotent after the pending record has been removed.
    #[pallet::storage]
    #[pallet::getter(fn timed_out_pack_opening_v2)]
    pub type TimedOutPackOpeningsV2<T> = StorageMap<_, Blake2_128Concat, Hash32, u64, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn locked_pack_credit_v2)]
    pub type LockedPackCreditsV2<T: Config> =
        StorageMap<_, Blake2_128Concat, Hash32, PackCreditOf<T>, OptionQuery>;

    /// The conversion-ready Common candidates frozen when a tutorial opening
    /// commits. Later activation changes cannot strand that pending opening.
    #[pallet::storage]
    #[pallet::getter(fn tutorial_conversion_profile_ids_v2)]
    pub type TutorialConversionProfileIdsV2<T: Config> =
        StorageMap<_, Blake2_128Concat, Hash32, TutorialConversionProfileIdsOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn processed_acquisition_v2)]
    pub type ProcessedAcquisitionsV2<T> =
        StorageMap<_, Blake2_128Concat, Hash32, BoundedVec<CardIdV2, ConstU32<6>>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pack_protection_bitmap_v2)]
    pub type PackProtectionHistoryBitmapsV2<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        u32,
        ProtectionBitmapOf<T>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn cosmetic_protection_bitmap_v2)]
    pub type CosmeticProtectionBitmapsV2<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        u32,
        ProtectionBitmapOf<T>,
        ValueQuery,
    >;

    /// Stable, append-only subject positions for a set's protection bitmap.
    /// Pool versions may reorder or temporarily omit entries without changing
    /// the meaning of an already-written protection bit.
    #[pallet::storage]
    #[pallet::getter(fn next_subject_protection_slot_v2)]
    pub type NextSubjectProtectionSlotV2<T> = StorageMap<_, Blake2_128Concat, u32, u16, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn subject_protection_slot_v2)]
    pub type SubjectProtectionSlotsV2<T> =
        StorageDoubleMap<_, Blake2_128Concat, u32, Blake2_128Concat, SubjectId, u16, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_pose_protection_slot_v2)]
    pub type NextPoseProtectionSlotV2<T> =
        StorageMap<_, Blake2_128Concat, (u32, SubjectId), u8, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pose_protection_slot_v2)]
    pub type PoseProtectionSlotsV2<T> =
        StorageDoubleMap<_, Blake2_128Concat, u32, Blake2_128Concat, u32, u8, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_background_protection_slot_v2)]
    pub type NextBackgroundProtectionSlotV2<T> =
        StorageMap<_, Blake2_128Concat, u32, u8, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn background_protection_slot_v2)]
    pub type BackgroundProtectionSlotsV2<T> =
        StorageDoubleMap<_, Blake2_128Concat, u32, Blake2_128Concat, u32, u8, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn live_supply_v2)]
    pub type LiveSupplyBySubjectRarityV2<T> =
        StorageMap<_, Blake2_128Concat, (SubjectId, CardRarity, EconomicRealm), u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn converted_supply_v2)]
    pub type ConvertedSupplyBySubjectRarityV2<T> =
        StorageMap<_, Blake2_128Concat, (SubjectId, CardRarity, EconomicRealm), u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn competitive_format_v2)]
    pub type CompetitiveFormatsV2<T> =
        StorageMap<_, Blake2_128Concat, (u32, u32), CompetitiveFormatV2, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn competitive_team_v2)]
    pub type CompetitiveTeamsV2<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        u32,
        CompetitiveTeamV2Of<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn v2_feature_enabled)]
    pub type V2FeatureEnabled<T> = StorageMap<_, Blake2_128Concat, V2Feature, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn card_conversion_tombstone)]
    pub type CardConversionTombstones<T: Config> =
        StorageMap<_, Blake2_128Concat, Hash32, ConversionTombstoneOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn conversion_request_by_card)]
    pub type ConversionRequestByCard<T> =
        StorageMap<_, Blake2_128Concat, CardIdV2, Hash32, OptionQuery>;

    /// The most recently saved, fully validated Bring-5 roster for an account,
    /// set, and realm. Conversion revalidates the referenced team from current
    /// card state and fails closed when this pointer or its roster is stale.
    #[pallet::storage]
    #[pallet::getter(fn conversion_safety_team_v2)]
    pub type ConversionSafetyTeamByRealmSetV2<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        (u32, EconomicRealm),
        u32,
        OptionQuery,
    >;

    /// Bounded liveness sidecar for non-terminal conversion commitments.
    #[pallet::storage]
    #[pallet::getter(fn pending_conversion_count_v2)]
    pub type PendingConversionCountByAccountV2<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn mythical_ascension_season_config)]
    pub type MythicalAscensionSeasonConfigsV2<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, MythicalAscensionSeasonConfigOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn mythical_ascension_subject_config)]
    pub type MythicalAscensionSubjectConfigsV2<T> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u32,
        Blake2_128Concat,
        SubjectId,
        MythicalAscensionSubjectConfig,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn season_eligibility_for_account)]
    pub type SeasonEligibilityByAccountV2<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        u32,
        Hash32,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn season_eligibility_registered)]
    pub type RegisteredSeasonEligibilityV2<T> =
        StorageDoubleMap<_, Blake2_128Concat, u32, Blake2_128Concat, Hash32, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn mythical_subject_mastery)]
    pub type MythicalSubjectMasteryV2<T> =
        StorageMap<_, Blake2_128Concat, (Hash32, u32, SubjectId), u8, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn legendary_foundation_available)]
    pub type LegendaryFoundationsV2<T> =
        StorageMap<_, Blake2_128Concat, (Hash32, u32, SubjectId), bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn convergence_progress)]
    pub type ConvergenceProgressV2<T> =
        StorageMap<_, Blake2_128Concat, (Hash32, u32), ConvergenceProgress, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn mythic_catalyst_available)]
    pub type MythicCatalystsV2<T> =
        StorageMap<_, Blake2_128Concat, (Hash32, u32), bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn processed_ascension_progress_evidence)]
    pub type ProcessedAscensionProgressEvidenceV2<T> =
        StorageMap<_, Blake2_128Concat, Hash32, Hash32, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn mythical_ascension_for_eligibility)]
    pub type MythicalAscensionByEligibilityV2<T> =
        StorageMap<_, Blake2_128Concat, (Hash32, u32), Hash32, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn mythical_ascension_receipt)]
    pub type MythicalAscensionReceiptsV2<T: Config> =
        StorageMap<_, Blake2_128Concat, Hash32, MythicalAscensionReceiptOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn v2_card_account_bound_until)]
    pub type V2CardAccountBoundUntil<T: Config> =
        StorageMap<_, Blake2_128Concat, CardIdV2, BlockNumberFor<T>, OptionQuery>;

    // V14/V15→V16 bounded custody-aware migration.
    #[pallet::storage]
    #[pallet::getter(fn tcg_migration_state_v16)]
    pub type TcgMigrationStateStorageV16<T> = StorageValue<_, TcgMigrationStateV16, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn legacy_card_classification)]
    pub type LegacyCardClassifications<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, LegacyClassificationOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn tcg_migration_anomaly_v16)]
    pub type TcgMigrationAnomaliesV16<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, MigrationAnomalyOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn repaired_legacy_cards_by_owner)]
    pub type RepairedLegacyCardsByOwnerV16<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        u32,
        bool,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn repaired_legacy_subject_count)]
    pub type RepairedLegacySubjectCountsV16<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        SubjectId,
        u32,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn legacy_writes_paused_v16)]
    pub type LegacyWritesPausedV16<T> = StorageValue<_, bool, ValueQuery>;

    /// Off-chain copied-state verifier commitment required before legacy safe
    /// exits are unpaused. The runtime never self-attests an unbounded map.
    #[pallet::storage]
    #[pallet::getter(fn v16_migration_verification_hash)]
    pub type V16MigrationVerificationHash<T> = StorageValue<_, Hash32, OptionQuery>;

    #[pallet::type_value]
    pub fn LegacyCreationSealedByDefault() -> bool {
        true
    }

    /// Legacy creation is sealed after V16. The `true` on-empty default also
    /// makes a fresh V2 genesis safe without a mutable genesis flag.
    #[pallet::storage]
    #[pallet::getter(fn legacy_creation_sealed_v16)]
    pub type LegacyCreationSealedV16<T> =
        StorageValue<_, bool, ValueQuery, LegacyCreationSealedByDefault>;

    // ------------------
    // Events
    // ------------------

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A new pack was minted for `player` with ID `pack_id`, containing multiple new cards.
        PackMinted {
            player: T::AccountId,
            pack_id: u32,
        },
        /// A single card was minted for `player` with ID `card_id`.
        CardMinted {
            player: T::AccountId,
            card_id: u32,
        },
        /// A card’s slot was generated.
        SlotGenerated {
            card_id: u32,
            values: [u8; 4],
        },
        /// A card’s slot was accepted (finalized).
        SlotAccepted {
            card_id: u32,
        },
        /// A card was finalized (forced finalize).
        SlotFinalized {
            card_id: u32,
        },
        /// A pack was completed (all cards finalized).
        PackCompleted {
            player: T::AccountId,
            pack_id: u32,
        },
        /// A card was transferred from `from` to `to`.
        CardTransferred {
            from: T::AccountId,
            to: T::AccountId,
            card_id: u32,
        },
        /// A card was listed for sale by `owner` at `price`.
        CardListed {
            owner: T::AccountId,
            card_id: u32,
            price: BalanceOf<T>,
        },
        /// A card was unlisted (by owner or due to transfer).
        CardUnlisted {
            owner: T::AccountId,
            card_id: u32,
        },
        /// A listed card was bought by `buyer` from `seller` for `price`.
        CardBought {
            buyer: T::AccountId,
            seller: T::AccountId,
            card_id: u32,
            price: BalanceOf<T>,
        },
        /// A new season art collection was created.
        SeasonCollectionCreated {
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
        },
        /// A season art collection was published and became mint-eligible.
        SeasonCollectionPublished {
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
        },
        /// A draft season art collection was removed.
        SeasonCollectionRemoved {
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
        },
        /// A collection-scoped seasonal artwork layer was added.
        SeasonCollectionAssetAdded {
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
            kind: AssetKind,
            media_id: MediaId,
        },
        /// A collection-scoped seasonal artwork layer was removed.
        SeasonCollectionAssetRemoved {
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
            kind: AssetKind,
            media_id: MediaId,
        },
        /// A collection-scoped seasonal artwork layer was moved within its list.
        SeasonCollectionAssetMoved {
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
            kind: AssetKind,
            media_id: MediaId,
            old_index: u32,
            new_index: u32,
        },
        /// Collection-scoped asset selection weights were updated.
        SeasonCollectionAssetWeightsSet {
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
            kind: AssetWeightKind,
            custom: bool,
        },
        /// The NFT collection used for converted cards was initialized.
        CardNftCollectionInitialized {
            collection_id: u32,
            admin: T::AccountId,
        },
        /// A card was converted to an NFT (card is escrowed in TCG, NFT is owned by player).
        CardConvertedToNft {
            card_id: u32,
            collection_id: u32,
            item_id: u32,
        },
        /// A card NFT was burned and the card was returned from escrow to the NFT owner.
        CardUnwrappedFromNft {
            card_id: u32,
        },
        /// An account bought additional card storage capacity.
        CardCapacityUpgraded {
            player: T::AccountId,
            added_slots: u32,
            new_capacity: u32,
            price_paid: BalanceOf<T>,
        },
        /// Nexus Starter Grant state was claimed for an account.
        StarterGrantClaimed {
            account_id: T::AccountId,
            path: StarterPath,
            grant_id: u32,
            config_version: NexusConfigVersion,
        },
        /// Internal-alpha starter team templates were configured for a path.
        StarterTeamConfigSet {
            path: StarterPath,
            card_count: u32,
            config_version: NexusConfigVersion,
        },
        NexusPrizePoolSet {
            pool_id: NexusPrizePoolId,
            subject_count: u32,
            config_version: NexusConfigVersion,
        },
        /// Nexus Starter path was swapped before grant finalization.
        StarterPathSwapped {
            account_id: T::AccountId,
            old_path: StarterPath,
            new_path: StarterPath,
        },
        /// A Nexus Collection card was claimed into runtime state.
        NexusCardClaimed {
            account_id: T::AccountId,
            card_record_id: u32,
            subject_id: SubjectId,
            source: NexusCardOrigin,
            config_version: NexusConfigVersion,
        },
        /// A Nexus card was pulled into runtime state.
        NexusCardPulled {
            account_id: T::AccountId,
            pull_id: u32,
            card_record_id: u32,
            subject_id: SubjectId,
            pack_pool_version: NexusConfigVersion,
        },
        /// A Nexus card rank slot result was resolved.
        RankSlotResolved {
            card_record_id: u32,
            base_ranks: [RankValue; 4],
            apex_side: Option<ApexSide>,
            style_label: RankStyleLabel,
            card_power: u16,
            config_version: NexusConfigVersion,
        },
        /// Nexus card Genes and Element Profile were resolved.
        GenesResolved {
            card_record_id: u32,
            genes: GeneProfile,
            element_profile: ElementProfile,
            config_version: NexusConfigVersion,
        },
        /// A 6th or later subject copy entered Overflow.
        CardEnteredOverflow {
            account_id: T::AccountId,
            card_record_id: u32,
            subject_id: SubjectId,
            reason: OverflowReason,
        },
        /// A Nexus Overflow card was resolved.
        OverflowResolved {
            account_id: T::AccountId,
            card_record_id: u32,
            action: OverflowResolutionAction,
        },
        /// A Nexus Collection card was salvaged.
        CardSalvaged {
            account_id: T::AccountId,
            card_record_id: u32,
            outputs: ResourceBundle,
            salvage_table_version: NexusConfigVersion,
        },
        /// A Nexus Collection card was sealed into a Vault Variant.
        CardSealed {
            account_id: T::AccountId,
            card_record_id: u32,
            variant_id: VaultVariantId,
            metadata_uri: BoundedNexusMetadataUri<T>,
        },
        /// A Nexus Vault expansion was applied.
        VaultExpanded {
            account_id: T::AccountId,
            old_capacity: u32,
            new_capacity: u32,
            payment_ref: Option<u32>,
        },
        /// Nexus gear was crafted.
        GearCrafted {
            account_id: T::AccountId,
            gear_id: GearId,
            recipe_id: u32,
            cost: ResourceBundle,
            config_version: NexusConfigVersion,
        },
        /// A Nexus spell was crafted.
        SpellCrafted {
            account_id: T::AccountId,
            spell_id: SpellId,
            cost: ResourceBundle,
        },
        /// Internal-alpha operator inventory seed for progression-node forge testing.
        AlphaProgressionGearSeeded {
            account_id: T::AccountId,
            gear_id: GearId,
            item_template_id: ItemTemplateId,
            slot_type: GearSlotType,
            tier: GearTier,
            power: u16,
            season_id: SeasonId,
            config_version: NexusConfigVersion,
        },
        /// Internal-alpha operator spell seed for removable magic testing.
        AlphaSpellSeeded {
            account_id: T::AccountId,
            spell_id: SpellId,
            element: Element,
            power: u16,
            config_version: NexusConfigVersion,
        },
        /// A card progression tree was configured.
        ProgressionTreeSet {
            tree_id: ProgressionTreeId,
            subject_id: SubjectId,
            rarity: Option<u8>,
            node_count: u32,
            nodes: BoundedProgressionNodes<T>,
            config_version: NexusConfigVersion,
        },
        /// A card received its progression record.
        CardProgressionInitialized {
            card_id: u32,
            tree_id: ProgressionTreeId,
            config_version: NexusConfigVersion,
        },
        /// Authorized game/reward logic granted card XP.
        CardExperienceGranted {
            issuer: T::AccountId,
            authority_id: ProgressionAuthorityId,
            game_id: ProgressionGameId,
            version_id: ProgressionVersionId,
            event_type_id: ProgressionEventTypeId,
            card_id: u32,
            amount: u32,
            experience: u32,
            level: u16,
            config_version: NexusConfigVersion,
        },
        /// A required inventory item was forged into a card progression node.
        ProgressionNodeForged {
            account_id: T::AccountId,
            card_id: u32,
            node_id: ProgressionNodeId,
            gear_id: GearId,
            item_template_id: ItemTemplateId,
            power_delta: u16,
            config_version: NexusConfigVersion,
        },
        /// A card's removable magic loadout was replaced.
        CardMagicLoadoutUpdated {
            account_id: T::AccountId,
            card_id: u32,
            spells: BoundedMagicSpells<T>,
            config_version: NexusConfigVersion,
        },
        /// A card's removable magic loadout was cleared because ownership changed.
        CardMagicLoadoutCleared {
            card_id: u32,
            old_owner: T::AccountId,
            new_owner: T::AccountId,
            config_version: NexusConfigVersion,
        },
        /// A Nexus team was saved.
        TeamSaved {
            account_id: T::AccountId,
            team_id: TeamId,
            card_ids: BoundedNexusTeamCardIds<T>,
            team_power: u16,
            config_version: NexusConfigVersion,
        },
        /// A Nexus team was validated for a mode.
        TeamValidated {
            account_id: T::AccountId,
            team_id: TeamId,
            mode: MatchMode,
            team_power: u16,
            valid: bool,
        },
        /// A Nexus match was started.
        MatchStarted {
            match_id: MatchId,
            mode: MatchMode,
            board_id: BoardId,
            players: BoundedNexusMatchPlayers<T>,
            first_player: T::AccountId,
            config_version: NexusConfigVersion,
        },
        /// A card was placed in a Nexus match.
        CardPlaced {
            match_id: MatchId,
            turn_index: u8,
            account_id: T::AccountId,
            card_id: u32,
            cell: u8,
        },
        /// A Rune Cell was created from a Mana Well.
        RuneCreated {
            match_id: MatchId,
            turn_index: u8,
            caster_card_id: u32,
            well_cell: u8,
            element: Element,
        },
        /// A Rune Cell triggered when a card was placed on it.
        RuneTriggered {
            match_id: MatchId,
            turn_index: u8,
            card_id: u32,
            well_cell: u8,
            element: Element,
            effect: i8,
        },
        /// A Nexus card captured another card.
        CardCaptured {
            match_id: MatchId,
            turn_index: u8,
            attacker_card_id: u32,
            captured_card_id: u32,
            side: ApexSide,
        },
        /// A Nexus match ended.
        MatchEnded {
            match_id: MatchId,
            winner: Option<T::AccountId>,
            score: [u8; 2],
            duration: u32,
            reward_status: bool,
        },
        /// A Nexus reward was granted.
        RewardGranted {
            account_id: T::AccountId,
            reward_id: u32,
            reason: BoundedNexusReason<T>,
            amounts: ResourceBundle,
            config_version: NexusConfigVersion,
        },
        /// A Nexus Trial was started.
        TrialStarted {
            account_id: T::AccountId,
            trial_id: TrialId,
            board_id: BoardId,
        },
        /// A Nexus Trial was completed.
        TrialCompleted {
            account_id: T::AccountId,
            trial_id: TrialId,
            result: TrialStatus,
            rewards: ResourceBundle,
        },
        /// A Nexus system was paused.
        SystemPaused {
            system_key: SystemKey,
            reason: BoundedNexusReason<T>,
            actor: T::AccountId,
            timestamp: BlockNumberFor<T>,
        },
        /// A Nexus system was unpaused.
        SystemUnpaused {
            system_key: SystemKey,
            actor: T::AccountId,
            timestamp: BlockNumberFor<T>,
        },
        /// A Nexus asset was locked.
        AssetLocked {
            asset_type: SystemKey,
            asset_id: u32,
            reason: BoundedNexusReason<T>,
            actor: T::AccountId,
            timestamp: BlockNumberFor<T>,
        },
        /// A Nexus asset was unlocked.
        AssetUnlocked {
            asset_type: SystemKey,
            asset_id: u32,
            actor: T::AccountId,
            timestamp: BlockNumberFor<T>,
        },
        /// A Nexus config version changed.
        ConfigUpdated {
            config_key: BoundedNexusReason<T>,
            old_version: NexusConfigVersion,
            new_version: NexusConfigVersion,
            actor: T::AccountId,
            timestamp: BlockNumberFor<T>,
        },

        /// A new "pro" card was started for `player` with global `card_id`.
        ProMintStarted {
            player: T::AccountId,
            card_id: u32,
        },
        /// A "pro" card spin generated new directional ranks.
        ProSpin {
            card_id: u32,
            values: [u8; 4],
            spin: u8,
        },
        /// A "pro" card was accepted (finalized) with its current ranks.
        ProMintAccepted {
            player: T::AccountId,
            card_id: u32,
            values: [u8; 4],
        },
        /// A "pro" card hit the max spins and was finalized automatically.
        ProMintForcedFinalized {
            player: T::AccountId,
            card_id: u32,
            values: [u8; 4],
        },
        SubjectDefinitionV2Published {
            definition: SubjectDefinitionV2,
        },
        SubjectActivationV2Changed {
            state: SubjectActivationState,
        },
        SubjectRarityProfilesV2Published {
            subject_id: SubjectId,
            subject_version: u32,
            profile_ids: [u32; 5],
            catalog_version: u32,
        },
        PoseDefinitionV2Published {
            definition: MediaDefinitionV2,
        },
        BackgroundDefinitionV2Published {
            definition: MediaDefinitionV2,
        },
        AcquisitionPoolVersionV2Published {
            pool_id: u32,
            version: u32,
            immutable_config_hash: Hash32,
        },
        PackSkuVersionV2Published {
            pack_sku: u32,
            version: u32,
            immutable_config_hash: Hash32,
            odds_metadata_hash: Hash32,
        },
        PackCreditIssuedV2 {
            owner: T::AccountId,
            credit_id: u64,
            pack_sku: u32,
            sku_version: u32,
            economic_realm: EconomicRealm,
            source: PackCreditSource,
        },
        PackOpenRequestedV2 {
            owner: T::AccountId,
            opening_id: Hash32,
            credit_id: u64,
            randomness_request_id: Hash32,
            immutable_config_hash: Hash32,
        },
        CardAcquiredV2 {
            owner: T::AccountId,
            card_id: CardIdV2,
            subject_id: SubjectId,
            subject_version: u32,
            rarity: CardRarity,
            profile_id: u32,
            pose_definition_id: u32,
            background_definition_id: u32,
            resolved_ranks: [u8; 4],
            economic_realm: EconomicRealm,
        },
        V2OwnerCardOperationalWarning {
            owner: T::AccountId,
            lifetime_card_count: u64,
            unopened_limit: u64,
        },
        PackOpenedV2 {
            owner: T::AccountId,
            opening_id: Hash32,
            card_ids: BoundedVec<CardIdV2, ConstU32<6>>,
        },
        PackOpenTimedOutV2 {
            owner: T::AccountId,
            opening_id: Hash32,
            restored_credit_id: u64,
        },
        CompetitiveFormatV2Published {
            format: CompetitiveFormatV2,
        },
        CompetitiveTeamV2Saved {
            owner: T::AccountId,
            team_id: u32,
            format_id: u32,
            format_version: u32,
            card_ids: V2TeamCardsOf<T>,
            rarity_load: u8,
        },
        V2FeatureStatusChanged {
            feature: V2Feature,
            enabled: bool,
        },
        CardConversionCommitted {
            owner: T::AccountId,
            card_id: CardIdV2,
            request_id: Hash32,
            reserved_entity_id: u64,
            randomness_request_id: Hash32,
            source_card_snapshot_hash: Hash32,
        },
        CardConvertedToEntity {
            owner: T::AccountId,
            card_id: CardIdV2,
            request_id: Hash32,
            entity_id: u64,
            stasis_genome: bool,
        },
        LegacyMigrationStarted {
            from_storage_version: u16,
            upper_bound: u32,
        },
        LegacyMigrationProgress {
            from_storage_version: u16,
            cursor: u32,
            cards_seen: u32,
            anomalies: u32,
        },
        LegacyMigrationAwaitingVerification {
            from_storage_version: u16,
            cards_seen: u32,
            anomalies: u32,
        },
        LegacyMigrationCompleted {
            from_storage_version: u16,
            cards_seen: u32,
            ordinary: u32,
            nft_wrapped: u32,
            known_escrow: u32,
            anomalies: u32,
            next_card_id: u32,
            max_card_id_seen: Option<u32>,
        },
        LegacyMigrationAnomalyRecorded {
            card_id: u32,
            reason_hash: Hash32,
        },
        MythicalAscensionSeasonConfiguredV2 {
            config: MythicalAscensionSeasonConfig<BlockNumberFor<T>>,
        },
        MythicalAscensionSubjectConfiguredV2 {
            config: MythicalAscensionSubjectConfig,
        },
        SeasonEligibilityLinkedV2 {
            account: T::AccountId,
            season_id: u32,
            season_eligibility_id: Hash32,
        },
        MythicalAscensionProgressRecordedV2 {
            season_eligibility_id: Hash32,
            season_id: u32,
            subject_id: SubjectId,
            economic_realm: EconomicRealm,
            mastery_level: u8,
            marks_earned: u8,
            catalyst_available: bool,
            evidence_id: Hash32,
        },
        MythicalAscendedV2 {
            owner: T::AccountId,
            season_eligibility_id: Hash32,
            season_id: u32,
            subject_id: SubjectId,
            input: MythicalAscensionInput,
            output_card_id: CardIdV2,
            ascension_id: Hash32,
            account_bound_until: BlockNumberFor<T>,
        },
        LegacyMigrationSourceRejectedV16 {
            from_storage_version: u16,
        },
        /// A wrapped legacy card NFT moved through the custody-aware V16 wrapper.
        CardNftTransferredV16 {
            card_id: u32,
            from: T::AccountId,
            to: T::AccountId,
        },
    }

    // ------------------
    // Errors
    // ------------------

    #[pallet::error]
    pub enum Error<T> {
        /// Card attempts exceeded `MaxAttempts`.
        MaxAttemptsExceeded,
        /// No active card found for the user in the current pack context.
        NoActiveCard,
        /// Pack is already completed, no further changes allowed.
        PackAlreadyCompleted,
        /// The user has no pack to operate on.
        NoPackFound,
        /// Card does not exist in storage.
        NoSuchCard,
        /// You do not own the card you’re trying to act upon.
        NotCardOwner,
        /// Card must be finalized before it can be transferred or listed.
        CardNotFinalized,
        /// Card is currently part of the owner's configured "current hand" and cannot be listed,
        /// sold, or transferred until removed from that hand.
        CardInCurrentHand,
        /// The card was already finalized and cannot be mutated.
        CardAlreadyFinalized,
        /// No more card IDs are available.
        CardIdExhausted,
        ArithmeticOverflow,
        /// This action would exceed the account's configured card capacity.
        CardCapacityExceeded,
        /// Starter Grant state already exists for this account.
        NexusStarterGrantAlreadyClaimed,
        /// Starter team config is missing for the requested path.
        StarterTeamConfigMissing,
        /// Starter team config must contain exactly one valid fixed-rank template per team slot.
        InvalidStarterTeamConfig,
        NexusPrizePoolMissing,
        NexusPrizePoolAlreadyExists,
        InvalidNexusPrizePool,
        NexusPrizeSubjectUnavailable,
        /// Account-bound starter cards cannot be listed, transferred, converted, or escrowed.
        AccountBoundCardLocked,
        /// Nexus team must contain exactly the configured Season 1 team size.
        NexusTeamSizeInvalid,
        /// Nexus subject copy cap has been reached for Collection + Vault.
        NexusSubjectCopyCapReached,
        /// Nexus Overflow has reached its total capacity.
        NexusOverflowCapacityExceeded,
        /// Nexus Overflow has reached its per-subject capacity.
        NexusOverflowSubjectCapacityExceeded,
        /// No more card capacity can be purchased because the hard storage ceiling was reached.
        CardCapacityMaxReached,
        /// The caller's owned-card limit is reached.
        MaxOwnedCardsReached,
        /// The caller's listed-card limit is reached.
        MaxListedCardsReached,
        /// Card is not listed for sale.
        NotForSale,
        /// Buyer cannot buy their own card.
        CannotBuyOwnCard,

        /// Caller is not an allowlisted season admin.
        NotSeasonAdmin,
        /// Season does not exist in the seasons pallet.
        UnknownSeason,
        /// Season is closed and can no longer accept new collections or assets.
        SeasonClosed,
        /// Season art collection does not exist.
        UnknownSeasonCollection,
        /// Season art collection is not in Draft status.
        SeasonCollectionNotDraft,
        /// Season art collection is already published.
        SeasonCollectionAlreadyPublished,
        /// Season art collection does not satisfy the current publish requirements.
        SeasonCollectionIncomplete,
        /// MediaId not found in the media registry.
        UnknownMedia,
        /// MediaId is deprecated and cannot be used.
        MediaDeprecated,
        /// The seasonal asset list is full for this kind.
        AssetListFull,
        /// The specified MediaId is not present in the seasonal asset list.
        AssetNotFound,
        /// The specified seasonal asset index is outside the current list bounds.
        AssetIndexOutOfBounds,
        /// The provided asset weight vector does not match the current asset count for that kind.
        AssetWeightCountMismatch,
        /// The provided asset weights must sum to exactly 100%.
        AssetWeightTotalInvalid,
        /// The provided asset weights result in an effective total weight of zero.
        AssetWeightMultiplierInvalid,
        /// No active season is currently set.
        NoActiveSeason,
        /// The active season has no published asset pool with at least one border, background,
        /// subject, back, and card packaging pair.
        NoPublishedSeasonCollection,
        /// Card artwork has not been assigned for this card.
        CardArtworkMissing,
        /// The card NFT collection has already been initialized.
        NftCollectionAlreadyInitialized,
        /// The card NFT collection is not initialized.
        NftCollectionNotInitialized,
        /// Card is already converted to an NFT.
        CardAlreadyConverted,
        /// Card is not converted to an NFT.
        CardNotConverted,
        /// Card is not held by escrow (unexpected state).
        CardNotEscrowed,
        /// Caller does not own the NFT item.
        NotNftOwner,

        /// Progression tree does not exist.
        ProgressionTreeMissing,
        /// Progression node does not exist in the card's tree.
        ProgressionNodeMissing,
        /// Card level does not satisfy the progression node gate.
        ProgressionNodeLocked,
        /// Progression node is already completed for this card.
        ProgressionNodeAlreadyCompleted,
        /// Card has no progression record.
        CardProgressionMissing,
        /// Required inventory item is missing or not owned by the caller.
        RequiredItemMissing,
        /// Required inventory item does not match the progression node.
        RequiredItemMismatch,
        /// Magic loadout exceeds the configured slot limit.
        MagicSlotLimitExceeded,
        /// The same spell appears more than once in a removable magic loadout.
        DuplicateSpellInLoadout,
        /// Spell is missing or not owned by the caller.
        SpellNotOwned,
        /// Gear item is already attached to a card.
        GearAlreadyAttached,
        /// Card build cannot be changed while listed, locked, or otherwise not safely mutable.
        CardBuildLocked,
        /// Converted NFT cards cannot be modified until unwrapped.
        CardConvertedBuildLocked,
        /// Progression tree input is invalid.
        InvalidProgressionTree,
        /// Progression tree has already been assigned to one or more cards.
        ProgressionTreeAlreadyInUse,
        /// Caller is not authorized to issue card progression XP.
        NotAuthorizedProgressionIssuer,
        /// Requested XP grant exceeds the configured per-call cap.
        CardXpGrantTooLarge,
        /// Internal-alpha gear seed would overwrite existing gear/template state.
        AlphaGearAlreadyExists,
        /// Internal-alpha spell seed would overwrite an existing spellbook entry.
        AlphaSpellAlreadyExists,

        /// A "pro" mint is already in progress for this account.
        ProMintAlreadyInProgress,
        /// No "pro" mint is currently in progress for this account.
        NoProMintInProgress,
        /// Pro spins exceeded `MaxProSpins`.
        MaxProSpinsExceeded,
        /// Pro card has no spin values to accept yet.
        ProCardNotSpun,
        LegacyWritesPaused,
        LegacyCreationSealed,
        LegacyCardFrozen,
        V2FeatureDisabled,
        V2DefinitionAlreadyPublished,
        V2DefinitionMissing,
        V2DefinitionMismatch,
        V2InvalidProfiles,
        V2PoolAlreadyPublished,
        V2PoolMissing,
        V2InvalidPool,
        V2PackSkuAlreadyPublished,
        V2PackSkuMissing,
        V2InvalidPackSku,
        V2PackSkuInactive,
        V2CreditIdExhausted,
        V2CreditQueueFull,
        V2PackCreditMissing,
        V2PackCreditOwnerMismatch,
        V2PackCreditRealmMismatch,
        V2PackOpeningAlreadyExists,
        V2PackOpeningMissing,
        V2PackOpeningNotReady,
        V2PackOpeningNotTimedOut,
        V2CardIdExhausted,
        V2CardMissing,
        V2NotCardOwner,
        V2CardNotActive,
        V2NoEligibleProfile,
        V2NoEligiblePose,
        V2NoEligibleBackground,
        V2ProtectionBitmapTooSmall,
        V2FormatAlreadyPublished,
        V2FormatMissing,
        V2InvalidFormat,
        V2TeamAlreadyExists,
        V2TeamSizeInvalid,
        V2DuplicateSubject,
        V2TeamRarityLoadExceeded,
        V2TooManyMythicals,
        V2TooManyTopRarities,
        V2ConversionNotAllowed,
        V2ConversionAlreadyRequested,
        V2ConversionMissing,
        V2ConversionNotReady,
        V2ConversionNotTimedOut,
        V2PlayableRosterTooSmall,
        V2CatalogVersionMismatch,
        V2ArithmeticOverflow,
        V2ProductionAlphaIssuanceDisabled,
        V16MigrationNotRunning,
        V16MigrationInvariantFailed,
        V16MigrationIncomplete,
        V2AscensionSeasonAlreadyConfigured,
        V2AscensionSeasonMissing,
        V2AscensionSubjectAlreadyConfigured,
        V2AscensionSubjectMissing,
        V2AscensionConfigInvalid,
        V2SeasonEligibilityAlreadyLinked,
        V2SeasonEligibilityMissing,
        V2AscensionProgressEvidenceConflict,
        V2AscensionMasteryInvalid,
        V2AscensionWeekInvalid,
        V2AscensionWeekAlreadyCredited,
        V2AscensionAlreadyCompleted,
        V2AscensionNotActive,
        V2AscensionRequirementsMissing,
        V2AscensionLegendaryInvalid,
        V2AscensionFoundationMissing,
        V2PendingConversionLimitReached,
        V2RandomSamplingExhausted,
        V2PackOpeningRequestConflict,
        V2ConversionRequestConflict,
        V2TutorialConversionCardUnavailable,
        V2TutorialPackCreditGrantConflict,
        V2EntropyCommitmentRequired,
        V2OperationalCardLimitReached,
        V2OperationalCardLimitInvalid,
        V2PackOpeningTerminalConflict,
        V2ConversionTerminalConflict,
        V2TutorialIdRequired,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let on_chain = StorageVersion::get::<Pallet<T>>();
            if on_chain >= STORAGE_VERSION {
                return T::DbWeight::get().reads(1);
            }
            let observed_version = Self::observed_storage_version();
            let from_storage_version = if matches!(observed_version, 14 | 15) {
                observed_version
            } else {
                Self::disable_all_v2_features();
                LegacyWritesPausedV16::<T>::put(true);
                LegacyCreationSealedV16::<T>::put(true);
                TcgMigrationStateStorageV16::<T>::put(TcgMigrationStateV16 {
                    phase: MigrationPhaseV16::UnsupportedSource,
                    from_storage_version: observed_version,
                    cursor: 0,
                    upper_bound: 0,
                    cards_seen: 0,
                    ordinary: 0,
                    nft_wrapped: 0,
                    known_escrow: 0,
                    anomalies: 0,
                    max_card_id_seen: None,
                });
                Self::deposit_event(Event::LegacyMigrationSourceRejectedV16 {
                    from_storage_version: observed_version,
                });
                return T::DbWeight::get().reads_writes(1, 7);
            };
            let upper_bound = NextCardId::<T>::get();
            Self::disable_all_v2_features();
            TcgMigrationStateStorageV16::<T>::put(TcgMigrationStateV16 {
                phase: MigrationPhaseV16::Running,
                from_storage_version,
                cursor: 0,
                upper_bound,
                cards_seen: 0,
                ordinary: 0,
                nft_wrapped: 0,
                known_escrow: 0,
                anomalies: 0,
                max_card_id_seen: None,
            });
            LegacyWritesPausedV16::<T>::put(true);
            LegacyCreationSealedV16::<T>::put(true);
            STORAGE_VERSION.put::<Pallet<T>>();
            Self::deposit_event(Event::LegacyMigrationStarted {
                from_storage_version,
                upper_bound,
            });
            T::DbWeight::get().reads_writes(2, 8)
        }

        fn on_idle(_now: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
            let state_read = T::DbWeight::get().reads(1);
            if state_read.any_gt(remaining_weight) {
                return Weight::zero();
            }
            let Some(state) = TcgMigrationStateStorageV16::<T>::get() else {
                return state_read;
            };
            if state.phase != MigrationPhaseV16::Running {
                return state_read;
            }
            // The classifier is sparse-ID-safe and charges every visited
            // cursor as the worst custody path. The fixed allowance includes
            // both state reads, terminal-state work, events, and the final
            // state write. Per-cursor proof size is explicitly non-zero so a
            // proof-size-constrained block can never be overrun even when NFT
            // and escrow ownership are inspected.
            let fixed = Weight::from_parts(75_000_000, 64 * 1024)
                .saturating_add(T::DbWeight::get().reads_writes(4, 4));
            if fixed.any_gt(remaining_weight) {
                return state_read;
            }
            let per_cursor = Weight::from_parts(175_000_000, 128 * 1024)
                .saturating_add(T::DbWeight::get().reads_writes(15, 8));
            let available = remaining_weight.saturating_sub(fixed);
            let by_weight = available
                .checked_div_per_component(&per_cursor)
                .unwrap_or(0)
                .min(u64::from(T::V16MigrationBatchSize::get())) as u32;
            if by_weight == 0 {
                return state_read;
            }
            let processed = Self::migrate_v16_batch(by_weight);
            fixed.saturating_add(per_cursor.saturating_mul(u64::from(processed)))
        }

        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
            let evidence = Self::legacy_migration_evidence()?;
            match evidence.from_storage_version {
                14 | 15 => {}
                16 => Self::validate_v16_migration_state()?,
                _ => {
                    return Err(
                        "TCG V16 migration only supports storage version 14, 15, or a validated V16 no-op"
                            .into(),
                    )
                }
            }
            Ok(evidence.encode())
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
            let before = TcgMigrationPreUpgradeEvidenceV16::decode(&mut &state[..])
                .map_err(|_| "TCG V16 pre-upgrade evidence did not decode")?;
            if Self::observed_storage_version() != 16 {
                return Err("TCG V16 migration did not install storage version 16".into());
            }
            if before.from_storage_version == 16 {
                let after = Self::legacy_migration_evidence()?;
                if after != before {
                    return Err("TCG V16 no-op changed pallet storage".into());
                }
                return Self::validate_v16_migration_state();
            }
            let migration = TcgMigrationStateStorageV16::<T>::get()
                .ok_or("TCG V16 migration state was not created")?;
            if migration.phase != MigrationPhaseV16::Running
                || migration.from_storage_version != before.from_storage_version
                || migration.cursor != 0
                || migration.upper_bound != before.next_card_id
                || migration.cards_seen != 0
                || migration.ordinary != 0
                || migration.nft_wrapped != 0
                || migration.known_escrow != 0
                || migration.anomalies != 0
                || migration.max_card_id_seen.is_some()
                || !LegacyWritesPausedV16::<T>::get()
                || !LegacyCreationSealedV16::<T>::get()
            {
                return Err("TCG V16 migration did not start in a fail-closed state".into());
            }
            Self::ensure_all_v2_features_disabled()?;

            let after = Self::legacy_migration_evidence()?;
            if after.card_count != before.card_count
                || after.cards_hash != before.cards_hash
                || after.nexus_card_count != before.nexus_card_count
                || after.nexus_cards_hash != before.nexus_cards_hash
                || after.converted_count != before.converted_count
                || after.converted_hash != before.converted_hash
                || after.owner_index_count != before.owner_index_count
                || after.owner_index_hash != before.owner_index_hash
                || after.vault_variant_count != before.vault_variant_count
                || after.vault_variants_hash != before.vault_variants_hash
                || after.nexus_subject_index_count != before.nexus_subject_index_count
                || after.nexus_subject_indexes_hash != before.nexus_subject_indexes_hash
                || after.overflow_owner_index_count != before.overflow_owner_index_count
                || after.overflow_owner_indexes_hash != before.overflow_owner_indexes_hash
                || after.overflow_subject_index_count != before.overflow_subject_index_count
                || after.overflow_subject_indexes_hash != before.overflow_subject_indexes_hash
                || after.next_card_id != before.next_card_id
            {
                return Err("TCG V16 migration rewrote critical legacy storage on start".into());
            }
            Self::validate_v16_migration_state()
        }

        #[cfg(feature = "try-runtime")]
        fn try_state(_now: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
            Self::validate_v16_migration_state()?;
            for (owner, recorded) in PendingConversionCountByAccountV2::<T>::iter() {
                if recorded > T::MaxPendingConversionsPerAccount::get() {
                    return Err("TCG V2 pending conversion count exceeds its bound".into());
                }
                let actual: u32 = CardConversionTombstones::<T>::iter_values()
                    .filter(|tombstone| {
                        tombstone.owner == owner
                            && tombstone.resolution == ConversionResolution::Pending
                    })
                    .count()
                    .try_into()
                    .map_err(|_| "TCG V2 pending conversion count exceeds u32")?;
                if actual != recorded {
                    return Err("TCG V2 pending conversion count does not reconcile".into());
                }
            }
            for tombstone in CardConversionTombstones::<T>::iter_values()
                .filter(|tombstone| tombstone.resolution == ConversionResolution::Pending)
            {
                if PendingConversionCountByAccountV2::<T>::get(&tombstone.owner) == 0 {
                    return Err("TCG V2 pending tombstone has no account counter".into());
                }
            }
            let (_, operational_limit) = Self::operational_card_thresholds()
                .map_err(|_| "TCG V2 operational card thresholds are invalid")?;
            for (owner, reserved) in ReservedV2PackCardCount::<T>::iter() {
                let actual = PendingPackOpeningsV2::<T>::iter_values()
                    .filter(|opening| opening.owner == owner)
                    .try_fold(0u64, |sum, opening| {
                        let sku =
                            PackSkuVersionsV2::<T>::get((opening.pack_sku, opening.sku_version))
                                .ok_or("TCG V2 pending opening references a missing SKU")?;
                        sum.checked_add(u64::from(sku.card_count))
                            .ok_or("TCG V2 pending card reservation overflow")
                    })?;
                if actual != reserved {
                    return Err("TCG V2 pending pack capacity does not reconcile".into());
                }
                let committed = V2OwnerCardCount::<T>::get(&owner)
                    .checked_add(reserved)
                    .ok_or("TCG V2 committed card count overflow")?;
                if committed > operational_limit {
                    return Err("TCG V2 committed card count exceeds operational limit".into());
                }
            }
            for (opening_id, opening) in PendingPackOpeningsV2::<T>::iter() {
                let locked = LockedPackCreditsV2::<T>::get(opening_id)
                    .ok_or("TCG V2 pending opening has no locked credit")?;
                if locked.owner != opening.owner
                    || locked.credit_id != opening.credit_id
                    || locked.pack_sku != opening.pack_sku
                    || locked.sku_version != opening.sku_version
                    || locked.economic_realm != opening.economic_realm
                {
                    return Err("TCG V2 pending opening locked credit does not match".into());
                }
                let receipt =
                    PackOpeningRequestReceiptsV2::<T>::get(&opening.owner, opening.commitment)
                        .ok_or("TCG V2 pending opening has no replay receipt")?;
                if receipt.opening_id != opening_id
                    || receipt.pack_sku != opening.pack_sku
                    || receipt.sku_version != opening.sku_version
                    || receipt.economic_realm != opening.economic_realm
                {
                    return Err("TCG V2 pending opening replay receipt does not match".into());
                }
                if ReservedV2PackCardCount::<T>::get(&opening.owner) == 0 {
                    return Err("TCG V2 pending opening has no capacity reservation".into());
                }
            }
            Ok(())
        }
    }

    // ------------------
    // Calls (Extrinsics)
    // ------------------

    #[pallet::call]
    #[allow(clippy::too_many_arguments)]
    impl<T: Config> Pallet<T> {
        /// Mint a new pack of cards for the caller.
        ///
        /// Charges `PackPrice` (in native `COIN`) and mints `CardsPerPack` unique card IDs.
        /// Each card is stored globally in `Cards<T>`.
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::mint_pack())]
        #[transactional]
        pub fn mint_pack(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_legacy_creation_allowed()?;
            Self::note_minter(&player);

            let mut packs = PlayerPacks::<T>::get(&player);
            Self::prune_completed_packs(&mut packs);
            Self::ensure_can_receive_cards(&player, u32::from(T::CardsPerPack::get()))?;

            // Charge the pack price up-front.
            let price = T::PackPrice::get();
            let receiver = T::PackPriceReceiver::get();
            T::PaymentCurrency::transfer(
                &player,
                &receiver,
                price,
                ExistenceRequirement::KeepAlive,
            )?;

            let pack_id = <frame_system::Pallet<T>>::block_number().saturated_into::<u32>();

            // Build a new pack with references to newly minted card IDs
            let mut card_ids: BoundedVec<u32, ConstU32<16>> = BoundedVec::default();

            for _ in 0..T::CardsPerPack::get() {
                let new_card_id = Self::create_new_card(&player)?;
                // Attach this card to the pack
                card_ids
                    .try_push(new_card_id)
                    .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;
            }

            let first_card_id = card_ids.first().copied();

            let new_pack = Pack {
                id: pack_id,
                card_ids,
                active_card_index: 0,
                completed: false,
            };

            packs
                .try_push(new_pack)
                .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;

            PlayerPacks::<T>::insert(&player, packs);
            ActiveCard::<T>::insert(&player, Some(0));
            PackInProgress::<T>::insert(&player, pack_id);
            // We just minted the pack, so index 0 must exist if `CardsPerPack > 0`.
            if let Some(first) = first_card_id {
                PackCardInProgress::<T>::insert(&player, first);
            }

            Self::deposit_event(Event::PackMinted { player, pack_id });
            Ok(())
        }

        /// Mint a single, immediately-finalized card for the caller.
        ///
        /// Charges `MintCardPrice` (in native `COIN`) and mints exactly one card ID with
        /// deterministic ranks based on on-chain entropy (consensus-safe, not cryptographic RNG).
        #[pallet::call_index(7)]
        #[pallet::weight(<T as Config>::WeightInfo::mint_card())]
        #[transactional]
        pub fn mint_card(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_legacy_creation_allowed()?;
            Self::note_minter(&player);
            Self::ensure_can_receive_cards(&player, 1)?;

            // Charge the mint price up-front.
            let price = T::MintCardPrice::get();
            let receiver = T::MintCardPriceReceiver::get();
            T::PaymentCurrency::transfer(
                &player,
                &receiver,
                price,
                ExistenceRequirement::KeepAlive,
            )?;

            let card_id = Self::create_new_finalized_card(&player)?;
            Self::deposit_event(Event::CardMinted { player, card_id });
            Ok(())
        }

        /// Claim the configured internal-alpha starter team for the caller.
        ///
        /// This cards-first alpha grant mints five account-bound starter cards. Starter
        /// weapon, spell, and badge issuance remain follow-up item/profile PIs.
        #[pallet::call_index(26)]
        #[pallet::weight(<T as Config>::WeightInfo::claim_starter_grant())]
        #[transactional]
        pub fn claim_starter_grant(origin: OriginFor<T>, path: StarterPath) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_legacy_creation_allowed()?;
            Self::note_minter(&player);
            ensure!(
                !StarterGrants::<T>::contains_key(&player),
                Error::<T>::NexusStarterGrantAlreadyClaimed
            );
            let starter_cards =
                StarterTeamConfigs::<T>::get(path).ok_or(Error::<T>::StarterTeamConfigMissing)?;
            Self::ensure_can_receive_cards(&player, starter_cards.len().saturated_into::<u32>())?;

            let grant_id = NextStarterGrantId::<T>::get();
            let next_grant_id = grant_id.checked_add(1).ok_or(Error::<T>::CardIdExhausted)?;
            let now = <frame_system::Pallet<T>>::block_number();
            let config = Self::current_nexus_config();

            NexusAccountStates::<T>::insert(
                &player,
                NexusAccountState {
                    starter_claimed: true,
                    starter_path: Some(path),
                    vault_capacity: config.base_vault_capacity,
                    created_at: now,
                    config_version: config.config_version,
                },
            );
            StarterGrants::<T>::insert(
                &player,
                StarterGrantState {
                    path,
                    grant_id,
                    claimed_at: now,
                    config_version: config.config_version,
                },
            );
            NextStarterGrantId::<T>::put(next_grant_id);

            for template in starter_cards.iter() {
                let card_id = Self::create_starter_card_from_template(&player, template)?;
                Self::deposit_event(Event::CardMinted {
                    player: player.clone(),
                    card_id,
                });
            }

            Self::deposit_event(Event::StarterGrantClaimed {
                account_id: player,
                path,
                grant_id,
                config_version: config.config_version,
            });
            Ok(())
        }

        /// Configure one internal-alpha starter team path.
        #[pallet::call_index(34)]
        #[pallet::weight(<T as Config>::WeightInfo::set_starter_team_config())]
        #[transactional]
        pub fn set_starter_team_config(
            origin: OriginFor<T>,
            path: StarterPath,
            cards: Vec<StarterCardTemplate>,
            config_version: NexusConfigVersion,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_legacy_writes_allowed()?;
            let starter_cards = Self::validated_starter_team_cards(cards, config_version)?;
            let card_count = starter_cards.len().saturated_into::<u32>();
            StarterTeamConfigs::<T>::insert(path, starter_cards);
            Self::deposit_event(Event::StarterTeamConfigSet {
                path,
                card_count,
                config_version,
            });
            Ok(())
        }

        /// Configure a versioned Prize Counter/Vending Machine subject pool.
        /// Templates define subject identity and controlled trait baselines;
        /// acquisitions resolve one final variation and never expose rerolls.
        #[pallet::call_index(35)]
        #[pallet::weight(<T as Config>::WeightInfo::set_starter_team_config())]
        #[transactional]
        pub fn set_nexus_prize_pool(
            origin: OriginFor<T>,
            pool_id: NexusPrizePoolId,
            templates: Vec<NexusPrizeTemplate>,
            config_version: NexusConfigVersion,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_legacy_writes_allowed()?;
            ensure!(
                !NexusPrizePools::<T>::contains_key(pool_id),
                Error::<T>::NexusPrizePoolAlreadyExists
            );
            ensure!(!templates.is_empty(), Error::<T>::InvalidNexusPrizePool);
            // Reject oversized legacy Vec payloads before the duplicate scan.
            // This preserves the call's frozen SCALE shape while bounding the
            // O(n²) `subjects.contains` validation below.
            ensure!(
                templates.len().saturated_into::<u32>() <= T::MaxSubjects::get(),
                Error::<T>::InvalidNexusPrizePool
            );
            let mut subjects = sp_std::vec::Vec::<SubjectId>::new();
            for template in templates.iter() {
                ensure!(
                    template.card.config_version == config_version
                        && template.card.apex_side.is_none()
                        && !template
                            .card
                            .base_ranks
                            .iter()
                            .any(|rank| matches!(rank, RankValue::Apex))
                        && !subjects.contains(&template.card.subject_id),
                    Error::<T>::InvalidNexusPrizePool
                );
                subjects.push(template.card.subject_id);
            }
            let templates = BoundedNexusPrizeTemplates::<T>::try_from(templates)
                .map_err(|_| Error::<T>::InvalidNexusPrizePool)?;
            let subject_count = templates.len().saturated_into::<u32>();
            NexusPrizePools::<T>::insert(
                pool_id,
                NexusPrizePool {
                    templates,
                    config_version,
                },
            );
            Self::deposit_event(Event::NexusPrizePoolSet {
                pool_id,
                subject_count,
                config_version,
            });
            Ok(())
        }

        /// Generate new slot values for the user’s current (active) card, up to `MaxAttempts`.
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::generate_slot())]
        #[transactional]
        pub fn generate_slot(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_legacy_creation_allowed()?;

            // 1) Find the user’s last minted pack
            PlayerPacks::<T>::mutate(&player, |packs| -> DispatchResult {
                let pack = packs.last_mut().ok_or(Error::<T>::NoPackFound)?;
                ensure!(!pack.completed, Error::<T>::PackAlreadyCompleted);

                // 2) Get the active card index
                let active_card_idx =
                    ActiveCard::<T>::get(&player).ok_or(Error::<T>::NoActiveCard)?;
                let card_id = *pack
                    .card_ids
                    .get(active_card_idx as usize)
                    .ok_or(Error::<T>::NoActiveCard)?;

                // 3) Check ownership
                let mut card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == player, Error::<T>::NotCardOwner);
                ensure!(!card_info.finalized, Error::<T>::CardAlreadyFinalized);

                // 4) Check attempts
                let mut attempts = CardAttempts::<T>::get(card_id);
                ensure!(
                    attempts < T::MaxAttempts::get(),
                    Error::<T>::MaxAttemptsExceeded
                );

                // Derive deterministic ranks from on-chain entropy + (player, card_id, attempts).
                let values = Self::spin_values(&player, card_id, attempts, b"eterra-tcg/slot");

                // 6) Update card’s slot values
                card_info.slot_values = Some(values);

                // 7) Store back
                Cards::<T>::insert(card_id, card_info);

                // 8) Increment attempts
                attempts += 1;
                CardAttempts::<T>::insert(card_id, attempts);

                // 9) If attempts == max, finalize now
                let pack_completed = if attempts == T::MaxAttempts::get() {
                    Self::finalize_card_and_advance(&player, card_id, pack, active_card_idx)?;
                    pack.completed
                } else {
                    false
                };

                if pack_completed {
                    Self::prune_completed_packs(packs);
                }

                Self::deposit_event(Event::SlotGenerated { card_id, values });
                Ok(())
            })?;

            Ok(())
        }

        /// Accept (finalize) the user’s current card’s slot values immediately.
        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::accept_slot())]
        #[transactional]
        pub fn accept_slot(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_legacy_creation_allowed()?;

            PlayerPacks::<T>::mutate(&player, |packs| -> DispatchResult {
                let pack = packs.last_mut().ok_or(Error::<T>::NoPackFound)?;
                ensure!(!pack.completed, Error::<T>::PackAlreadyCompleted);
                let active_card_idx =
                    ActiveCard::<T>::get(&player).ok_or(Error::<T>::NoActiveCard)?;
                let card_id = *pack
                    .card_ids
                    .get(active_card_idx as usize)
                    .ok_or(Error::<T>::NoActiveCard)?;

                // Must have a card
                let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == player, Error::<T>::NotCardOwner);
                ensure!(!card_info.finalized, Error::<T>::CardAlreadyFinalized);

                // Must have generated at least once
                ensure!(card_info.slot_values.is_some(), Error::<T>::NoActiveCard);

                // Finalize
                Self::finalize_card_and_advance(&player, card_id, pack, active_card_idx)?;
                let pack_completed = pack.completed;

                if pack_completed {
                    Self::prune_completed_packs(packs);
                }

                Self::deposit_event(Event::SlotAccepted { card_id });
                Ok(())
            })?;

            Ok(())
        }

        /// **New**: Transfer a single card from `origin` to `to`.
        /// If that card is also part of a pack, it still references it, but ownership
        /// changes to `to`.
        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::transfer_card())]
        #[transactional]
        pub fn transfer_card(
            origin: OriginFor<T>,
            card_id: u32,
            to: T::AccountId,
        ) -> DispatchResult {
            let from = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&from)?;
            Self::ensure_legacy_writes_allowed()?;

            // Ensure card exists, is owned, and is finalized before allowing transfer.
            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == from, Error::<T>::NotCardOwner);
            ensure!(card_info.finalized, Error::<T>::CardNotFinalized);
            ensure!(
                !T::HandChecker::is_card_in_current_hand(&from, card_id),
                Error::<T>::CardInCurrentHand
            );
            Self::ensure_card_not_account_bound(card_id)?;

            // If listed, unlist first so indices remain consistent.
            if CardPrices::<T>::contains_key(card_id) {
                Self::unlist(card_id, &from);
            }

            Self::do_transfer(&from, &to, card_id)?;
            Self::transition_v16_beneficial_owner(
                card_id,
                &from,
                &to,
                LegacyCustodyKind::Ordinary,
                true,
            )?;

            Self::deposit_event(Event::CardTransferred { from, to, card_id });
            Ok(())
        }

        /// Start a new "pro" mint: pay `ProPrice`, mint a single in-progress card,
        /// then use `spin_pro` (up to `MaxProSpins`) to generate ranks and `accept_pro` to finalize.
        #[pallet::call_index(4)]
        #[pallet::weight(<T as Config>::WeightInfo::mint_pro())]
        #[transactional]
        pub fn mint_pro(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_legacy_creation_allowed()?;
            Self::note_minter(&player);
            ensure!(
                !ProInProgress::<T>::contains_key(&player),
                Error::<T>::ProMintAlreadyInProgress
            );
            Self::ensure_can_receive_cards(&player, 1)?;

            // Charge the pro price up-front.
            let price = T::ProPrice::get();
            let receiver = T::ProPriceReceiver::get();
            T::PaymentCurrency::transfer(
                &player,
                &receiver,
                price,
                ExistenceRequirement::KeepAlive,
            )?;

            // Create the in-progress card.
            let card_id = Self::create_new_card(&player)?;
            ProInProgress::<T>::insert(&player, card_id);
            Self::deposit_event(Event::ProMintStarted {
                player: player.clone(),
                card_id,
            });

            Ok(())
        }

        /// Spin the "pro" card in progress, up to `MaxProSpins`.
        /// Updates the in-progress card's directional ranks.
        #[pallet::call_index(5)]
        #[pallet::weight(<T as Config>::WeightInfo::spin_pro())]
        #[transactional]
        pub fn spin_pro(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_legacy_creation_allowed()?;
            let card_id =
                ProInProgress::<T>::get(&player).ok_or(Error::<T>::NoProMintInProgress)?;

            let (values, spins_used, forced_finalized) = Self::do_pro_spin(&player, card_id)?;

            if forced_finalized {
                Self::deposit_event(Event::ProMintForcedFinalized {
                    player,
                    card_id,
                    values,
                });
            } else {
                Self::deposit_event(Event::ProSpin {
                    card_id,
                    values,
                    spin: spins_used,
                });
            }

            Ok(())
        }

        /// Accept (finalize) the current "pro" card with whatever values are currently set.
        #[pallet::call_index(6)]
        #[pallet::weight(<T as Config>::WeightInfo::accept_pro())]
        #[transactional]
        pub fn accept_pro(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_legacy_creation_allowed()?;
            let card_id =
                ProInProgress::<T>::get(&player).ok_or(Error::<T>::NoProMintInProgress)?;

            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == player, Error::<T>::NotCardOwner);
            ensure!(!card_info.finalized, Error::<T>::CardAlreadyFinalized);
            let values = card_info.slot_values.ok_or(Error::<T>::ProCardNotSpun)?;

            // Finalize the card and clear pro state.
            Self::finalize_pro_card(&player, card_id)?;

            Self::deposit_event(Event::ProMintAccepted {
                player,
                card_id,
                values,
            });
            Ok(())
        }

        /// List a finalized card for sale at a fixed `price` (in native balance units).
        #[pallet::call_index(8)]
        #[pallet::weight(<T as Config>::WeightInfo::set_price())]
        #[transactional]
        pub fn set_price(
            origin: OriginFor<T>,
            card_id: u32,
            price: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_legacy_card_not_frozen(card_id)?;
            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == who, Error::<T>::NotCardOwner);
            ensure!(card_info.finalized, Error::<T>::CardNotFinalized);
            Self::ensure_card_not_account_bound(card_id)?;
            ensure!(
                !T::HandChecker::is_card_in_current_hand(&who, card_id),
                Error::<T>::CardInCurrentHand
            );

            CardPrices::<T>::insert(card_id, price);
            ListedByOwner::<T>::try_mutate(&who, |set| -> DispatchResult {
                set.try_insert(card_id)
                    .map_err(|_| Error::<T>::MaxListedCardsReached)?;
                Ok(())
            })?;

            Self::deposit_event(Event::CardListed {
                owner: who,
                card_id,
                price,
            });
            Ok(())
        }

        /// Remove a card from sale.
        #[pallet::call_index(9)]
        #[pallet::weight(<T as Config>::WeightInfo::remove_price())]
        #[transactional]
        pub fn remove_price(origin: OriginFor<T>, card_id: u32) -> DispatchResult {
            let who = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_legacy_card_not_frozen(card_id)?;
            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == who, Error::<T>::NotCardOwner);

            ensure!(
                CardPrices::<T>::contains_key(card_id),
                Error::<T>::NotForSale
            );
            Self::unlist(card_id, &who);
            Ok(())
        }

        /// Buy a listed card at the asking price.
        #[pallet::call_index(10)]
        #[pallet::weight(<T as Config>::WeightInfo::buy_card())]
        #[transactional]
        pub fn buy_card(origin: OriginFor<T>, card_id: u32) -> DispatchResult {
            let buyer = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&buyer)?;
            Self::ensure_legacy_writes_allowed()?;

            let price = CardPrices::<T>::get(card_id).ok_or(Error::<T>::NotForSale)?;
            let seller = Cards::<T>::get(card_id)
                .map(|c| c.owner)
                .ok_or(Error::<T>::NoSuchCard)?;

            ensure!(seller != buyer, Error::<T>::CannotBuyOwnCard);
            Self::ensure_card_not_account_bound(card_id)?;
            ensure!(
                !T::HandChecker::is_card_in_current_hand(&seller, card_id),
                Error::<T>::CardInCurrentHand
            );

            // Transfer funds buyer -> seller.
            T::PaymentCurrency::transfer(&buyer, &seller, price, ExistenceRequirement::AllowDeath)?;

            // Unlist before transfer (so indices are consistent).
            Self::unlist(card_id, &seller);

            // Transfer ownership seller -> buyer.
            Self::do_transfer(&seller, &buyer, card_id)?;
            Self::transition_v16_beneficial_owner(
                card_id,
                &seller,
                &buyer,
                LegacyCustodyKind::Ordinary,
                true,
            )?;

            Self::deposit_event(Event::CardBought {
                buyer,
                seller,
                card_id,
                price,
            });
            Ok(())
        }

        /// Create a new season-scoped art collection.
        ///
        /// Collections may be created while the season is Draft or Active. They remain in Draft
        /// until explicitly published, at which point they become eligible for minting.
        #[pallet::call_index(19)]
        #[pallet::weight(<T as Config>::WeightInfo::create_season_collection())]
        #[transactional]
        pub fn create_season_collection(
            origin: OriginFor<T>,
            season_id: SeasonId,
            name: BoundedSeasonCollectionName<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_season_manageable(season_id)?;

            let collection_id = NextSeasonCollectionId::<T>::get(season_id);
            let next_collection_id = collection_id
                .checked_add(1)
                .ok_or(Error::<T>::AssetListFull)?;

            SeasonCollectionIds::<T>::try_mutate(season_id, |ids| -> DispatchResult {
                ids.try_push(collection_id)
                    .map_err(|_| Error::<T>::AssetListFull)?;
                Ok(())
            })?;

            SeasonCollections::<T>::insert(
                season_id,
                collection_id,
                SeasonCollectionInfo {
                    name,
                    status: SeasonCollectionStatus::Draft,
                    created_at: <frame_system::Pallet<T>>::block_number(),
                    published_at: None,
                },
            );
            SeasonCollectionAssets::<T>::insert(
                season_id,
                collection_id,
                SeasonAssetsInfoOf::<T>::default(),
            );
            NextSeasonCollectionId::<T>::insert(season_id, next_collection_id);

            Self::deposit_event(Event::SeasonCollectionCreated {
                season_id,
                collection_id,
            });
            Ok(())
        }

        /// Publish a season art collection so it contributes layers into the season-wide mint pool.
        #[pallet::call_index(20)]
        #[pallet::weight(<T as Config>::WeightInfo::publish_season_collection())]
        #[transactional]
        pub fn publish_season_collection(
            origin: OriginFor<T>,
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_season_manageable(season_id)?;

            let assets = SeasonCollectionAssets::<T>::get(season_id, collection_id);
            Self::ensure_collection_has_any_assets(&assets)
                .map_err(|_| Error::<T>::SeasonCollectionIncomplete)?;
            Self::ensure_collection_can_publish_into_season(season_id, &assets)
                .map_err(|_| Error::<T>::SeasonCollectionIncomplete)?;

            SeasonCollections::<T>::try_mutate(
                season_id,
                collection_id,
                |maybe_collection| -> DispatchResult {
                    let collection = maybe_collection
                        .as_mut()
                        .ok_or(Error::<T>::UnknownSeasonCollection)?;
                    ensure!(
                        collection.status == SeasonCollectionStatus::Draft,
                        Error::<T>::SeasonCollectionAlreadyPublished
                    );
                    collection.status = SeasonCollectionStatus::Published;
                    collection.published_at = Some(<frame_system::Pallet<T>>::block_number());
                    Ok(())
                },
            )?;

            Self::deposit_event(Event::SeasonCollectionPublished {
                season_id,
                collection_id,
            });
            Ok(())
        }

        /// Remove a draft season art collection.
        #[pallet::call_index(21)]
        #[pallet::weight(<T as Config>::WeightInfo::remove_season_collection())]
        #[transactional]
        pub fn remove_season_collection(
            origin: OriginFor<T>,
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_season_collection_draft(season_id, collection_id)?;

            SeasonCollectionIds::<T>::mutate(season_id, |ids| {
                if let Some(position) = ids.iter().position(|id| *id == collection_id) {
                    ids.remove(position);
                }
            });
            SeasonCollections::<T>::remove(season_id, collection_id);
            SeasonCollectionAssets::<T>::remove(season_id, collection_id);

            Self::deposit_event(Event::SeasonCollectionRemoved {
                season_id,
                collection_id,
            });
            Ok(())
        }

        /// Add an artwork layer to a draft season art collection.
        #[pallet::call_index(22)]
        #[pallet::weight(<T as Config>::WeightInfo::add_season_collection_asset())]
        #[transactional]
        pub fn add_season_collection_asset(
            origin: OriginFor<T>,
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
            kind: AssetKind,
            media_id: MediaId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_season_collection_draft(season_id, collection_id)?;
            Self::ensure_media_valid(media_id)?;

            let inserted = SeasonCollectionAssets::<T>::try_mutate(
                season_id,
                collection_id,
                |assets| -> Result<bool, DispatchError> {
                    match kind {
                        AssetKind::Border => {
                            if assets.borders.contains(&media_id) {
                                return Ok(false);
                            }
                            assets
                                .borders
                                .try_push(media_id)
                                .map_err(|_| Error::<T>::AssetListFull)?;
                            Self::clear_asset_weight_config(&mut assets.border_weights);
                        }
                        AssetKind::Background => {
                            if assets.backgrounds.contains(&media_id) {
                                return Ok(false);
                            }
                            assets
                                .backgrounds
                                .try_push(media_id)
                                .map_err(|_| Error::<T>::AssetListFull)?;
                            Self::clear_asset_weight_config(&mut assets.background_weights);
                        }
                        AssetKind::Subject => {
                            if assets.subjects.contains(&media_id) {
                                return Ok(false);
                            }
                            assets
                                .subjects
                                .try_push(media_id)
                                .map_err(|_| Error::<T>::AssetListFull)?;
                            Self::clear_asset_weight_config(&mut assets.subject_weights);
                        }
                        AssetKind::Back => {
                            if assets.backs.contains(&media_id) {
                                return Ok(false);
                            }
                            assets
                                .backs
                                .try_push(media_id)
                                .map_err(|_| Error::<T>::AssetListFull)?;
                            Self::clear_asset_weight_config(&mut assets.back_weights);
                        }
                        AssetKind::PackagingFront => {
                            if assets.packaging_fronts.contains(&media_id) {
                                return Ok(false);
                            }
                            assets
                                .packaging_fronts
                                .try_push(media_id)
                                .map_err(|_| Error::<T>::AssetListFull)?;
                            Self::clear_asset_weight_config(&mut assets.packaging_weights);
                        }
                        AssetKind::PackagingBack => {
                            if assets.packaging_backs.contains(&media_id) {
                                return Ok(false);
                            }
                            assets
                                .packaging_backs
                                .try_push(media_id)
                                .map_err(|_| Error::<T>::AssetListFull)?;
                            Self::clear_asset_weight_config(&mut assets.packaging_weights);
                        }
                    }
                    Ok(true)
                },
            )?;

            if inserted {
                Self::deposit_event(Event::SeasonCollectionAssetAdded {
                    season_id,
                    collection_id,
                    kind,
                    media_id,
                });
            }
            Ok(())
        }

        /// Remove an artwork layer from a draft season art collection.
        #[pallet::call_index(23)]
        #[pallet::weight(<T as Config>::WeightInfo::remove_season_collection_asset())]
        #[transactional]
        pub fn remove_season_collection_asset(
            origin: OriginFor<T>,
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
            kind: AssetKind,
            media_id: MediaId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_season_collection_draft(season_id, collection_id)?;

            SeasonCollectionAssets::<T>::try_mutate(
                season_id,
                collection_id,
                |assets| -> DispatchResult {
                    let removed = match kind {
                        AssetKind::Border => {
                            let removed =
                                Self::remove_asset_from_list(&mut assets.borders, media_id);
                            if removed {
                                Self::clear_asset_weight_config(&mut assets.border_weights);
                            }
                            removed
                        }
                        AssetKind::Background => {
                            let removed =
                                Self::remove_asset_from_list(&mut assets.backgrounds, media_id);
                            if removed {
                                Self::clear_asset_weight_config(&mut assets.background_weights);
                            }
                            removed
                        }
                        AssetKind::Subject => {
                            let removed =
                                Self::remove_asset_from_list(&mut assets.subjects, media_id);
                            if removed {
                                Self::clear_asset_weight_config(&mut assets.subject_weights);
                            }
                            removed
                        }
                        AssetKind::Back => {
                            let removed = Self::remove_asset_from_list(&mut assets.backs, media_id);
                            if removed {
                                Self::clear_asset_weight_config(&mut assets.back_weights);
                            }
                            removed
                        }
                        AssetKind::PackagingFront => {
                            let removed = Self::remove_asset_from_list(
                                &mut assets.packaging_fronts,
                                media_id,
                            );
                            if removed {
                                Self::clear_asset_weight_config(&mut assets.packaging_weights);
                            }
                            removed
                        }
                        AssetKind::PackagingBack => {
                            let removed =
                                Self::remove_asset_from_list(&mut assets.packaging_backs, media_id);
                            if removed {
                                Self::clear_asset_weight_config(&mut assets.packaging_weights);
                            }
                            removed
                        }
                    };
                    ensure!(removed, Error::<T>::AssetNotFound);
                    Ok(())
                },
            )?;

            Self::deposit_event(Event::SeasonCollectionAssetRemoved {
                season_id,
                collection_id,
                kind,
                media_id,
            });
            Ok(())
        }

        /// Reorder an artwork layer inside a draft season art collection.
        #[pallet::call_index(24)]
        #[pallet::weight(<T as Config>::WeightInfo::move_season_collection_asset())]
        #[transactional]
        pub fn move_season_collection_asset(
            origin: OriginFor<T>,
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
            kind: AssetKind,
            media_id: MediaId,
            new_index: u32,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_season_collection_draft(season_id, collection_id)?;

            let (old_index, bounded_new_index) = SeasonCollectionAssets::<T>::try_mutate(
                season_id,
                collection_id,
                |assets| -> Result<(u32, u32), DispatchError> {
                    let (old_index, bounded_new_index) = match kind {
                        AssetKind::Border => {
                            let moved = Self::move_asset_within_list(
                                &mut assets.borders,
                                media_id,
                                new_index,
                            )?;
                            Self::move_asset_weight_config_entry(
                                &mut assets.border_weights,
                                moved.0 as usize,
                                moved.1 as usize,
                            )?;
                            moved
                        }
                        AssetKind::Background => {
                            let moved = Self::move_asset_within_list(
                                &mut assets.backgrounds,
                                media_id,
                                new_index,
                            )?;
                            Self::move_asset_weight_config_entry(
                                &mut assets.background_weights,
                                moved.0 as usize,
                                moved.1 as usize,
                            )?;
                            moved
                        }
                        AssetKind::Subject => {
                            let moved = Self::move_asset_within_list(
                                &mut assets.subjects,
                                media_id,
                                new_index,
                            )?;
                            Self::move_asset_weight_config_entry(
                                &mut assets.subject_weights,
                                moved.0 as usize,
                                moved.1 as usize,
                            )?;
                            moved
                        }
                        AssetKind::Back => {
                            let moved = Self::move_asset_within_list(
                                &mut assets.backs,
                                media_id,
                                new_index,
                            )?;
                            Self::move_asset_weight_config_entry(
                                &mut assets.back_weights,
                                moved.0 as usize,
                                moved.1 as usize,
                            )?;
                            moved
                        }
                        AssetKind::PackagingFront => {
                            let moved = Self::move_asset_within_list(
                                &mut assets.packaging_fronts,
                                media_id,
                                new_index,
                            )?;
                            Self::clear_asset_weight_config(&mut assets.packaging_weights);
                            moved
                        }
                        AssetKind::PackagingBack => {
                            let moved = Self::move_asset_within_list(
                                &mut assets.packaging_backs,
                                media_id,
                                new_index,
                            )?;
                            Self::clear_asset_weight_config(&mut assets.packaging_weights);
                            moved
                        }
                    };
                    Ok((old_index, bounded_new_index))
                },
            )?;

            Self::deposit_event(Event::SeasonCollectionAssetMoved {
                season_id,
                collection_id,
                kind,
                media_id,
                old_index,
                new_index: bounded_new_index,
            });
            Ok(())
        }

        /// Set explicit asset selection weights for a draft season art collection.
        #[pallet::call_index(25)]
        #[pallet::weight(<T as Config>::WeightInfo::set_season_collection_asset_weights())]
        #[transactional]
        pub fn set_season_collection_asset_weights(
            origin: OriginFor<T>,
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
            kind: AssetWeightKind,
            weights: Vec<WeightPercentage>,
            multipliers: Vec<WeightMultiplier>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_season_collection_draft(season_id, collection_id)?;

            let custom = SeasonCollectionAssets::<T>::try_mutate(
                season_id,
                collection_id,
                |assets| -> Result<bool, DispatchError> {
                    Self::set_asset_weight_config_for_kind(assets, kind, weights, multipliers)
                },
            )?;

            Self::deposit_event(Event::SeasonCollectionAssetWeightsSet {
                season_id,
                collection_id,
                kind,
                custom,
            });
            Ok(())
        }

        /// Buy one configured step of additional card storage capacity.
        #[pallet::call_index(13)]
        #[pallet::weight(<T as Config>::WeightInfo::buy_card_capacity())]
        #[transactional]
        pub fn buy_card_capacity(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_legacy_writes_allowed()?;

            let added_slots = T::CardCapacityUpgradeAmount::get();
            let current_bonus = CardCapacityBonus::<T>::get(&player);
            let next_bonus = current_bonus.saturating_add(added_slots);
            let new_capacity = T::BaseCardCapacity::get().saturating_add(next_bonus);
            ensure!(
                new_capacity <= T::MaxOwnedCards::get(),
                Error::<T>::CardCapacityMaxReached
            );

            let price = T::CardCapacityUpgradePrice::get();
            let receiver = T::CardCapacityUpgradePriceReceiver::get();
            T::PaymentCurrency::transfer(
                &player,
                &receiver,
                price,
                ExistenceRequirement::KeepAlive,
            )?;

            CardCapacityBonus::<T>::insert(&player, next_bonus);
            Self::deposit_event(Event::CardCapacityUpgraded {
                player,
                added_slots,
                new_capacity,
                price_paid: price,
            });
            Ok(())
        }

        /// Initialize the single NFT collection used for converted cards.
        ///
        /// The `nft_admin` account becomes the NFT collection admin (intended to be the media
        /// service signer, so it can later call `nfts.set_metadata` on items).
        #[pallet::call_index(15)]
        #[pallet::weight(<T as Config>::WeightInfo::init_card_nft_collection())]
        #[transactional]
        pub fn init_card_nft_collection(
            origin: OriginFor<T>,
            nft_admin: T::AccountId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            ensure!(
                CardNftCollectionId::<T>::get().is_none(),
                Error::<T>::NftCollectionAlreadyInitialized
            );

            let collection_id = pallet_nfts::NextCollectionId::<T>::get().unwrap_or(0);
            let admin = T::Lookup::unlookup(nft_admin.clone());

            let config = pallet_nfts::CollectionConfig {
                settings: pallet_nfts::CollectionSettings::all_enabled(),
                max_supply: None,
                mint_settings: pallet_nfts::MintSettings::default(),
            };

            pallet_nfts::Pallet::<T>::create(
                frame_system::RawOrigin::Signed(who).into(),
                admin,
                config,
            )?;

            CardNftCollectionId::<T>::put(collection_id);
            Self::deposit_event(Event::CardNftCollectionInitialized {
                collection_id,
                admin: nft_admin,
            });
            Ok(())
        }

        /// Convert a finalized card to an NFT (withdraw model).
        ///
        /// The card is transferred to an escrow account controlled by this pallet, and an NFT
        /// item is minted with `item_id = card_id`.
        #[pallet::call_index(16)]
        #[pallet::weight(<T as Config>::WeightInfo::convert_to_nft())]
        #[transactional]
        pub fn convert_to_nft(origin: OriginFor<T>, card_id: u32) -> DispatchResult {
            let who = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&who)?;
            Self::ensure_legacy_writes_allowed()?;

            let collection_id =
                CardNftCollectionId::<T>::get().ok_or(Error::<T>::NftCollectionNotInitialized)?;
            ensure!(
                !Converted::<T>::contains_key(card_id),
                Error::<T>::CardAlreadyConverted
            );

            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == who, Error::<T>::NotCardOwner);
            ensure!(card_info.finalized, Error::<T>::CardNotFinalized);
            Self::ensure_card_not_account_bound(card_id)?;
            ensure!(
                !T::HandChecker::is_card_in_current_hand(&who, card_id),
                Error::<T>::CardInCurrentHand
            );
            ensure!(
                CardArtwork::<T>::contains_key(card_id),
                Error::<T>::CardArtworkMissing
            );

            // If listed, unlist first so indices remain consistent.
            if CardPrices::<T>::contains_key(card_id) {
                Self::unlist(card_id, &who);
            }

            let escrow = Self::escrow_account_id();
            Self::do_transfer(&who, &escrow, card_id)?;

            Converted::<T>::insert(card_id, ());

            pallet_nfts::Pallet::<T>::do_mint(
                collection_id,
                card_id,
                None,
                who.clone(),
                pallet_nfts::ItemConfig::default(),
                |_, _| Ok(()),
            )?;
            Self::transition_v16_beneficial_owner(
                card_id,
                &who,
                &who,
                LegacyCustodyKind::NftWrapped,
                false,
            )?;

            Self::deposit_event(Event::CardConvertedToNft {
                card_id,
                collection_id,
                item_id: card_id,
            });
            Ok(())
        }

        /// Unwrap a converted card NFT back into a playable TCG card.
        ///
        /// Burns the NFT item and transfers the card out of escrow to the NFT owner.
        #[pallet::call_index(17)]
        #[pallet::weight(<T as Config>::WeightInfo::unwrap_from_nft())]
        #[transactional]
        pub fn unwrap_from_nft(origin: OriginFor<T>, card_id: u32) -> DispatchResult {
            let who = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&who)?;
            Self::ensure_legacy_writes_allowed()?;

            let collection_id =
                CardNftCollectionId::<T>::get().ok_or(Error::<T>::NftCollectionNotInitialized)?;
            ensure!(
                Converted::<T>::contains_key(card_id),
                Error::<T>::CardNotConverted
            );

            let escrow = Self::escrow_account_id();
            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == escrow, Error::<T>::CardNotEscrowed);

            let nft_owner = pallet_nfts::Pallet::<T>::owner(collection_id, card_id)
                .ok_or(Error::<T>::NotNftOwner)?;
            ensure!(nft_owner == who, Error::<T>::NotNftOwner);

            pallet_nfts::Pallet::<T>::do_burn(collection_id, card_id, |_| Ok(()))?;
            Converted::<T>::remove(card_id);

            Self::do_transfer(&escrow, &who, card_id)?;
            Self::transition_v16_beneficial_owner(
                card_id,
                &who,
                &who,
                LegacyCustodyKind::Ordinary,
                false,
            )?;

            Self::deposit_event(Event::CardUnwrappedFromNft { card_id });
            Ok(())
        }

        /// Create or replace a card-specific progression tree.
        #[pallet::call_index(27)]
        #[pallet::weight(<T as Config>::WeightInfo::set_progression_tree())]
        #[transactional]
        pub fn set_progression_tree(
            origin: OriginFor<T>,
            tree_id: ProgressionTreeId,
            subject_id: SubjectId,
            rarity: Option<u8>,
            nodes: Vec<ProgressionNode>,
            config_version: NexusConfigVersion,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_legacy_writes_allowed()?;
            ensure!(rarity.is_none(), Error::<T>::InvalidProgressionTree);
            let bounded_nodes = Self::validated_progression_nodes(nodes, config_version)?;

            if let Some(old_tree) = ProgressionTrees::<T>::get(tree_id) {
                ensure!(
                    ProgressionTreeUseCounts::<T>::get(tree_id) == 0,
                    Error::<T>::ProgressionTreeAlreadyInUse
                );
                if old_tree.subject_id != subject_id || old_tree.rarity != rarity {
                    ProgressionTreeBySubject::<T>::remove(old_tree.subject_id, old_tree.rarity);
                }
            } else {
                ProgressionTreeIds::<T>::try_mutate(|ids| -> DispatchResult {
                    ids.try_push(tree_id)
                        .map_err(|_| Error::<T>::InvalidProgressionTree)?;
                    Ok(())
                })?;
            }

            let node_count = bounded_nodes.len().saturated_into::<u32>();
            ProgressionTrees::<T>::insert(
                tree_id,
                ProgressionTree {
                    tree_id,
                    subject_id,
                    rarity,
                    nodes: bounded_nodes.clone(),
                    config_version,
                },
            );
            ProgressionTreeBySubject::<T>::insert(subject_id, rarity, tree_id);

            Self::deposit_event(Event::ProgressionTreeSet {
                tree_id,
                subject_id,
                rarity,
                node_count,
                nodes: bounded_nodes,
                config_version,
            });
            Ok(())
        }

        /// Root repair path to initialize progression for an existing card.
        #[pallet::call_index(28)]
        #[pallet::weight(<T as Config>::WeightInfo::assign_progression_tree_to_card())]
        #[transactional]
        pub fn assign_progression_tree_to_card(
            origin: OriginFor<T>,
            card_id: u32,
            tree_id: ProgressionTreeId,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_legacy_card_not_frozen(card_id)?;
            Self::ensure_card_exists(card_id)?;
            let tree =
                ProgressionTrees::<T>::get(tree_id).ok_or(Error::<T>::ProgressionTreeMissing)?;
            if CardProgressions::<T>::contains_key(card_id) {
                return Ok(());
            }
            Self::initialize_card_progression(card_id, tree_id, tree.config_version)
        }

        /// Grant XP to a card from an authorized game/reward issuer.
        #[pallet::call_index(29)]
        #[pallet::weight(<T as Config>::WeightInfo::grant_card_experience())]
        #[transactional]
        pub fn grant_card_experience(
            origin: OriginFor<T>,
            game_id: ProgressionGameId,
            version_id: ProgressionVersionId,
            event_type_id: ProgressionEventTypeId,
            card_id: u32,
            amount: u32,
        ) -> DispatchResult {
            let issuer = ensure_signed(origin)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_legacy_card_not_frozen(card_id)?;
            ensure!(
                amount <= T::MaxCardXpGrantAmount::get(),
                Error::<T>::CardXpGrantTooLarge
            );
            let authority_id = T::ProgressionAuthorityProvider::resolve_authority(
                &issuer,
                game_id,
                Some(version_id),
                event_type_id,
            )
            .ok_or(Error::<T>::NotAuthorizedProgressionIssuer)?;
            ensure!(
                CardProgressions::<T>::contains_key(card_id),
                Error::<T>::CardProgressionMissing
            );

            CardProgressions::<T>::try_mutate(card_id, |maybe_progression| -> DispatchResult {
                let progression = maybe_progression
                    .as_mut()
                    .ok_or(Error::<T>::CardProgressionMissing)?;
                progression.experience = progression.experience.saturating_add(amount);
                progression.level = Self::level_for_experience(progression.experience);

                Self::deposit_event(Event::CardExperienceGranted {
                    issuer: issuer.clone(),
                    authority_id,
                    game_id,
                    version_id,
                    event_type_id,
                    card_id,
                    amount,
                    experience: progression.experience,
                    level: progression.level,
                    config_version: progression.config_version,
                });
                Ok(())
            })
        }

        /// Permanently attach a required inventory item to an unlocked card progression node.
        #[pallet::call_index(30)]
        #[pallet::weight(<T as Config>::WeightInfo::forge_progression_node())]
        #[transactional]
        pub fn forge_progression_node(
            origin: OriginFor<T>,
            card_id: u32,
            node_id: ProgressionNodeId,
            gear_id: GearId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_card_build_mutable(card_id, &who)?;

            let progression =
                CardProgressions::<T>::get(card_id).ok_or(Error::<T>::CardProgressionMissing)?;
            ensure!(
                !progression.completed_nodes.contains(&node_id),
                Error::<T>::ProgressionNodeAlreadyCompleted
            );
            let tree = ProgressionTrees::<T>::get(progression.tree_id)
                .ok_or(Error::<T>::ProgressionTreeMissing)?;
            let node = tree
                .nodes
                .iter()
                .find(|candidate| candidate.node_id == node_id)
                .copied()
                .ok_or(Error::<T>::ProgressionNodeMissing)?;
            ensure!(
                progression.level >= node.required_level,
                Error::<T>::ProgressionNodeLocked
            );

            let item_template_id =
                GearItemTemplates::<T>::get(gear_id).ok_or(Error::<T>::RequiredItemMissing)?;
            ensure!(
                item_template_id == node.required_item_template_id,
                Error::<T>::RequiredItemMismatch
            );

            NexusGearItems::<T>::try_mutate_exists(gear_id, |maybe_gear| -> DispatchResult {
                let gear = maybe_gear.as_ref().ok_or(Error::<T>::RequiredItemMissing)?;
                ensure!(gear.owner == who, Error::<T>::RequiredItemMissing);
                if let Some(slot_type) = node.gear_slot_type {
                    ensure!(
                        gear.slot_type == slot_type,
                        Error::<T>::RequiredItemMismatch
                    );
                }
                ensure!(
                    gear.equipped_card_id.is_none(),
                    Error::<T>::GearAlreadyAttached
                );
                *maybe_gear = None;
                Ok(())
            })?;
            GearItemTemplates::<T>::remove(gear_id);

            CardProgressions::<T>::try_mutate(card_id, |maybe_progression| -> DispatchResult {
                let progression = maybe_progression
                    .as_mut()
                    .ok_or(Error::<T>::CardProgressionMissing)?;
                progression
                    .completed_nodes
                    .try_push(node_id)
                    .map_err(|_| Error::<T>::InvalidProgressionTree)?;
                Ok(())
            })?;

            let attachment = CardEquipmentAttachment {
                card_id,
                node_id,
                gear_id,
                item_template_id,
                attached_at: <frame_system::Pallet<T>>::block_number(),
                config_version: progression.config_version,
            };
            CardEquipmentAttachments::<T>::insert(card_id, node_id, attachment);

            Self::deposit_event(Event::ProgressionNodeForged {
                account_id: who,
                card_id,
                node_id,
                gear_id,
                item_template_id,
                power_delta: node.power_delta,
                config_version: progression.config_version,
            });
            Ok(())
        }

        /// Replace a card's removable magic loadout.
        #[pallet::call_index(31)]
        #[pallet::weight(<T as Config>::WeightInfo::set_card_magic_loadout(spells.len() as u32))]
        #[transactional]
        pub fn set_card_magic_loadout(
            origin: OriginFor<T>,
            card_id: u32,
            spells: Vec<SpellId>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&who)?;
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_card_build_mutable(card_id, &who)?;

            let bounded_spells: BoundedMagicSpells<T> = spells
                .try_into()
                .map_err(|_| Error::<T>::MagicSlotLimitExceeded)?;
            let mut seen = BoundedMagicSpellSet::<T>::new();
            for spell_id in bounded_spells.iter() {
                let inserted = seen
                    .try_insert(*spell_id)
                    .map_err(|_| Error::<T>::MagicSlotLimitExceeded)?;
                ensure!(inserted, Error::<T>::DuplicateSpellInLoadout);
                let spell = NexusSpellbook::<T>::get(spell_id).ok_or(Error::<T>::SpellNotOwned)?;
                ensure!(spell.owner == who, Error::<T>::SpellNotOwned);
            }

            let config_version = CardProgressions::<T>::get(card_id)
                .map(|progression| progression.config_version)
                .unwrap_or_else(|| Self::current_nexus_config().config_version);
            let loadout = CardMagicLoadout {
                card_id,
                spells: bounded_spells.clone(),
                config_version,
            };
            CardMagicLoadouts::<T>::insert(card_id, loadout);

            Self::deposit_event(Event::CardMagicLoadoutUpdated {
                account_id: who,
                card_id,
                spells: bounded_spells,
                config_version,
            });
            Ok(())
        }

        /// Root-only internal-alpha helper to seed template-backed progression gear.
        ///
        /// This is not the public Craft Items loop. It exists so alpha operators can seed
        /// required inventory items that can be consumed by `forge_progression_node`.
        #[pallet::call_index(32)]
        #[pallet::weight(<T as Config>::WeightInfo::seed_alpha_progression_gear())]
        #[transactional]
        #[allow(clippy::too_many_arguments)]
        pub fn seed_alpha_progression_gear(
            origin: OriginFor<T>,
            owner: T::AccountId,
            gear_id: GearId,
            item_template_id: ItemTemplateId,
            slot_type: GearSlotType,
            tier: GearTier,
            power: u16,
            season_id: SeasonId,
            config_version: NexusConfigVersion,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_legacy_writes_allowed()?;
            ensure!(
                !NexusGearItems::<T>::contains_key(gear_id)
                    && !GearItemTemplates::<T>::contains_key(gear_id),
                Error::<T>::AlphaGearAlreadyExists
            );

            NexusGearItems::<T>::insert(
                gear_id,
                GearItem {
                    owner: owner.clone(),
                    gear_id,
                    slot_type,
                    tier,
                    power,
                    spell_slots: BoundedNexusSpellSlots::<T>::default(),
                    equipped_card_id: None,
                    season_id,
                    config_version,
                },
            );
            GearItemTemplates::<T>::insert(gear_id, item_template_id);

            Self::deposit_event(Event::AlphaProgressionGearSeeded {
                account_id: owner,
                gear_id,
                item_template_id,
                slot_type,
                tier,
                power,
                season_id,
                config_version,
            });
            Ok(())
        }

        /// Root-only internal-alpha helper to seed removable magic spells.
        ///
        /// This is not the public Craft Items loop. It exists so alpha operators can seed
        /// spellbook entries that can be selected by `set_card_magic_loadout`.
        #[pallet::call_index(33)]
        #[pallet::weight(<T as Config>::WeightInfo::seed_alpha_spell())]
        #[transactional]
        pub fn seed_alpha_spell(
            origin: OriginFor<T>,
            owner: T::AccountId,
            spell_id: SpellId,
            element: Element,
            power: u16,
            config_version: NexusConfigVersion,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_legacy_writes_allowed()?;
            ensure!(
                !NexusSpellbook::<T>::contains_key(spell_id),
                Error::<T>::AlphaSpellAlreadyExists
            );

            NexusSpellbook::<T>::insert(
                spell_id,
                SpellEntry {
                    owner: owner.clone(),
                    spell_id,
                    element,
                    power,
                    slotted_to: None,
                    config_version,
                },
            );

            Self::deposit_event(Event::AlphaSpellSeeded {
                account_id: owner,
                spell_id,
                element,
                power,
                config_version,
            });
            Ok(())
        }

        /// Publish an immutable Nexus V2 subject definition. Definitions are never
        /// overwritten; their independently stored activation state controls future use.
        #[pallet::call_index(36)]
        #[pallet::weight(<T as Config>::WeightInfo::publish_v2_catalog())]
        #[transactional]
        pub fn publish_subject_definition_v2(
            origin: OriginFor<T>,
            definition: SubjectDefinitionV2,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            ensure!(
                !SubjectDefinitionsV2::<T>::contains_key(definition.subject_definition_id)
                    && !SubjectDefinitionByKeyV2::<T>::contains_key((
                        definition.subject_id,
                        definition.subject_version,
                    )),
                Error::<T>::V2DefinitionAlreadyPublished
            );
            SubjectDefinitionsV2::<T>::insert(definition.subject_definition_id, definition);
            SubjectDefinitionByKeyV2::<T>::insert(
                (definition.subject_id, definition.subject_version),
                definition.subject_definition_id,
            );
            SubjectActivationStatesV2::<T>::insert(
                definition.subject_definition_id,
                SubjectActivationState {
                    subject_definition_id: definition.subject_definition_id,
                    mint_enabled: false,
                    conversion_enabled: false,
                },
            );
            Self::deposit_event(Event::SubjectDefinitionV2Published { definition });
            Ok(())
        }

        #[pallet::call_index(37)]
        #[pallet::weight(<T as Config>::WeightInfo::publish_v2_catalog())]
        pub fn set_subject_activation_v2(
            origin: OriginFor<T>,
            state: SubjectActivationState,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            ensure!(
                SubjectDefinitionsV2::<T>::contains_key(state.subject_definition_id),
                Error::<T>::V2DefinitionMissing
            );
            SubjectActivationStatesV2::<T>::insert(state.subject_definition_id, state);
            Self::deposit_event(Event::SubjectActivationV2Changed { state });
            Ok(())
        }

        /// Publish all five deterministic rarity profiles atomically. The array order
        /// is the frozen rarity wire order Common through Mythical.
        #[pallet::call_index(38)]
        #[pallet::weight(<T as Config>::WeightInfo::publish_v2_catalog())]
        #[transactional]
        pub fn publish_subject_rarity_profiles_v2(
            origin: OriginFor<T>,
            subject_id: SubjectId,
            subject_version: u32,
            profiles: [SubjectRarityProfile; 5],
            catalog_version: u32,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            let definition_id = SubjectDefinitionByKeyV2::<T>::get((subject_id, subject_version))
                .ok_or(Error::<T>::V2DefinitionMissing)?;
            let definition = SubjectDefinitionsV2::<T>::get(definition_id)
                .ok_or(Error::<T>::V2DefinitionMissing)?;
            ensure!(
                definition.catalog_version == catalog_version,
                Error::<T>::V2CatalogVersionMismatch
            );
            let rarities = [
                CardRarity::Common,
                CardRarity::Rare,
                CardRarity::Epic,
                CardRarity::Legendary,
                CardRarity::Mythical,
            ];
            let mut profile_ids = [0u32; 5];
            for (index, profile) in profiles.iter().copied().enumerate() {
                let monotonic = index == 0
                    || profile.does_not_decrease_from(
                        profiles
                            .get(index - 1)
                            .expect("index is nonzero; previous profile exists"),
                    );
                ensure!(
                    profile.subject_id == subject_id
                        && profile.subject_version == subject_version
                        && profile.rarity == rarities[index]
                        && profile.validate()
                        && monotonic
                        && !SubjectRarityProfilesV2::<T>::contains_key(profile.profile_id)
                        && !SubjectRarityProfileByKeyV2::<T>::contains_key(
                            (subject_id, subject_version),
                            profile.rarity,
                        ),
                    Error::<T>::V2InvalidProfiles
                );
                profile_ids[index] = profile.profile_id;
                SubjectRarityProfilesV2::<T>::insert(profile.profile_id, profile);
                SubjectRarityProfileByKeyV2::<T>::insert(
                    (subject_id, subject_version),
                    profile.rarity,
                    profile.profile_id,
                );
            }
            Self::deposit_event(Event::SubjectRarityProfilesV2Published {
                subject_id,
                subject_version,
                profile_ids,
                catalog_version,
            });
            Ok(())
        }

        #[pallet::call_index(39)]
        #[pallet::weight(<T as Config>::WeightInfo::publish_v2_catalog())]
        pub fn publish_pose_definition_v2(
            origin: OriginFor<T>,
            definition: MediaDefinitionV2,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            ensure!(
                !PoseDefinitionsV2::<T>::contains_key(definition.definition_id),
                Error::<T>::V2DefinitionAlreadyPublished
            );
            ensure!(
                definition.subject_id.is_some(),
                Error::<T>::V2DefinitionMismatch
            );
            PoseDefinitionsV2::<T>::insert(definition.definition_id, definition);
            Self::deposit_event(Event::PoseDefinitionV2Published { definition });
            Ok(())
        }

        #[pallet::call_index(40)]
        #[pallet::weight(<T as Config>::WeightInfo::publish_v2_catalog())]
        pub fn publish_background_definition_v2(
            origin: OriginFor<T>,
            definition: MediaDefinitionV2,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            ensure!(
                !BackgroundDefinitionsV2::<T>::contains_key(definition.definition_id),
                Error::<T>::V2DefinitionAlreadyPublished
            );
            BackgroundDefinitionsV2::<T>::insert(definition.definition_id, definition);
            Self::deposit_event(Event::BackgroundDefinitionV2Published { definition });
            Ok(())
        }

        #[pallet::call_index(41)]
        #[pallet::weight(<T as Config>::WeightInfo::publish_v2_pool())]
        #[transactional]
        pub fn publish_acquisition_pool_v2(
            origin: OriginFor<T>,
            pool_id: u32,
            version: u32,
            set_id: u32,
            profile_ids: Vec<u32>,
            pose_definition_ids: Vec<u32>,
            background_definition_ids: Vec<u32>,
            immutable_config_hash: Hash32,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            ensure!(
                !AcquisitionPoolVersionsV2::<T>::contains_key((pool_id, version)),
                Error::<T>::V2PoolAlreadyPublished
            );
            ensure!(
                !profile_ids.is_empty()
                    && !pose_definition_ids.is_empty()
                    && !background_definition_ids.is_empty(),
                Error::<T>::V2InvalidPool
            );
            let mut unique_profiles = sp_std::collections::btree_set::BTreeSet::new();
            let mut profiles = PoolProfileEntriesOf::<T>::default();
            for profile_id in profile_ids {
                ensure!(
                    unique_profiles.insert(profile_id)
                        && SubjectRarityProfilesV2::<T>::contains_key(profile_id),
                    Error::<T>::V2InvalidPool
                );
                profiles
                    .try_push(PoolProfileEntry { profile_id })
                    .map_err(|_| Error::<T>::V2InvalidPool)?;
            }
            let mut unique_poses = sp_std::collections::btree_set::BTreeSet::new();
            let mut poses = PoolPoseIdsOf::<T>::default();
            for definition_id in pose_definition_ids {
                ensure!(
                    unique_poses.insert(definition_id)
                        && PoseDefinitionsV2::<T>::contains_key(definition_id),
                    Error::<T>::V2InvalidPool
                );
                poses
                    .try_push(definition_id)
                    .map_err(|_| Error::<T>::V2InvalidPool)?;
            }
            let mut unique_backgrounds = sp_std::collections::btree_set::BTreeSet::new();
            let mut backgrounds = PoolBackgroundIdsOf::<T>::default();
            for definition_id in background_definition_ids {
                ensure!(
                    unique_backgrounds.insert(definition_id)
                        && BackgroundDefinitionsV2::<T>::contains_key(definition_id),
                    Error::<T>::V2InvalidPool
                );
                backgrounds
                    .try_push(definition_id)
                    .map_err(|_| Error::<T>::V2InvalidPool)?;
            }
            Self::register_v2_protection_layout(set_id, &profiles, &poses, &backgrounds)?;
            AcquisitionPoolVersionsV2::<T>::insert(
                (pool_id, version),
                AcquisitionPoolVersion {
                    pool_id,
                    version,
                    set_id,
                    profiles,
                    poses,
                    backgrounds,
                    immutable_config_hash,
                },
            );
            Self::deposit_event(Event::AcquisitionPoolVersionV2Published {
                pool_id,
                version,
                immutable_config_hash,
            });
            Ok(())
        }

        #[pallet::call_index(42)]
        #[pallet::weight(<T as Config>::WeightInfo::publish_v2_pack_sku())]
        pub fn publish_pack_sku_version_v2(
            origin: OriginFor<T>,
            sku: PackSkuVersion<BlockNumberFor<T>>,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            ensure!(
                !PackSkuVersionsV2::<T>::contains_key((sku.pack_sku, sku.version)),
                Error::<T>::V2PackSkuAlreadyPublished
            );
            ensure!(
                sku.validates_weights()
                    && AcquisitionPoolVersionsV2::<T>::get((sku.pool_id, sku.pool_version))
                        .map(|pool| pool.set_id == sku.set_id
                            && pool.immutable_config_hash == sku.immutable_config_hash)
                        .unwrap_or(false)
                    && sku
                        .active_until
                        .map(|until| until > sku.active_from)
                        .unwrap_or(true),
                Error::<T>::V2InvalidPackSku
            );
            PackSkuVersionsV2::<T>::insert((sku.pack_sku, sku.version), sku);
            Self::deposit_event(Event::PackSkuVersionV2Published {
                pack_sku: sku.pack_sku,
                version: sku.version,
                immutable_config_hash: sku.immutable_config_hash,
                odds_metadata_hash: sku.odds_metadata_hash,
            });
            Ok(())
        }

        /// Private-alpha seed only. Production and paid credit issuance are absent.
        #[pallet::call_index(43)]
        #[pallet::weight(<T as Config>::WeightInfo::issue_v2_training_credit())]
        #[transactional]
        pub fn issue_training_pack_credit_v2(
            origin: OriginFor<T>,
            owner: T::AccountId,
            pack_sku: u32,
            sku_version: u32,
            tutorial_id: Hash32,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(
                tutorial_id.iter().any(|byte| *byte != 0),
                Error::<T>::V2TutorialIdRequired
            );
            if let Some(receipt) = TutorialPackCreditGrantReceiptsV2::<T>::get(tutorial_id) {
                ensure!(
                    receipt.owner == owner
                        && receipt.pack_sku == pack_sku
                        && receipt.sku_version == sku_version,
                    Error::<T>::V2TutorialPackCreditGrantConflict
                );
                return Ok(());
            }
            Self::ensure_v16_migration_complete()?;
            let credit_id = Self::do_issue_credit(
                &owner,
                pack_sku,
                sku_version,
                EconomicRealm::Training,
                PackCreditSource::TutorialTraining { tutorial_id },
            )?;
            TutorialPackCreditGrantReceiptsV2::<T>::insert(
                tutorial_id,
                TutorialPackCreditGrantReceipt {
                    owner,
                    credit_id,
                    pack_sku,
                    sku_version,
                },
            );
            Ok(())
        }

        #[pallet::call_index(44)]
        #[pallet::weight(<T as Config>::WeightInfo::request_v2_pack_open())]
        #[transactional]
        pub fn request_pack_open_v2(
            origin: OriginFor<T>,
            pack_sku: u32,
            sku_version: u32,
            economic_realm: EconomicRealm,
            commitment: Hash32,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            if Self::existing_pack_opening_request(
                &owner,
                pack_sku,
                sku_version,
                economic_realm,
                commitment,
            )?
            .is_some()
            {
                return Ok(());
            }
            ensure!(
                commitment.iter().any(|byte| *byte != 0),
                Error::<T>::V2EntropyCommitmentRequired
            );
            T::AccessControl::ensure_whitelisted(&owner)?;
            ensure!(
                V2FeatureEnabled::<T>::get(V2Feature::Packs),
                Error::<T>::V2FeatureDisabled
            );
            Self::do_request_pack_open(&owner, pack_sku, sku_version, economic_realm, commitment)
        }

        /// Permissionless retryable finalization. No card is written unless all six
        /// selections and card writes succeed transactionally.
        #[pallet::call_index(45)]
        #[pallet::weight(<T as Config>::WeightInfo::finalize_v2_pack_open())]
        #[transactional]
        pub fn finalize_pack_open_v2(origin: OriginFor<T>, opening_id: Hash32) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            Self::do_finalize_pack_open(opening_id)
        }

        #[pallet::call_index(46)]
        #[pallet::weight(<T as Config>::WeightInfo::timeout_v2_pack_open())]
        #[transactional]
        pub fn timeout_pack_open_v2(origin: OriginFor<T>, opening_id: Hash32) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            Self::do_timeout_pack_open(opening_id)
        }

        #[pallet::call_index(47)]
        #[pallet::weight(<T as Config>::WeightInfo::publish_v2_format())]
        pub fn publish_competitive_format_v2(
            origin: OriginFor<T>,
            format: CompetitiveFormatV2,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            ensure!(
                !CompetitiveFormatsV2::<T>::contains_key((format.format_id, format.version)),
                Error::<T>::V2FormatAlreadyPublished
            );
            ensure!(
                format.team_size == V2_BRING_FIVE_TEAM_SIZE
                    && u32::from(V2_BRING_FIVE_TEAM_SIZE) <= T::MaxV2TeamSize::get()
                    && format.rarity_load_budget >= format.team_size
                    && format.max_mythical <= format.team_size
                    && format.max_legendary_or_better <= format.team_size,
                Error::<T>::V2InvalidFormat
            );
            CompetitiveFormatsV2::<T>::insert((format.format_id, format.version), format);
            Self::deposit_event(Event::CompetitiveFormatV2Published { format });
            Ok(())
        }

        #[pallet::call_index(48)]
        #[pallet::weight(<T as Config>::WeightInfo::save_v2_team())]
        #[transactional]
        pub fn save_competitive_team_v2(
            origin: OriginFor<T>,
            team_id: u32,
            format_id: u32,
            format_version: u32,
            card_ids: V2TeamCardsOf<T>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&owner)?;
            ensure!(
                V2FeatureEnabled::<T>::get(V2Feature::Ranked),
                Error::<T>::V2FeatureDisabled
            );
            let (cards, rarity_load, realm, set_id) = Self::validated_competitive_team(
                &owner,
                format_id,
                format_version,
                card_ids.into_inner(),
            )?;
            CompetitiveTeamsV2::<T>::insert(
                &owner,
                team_id,
                CompetitiveTeamV2 {
                    owner: owner.clone(),
                    team_id,
                    format_id,
                    format_version,
                    cards: cards.clone(),
                    rarity_load,
                },
            );
            ConversionSafetyTeamByRealmSetV2::<T>::insert(&owner, (set_id, realm), team_id);
            Self::deposit_event(Event::CompetitiveTeamV2Saved {
                owner,
                team_id,
                format_id,
                format_version,
                card_ids: cards,
                rarity_load,
            });
            Ok(())
        }

        #[pallet::call_index(49)]
        #[pallet::weight(<T as Config>::WeightInfo::set_v2_feature())]
        pub fn set_v2_feature_enabled(
            origin: OriginFor<T>,
            feature: V2Feature,
            enabled: bool,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            V2FeatureEnabled::<T>::insert(feature, enabled);
            Self::deposit_event(Event::V2FeatureStatusChanged { feature, enabled });
            Ok(())
        }

        /// Irreversibly commit a card before future randomness exists. There is no
        /// cancellation call and the card is removed from active supply immediately.
        #[pallet::call_index(50)]
        #[pallet::weight(<T as Config>::WeightInfo::request_v2_conversion())]
        #[transactional]
        pub fn request_conversion_v2(
            origin: OriginFor<T>,
            card_id: CardIdV2,
            expected_catalog_version: u32,
            entropy_commitment: Hash32,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            if let Some(request_id) = ConversionRequestByCard::<T>::get(card_id) {
                let tombstone = CardConversionTombstones::<T>::get(request_id)
                    .ok_or(Error::<T>::V2ConversionRequestConflict)?;
                ensure!(
                    tombstone.owner == owner
                        && tombstone.commitment == entropy_commitment
                        && tombstone.expected_catalog_version == expected_catalog_version,
                    Error::<T>::V2ConversionRequestConflict
                );
                return Ok(());
            }
            ensure!(
                entropy_commitment.iter().any(|byte| *byte != 0),
                Error::<T>::V2EntropyCommitmentRequired
            );
            T::AccessControl::ensure_whitelisted(&owner)?;
            ensure!(
                V2FeatureEnabled::<T>::get(V2Feature::Conversion),
                Error::<T>::V2FeatureDisabled
            );
            Self::do_request_conversion(
                &owner,
                card_id,
                expected_catalog_version,
                entropy_commitment,
            )
        }

        #[pallet::call_index(51)]
        #[pallet::weight(<T as Config>::WeightInfo::finalize_v2_conversion())]
        #[transactional]
        pub fn finalize_conversion_v2(origin: OriginFor<T>, request_id: Hash32) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            Self::do_finalize_conversion(request_id, false)
        }

        /// Conversion outage resolution never restores the card. It creates the
        /// pre-reserved entity with a clearly tagged all-15 Stasis genome.
        #[pallet::call_index(52)]
        #[pallet::weight(<T as Config>::WeightInfo::timeout_v2_conversion())]
        #[transactional]
        pub fn timeout_conversion_v2(origin: OriginFor<T>, request_id: Hash32) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            let tombstone = CardConversionTombstones::<T>::get(request_id)
                .ok_or(Error::<T>::V2ConversionMissing)?;
            match tombstone.resolution {
                ConversionResolution::StasisTimeout => return Ok(()),
                ConversionResolution::Created => {
                    return Err(Error::<T>::V2ConversionTerminalConflict.into())
                }
                ConversionResolution::Pending => {}
            }
            ensure!(
                T::V2Randomness::timed_out(tombstone.randomness_request_id),
                Error::<T>::V2ConversionNotTimedOut
            );
            Self::do_finalize_conversion(request_id, true)
        }

        #[pallet::call_index(53)]
        #[pallet::weight(<T as Config>::WeightInfo::configure_v2_ascension())]
        pub fn configure_mythical_ascension_season_v2(
            origin: OriginFor<T>,
            config: MythicalAscensionSeasonConfig<BlockNumberFor<T>>,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            ensure!(
                !MythicalAscensionSeasonConfigsV2::<T>::contains_key(config.season_id),
                Error::<T>::V2AscensionSeasonAlreadyConfigured
            );
            let pool = AcquisitionPoolVersionsV2::<T>::get((config.pool_id, config.pool_version))
                .ok_or(Error::<T>::V2PoolMissing)?;
            let starts_at = config.starts_at.saturated_into::<u128>();
            let ends_at = config.ends_at.saturated_into::<u128>();
            let configured_duration = ends_at.checked_sub(starts_at);
            let required_duration =
                T::MythicalAscensionSeasonDurationBlocks::get().saturated_into::<u128>();
            let week_duration =
                T::MythicalAscensionWeekDurationBlocks::get().saturated_into::<u128>();
            ensure!(
                pool.set_id == config.set_id
                    && config.starts_at < config.ends_at
                    && configured_duration == Some(required_duration)
                    && required_duration > 0
                    && week_duration > 0
                    && week_duration
                        .checked_mul(u128::from(config.available_weeks))
                        .map(|available| available <= required_duration)
                        .unwrap_or(false)
                    && config.required_mastery == 10
                    && config.required_marks == 10
                    && config.available_weeks == 12
                    && config.config_hash.iter().any(|byte| *byte != 0),
                Error::<T>::V2AscensionConfigInvalid
            );
            MythicalAscensionSeasonConfigsV2::<T>::insert(config.season_id, config);
            Self::deposit_event(Event::MythicalAscensionSeasonConfiguredV2 { config });
            Ok(())
        }

        #[pallet::call_index(54)]
        #[pallet::weight(<T as Config>::WeightInfo::configure_v2_ascension())]
        pub fn configure_mythical_ascension_subject_v2(
            origin: OriginFor<T>,
            config: MythicalAscensionSubjectConfig,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            ensure!(
                !MythicalAscensionSubjectConfigsV2::<T>::contains_key(
                    config.season_id,
                    config.subject_id,
                ),
                Error::<T>::V2AscensionSubjectAlreadyConfigured
            );
            let season = MythicalAscensionSeasonConfigsV2::<T>::get(config.season_id)
                .ok_or(Error::<T>::V2AscensionSeasonMissing)?;
            let definition_id =
                SubjectDefinitionByKeyV2::<T>::get((config.subject_id, config.subject_version))
                    .ok_or(Error::<T>::V2DefinitionMissing)?;
            let activation = SubjectActivationStatesV2::<T>::get(definition_id)
                .ok_or(Error::<T>::V2DefinitionMissing)?;
            let profile_id = SubjectRarityProfileByKeyV2::<T>::get(
                (config.subject_id, config.subject_version),
                CardRarity::Mythical,
            )
            .ok_or(Error::<T>::V2DefinitionMissing)?;
            let pose = PoseDefinitionsV2::<T>::get(config.foundation_pose_definition_id)
                .ok_or(Error::<T>::V2DefinitionMissing)?;
            let background =
                BackgroundDefinitionsV2::<T>::get(config.foundation_background_definition_id)
                    .ok_or(Error::<T>::V2DefinitionMissing)?;
            let pool = AcquisitionPoolVersionsV2::<T>::get((season.pool_id, season.pool_version))
                .ok_or(Error::<T>::V2PoolMissing)?;
            ensure!(
                activation.mint_enabled
                    && pose.subject_id == Some(config.subject_id)
                    && (background.subject_id.is_none()
                        || background.subject_id == Some(config.subject_id))
                    && pool
                        .profiles
                        .iter()
                        .any(|entry| entry.profile_id == profile_id)
                    && pool.poses.contains(&config.foundation_pose_definition_id)
                    && pool
                        .backgrounds
                        .contains(&config.foundation_background_definition_id)
                    && config.config_hash.iter().any(|byte| *byte != 0),
                Error::<T>::V2AscensionConfigInvalid
            );
            MythicalAscensionSubjectConfigsV2::<T>::insert(
                config.season_id,
                config.subject_id,
                config,
            );
            Self::deposit_event(Event::MythicalAscensionSubjectConfiguredV2 { config });
            Ok(())
        }

        #[pallet::call_index(55)]
        #[pallet::weight(<T as Config>::WeightInfo::link_v2_season_eligibility())]
        pub fn link_season_eligibility_v2(
            origin: OriginFor<T>,
            account: T::AccountId,
            season_id: u32,
            season_eligibility_id: Hash32,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            ensure!(
                MythicalAscensionSeasonConfigsV2::<T>::contains_key(season_id)
                    && season_eligibility_id.iter().any(|byte| *byte != 0),
                Error::<T>::V2AscensionConfigInvalid
            );
            if let Some(existing) = SeasonEligibilityByAccountV2::<T>::get(&account, season_id) {
                ensure!(
                    existing == season_eligibility_id,
                    Error::<T>::V2SeasonEligibilityAlreadyLinked
                );
                return Ok(());
            }
            SeasonEligibilityByAccountV2::<T>::insert(&account, season_id, season_eligibility_id);
            RegisteredSeasonEligibilityV2::<T>::insert(season_id, season_eligibility_id, true);
            Self::deposit_event(Event::SeasonEligibilityLinkedV2 {
                account,
                season_id,
                season_eligibility_id,
            });
            Ok(())
        }

        #[pallet::call_index(56)]
        #[pallet::weight(<T as Config>::WeightInfo::record_v2_ascension_progress())]
        #[transactional]
        pub fn record_mythical_ascension_progress_v2(
            origin: OriginFor<T>,
            season_eligibility_id: Hash32,
            season_id: u32,
            subject_id: SubjectId,
            economic_realm: EconomicRealm,
            mastery_level: Option<u8>,
            convergence_week: Option<u8>,
            grant_catalyst: bool,
            evidence_id: Hash32,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::ensure_v16_migration_complete()?;
            let config = MythicalAscensionSeasonConfigsV2::<T>::get(season_id)
                .ok_or(Error::<T>::V2AscensionSeasonMissing)?;
            let now = frame_system::Pallet::<T>::block_number();
            ensure!(
                RegisteredSeasonEligibilityV2::<T>::get(season_id, season_eligibility_id),
                Error::<T>::V2SeasonEligibilityMissing
            );
            ensure!(
                economic_realm == EconomicRealm::Production
                    && now >= config.starts_at
                    && now < config.ends_at,
                Error::<T>::V2AscensionNotActive
            );
            ensure!(
                MythicalAscensionSubjectConfigsV2::<T>::contains_key(season_id, subject_id)
                    && (mastery_level.is_some() || convergence_week.is_some() || grant_catalyst)
                    && evidence_id.iter().any(|byte| *byte != 0),
                Error::<T>::V2AscensionSubjectMissing
            );
            let payload_hash = Self::hash_encoded(&(
                season_eligibility_id,
                season_id,
                subject_id,
                economic_realm,
                mastery_level,
                convergence_week,
                grant_catalyst,
            ));
            if let Some(existing) = ProcessedAscensionProgressEvidenceV2::<T>::get(evidence_id) {
                ensure!(
                    existing == payload_hash,
                    Error::<T>::V2AscensionProgressEvidenceConflict
                );
                return Ok(());
            }
            let mastery_key = (season_eligibility_id, season_id, subject_id);
            if let Some(level) = mastery_level {
                let current = MythicalSubjectMasteryV2::<T>::get(mastery_key);
                ensure!(
                    level >= current && level <= config.required_mastery,
                    Error::<T>::V2AscensionMasteryInvalid
                );
                MythicalSubjectMasteryV2::<T>::insert(mastery_key, level);
                if level == config.required_mastery {
                    LegendaryFoundationsV2::<T>::insert(mastery_key, true);
                }
            }
            if let Some(week) = convergence_week {
                let start = config.starts_at.saturated_into::<u128>();
                let current = now.saturated_into::<u128>();
                let week_duration =
                    T::MythicalAscensionWeekDurationBlocks::get().saturated_into::<u128>();
                let expected_week = current
                    .checked_sub(start)
                    .and_then(|elapsed| elapsed.checked_div(week_duration))
                    .ok_or(Error::<T>::V2AscensionWeekInvalid)?;
                ensure!(
                    week < config.available_weeks && week < 16 && expected_week == u128::from(week),
                    Error::<T>::V2AscensionWeekInvalid
                );
                ConvergenceProgressV2::<T>::try_mutate(
                    (season_eligibility_id, season_id),
                    |progress| -> DispatchResult {
                        let mask = 1u16 << week;
                        ensure!(
                            progress.credited_week_bitmap & mask == 0,
                            Error::<T>::V2AscensionWeekAlreadyCredited
                        );
                        progress.credited_week_bitmap |= mask;
                        progress.marks_earned = progress
                            .marks_earned
                            .checked_add(1)
                            .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                        Ok(())
                    },
                )?;
            }
            if grant_catalyst {
                MythicCatalystsV2::<T>::insert((season_eligibility_id, season_id), true);
            }
            ProcessedAscensionProgressEvidenceV2::<T>::insert(evidence_id, payload_hash);
            let mastery_level = MythicalSubjectMasteryV2::<T>::get(mastery_key);
            let marks_earned =
                ConvergenceProgressV2::<T>::get((season_eligibility_id, season_id)).marks_earned;
            let catalyst_available =
                MythicCatalystsV2::<T>::get((season_eligibility_id, season_id));
            Self::deposit_event(Event::MythicalAscensionProgressRecordedV2 {
                season_eligibility_id,
                season_id,
                subject_id,
                economic_realm,
                mastery_level,
                marks_earned,
                catalyst_available,
                evidence_id,
            });
            Ok(())
        }

        #[pallet::call_index(57)]
        #[pallet::weight(<T as Config>::WeightInfo::execute_v2_ascension())]
        #[transactional]
        pub fn ascend_mythical_v2(
            origin: OriginFor<T>,
            season_id: u32,
            subject_id: SubjectId,
            input: MythicalAscensionInput,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&owner)?;
            ensure!(
                V2FeatureEnabled::<T>::get(V2Feature::MythicalAscension),
                Error::<T>::V2FeatureDisabled
            );
            Self::do_mythical_ascension(&owner, season_id, subject_id, input)
        }

        /// Complete V16 only after the copied-state verifier has proven full
        /// map coverage (including keys outside `NextCardId`) and committed its
        /// evidence hash. This explicit root attestation prevents `on_idle`
        /// from unpausing legacy exits based on a bounded cursor tautology.
        #[pallet::call_index(58)]
        #[pallet::weight(<T as Config>::WeightInfo::complete_v16_migration())]
        #[transactional]
        pub fn complete_legacy_migration_v16(
            origin: OriginFor<T>,
            expected_cards_seen: u32,
            expected_anomalies: u32,
            verification_hash: Hash32,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(
                verification_hash.iter().any(|byte| *byte != 0),
                Error::<T>::V16MigrationInvariantFailed
            );
            TcgMigrationStateStorageV16::<T>::try_mutate(|maybe| -> DispatchResult {
                let state = maybe.as_mut().ok_or(Error::<T>::V16MigrationNotRunning)?;
                ensure!(
                    state.phase == MigrationPhaseV16::AwaitingVerification
                        && state.cursor == state.upper_bound
                        && state.cards_seen == expected_cards_seen
                        && state.anomalies == expected_anomalies
                        && NextCardId::<T>::get() == state.upper_bound,
                    Error::<T>::V16MigrationInvariantFailed
                );
                state.phase = MigrationPhaseV16::Completed;
                V16MigrationVerificationHash::<T>::put(verification_hash);
                LegacyWritesPausedV16::<T>::put(false);
                Self::deposit_event(Event::LegacyMigrationCompleted {
                    from_storage_version: state.from_storage_version,
                    cards_seen: state.cards_seen,
                    ordinary: state.ordinary,
                    nft_wrapped: state.nft_wrapped,
                    known_escrow: state.known_escrow,
                    anomalies: state.anomalies,
                    next_card_id: state.upper_bound,
                    max_card_id_seen: state.max_card_id_seen,
                });
                Ok(())
            })
        }

        /// Transfer a wrapped legacy card NFT while atomically maintaining the
        /// V16 beneficial-owner and Nexus indexes. Raw ownership-changing calls
        /// on the dedicated TCG NFT collection are blocked by the runtime.
        #[pallet::call_index(59)]
        #[pallet::weight(<T as Config>::WeightInfo::transfer_wrapped_nft_v16())]
        #[transactional]
        pub fn transfer_wrapped_card_nft_v16(
            origin: OriginFor<T>,
            card_id: u32,
            new_owner: T::AccountId,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&owner)?;
            T::AccessControl::ensure_whitelisted(&new_owner)?;
            Self::ensure_v16_migration_complete()?;

            let collection_id =
                CardNftCollectionId::<T>::get().ok_or(Error::<T>::NftCollectionNotInitialized)?;
            ensure!(
                Converted::<T>::contains_key(card_id),
                Error::<T>::CardNotConverted
            );
            ensure!(
                pallet_nfts::Pallet::<T>::owner(collection_id, card_id).as_ref() == Some(&owner),
                Error::<T>::NotNftOwner
            );
            let classification =
                LegacyCardClassifications::<T>::get(card_id).ok_or(Error::<T>::LegacyCardFrozen)?;
            ensure!(
                !classification.frozen
                    && classification.custody == LegacyCustodyKind::NftWrapped
                    && classification.beneficial_owner.as_ref() == Some(&owner),
                Error::<T>::LegacyCardFrozen
            );

            pallet_nfts::Pallet::<T>::do_transfer(
                collection_id,
                card_id,
                new_owner.clone(),
                |_, _| Ok(()),
            )?;
            Self::transition_v16_beneficial_owner(
                card_id,
                &owner,
                &new_owner,
                LegacyCustodyKind::NftWrapped,
                true,
            )?;
            Self::deposit_event(Event::CardNftTransferredV16 {
                card_id,
                from: owner,
                to: new_owner,
            });
            Ok(())
        }
    }

    // ------------------
    // Pallet Internals
    // ------------------

    impl<T: Config> Pallet<T> {
        pub fn current_nexus_config() -> NexusConfigStateOf<T> {
            NexusConfig::<T>::get().unwrap_or_else(|| NexusConfigState {
                config_version: 1,
                subject_copy_cap: T::NexusSubjectCopyCap::get(),
                overflow_total_capacity: T::NexusOverflowTotalCapacity::get(),
                overflow_per_subject_capacity: T::NexusOverflowPerSubjectCapacity::get(),
                base_vault_capacity: T::NexusBaseVaultCapacity::get(),
                team_size: T::NexusTeamSize::get(),
                updated_at: <frame_system::Pallet<T>>::block_number(),
            })
        }

        fn hash_encoded(value: &impl Encode) -> Hash32 {
            sp_io::hashing::blake2_256(&value.encode())
        }

        fn observed_storage_version() -> u16 {
            let encoded_version = StorageVersion::get::<Pallet<T>>().encode();
            u16::decode(&mut &encoded_version[..]).unwrap_or(u16::MAX)
        }

        fn disable_all_v2_features() {
            for feature in [
                V2Feature::Packs,
                V2Feature::Conversion,
                V2Feature::Ranked,
                V2Feature::MythicalAscension,
            ] {
                V2FeatureEnabled::<T>::remove(feature);
            }
        }

        #[cfg(feature = "try-runtime")]
        fn legacy_migration_evidence(
        ) -> Result<TcgMigrationPreUpgradeEvidenceV16, sp_runtime::TryRuntimeError> {
            let pallet_prefix =
                sp_io::hashing::twox_128(<Pallet<T> as PalletInfoAccess>::name().as_bytes());
            let mut cursor = pallet_prefix.to_vec();
            let mut pallet_storage_entries = Vec::new();
            while let Some(next_key) = sp_io::storage::next_key(&cursor) {
                if !next_key.starts_with(&pallet_prefix) {
                    break;
                }
                let value = sp_io::storage::get(&next_key)
                    .ok_or("TCG V16 evidence key disappeared during pallet scan")?;
                pallet_storage_entries.push(Self::hash_encoded(&(next_key.clone(), value)));
                cursor = next_key;
            }
            pallet_storage_entries.sort_unstable();
            let pallet_storage_entry_count = pallet_storage_entries
                .len()
                .try_into()
                .map_err(|_| "TCG V16 evidence pallet storage count exceeds u32")?;

            let mut cards = Cards::<T>::iter()
                .map(|(card_id, card)| Self::hash_encoded(&(card_id, card)))
                .collect::<Vec<_>>();
            cards.sort_unstable();
            let card_count = cards
                .len()
                .try_into()
                .map_err(|_| "TCG V16 evidence card count exceeds u32")?;

            let mut nexus_cards = NexusCollectionCards::<T>::iter()
                .map(|(card_id, card)| Self::hash_encoded(&(card_id, card)))
                .collect::<Vec<_>>();
            nexus_cards.sort_unstable();
            let nexus_card_count = nexus_cards
                .len()
                .try_into()
                .map_err(|_| "TCG V16 evidence Nexus card count exceeds u32")?;

            let mut converted = Converted::<T>::iter_keys()
                .map(|card_id| Self::hash_encoded(&card_id))
                .collect::<Vec<_>>();
            converted.sort_unstable();
            let converted_count = converted
                .len()
                .try_into()
                .map_err(|_| "TCG V16 evidence converted count exceeds u32")?;

            let mut owner_indexes = CardsByOwner::<T>::iter()
                .map(|(owner, card_ids)| Self::hash_encoded(&(owner, card_ids)))
                .collect::<Vec<_>>();
            owner_indexes.sort_unstable();
            let owner_index_count = owner_indexes
                .len()
                .try_into()
                .map_err(|_| "TCG V16 evidence owner index count exceeds u32")?;

            let mut vault_variants = VaultVariants::<T>::iter()
                .map(|(variant_id, variant)| Self::hash_encoded(&(variant_id, variant)))
                .collect::<Vec<_>>();
            vault_variants.sort_unstable();
            let vault_variant_count = vault_variants
                .len()
                .try_into()
                .map_err(|_| "TCG V16 evidence Vault count exceeds u32")?;

            let mut nexus_subject_indexes = NexusSubjectCopyCounts::<T>::iter()
                .map(|(owner, subject_id, count)| Self::hash_encoded(&(owner, subject_id, count)))
                .collect::<Vec<_>>();
            nexus_subject_indexes.sort_unstable();
            let nexus_subject_index_count = nexus_subject_indexes
                .len()
                .try_into()
                .map_err(|_| "TCG V16 evidence subject-index count exceeds u32")?;

            let mut overflow_owner_indexes = NexusOverflowCards::<T>::iter()
                .map(|(owner, cards)| Self::hash_encoded(&(owner, cards)))
                .collect::<Vec<_>>();
            overflow_owner_indexes.sort_unstable();
            let overflow_owner_index_count = overflow_owner_indexes
                .len()
                .try_into()
                .map_err(|_| "TCG V16 evidence Overflow owner-index count exceeds u32")?;

            let mut overflow_subject_indexes = NexusOverflowSubjectCounts::<T>::iter()
                .map(|(owner, subject_id, count)| Self::hash_encoded(&(owner, subject_id, count)))
                .collect::<Vec<_>>();
            overflow_subject_indexes.sort_unstable();
            let overflow_subject_index_count = overflow_subject_indexes
                .len()
                .try_into()
                .map_err(|_| "TCG V16 evidence Overflow subject-index count exceeds u32")?;

            Ok(TcgMigrationPreUpgradeEvidenceV16 {
                from_storage_version: Self::observed_storage_version(),
                pallet_storage_entry_count,
                pallet_storage_hash: Self::hash_encoded(&(
                    b"ETERRA_TCG_V16_PALLET_STORAGE",
                    pallet_storage_entries,
                )),
                card_count,
                cards_hash: Self::hash_encoded(&(b"ETERRA_TCG_V16_CARDS", cards)),
                nexus_card_count,
                nexus_cards_hash: Self::hash_encoded(&(b"ETERRA_TCG_V16_NEXUS_CARDS", nexus_cards)),
                converted_count,
                converted_hash: Self::hash_encoded(&(b"ETERRA_TCG_V16_CONVERTED", converted)),
                owner_index_count,
                owner_index_hash: Self::hash_encoded(&(
                    b"ETERRA_TCG_V16_OWNER_INDEXES",
                    owner_indexes,
                )),
                vault_variant_count,
                vault_variants_hash: Self::hash_encoded(&(
                    b"ETERRA_TCG_V16_VAULT_VARIANTS",
                    vault_variants,
                )),
                nexus_subject_index_count,
                nexus_subject_indexes_hash: Self::hash_encoded(&(
                    b"ETERRA_TCG_V16_NEXUS_SUBJECT_INDEXES",
                    nexus_subject_indexes,
                )),
                overflow_owner_index_count,
                overflow_owner_indexes_hash: Self::hash_encoded(&(
                    b"ETERRA_TCG_V16_OVERFLOW_OWNER_INDEXES",
                    overflow_owner_indexes,
                )),
                overflow_subject_index_count,
                overflow_subject_indexes_hash: Self::hash_encoded(&(
                    b"ETERRA_TCG_V16_OVERFLOW_SUBJECT_INDEXES",
                    overflow_subject_indexes,
                )),
                next_card_id: NextCardId::<T>::get(),
            })
        }

        #[cfg(feature = "try-runtime")]
        fn ensure_all_v2_features_disabled() -> Result<(), sp_runtime::TryRuntimeError> {
            for feature in [
                V2Feature::Packs,
                V2Feature::Conversion,
                V2Feature::Ranked,
                V2Feature::MythicalAscension,
            ] {
                if V2FeatureEnabled::<T>::get(feature) {
                    return Err("TCG V16 migration cannot run with V2 features enabled".into());
                }
            }
            Ok(())
        }

        #[cfg(feature = "try-runtime")]
        fn validate_v16_migration_state() -> Result<(), sp_runtime::TryRuntimeError> {
            if Self::observed_storage_version() == 16 && !LegacyCreationSealedV16::<T>::get() {
                return Err("TCG V16 state must seal legacy creation".into());
            }
            let Some(state) = TcgMigrationStateStorageV16::<T>::get() else {
                if Self::observed_storage_version() == 16
                    && (LegacyWritesPausedV16::<T>::get()
                        || V16MigrationVerificationHash::<T>::get().is_some()
                        || Cards::<T>::iter().next().is_some()
                        || NexusCollectionCards::<T>::iter().next().is_some()
                        || LegacyCardClassifications::<T>::iter().next().is_some()
                        || TcgMigrationAnomaliesV16::<T>::iter().next().is_some())
                {
                    return Err("TCG fresh V16 state has legacy or migration residue".into());
                }
                return Ok(());
            };
            if !LegacyCreationSealedV16::<T>::get() {
                return Err("TCG V16 migration must seal legacy creation".into());
            }
            if state.cards_seen > state.upper_bound || state.cursor > state.upper_bound {
                return Err("TCG V16 migration counters exceed the migration bound".into());
            }
            if NextCardId::<T>::get() != state.upper_bound
                && state.phase != MigrationPhaseV16::UnsupportedSource
            {
                return Err(
                    "TCG V16 migration NextCardId changed while legacy writes were sealed".into(),
                );
            }

            let classification_count: u32 = LegacyCardClassifications::<T>::iter()
                .count()
                .try_into()
                .map_err(|_| "TCG V16 classification count exceeds u32")?;
            let anomaly_count: u32 = TcgMigrationAnomaliesV16::<T>::iter()
                .count()
                .try_into()
                .map_err(|_| "TCG V16 anomaly count exceeds u32")?;
            if classification_count != state.cards_seen || anomaly_count != state.anomalies {
                return Err("TCG V16 migration classification counts do not reconcile".into());
            }
            let classified_total = state
                .ordinary
                .saturating_add(state.nft_wrapped)
                .saturating_add(state.known_escrow)
                .saturating_add(state.anomalies);
            if classified_total != state.cards_seen {
                return Err("TCG V16 migration custody counts do not reconcile".into());
            }

            for (card_id, classification) in LegacyCardClassifications::<T>::iter() {
                if classification.frozen
                    != (classification.custody == LegacyCustodyKind::UnknownFrozen)
                {
                    return Err("TCG V16 migration frozen classification is inconsistent".into());
                }
                if let Some(owner) = classification.beneficial_owner {
                    if classification.frozen
                        || !RepairedLegacyCardsByOwnerV16::<T>::get(owner, card_id)
                    {
                        return Err(
                            "TCG V16 migration repaired beneficial-owner index is inconsistent"
                                .into(),
                        );
                    }
                } else if !classification.frozen {
                    return Err("TCG V16 non-frozen classification has no beneficial owner".into());
                }
            }

            match state.phase {
                MigrationPhaseV16::Running => {
                    if !matches!(state.from_storage_version, 14 | 15)
                        || Self::observed_storage_version() != 16
                        || !LegacyWritesPausedV16::<T>::get()
                    {
                        return Err("TCG V16 running migration state is not fail-closed".into());
                    }
                    Self::ensure_all_v2_features_disabled()?;
                }
                MigrationPhaseV16::Completed => {
                    if !matches!(state.from_storage_version, 14 | 15)
                        || Self::observed_storage_version() != 16
                        || LegacyWritesPausedV16::<T>::get()
                        || state.cursor != state.upper_bound
                        || V16MigrationVerificationHash::<T>::get().is_none()
                    {
                        return Err("TCG V16 completed migration state is inconsistent".into());
                    }
                    let legacy_card_count: u32 = Cards::<T>::iter()
                        .count()
                        .try_into()
                        .map_err(|_| "TCG V16 legacy card count exceeds u32")?;
                    if legacy_card_count != state.cards_seen {
                        return Err(
                            "TCG V16 completed migration does not cover every legacy card".into(),
                        );
                    }
                }
                MigrationPhaseV16::AwaitingVerification => {
                    if !matches!(state.from_storage_version, 14 | 15)
                        || Self::observed_storage_version() != 16
                        || !LegacyWritesPausedV16::<T>::get()
                        || state.cursor != state.upper_bound
                        || V16MigrationVerificationHash::<T>::get().is_some()
                    {
                        return Err("TCG V16 awaiting-verification state is not fail-closed".into());
                    }
                    Self::ensure_all_v2_features_disabled()?;
                }
                MigrationPhaseV16::UnsupportedSource => {
                    if matches!(state.from_storage_version, 14 | 15)
                        || Self::observed_storage_version() == 16
                        || !LegacyWritesPausedV16::<T>::get()
                    {
                        return Err("TCG V16 unsupported source is not fail-closed".into());
                    }
                    Self::ensure_all_v2_features_disabled()?;
                }
            }
            Ok(())
        }

        pub(crate) fn draw_hash(
            domain: &[u8],
            transcript: &V2DrawTranscript,
            draw_index: u32,
        ) -> Hash32 {
            Self::hash_encoded(&(
                domain,
                T::V2ChainDomain::genesis_hash(),
                transcript.request_id,
                transcript.immutable_config_hash,
                transcript.account_commitment,
                transcript.verified_randomness_output,
                draw_index,
            ))
        }

        pub(crate) fn draw_u32(seed: &Hash32) -> u32 {
            u32::from_le_bytes([seed[0], seed[1], seed[2], seed[3]])
        }

        pub(crate) fn unbiased_index_for_sample(sample: u32, upper: u32) -> Option<usize> {
            if upper == 0 {
                return None;
            }
            let sample_space = u64::from(u32::MAX).saturating_add(1);
            let upper = u64::from(upper);
            let acceptance_limit = sample_space.saturating_sub(sample_space % upper);
            let sample = u64::from(sample);
            (sample < acceptance_limit).then_some((sample % upper) as usize)
        }

        pub(crate) fn rejection_sample_with<F>(
            upper: usize,
            mut sample_at: F,
        ) -> Result<usize, DispatchError>
        where
            F: FnMut(u32) -> Result<u32, DispatchError>,
        {
            ensure!(
                upper > 0 && upper <= u32::MAX as usize,
                Error::<T>::V2RandomSamplingExhausted
            );
            let upper = upper as u32;
            for attempt in 0..V2_MAX_REJECTION_ATTEMPTS {
                if let Some(index) = Self::unbiased_index_for_sample(sample_at(attempt)?, upper) {
                    return Ok(index);
                }
            }
            Err(Error::<T>::V2RandomSamplingExhausted.into())
        }

        pub(crate) fn uniform_index(
            domain: &[u8],
            transcript: &V2DrawTranscript,
            logical_draw_index: u32,
            upper: usize,
        ) -> Result<usize, DispatchError> {
            ensure!(
                upper > 0 && upper <= u32::MAX as usize,
                Error::<T>::V2RandomSamplingExhausted
            );
            let base_index = logical_draw_index
                .checked_mul(V2_MAX_REJECTION_ATTEMPTS)
                .ok_or(Error::<T>::V2ArithmeticOverflow)?;
            Self::rejection_sample_with(upper, |attempt| {
                let draw_index = base_index
                    .checked_add(attempt)
                    .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                let seed = Self::draw_hash(domain, transcript, draw_index);
                Ok(Self::draw_u32(&seed))
            })
        }

        pub(crate) fn conversion_genome_hash(
            request_id: Hash32,
            source_card_snapshot_hash: Hash32,
            reserved_entity_id: u64,
            player_commitment: Hash32,
            verified_randomness_output: Hash32,
            subject_version: u32,
            genome_version: u16,
        ) -> Hash32 {
            Self::hash_encoded(&(
                b"ETERRA_ENTITY_GENOME_V1".as_slice(),
                T::V2ChainDomain::genesis_hash(),
                request_id,
                source_card_snapshot_hash,
                reserved_entity_id,
                player_commitment,
                verified_randomness_output,
                subject_version,
                genome_version,
            ))
        }

        fn ensure_bitmap_capacity(
            bitmap: &mut ProtectionBitmapOf<T>,
            bit_count: usize,
        ) -> DispatchResult {
            let required_bytes = bit_count.saturating_add(7) / 8;
            ensure!(
                required_bytes <= T::MaxV2ProtectionBytes::get() as usize,
                Error::<T>::V2ProtectionBitmapTooSmall
            );
            while bitmap.len() < required_bytes {
                bitmap
                    .try_push(0)
                    .map_err(|_| Error::<T>::V2ProtectionBitmapTooSmall)?;
            }
            Ok(())
        }

        fn bitmap_contains(bitmap: &ProtectionBitmapOf<T>, bit: usize) -> bool {
            let byte = bit / 8;
            let shift = bit % 8;
            bitmap
                .get(byte)
                .map(|value| value & (1u8 << shift) != 0)
                .unwrap_or(false)
        }

        fn bitmap_insert(bitmap: &mut ProtectionBitmapOf<T>, bit: usize) -> DispatchResult {
            Self::ensure_bitmap_capacity(bitmap, bit.saturating_add(1))?;
            let byte = bit / 8;
            let shift = bit % 8;
            if let Some(value) = bitmap.get_mut(byte) {
                *value |= 1u8 << shift;
            }
            Ok(())
        }

        fn register_v2_protection_layout(
            set_id: u32,
            profiles: &PoolProfileEntriesOf<T>,
            poses: &PoolPoseIdsOf<T>,
            backgrounds: &PoolBackgroundIdsOf<T>,
        ) -> DispatchResult {
            let mut coverage =
                sp_std::collections::btree_map::BTreeMap::<SubjectId, [bool; 5]>::new();
            for entry in profiles {
                let profile = SubjectRarityProfilesV2::<T>::get(entry.profile_id)
                    .ok_or(Error::<T>::V2InvalidPool)?;
                let row = coverage.entry(profile.subject_id).or_insert([false; 5]);
                ensure!(!row[profile.rarity.index()], Error::<T>::V2InvalidPool);
                row[profile.rarity.index()] = true;

                if !SubjectProtectionSlotsV2::<T>::contains_key(set_id, profile.subject_id) {
                    let slot = NextSubjectProtectionSlotV2::<T>::get(set_id);
                    let required_bits = usize::from(slot)
                        .saturating_add(1)
                        .saturating_mul(V2_COSMETIC_PROTECTION_SLOTS_PER_SUBJECT);
                    ensure!(
                        required_bits
                            <= (T::MaxV2ProtectionBytes::get() as usize).saturating_mul(8),
                        Error::<T>::V2ProtectionBitmapTooSmall
                    );
                    SubjectProtectionSlotsV2::<T>::insert(set_id, profile.subject_id, slot);
                    NextSubjectProtectionSlotV2::<T>::insert(
                        set_id,
                        slot.checked_add(1)
                            .ok_or(Error::<T>::V2ArithmeticOverflow)?,
                    );
                }
            }
            ensure!(
                !coverage.is_empty() && coverage.values().all(|row| row.iter().all(|seen| *seen)),
                Error::<T>::V2InvalidPool
            );

            let mut pose_counts = sp_std::collections::btree_map::BTreeMap::<SubjectId, u8>::new();
            for subject_id in coverage.keys().copied() {
                pose_counts.insert(subject_id, 0);
            }
            for definition_id in poses {
                let pose =
                    PoseDefinitionsV2::<T>::get(definition_id).ok_or(Error::<T>::V2InvalidPool)?;
                let subject_id = pose.subject_id.ok_or(Error::<T>::V2InvalidPool)?;
                ensure!(
                    coverage.contains_key(&subject_id),
                    Error::<T>::V2InvalidPool
                );
                let count = pose_counts
                    .get_mut(&subject_id)
                    .ok_or(Error::<T>::V2InvalidPool)?;
                *count = count
                    .checked_add(1)
                    .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                ensure!(
                    *count <= V2_MAX_POSE_SLOTS_PER_SUBJECT,
                    Error::<T>::V2InvalidPool
                );
                if !PoseProtectionSlotsV2::<T>::contains_key(set_id, definition_id) {
                    let key = (set_id, subject_id);
                    let slot = NextPoseProtectionSlotV2::<T>::get(key);
                    ensure!(
                        slot < V2_MAX_POSE_SLOTS_PER_SUBJECT,
                        Error::<T>::V2InvalidPool
                    );
                    PoseProtectionSlotsV2::<T>::insert(set_id, definition_id, slot);
                    NextPoseProtectionSlotV2::<T>::insert(
                        key,
                        slot.checked_add(1)
                            .ok_or(Error::<T>::V2ArithmeticOverflow)?,
                    );
                }
            }
            ensure!(
                pose_counts
                    .values()
                    .all(|count| *count == V2_MAX_POSE_SLOTS_PER_SUBJECT),
                Error::<T>::V2InvalidPool
            );

            ensure!(
                backgrounds.len() == usize::from(V2_MAX_BACKGROUND_SLOTS_PER_SET),
                Error::<T>::V2InvalidPool
            );
            let mut globally_eligible_background = false;
            let mut subject_eligible_backgrounds =
                sp_std::collections::btree_set::BTreeSet::<SubjectId>::new();
            for definition_id in backgrounds {
                let background = BackgroundDefinitionsV2::<T>::get(definition_id)
                    .ok_or(Error::<T>::V2InvalidPool)?;
                if let Some(subject_id) = background.subject_id {
                    subject_eligible_backgrounds.insert(subject_id);
                } else {
                    globally_eligible_background = true;
                }
                if !BackgroundProtectionSlotsV2::<T>::contains_key(set_id, definition_id) {
                    let slot = NextBackgroundProtectionSlotV2::<T>::get(set_id);
                    ensure!(
                        slot < V2_MAX_BACKGROUND_SLOTS_PER_SET,
                        Error::<T>::V2InvalidPool
                    );
                    BackgroundProtectionSlotsV2::<T>::insert(set_id, definition_id, slot);
                    NextBackgroundProtectionSlotV2::<T>::insert(
                        set_id,
                        slot.checked_add(1)
                            .ok_or(Error::<T>::V2ArithmeticOverflow)?,
                    );
                }
            }
            ensure!(
                globally_eligible_background
                    || coverage
                        .keys()
                        .all(|subject_id| subject_eligible_backgrounds.contains(subject_id)),
                Error::<T>::V2InvalidPool
            );
            Ok(())
        }

        pub(crate) fn profile_protection_bit(
            set_id: u32,
            profile: &SubjectRarityProfile,
        ) -> Result<usize, DispatchError> {
            let subject_slot = SubjectProtectionSlotsV2::<T>::get(set_id, profile.subject_id)
                .ok_or(Error::<T>::V2InvalidPool)?;
            Ok(usize::from(subject_slot)
                .saturating_mul(V2_RARITY_PROTECTION_SLOTS)
                .saturating_add(profile.rarity.index()))
        }

        pub(crate) fn cosmetic_protection_bit(
            set_id: u32,
            subject_id: SubjectId,
            rarity: CardRarity,
            pose_definition_id: u32,
            background_definition_id: u32,
        ) -> Result<usize, DispatchError> {
            let subject_slot = SubjectProtectionSlotsV2::<T>::get(set_id, subject_id)
                .ok_or(Error::<T>::V2InvalidPool)?;
            let pose_slot = PoseProtectionSlotsV2::<T>::get(set_id, pose_definition_id)
                .ok_or(Error::<T>::V2InvalidPool)?;
            let background_slot =
                BackgroundProtectionSlotsV2::<T>::get(set_id, background_definition_id)
                    .ok_or(Error::<T>::V2InvalidPool)?;
            Ok(usize::from(subject_slot)
                .saturating_mul(V2_COSMETIC_PROTECTION_SLOTS_PER_SUBJECT)
                .saturating_add(
                    rarity
                        .index()
                        .saturating_mul(V2_COSMETIC_PROTECTION_SLOTS_PER_RARITY),
                )
                .saturating_add(
                    usize::from(pose_slot).saturating_mul(V2_MAX_BACKGROUND_SLOTS_PER_SET as usize),
                )
                .saturating_add(usize::from(background_slot)))
        }

        fn ensure_pool_active_for_new_request(
            pool: &AcquisitionPoolVersionOf<T>,
        ) -> DispatchResult {
            let mut checked_subjects =
                sp_std::collections::btree_set::BTreeSet::<(SubjectId, u32)>::new();
            for entry in &pool.profiles {
                let profile = SubjectRarityProfilesV2::<T>::get(entry.profile_id)
                    .ok_or(Error::<T>::V2NoEligibleProfile)?;
                if !checked_subjects.insert((profile.subject_id, profile.subject_version)) {
                    continue;
                }
                let definition_id = SubjectDefinitionByKeyV2::<T>::get((
                    profile.subject_id,
                    profile.subject_version,
                ))
                .ok_or(Error::<T>::V2NoEligibleProfile)?;
                let activation = SubjectActivationStatesV2::<T>::get(definition_id)
                    .ok_or(Error::<T>::V2NoEligibleProfile)?;
                ensure!(activation.mint_enabled, Error::<T>::V2NoEligibleProfile);
            }
            ensure!(
                !checked_subjects.is_empty(),
                Error::<T>::V2NoEligibleProfile
            );
            Ok(())
        }

        fn subject_seen_in_bitmap(
            set_id: u32,
            bitmap: &ProtectionBitmapOf<T>,
            subject_id: SubjectId,
            rarity: Option<CardRarity>,
        ) -> bool {
            let Some(subject_slot) = SubjectProtectionSlotsV2::<T>::get(set_id, subject_id) else {
                return false;
            };
            let start = usize::from(subject_slot).saturating_mul(V2_RARITY_PROTECTION_SLOTS);
            match rarity {
                Some(value) => Self::bitmap_contains(bitmap, start.saturating_add(value.index())),
                None => (0..V2_RARITY_PROTECTION_SLOTS)
                    .any(|offset| Self::bitmap_contains(bitmap, start.saturating_add(offset))),
            }
        }

        fn roll_rarity(weights: [u32; 5], roll: u32) -> CardRarity {
            let mut cumulative = 0u32;
            for (index, weight) in weights.iter().copied().enumerate() {
                cumulative = cumulative.saturating_add(weight);
                if roll < cumulative {
                    return match index {
                        0 => CardRarity::Common,
                        1 => CardRarity::Rare,
                        2 => CardRarity::Epic,
                        3 => CardRarity::Legendary,
                        _ => CardRarity::Mythical,
                    };
                }
            }
            CardRarity::Mythical
        }

        pub(crate) fn select_profile_for_pack(
            pool: &AcquisitionPoolVersionOf<T>,
            set_id: u32,
            rarity: CardRarity,
            transcript: &V2DrawTranscript,
            logical_draw_index: u32,
            used_profile_ids: &[u32],
            protection: &ProtectionBitmapOf<T>,
            subject_discovery_slot: bool,
        ) -> Result<(usize, SubjectRarityProfile), DispatchError> {
            let mut candidates: Vec<SubjectRarityProfile> = pool
                .profiles
                .iter()
                .filter_map(|entry| {
                    SubjectRarityProfilesV2::<T>::get(entry.profile_id)
                        .filter(|profile| profile.rarity == rarity)
                })
                .collect();
            candidates.sort_by_key(|profile| profile.profile_id);
            ensure!(!candidates.is_empty(), Error::<T>::V2NoEligibleProfile);
            let has_unused = candidates
                .iter()
                .any(|profile| !used_profile_ids.contains(&profile.profile_id));
            let eligible: Vec<SubjectRarityProfile> = candidates
                .into_iter()
                .filter(|profile| !has_unused || !used_profile_ids.contains(&profile.profile_id))
                .collect();
            ensure!(!eligible.is_empty(), Error::<T>::V2NoEligibleProfile);

            if matches!(rarity, CardRarity::Legendary | CardRarity::Mythical)
                || subject_discovery_slot
            {
                let novel: Vec<SubjectRarityProfile> = eligible
                    .iter()
                    .copied()
                    .filter(|profile| {
                        if matches!(rarity, CardRarity::Legendary | CardRarity::Mythical) {
                            !Self::subject_seen_in_bitmap(
                                set_id,
                                protection,
                                profile.subject_id,
                                Some(rarity),
                            )
                        } else {
                            !Self::subject_seen_in_bitmap(
                                set_id,
                                protection,
                                profile.subject_id,
                                None,
                            )
                        }
                    })
                    .collect();
                if !novel.is_empty() {
                    let selected = novel[Self::uniform_index(
                        b"ETERRA_PACK_SUBJECT_V3",
                        transcript,
                        logical_draw_index,
                        novel.len(),
                    )?];
                    return Ok((Self::profile_protection_bit(set_id, &selected)?, selected));
                }
            }

            let initial = eligible[Self::uniform_index(
                b"ETERRA_PACK_SUBJECT_V3",
                transcript,
                logical_draw_index,
                eligible.len(),
            )?];
            let initial_bit = Self::profile_protection_bit(set_id, &initial)?;
            if matches!(
                rarity,
                CardRarity::Common | CardRarity::Rare | CardRarity::Epic
            ) && !subject_discovery_slot
                && Self::bitmap_contains(protection, initial_bit)
            {
                let rerolled = eligible[Self::uniform_index(
                    b"ETERRA_PACK_SUBJECT_REROLL_V3",
                    transcript,
                    logical_draw_index,
                    eligible.len(),
                )?];
                return Ok((Self::profile_protection_bit(set_id, &rerolled)?, rerolled));
            }
            Ok((initial_bit, initial))
        }

        fn tutorial_conversion_profiles(
            pool: &AcquisitionPoolVersionOf<T>,
        ) -> Vec<SubjectRarityProfile> {
            let mut profiles = Vec::new();
            for entry in &pool.profiles {
                let Some(profile) = SubjectRarityProfilesV2::<T>::get(entry.profile_id) else {
                    continue;
                };
                if profile.rarity != CardRarity::Common {
                    continue;
                }
                let Some(definition_id) = SubjectDefinitionByKeyV2::<T>::get((
                    profile.subject_id,
                    profile.subject_version,
                )) else {
                    continue;
                };
                let Some(definition) = SubjectDefinitionsV2::<T>::get(definition_id) else {
                    continue;
                };
                if definition.subject_id != profile.subject_id
                    || definition.subject_version != profile.subject_version
                    || !definition.conversion_policy.permits_conversion()
                {
                    continue;
                }
                profiles.push(profile);
            }
            profiles.sort_by_key(|profile| profile.profile_id);
            profiles
        }

        fn ensure_tutorial_conversion_pool_ready(
            pool: &AcquisitionPoolVersionOf<T>,
        ) -> Result<TutorialConversionProfileIdsOf<T>, DispatchError> {
            let profiles = Self::tutorial_conversion_profiles(pool);
            ensure!(
                !profiles.is_empty(),
                Error::<T>::V2TutorialConversionCardUnavailable
            );
            let mut eligible = TutorialConversionProfileIdsOf::<T>::default();
            for profile in profiles {
                let definition_id = SubjectDefinitionByKeyV2::<T>::get((
                    profile.subject_id,
                    profile.subject_version,
                ))
                .ok_or(Error::<T>::V2TutorialConversionCardUnavailable)?;
                let activation = SubjectActivationStatesV2::<T>::get(definition_id)
                    .ok_or(Error::<T>::V2TutorialConversionCardUnavailable)?;
                if activation.conversion_enabled
                    && T::V2Entities::ensure_conversion_profile_active(
                        profile.subject_id,
                        profile.subject_version,
                        profile.rarity,
                    )
                    .is_ok()
                {
                    eligible
                        .try_push(profile.profile_id)
                        .map_err(|_| Error::<T>::V2InvalidPool)?;
                }
            }
            ensure!(
                !eligible.is_empty(),
                Error::<T>::V2TutorialConversionCardUnavailable
            );
            Ok(eligible)
        }

        fn select_tutorial_conversion_profile(
            pool: &AcquisitionPoolVersionOf<T>,
            set_id: u32,
            transcript: &V2DrawTranscript,
            logical_draw_index: u32,
            used_profile_ids: &[u32],
            eligible_profile_ids: &[u32],
        ) -> Result<(usize, SubjectRarityProfile), DispatchError> {
            let candidates: Vec<SubjectRarityProfile> = Self::tutorial_conversion_profiles(pool)
                .into_iter()
                .filter(|profile| eligible_profile_ids.contains(&profile.profile_id))
                .collect();
            ensure!(
                !candidates.is_empty(),
                Error::<T>::V2TutorialConversionCardUnavailable
            );
            let has_unused = candidates
                .iter()
                .any(|profile| !used_profile_ids.contains(&profile.profile_id));
            let eligible: Vec<SubjectRarityProfile> = candidates
                .into_iter()
                .filter(|profile| !has_unused || !used_profile_ids.contains(&profile.profile_id))
                .collect();
            let selected = eligible[Self::uniform_index(
                b"ETERRA_TUTORIAL_CONVERSION_SUBJECT_V3",
                transcript,
                logical_draw_index,
                eligible.len(),
            )?];
            Ok((Self::profile_protection_bit(set_id, &selected)?, selected))
        }

        fn select_cosmetics(
            pool: &AcquisitionPoolVersionOf<T>,
            set_id: u32,
            subject_id: SubjectId,
            rarity: CardRarity,
            transcript: &V2DrawTranscript,
            logical_draw_index: u32,
            protection: &ProtectionBitmapOf<T>,
            novelty_slot: bool,
        ) -> Result<(usize, MediaDefinitionV2, MediaDefinitionV2), DispatchError> {
            let mut eligible_poses: Vec<MediaDefinitionV2> = pool
                .poses
                .iter()
                .filter_map(|definition_id| {
                    PoseDefinitionsV2::<T>::get(definition_id)
                        .filter(|definition| definition.subject_id == Some(subject_id))
                })
                .collect();
            let mut eligible_backgrounds: Vec<MediaDefinitionV2> = pool
                .backgrounds
                .iter()
                .filter_map(|definition_id| {
                    BackgroundDefinitionsV2::<T>::get(definition_id).filter(|definition| {
                        definition.subject_id.is_none() || definition.subject_id == Some(subject_id)
                    })
                })
                .collect();
            eligible_poses.sort_by_key(|definition| definition.definition_id);
            eligible_backgrounds.sort_by_key(|definition| definition.definition_id);
            ensure!(!eligible_poses.is_empty(), Error::<T>::V2NoEligiblePose);
            ensure!(
                !eligible_backgrounds.is_empty(),
                Error::<T>::V2NoEligibleBackground
            );

            if novelty_slot {
                let mut novel_pairs = Vec::new();
                for pose in eligible_poses.iter().copied() {
                    for background in eligible_backgrounds.iter().copied() {
                        let bit = Self::cosmetic_protection_bit(
                            set_id,
                            subject_id,
                            rarity,
                            pose.definition_id,
                            background.definition_id,
                        )?;
                        if !Self::bitmap_contains(protection, bit) {
                            novel_pairs.push((bit, pose, background));
                        }
                    }
                }
                if !novel_pairs.is_empty() {
                    return Ok(novel_pairs[Self::uniform_index(
                        b"ETERRA_PACK_COSMETIC_DISCOVERY_V3",
                        transcript,
                        logical_draw_index,
                        novel_pairs.len(),
                    )?]);
                }
            }

            let pose = eligible_poses[Self::uniform_index(
                b"ETERRA_PACK_POSE_V3",
                transcript,
                logical_draw_index,
                eligible_poses.len(),
            )?];
            let background = eligible_backgrounds[Self::uniform_index(
                b"ETERRA_PACK_BACKGROUND_V3",
                transcript,
                logical_draw_index,
                eligible_backgrounds.len(),
            )?];
            Ok((
                Self::cosmetic_protection_bit(
                    set_id,
                    subject_id,
                    rarity,
                    pose.definition_id,
                    background.definition_id,
                )?,
                pose,
                background,
            ))
        }

        fn do_issue_credit(
            owner: &T::AccountId,
            pack_sku: u32,
            sku_version: u32,
            economic_realm: EconomicRealm,
            source: PackCreditSource,
        ) -> Result<u64, DispatchError> {
            ensure!(
                economic_realm == EconomicRealm::Training
                    && !matches!(source, PackCreditSource::PaidPurchase { .. }),
                Error::<T>::V2ProductionAlphaIssuanceDisabled
            );
            ensure!(
                PackSkuVersionsV2::<T>::contains_key((pack_sku, sku_version)),
                Error::<T>::V2PackSkuMissing
            );
            let key = (pack_sku, sku_version, economic_realm);
            let outstanding = OutstandingPackCreditCountV2::<T>::get(owner, key);
            ensure!(
                outstanding < T::MaxV2CreditsPerAccountSku::get(),
                Error::<T>::V2CreditQueueFull
            );
            let raw_id = NextPackCreditIdV2::<T>::get();
            let credit_id = raw_id.max(1);
            let next = credit_id
                .checked_add(1)
                .ok_or(Error::<T>::V2CreditIdExhausted)?;
            let credit = PackCredit {
                credit_id,
                owner: owner.clone(),
                pack_sku,
                sku_version,
                economic_realm,
                source,
                amount: 1,
            };
            AvailablePackCreditIdsV2::<T>::try_mutate(owner, key, |queue| -> DispatchResult {
                queue
                    .try_push(credit_id)
                    .map_err(|_| Error::<T>::V2CreditQueueFull)?;
                Ok(())
            })?;
            NextPackCreditIdV2::<T>::put(next);
            PackCreditsV2::<T>::insert(credit_id, credit.clone());
            OutstandingPackCreditCountV2::<T>::insert(
                owner,
                key,
                outstanding
                    .checked_add(1)
                    .ok_or(Error::<T>::V2ArithmeticOverflow)?,
            );
            Self::deposit_event(Event::PackCreditIssuedV2 {
                owner: owner.clone(),
                credit_id,
                pack_sku,
                sku_version,
                economic_realm,
                source,
            });
            Ok(credit_id)
        }

        fn operational_card_thresholds() -> Result<(u64, u64), DispatchError> {
            let warning = T::V2OperationalCardWarningThreshold::get();
            let limit = T::V2OperationalCardLimit::get();
            ensure!(
                warning > 0 && warning < limit,
                Error::<T>::V2OperationalCardLimitInvalid
            );
            Ok((warning, limit))
        }

        fn ensure_operational_card_capacity(
            owner: &T::AccountId,
            incoming_cards: u64,
        ) -> DispatchResult {
            let (_, limit) = Self::operational_card_thresholds()?;
            let committed = V2OwnerCardCount::<T>::get(owner)
                .checked_add(ReservedV2PackCardCount::<T>::get(owner))
                .and_then(|count| count.checked_add(incoming_cards))
                .ok_or(Error::<T>::V2ArithmeticOverflow)?;
            ensure!(
                committed <= limit,
                Error::<T>::V2OperationalCardLimitReached
            );
            Ok(())
        }

        fn reserve_pack_card_capacity(owner: &T::AccountId, card_count: u8) -> DispatchResult {
            ReservedV2PackCardCount::<T>::try_mutate(owner, |reserved| -> DispatchResult {
                *reserved = reserved
                    .checked_add(u64::from(card_count))
                    .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                Ok(())
            })
        }

        fn release_pack_card_capacity(owner: &T::AccountId, card_count: u8) -> DispatchResult {
            ReservedV2PackCardCount::<T>::try_mutate_exists(
                owner,
                |maybe_reserved| -> DispatchResult {
                    let next = maybe_reserved
                        .unwrap_or(0)
                        .checked_sub(u64::from(card_count))
                        .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                    *maybe_reserved = (next != 0).then_some(next);
                    Ok(())
                },
            )
        }

        fn do_request_pack_open(
            owner: &T::AccountId,
            pack_sku: u32,
            sku_version: u32,
            economic_realm: EconomicRealm,
            commitment: Hash32,
        ) -> DispatchResult {
            let sku = PackSkuVersionsV2::<T>::get((pack_sku, sku_version))
                .ok_or(Error::<T>::V2PackSkuMissing)?;
            let now = frame_system::Pallet::<T>::block_number();
            ensure!(
                now >= sku.active_from && sku.active_until.map(|until| now < until).unwrap_or(true),
                Error::<T>::V2PackSkuInactive
            );
            let pool = AcquisitionPoolVersionsV2::<T>::get((sku.pool_id, sku.pool_version))
                .ok_or(Error::<T>::V2PoolMissing)?;
            ensure!(
                pool.set_id == sku.set_id
                    && pool.immutable_config_hash == sku.immutable_config_hash,
                Error::<T>::V2DefinitionMismatch
            );
            Self::ensure_pool_active_for_new_request(&pool)?;
            Self::ensure_operational_card_capacity(owner, u64::from(sku.card_count))?;
            let key = (pack_sku, sku_version, economic_realm);
            let credit_id = AvailablePackCreditIdsV2::<T>::try_mutate(
                owner,
                key,
                |queue| -> Result<u64, DispatchError> {
                    ensure!(!queue.is_empty(), Error::<T>::V2PackCreditMissing);
                    Ok(queue.remove(0))
                },
            )?;
            let credit =
                PackCreditsV2::<T>::take(credit_id).ok_or(Error::<T>::V2PackCreditMissing)?;
            ensure!(
                credit.owner == *owner,
                Error::<T>::V2PackCreditOwnerMismatch
            );
            ensure!(
                credit.economic_realm == economic_realm,
                Error::<T>::V2PackCreditRealmMismatch
            );
            let tutorial_profile_ids =
                if matches!(&credit.source, PackCreditSource::TutorialTraining { .. }) {
                    ensure!(
                        economic_realm == EconomicRealm::Training,
                        Error::<T>::V2PackCreditRealmMismatch
                    );
                    Some(Self::ensure_tutorial_conversion_pool_ready(&pool)?)
                } else {
                    None
                };
            let opening_id = Self::hash_encoded(&(
                b"ETERRA_PACK_OPENING_V2",
                owner,
                credit_id,
                commitment,
                sku.immutable_config_hash,
            ));
            ensure!(
                !PendingPackOpeningsV2::<T>::contains_key(opening_id)
                    && !ProcessedAcquisitionsV2::<T>::contains_key(opening_id),
                Error::<T>::V2PackOpeningAlreadyExists
            );
            let domain = Self::hash_encoded(&b"ETERRA_PACK_RANDOMNESS_V2");
            let expected_randomness_provenance = match economic_realm {
                EconomicRealm::Training => T::V2Randomness::current_mode(),
                EconomicRealm::Production => RandomnessMode::DrandQuicknet,
            };
            let randomness_request_id = T::V2Randomness::request_for(
                economic_realm,
                expected_randomness_provenance,
                domain,
                commitment,
                sku.immutable_config_hash,
                0,
            )?;
            LockedPackCreditsV2::<T>::insert(opening_id, credit);
            if let Some(profile_ids) = tutorial_profile_ids {
                TutorialConversionProfileIdsV2::<T>::insert(opening_id, profile_ids);
            }
            PendingPackOpeningsV2::<T>::insert(
                opening_id,
                PendingPackOpening {
                    opening_id,
                    owner: owner.clone(),
                    credit_id,
                    pack_sku,
                    sku_version,
                    economic_realm,
                    randomness_request_id,
                    commitment,
                    immutable_config_hash: sku.immutable_config_hash,
                    requested_at: now,
                    expected_randomness_provenance,
                },
            );
            Self::reserve_pack_card_capacity(owner, sku.card_count)?;
            PackOpeningRequestReceiptsV2::<T>::insert(
                owner,
                commitment,
                PackOpeningRequestReceipt {
                    opening_id,
                    pack_sku,
                    sku_version,
                    economic_realm,
                },
            );
            Self::deposit_event(Event::PackOpenRequestedV2 {
                owner: owner.clone(),
                opening_id,
                credit_id,
                randomness_request_id,
                immutable_config_hash: sku.immutable_config_hash,
            });
            Ok(())
        }

        fn existing_pack_opening_request(
            owner: &T::AccountId,
            pack_sku: u32,
            sku_version: u32,
            economic_realm: EconomicRealm,
            commitment: Hash32,
        ) -> Result<Option<Hash32>, DispatchError> {
            let Some(receipt) = PackOpeningRequestReceiptsV2::<T>::get(owner, commitment) else {
                return Ok(None);
            };
            ensure!(
                receipt.pack_sku == pack_sku
                    && receipt.sku_version == sku_version
                    && receipt.economic_realm == economic_realm,
                Error::<T>::V2PackOpeningRequestConflict
            );
            Ok(Some(receipt.opening_id))
        }

        fn mint_v2_card(
            opening: &PendingPackOpeningOf<T>,
            sku: &PackSkuVersionOf<T>,
            pool: &AcquisitionPoolVersionOf<T>,
            slot: u8,
            profile: SubjectRarityProfile,
            pose: MediaDefinitionV2,
            background: MediaDefinitionV2,
        ) -> Result<CardIdV2, DispatchError> {
            let raw_id = NextCardIdV2::<T>::get();
            let card_id = raw_id.max(1);
            let next = card_id
                .checked_add(1)
                .ok_or(Error::<T>::V2CardIdExhausted)?;
            let serial_raw = NextSerialV2::<T>::get((profile.subject_id, profile.rarity));
            let serial_number = serial_raw
                .checked_add(1)
                .ok_or(Error::<T>::V2ArithmeticOverflow)?;
            let acquisition_id = Self::hash_encoded(&(
                opening.opening_id,
                slot,
                profile.profile_id,
                pose.definition_id,
                background.definition_id,
            ));
            let card = CardInstanceV2 {
                card_id,
                owner: opening.owner.clone(),
                set_id: sku.set_id,
                season_id: sku.set_id,
                subject_id: profile.subject_id,
                subject_version: profile.subject_version,
                rarity: profile.rarity,
                profile_id: profile.profile_id,
                pose_definition_id: pose.definition_id,
                background_definition_id: background.definition_id,
                serial_number,
                economic_realm: opening.economic_realm,
                origin: CardOriginV2::Pack {
                    opening_id: opening.opening_id,
                    slot,
                },
                acquisition_id,
                pool_id: pool.pool_id,
                pool_version: pool.version,
                state: CardStateV2::Active,
                acquired_at: frame_system::Pallet::<T>::block_number(),
            };
            CardsV2::<T>::insert(card_id, card);
            NextCardIdV2::<T>::put(next);
            NextSerialV2::<T>::insert((profile.subject_id, profile.rarity), serial_number);
            let mut crossed_operational_warning = None;
            V2OwnerCardCount::<T>::try_mutate(&opening.owner, |count| -> DispatchResult {
                let previous = *count;
                *count = count
                    .checked_add(1)
                    .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                if previous < T::V2OperationalCardWarningThreshold::get()
                    && *count >= T::V2OperationalCardWarningThreshold::get()
                {
                    crossed_operational_warning = Some(*count);
                }
                Ok(())
            })?;
            if let Some(lifetime_card_count) = crossed_operational_warning {
                Self::deposit_event(Event::V2OwnerCardOperationalWarning {
                    owner: opening.owner.clone(),
                    lifetime_card_count,
                    unopened_limit: T::V2OperationalCardLimit::get(),
                });
            }
            V2OwnerActiveCardCount::<T>::try_mutate(&opening.owner, |count| -> DispatchResult {
                *count = count
                    .checked_add(1)
                    .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                Ok(())
            })?;
            LiveSupplyBySubjectRarityV2::<T>::try_mutate(
                (profile.subject_id, profile.rarity, opening.economic_realm),
                |count| -> DispatchResult {
                    *count = count
                        .checked_add(1)
                        .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            Self::deposit_event(Event::CardAcquiredV2 {
                owner: opening.owner.clone(),
                card_id,
                subject_id: profile.subject_id,
                subject_version: profile.subject_version,
                rarity: profile.rarity,
                profile_id: profile.profile_id,
                pose_definition_id: pose.definition_id,
                background_definition_id: background.definition_id,
                resolved_ranks: profile.base_ranks,
                economic_realm: opening.economic_realm,
            });
            Ok(card_id)
        }

        fn do_finalize_pack_open(opening_id: Hash32) -> DispatchResult {
            if ProcessedAcquisitionsV2::<T>::contains_key(opening_id) {
                return Ok(());
            }
            ensure!(
                !TimedOutPackOpeningsV2::<T>::contains_key(opening_id),
                Error::<T>::V2PackOpeningTerminalConflict
            );
            let opening = PendingPackOpeningsV2::<T>::get(opening_id)
                .ok_or(Error::<T>::V2PackOpeningMissing)?;
            let sku = PackSkuVersionsV2::<T>::get((opening.pack_sku, opening.sku_version))
                .ok_or(Error::<T>::V2PackSkuMissing)?;
            ensure!(
                sku.immutable_config_hash == opening.immutable_config_hash,
                Error::<T>::V2DefinitionMismatch
            );
            Self::ensure_operational_card_capacity(&opening.owner, 0)?;
            ensure!(
                ReservedV2PackCardCount::<T>::get(&opening.owner) >= u64::from(sku.card_count),
                Error::<T>::V2ArithmeticOverflow
            );
            let randomness_output = T::V2Randomness::output_for(
                opening.randomness_request_id,
                opening.economic_realm,
                opening.expected_randomness_provenance,
            )
            .ok_or(Error::<T>::V2PackOpeningNotReady)?
            .output;
            let pool = AcquisitionPoolVersionsV2::<T>::get((sku.pool_id, sku.pool_version))
                .ok_or(Error::<T>::V2PoolMissing)?;
            ensure!(
                pool.immutable_config_hash == sku.immutable_config_hash,
                Error::<T>::V2DefinitionMismatch
            );
            let locked_credit =
                LockedPackCreditsV2::<T>::get(opening_id).ok_or(Error::<T>::V2PackCreditMissing)?;
            ensure!(
                locked_credit.owner == opening.owner,
                Error::<T>::V2PackCreditOwnerMismatch
            );
            ensure!(
                locked_credit.credit_id == opening.credit_id
                    && locked_credit.pack_sku == opening.pack_sku
                    && locked_credit.sku_version == opening.sku_version,
                Error::<T>::V2DefinitionMismatch
            );
            ensure!(
                locked_credit.economic_realm == opening.economic_realm,
                Error::<T>::V2PackCreditRealmMismatch
            );
            let tutorial_credit = matches!(
                &locked_credit.source,
                PackCreditSource::TutorialTraining { .. }
            );
            let tutorial_profile_ids = if tutorial_credit {
                Some(
                    TutorialConversionProfileIdsV2::<T>::get(opening_id)
                        .ok_or(Error::<T>::V2TutorialConversionCardUnavailable)?,
                )
            } else {
                None
            };
            let transcript = V2DrawTranscript {
                request_id: opening_id,
                immutable_config_hash: opening.immutable_config_hash,
                account_commitment: opening.commitment,
                verified_randomness_output: randomness_output,
            };
            let mut profile_protection =
                PackProtectionHistoryBitmapsV2::<T>::get(&opening.owner, sku.set_id);
            let subject_slots = usize::from(NextSubjectProtectionSlotV2::<T>::get(sku.set_id));
            Self::ensure_bitmap_capacity(
                &mut profile_protection,
                subject_slots.saturating_mul(V2_RARITY_PROTECTION_SLOTS),
            )?;
            let cosmetic_bits =
                subject_slots.saturating_mul(V2_COSMETIC_PROTECTION_SLOTS_PER_SUBJECT);
            let mut cosmetic_protection =
                CosmeticProtectionBitmapsV2::<T>::get(&opening.owner, sku.set_id);
            Self::ensure_bitmap_capacity(&mut cosmetic_protection, cosmetic_bits)?;
            let discovery_slots = match sku.discovery_policy {
                DiscoveryPolicy::Standard => 1usize,
                DiscoveryPolicy::Earned => 2usize,
                DiscoveryPolicy::PremiumCosmetic => 1usize,
            };
            let mut used_profiles = Vec::<u32>::new();
            let mut card_ids = BoundedVec::<CardIdV2, ConstU32<6>>::default();
            for slot in 0..sku.card_count {
                let logical_draw_index = u32::from(slot);
                let subject_discovery_slot = usize::from(slot) < discovery_slots;
                let (profile_position, profile) =
                    if tutorial_credit && slot == V2_TUTORIAL_CONVERSION_SLOT {
                        Self::select_tutorial_conversion_profile(
                            &pool,
                            sku.set_id,
                            &transcript,
                            logical_draw_index,
                            used_profiles.as_slice(),
                            tutorial_profile_ids
                                .as_ref()
                                .map(|profile_ids| profile_ids.as_slice())
                                .unwrap_or(&[]),
                        )?
                    } else {
                        let rarity_roll = Self::uniform_index(
                            b"ETERRA_PACK_RARITY_V3",
                            &transcript,
                            logical_draw_index,
                            10_000,
                        )? as u32;
                        let rarity = Self::roll_rarity(sku.rarity_weights, rarity_roll);
                        Self::select_profile_for_pack(
                            &pool,
                            sku.set_id,
                            rarity,
                            &transcript,
                            logical_draw_index,
                            used_profiles.as_slice(),
                            &profile_protection,
                            subject_discovery_slot,
                        )?
                    };
                let cosmetic_novelty =
                    sku.discovery_policy == DiscoveryPolicy::PremiumCosmetic && slot == 1;
                let (cosmetic_position, pose, background) = Self::select_cosmetics(
                    &pool,
                    sku.set_id,
                    profile.subject_id,
                    profile.rarity,
                    &transcript,
                    logical_draw_index,
                    &cosmetic_protection,
                    cosmetic_novelty,
                )?;
                let card_id =
                    Self::mint_v2_card(&opening, &sku, &pool, slot, profile, pose, background)?;
                used_profiles.push(profile.profile_id);
                card_ids
                    .try_push(card_id)
                    .map_err(|_| Error::<T>::V2InvalidPackSku)?;
                if opening.economic_realm == EconomicRealm::Production {
                    Self::bitmap_insert(&mut profile_protection, profile_position)?;
                    Self::bitmap_insert(&mut cosmetic_protection, cosmetic_position)?;
                }
            }
            if opening.economic_realm == EconomicRealm::Production {
                PackProtectionHistoryBitmapsV2::<T>::insert(
                    &opening.owner,
                    sku.set_id,
                    profile_protection,
                );
                CosmeticProtectionBitmapsV2::<T>::insert(
                    &opening.owner,
                    sku.set_id,
                    cosmetic_protection,
                );
            }
            Self::release_pack_card_capacity(&opening.owner, sku.card_count)?;
            PendingPackOpeningsV2::<T>::remove(opening_id);
            LockedPackCreditsV2::<T>::remove(opening_id);
            TutorialConversionProfileIdsV2::<T>::remove(opening_id);
            OutstandingPackCreditCountV2::<T>::try_mutate(
                &opening.owner,
                (
                    opening.pack_sku,
                    opening.sku_version,
                    opening.economic_realm,
                ),
                |count| -> DispatchResult {
                    *count = count
                        .checked_sub(1)
                        .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            ProcessedAcquisitionsV2::<T>::insert(opening_id, card_ids.clone());
            Self::deposit_event(Event::PackOpenedV2 {
                owner: opening.owner,
                opening_id,
                card_ids,
            });
            Ok(())
        }

        fn do_timeout_pack_open(opening_id: Hash32) -> DispatchResult {
            if TimedOutPackOpeningsV2::<T>::contains_key(opening_id) {
                return Ok(());
            }
            ensure!(
                !ProcessedAcquisitionsV2::<T>::contains_key(opening_id),
                Error::<T>::V2PackOpeningTerminalConflict
            );
            let opening = PendingPackOpeningsV2::<T>::get(opening_id)
                .ok_or(Error::<T>::V2PackOpeningMissing)?;
            ensure!(
                T::V2Randomness::timed_out(opening.randomness_request_id),
                Error::<T>::V2PackOpeningNotTimedOut
            );
            let credit =
                LockedPackCreditsV2::<T>::get(opening_id).ok_or(Error::<T>::V2PackCreditMissing)?;
            let key = (
                opening.pack_sku,
                opening.sku_version,
                opening.economic_realm,
            );
            AvailablePackCreditIdsV2::<T>::try_mutate(
                &opening.owner,
                key,
                |queue| -> DispatchResult {
                    queue
                        .try_push(credit.credit_id)
                        .map_err(|_| Error::<T>::V2CreditQueueFull)?;
                    Ok(())
                },
            )?;
            PackCreditsV2::<T>::insert(credit.credit_id, credit.clone());
            let sku = PackSkuVersionsV2::<T>::get((opening.pack_sku, opening.sku_version))
                .ok_or(Error::<T>::V2PackSkuMissing)?;
            ensure!(
                sku.immutable_config_hash == opening.immutable_config_hash,
                Error::<T>::V2DefinitionMismatch
            );
            Self::release_pack_card_capacity(&opening.owner, sku.card_count)?;
            LockedPackCreditsV2::<T>::remove(opening_id);
            PendingPackOpeningsV2::<T>::remove(opening_id);
            TutorialConversionProfileIdsV2::<T>::remove(opening_id);
            TimedOutPackOpeningsV2::<T>::insert(opening_id, credit.credit_id);
            Self::deposit_event(Event::PackOpenTimedOutV2 {
                owner: opening.owner,
                opening_id,
                restored_credit_id: credit.credit_id,
            });
            Ok(())
        }

        fn validated_competitive_team(
            owner: &T::AccountId,
            format_id: u32,
            format_version: u32,
            card_ids: Vec<CardIdV2>,
        ) -> Result<(V2TeamCardsOf<T>, u8, EconomicRealm, u32), DispatchError> {
            let format = CompetitiveFormatsV2::<T>::get((format_id, format_version))
                .ok_or(Error::<T>::V2FormatMissing)?;
            ensure!(
                format.team_size == V2_BRING_FIVE_TEAM_SIZE
                    && card_ids.len() == usize::from(V2_BRING_FIVE_TEAM_SIZE),
                Error::<T>::V2TeamSizeInvalid
            );
            let mut subjects = sp_std::collections::btree_set::BTreeSet::new();
            let mut rarity_load = 0u8;
            let mut mythicals = 0u8;
            let mut top_rarities = 0u8;
            let mut realm = None;
            for card_id in card_ids.iter().copied() {
                let card = CardsV2::<T>::get(card_id).ok_or(Error::<T>::V2CardMissing)?;
                ensure!(card.owner == *owner, Error::<T>::V2NotCardOwner);
                ensure!(card.set_id == format.set_id, Error::<T>::V2InvalidFormat);
                ensure!(
                    card.state == CardStateV2::Active,
                    Error::<T>::V2CardNotActive
                );
                let profile = SubjectRarityProfilesV2::<T>::get(card.profile_id)
                    .ok_or(Error::<T>::V2DefinitionMissing)?;
                ensure!(
                    profile.subject_id == card.subject_id
                        && profile.subject_version == card.subject_version
                        && profile.rarity == card.rarity
                        && SubjectRarityProfileByKeyV2::<T>::get(
                            (card.subject_id, card.subject_version),
                            card.rarity,
                        ) == Some(card.profile_id),
                    Error::<T>::V2DefinitionMismatch
                );
                let definition_id =
                    SubjectDefinitionByKeyV2::<T>::get((card.subject_id, card.subject_version))
                        .ok_or(Error::<T>::V2DefinitionMissing)?;
                let definition = SubjectDefinitionsV2::<T>::get(definition_id)
                    .ok_or(Error::<T>::V2DefinitionMissing)?;
                ensure!(
                    definition.subject_id == card.subject_id
                        && definition.subject_version == card.subject_version,
                    Error::<T>::V2DefinitionMismatch
                );
                let pose = PoseDefinitionsV2::<T>::get(card.pose_definition_id)
                    .ok_or(Error::<T>::V2NoEligiblePose)?;
                ensure!(
                    pose.subject_id == Some(card.subject_id),
                    Error::<T>::V2DefinitionMismatch
                );
                let background = BackgroundDefinitionsV2::<T>::get(card.background_definition_id)
                    .ok_or(Error::<T>::V2NoEligibleBackground)?;
                ensure!(
                    background.subject_id.is_none()
                        || background.subject_id == Some(card.subject_id),
                    Error::<T>::V2DefinitionMismatch
                );
                ensure!(
                    subjects.insert(card.subject_id),
                    Error::<T>::V2DuplicateSubject
                );
                if let Some(expected) = realm {
                    ensure!(expected == card.economic_realm, Error::<T>::V2InvalidFormat);
                } else {
                    realm = Some(card.economic_realm);
                }
                rarity_load = rarity_load
                    .checked_add(profile.rarity_load)
                    .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                if card.rarity == CardRarity::Mythical {
                    mythicals = mythicals.saturating_add(1);
                }
                if card.rarity.is_legendary_or_better() {
                    top_rarities = top_rarities.saturating_add(1);
                }
            }
            ensure!(
                rarity_load <= format.rarity_load_budget,
                Error::<T>::V2TeamRarityLoadExceeded
            );
            ensure!(
                mythicals <= format.max_mythical,
                Error::<T>::V2TooManyMythicals
            );
            ensure!(
                top_rarities <= format.max_legendary_or_better,
                Error::<T>::V2TooManyTopRarities
            );
            let cards = V2TeamCardsOf::<T>::try_from(card_ids)
                .map_err(|_| Error::<T>::V2TeamSizeInvalid)?;
            let realm = realm.ok_or(Error::<T>::V2TeamSizeInvalid)?;
            Ok((cards, rarity_load, realm, format.set_id))
        }

        fn ensure_conversion_roster_safe(
            owner: &T::AccountId,
            candidate: &CardInstanceV2<T::AccountId, BlockNumberFor<T>>,
        ) -> DispatchResult {
            let team_id = ConversionSafetyTeamByRealmSetV2::<T>::get(
                owner,
                (candidate.set_id, candidate.economic_realm),
            )
            .ok_or(Error::<T>::V2PlayableRosterTooSmall)?;
            let team = CompetitiveTeamsV2::<T>::get(owner, team_id)
                .ok_or(Error::<T>::V2PlayableRosterTooSmall)?;
            ensure!(
                team.owner == *owner && team.team_id == team_id,
                Error::<T>::V2PlayableRosterTooSmall
            );
            ensure!(
                !team.cards.contains(&candidate.card_id),
                Error::<T>::V2PlayableRosterTooSmall
            );
            let (cards, _, realm, set_id) = Self::validated_competitive_team(
                owner,
                team.format_id,
                team.format_version,
                team.cards.to_vec(),
            )
            .map_err(|_| Error::<T>::V2PlayableRosterTooSmall)?;
            ensure!(
                cards == team.cards
                    && realm == candidate.economic_realm
                    && set_id == candidate.set_id,
                Error::<T>::V2PlayableRosterTooSmall
            );
            Ok(())
        }

        fn do_request_conversion(
            owner: &T::AccountId,
            card_id: CardIdV2,
            expected_catalog_version: u32,
            entropy_commitment: Hash32,
        ) -> DispatchResult {
            ensure!(
                !ConversionRequestByCard::<T>::contains_key(card_id),
                Error::<T>::V2ConversionAlreadyRequested
            );
            let mut card = CardsV2::<T>::get(card_id).ok_or(Error::<T>::V2CardMissing)?;
            ensure!(card.owner == *owner, Error::<T>::V2NotCardOwner);
            ensure!(
                card.state == CardStateV2::Active,
                Error::<T>::V2CardNotActive
            );
            let pending = PendingConversionCountByAccountV2::<T>::get(owner);
            ensure!(
                T::MaxPendingConversionsPerAccount::get() > 0
                    && pending < T::MaxPendingConversionsPerAccount::get(),
                Error::<T>::V2PendingConversionLimitReached
            );
            if let Some(bound_until) = V2CardAccountBoundUntil::<T>::get(card_id) {
                ensure!(
                    frame_system::Pallet::<T>::block_number() >= bound_until,
                    Error::<T>::V2ConversionNotAllowed
                );
            }
            let definition_id =
                SubjectDefinitionByKeyV2::<T>::get((card.subject_id, card.subject_version))
                    .ok_or(Error::<T>::V2DefinitionMissing)?;
            let definition = SubjectDefinitionsV2::<T>::get(definition_id)
                .ok_or(Error::<T>::V2DefinitionMissing)?;
            let activation = SubjectActivationStatesV2::<T>::get(definition_id)
                .ok_or(Error::<T>::V2DefinitionMissing)?;
            ensure!(
                definition.catalog_version == expected_catalog_version,
                Error::<T>::V2CatalogVersionMismatch
            );
            ensure!(
                definition.conversion_policy.permits_conversion() && activation.conversion_enabled,
                Error::<T>::V2ConversionNotAllowed
            );
            T::V2Entities::ensure_conversion_profile_active(
                card.subject_id,
                card.subject_version,
                card.rarity,
            )?;
            Self::ensure_conversion_roster_safe(owner, &card)?;
            let active_count = V2OwnerActiveCardCount::<T>::get(owner);
            let remaining = active_count
                .checked_sub(1)
                .ok_or(Error::<T>::V2ArithmeticOverflow)?;
            let reserved_entity_id = T::V2Entities::reserve_entity_id()?;
            let source_card_snapshot_hash = Self::hash_encoded(&card);
            let request_id = Self::hash_encoded(&(
                b"ETERRA_CARD_CONVERSION_V2",
                owner,
                card_id,
                reserved_entity_id,
                source_card_snapshot_hash,
                entropy_commitment,
            ));
            ensure!(
                !CardConversionTombstones::<T>::contains_key(request_id),
                Error::<T>::V2ConversionAlreadyRequested
            );
            let immutable_config_hash = Self::hash_encoded(&(
                definition.definition_hash,
                card.profile_id,
                card.pool_id,
                card.pool_version,
            ));
            let expected_randomness_provenance = match card.economic_realm {
                EconomicRealm::Training => T::V2Randomness::current_mode(),
                EconomicRealm::Production => RandomnessMode::DrandQuicknet,
            };
            let randomness_request_id = T::V2Randomness::request_for(
                card.economic_realm,
                expected_randomness_provenance,
                Self::hash_encoded(&b"ETERRA_ENTITY_GENOME_RANDOMNESS_V1"),
                entropy_commitment,
                immutable_config_hash,
                0,
            )?;
            card.state = CardStateV2::ConversionCommitted { request_id };
            CardsV2::<T>::insert(card_id, card.clone());
            V2OwnerActiveCardCount::<T>::insert(owner, remaining);
            LiveSupplyBySubjectRarityV2::<T>::try_mutate(
                (card.subject_id, card.rarity, card.economic_realm),
                |count| -> DispatchResult {
                    *count = count
                        .checked_sub(1)
                        .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            let tombstone = ConversionTombstone {
                request_id,
                owner: owner.clone(),
                source_card_id: card_id,
                source_card_snapshot_hash,
                source_rarity: card.rarity,
                subject_id: card.subject_id,
                subject_version: card.subject_version,
                reserved_entity_id,
                randomness_request_id,
                commitment: entropy_commitment,
                committed_at: frame_system::Pallet::<T>::block_number(),
                resolution: ConversionResolution::Pending,
                expected_randomness_provenance,
                expected_catalog_version,
            };
            CardConversionTombstones::<T>::insert(request_id, tombstone);
            ConversionRequestByCard::<T>::insert(card_id, request_id);
            PendingConversionCountByAccountV2::<T>::insert(
                owner,
                pending
                    .checked_add(1)
                    .ok_or(Error::<T>::V2ArithmeticOverflow)?,
            );
            Self::deposit_event(Event::CardConversionCommitted {
                owner: owner.clone(),
                card_id,
                request_id,
                reserved_entity_id,
                randomness_request_id,
                source_card_snapshot_hash,
            });
            Ok(())
        }

        fn do_finalize_conversion(request_id: Hash32, stasis_genome: bool) -> DispatchResult {
            let mut tombstone = CardConversionTombstones::<T>::get(request_id)
                .ok_or(Error::<T>::V2ConversionMissing)?;
            match (tombstone.resolution, stasis_genome) {
                (ConversionResolution::Created, false)
                | (ConversionResolution::StasisTimeout, true) => return Ok(()),
                (ConversionResolution::Created, true)
                | (ConversionResolution::StasisTimeout, false) => {
                    return Err(Error::<T>::V2ConversionTerminalConflict.into())
                }
                (ConversionResolution::Pending, _) => {}
            }
            let mut card =
                CardsV2::<T>::get(tombstone.source_card_id).ok_or(Error::<T>::V2CardMissing)?;
            ensure!(
                card.state == CardStateV2::ConversionCommitted { request_id },
                Error::<T>::V2CardNotActive
            );
            let genome_seed = if stasis_genome {
                Self::hash_encoded(&(
                    b"ETERRA_STASIS_GENOME_V1".as_slice(),
                    T::V2ChainDomain::genesis_hash(),
                    request_id,
                    tombstone.source_card_snapshot_hash,
                    tombstone.reserved_entity_id,
                    tombstone.commitment,
                ))
            } else {
                let randomness_output = T::V2Randomness::output_for(
                    tombstone.randomness_request_id,
                    card.economic_realm,
                    tombstone.expected_randomness_provenance,
                )
                .ok_or(Error::<T>::V2ConversionNotReady)?
                .output;
                Self::conversion_genome_hash(
                    request_id,
                    tombstone.source_card_snapshot_hash,
                    tombstone.reserved_entity_id,
                    tombstone.commitment,
                    randomness_output,
                    tombstone.subject_version,
                    1u16,
                )
            };
            T::V2Entities::create_from_conversion(
                pallet_eterra_creatures::ConversionEntityInput {
                    entity_id: tombstone.reserved_entity_id,
                    owner: tombstone.owner.clone(),
                    economic_realm: card.economic_realm,
                    source_card_id: tombstone.source_card_id,
                    source_rarity: tombstone.source_rarity,
                    subject_id: tombstone.subject_id,
                    subject_version: tombstone.subject_version,
                    genome_seed,
                    stasis_genome,
                },
            )?;
            card.state = CardStateV2::Converted {
                entity_id: tombstone.reserved_entity_id,
            };
            CardsV2::<T>::insert(tombstone.source_card_id, card.clone());
            tombstone.resolution = if stasis_genome {
                ConversionResolution::StasisTimeout
            } else {
                ConversionResolution::Created
            };
            CardConversionTombstones::<T>::insert(request_id, tombstone.clone());
            ConvertedSupplyBySubjectRarityV2::<T>::try_mutate(
                (
                    tombstone.subject_id,
                    tombstone.source_rarity,
                    card.economic_realm,
                ),
                |count| -> DispatchResult {
                    *count = count
                        .checked_add(1)
                        .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            PendingConversionCountByAccountV2::<T>::try_mutate(
                &tombstone.owner,
                |pending| -> DispatchResult {
                    *pending = pending
                        .checked_sub(1)
                        .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            Self::deposit_event(Event::CardConvertedToEntity {
                owner: tombstone.owner,
                card_id: tombstone.source_card_id,
                request_id,
                entity_id: tombstone.reserved_entity_id,
                stasis_genome,
            });
            Ok(())
        }

        fn do_mythical_ascension(
            owner: &T::AccountId,
            season_id: u32,
            subject_id: SubjectId,
            input: MythicalAscensionInput,
        ) -> DispatchResult {
            Self::ensure_v16_migration_complete()?;
            let season_eligibility_id = SeasonEligibilityByAccountV2::<T>::get(owner, season_id)
                .ok_or(Error::<T>::V2SeasonEligibilityMissing)?;

            if let Some(existing_ascension_id) =
                MythicalAscensionByEligibilityV2::<T>::get((season_eligibility_id, season_id))
            {
                let receipt = MythicalAscensionReceiptsV2::<T>::get(existing_ascension_id)
                    .ok_or(Error::<T>::V2AscensionAlreadyCompleted)?;
                ensure!(
                    receipt.owner == *owner
                        && receipt.season_id == season_id
                        && receipt.subject_id == subject_id
                        && receipt.input == input,
                    Error::<T>::V2AscensionAlreadyCompleted
                );
                return Ok(());
            }
            Self::ensure_operational_card_capacity(owner, 1)?;

            let season = MythicalAscensionSeasonConfigsV2::<T>::get(season_id)
                .ok_or(Error::<T>::V2AscensionSeasonMissing)?;
            let now = frame_system::Pallet::<T>::block_number();
            ensure!(
                now >= season.starts_at && now < season.ends_at,
                Error::<T>::V2AscensionNotActive
            );
            let subject = MythicalAscensionSubjectConfigsV2::<T>::get(season_id, subject_id)
                .ok_or(Error::<T>::V2AscensionSubjectMissing)?;
            ensure!(
                subject.season_id == season_id && subject.subject_id == subject_id,
                Error::<T>::V2AscensionConfigInvalid
            );

            let mastery_key = (season_eligibility_id, season_id, subject_id);
            let convergence = ConvergenceProgressV2::<T>::get((season_eligibility_id, season_id));
            ensure!(
                MythicalSubjectMasteryV2::<T>::get(mastery_key) == season.required_mastery
                    && convergence.marks_earned >= season.required_marks
                    && convergence.credited_week_bitmap.count_ones()
                        >= u32::from(season.required_marks)
                    && MythicCatalystsV2::<T>::get((season_eligibility_id, season_id)),
                Error::<T>::V2AscensionRequirementsMissing
            );

            let definition_id =
                SubjectDefinitionByKeyV2::<T>::get((subject_id, subject.subject_version))
                    .ok_or(Error::<T>::V2DefinitionMissing)?;
            let activation = SubjectActivationStatesV2::<T>::get(definition_id)
                .ok_or(Error::<T>::V2DefinitionMissing)?;
            let profile_id = SubjectRarityProfileByKeyV2::<T>::get(
                (subject_id, subject.subject_version),
                CardRarity::Mythical,
            )
            .ok_or(Error::<T>::V2DefinitionMissing)?;
            let profile = SubjectRarityProfilesV2::<T>::get(profile_id)
                .ok_or(Error::<T>::V2DefinitionMissing)?;
            let pool = AcquisitionPoolVersionsV2::<T>::get((season.pool_id, season.pool_version))
                .ok_or(Error::<T>::V2PoolMissing)?;
            ensure!(
                activation.mint_enabled
                    && profile.subject_id == subject_id
                    && profile.subject_version == subject.subject_version
                    && profile.rarity == CardRarity::Mythical
                    && profile.validate()
                    && pool.set_id == season.set_id
                    && pool
                        .profiles
                        .iter()
                        .any(|entry| entry.profile_id == profile_id),
                Error::<T>::V2AscensionConfigInvalid
            );

            let (pose_definition_id, background_definition_id, source_card) = match input {
                MythicalAscensionInput::LegendaryCard { card_id } => {
                    let card = CardsV2::<T>::get(card_id)
                        .ok_or(Error::<T>::V2AscensionLegendaryInvalid)?;
                    ensure!(
                        card.owner == *owner
                            && card.state == CardStateV2::Active
                            && card.economic_realm == EconomicRealm::Production
                            && card.rarity == CardRarity::Legendary
                            && card.subject_id == subject_id
                            && card.subject_version == subject.subject_version,
                        Error::<T>::V2AscensionLegendaryInvalid
                    );
                    (
                        card.pose_definition_id,
                        card.background_definition_id,
                        Some(card),
                    )
                }
                MythicalAscensionInput::LegendaryFoundation => {
                    ensure!(
                        LegendaryFoundationsV2::<T>::get(mastery_key),
                        Error::<T>::V2AscensionFoundationMissing
                    );
                    (
                        subject.foundation_pose_definition_id,
                        subject.foundation_background_definition_id,
                        None,
                    )
                }
            };

            let pose = PoseDefinitionsV2::<T>::get(pose_definition_id)
                .ok_or(Error::<T>::V2DefinitionMissing)?;
            let background = BackgroundDefinitionsV2::<T>::get(background_definition_id)
                .ok_or(Error::<T>::V2DefinitionMissing)?;
            ensure!(
                pose.subject_id == Some(subject_id)
                    && (background.subject_id.is_none()
                        || background.subject_id == Some(subject_id)),
                Error::<T>::V2AscensionConfigInvalid
            );

            let raw_id = NextCardIdV2::<T>::get();
            let output_card_id = raw_id.max(1);
            let next_card_id = output_card_id
                .checked_add(1)
                .ok_or(Error::<T>::V2CardIdExhausted)?;
            let serial_number = NextSerialV2::<T>::get((subject_id, CardRarity::Mythical))
                .checked_add(1)
                .ok_or(Error::<T>::V2ArithmeticOverflow)?;
            let combined_config_hash =
                Self::hash_encoded(&(season.config_hash, subject.config_hash));
            let ascension_id = Self::hash_encoded(&(
                b"ETERRA_MYTHICAL_ASCENSION_V2",
                season_eligibility_id,
                season_id,
                subject_id,
                subject.subject_version,
                input,
                combined_config_hash,
            ));
            ensure!(
                !MythicalAscensionReceiptsV2::<T>::contains_key(ascension_id),
                Error::<T>::V2AscensionAlreadyCompleted
            );

            let mut profile_protection =
                PackProtectionHistoryBitmapsV2::<T>::get(owner, season.set_id);
            let profile_bit = Self::profile_protection_bit(season.set_id, &profile)?;
            Self::bitmap_insert(&mut profile_protection, profile_bit)?;

            let cosmetic_bit =
                if PoseProtectionSlotsV2::<T>::contains_key(season.set_id, pose_definition_id)
                    && BackgroundProtectionSlotsV2::<T>::contains_key(
                        season.set_id,
                        background_definition_id,
                    )
                {
                    Some(Self::cosmetic_protection_bit(
                        season.set_id,
                        subject_id,
                        CardRarity::Mythical,
                        pose_definition_id,
                        background_definition_id,
                    )?)
                } else {
                    None
                };
            let mut cosmetic_protection =
                CosmeticProtectionBitmapsV2::<T>::get(owner, season.set_id);
            if let Some(bit) = cosmetic_bit {
                Self::bitmap_insert(&mut cosmetic_protection, bit)?;
            }

            if let Some(mut card) = source_card {
                card.state = CardStateV2::MythicalAscended { output_card_id };
                CardsV2::<T>::insert(card.card_id, &card);
                V2OwnerActiveCardCount::<T>::try_mutate(owner, |count| -> DispatchResult {
                    *count = count
                        .checked_sub(1)
                        .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                    Ok(())
                })?;
                LiveSupplyBySubjectRarityV2::<T>::try_mutate(
                    (subject_id, CardRarity::Legendary, EconomicRealm::Production),
                    |count| -> DispatchResult {
                        *count = count
                            .checked_sub(1)
                            .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                        Ok(())
                    },
                )?;
            } else {
                LegendaryFoundationsV2::<T>::remove(mastery_key);
            }

            let output = CardInstanceV2 {
                card_id: output_card_id,
                owner: owner.clone(),
                set_id: season.set_id,
                season_id,
                subject_id,
                subject_version: subject.subject_version,
                rarity: CardRarity::Mythical,
                profile_id,
                pose_definition_id,
                background_definition_id,
                serial_number,
                economic_realm: EconomicRealm::Production,
                origin: CardOriginV2::MythicalAscension { ascension_id },
                acquisition_id: ascension_id,
                pool_id: season.pool_id,
                pool_version: season.pool_version,
                state: CardStateV2::Active,
                acquired_at: now,
            };
            CardsV2::<T>::insert(output_card_id, &output);
            NextCardIdV2::<T>::put(next_card_id);
            NextSerialV2::<T>::insert((subject_id, CardRarity::Mythical), serial_number);
            let mut crossed_operational_warning = None;
            V2OwnerCardCount::<T>::try_mutate(owner, |count| -> DispatchResult {
                let previous = *count;
                *count = count
                    .checked_add(1)
                    .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                if previous < T::V2OperationalCardWarningThreshold::get()
                    && *count >= T::V2OperationalCardWarningThreshold::get()
                {
                    crossed_operational_warning = Some(*count);
                }
                Ok(())
            })?;
            if let Some(lifetime_card_count) = crossed_operational_warning {
                Self::deposit_event(Event::V2OwnerCardOperationalWarning {
                    owner: owner.clone(),
                    lifetime_card_count,
                    unopened_limit: T::V2OperationalCardLimit::get(),
                });
            }
            V2OwnerActiveCardCount::<T>::try_mutate(owner, |count| -> DispatchResult {
                *count = count
                    .checked_add(1)
                    .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                Ok(())
            })?;
            LiveSupplyBySubjectRarityV2::<T>::try_mutate(
                (subject_id, CardRarity::Mythical, EconomicRealm::Production),
                |count| -> DispatchResult {
                    *count = count
                        .checked_add(1)
                        .ok_or(Error::<T>::V2ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            PackProtectionHistoryBitmapsV2::<T>::insert(owner, season.set_id, profile_protection);
            if cosmetic_bit.is_some() {
                CosmeticProtectionBitmapsV2::<T>::insert(owner, season.set_id, cosmetic_protection);
            }
            ConvergenceProgressV2::<T>::remove((season_eligibility_id, season_id));
            MythicCatalystsV2::<T>::remove((season_eligibility_id, season_id));
            V2CardAccountBoundUntil::<T>::insert(output_card_id, season.ends_at);
            MythicalAscensionByEligibilityV2::<T>::insert(
                (season_eligibility_id, season_id),
                ascension_id,
            );
            MythicalAscensionReceiptsV2::<T>::insert(
                ascension_id,
                MythicalAscensionReceipt {
                    ascension_id,
                    season_eligibility_id,
                    owner: owner.clone(),
                    season_id,
                    subject_id,
                    subject_version: subject.subject_version,
                    input,
                    output_card_id,
                    pose_definition_id,
                    background_definition_id,
                    config_hash: combined_config_hash,
                    ascended_at: now,
                },
            );
            Self::deposit_event(Event::CardAcquiredV2 {
                owner: owner.clone(),
                card_id: output_card_id,
                subject_id,
                subject_version: subject.subject_version,
                rarity: CardRarity::Mythical,
                profile_id,
                pose_definition_id,
                background_definition_id,
                resolved_ranks: profile.base_ranks,
                economic_realm: EconomicRealm::Production,
            });
            Self::deposit_event(Event::MythicalAscendedV2 {
                owner: owner.clone(),
                season_eligibility_id,
                season_id,
                subject_id,
                input,
                output_card_id,
                ascension_id,
                account_bound_until: season.ends_at,
            });
            Ok(())
        }

        fn record_migration_anomaly(
            state: &mut TcgMigrationStateV16,
            card_id: u32,
            card_owner: T::AccountId,
            nexus_owner: Option<T::AccountId>,
            reason: &[u8],
        ) -> LegacyClassificationOf<T> {
            let reason_hash = Self::hash_encoded(&(b"ETERRA_V16_CUSTODY_ANOMALY", card_id, reason));
            TcgMigrationAnomaliesV16::<T>::insert(
                card_id,
                TcgMigrationAnomalyV16 {
                    card_id,
                    card_owner,
                    nexus_owner,
                    reason_hash,
                },
            );
            state.anomalies = state.anomalies.saturating_add(1);
            Self::deposit_event(Event::LegacyMigrationAnomalyRecorded {
                card_id,
                reason_hash,
            });
            LegacyCardClassification {
                beneficial_owner: None,
                custody: LegacyCustodyKind::UnknownFrozen,
                frozen: true,
                record_hash: reason_hash,
            }
        }

        /// Bounded, sparse-ID-safe V14/V15→V16 classifier. It never rewrites a legacy
        /// SCALE value; it builds sidecar custody and repaired-owner indexes.
        pub fn migrate_v16_batch(limit: u32) -> u32 {
            let Some(mut state) = TcgMigrationStateStorageV16::<T>::get() else {
                return 0;
            };
            if state.phase != MigrationPhaseV16::Running || limit == 0 {
                return 0;
            }
            let mut visited = 0u32;
            while state.cursor < state.upper_bound && visited < limit {
                let card_id = state.cursor;
                state.cursor = state.cursor.saturating_add(1);
                visited = visited.saturating_add(1);
                let Some(card) = Cards::<T>::get(card_id) else {
                    continue;
                };
                state.cards_seen = state.cards_seen.saturating_add(1);
                state.max_card_id_seen = Some(
                    state
                        .max_card_id_seen
                        .map(|current| current.max(card_id))
                        .unwrap_or(card_id),
                );
                let nexus = NexusCollectionCards::<T>::get(card_id);
                let nexus_owner = nexus.as_ref().map(|record| record.owner.clone());
                let escrow = Self::escrow_account_id();
                let external_escrow_owner = T::LegacyEscrowOwnerProvider::beneficial_owner(card_id);
                let external_escrow_custodian = T::LegacyEscrowOwnerProvider::custodian_account();
                let mut custody = LegacyCustodyKind::Ordinary;
                let mut beneficial_owner = Some(card.owner.clone());
                let mut invalid_reason: Option<&[u8]> = None;

                if Converted::<T>::contains_key(card_id) {
                    let nft_owner = CardNftCollectionId::<T>::get().and_then(|collection_id| {
                        pallet_nfts::Pallet::<T>::owner(collection_id, card_id)
                    });
                    if external_escrow_owner.is_some()
                        || card.owner != escrow
                        || nft_owner.is_none()
                    {
                        invalid_reason = Some(b"nft wrapper has missing owner or non-escrow card");
                        beneficial_owner = None;
                    } else {
                        custody = LegacyCustodyKind::NftWrapped;
                        beneficial_owner = nft_owner;
                    }
                } else if let Some(owner) = external_escrow_owner {
                    if external_escrow_custodian.as_ref() != Some(&card.owner) {
                        invalid_reason = Some(b"external escrow entry and card custody diverged");
                        beneficial_owner = None;
                    } else {
                        custody = LegacyCustodyKind::KnownEscrow;
                        beneficial_owner = Some(owner);
                    }
                } else if card.owner == escrow {
                    invalid_reason = Some(b"unrecognized escrow custody");
                    beneficial_owner = None;
                } else if external_escrow_custodian.as_ref() == Some(&card.owner) {
                    invalid_reason = Some(b"external escrow custody has no owner entry");
                    beneficial_owner = None;
                }

                if invalid_reason.is_none() {
                    if let (Some(expected), Some(actual)) =
                        (beneficial_owner.as_ref(), nexus_owner.as_ref())
                    {
                        if expected != actual {
                            invalid_reason =
                                Some(b"legacy and nexus beneficial ownership diverged");
                        }
                    }
                }

                if invalid_reason.is_none() {
                    if let (Some(owner), Some(record)) = (beneficial_owner.as_ref(), nexus.as_ref())
                    {
                        let overflow_cards = NexusOverflowCards::<T>::get(owner);
                        invalid_reason = match record.location {
                            NexusStorageLocation::Collection => {
                                if overflow_cards.contains(&card_id)
                                    || NexusSubjectCopyCounts::<T>::get(owner, record.subject_id)
                                        == 0
                                {
                                    Some(b"collection location indexes diverged")
                                } else {
                                    None
                                }
                            }
                            NexusStorageLocation::Overflow => {
                                if !overflow_cards.contains(&card_id)
                                    || NexusOverflowSubjectCounts::<T>::get(
                                        owner,
                                        record.subject_id,
                                    ) == 0
                                {
                                    Some(b"overflow location indexes diverged")
                                } else {
                                    None
                                }
                            }
                            NexusStorageLocation::Vault => {
                                // Vault is an internal presentation/storage
                                // location, not third-party custody. The
                                // authoritative legacy card and Nexus record
                                // still name the same beneficial owner.
                                //
                                // `VaultVariants` is keyed by an independent,
                                // sparse variant id and contains no owner. A
                                // missing reverse metadata record therefore
                                // cannot make ownership ambiguous and must not
                                // strand an otherwise legitimate card. Preserve
                                // the known owner while still requiring the
                                // owner/subject indexes that make a Vault card
                                // usable through the normal safe-exit paths.
                                if overflow_cards.contains(&card_id)
                                    || NexusSubjectCopyCounts::<T>::get(owner, record.subject_id)
                                        == 0
                                {
                                    Some(b"vault location indexes diverged")
                                } else {
                                    None
                                }
                            }
                        };
                    }
                }

                let classification = if let Some(reason) = invalid_reason {
                    Self::record_migration_anomaly(
                        &mut state,
                        card_id,
                        card.owner.clone(),
                        nexus_owner.clone(),
                        reason,
                    )
                } else {
                    let record_hash = Self::hash_encoded(&(
                        b"ETERRA_LEGACY_V1_CLASSIFICATION",
                        card_id,
                        card.clone(),
                        nexus.clone(),
                        custody,
                        beneficial_owner.clone(),
                    ));
                    LegacyCardClassification {
                        beneficial_owner: beneficial_owner.clone(),
                        custody,
                        frozen: false,
                        record_hash,
                    }
                };

                if let Some(owner) = classification.beneficial_owner.clone() {
                    RepairedLegacyCardsByOwnerV16::<T>::insert(&owner, card_id, true);
                    if let Some(record) = nexus.as_ref() {
                        RepairedLegacySubjectCountsV16::<T>::mutate(
                            &owner,
                            record.subject_id,
                            |count| *count = count.saturating_add(1),
                        );
                    }
                }

                match classification.custody {
                    LegacyCustodyKind::Ordinary => {
                        state.ordinary = state.ordinary.saturating_add(1)
                    }
                    LegacyCustodyKind::NftWrapped => {
                        state.nft_wrapped = state.nft_wrapped.saturating_add(1)
                    }
                    LegacyCustodyKind::KnownEscrow => {
                        state.known_escrow = state.known_escrow.saturating_add(1)
                    }
                    LegacyCustodyKind::UnknownFrozen => {}
                }
                LegacyCardClassifications::<T>::insert(card_id, classification);
            }

            if state.cursor >= state.upper_bound {
                state.phase = MigrationPhaseV16::AwaitingVerification;
                LegacyWritesPausedV16::<T>::put(true);
                log::info!(
                    target: "runtime::eterra_tcg",
                    "ETERRA_V16_MIGRATION_AWAITING_VERIFICATION source_version={} cards_seen={} ordinary={} nft_wrapped={} known_escrow={} anomalies={} upper_bound={} max_card_id_seen={}",
                    state.from_storage_version,
                    state.cards_seen,
                    state.ordinary,
                    state.nft_wrapped,
                    state.known_escrow,
                    state.anomalies,
                    state.upper_bound,
                    state.max_card_id_seen.unwrap_or(u32::MAX),
                );
                Self::deposit_event(Event::LegacyMigrationAwaitingVerification {
                    from_storage_version: state.from_storage_version,
                    cards_seen: state.cards_seen,
                    anomalies: state.anomalies,
                });
            } else {
                Self::deposit_event(Event::LegacyMigrationProgress {
                    from_storage_version: state.from_storage_version,
                    cursor: state.cursor,
                    cards_seen: state.cards_seen,
                    anomalies: state.anomalies,
                });
            }
            TcgMigrationStateStorageV16::<T>::put(state);
            visited
        }

        fn ensure_legacy_writes_allowed() -> DispatchResult {
            ensure!(
                !LegacyWritesPausedV16::<T>::get(),
                Error::<T>::LegacyWritesPaused
            );
            Ok(())
        }

        fn ensure_legacy_card_not_frozen(card_id: u32) -> DispatchResult {
            if let Some(classification) = LegacyCardClassifications::<T>::get(card_id) {
                ensure!(
                    !classification.frozen
                        && classification.custody != LegacyCustodyKind::UnknownFrozen,
                    Error::<T>::LegacyCardFrozen
                );
            }
            Ok(())
        }

        fn ensure_legacy_creation_allowed() -> DispatchResult {
            Self::ensure_legacy_writes_allowed()?;
            ensure!(
                !LegacyCreationSealedV16::<T>::get(),
                Error::<T>::LegacyCreationSealed
            );
            Ok(())
        }

        fn ensure_v16_migration_complete() -> DispatchResult {
            ensure!(
                TcgMigrationStateStorageV16::<T>::get()
                    .map(|state| state.phase == MigrationPhaseV16::Completed)
                    .unwrap_or(true),
                Error::<T>::V16MigrationIncomplete
            );
            Ok(())
        }

        fn validated_starter_team_cards(
            cards: Vec<StarterCardTemplate>,
            config_version: NexusConfigVersion,
        ) -> Result<BoundedStarterTeamCards<T>, DispatchError> {
            ensure!(
                cards.len().saturated_into::<u32>() == T::NexusTeamSize::get(),
                Error::<T>::InvalidStarterTeamConfig
            );

            for template in cards.iter() {
                ensure!(
                    template.config_version == config_version,
                    Error::<T>::InvalidStarterTeamConfig
                );
                ensure!(
                    template.apex_side.is_none(),
                    Error::<T>::InvalidStarterTeamConfig
                );
                let _ = Self::starter_slot_values(template.base_ranks)?;
            }

            cards
                .try_into()
                .map_err(|_| Error::<T>::InvalidStarterTeamConfig.into())
        }

        fn starter_slot_values(base_ranks: [RankValue; 4]) -> Result<[u8; 4], DispatchError> {
            let mut values = [0u8; 4];
            for (idx, rank) in base_ranks.iter().enumerate() {
                match rank {
                    RankValue::Number(value) if *value >= 1 && *value <= 9 => {
                        values[idx] = *value;
                    }
                    _ => return Err(Error::<T>::InvalidStarterTeamConfig.into()),
                }
            }
            Ok(values)
        }

        fn ensure_card_not_account_bound(card_id: u32) -> DispatchResult {
            Self::ensure_legacy_card_not_frozen(card_id)?;
            if let Some(card) = NexusCollectionCards::<T>::get(card_id) {
                ensure!(!card.account_bound, Error::<T>::AccountBoundCardLocked);
            }
            Ok(())
        }

        fn validated_progression_nodes(
            nodes: Vec<ProgressionNode>,
            config_version: NexusConfigVersion,
        ) -> Result<BoundedProgressionNodes<T>, DispatchError> {
            ensure!(!nodes.is_empty(), Error::<T>::InvalidProgressionTree);
            let mut seen =
                BoundedBTreeSet::<ProgressionNodeId, T::MaxProgressionNodesPerTree>::new();
            for node in nodes.iter() {
                ensure!(
                    node.config_version == config_version,
                    Error::<T>::InvalidProgressionTree
                );
                let inserted = seen
                    .try_insert(node.node_id)
                    .map_err(|_| Error::<T>::InvalidProgressionTree)?;
                ensure!(inserted, Error::<T>::InvalidProgressionTree);
            }

            nodes
                .try_into()
                .map_err(|_| Error::<T>::InvalidProgressionTree.into())
        }

        fn ensure_card_exists(card_id: u32) -> DispatchResult {
            Self::ensure_legacy_card_not_frozen(card_id)?;
            ensure!(Cards::<T>::contains_key(card_id), Error::<T>::NoSuchCard);
            Ok(())
        }

        fn ensure_card_owner(
            card_id: u32,
            owner: &T::AccountId,
        ) -> Result<CardInfo<T::AccountId>, DispatchError> {
            Self::ensure_legacy_card_not_frozen(card_id)?;
            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == *owner, Error::<T>::NotCardOwner);
            Ok(card_info)
        }

        fn ensure_card_build_mutable(
            card_id: u32,
            owner: &T::AccountId,
        ) -> Result<CardInfo<T::AccountId>, DispatchError> {
            let card_info = Self::ensure_card_owner(card_id, owner)?;
            ensure!(card_info.finalized, Error::<T>::CardNotFinalized);
            ensure!(
                !Converted::<T>::contains_key(card_id),
                Error::<T>::CardConvertedBuildLocked
            );
            ensure!(
                !T::HandChecker::is_card_in_current_hand(owner, card_id),
                Error::<T>::CardInCurrentHand
            );
            ensure!(
                !CardPrices::<T>::contains_key(card_id),
                Error::<T>::CardBuildLocked
            );
            Ok(card_info)
        }

        fn clear_magic_loadout_on_owner_change(
            card_id: u32,
            old_owner: &T::AccountId,
            new_owner: &T::AccountId,
        ) {
            if old_owner == new_owner {
                return;
            }
            if let Some(loadout) = CardMagicLoadouts::<T>::take(card_id) {
                Self::deposit_event(Event::CardMagicLoadoutCleared {
                    card_id,
                    old_owner: old_owner.clone(),
                    new_owner: new_owner.clone(),
                    config_version: loadout.config_version,
                });
            }
        }

        fn initialize_card_progression(
            card_id: u32,
            tree_id: ProgressionTreeId,
            config_version: NexusConfigVersion,
        ) -> DispatchResult {
            let progression = CardProgression {
                card_id,
                tree_id,
                level: 1,
                experience: 0,
                completed_nodes: BoundedProgressionNodeIds::<T>::default(),
                config_version,
            };
            CardProgressions::<T>::insert(card_id, progression);
            ProgressionTreeUseCounts::<T>::mutate(tree_id, |count| {
                *count = count.saturating_add(1);
            });
            Self::deposit_event(Event::CardProgressionInitialized {
                card_id,
                tree_id,
                config_version,
            });
            Ok(())
        }

        fn level_for_experience(experience: u32) -> u16 {
            let xp_per_level = T::CardXpPerLevel::get().max(1);
            let level = 1u32.saturating_add(experience / xp_per_level);
            level.min(u16::MAX as u32) as u16
        }

        fn progression_subject_for_card(card_id: u32) -> Option<SubjectId> {
            if let Some(card) = NexusCollectionCards::<T>::get(card_id) {
                return Some(card.subject_id);
            }
            CardArtwork::<T>::get(card_id)
                .map(|artwork| artwork.subject_media_id.saturated_into::<SubjectId>())
        }

        fn try_assign_progression_tree_for_card(card_id: u32) -> DispatchResult {
            if CardProgressions::<T>::contains_key(card_id) {
                return Ok(());
            }
            let Some(subject_id) = Self::progression_subject_for_card(card_id) else {
                return Ok(());
            };
            let Some(tree_id) = ProgressionTreeBySubject::<T>::get(subject_id, None::<u8>) else {
                return Ok(());
            };
            let Some(tree) = ProgressionTrees::<T>::get(tree_id) else {
                return Ok(());
            };
            Self::initialize_card_progression(card_id, tree_id, tree.config_version)
        }

        pub fn progression_node_status(
            card_id: u32,
            node_id: ProgressionNodeId,
        ) -> Result<ProgressionNodeStatus, DispatchError> {
            let progression =
                CardProgressions::<T>::get(card_id).ok_or(Error::<T>::CardProgressionMissing)?;
            if progression.completed_nodes.contains(&node_id) {
                return Ok(ProgressionNodeStatus::Completed);
            }
            let tree = ProgressionTrees::<T>::get(progression.tree_id)
                .ok_or(Error::<T>::ProgressionTreeMissing)?;
            let node = tree
                .nodes
                .iter()
                .find(|candidate| candidate.node_id == node_id)
                .ok_or(Error::<T>::ProgressionNodeMissing)?;
            if progression.level >= node.required_level {
                Ok(ProgressionNodeStatus::Unlocked)
            } else {
                Ok(ProgressionNodeStatus::Locked)
            }
        }

        pub fn nexus_card_total_power(card_id: u32) -> u16 {
            let base_power = NexusCollectionCards::<T>::get(card_id)
                .map(|card| card.card_power as u32)
                .unwrap_or(0);
            let progression_power = Self::completed_progression_power(card_id);
            let magic_power = Self::current_magic_power(card_id);

            base_power
                .saturating_add(progression_power)
                .saturating_add(magic_power)
                .min(u16::MAX as u32) as u16
        }

        fn completed_progression_power(card_id: u32) -> u32 {
            let Some(progression) = CardProgressions::<T>::get(card_id) else {
                return 0;
            };
            let Some(tree) = ProgressionTrees::<T>::get(progression.tree_id) else {
                return 0;
            };
            progression
                .completed_nodes
                .iter()
                .filter_map(|completed_id| {
                    tree.nodes
                        .iter()
                        .find(|node| node.node_id == *completed_id)
                        .map(|node| node.power_delta as u32)
                })
                .fold(0u32, |sum, power| sum.saturating_add(power))
        }

        fn current_magic_power(card_id: u32) -> u32 {
            let Some(card) = Cards::<T>::get(card_id) else {
                return 0;
            };
            let Some(loadout) = CardMagicLoadouts::<T>::get(card_id) else {
                return 0;
            };
            let mut seen = BoundedMagicSpellSet::<T>::new();
            loadout
                .spells
                .iter()
                .filter_map(|spell_id| {
                    let inserted = seen.try_insert(*spell_id).unwrap_or_default();
                    if !inserted {
                        return None;
                    }
                    NexusSpellbook::<T>::get(*spell_id).filter(|spell| spell.owner == card.owner)
                })
                .fold(0u32, |sum, spell| sum.saturating_add(spell.power as u32))
        }

        #[cfg(feature = "runtime-benchmarks")]
        pub fn benchmark_seed_finalized_card(
            owner: &T::AccountId,
            card_id: u32,
            slot_values: [u8; 4],
        ) -> DispatchResult {
            ensure!(!Cards::<T>::contains_key(card_id), Error::<T>::NoSuchCard);
            let next_card_id = card_id.checked_add(1).ok_or(Error::<T>::CardIdExhausted)?;

            Cards::<T>::insert(
                card_id,
                CardInfo {
                    owner: owner.clone(),
                    finalized: true,
                    slot_values: Some(slot_values),
                },
            );
            Self::record_card_mint(card_id, owner);
            CardsByOwner::<T>::try_mutate(owner, |set| -> DispatchResult {
                set.try_insert(card_id)
                    .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;
                Ok(())
            })?;
            NextCardId::<T>::mutate(|next| {
                if *next < next_card_id {
                    *next = next_card_id;
                }
            });
            let _ = Self::try_assign_progression_tree_for_card(card_id);
            Ok(())
        }

        pub fn validate_nexus_team_size(card_count: u32) -> DispatchResult {
            ensure!(
                card_count == T::NexusTeamSize::get(),
                Error::<T>::NexusTeamSizeInvalid
            );
            Ok(())
        }

        /// Shared chain-authoritative fulfillment path for the arcade Prize
        /// Counter, Vending Machine, and other catalog clients.
        #[transactional]
        pub fn try_fulfill_nexus_prize(
            owner: &T::AccountId,
            kind: NexusPrizeKind,
            pool_id: NexusPrizePoolId,
            featured_subject: Option<SubjectId>,
            entropy: [u8; 32],
            origin: NexusCardOrigin,
        ) -> Result<Vec<u32>, DispatchError> {
            T::AccessControl::ensure_whitelisted(owner)?;
            Self::ensure_legacy_creation_allowed()?;
            ensure!(
                matches!(origin, NexusCardOrigin::Claim | NexusCardOrigin::Pull),
                Error::<T>::InvalidNexusPrizePool
            );
            let pool =
                NexusPrizePools::<T>::get(pool_id).ok_or(Error::<T>::NexusPrizePoolMissing)?;
            let card_count = match kind {
                NexusPrizeKind::RandomPack => u32::from(T::CardsPerPack::get()),
                NexusPrizeKind::RandomSingle | NexusPrizeKind::FeaturedSubject => 1,
            };
            ensure!(card_count > 0, Error::<T>::InvalidNexusPrizePool);
            Self::ensure_can_receive_cards(owner, card_count)?;

            let mut selected: Vec<(
                NexusPrizeTemplate,
                StarterCardTemplate,
                NexusStorageLocation,
                [u8; 32],
            )> = Vec::new();
            let mut simulated_counts: Vec<(SubjectId, u32, u32)> = Vec::new();
            let mut simulated_overflow_total = NexusOverflowCards::<T>::get(owner).len() as u32;
            let config = Self::current_nexus_config();

            for card_index in 0..card_count {
                let derived_hash = T::Hashing::hash_of(&(entropy, pool_id, card_index));
                let mut card_entropy = [0u8; 32];
                card_entropy.copy_from_slice(&derived_hash.as_ref()[..32]);
                let template = match kind {
                    NexusPrizeKind::FeaturedSubject => {
                        let subject =
                            featured_subject.ok_or(Error::<T>::NexusPrizeSubjectUnavailable)?;
                        pool.templates
                            .iter()
                            .find(|candidate| candidate.card.subject_id == subject)
                            .copied()
                            .ok_or(Error::<T>::NexusPrizeSubjectUnavailable)?
                    }
                    NexusPrizeKind::RandomSingle | NexusPrizeKind::RandomPack => {
                        let index = u32::from_le_bytes([
                            card_entropy[0],
                            card_entropy[1],
                            card_entropy[2],
                            card_entropy[3],
                        ]) as usize
                            % pool.templates.len();
                        pool.templates[index]
                    }
                };
                let resolved = Self::resolve_nexus_prize_template(&template.card, card_entropy)?;
                let state_index = simulated_counts
                    .iter()
                    .position(|(subject, _, _)| *subject == resolved.subject_id);
                let index = match state_index {
                    Some(index) => index,
                    None => {
                        simulated_counts.push((
                            resolved.subject_id,
                            NexusSubjectCopyCounts::<T>::get(owner, resolved.subject_id),
                            NexusOverflowSubjectCounts::<T>::get(owner, resolved.subject_id),
                        ));
                        simulated_counts.len() - 1
                    }
                };
                let (_, collection_count, overflow_count) = &mut simulated_counts[index];
                let location = if *collection_count < config.subject_copy_cap {
                    *collection_count = collection_count
                        .checked_add(1)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    NexusStorageLocation::Collection
                } else {
                    ensure!(
                        simulated_overflow_total < config.overflow_total_capacity,
                        Error::<T>::NexusOverflowCapacityExceeded
                    );
                    ensure!(
                        *overflow_count < config.overflow_per_subject_capacity,
                        Error::<T>::NexusOverflowSubjectCapacityExceeded
                    );
                    simulated_overflow_total = simulated_overflow_total
                        .checked_add(1)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    *overflow_count = overflow_count
                        .checked_add(1)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    NexusStorageLocation::Overflow
                };
                selected.push((template, resolved, location, card_entropy));
            }

            let pull_id = if origin == NexusCardOrigin::Pull {
                let pull_id = NextNexusPullId::<T>::get();
                NextNexusPullId::<T>::put(
                    pull_id
                        .checked_add(1)
                        .ok_or(Error::<T>::ArithmeticOverflow)?,
                );
                Some(pull_id)
            } else {
                None
            };
            let mut card_ids = Vec::with_capacity(card_count as usize);
            for (template, resolved, location, card_entropy) in selected {
                let card_id = Self::create_nexus_prize_card(
                    owner,
                    template.kind,
                    &resolved,
                    location,
                    card_entropy,
                    origin,
                    pull_id,
                    pool.config_version,
                )?;
                card_ids.push(card_id);
            }
            Ok(card_ids)
        }

        fn resolve_nexus_prize_template(
            template: &StarterCardTemplate,
            entropy: [u8; 32],
        ) -> Result<StarterCardTemplate, DispatchError> {
            let mut base_ranks = template.base_ranks;
            let mut original_total: i32 = 0;
            let mut resolved_total: i32 = 0;
            for (index, rank) in base_ranks.iter_mut().enumerate() {
                let RankValue::Number(value) = *rank else {
                    return Err(Error::<T>::InvalidNexusPrizePool.into());
                };
                original_total += i32::from(value);
                let delta = i16::from(entropy[index] % 3) - 1;
                let resolved = (i16::from(value) + delta).clamp(1, 9) as u8;
                resolved_total += i32::from(resolved);
                *rank = RankValue::Number(resolved);
            }
            let vary_gene = |value: u8, byte: u8| -> u8 {
                let delta = i16::from(byte % 3) - 1;
                (i16::from(value) + delta).clamp(0, 100) as u8
            };
            let genes = GeneProfile {
                strength: vary_gene(template.genes.strength, entropy[8]),
                agility: vary_gene(template.genes.agility, entropy[9]),
                vitality: vary_gene(template.genes.vitality, entropy[10]),
                defense: vary_gene(template.genes.defense, entropy[11]),
                magic: vary_gene(template.genes.magic, entropy[12]),
                resist: vary_gene(template.genes.resist, entropy[13]),
            };
            let numeric = base_ranks.map(|rank| match rank {
                RankValue::Number(value) => value,
                RankValue::Apex => 10,
            });
            let min = *numeric.iter().min().unwrap_or(&1);
            let max = *numeric.iter().max().unwrap_or(&1);
            let style_label = if max.saturating_sub(min) <= 2 {
                RankStyleLabel::Balanced
            } else if max >= 8 {
                RankStyleLabel::Sharp
            } else {
                RankStyleLabel::Guarded
            };
            let power_delta = resolved_total - original_total;
            let card_power =
                (i32::from(template.card_power) + power_delta).clamp(1, i32::from(u16::MAX)) as u16;
            Ok(StarterCardTemplate {
                subject_id: template.subject_id,
                base_ranks,
                apex_side: None,
                style_label,
                genes,
                element_profile: template.element_profile,
                card_power,
                config_version: template.config_version,
            })
        }

        #[allow(clippy::too_many_arguments)]
        fn create_nexus_prize_card(
            owner: &T::AccountId,
            kind: NexusCardKind,
            template: &StarterCardTemplate,
            location: NexusStorageLocation,
            _entropy: [u8; 32],
            origin: NexusCardOrigin,
            pull_id: Option<u32>,
            pool_version: NexusConfigVersion,
        ) -> Result<u32, DispatchError> {
            let values = Self::starter_slot_values(template.base_ranks)?;
            let card_id = NextCardId::<T>::get();
            let next_card_id = card_id.checked_add(1).ok_or(Error::<T>::CardIdExhausted)?;
            let now = frame_system::Pallet::<T>::block_number();
            Cards::<T>::insert(
                card_id,
                CardInfo {
                    owner: owner.clone(),
                    finalized: true,
                    slot_values: Some(values),
                },
            );
            Self::record_card_mint(card_id, owner);
            CardsByOwner::<T>::try_mutate(owner, |set| -> DispatchResult {
                set.try_insert(card_id)
                    .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;
                Ok(())
            })?;
            NextCardId::<T>::put(next_card_id);
            Self::assign_starter_artwork_from_active_season(
                card_id,
                template.subject_id.saturated_into::<MediaId>(),
            )?;
            let _ = Self::ensure_card_genome(card_id)?;
            NexusCollectionCards::<T>::insert(
                card_id,
                CollectionCard {
                    owner: owner.clone(),
                    subject_id: template.subject_id,
                    kind,
                    origin,
                    base_ranks: template.base_ranks,
                    apex_side: template.apex_side,
                    genes: template.genes,
                    element_profile: template.element_profile,
                    card_power: template.card_power,
                    location,
                    account_bound: false,
                    acquired_at: now,
                    config_version: template.config_version,
                },
            );
            match location {
                NexusStorageLocation::Collection => {
                    NexusSubjectCopyCounts::<T>::try_mutate(
                        owner,
                        template.subject_id,
                        |count| -> DispatchResult {
                            *count = count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                            Ok(())
                        },
                    )?;
                }
                NexusStorageLocation::Overflow => {
                    NexusOverflowCards::<T>::try_mutate(owner, |cards| -> DispatchResult {
                        cards
                            .try_push(card_id)
                            .map_err(|_| Error::<T>::NexusOverflowCapacityExceeded)?;
                        Ok(())
                    })?;
                    NexusOverflowSubjectCounts::<T>::try_mutate(
                        owner,
                        template.subject_id,
                        |count| -> DispatchResult {
                            *count = count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                            Ok(())
                        },
                    )?;
                    Self::deposit_event(Event::CardEnteredOverflow {
                        account_id: owner.clone(),
                        card_record_id: card_id,
                        subject_id: template.subject_id,
                        reason: OverflowReason::SubjectCopyCapExceeded,
                    });
                }
                NexusStorageLocation::Vault => {
                    return Err(Error::<T>::InvalidNexusPrizePool.into());
                }
            }
            Self::try_assign_progression_tree_for_card(card_id)?;
            match origin {
                NexusCardOrigin::Claim => Self::deposit_event(Event::NexusCardClaimed {
                    account_id: owner.clone(),
                    card_record_id: card_id,
                    subject_id: template.subject_id,
                    source: origin,
                    config_version: template.config_version,
                }),
                NexusCardOrigin::Pull => Self::deposit_event(Event::NexusCardPulled {
                    account_id: owner.clone(),
                    pull_id: pull_id.ok_or(Error::<T>::InvalidNexusPrizePool)?,
                    card_record_id: card_id,
                    subject_id: template.subject_id,
                    pack_pool_version: pool_version,
                }),
                _ => return Err(Error::<T>::InvalidNexusPrizePool.into()),
            }
            Self::deposit_event(Event::RankSlotResolved {
                card_record_id: card_id,
                base_ranks: template.base_ranks,
                apex_side: template.apex_side,
                style_label: template.style_label,
                card_power: template.card_power,
                config_version: template.config_version,
            });
            Self::deposit_event(Event::GenesResolved {
                card_record_id: card_id,
                genes: template.genes,
                element_profile: template.element_profile,
                config_version: template.config_version,
            });
            Self::deposit_event(Event::CardMinted {
                player: owner.clone(),
                card_id,
            });
            Ok(card_id)
        }

        pub fn classify_nexus_card_location(
            owner: &T::AccountId,
            subject_id: SubjectId,
        ) -> Result<NexusStorageLocation, Error<T>> {
            let config = Self::current_nexus_config();
            let collection_and_vault_count = NexusSubjectCopyCounts::<T>::get(owner, subject_id);
            if collection_and_vault_count < config.subject_copy_cap {
                return Ok(NexusStorageLocation::Collection);
            }

            let overflow_cards = NexusOverflowCards::<T>::get(owner);
            if overflow_cards.len() as u32 >= config.overflow_total_capacity {
                return Err(Error::<T>::NexusOverflowCapacityExceeded);
            }

            let overflow_subject_count = NexusOverflowSubjectCounts::<T>::get(owner, subject_id);
            if overflow_subject_count >= config.overflow_per_subject_capacity {
                return Err(Error::<T>::NexusOverflowSubjectCapacityExceeded);
            }

            Ok(NexusStorageLocation::Overflow)
        }

        fn escrow_account_id() -> T::AccountId {
            ESCROW_PALLET_ID.into_account_truncating()
        }

        fn build_card_genome(card_id: u32) -> Result<CardGenomeHash, DispatchError> {
            let mint_info = CardMintInfoByCard::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            let artwork = CardArtwork::<T>::get(card_id).ok_or(Error::<T>::CardArtworkMissing)?;
            let collection_id = CardArtworkCollectionId::<T>::get(card_id).unwrap_or_default();
            let subject = (
                b"eterra-tcg/genome/v1",
                card_id,
                mint_info.minter,
                mint_info.minted_at,
                artwork,
                collection_id,
            )
                .encode();
            let hash = T::Hashing::hash(&subject);
            let mut genome = [0u8; 32];
            genome.copy_from_slice(&hash.as_ref()[..32]);
            Ok(genome)
        }

        pub fn ensure_card_genome(card_id: u32) -> Result<CardGenomeHash, DispatchError> {
            if let Some(genome) = CardGenome::<T>::get(card_id) {
                return Ok(genome);
            }

            let genome = Self::build_card_genome(card_id)?;
            CardGenome::<T>::insert(card_id, genome);
            Ok(genome)
        }

        fn owned_card_count(owner: &T::AccountId) -> u32 {
            CardsByOwner::<T>::get(owner).len().saturated_into::<u32>()
        }

        fn note_minter(account: &T::AccountId) {
            if HasMinted::<T>::contains_key(account) {
                return;
            }

            HasMinted::<T>::insert(account, ());
            UniqueMinterCount::<T>::mutate(|count| {
                *count = count.saturating_add(1);
            });
        }

        fn record_card_mint(card_id: u32, owner: &T::AccountId) {
            CardMintInfoByCard::<T>::insert(
                card_id,
                CardMintInfo {
                    minter: owner.clone(),
                    minted_at: <frame_system::Pallet<T>>::block_number(),
                },
            );
        }

        fn owned_card_capacity(owner: &T::AccountId) -> u32 {
            T::BaseCardCapacity::get().saturating_add(CardCapacityBonus::<T>::get(owner))
        }

        fn ensure_can_receive_cards(owner: &T::AccountId, additional_cards: u32) -> DispatchResult {
            let next_total = Self::owned_card_count(owner).saturating_add(additional_cards);
            ensure!(
                next_total <= Self::owned_card_capacity(owner),
                Error::<T>::CardCapacityExceeded
            );
            Ok(())
        }

        fn prune_completed_packs(packs: &mut BoundedVec<Pack, T::MaxOwnedCards>) {
            let retained: Vec<Pack> = packs
                .iter()
                .filter(|pack| !pack.completed)
                .cloned()
                .collect();
            *packs = match retained.try_into() {
                Ok(filtered) => filtered,
                Err(_) => unreachable!("filtered packs cannot exceed original bounded length"),
            };
        }

        fn ensure_season_admin(who: &T::AccountId) -> DispatchResult {
            ensure!(
                pallet_eterra_seasons::Admins::<T>::contains_key(who),
                Error::<T>::NotSeasonAdmin
            );
            Ok(())
        }

        fn ensure_season_manageable(season_id: SeasonId) -> DispatchResult {
            let season = pallet_eterra_seasons::Seasons::<T>::get(season_id)
                .ok_or(Error::<T>::UnknownSeason)?;
            ensure!(
                season.status != pallet_eterra_seasons::SeasonStatus::Closed,
                Error::<T>::SeasonClosed
            );
            Ok(())
        }

        fn ensure_season_collection_draft(
            season_id: SeasonId,
            collection_id: SeasonCollectionId,
        ) -> DispatchResult {
            Self::ensure_season_manageable(season_id)?;
            let collection = SeasonCollections::<T>::get(season_id, collection_id)
                .ok_or(Error::<T>::UnknownSeasonCollection)?;
            ensure!(
                collection.status == SeasonCollectionStatus::Draft,
                Error::<T>::SeasonCollectionNotDraft
            );
            Ok(())
        }

        fn ensure_media_valid(media_id: MediaId) -> DispatchResult {
            let meta =
                pallet_eterra_media::Media::<T>::get(media_id).ok_or(Error::<T>::UnknownMedia)?;
            ensure!(!meta.is_deprecated, Error::<T>::MediaDeprecated);
            Ok(())
        }

        fn ensure_collection_has_any_assets(assets: &SeasonAssetsInfoOf<T>) -> DispatchResult {
            ensure!(
                !assets.borders.is_empty()
                    || !assets.backgrounds.is_empty()
                    || !assets.subjects.is_empty()
                    || !assets.backs.is_empty()
                    || !assets.packaging_fronts.is_empty()
                    || !assets.packaging_backs.is_empty(),
                Error::<T>::SeasonCollectionIncomplete
            );
            Ok(())
        }

        fn ensure_collection_can_publish_into_season(
            season_id: SeasonId,
            assets: &SeasonAssetsInfoOf<T>,
        ) -> DispatchResult {
            let existing_pools = Self::published_season_asset_pools(season_id)?;
            ensure!(
                assets.packaging_fronts.len() == assets.packaging_backs.len(),
                Error::<T>::SeasonCollectionIncomplete
            );
            Self::ensure_collection_asset_weights_valid(assets)?;
            ensure!(
                !existing_pools.backs.is_empty() || !assets.backs.is_empty(),
                Error::<T>::SeasonCollectionIncomplete
            );
            ensure!(
                !existing_pools.packagings.is_empty() || !assets.packaging_fronts.is_empty(),
                Error::<T>::SeasonCollectionIncomplete
            );
            Ok(())
        }

        fn ensure_required_card_art_pools(pools: &PublishedSeasonAssetPools) -> DispatchResult {
            ensure!(
                !pools.borders.is_empty()
                    && !pools.backgrounds.is_empty()
                    && !pools.subjects.is_empty()
                    && !pools.backs.is_empty(),
                Error::<T>::NoPublishedSeasonCollection
            );
            Ok(())
        }

        fn ensure_required_season_pools(pools: &PublishedSeasonAssetPools) -> DispatchResult {
            Self::ensure_required_card_art_pools(pools)?;
            ensure!(
                !pools.packagings.is_empty(),
                Error::<T>::NoPublishedSeasonCollection
            );
            Ok(())
        }

        fn clear_asset_weight_config<ListLen: Get<u32>>(
            config: &mut AssetWeightConfig<
                BoundedVec<WeightPercentage, ListLen>,
                BoundedVec<WeightMultiplier, ListLen>,
            >,
        ) {
            config.weights = Default::default();
            config.multipliers = Default::default();
        }

        fn ensure_valid_asset_weight_config<ListLen: Get<u32>>(
            expected_len: usize,
            config: &AssetWeightConfig<
                BoundedVec<WeightPercentage, ListLen>,
                BoundedVec<WeightMultiplier, ListLen>,
            >,
        ) -> DispatchResult {
            if config.weights.is_empty() && config.multipliers.is_empty() {
                return Ok(());
            }

            ensure!(
                config.weights.len() == expected_len
                    && config.multipliers.len() == expected_len
                    && config.weights.len() == config.multipliers.len(),
                Error::<T>::AssetWeightCountMismatch
            );

            let total = config
                .weights
                .iter()
                .fold(0u32, |sum, weight| sum.saturating_add(*weight as u32));
            ensure!(
                total == WEIGHT_TOTAL_PERCENT,
                Error::<T>::AssetWeightTotalInvalid
            );

            let has_positive_effective_weight = config
                .weights
                .iter()
                .zip(config.multipliers.iter())
                .any(|(weight, multiplier)| *weight > 0 && *multiplier > 0);
            ensure!(
                has_positive_effective_weight,
                Error::<T>::AssetWeightMultiplierInvalid
            );
            Ok(())
        }

        fn ensure_collection_asset_weights_valid(assets: &SeasonAssetsInfoOf<T>) -> DispatchResult {
            Self::ensure_valid_asset_weight_config(assets.borders.len(), &assets.border_weights)?;
            Self::ensure_valid_asset_weight_config(
                assets.backgrounds.len(),
                &assets.background_weights,
            )?;
            Self::ensure_valid_asset_weight_config(assets.subjects.len(), &assets.subject_weights)?;
            Self::ensure_valid_asset_weight_config(assets.backs.len(), &assets.back_weights)?;
            Self::ensure_valid_asset_weight_config(
                assets
                    .packaging_fronts
                    .len()
                    .min(assets.packaging_backs.len()),
                &assets.packaging_weights,
            )?;
            Ok(())
        }

        fn set_asset_weight_config<ListLen: Get<u32>>(
            expected_len: usize,
            config: &mut AssetWeightConfig<
                BoundedVec<WeightPercentage, ListLen>,
                BoundedVec<WeightMultiplier, ListLen>,
            >,
            weights: Vec<WeightPercentage>,
            multipliers: Vec<WeightMultiplier>,
        ) -> Result<bool, DispatchError> {
            if weights.is_empty() && multipliers.is_empty() {
                Self::clear_asset_weight_config(config);
                return Ok(false);
            }

            let next_config = AssetWeightConfig {
                weights: weights.try_into().map_err(|_| Error::<T>::AssetListFull)?,
                multipliers: multipliers
                    .try_into()
                    .map_err(|_| Error::<T>::AssetListFull)?,
            };
            Self::ensure_valid_asset_weight_config(expected_len, &next_config)?;
            *config = next_config;
            Ok(true)
        }

        fn set_asset_weight_config_for_kind(
            assets: &mut SeasonAssetsInfoOf<T>,
            kind: AssetWeightKind,
            weights: Vec<WeightPercentage>,
            multipliers: Vec<WeightMultiplier>,
        ) -> Result<bool, DispatchError> {
            match kind {
                AssetWeightKind::Border => Self::set_asset_weight_config(
                    assets.borders.len(),
                    &mut assets.border_weights,
                    weights,
                    multipliers,
                ),
                AssetWeightKind::Background => Self::set_asset_weight_config(
                    assets.backgrounds.len(),
                    &mut assets.background_weights,
                    weights,
                    multipliers,
                ),
                AssetWeightKind::Subject => Self::set_asset_weight_config(
                    assets.subjects.len(),
                    &mut assets.subject_weights,
                    weights,
                    multipliers,
                ),
                AssetWeightKind::Back => Self::set_asset_weight_config(
                    assets.backs.len(),
                    &mut assets.back_weights,
                    weights,
                    multipliers,
                ),
                AssetWeightKind::Packaging => {
                    ensure!(
                        assets.packaging_fronts.len() == assets.packaging_backs.len(),
                        Error::<T>::SeasonCollectionIncomplete
                    );
                    Self::set_asset_weight_config(
                        assets.packaging_fronts.len(),
                        &mut assets.packaging_weights,
                        weights,
                        multipliers,
                    )
                }
            }
        }

        fn move_list_entry<Value: Clone, ListLen: Get<u32>>(
            list: &mut BoundedVec<Value, ListLen>,
            old_index: usize,
            new_index: usize,
        ) -> Result<(), DispatchError> {
            ensure!(old_index < list.len(), Error::<T>::AssetIndexOutOfBounds);
            ensure!(new_index < list.len(), Error::<T>::AssetIndexOutOfBounds);
            if old_index == new_index {
                return Ok(());
            }

            let mut reordered: Vec<Value> = list.iter().cloned().collect();
            let value = reordered.remove(old_index);
            reordered.insert(new_index, value);
            *list = reordered
                .try_into()
                .map_err(|_| Error::<T>::AssetListFull)?;
            Ok(())
        }

        fn move_asset_weight_config_entry<ListLen: Get<u32>>(
            config: &mut AssetWeightConfig<
                BoundedVec<WeightPercentage, ListLen>,
                BoundedVec<WeightMultiplier, ListLen>,
            >,
            old_index: usize,
            new_index: usize,
        ) -> DispatchResult {
            if config.weights.is_empty() && config.multipliers.is_empty() {
                return Ok(());
            }

            ensure!(
                config.weights.len() == config.multipliers.len(),
                Error::<T>::AssetWeightCountMismatch
            );
            Self::move_list_entry(&mut config.weights, old_index, new_index)?;
            Self::move_list_entry(&mut config.multipliers, old_index, new_index)?;
            Ok(())
        }

        fn normalized_weight_points(asset_count: usize, index: usize) -> u32 {
            if asset_count == 0 {
                return 0;
            }
            let asset_count_u32 = asset_count as u32;
            let base = NORMALIZED_WEIGHT_POINTS / asset_count_u32;
            let remainder = NORMALIZED_WEIGHT_POINTS % asset_count_u32;
            base + u32::from((index as u32) < remainder)
        }

        fn effective_weight_points<ListLen: Get<u32>>(
            config: &AssetWeightConfig<
                BoundedVec<WeightPercentage, ListLen>,
                BoundedVec<WeightMultiplier, ListLen>,
            >,
            asset_count: usize,
            index: usize,
        ) -> u32 {
            if config.weights.is_empty() || config.multipliers.is_empty() {
                return Self::normalized_weight_points(asset_count, index)
                    .saturating_mul(DEFAULT_WEIGHT_MULTIPLIER as u32);
            }

            (config.weights.get(index).copied().unwrap_or(0) as u32)
                .saturating_mul(NORMALIZED_WEIGHT_POINTS / WEIGHT_TOTAL_PERCENT)
                .saturating_mul(config.multipliers.get(index).copied().unwrap_or(0) as u32)
        }

        fn remove_asset_from_list<ListLen: Get<u32>>(
            list: &mut BoundedVec<MediaId, ListLen>,
            media_id: MediaId,
        ) -> bool {
            if let Some(pos) = list.iter().position(|&id| id == media_id) {
                list.remove(pos);
                return true;
            }
            false
        }

        fn move_asset_within_list<ListLen: Get<u32>>(
            list: &mut BoundedVec<MediaId, ListLen>,
            media_id: MediaId,
            new_index: u32,
        ) -> Result<(u32, u32), DispatchError> {
            ensure!(
                (new_index as usize) < list.len(),
                Error::<T>::AssetIndexOutOfBounds
            );
            let old_index = list
                .iter()
                .position(|&id| id == media_id)
                .ok_or(Error::<T>::AssetNotFound)? as u32;

            if new_index == old_index {
                return Ok((old_index, new_index));
            }

            let mut reordered: Vec<MediaId> = list.iter().copied().collect();
            let value = reordered.remove(old_index as usize);
            let insert_at = new_index as usize;
            ensure!(
                insert_at <= reordered.len(),
                Error::<T>::AssetIndexOutOfBounds
            );
            reordered.insert(insert_at, value);
            *list = reordered
                .try_into()
                .map_err(|_| Error::<T>::AssetListFull)?;

            Ok((old_index, new_index))
        }

        fn append_media_pool_with_weights<ListLen: Get<u32>>(
            pool: &mut Vec<SelectedSeasonAsset>,
            collection_id: SeasonCollectionId,
            media_ids: &BoundedVec<MediaId, ListLen>,
            config: &AssetWeightConfig<
                BoundedVec<WeightPercentage, ListLen>,
                BoundedVec<WeightMultiplier, ListLen>,
            >,
        ) {
            let asset_count = media_ids.len();
            for (index, media_id) in media_ids.iter().copied().enumerate() {
                pool.push(SelectedSeasonAsset {
                    collection_id,
                    media_id,
                    selection_weight: Self::effective_weight_points(config, asset_count, index),
                });
            }
        }

        fn append_packaging_pool_with_weights(
            pool: &mut Vec<SelectedPackagingAsset>,
            collection_id: SeasonCollectionId,
            assets: &SeasonAssetsInfoOf<T>,
        ) {
            let asset_count = assets
                .packaging_fronts
                .len()
                .min(assets.packaging_backs.len());
            for (index, (front_media_id, back_media_id)) in assets
                .packaging_fronts
                .iter()
                .copied()
                .zip(assets.packaging_backs.iter().copied())
                .enumerate()
            {
                pool.push(SelectedPackagingAsset {
                    collection_id,
                    front_media_id,
                    back_media_id,
                    selection_weight: Self::effective_weight_points(
                        &assets.packaging_weights,
                        asset_count,
                        index,
                    ),
                });
            }
        }

        fn published_season_asset_pools(
            season_id: SeasonId,
        ) -> Result<PublishedSeasonAssetPools, DispatchError> {
            let mut pools = PublishedSeasonAssetPools::default();

            for collection_id in SeasonCollectionIds::<T>::get(season_id) {
                let is_published = matches!(
                    SeasonCollections::<T>::get(season_id, collection_id)
                        .map(|collection| collection.status),
                    Some(SeasonCollectionStatus::Published)
                );
                if !is_published {
                    continue;
                }

                let assets = SeasonCollectionAssets::<T>::get(season_id, collection_id);
                Self::ensure_collection_asset_weights_valid(&assets)?;
                Self::append_media_pool_with_weights(
                    &mut pools.borders,
                    collection_id,
                    &assets.borders,
                    &assets.border_weights,
                );
                Self::append_media_pool_with_weights(
                    &mut pools.backgrounds,
                    collection_id,
                    &assets.backgrounds,
                    &assets.background_weights,
                );
                Self::append_media_pool_with_weights(
                    &mut pools.subjects,
                    collection_id,
                    &assets.subjects,
                    &assets.subject_weights,
                );
                Self::append_media_pool_with_weights(
                    &mut pools.backs,
                    collection_id,
                    &assets.backs,
                    &assets.back_weights,
                );
                Self::append_packaging_pool_with_weights(
                    &mut pools.packagings,
                    collection_id,
                    &assets,
                );
            }

            Ok(pools)
        }

        pub fn ensure_season_ready_for_activation(season_id: SeasonId) -> DispatchResult {
            let pools = Self::published_season_asset_pools(season_id)?;
            Self::ensure_required_season_pools(&pools)
        }

        fn random_u32(bytes: &[u8], offset: usize) -> u32 {
            let b0 = bytes.get(offset).copied().unwrap_or(0);
            let b1 = bytes.get(offset + 1).copied().unwrap_or(0);
            let b2 = bytes.get(offset + 2).copied().unwrap_or(0);
            let b3 = bytes.get(offset + 3).copied().unwrap_or(0);
            u32::from_le_bytes([b0, b1, b2, b3])
        }

        fn select_weighted_item<Item, F>(
            items: &[Item],
            random: u32,
            mut weight_of: F,
        ) -> Result<&Item, DispatchError>
        where
            F: FnMut(&Item) -> u32,
        {
            let total_weight = items
                .iter()
                .fold(0u64, |sum, item| sum.saturating_add(weight_of(item) as u64));
            ensure!(total_weight > 0, Error::<T>::AssetWeightMultiplierInvalid);

            let mut remaining = (random as u64) % total_weight;
            for item in items {
                let weight = weight_of(item) as u64;
                if weight == 0 {
                    continue;
                }
                if remaining < weight {
                    return Ok(item);
                }
                remaining = remaining.saturating_sub(weight);
            }

            items
                .iter()
                .rev()
                .find(|item| weight_of(item) > 0)
                .ok_or(Error::<T>::AssetWeightMultiplierInvalid.into())
        }

        fn assign_artwork_from_active_season(card_id: u32) -> DispatchResult {
            let season_id = pallet_eterra_seasons::ActiveSeasonId::<T>::get()
                .ok_or(Error::<T>::NoActiveSeason)?;
            Self::assign_artwork_for_card(card_id, season_id, b"eterra-tcg/art")
        }

        fn assign_starter_artwork_from_active_season(
            card_id: u32,
            subject_media_id: MediaId,
        ) -> DispatchResult {
            let season_id = pallet_eterra_seasons::ActiveSeasonId::<T>::get()
                .ok_or(Error::<T>::NoActiveSeason)?;
            Self::assign_artwork_for_card_with_subject(
                card_id,
                season_id,
                b"eterra-tcg/starter-art",
                subject_media_id,
            )
        }

        fn assign_artwork_for_card(
            card_id: u32,
            season_id: SeasonId,
            domain: &'static [u8],
        ) -> DispatchResult {
            let parent_hash = <frame_system::Pallet<T>>::parent_hash();
            let ext_index = <frame_system::Pallet<T>>::extrinsic_index().unwrap_or(0);
            let now = <frame_system::Pallet<T>>::block_number();

            let subject = (domain, season_id, card_id, now, parent_hash, ext_index).encode();
            let hash = T::Hashing::hash(&subject);
            let bytes = hash.as_ref();

            let pools = Self::published_season_asset_pools(season_id)?;
            Self::ensure_required_card_art_pools(&pools)?;

            let border_selection =
                *Self::select_weighted_item(&pools.borders, Self::random_u32(bytes, 0), |item| {
                    item.selection_weight
                })?;
            let background_selection = *Self::select_weighted_item(
                &pools.backgrounds,
                Self::random_u32(bytes, 4),
                |item| item.selection_weight,
            )?;
            let subject_selection =
                *Self::select_weighted_item(&pools.subjects, Self::random_u32(bytes, 8), |item| {
                    item.selection_weight
                })?;
            let back_selection =
                *Self::select_weighted_item(&pools.backs, Self::random_u32(bytes, 12), |item| {
                    item.selection_weight
                })?;

            CardArtwork::<T>::insert(
                card_id,
                CardArtworkInfo {
                    season_id,
                    border_media_id: border_selection.media_id,
                    background_media_id: background_selection.media_id,
                    subject_media_id: subject_selection.media_id,
                    back_media_id: back_selection.media_id,
                },
            );
            CardArtworkCollectionId::<T>::insert(card_id, subject_selection.collection_id);
            Ok(())
        }

        fn assign_artwork_for_card_with_subject(
            card_id: u32,
            season_id: SeasonId,
            domain: &'static [u8],
            subject_media_id: MediaId,
        ) -> DispatchResult {
            let parent_hash = <frame_system::Pallet<T>>::parent_hash();
            let ext_index = <frame_system::Pallet<T>>::extrinsic_index().unwrap_or(0);
            let now = <frame_system::Pallet<T>>::block_number();

            let subject = (domain, season_id, card_id, now, parent_hash, ext_index).encode();
            let hash = T::Hashing::hash(&subject);
            let bytes = hash.as_ref();

            let pools = Self::published_season_asset_pools(season_id)?;
            Self::ensure_required_card_art_pools(&pools)?;

            let border_selection =
                *Self::select_weighted_item(&pools.borders, Self::random_u32(bytes, 0), |item| {
                    item.selection_weight
                })?;
            let background_selection = *Self::select_weighted_item(
                &pools.backgrounds,
                Self::random_u32(bytes, 4),
                |item| item.selection_weight,
            )?;
            let subject_selection = *pools
                .subjects
                .iter()
                .find(|item| item.media_id == subject_media_id)
                .ok_or(Error::<T>::AssetNotFound)?;
            let back_selection =
                *Self::select_weighted_item(&pools.backs, Self::random_u32(bytes, 12), |item| {
                    item.selection_weight
                })?;

            CardArtwork::<T>::insert(
                card_id,
                CardArtworkInfo {
                    season_id,
                    border_media_id: border_selection.media_id,
                    background_media_id: background_selection.media_id,
                    subject_media_id: subject_selection.media_id,
                    back_media_id: back_selection.media_id,
                },
            );
            CardArtworkCollectionId::<T>::insert(card_id, subject_selection.collection_id);
            Ok(())
        }

        /// Create a brand-new card with `owner`.
        fn create_new_card(owner: &T::AccountId) -> Result<u32, DispatchError> {
            Self::ensure_can_receive_cards(owner, 1)?;
            let card_id = NextCardId::<T>::get();
            let next_card_id = card_id.checked_add(1).ok_or(Error::<T>::CardIdExhausted)?;
            let new_card_info = CardInfo {
                owner: owner.clone(),
                finalized: false,
                slot_values: None,
            };

            Cards::<T>::insert(card_id, new_card_info);
            Self::record_card_mint(card_id, owner);
            CardsByOwner::<T>::try_mutate(owner, |set| -> Result<(), DispatchError> {
                set.try_insert(card_id)
                    .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;
                Ok(())
            })?;
            NextCardId::<T>::put(next_card_id);

            Self::assign_artwork_from_active_season(card_id)?;
            let _ = Self::ensure_card_genome(card_id)?;
            Self::try_assign_progression_tree_for_card(card_id)?;
            Ok(card_id)
        }

        /// Create a brand-new, immediately-finalized starter card with fixed template ranks.
        fn create_starter_card_from_template(
            owner: &T::AccountId,
            template: &StarterCardTemplate,
        ) -> Result<u32, DispatchError> {
            let values = Self::starter_slot_values(template.base_ranks)?;
            let card_id = NextCardId::<T>::get();
            let next_card_id = card_id.checked_add(1).ok_or(Error::<T>::CardIdExhausted)?;
            let now = <frame_system::Pallet<T>>::block_number();

            Cards::<T>::insert(
                card_id,
                CardInfo {
                    owner: owner.clone(),
                    finalized: true,
                    slot_values: Some(values),
                },
            );
            Self::record_card_mint(card_id, owner);
            CardsByOwner::<T>::try_mutate(owner, |set| -> Result<(), DispatchError> {
                set.try_insert(card_id)
                    .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;
                Ok(())
            })?;
            NextCardId::<T>::put(next_card_id);

            Self::assign_starter_artwork_from_active_season(
                card_id,
                template.subject_id.saturated_into::<MediaId>(),
            )?;
            let _ = Self::ensure_card_genome(card_id)?;

            NexusCollectionCards::<T>::insert(
                card_id,
                CollectionCard {
                    owner: owner.clone(),
                    subject_id: template.subject_id,
                    kind: NexusCardKind::Echo,
                    origin: NexusCardOrigin::StarterGrant,
                    base_ranks: template.base_ranks,
                    apex_side: template.apex_side,
                    genes: template.genes,
                    element_profile: template.element_profile,
                    card_power: template.card_power,
                    location: NexusStorageLocation::Collection,
                    account_bound: true,
                    acquired_at: now,
                    config_version: template.config_version,
                },
            );
            Self::try_assign_progression_tree_for_card(card_id)?;

            Self::deposit_event(Event::NexusCardClaimed {
                account_id: owner.clone(),
                card_record_id: card_id,
                subject_id: template.subject_id,
                source: NexusCardOrigin::StarterGrant,
                config_version: template.config_version,
            });
            Self::deposit_event(Event::RankSlotResolved {
                card_record_id: card_id,
                base_ranks: template.base_ranks,
                apex_side: template.apex_side,
                style_label: template.style_label,
                card_power: template.card_power,
                config_version: template.config_version,
            });
            Self::deposit_event(Event::GenesResolved {
                card_record_id: card_id,
                genes: template.genes,
                element_profile: template.element_profile,
                config_version: template.config_version,
            });

            Ok(card_id)
        }

        /// Create a brand-new, immediately-finalized card with `owner`.
        fn create_new_finalized_card(owner: &T::AccountId) -> Result<u32, DispatchError> {
            Self::ensure_can_receive_cards(owner, 1)?;
            let card_id = NextCardId::<T>::get();
            let next_card_id = card_id.checked_add(1).ok_or(Error::<T>::CardIdExhausted)?;

            let values = Self::spin_values(owner, card_id, 0, b"eterra-tcg/mint-card");
            let new_card_info = CardInfo {
                owner: owner.clone(),
                finalized: true,
                slot_values: Some(values),
            };

            Cards::<T>::insert(card_id, new_card_info);
            Self::record_card_mint(card_id, owner);
            CardsByOwner::<T>::try_mutate(owner, |set| -> Result<(), DispatchError> {
                set.try_insert(card_id)
                    .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;
                Ok(())
            })?;
            NextCardId::<T>::put(next_card_id);

            Self::assign_artwork_from_active_season(card_id)?;
            let _ = Self::ensure_card_genome(card_id)?;
            Self::try_assign_progression_tree_for_card(card_id)?;
            Ok(card_id)
        }

        /// Internal: remove a card from marketplace listings, updating indices.
        fn unlist(card_id: u32, owner: &T::AccountId) {
            CardPrices::<T>::remove(card_id);
            ListedByOwner::<T>::mutate(owner, |set| {
                set.remove(&card_id);
            });
            Self::deposit_event(Event::CardUnlisted {
                owner: owner.clone(),
                card_id,
            });
        }

        /// Internal: transfer ownership from `from` to `to` and ensure indices are updated.
        fn do_transfer(from: &T::AccountId, to: &T::AccountId, card_id: u32) -> DispatchResult {
            let ignore_receiver_capacity = from != to && *to == Self::escrow_account_id();
            Self::do_transfer_with_options(from, to, card_id, ignore_receiver_capacity)
        }

        fn do_transfer_with_options(
            from: &T::AccountId,
            to: &T::AccountId,
            card_id: u32,
            ignore_receiver_capacity: bool,
        ) -> DispatchResult {
            Self::ensure_legacy_card_not_frozen(card_id)?;
            ensure!(
                !T::HandChecker::is_card_in_current_hand(from, card_id),
                Error::<T>::CardInCurrentHand
            );

            if from != to && !ignore_receiver_capacity {
                Self::ensure_can_receive_cards(to, 1)?;
            }

            // Update the card owner in main storage (ensures existence and ownership)
            Cards::<T>::try_mutate(card_id, |maybe_card| -> DispatchResult {
                let card_info = maybe_card.as_mut().ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == *from, Error::<T>::NotCardOwner);
                ensure!(card_info.finalized, Error::<T>::CardNotFinalized);
                card_info.owner = to.clone();
                Ok(())
            })?;

            // Remove card_id from `from`'s CardsByOwner set, then insert into `to`'s.
            CardsByOwner::<T>::mutate(from, |set| {
                set.remove(&card_id);
            });
            CardsByOwner::<T>::try_mutate(to, |set| -> DispatchResult {
                set.try_insert(card_id)
                    .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;
                Ok(())
            })?;

            Self::clear_magic_loadout_on_owner_change(card_id, from, to);

            Ok(())
        }

        fn move_nexus_owner_indexes(
            card_id: u32,
            from: &T::AccountId,
            to: &T::AccountId,
        ) -> DispatchResult {
            if from == to {
                return Ok(());
            }
            NexusCollectionCards::<T>::try_mutate(card_id, |maybe| -> DispatchResult {
                let Some(record) = maybe.as_mut() else {
                    return Ok(());
                };
                ensure!(
                    record.owner == *from,
                    Error::<T>::V16MigrationInvariantFailed
                );
                match record.location {
                    NexusStorageLocation::Collection | NexusStorageLocation::Vault => {
                        NexusSubjectCopyCounts::<T>::try_mutate(
                            from,
                            record.subject_id,
                            |count| -> DispatchResult {
                                *count = count
                                    .checked_sub(1)
                                    .ok_or(Error::<T>::V16MigrationInvariantFailed)?;
                                Ok(())
                            },
                        )?;
                        NexusSubjectCopyCounts::<T>::try_mutate(
                            to,
                            record.subject_id,
                            |count| -> DispatchResult {
                                *count =
                                    count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                                Ok(())
                            },
                        )?;
                    }
                    NexusStorageLocation::Overflow => {
                        NexusOverflowCards::<T>::try_mutate(from, |cards| -> DispatchResult {
                            let index = cards
                                .iter()
                                .position(|candidate| *candidate == card_id)
                                .ok_or(Error::<T>::V16MigrationInvariantFailed)?;
                            cards.remove(index);
                            Ok(())
                        })?;
                        NexusOverflowSubjectCounts::<T>::try_mutate(
                            from,
                            record.subject_id,
                            |count| -> DispatchResult {
                                *count = count
                                    .checked_sub(1)
                                    .ok_or(Error::<T>::V16MigrationInvariantFailed)?;
                                Ok(())
                            },
                        )?;
                        NexusOverflowCards::<T>::try_mutate(to, |cards| -> DispatchResult {
                            cards
                                .try_push(card_id)
                                .map_err(|_| Error::<T>::NexusOverflowCapacityExceeded)?;
                            Ok(())
                        })?;
                        NexusOverflowSubjectCounts::<T>::try_mutate(
                            to,
                            record.subject_id,
                            |count| -> DispatchResult {
                                *count =
                                    count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                                Ok(())
                            },
                        )?;
                    }
                }
                record.owner = to.clone();
                Ok(())
            })
        }

        fn transition_v16_beneficial_owner(
            card_id: u32,
            expected_owner: &T::AccountId,
            new_owner: &T::AccountId,
            custody: LegacyCustodyKind,
            move_nexus_owner: bool,
        ) -> DispatchResult {
            let Some(mut classification) = LegacyCardClassifications::<T>::get(card_id) else {
                return Ok(());
            };
            ensure!(
                !classification.frozen
                    && classification.custody != LegacyCustodyKind::UnknownFrozen
                    && classification.beneficial_owner.as_ref() == Some(expected_owner),
                Error::<T>::LegacyCardFrozen
            );
            let nexus = NexusCollectionCards::<T>::get(card_id);
            RepairedLegacyCardsByOwnerV16::<T>::remove(expected_owner, card_id);
            if let Some(record) = nexus.as_ref() {
                RepairedLegacySubjectCountsV16::<T>::try_mutate(
                    expected_owner,
                    record.subject_id,
                    |count| -> DispatchResult {
                        *count = count
                            .checked_sub(1)
                            .ok_or(Error::<T>::V16MigrationInvariantFailed)?;
                        Ok(())
                    },
                )?;
            }
            if move_nexus_owner {
                Self::move_nexus_owner_indexes(card_id, expected_owner, new_owner)?;
            }
            RepairedLegacyCardsByOwnerV16::<T>::insert(new_owner, card_id, true);
            if let Some(record) = NexusCollectionCards::<T>::get(card_id) {
                RepairedLegacySubjectCountsV16::<T>::try_mutate(
                    new_owner,
                    record.subject_id,
                    |count| -> DispatchResult {
                        *count = count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                        Ok(())
                    },
                )?;
            }
            classification.beneficial_owner = Some(new_owner.clone());
            classification.custody = custody;
            classification.frozen = false;
            classification.record_hash = Self::hash_encoded(&(
                b"ETERRA_LEGACY_V1_CLASSIFICATION",
                card_id,
                Cards::<T>::get(card_id),
                NexusCollectionCards::<T>::get(card_id),
                custody,
                new_owner,
            ));
            LegacyCardClassifications::<T>::insert(card_id, classification);
            Ok(())
        }

        pub fn move_card_to_external_escrow(
            owner: &T::AccountId,
            escrow_account: &T::AccountId,
            card_id: u32,
        ) -> Result<CardGenomeHash, DispatchError> {
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_legacy_card_not_frozen(card_id)?;
            ensure!(
                !Converted::<T>::contains_key(card_id),
                Error::<T>::CardAlreadyConverted
            );

            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == *owner, Error::<T>::NotCardOwner);
            ensure!(card_info.finalized, Error::<T>::CardNotFinalized);
            Self::ensure_card_not_account_bound(card_id)?;
            ensure!(
                !T::HandChecker::is_card_in_current_hand(owner, card_id),
                Error::<T>::CardInCurrentHand
            );

            if CardPrices::<T>::contains_key(card_id) {
                Self::unlist(card_id, owner);
            }

            let genome = Self::ensure_card_genome(card_id)?;
            Self::do_transfer_with_options(owner, escrow_account, card_id, true)?;
            Self::transition_v16_beneficial_owner(
                card_id,
                owner,
                owner,
                LegacyCustodyKind::KnownEscrow,
                false,
            )?;
            Ok(genome)
        }

        pub fn move_card_from_external_escrow(
            escrow_account: &T::AccountId,
            owner: &T::AccountId,
            card_id: u32,
        ) -> DispatchResult {
            Self::ensure_legacy_writes_allowed()?;
            Self::ensure_legacy_card_not_frozen(card_id)?;
            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(
                card_info.owner == *escrow_account,
                Error::<T>::CardNotEscrowed
            );
            Self::do_transfer_with_options(escrow_account, owner, card_id, false)?;
            Self::transition_v16_beneficial_owner(
                card_id,
                owner,
                owner,
                LegacyCustodyKind::Ordinary,
                false,
            )
        }

        /// Generate new ranks for a card based on on-chain entropy + (player, card_id, attempts).
        fn spin_values(
            player: &T::AccountId,
            card_id: u32,
            attempts: u8,
            domain: &'static [u8],
        ) -> [u8; 4] {
            let parent_hash = <frame_system::Pallet<T>>::parent_hash();
            let ext_index = <frame_system::Pallet<T>>::extrinsic_index().unwrap_or(0);
            let now = <frame_system::Pallet<T>>::block_number();

            let subject = (
                domain,
                now,
                parent_hash,
                ext_index,
                player,
                card_id,
                attempts,
            )
                .encode();
            let hash = T::Hashing::hash(&subject);
            let bytes = hash.as_ref();

            // Map bytes into a small "rank" range (1..=9) for game-friendly stats.
            let to_rank = |b: u8| -> u8 { (b % 9).saturating_add(1) };
            [
                to_rank(bytes.first().copied().unwrap_or(0)),
                to_rank(bytes.get(1).copied().unwrap_or(0)),
                to_rank(bytes.get(2).copied().unwrap_or(0)),
                to_rank(bytes.get(3).copied().unwrap_or(0)),
            ]
        }

        /// Execute a single pro spin, updating storage. Returns:
        /// - values (new ranks)
        /// - spins_used (after increment)
        /// - forced_finalized (true if this spin hit max and finalized)
        fn do_pro_spin(
            player: &T::AccountId,
            card_id: u32,
        ) -> Result<([u8; 4], u8, bool), DispatchError> {
            // Validate card ownership and in-progress state.
            let mut card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == *player, Error::<T>::NotCardOwner);
            ensure!(!card_info.finalized, Error::<T>::CardAlreadyFinalized);

            let mut spins_used = CardAttempts::<T>::get(card_id);
            ensure!(
                spins_used < T::MaxProSpins::get(),
                Error::<T>::MaxProSpinsExceeded
            );

            let values = Self::spin_values(player, card_id, spins_used, b"eterra-tcg/pro-spin");
            card_info.slot_values = Some(values);
            Cards::<T>::insert(card_id, card_info);

            spins_used = spins_used.saturating_add(1);
            CardAttempts::<T>::insert(card_id, spins_used);

            if spins_used == T::MaxProSpins::get() {
                // Auto-finalize on the last allowed spin.
                Self::finalize_pro_card(player, card_id)?;
                return Ok((values, spins_used, true));
            }

            Ok((values, spins_used, false))
        }

        fn finalize_pro_card(player: &T::AccountId, card_id: u32) -> DispatchResult {
            Cards::<T>::mutate(card_id, |maybe_card| -> DispatchResult {
                let card_info = maybe_card.as_mut().ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == *player, Error::<T>::NotCardOwner);
                ensure!(!card_info.finalized, Error::<T>::CardAlreadyFinalized);
                card_info.finalized = true;
                Ok(())
            })?;

            CardAttempts::<T>::remove(card_id);
            ProInProgress::<T>::remove(player);
            Ok(())
        }

        /// Finalize a card, remove attempts, possibly mark the pack completed, etc.
        fn internal_finalize_card(card_id: u32, pack: &mut Pack) -> DispatchResult {
            // 1) Mark card as finalized, remove attempts
            if let Some(mut card_info) = Cards::<T>::get(card_id) {
                card_info.finalized = true;
                Cards::<T>::insert(card_id, card_info);
            }

            CardAttempts::<T>::remove(card_id);

            // 2) Emit
            Self::deposit_event(Event::SlotFinalized { card_id });

            // 3) If this was the last card in the pack, set `pack.completed = true`.
            //    We'll check if all of them are finalized:
            let all_final = pack
                .card_ids
                .iter()
                .all(|id| Cards::<T>::get(*id).map(|c| c.finalized).unwrap_or(true));
            if all_final {
                pack.completed = true;
                // The user might be stored somewhere else, so we can’t easily remove
                // it here. But if the user minted the pack, they're the pack "owner".
                // If you want to store pack ownership, you'd embed that in `Pack` too.

                // For demonstration, we just say the pack is completed, but not
                // removed from the user’s `PlayerPacks`.
                // If you want an event:
                // Self::deposit_event(Event::PackCompleted {
                //   player: ???,
                //   pack_id: pack.id
                // });
            }

            Ok(())
        }

        /// Finalize the current card and advance the active card index (or complete the pack).
        fn finalize_card_and_advance(
            player: &T::AccountId,
            card_id: u32,
            pack: &mut Pack,
            active_card_idx: u8,
        ) -> DispatchResult {
            Self::internal_finalize_card(card_id, pack)?;

            let mut next_idx: Option<u8> = None;
            let start = (active_card_idx as usize).saturating_add(1);
            let len = pack.card_ids.len();

            for i in start..len {
                let cid = pack.card_ids[i];
                if let Some(info) = Cards::<T>::get(cid) {
                    if !info.finalized {
                        next_idx = Some(i as u8);
                        break;
                    }
                }
            }

            if next_idx.is_none() {
                for i in 0..start.min(len) {
                    let cid = pack.card_ids[i];
                    if let Some(info) = Cards::<T>::get(cid) {
                        if !info.finalized {
                            next_idx = Some(i as u8);
                            break;
                        }
                    }
                }
            }

            if let Some(idx) = next_idx {
                pack.active_card_index = idx;
                ActiveCard::<T>::insert(player, Some(idx));
                if let Some(cid) = pack.card_ids.get(idx as usize) {
                    PackCardInProgress::<T>::insert(player, *cid);
                }
            } else {
                pack.completed = true;
                ActiveCard::<T>::insert(player, Option::<u8>::None);
                PackInProgress::<T>::remove(player);
                PackCardInProgress::<T>::remove(player);
                Self::deposit_event(Event::PackCompleted {
                    player: player.clone(),
                    pack_id: pack.id,
                });
            }

            Ok(())
        }
    }

    impl<T: Config> crate::V2PackCreditManager<T::AccountId> for Pallet<T> {
        fn issue_credit(
            owner: &T::AccountId,
            pack_sku: u32,
            sku_version: u32,
            realm: EconomicRealm,
            source: PackCreditSource,
        ) -> frame_support::dispatch::DispatchResult {
            Self::do_issue_credit(owner, pack_sku, sku_version, realm, source).map(|_| ())
        }
    }
}
