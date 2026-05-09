#![cfg_attr(not(feature = "std"), no_std)]

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
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
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

const NEXUS_BOARD_CELL_COUNT: u8 = 16;

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
    Salvaged,
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
pub enum SpellSlotKind {
    Open,
    Element(Element),
    Locked,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum ForgeBranch {
    Sword,
    Staff,
    Claw,
    Crossbow,
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
    Forge,
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

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusBoardLayout {
    pub board_id: BoardId,
    pub locked_cells: u16,
    pub mana_wells: u16,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusBoardCard<AccountId> {
    pub card_id: u32,
    pub original_owner: AccountId,
    pub controller: AccountId,
    pub ranks: [RankValue; 4],
    pub element_profile: ElementProfile,
}

#[derive(Clone, Encode, Decode, Default, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusBoardCell<AccountId> {
    pub card: Option<NexusBoardCard<AccountId>>,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusRuneCell {
    pub cell: u8,
    pub caster_card_id: u32,
    pub element: Element,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusMatchBoard<BCells, BRunes> {
    pub board_id: BoardId,
    pub locked_cells: u16,
    pub mana_wells: u16,
    pub cells: BCells,
    pub rune_cells: BRunes,
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

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusGearRecipe {
    pub slot_type: GearSlotType,
    pub tier: GearTier,
    pub power: u16,
    pub spell_slots: [SpellSlotKind; 3],
    pub cost: ResourceBundle,
    pub season_id: SeasonId,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusSpellRecipe {
    pub element: Element,
    pub power: u16,
    pub cost: ResourceBundle,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct NexusTrialSpec {
    pub trial_type: TrialType,
    pub board_id: BoardId,
    pub rewards: ResourceBundle,
}

pub trait NexusSeasonRules {
    fn season_id() -> SeasonId;
    fn ranked_team_power_limit() -> u16;
    fn board_layout(board_id: BoardId) -> Option<NexusBoardLayout>;
    fn gear_recipe(recipe_id: u32) -> Option<NexusGearRecipe>;
    fn spell_recipe(recipe_id: u32) -> Option<NexusSpellRecipe>;
    fn salvage_outputs(
        kind: NexusCardKind,
        card_power: u16,
        element_profile: ElementProfile,
    ) -> ResourceBundle;
    fn next_weapon_tier(tier: GearTier) -> Option<GearTier>;
    fn forge_cost(tier: GearTier) -> Option<ResourceBundle>;
    fn forge_gate_trial(tier: GearTier) -> Option<TrialId>;
    fn trial_spec(trial_id: TrialId) -> Option<NexusTrialSpec>;
}

pub struct NexusSeasonOneRules;

impl NexusSeasonRules for NexusSeasonOneRules {
    fn season_id() -> SeasonId {
        1
    }

    fn ranked_team_power_limit() -> u16 {
        25
    }

    fn board_layout(board_id: BoardId) -> Option<NexusBoardLayout> {
        let layout = match board_id {
            0 => NexusBoardLayout {
                board_id,
                locked_cells: 0,
                mana_wells: 0,
            },
            1 => NexusBoardLayout {
                board_id,
                locked_cells: (1u16 << 0) | (1u16 << 15),
                mana_wells: 0,
            },
            2 => NexusBoardLayout {
                board_id,
                locked_cells: (1u16 << 3) | (1u16 << 12),
                mana_wells: (1u16 << 5) | (1u16 << 10),
            },
            3 => NexusBoardLayout {
                board_id,
                locked_cells: (1u16 << 0) | (1u16 << 7) | (1u16 << 8) | (1u16 << 15),
                mana_wells: (1u16 << 5) | (1u16 << 10),
            },
            _ => return None,
        };
        Some(layout)
    }

    fn gear_recipe(recipe_id: u32) -> Option<NexusGearRecipe> {
        let mut cost = ResourceBundle::default();
        let recipe = match recipe_id {
            1 => {
                cost.gear_parts = 10;
                cost.element_shards = 2;
                NexusGearRecipe {
                    slot_type: GearSlotType::Weapon,
                    tier: GearTier::Common,
                    power: 2,
                    spell_slots: [
                        SpellSlotKind::Element(Element::Fire),
                        SpellSlotKind::Open,
                        SpellSlotKind::Locked,
                    ],
                    cost,
                    season_id: Self::season_id(),
                }
            }
            2 => {
                cost.gear_parts = 8;
                cost.element_shards = 2;
                NexusGearRecipe {
                    slot_type: GearSlotType::Armor,
                    tier: GearTier::Common,
                    power: 1,
                    spell_slots: [
                        SpellSlotKind::Element(Element::Earth),
                        SpellSlotKind::Locked,
                        SpellSlotKind::Locked,
                    ],
                    cost,
                    season_id: Self::season_id(),
                }
            }
            3 => {
                cost.gear_parts = 6;
                NexusGearRecipe {
                    slot_type: GearSlotType::Accessory,
                    tier: GearTier::Common,
                    power: 1,
                    spell_slots: [
                        SpellSlotKind::Open,
                        SpellSlotKind::Locked,
                        SpellSlotKind::Locked,
                    ],
                    cost,
                    season_id: Self::season_id(),
                }
            }
            _ => return None,
        };
        Some(recipe)
    }

    fn spell_recipe(recipe_id: u32) -> Option<NexusSpellRecipe> {
        let mut cost = ResourceBundle::default();
        cost.element_shards = 3;
        let element = match recipe_id {
            1 => Element::Fire,
            2 => Element::Earth,
            3 => Element::Water,
            4 => Element::Wind,
            _ => return None,
        };
        Some(NexusSpellRecipe {
            element,
            power: 1,
            cost,
        })
    }

    fn salvage_outputs(
        kind: NexusCardKind,
        card_power: u16,
        element_profile: ElementProfile,
    ) -> ResourceBundle {
        let mut outputs = ResourceBundle::default();
        outputs.gear_parts = 4u32.saturating_add(u32::from(card_power / 2));
        outputs.element_shards = if element_profile.minor.is_some() {
            2
        } else {
            1
        };
        if kind == NexusCardKind::Boss || card_power >= 8 {
            outputs.echo_core_fragments = 1;
        }
        outputs
    }

    fn next_weapon_tier(tier: GearTier) -> Option<GearTier> {
        match tier {
            GearTier::Common => Some(GearTier::Rare),
            GearTier::Rare => Some(GearTier::Epic),
            GearTier::Epic => Some(GearTier::Legendary),
            GearTier::Legendary => Some(GearTier::Mythical),
            GearTier::Mythical | GearTier::Basic | GearTier::Improved | GearTier::Refined => None,
        }
    }

    fn forge_cost(tier: GearTier) -> Option<ResourceBundle> {
        let mut cost = ResourceBundle::default();
        match tier {
            GearTier::Common => {
                cost.gear_parts = 15;
                cost.element_shards = 5;
            }
            GearTier::Rare => {
                cost.gear_parts = 25;
                cost.element_shards = 10;
            }
            GearTier::Epic => {
                cost.gear_parts = 40;
                cost.element_shards = 15;
                cost.forge_stars = 1;
            }
            GearTier::Legendary => {
                cost.gear_parts = 60;
                cost.element_shards = 20;
                cost.echo_cores = 1;
                cost.forge_stars = 3;
            }
            GearTier::Mythical | GearTier::Basic | GearTier::Improved | GearTier::Refined => {
                return None;
            }
        }
        Some(cost)
    }

    fn forge_gate_trial(tier: GearTier) -> Option<TrialId> {
        match tier {
            GearTier::Epic => Some(1),
            GearTier::Legendary => Some(3),
            _ => None,
        }
    }

    fn trial_spec(trial_id: TrialId) -> Option<NexusTrialSpec> {
        let mut rewards = ResourceBundle::default();
        let spec = match trial_id {
            1 => {
                rewards.gear_parts = 12;
                rewards.forge_stars = 1;
                NexusTrialSpec {
                    trial_type: TrialType::Weapon,
                    board_id: 2,
                    rewards,
                }
            }
            2 => {
                rewards.element_shards = 8;
                rewards.forge_stars = 1;
                NexusTrialSpec {
                    trial_type: TrialType::Element,
                    board_id: 2,
                    rewards,
                }
            }
            3 => {
                rewards.echo_core_fragments = 2;
                rewards.echo_cores = 1;
                rewards.forge_stars = 2;
                NexusTrialSpec {
                    trial_type: TrialType::Season,
                    board_id: 3,
                    rewards,
                }
            }
            _ => return None,
        };
        Some(spec)
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use frame_support::traits::ConstU32;
    use frame_support::transactional;
    use frame_system::pallet_prelude::BlockNumberFor;
    use pallet_alpha_access::AccessControl;
    use sp_runtime::traits::StaticLookup;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(13);
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
    type BoundedNexusVaultVariants<T> = BoundedVec<VaultVariantId, <T as Config>::MaxOwnedCards>;
    type BoundedNexusBoardCells<T> =
        BoundedVec<NexusBoardCell<<T as frame_system::Config>::AccountId>, ConstU32<16>>;
    type BoundedNexusRuneCells = BoundedVec<NexusRuneCell, ConstU32<16>>;
    type BoundedNexusSpellSlots<T> =
        BoundedVec<SpellSlotEntry, <T as Config>::MaxNexusSpellSlotsPerCard>;
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
    type CollectionCardOf<T> =
        CollectionCard<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>;
    type VaultVariantOf<T> = VaultVariant<BlockNumberFor<T>, BoundedNexusMetadataUri<T>>;
    type GearItemOf<T> =
        GearItem<<T as frame_system::Config>::AccountId, BoundedNexusSpellSlots<T>>;
    type SpellEntryOf<T> = SpellEntry<<T as frame_system::Config>::AccountId>;
    type TeamOf<T> = Team<<T as frame_system::Config>::AccountId, BoundedNexusTeamCardIds<T>>;
    type NexusMatchBoardOf<T> = NexusMatchBoard<BoundedNexusBoardCells<T>, BoundedNexusRuneCells>;
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

        /// Runtime-selected Nexus season rules and deterministic lookup tables.
        type SeasonRules: NexusSeasonRules;

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

    /// Vault Variant ids owned by account, used to bound Season 1 Vault capacity checks.
    #[pallet::storage]
    #[pallet::getter(fn nexus_vault_variants_by_owner)]
    pub type NexusVaultVariantsByOwner<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BoundedNexusVaultVariants<T>, ValueQuery>;

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

    /// Next Nexus gear id.
    #[pallet::storage]
    #[pallet::getter(fn next_nexus_gear_id)]
    pub type NextNexusGearId<T: Config> = StorageValue<_, GearId, ValueQuery>;

    /// Gear currently equipped to a card by slot type.
    #[pallet::storage]
    #[pallet::getter(fn nexus_equipped_gear)]
    pub type NexusEquippedGear<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u32,
        Blake2_128Concat,
        GearSlotType,
        GearId,
        OptionQuery,
    >;

    /// Nexus spellbook records.
    #[pallet::storage]
    #[pallet::getter(fn nexus_spell_entry)]
    pub type NexusSpellbook<T: Config> =
        StorageMap<_, Blake2_128Concat, SpellId, SpellEntryOf<T>, OptionQuery>;

    /// Next Nexus spell id.
    #[pallet::storage]
    #[pallet::getter(fn next_nexus_spell_id)]
    pub type NextNexusSpellId<T: Config> = StorageValue<_, SpellId, ValueQuery>;

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

    /// Next Nexus match id.
    #[pallet::storage]
    #[pallet::getter(fn next_nexus_match_id)]
    pub type NextNexusMatchId<T: Config> = StorageValue<_, MatchId, ValueQuery>;

    /// Runtime-authoritative 4x4 board state for each Nexus match.
    #[pallet::storage]
    #[pallet::getter(fn nexus_match_board)]
    pub type NexusMatchBoards<T: Config> =
        StorageMap<_, Blake2_128Concat, MatchId, NexusMatchBoardOf<T>, OptionQuery>;

    /// Starting five-card hand for each Nexus match player.
    #[pallet::storage]
    #[pallet::getter(fn nexus_match_hand)]
    pub type NexusMatchHands<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        MatchId,
        Blake2_128Concat,
        T::AccountId,
        BoundedNexusTeamCardIds<T>,
        ValueQuery,
    >;

    /// Cards already played by each Nexus match player.
    #[pallet::storage]
    #[pallet::getter(fn nexus_match_played_cards)]
    pub type NexusMatchPlayedCards<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        MatchId,
        Blake2_128Concat,
        T::AccountId,
        BoundedNexusTeamCardIds<T>,
        ValueQuery,
    >;

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
        /// Nexus gear was equipped.
        GearEquipped {
            account_id: T::AccountId,
            card_id: u32,
            gear_id: GearId,
            slot_type: GearSlotType,
        },
        /// Nexus gear was unequipped.
        GearUnequipped {
            account_id: T::AccountId,
            card_id: u32,
            gear_id: GearId,
            slot_type: GearSlotType,
        },
        /// A Nexus weapon advanced through a Forge path.
        WeaponForged {
            account_id: T::AccountId,
            gear_id: GearId,
            old_tier: GearTier,
            new_tier: GearTier,
            branch: ForgeBranch,
            cost: ResourceBundle,
            forge_table_version: NexusConfigVersion,
        },
        /// A Nexus seasonal weapon was reforged into a later season path.
        WeaponReforged {
            account_id: T::AccountId,
            old_gear_id: GearId,
            new_gear_id: GearId,
            season_from: SeasonId,
            season_to: SeasonId,
        },
        /// Legacy gear was attuned to a sealed Vault Variant.
        LegacyGearAttuned {
            account_id: T::AccountId,
            variant_id: VaultVariantId,
            gear_id: GearId,
        },
        /// A Nexus spell was crafted.
        SpellCrafted {
            account_id: T::AccountId,
            spell_id: SpellId,
            cost: ResourceBundle,
        },
        /// A Nexus spell was slotted into gear.
        SpellSlotted {
            account_id: T::AccountId,
            card_id: u32,
            gear_id: GearId,
            slot_index: u8,
            spell_id: SpellId,
        },
        /// A Nexus spell was removed from a gear slot.
        SpellUnslotted {
            account_id: T::AccountId,
            card_id: u32,
            gear_id: GearId,
            slot_index: u8,
            spell_id: SpellId,
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
        /// Forge Stars were granted.
        ForgeStarsGranted {
            account_id: T::AccountId,
            amount: u32,
            reason: BoundedNexusReason<T>,
            season: SeasonId,
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
        /// This action would exceed the account's configured card capacity.
        CardCapacityExceeded,
        /// Starter Grant state already exists for this account.
        NexusStarterGrantAlreadyClaimed,
        /// Nexus team must contain exactly the configured Season 1 team size.
        NexusTeamSizeInvalid,
        /// Nexus subject copy cap has been reached for Collection + Vault.
        NexusSubjectCopyCapReached,
        /// Nexus Overflow has reached its total capacity.
        NexusOverflowCapacityExceeded,
        /// Nexus Overflow has reached its per-subject capacity.
        NexusOverflowSubjectCapacityExceeded,
        /// Nexus card record does not exist.
        UnknownNexusCard,
        /// Nexus card cannot be played from its current storage location.
        NexusCardNotPlayable,
        /// Nexus team contains the same card more than once.
        NexusTeamDuplicateCard,
        /// Nexus team does not exist for the requested owner.
        NexusTeamMissing,
        /// Nexus team stored state no longer matches its current card records.
        NexusTeamStale,
        /// Nexus team power exceeds the mode limit.
        NexusTeamPowerLimitExceeded,
        /// Nexus team power could not fit in the runtime team power type.
        NexusTeamPowerOverflow,
        /// Nexus board id is not one of the curated Season 1 layouts.
        UnknownNexusBoard,
        /// Nexus board layout violates locked-cell or Mana Well constraints.
        NexusBoardLayoutInvalid,
        /// No more Nexus match ids are available.
        NexusMatchIdExhausted,
        /// Nexus match does not exist.
        NexusMatchMissing,
        /// Nexus match board does not exist.
        NexusMatchBoardMissing,
        /// Nexus match is not active.
        NexusMatchNotActive,
        /// Account is not one of the match players.
        NexusNotMatchPlayer,
        /// It is another player's turn.
        NexusNotPlayerTurn,
        /// Nexus board cell is outside the 4x4 board.
        NexusCellOutOfBounds,
        /// Nexus board cell is locked.
        NexusCellLocked,
        /// Nexus board cell is already occupied.
        NexusCellOccupied,
        /// Nexus card is not in the player's match hand.
        NexusCardNotInHand,
        /// Nexus card has already been played in this match.
        NexusCardAlreadyPlayed,
        /// Nexus Rune cast target is invalid.
        NexusInvalidRuneCast,
        /// Nexus turn counter overflowed.
        NexusMatchTurnOverflow,
        /// Nexus card must be in Collection for this action.
        NexusCardNotInCollection,
        /// Nexus Vault has reached the account's configured capacity.
        NexusVaultCapacityExceeded,
        /// Nexus metadata URI exceeds the runtime maximum.
        NexusMetadataUriTooLong,
        /// Nexus resource arithmetic overflowed.
        NexusResourceOverflow,
        /// Nexus resource balance is too low for the requested action.
        NexusResourceInsufficient,
        /// Nexus deterministic recipe id is unknown.
        NexusUnknownRecipe,
        /// No more Nexus gear ids are available.
        NexusGearIdExhausted,
        /// Nexus gear record does not exist.
        NexusGearMissing,
        /// Nexus gear is not owned by the caller.
        NexusGearNotOwned,
        /// Nexus gear is already equipped.
        NexusGearAlreadyEquipped,
        /// Nexus gear slot is already occupied on this card.
        NexusGearSlotOccupied,
        /// Nexus gear is not equipped to the requested card.
        NexusGearNotEquippedToCard,
        /// No more Nexus spell ids are available.
        NexusSpellIdExhausted,
        /// Nexus spell record does not exist.
        NexusSpellMissing,
        /// Nexus spell is not owned by the caller.
        NexusSpellNotOwned,
        /// Nexus spell is already slotted.
        NexusSpellAlreadySlotted,
        /// Nexus spell slot index is invalid.
        NexusSpellSlotInvalid,
        /// Nexus spell slot is locked.
        NexusSpellSlotLocked,
        /// Nexus spell slot is already occupied.
        NexusSpellSlotOccupied,
        /// Nexus spell element does not match the slot requirement.
        NexusSpellElementMismatch,
        /// Nexus spell is not slotted in the requested gear slot.
        NexusSpellNotSlotted,
        /// Nexus weapon cannot be advanced from its current Forge tier.
        NexusWeaponTierInvalid,
        /// Nexus Forge requires a completed Trial gate.
        NexusForgeGateMissing,
        /// Nexus Trial id is unknown or not active for this caller.
        NexusTrialMissing,
        /// Nexus Trial is already started.
        NexusTrialAlreadyStarted,
        /// Nexus Trial is already completed.
        NexusTrialAlreadyCompleted,
        /// Nexus Trial must be started before completion.
        NexusTrialNotStarted,
        /// Nexus match requires two different players.
        NexusInvalidMatchPlayers,
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

            let first_card_id = card_ids.get(0).copied();

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

        /// Record Nexus Starter Grant state for the caller.
        ///
        /// PI-01 only initializes the account/grant state skeleton. Starter card, gear,
        /// spell, and badge issuance must be implemented by later acquisition/workshop PIs
        /// once starter subject IDs and loadouts are locked in config.
        #[pallet::call_index(26)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn claim_starter_grant(origin: OriginFor<T>, path: StarterPath) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            ensure!(
                !StarterGrants::<T>::contains_key(&player),
                Error::<T>::NexusStarterGrantAlreadyClaimed
            );

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

            Self::deposit_event(Event::StarterGrantClaimed {
                account_id: player,
                path,
                grant_id,
                config_version: config.config_version,
            });
            Ok(())
        }

        /// Save a Nexus Season 1 five-card team for the caller.
        #[pallet::call_index(27)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn save_nexus_team(
            origin: OriginFor<T>,
            team_id: TeamId,
            card_ids: Vec<u32>,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::validate_nexus_team_size(card_ids.len() as u32)?;

            let (bounded_card_ids, team_power) =
                Self::validate_nexus_team_cards(&player, card_ids)?;
            let config = Self::current_nexus_config();

            NexusTeams::<T>::insert(
                &player,
                team_id,
                Team {
                    owner: player.clone(),
                    team_id,
                    card_ids: bounded_card_ids.clone(),
                    team_power,
                    config_version: config.config_version,
                },
            );

            Self::deposit_event(Event::TeamSaved {
                account_id: player,
                team_id,
                card_ids: bounded_card_ids,
                team_power,
                config_version: config.config_version,
            });
            Ok(())
        }

        /// Start a deterministic Nexus Season 1 match between two saved teams.
        #[pallet::call_index(28)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn start_nexus_match(
            origin: OriginFor<T>,
            opponent: T::AccountId,
            mode: MatchMode,
            board_id: BoardId,
            team_id: TeamId,
            opponent_team_id: TeamId,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            T::AccessControl::ensure_whitelisted(&opponent)?;
            ensure!(player != opponent, Error::<T>::NexusInvalidMatchPlayers);

            let player_team = Self::load_valid_nexus_team(&player, team_id, mode)?;
            let opponent_team = Self::load_valid_nexus_team(&opponent, opponent_team_id, mode)?;
            let layout = Self::nexus_board_layout(board_id)?;

            let match_id = NextNexusMatchId::<T>::get();
            let next_match_id = match_id
                .checked_add(1)
                .ok_or(Error::<T>::NexusMatchIdExhausted)?;
            let players: BoundedNexusMatchPlayers<T> = vec![player.clone(), opponent.clone()]
                .try_into()
                .map_err(|_| Error::<T>::NexusInvalidMatchPlayers)?;
            let first_player = Self::choose_nexus_first_player(match_id, &players)?;
            let config = Self::current_nexus_config();

            NexusMatches::<T>::insert(
                match_id,
                MatchState {
                    match_id,
                    mode,
                    board_id,
                    players: players.clone(),
                    first_player: Some(first_player.clone()),
                    status: MatchStatus::Active,
                    turn_index: 0,
                    winner: None,
                    config_version: config.config_version,
                },
            );
            NexusMatchBoards::<T>::insert(match_id, Self::empty_nexus_match_board(layout)?);
            NexusMatchHands::<T>::insert(match_id, &player, player_team.card_ids.clone());
            NexusMatchHands::<T>::insert(match_id, &opponent, opponent_team.card_ids.clone());
            NexusMatchPlayedCards::<T>::remove(match_id, &player);
            NexusMatchPlayedCards::<T>::remove(match_id, &opponent);
            NextNexusMatchId::<T>::put(next_match_id);

            Self::deposit_event(Event::TeamValidated {
                account_id: player.clone(),
                team_id,
                mode,
                team_power: player_team.team_power,
                valid: true,
            });
            Self::deposit_event(Event::TeamValidated {
                account_id: opponent.clone(),
                team_id: opponent_team_id,
                mode,
                team_power: opponent_team.team_power,
                valid: true,
            });
            Self::deposit_event(Event::MatchStarted {
                match_id,
                mode,
                board_id,
                players,
                first_player,
                config_version: config.config_version,
            });
            Ok(())
        }

        /// Place one card in a Nexus match and resolve Rune and direct capture validation.
        #[pallet::call_index(29)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn play_nexus_match_card(
            origin: OriginFor<T>,
            match_id: MatchId,
            card_id: u32,
            cell: u8,
            cast_rune: Option<(u8, Element)>,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;

            let mut match_state =
                NexusMatches::<T>::get(match_id).ok_or(Error::<T>::NexusMatchMissing)?;
            ensure!(
                match_state.status == MatchStatus::Active,
                Error::<T>::NexusMatchNotActive
            );
            ensure!(
                match_state.players.iter().any(|account| account == &player),
                Error::<T>::NexusNotMatchPlayer
            );
            let current_player = Self::current_nexus_match_player(&match_state)?;
            ensure!(current_player == player, Error::<T>::NexusNotPlayerTurn);

            let hand = NexusMatchHands::<T>::get(match_id, &player);
            ensure!(
                hand.iter().any(|id| *id == card_id),
                Error::<T>::NexusCardNotInHand
            );
            let mut played = NexusMatchPlayedCards::<T>::get(match_id, &player);
            ensure!(
                !played.iter().any(|id| *id == card_id),
                Error::<T>::NexusCardAlreadyPlayed
            );

            let collection_card = Self::ensure_playable_nexus_card(&player, card_id)?;
            let mut board =
                NexusMatchBoards::<T>::get(match_id).ok_or(Error::<T>::NexusMatchBoardMissing)?;
            Self::ensure_nexus_cell_can_receive_card(&board, cell)?;

            let (ranks, triggered_rune) = Self::trigger_nexus_rune_if_present(
                &mut board,
                cell,
                collection_card.base_ranks,
                collection_card.element_profile,
            );
            let board_card = NexusBoardCard {
                card_id,
                original_owner: player.clone(),
                controller: player.clone(),
                ranks,
                element_profile: collection_card.element_profile,
            };
            board.cells[cell as usize].card = Some(board_card);

            Self::deposit_event(Event::CardPlaced {
                match_id,
                turn_index: match_state.turn_index,
                account_id: player.clone(),
                card_id,
                cell,
            });

            if let Some((rune, effect)) = triggered_rune {
                Self::deposit_event(Event::RuneTriggered {
                    match_id,
                    turn_index: match_state.turn_index,
                    card_id,
                    well_cell: cell,
                    element: rune.element,
                    effect,
                });
            }

            let captures = Self::resolve_nexus_direct_captures(&mut board, cell, &player)?;
            for (captured_card_id, side) in captures {
                Self::deposit_event(Event::CardCaptured {
                    match_id,
                    turn_index: match_state.turn_index,
                    attacker_card_id: card_id,
                    captured_card_id,
                    side,
                });
            }

            if let Some((well_cell, element)) = cast_rune {
                Self::create_nexus_rune(&mut board, cell, card_id, well_cell, element)?;
                Self::deposit_event(Event::RuneCreated {
                    match_id,
                    turn_index: match_state.turn_index,
                    caster_card_id: card_id,
                    well_cell,
                    element,
                });
            }

            played
                .try_push(card_id)
                .map_err(|_| Error::<T>::NexusCardAlreadyPlayed)?;
            NexusMatchPlayedCards::<T>::insert(match_id, &player, played);

            let next_turn = match_state
                .turn_index
                .checked_add(1)
                .ok_or(Error::<T>::NexusMatchTurnOverflow)?;
            match_state.turn_index = next_turn;

            if Self::nexus_match_should_end(match_id, &match_state, &board) {
                let score = Self::score_nexus_match(&match_state, &board);
                let winner = Self::nexus_match_winner(&match_state, score);
                match_state.status = MatchStatus::Complete;
                match_state.winner = winner.clone();
                Self::deposit_event(Event::MatchEnded {
                    match_id,
                    winner,
                    score,
                    duration: u32::from(next_turn),
                    reward_status: false,
                });
            }

            NexusMatchBoards::<T>::insert(match_id, board);
            NexusMatches::<T>::insert(match_id, match_state);
            Ok(())
        }

        /// Salvage a Collection card into deterministic non-tradable Workshop resources.
        #[pallet::call_index(30)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn salvage_nexus_card(origin: OriginFor<T>, card_record_id: u32) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;

            let mut card = Self::ensure_nexus_card_owned(&player, card_record_id)?;
            ensure!(
                card.location == NexusStorageLocation::Collection,
                Error::<T>::NexusCardNotInCollection
            );

            let outputs = Self::nexus_salvage_outputs(&card);
            card.location = NexusStorageLocation::Salvaged;
            NexusCollectionCards::<T>::insert(card_record_id, card.clone());
            NexusSubjectCopyCounts::<T>::mutate(&player, card.subject_id, |count| {
                *count = count.saturating_sub(1);
            });
            Self::add_nexus_resources(&player, outputs)?;

            Self::deposit_event(Event::CardSalvaged {
                account_id: player,
                card_record_id,
                outputs,
                salvage_table_version: Self::current_nexus_config().config_version,
            });
            Ok(())
        }

        /// Seal a Collection card into a Vault Variant display snapshot.
        #[pallet::call_index(31)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn seal_nexus_card(
            origin: OriginFor<T>,
            card_record_id: u32,
            metadata_uri: Vec<u8>,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;

            let mut card = Self::ensure_nexus_card_owned(&player, card_record_id)?;
            ensure!(
                card.location == NexusStorageLocation::Collection,
                Error::<T>::NexusCardNotInCollection
            );
            let bounded_uri: BoundedNexusMetadataUri<T> = metadata_uri
                .try_into()
                .map_err(|_| Error::<T>::NexusMetadataUriTooLong)?;
            let capacity = NexusAccountStates::<T>::get(&player)
                .map(|state| state.vault_capacity)
                .unwrap_or_else(|| Self::current_nexus_config().base_vault_capacity);
            let mut owned_variants = NexusVaultVariantsByOwner::<T>::get(&player);
            ensure!(
                (owned_variants.len() as u32) < capacity,
                Error::<T>::NexusVaultCapacityExceeded
            );

            let variant_id = NextVaultVariantId::<T>::get();
            let next_variant_id = variant_id
                .checked_add(1)
                .ok_or(Error::<T>::CardIdExhausted)?;
            owned_variants
                .try_push(variant_id)
                .map_err(|_| Error::<T>::NexusVaultCapacityExceeded)?;

            card.location = NexusStorageLocation::Vault;
            NexusCollectionCards::<T>::insert(card_record_id, card.clone());
            VaultVariants::<T>::insert(
                variant_id,
                VaultVariant {
                    variant_id,
                    card_record_id,
                    subject_id: card.subject_id,
                    sealed_at: <frame_system::Pallet<T>>::block_number(),
                    metadata_uri: bounded_uri.clone(),
                    trade_eligible: false,
                    config_version: Self::current_nexus_config().config_version,
                },
            );
            NexusVaultVariantsByOwner::<T>::insert(&player, owned_variants);
            NextVaultVariantId::<T>::put(next_variant_id);

            Self::deposit_event(Event::CardSealed {
                account_id: player,
                card_record_id,
                variant_id,
                metadata_uri: bounded_uri,
            });
            Ok(())
        }

        /// Craft deterministic Season 1 gear from Workshop resources.
        #[pallet::call_index(32)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn craft_nexus_gear(origin: OriginFor<T>, recipe_id: u32) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;

            let (slot_type, tier, power, spell_slots, cost, season_id) =
                Self::nexus_gear_recipe(recipe_id)?;
            Self::spend_nexus_resources(&player, cost)?;
            let gear_id = NextNexusGearId::<T>::get();
            let next_gear_id = gear_id
                .checked_add(1)
                .ok_or(Error::<T>::NexusGearIdExhausted)?;
            let config = Self::current_nexus_config();

            NexusGearItems::<T>::insert(
                gear_id,
                GearItem {
                    owner: player.clone(),
                    gear_id,
                    slot_type,
                    tier,
                    power,
                    spell_slots,
                    equipped_card_id: None,
                    season_id,
                    config_version: config.config_version,
                },
            );
            NextNexusGearId::<T>::put(next_gear_id);

            Self::deposit_event(Event::GearCrafted {
                account_id: player,
                gear_id,
                recipe_id,
                cost,
                config_version: config.config_version,
            });
            Ok(())
        }

        /// Equip gear to a Nexus card build.
        #[pallet::call_index(33)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn equip_nexus_gear(
            origin: OriginFor<T>,
            card_record_id: u32,
            gear_id: GearId,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_nexus_card_buildable(&player, card_record_id)?;

            let mut gear = NexusGearItems::<T>::get(gear_id).ok_or(Error::<T>::NexusGearMissing)?;
            ensure!(gear.owner == player, Error::<T>::NexusGearNotOwned);
            ensure!(
                gear.equipped_card_id.is_none(),
                Error::<T>::NexusGearAlreadyEquipped
            );
            ensure!(
                NexusEquippedGear::<T>::get(card_record_id, gear.slot_type).is_none(),
                Error::<T>::NexusGearSlotOccupied
            );

            gear.equipped_card_id = Some(card_record_id);
            NexusGearItems::<T>::insert(gear_id, gear.clone());
            NexusEquippedGear::<T>::insert(card_record_id, gear.slot_type, gear_id);

            Self::deposit_event(Event::GearEquipped {
                account_id: player,
                card_id: card_record_id,
                gear_id,
                slot_type: gear.slot_type,
            });
            Ok(())
        }

        /// Remove gear from a Nexus card build.
        #[pallet::call_index(34)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn unequip_nexus_gear(
            origin: OriginFor<T>,
            card_record_id: u32,
            gear_id: GearId,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_nexus_card_buildable(&player, card_record_id)?;

            let mut gear = NexusGearItems::<T>::get(gear_id).ok_or(Error::<T>::NexusGearMissing)?;
            ensure!(gear.owner == player, Error::<T>::NexusGearNotOwned);
            ensure!(
                gear.equipped_card_id == Some(card_record_id),
                Error::<T>::NexusGearNotEquippedToCard
            );

            gear.equipped_card_id = None;
            NexusGearItems::<T>::insert(gear_id, gear.clone());
            NexusEquippedGear::<T>::remove(card_record_id, gear.slot_type);

            Self::deposit_event(Event::GearUnequipped {
                account_id: player,
                card_id: card_record_id,
                gear_id,
                slot_type: gear.slot_type,
            });
            Ok(())
        }

        /// Craft a deterministic Season 1 spell.
        #[pallet::call_index(35)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn craft_nexus_spell(origin: OriginFor<T>, recipe_id: u32) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;

            let (element, power, cost) = Self::nexus_spell_recipe(recipe_id)?;
            Self::spend_nexus_resources(&player, cost)?;
            let spell_id = NextNexusSpellId::<T>::get();
            let next_spell_id = spell_id
                .checked_add(1)
                .ok_or(Error::<T>::NexusSpellIdExhausted)?;
            let config = Self::current_nexus_config();

            NexusSpellbook::<T>::insert(
                spell_id,
                SpellEntry {
                    owner: player.clone(),
                    spell_id,
                    element,
                    power,
                    slotted_to: None,
                    config_version: config.config_version,
                },
            );
            NextNexusSpellId::<T>::put(next_spell_id);

            Self::deposit_event(Event::SpellCrafted {
                account_id: player,
                spell_id,
                cost,
            });
            Ok(())
        }

        /// Slot a spell into equipped gear on a Nexus card build.
        #[pallet::call_index(36)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn slot_nexus_spell(
            origin: OriginFor<T>,
            card_record_id: u32,
            gear_id: GearId,
            slot_index: u8,
            spell_id: SpellId,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_nexus_card_buildable(&player, card_record_id)?;

            let mut spell =
                NexusSpellbook::<T>::get(spell_id).ok_or(Error::<T>::NexusSpellMissing)?;
            ensure!(spell.owner == player, Error::<T>::NexusSpellNotOwned);
            ensure!(
                spell.slotted_to.is_none(),
                Error::<T>::NexusSpellAlreadySlotted
            );

            let mut gear = NexusGearItems::<T>::get(gear_id).ok_or(Error::<T>::NexusGearMissing)?;
            ensure!(gear.owner == player, Error::<T>::NexusGearNotOwned);
            ensure!(
                gear.equipped_card_id == Some(card_record_id),
                Error::<T>::NexusGearNotEquippedToCard
            );
            let slot = gear
                .spell_slots
                .get_mut(slot_index as usize)
                .ok_or(Error::<T>::NexusSpellSlotInvalid)?;
            match slot.slot_kind {
                SpellSlotKind::Locked => return Err(Error::<T>::NexusSpellSlotLocked.into()),
                SpellSlotKind::Element(required) => {
                    ensure!(
                        required == spell.element,
                        Error::<T>::NexusSpellElementMismatch
                    );
                }
                SpellSlotKind::Open => {}
            }
            ensure!(slot.spell_id.is_none(), Error::<T>::NexusSpellSlotOccupied);

            slot.spell_id = Some(spell_id);
            spell.slotted_to = Some((gear_id, slot_index));
            NexusGearItems::<T>::insert(gear_id, gear);
            NexusSpellbook::<T>::insert(spell_id, spell);

            Self::deposit_event(Event::SpellSlotted {
                account_id: player,
                card_id: card_record_id,
                gear_id,
                slot_index,
                spell_id,
            });
            Ok(())
        }

        /// Remove a spell from equipped gear.
        #[pallet::call_index(37)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn unslot_nexus_spell(
            origin: OriginFor<T>,
            card_record_id: u32,
            gear_id: GearId,
            slot_index: u8,
            spell_id: SpellId,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;
            Self::ensure_nexus_card_buildable(&player, card_record_id)?;

            let mut gear = NexusGearItems::<T>::get(gear_id).ok_or(Error::<T>::NexusGearMissing)?;
            ensure!(gear.owner == player, Error::<T>::NexusGearNotOwned);
            ensure!(
                gear.equipped_card_id == Some(card_record_id),
                Error::<T>::NexusGearNotEquippedToCard
            );
            let slot = gear
                .spell_slots
                .get_mut(slot_index as usize)
                .ok_or(Error::<T>::NexusSpellSlotInvalid)?;
            ensure!(
                slot.spell_id == Some(spell_id),
                Error::<T>::NexusSpellNotSlotted
            );

            let mut spell =
                NexusSpellbook::<T>::get(spell_id).ok_or(Error::<T>::NexusSpellMissing)?;
            ensure!(spell.owner == player, Error::<T>::NexusSpellNotOwned);
            ensure!(
                spell.slotted_to == Some((gear_id, slot_index)),
                Error::<T>::NexusSpellNotSlotted
            );

            slot.spell_id = None;
            spell.slotted_to = None;
            NexusGearItems::<T>::insert(gear_id, gear);
            NexusSpellbook::<T>::insert(spell_id, spell);

            Self::deposit_event(Event::SpellUnslotted {
                account_id: player,
                card_id: card_record_id,
                gear_id,
                slot_index,
                spell_id,
            });
            Ok(())
        }

        /// Advance a Season 1 weapon through a deterministic Forge path.
        #[pallet::call_index(38)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn forge_nexus_weapon(
            origin: OriginFor<T>,
            gear_id: GearId,
            branch: ForgeBranch,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;

            let mut gear = NexusGearItems::<T>::get(gear_id).ok_or(Error::<T>::NexusGearMissing)?;
            ensure!(gear.owner == player, Error::<T>::NexusGearNotOwned);
            ensure!(
                gear.slot_type == GearSlotType::Weapon,
                Error::<T>::NexusWeaponTierInvalid
            );
            Self::ensure_nexus_forge_gate(&player, gear.tier)?;
            let old_tier = gear.tier;
            let new_tier = Self::nexus_next_weapon_tier(old_tier)?;
            let cost = Self::nexus_forge_cost(old_tier)?;
            Self::spend_nexus_resources(&player, cost)?;

            gear.tier = new_tier;
            gear.power = gear
                .power
                .checked_add(2)
                .ok_or(Error::<T>::NexusTeamPowerOverflow)?;
            let config = Self::current_nexus_config();
            gear.config_version = config.config_version;
            NexusGearItems::<T>::insert(gear_id, gear);

            Self::deposit_event(Event::WeaponForged {
                account_id: player,
                gear_id,
                old_tier,
                new_tier,
                branch,
                cost,
                forge_table_version: config.config_version,
            });
            Ok(())
        }

        /// Start a deterministic Season 1 Trial.
        #[pallet::call_index(39)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn start_nexus_trial(origin: OriginFor<T>, trial_id: TrialId) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;

            let (trial_type, board_id, _) = Self::nexus_trial_spec(trial_id)?;
            if let Some(existing) = NexusTrials::<T>::get(&player, trial_id) {
                match existing.status {
                    TrialStatus::Started => return Err(Error::<T>::NexusTrialAlreadyStarted.into()),
                    TrialStatus::Completed => {
                        return Err(Error::<T>::NexusTrialAlreadyCompleted.into());
                    }
                    TrialStatus::Failed => {}
                }
            }
            let config = Self::current_nexus_config();
            NexusTrials::<T>::insert(
                &player,
                trial_id,
                TrialState {
                    account_id: player.clone(),
                    trial_id,
                    trial_type,
                    board_id,
                    status: TrialStatus::Started,
                    config_version: config.config_version,
                },
            );

            Self::deposit_event(Event::TrialStarted {
                account_id: player,
                trial_id,
                board_id,
            });
            Ok(())
        }

        /// Complete a started Trial and grant deterministic rewards on success.
        #[pallet::call_index(40)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        #[transactional]
        pub fn complete_nexus_trial(
            origin: OriginFor<T>,
            trial_id: TrialId,
            success: bool,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&player)?;

            let (_, _, rewards) = Self::nexus_trial_spec(trial_id)?;
            let mut trial =
                NexusTrials::<T>::get(&player, trial_id).ok_or(Error::<T>::NexusTrialMissing)?;
            ensure!(
                trial.status == TrialStatus::Started,
                Error::<T>::NexusTrialNotStarted
            );
            trial.status = if success {
                TrialStatus::Completed
            } else {
                TrialStatus::Failed
            };
            let granted = if success {
                Self::add_nexus_resources(&player, rewards)?;
                rewards
            } else {
                ResourceBundle::default()
            };
            NexusTrials::<T>::insert(&player, trial_id, trial);

            Self::deposit_event(Event::TrialCompleted {
                account_id: player.clone(),
                trial_id,
                result: if success {
                    TrialStatus::Completed
                } else {
                    TrialStatus::Failed
                },
                rewards: granted,
            });
            if granted.forge_stars > 0 {
                Self::deposit_event(Event::ForgeStarsGranted {
                    account_id: player,
                    amount: granted.forge_stars,
                    reason: Self::nexus_reason(b"trial-complete")?,
                    season: T::SeasonRules::season_id(),
                });
            }
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

        pub fn validate_nexus_team_size(card_count: u32) -> DispatchResult {
            ensure!(
                card_count == T::NexusTeamSize::get(),
                Error::<T>::NexusTeamSizeInvalid
            );
            Ok(())
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

        fn nexus_reason(reason: &'static [u8]) -> Result<BoundedNexusReason<T>, DispatchError> {
            reason
                .to_vec()
                .try_into()
                .map_err(|_| Error::<T>::NexusMetadataUriTooLong.into())
        }

        fn ensure_nexus_card_owned(
            owner: &T::AccountId,
            card_id: u32,
        ) -> Result<CollectionCardOf<T>, Error<T>> {
            let card =
                NexusCollectionCards::<T>::get(card_id).ok_or(Error::<T>::UnknownNexusCard)?;
            ensure!(&card.owner == owner, Error::<T>::NotCardOwner);
            Ok(card)
        }

        fn ensure_nexus_card_buildable(
            owner: &T::AccountId,
            card_id: u32,
        ) -> Result<CollectionCardOf<T>, Error<T>> {
            let card = Self::ensure_nexus_card_owned(owner, card_id)?;
            ensure!(
                matches!(
                    card.location,
                    NexusStorageLocation::Collection | NexusStorageLocation::Vault
                ),
                Error::<T>::NexusCardNotPlayable
            );
            Ok(card)
        }

        fn nexus_resource_amount(bundle: ResourceBundle, kind: ResourceKind) -> u32 {
            match kind {
                ResourceKind::EonCoins => bundle.eon_coins,
                ResourceKind::GearParts => bundle.gear_parts,
                ResourceKind::ElementShards => bundle.element_shards,
                ResourceKind::EchoCoreFragments => bundle.echo_core_fragments,
                ResourceKind::EchoCores => bundle.echo_cores,
                ResourceKind::ForgeStars => bundle.forge_stars,
                ResourceKind::MakeUpStamps => bundle.make_up_stamps,
            }
        }

        fn nexus_resource_kinds() -> [ResourceKind; 7] {
            [
                ResourceKind::EonCoins,
                ResourceKind::GearParts,
                ResourceKind::ElementShards,
                ResourceKind::EchoCoreFragments,
                ResourceKind::EchoCores,
                ResourceKind::ForgeStars,
                ResourceKind::MakeUpStamps,
            ]
        }

        fn add_nexus_resources(owner: &T::AccountId, bundle: ResourceBundle) -> DispatchResult {
            for kind in Self::nexus_resource_kinds() {
                let delta = Self::nexus_resource_amount(bundle, kind);
                if delta == 0 {
                    continue;
                }
                let current = NexusResources::<T>::get(owner, kind);
                let next = current
                    .checked_add(delta)
                    .ok_or(Error::<T>::NexusResourceOverflow)?;
                NexusResources::<T>::insert(owner, kind, next);
            }
            Ok(())
        }

        fn spend_nexus_resources(owner: &T::AccountId, cost: ResourceBundle) -> DispatchResult {
            for kind in Self::nexus_resource_kinds() {
                let amount = Self::nexus_resource_amount(cost, kind);
                if amount == 0 {
                    continue;
                }
                ensure!(
                    NexusResources::<T>::get(owner, kind) >= amount,
                    Error::<T>::NexusResourceInsufficient
                );
            }
            for kind in Self::nexus_resource_kinds() {
                let amount = Self::nexus_resource_amount(cost, kind);
                if amount == 0 {
                    continue;
                }
                NexusResources::<T>::mutate(owner, kind, |balance| {
                    *balance = balance.saturating_sub(amount);
                });
            }
            Ok(())
        }

        fn nexus_spell_slots(
            slot_kinds: [SpellSlotKind; 3],
        ) -> Result<BoundedNexusSpellSlots<T>, Error<T>> {
            let mut slots = BoundedNexusSpellSlots::<T>::default();
            for slot_kind in slot_kinds.iter().copied() {
                slots
                    .try_push(SpellSlotEntry {
                        slot_kind,
                        spell_id: None,
                    })
                    .map_err(|_| Error::<T>::NexusSpellSlotInvalid)?;
            }
            Ok(slots)
        }

        fn nexus_gear_recipe(
            recipe_id: u32,
        ) -> Result<
            (
                GearSlotType,
                GearTier,
                u16,
                BoundedNexusSpellSlots<T>,
                ResourceBundle,
                SeasonId,
            ),
            Error<T>,
        > {
            let recipe =
                T::SeasonRules::gear_recipe(recipe_id).ok_or(Error::<T>::NexusUnknownRecipe)?;
            let spell_slots = Self::nexus_spell_slots(recipe.spell_slots)?;
            Ok((
                recipe.slot_type,
                recipe.tier,
                recipe.power,
                spell_slots,
                recipe.cost,
                recipe.season_id,
            ))
        }

        fn nexus_spell_recipe(recipe_id: u32) -> Result<(Element, u16, ResourceBundle), Error<T>> {
            let recipe =
                T::SeasonRules::spell_recipe(recipe_id).ok_or(Error::<T>::NexusTrialMissing)?;
            Ok((recipe.element, recipe.power, recipe.cost))
        }

        fn nexus_salvage_outputs(card: &CollectionCardOf<T>) -> ResourceBundle {
            T::SeasonRules::salvage_outputs(card.kind, card.card_power, card.element_profile)
        }

        fn nexus_next_weapon_tier(tier: GearTier) -> Result<GearTier, Error<T>> {
            T::SeasonRules::next_weapon_tier(tier).ok_or(Error::<T>::NexusWeaponTierInvalid)
        }

        fn nexus_forge_cost(tier: GearTier) -> Result<ResourceBundle, Error<T>> {
            T::SeasonRules::forge_cost(tier).ok_or(Error::<T>::NexusWeaponTierInvalid)
        }

        fn ensure_nexus_trial_completed(owner: &T::AccountId, trial_id: TrialId) -> DispatchResult {
            let completed = NexusTrials::<T>::get(owner, trial_id)
                .map(|trial| trial.status == TrialStatus::Completed)
                .unwrap_or(false);
            ensure!(completed, Error::<T>::NexusForgeGateMissing);
            Ok(())
        }

        fn ensure_nexus_forge_gate(owner: &T::AccountId, tier: GearTier) -> DispatchResult {
            if let Some(trial_id) = T::SeasonRules::forge_gate_trial(tier) {
                Self::ensure_nexus_trial_completed(owner, trial_id)
            } else {
                Ok(())
            }
        }

        fn nexus_trial_spec(
            trial_id: TrialId,
        ) -> Result<(TrialType, BoardId, ResourceBundle), Error<T>> {
            let spec =
                T::SeasonRules::trial_spec(trial_id).ok_or(Error::<T>::NexusUnknownRecipe)?;
            Ok((spec.trial_type, spec.board_id, spec.rewards))
        }

        fn nexus_card_build_power(
            owner: &T::AccountId,
            card_id: u32,
            card: &CollectionCardOf<T>,
        ) -> Result<u16, Error<T>> {
            let mut power = card.card_power;
            for (_, gear_id) in NexusEquippedGear::<T>::iter_prefix(card_id) {
                let gear = NexusGearItems::<T>::get(gear_id).ok_or(Error::<T>::NexusGearMissing)?;
                ensure!(&gear.owner == owner, Error::<T>::NexusGearNotOwned);
                ensure!(
                    gear.equipped_card_id == Some(card_id),
                    Error::<T>::NexusGearNotEquippedToCard
                );
                power = power
                    .checked_add(gear.power)
                    .ok_or(Error::<T>::NexusTeamPowerOverflow)?;
                for slot in gear.spell_slots.iter() {
                    if let Some(spell_id) = slot.spell_id {
                        let spell = NexusSpellbook::<T>::get(spell_id)
                            .ok_or(Error::<T>::NexusSpellMissing)?;
                        ensure!(&spell.owner == owner, Error::<T>::NexusSpellNotOwned);
                        ensure!(spell.slotted_to.is_some(), Error::<T>::NexusSpellNotSlotted);
                        power = power
                            .checked_add(spell.power)
                            .ok_or(Error::<T>::NexusTeamPowerOverflow)?;
                    }
                }
            }
            Ok(power)
        }

        fn cell_mask(cell: u8) -> Result<u16, Error<T>> {
            ensure!(
                cell < NEXUS_BOARD_CELL_COUNT,
                Error::<T>::NexusCellOutOfBounds
            );
            Ok(1u16 << cell)
        }

        fn nexus_board_layout(board_id: BoardId) -> Result<NexusBoardLayout, Error<T>> {
            let layout =
                T::SeasonRules::board_layout(board_id).ok_or(Error::<T>::UnknownNexusBoard)?;
            Self::ensure_nexus_board_layout_valid(layout)?;
            Ok(layout)
        }

        fn ensure_nexus_board_layout_valid(layout: NexusBoardLayout) -> Result<(), Error<T>> {
            ensure!(
                layout.locked_cells & layout.mana_wells == 0,
                Error::<T>::NexusBoardLayoutInvalid
            );

            for row in 0..4u8 {
                let row_mask = (0..4u8).fold(0u16, |mask, col| mask | (1u16 << (row * 4 + col)));
                ensure!(
                    layout.locked_cells & row_mask != row_mask,
                    Error::<T>::NexusBoardLayoutInvalid
                );
            }

            for col in 0..4u8 {
                let col_mask = (0..4u8).fold(0u16, |mask, row| mask | (1u16 << (row * 4 + col)));
                ensure!(
                    layout.locked_cells & col_mask != col_mask,
                    Error::<T>::NexusBoardLayoutInvalid
                );
            }

            Ok(())
        }

        fn empty_nexus_match_board(
            layout: NexusBoardLayout,
        ) -> Result<NexusMatchBoardOf<T>, Error<T>> {
            let cells: BoundedNexusBoardCells<T> = (0..NEXUS_BOARD_CELL_COUNT)
                .map(|_| NexusBoardCell { card: None })
                .collect::<Vec<_>>()
                .try_into()
                .map_err(|_| Error::<T>::NexusBoardLayoutInvalid)?;

            Ok(NexusMatchBoard {
                board_id: layout.board_id,
                locked_cells: layout.locked_cells,
                mana_wells: layout.mana_wells,
                cells,
                rune_cells: BoundedNexusRuneCells::default(),
            })
        }

        fn ensure_nexus_cell_can_receive_card(
            board: &NexusMatchBoardOf<T>,
            cell: u8,
        ) -> Result<(), Error<T>> {
            let mask = Self::cell_mask(cell)?;
            ensure!(board.locked_cells & mask == 0, Error::<T>::NexusCellLocked);
            ensure!(
                board
                    .cells
                    .get(cell as usize)
                    .ok_or(Error::<T>::NexusCellOutOfBounds)?
                    .card
                    .is_none(),
                Error::<T>::NexusCellOccupied
            );
            Ok(())
        }

        fn ensure_playable_nexus_card(
            owner: &T::AccountId,
            card_id: u32,
        ) -> Result<CollectionCardOf<T>, Error<T>> {
            let card =
                NexusCollectionCards::<T>::get(card_id).ok_or(Error::<T>::UnknownNexusCard)?;
            ensure!(&card.owner == owner, Error::<T>::NotCardOwner);
            ensure!(
                matches!(
                    card.location,
                    NexusStorageLocation::Collection | NexusStorageLocation::Vault
                ),
                Error::<T>::NexusCardNotPlayable
            );
            Ok(card)
        }

        fn validate_nexus_team_cards(
            owner: &T::AccountId,
            card_ids: Vec<u32>,
        ) -> Result<(BoundedNexusTeamCardIds<T>, u16), Error<T>> {
            let mut unique = BoundedBTreeSet::<u32, T::NexusTeamSize>::new();
            let mut team_power: u16 = 0;

            for card_id in card_ids.iter().copied() {
                ensure!(
                    matches!(unique.try_insert(card_id), Ok(true)),
                    Error::<T>::NexusTeamDuplicateCard
                );
                let card = Self::ensure_playable_nexus_card(owner, card_id)?;
                let card_power = Self::nexus_card_build_power(owner, card_id, &card)?;
                team_power = team_power
                    .checked_add(card_power)
                    .ok_or(Error::<T>::NexusTeamPowerOverflow)?;
            }

            let bounded_card_ids: BoundedNexusTeamCardIds<T> = card_ids
                .try_into()
                .map_err(|_| Error::<T>::NexusTeamSizeInvalid)?;
            Ok((bounded_card_ids, team_power))
        }

        fn load_valid_nexus_team(
            owner: &T::AccountId,
            team_id: TeamId,
            mode: MatchMode,
        ) -> Result<TeamOf<T>, Error<T>> {
            let team = NexusTeams::<T>::get(owner, team_id).ok_or(Error::<T>::NexusTeamMissing)?;
            Self::validate_nexus_team_size(team.card_ids.len() as u32)
                .map_err(|_| Error::<T>::NexusTeamSizeInvalid)?;

            let (validated_cards, team_power) =
                Self::validate_nexus_team_cards(owner, team.card_ids.to_vec())?;
            ensure!(
                validated_cards == team.card_ids && team_power == team.team_power,
                Error::<T>::NexusTeamStale
            );
            if mode == MatchMode::Ranked {
                ensure!(
                    team_power <= T::SeasonRules::ranked_team_power_limit(),
                    Error::<T>::NexusTeamPowerLimitExceeded
                );
            }
            Ok(team)
        }

        fn choose_nexus_first_player(
            match_id: MatchId,
            players: &BoundedNexusMatchPlayers<T>,
        ) -> Result<T::AccountId, Error<T>> {
            ensure!(players.len() == 2, Error::<T>::NexusInvalidMatchPlayers);
            let now = <frame_system::Pallet<T>>::block_number();
            let seed =
                T::Hashing::hash(&(b"nexus/match/first/v1", match_id, players, now).encode());
            let index = (seed.as_ref()[0] & 1) as usize;
            players
                .get(index)
                .cloned()
                .ok_or(Error::<T>::NexusInvalidMatchPlayers)
        }

        fn current_nexus_match_player(
            match_state: &MatchStateOf<T>,
        ) -> Result<T::AccountId, Error<T>> {
            ensure!(
                match_state.players.len() == 2,
                Error::<T>::NexusInvalidMatchPlayers
            );
            let first_player = match_state
                .first_player
                .as_ref()
                .ok_or(Error::<T>::NexusInvalidMatchPlayers)?;
            let first_index = match_state
                .players
                .iter()
                .position(|account| account == first_player)
                .ok_or(Error::<T>::NexusInvalidMatchPlayers)?;
            let turn_offset = (match_state.turn_index as usize) % match_state.players.len();
            let player_index = (first_index + turn_offset) % match_state.players.len();
            match_state
                .players
                .get(player_index)
                .cloned()
                .ok_or(Error::<T>::NexusInvalidMatchPlayers)
        }

        fn rank_for_side(ranks: &[RankValue; 4], side: ApexSide) -> RankValue {
            match side {
                ApexSide::Top => ranks[0],
                ApexSide::Right => ranks[1],
                ApexSide::Bottom => ranks[2],
                ApexSide::Left => ranks[3],
            }
        }

        fn opposite_side(side: ApexSide) -> ApexSide {
            match side {
                ApexSide::Top => ApexSide::Bottom,
                ApexSide::Right => ApexSide::Left,
                ApexSide::Bottom => ApexSide::Top,
                ApexSide::Left => ApexSide::Right,
            }
        }

        fn neighboring_cell(cell: u8, side: ApexSide) -> Option<u8> {
            match side {
                ApexSide::Top if cell >= 4 => Some(cell - 4),
                ApexSide::Right if cell % 4 < 3 => Some(cell + 1),
                ApexSide::Bottom if cell < 12 => Some(cell + 4),
                ApexSide::Left if cell % 4 > 0 => Some(cell - 1),
                _ => None,
            }
        }

        fn cells_are_adjacent(left: u8, right: u8) -> bool {
            [
                ApexSide::Top,
                ApexSide::Right,
                ApexSide::Bottom,
                ApexSide::Left,
            ]
            .iter()
            .any(|side| Self::neighboring_cell(left, *side) == Some(right))
        }

        fn rank_beats(attacker: RankValue, defender: RankValue) -> bool {
            match (attacker, defender) {
                (RankValue::Apex, RankValue::Apex) => false,
                (RankValue::Apex, RankValue::Number(_)) => true,
                (RankValue::Number(_), RankValue::Apex) => false,
                (RankValue::Number(attacker), RankValue::Number(defender)) => attacker > defender,
            }
        }

        fn rune_delta_for_element(profile: ElementProfile, rune_element: Element) -> i8 {
            if profile.main == rune_element || profile.minor == Some(rune_element) {
                1
            } else if profile.weakness == Some(rune_element) {
                -1
            } else {
                0
            }
        }

        fn apply_rune_delta(mut ranks: [RankValue; 4], delta: i8) -> ([RankValue; 4], i8) {
            if delta == 0 {
                return (ranks, 0);
            }

            let mut selected: Option<(usize, u8)> = None;
            for (index, rank) in ranks.iter().enumerate() {
                if let RankValue::Number(value) = rank {
                    if selected
                        .map(|(_, current)| *value > current)
                        .unwrap_or(true)
                    {
                        selected = Some((index, *value));
                    }
                }
            }

            let Some((index, value)) = selected else {
                return (ranks, 0);
            };

            if delta > 0 {
                if value >= 9 {
                    return (ranks, 0);
                }
                ranks[index] = RankValue::Number(value + 1);
                (ranks, 1)
            } else {
                if value <= 1 {
                    return (ranks, 0);
                }
                ranks[index] = RankValue::Number(value - 1);
                (ranks, -1)
            }
        }

        fn trigger_nexus_rune_if_present(
            board: &mut NexusMatchBoardOf<T>,
            cell: u8,
            ranks: [RankValue; 4],
            profile: ElementProfile,
        ) -> ([RankValue; 4], Option<(NexusRuneCell, i8)>) {
            let Some(index) = board.rune_cells.iter().position(|rune| rune.cell == cell) else {
                return (ranks, None);
            };
            let rune = board.rune_cells.remove(index);
            let delta = Self::rune_delta_for_element(profile, rune.element);
            let (ranks, effect) = Self::apply_rune_delta(ranks, delta);
            let triggered = NexusRuneCell {
                cell,
                caster_card_id: rune.caster_card_id,
                element: rune.element,
            };
            (ranks, Some((triggered, effect)))
        }

        fn resolve_nexus_direct_captures(
            board: &mut NexusMatchBoardOf<T>,
            cell: u8,
            player: &T::AccountId,
        ) -> Result<Vec<(u32, ApexSide)>, Error<T>> {
            let attacker = board
                .cells
                .get(cell as usize)
                .and_then(|board_cell| board_cell.card.clone())
                .ok_or(Error::<T>::NexusCellOutOfBounds)?;
            let mut captured_cells: Vec<(u8, u32, ApexSide)> = Vec::new();

            for side in [
                ApexSide::Top,
                ApexSide::Right,
                ApexSide::Bottom,
                ApexSide::Left,
            ] {
                let Some(neighbor_cell) = Self::neighboring_cell(cell, side) else {
                    continue;
                };
                let Some(defender) = board
                    .cells
                    .get(neighbor_cell as usize)
                    .and_then(|board_cell| board_cell.card.as_ref())
                else {
                    continue;
                };
                if defender.controller == attacker.controller {
                    continue;
                }

                let attacker_rank = Self::rank_for_side(&attacker.ranks, side);
                let defender_rank = Self::rank_for_side(&defender.ranks, Self::opposite_side(side));
                if Self::rank_beats(attacker_rank, defender_rank) {
                    captured_cells.push((neighbor_cell, defender.card_id, side));
                }
            }

            for (captured_cell, _, _) in captured_cells.iter().copied() {
                if let Some(Some(card)) = board
                    .cells
                    .get_mut(captured_cell as usize)
                    .map(|board_cell| board_cell.card.as_mut())
                {
                    card.controller = player.clone();
                }
            }

            Ok(captured_cells
                .into_iter()
                .map(|(_, card_id, side)| (card_id, side))
                .collect())
        }

        fn create_nexus_rune(
            board: &mut NexusMatchBoardOf<T>,
            caster_cell: u8,
            caster_card_id: u32,
            well_cell: u8,
            element: Element,
        ) -> Result<(), Error<T>> {
            let mask = Self::cell_mask(well_cell)?;
            ensure!(
                board.mana_wells & mask != 0,
                Error::<T>::NexusInvalidRuneCast
            );
            ensure!(
                board.locked_cells & mask == 0,
                Error::<T>::NexusInvalidRuneCast
            );
            ensure!(
                Self::cells_are_adjacent(caster_cell, well_cell),
                Error::<T>::NexusInvalidRuneCast
            );
            ensure!(
                board
                    .cells
                    .get(well_cell as usize)
                    .ok_or(Error::<T>::NexusCellOutOfBounds)?
                    .card
                    .is_none(),
                Error::<T>::NexusCellOccupied
            );
            ensure!(
                !board.rune_cells.iter().any(|rune| rune.cell == well_cell),
                Error::<T>::NexusInvalidRuneCast
            );
            board
                .rune_cells
                .try_push(NexusRuneCell {
                    cell: well_cell,
                    caster_card_id,
                    element,
                })
                .map_err(|_| Error::<T>::NexusInvalidRuneCast)?;
            Ok(())
        }

        fn nexus_match_should_end(
            match_id: MatchId,
            match_state: &MatchStateOf<T>,
            board: &NexusMatchBoardOf<T>,
        ) -> bool {
            let hands_empty = match_state.players.iter().all(|player| {
                NexusMatchPlayedCards::<T>::get(match_id, player).len() as u32
                    >= T::NexusTeamSize::get()
            });
            if hands_empty {
                return true;
            }

            !(0..NEXUS_BOARD_CELL_COUNT).any(|cell| {
                let mask = 1u16 << cell;
                board.locked_cells & mask == 0
                    && board
                        .cells
                        .get(cell as usize)
                        .map(|board_cell| board_cell.card.is_none())
                        .unwrap_or(false)
            })
        }

        fn score_nexus_match(
            match_state: &MatchStateOf<T>,
            board: &NexusMatchBoardOf<T>,
        ) -> [u8; 2] {
            let mut score = [0u8; 2];
            for board_cell in board.cells.iter() {
                let Some(card) = board_cell.card.as_ref() else {
                    continue;
                };
                for (index, player) in match_state.players.iter().take(2).enumerate() {
                    if &card.controller == player {
                        score[index] = score[index].saturating_add(1);
                    }
                }
            }
            score
        }

        fn nexus_match_winner(
            match_state: &MatchStateOf<T>,
            score: [u8; 2],
        ) -> Option<T::AccountId> {
            if score[0] > score[1] {
                match_state.players.get(0).cloned()
            } else if score[1] > score[0] {
                match_state.players.get(1).cloned()
            } else {
                None
            }
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
                to_rank(bytes.get(0).copied().unwrap_or(0)),
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
