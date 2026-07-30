#![cfg_attr(not(feature = "std"), no_std)]
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

use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ExistenceRequirement, Get},
    BoundedBTreeSet, BoundedVec, PalletId,
};
use frame_system::{ensure_root, ensure_signed, pallet_prelude::OriginFor};
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
    use sp_runtime::traits::StaticLookup;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(15);
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
    // Events
    // ------------------

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A new pack was minted for `player` with ID `pack_id`, containing multiple new cards.
        PackMinted { player: T::AccountId, pack_id: u32 },
        /// A single card was minted for `player` with ID `card_id`.
        CardMinted { player: T::AccountId, card_id: u32 },
        /// A card’s slot was generated.
        SlotGenerated { card_id: u32, values: [u8; 4] },
        /// A card’s slot was accepted (finalized).
        SlotAccepted { card_id: u32 },
        /// A card was finalized (forced finalize).
        SlotFinalized { card_id: u32 },
        /// A pack was completed (all cards finalized).
        PackCompleted { player: T::AccountId, pack_id: u32 },
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
        CardUnlisted { owner: T::AccountId, card_id: u32 },
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
        CardUnwrappedFromNft { card_id: u32 },
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
        ProMintStarted { player: T::AccountId, card_id: u32 },
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
            ensure!(
                !NexusPrizePools::<T>::contains_key(pool_id),
                Error::<T>::NexusPrizePoolAlreadyExists
            );
            ensure!(!templates.is_empty(), Error::<T>::InvalidNexusPrizePool);
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
        #[pallet::weight(<T as Config>::WeightInfo::set_card_magic_loadout())]
        #[transactional]
        pub fn set_card_magic_loadout(
            origin: OriginFor<T>,
            card_id: u32,
            spells: Vec<SpellId>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&who)?;
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
            ensure!(Cards::<T>::contains_key(card_id), Error::<T>::NoSuchCard);
            Ok(())
        }

        fn ensure_card_owner(
            card_id: u32,
            owner: &T::AccountId,
        ) -> Result<CardInfo<T::AccountId>, DispatchError> {
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

        pub fn move_card_to_external_escrow(
            owner: &T::AccountId,
            escrow_account: &T::AccountId,
            card_id: u32,
        ) -> Result<CardGenomeHash, DispatchError> {
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
            Ok(genome)
        }

        pub fn move_card_from_external_escrow(
            escrow_account: &T::AccountId,
            owner: &T::AccountId,
            card_id: u32,
        ) -> DispatchResult {
            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(
                card_info.owner == *escrow_account,
                Error::<T>::CardNotEscrowed
            );
            Self::do_transfer_with_options(escrow_account, owner, card_id, false)
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
}
