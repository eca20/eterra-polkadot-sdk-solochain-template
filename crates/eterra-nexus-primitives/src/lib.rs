#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

pub type Hash32 = [u8; 32];
pub type CardIdV2 = u64;
pub type EntityId = u64;
pub type PrismSpellId = u64;
pub type SubjectId = u32;
pub type MoveId = u32;
pub type ElementId = u8;
pub type SessionId = u64;

pub const MAX_ENTITY_LEVEL: u8 = 50;
pub const MAX_LEARNED_MOVES: u32 = 12;
pub const MAX_EQUIPPED_MOVES: u32 = 4;
pub const PACK_CARD_COUNT: u8 = 6;
pub const RANK_TOTALS: [u8; 5] = [18, 21, 24, 27, 30];
pub const DRAND_QUICKNET_CHAIN_HASH: Hash32 = [
    0x52, 0xdb, 0x9b, 0xa7, 0x0e, 0x0c, 0xc0, 0xf6, 0xea, 0xf7, 0x80, 0x3d, 0xd0, 0x74, 0x47, 0xa1,
    0xf5, 0x47, 0x77, 0x35, 0xfd, 0x3f, 0x66, 0x17, 0x92, 0xba, 0x94, 0x60, 0x0c, 0x84, 0xe9, 0x71,
];

#[derive(
    Encode,
    Decode,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum CardRarity {
    #[codec(index = 0)]
    Common,
    #[codec(index = 1)]
    Rare,
    #[codec(index = 2)]
    Epic,
    #[codec(index = 3)]
    Legendary,
    #[codec(index = 4)]
    Mythical,
}

impl CardRarity {
    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn rarity_load(self) -> u8 {
        self as u8 + 1
    }

    pub const fn target_rank_total(self) -> u8 {
        RANK_TOTALS[self.index()]
    }

    pub const fn is_legendary_or_better(self) -> bool {
        matches!(self, Self::Legendary | Self::Mythical)
    }
}

#[derive(
    Encode,
    Decode,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum EconomicRealm {
    #[codec(index = 0)]
    Training,
    #[codec(index = 1)]
    Production,
}

#[derive(
    Encode,
    Decode,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum SubjectRole {
    #[codec(index = 0)]
    Hero,
    #[codec(index = 1)]
    Villain,
    #[codec(index = 2)]
    Npc,
    #[codec(index = 3)]
    FriendlyCreature,
    #[codec(index = 4)]
    Monster,
    #[codec(index = 5)]
    Boss,
}

#[derive(
    Encode,
    Decode,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum ConversionPolicy {
    #[codec(index = 0)]
    PlayableEmbodiment,
    #[codec(index = 1)]
    Companion,
    #[codec(index = 2)]
    CombatCreature,
    #[codec(index = 3)]
    Npc,
    #[codec(index = 4)]
    Boss,
    #[codec(index = 5)]
    NonConvertible,
}

impl ConversionPolicy {
    pub const fn permits_conversion(self) -> bool {
        !matches!(self, Self::NonConvertible)
    }
}

#[derive(
    Encode,
    Decode,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    RuntimeDebug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum Element {
    #[codec(index = 0)]
    Neutral,
    #[codec(index = 1)]
    Fire,
    #[codec(index = 2)]
    Earth,
    #[codec(index = 3)]
    Water,
    #[codec(index = 4)]
    Wind,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ElementProfile {
    pub main: Element,
    pub minor: Option<Element>,
    pub resistance: Option<Element>,
    pub weakness: Option<Element>,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct SubjectDefinitionV2 {
    pub subject_definition_id: u32,
    pub subject_id: SubjectId,
    pub subject_version: u32,
    pub role: SubjectRole,
    pub conversion_policy: ConversionPolicy,
    pub element_profile: ElementProfile,
    pub display_metadata_id: u32,
    pub definition_hash: Hash32,
    pub catalog_version: u32,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct SubjectRarityProfile {
    pub profile_id: u32,
    pub subject_id: SubjectId,
    pub subject_version: u32,
    pub rarity: CardRarity,
    /// North, east, south, west. `10` represents Apex.
    pub base_ranks: [u8; 4],
    pub apex_side: Option<u8>,
    pub rarity_load: u8,
    pub profile_version: u32,
    pub profile_hash: Hash32,
}

impl SubjectRarityProfile {
    pub fn validate(&self) -> bool {
        let target = self.rarity.target_rank_total();
        let ranks_valid = self.base_ranks.iter().all(|rank| (1..=10).contains(rank));
        let apex_count = self.base_ranks.iter().filter(|rank| **rank == 10).count();
        let apex_valid = match self.rarity {
            CardRarity::Mythical => apex_count <= 1,
            _ => apex_count == 0,
        };
        let apex_side_valid = match self.apex_side {
            None => apex_count == 0,
            Some(side) => side < 4 && apex_count == 1 && self.base_ranks[side as usize] == 10,
        };
        ranks_valid
            && apex_valid
            && apex_side_valid
            && self.base_ranks.iter().copied().sum::<u8>() == target
            && self.rarity_load == self.rarity.rarity_load()
    }

    /// Returns true when every directional rank preserves or improves on the
    /// immediately lower rarity profile for the same subject definition.
    pub fn does_not_decrease_from(&self, lower: &Self) -> bool {
        self.subject_id == lower.subject_id
            && self.subject_version == lower.subject_version
            && self
                .base_ranks
                .iter()
                .zip(lower.base_ranks.iter())
                .all(|(current, previous)| current >= previous)
    }
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct SubjectActivationState {
    pub subject_definition_id: u32,
    pub mint_enabled: bool,
    pub conversion_enabled: bool,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct MediaDefinitionV2 {
    pub definition_id: u32,
    pub subject_id: Option<SubjectId>,
    pub media_id: u32,
    pub release_epoch: u32,
    pub definition_hash: Hash32,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum CardOriginV2 {
    #[codec(index = 0)]
    Pack { opening_id: Hash32, slot: u8 },
    #[codec(index = 1)]
    MythicalAscension { ascension_id: Hash32 },
    #[codec(index = 2)]
    Founder { entitlement_id: Hash32 },
    #[codec(index = 3)]
    Tutorial { tutorial_id: Hash32 },
    #[codec(index = 4)]
    FutureReforge { reforge_id: u64 },
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum CardStateV2 {
    #[codec(index = 0)]
    Active,
    #[codec(index = 1)]
    ConversionCommitted { request_id: Hash32 },
    #[codec(index = 2)]
    Converted { entity_id: EntityId },
    #[codec(index = 3)]
    MythicalAscended { output_card_id: CardIdV2 },
    #[codec(index = 4)]
    LegacyExchangeBurned,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct CardInstanceV2<AccountId, BlockNumber> {
    pub card_id: CardIdV2,
    pub owner: AccountId,
    pub set_id: u32,
    pub season_id: u32,
    pub subject_id: SubjectId,
    pub subject_version: u32,
    pub rarity: CardRarity,
    pub profile_id: u32,
    pub pose_definition_id: u32,
    pub background_definition_id: u32,
    pub serial_number: u64,
    pub economic_realm: EconomicRealm,
    pub origin: CardOriginV2,
    pub acquisition_id: Hash32,
    pub pool_id: u32,
    pub pool_version: u32,
    pub state: CardStateV2,
    pub acquired_at: BlockNumber,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum DiscoveryPolicy {
    #[codec(index = 0)]
    Standard,
    #[codec(index = 1)]
    Earned,
    #[codec(index = 2)]
    PremiumCosmetic,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PackSkuVersion<BlockNumber> {
    pub pack_sku: u32,
    pub version: u32,
    pub card_count: u8,
    pub set_id: u32,
    pub pool_id: u32,
    pub pool_version: u32,
    pub rarity_weights: [u32; 5],
    pub discovery_policy: DiscoveryPolicy,
    pub odds_metadata_hash: Hash32,
    pub immutable_config_hash: Hash32,
    pub active_from: BlockNumber,
    pub active_until: Option<BlockNumber>,
}

impl<BlockNumber> PackSkuVersion<BlockNumber> {
    pub fn validates_weights(&self) -> bool {
        self.card_count == PACK_CARD_COUNT
            && self.rarity_weights.iter().copied().sum::<u32>() == 10_000
            && self.rarity_weights == [6_800, 2_200, 750, 200, 50]
    }
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum PackCreditSource {
    #[codec(index = 0)]
    PaidPurchase { purchase_id: Hash32 },
    #[codec(index = 1)]
    NexusClaim {
        policy_version: u32,
        claim_id: Hash32,
    },
    #[codec(index = 2)]
    FpsPackTrack { track_id: u32, economy_version: u32 },
    #[codec(index = 3)]
    ArcadePrize {
        policy_version: u32,
        redemption_id: Hash32,
    },
    #[codec(index = 4)]
    Founder { entitlement_id: Hash32 },
    #[codec(index = 5)]
    TutorialTraining { tutorial_id: Hash32 },
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PackCredit<AccountId> {
    pub credit_id: u64,
    pub owner: AccountId,
    pub pack_sku: u32,
    pub sku_version: u32,
    pub economic_realm: EconomicRealm,
    pub source: PackCreditSource,
    pub amount: u32,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum AssetRole {
    #[codec(index = 0)]
    Card,
    #[codec(index = 1)]
    Entity,
    #[codec(index = 2)]
    PrismSpell,
    #[codec(index = 3)]
    SpellChargeReservation,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct AssetLock<BlockNumber> {
    pub session_id: SessionId,
    pub role: AssetRole,
    pub revision_at_lock: u32,
    pub expires_at: BlockNumber,
}

#[derive(
    Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen, Default,
)]
pub struct Genes {
    pub vitality: u8,
    pub attack: u8,
    pub defense: u8,
    pub agility: u8,
    pub focus: u8,
    pub resistance: u8,
}

impl Genes {
    pub fn validate(&self) -> bool {
        [
            self.vitality,
            self.attack,
            self.defense,
            self.agility,
            self.focus,
            self.resistance,
        ]
        .iter()
        .all(|gene| *gene <= 31)
    }

    pub fn score(&self) -> u32 {
        u32::from(self.vitality)
            + u32::from(self.attack)
            + u32::from(self.defense)
            + u32::from(self.agility)
            + u32::from(self.focus)
            + u32::from(self.resistance)
    }
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum EntityOrigin {
    #[codec(index = 0)]
    CardConversion {
        source_card_id: CardIdV2,
        source_rarity: CardRarity,
    },
    #[codec(index = 1)]
    LegendsBond { bond_id: u64 },
    #[codec(index = 2)]
    ApprovedMigration { provenance_hash: Hash32 },
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum EntityRole {
    #[codec(index = 0)]
    Hero,
    #[codec(index = 1)]
    Villain,
    #[codec(index = 2)]
    Companion,
    #[codec(index = 3)]
    Creature,
    #[codec(index = 4)]
    Monster,
    #[codec(index = 5)]
    Npc,
    #[codec(index = 6)]
    Boss,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct EntityInstance<AccountId, BlockNumber, LearnedMoves, EquippedMoves> {
    pub entity_id: EntityId,
    pub owner: AccountId,
    pub economic_realm: EconomicRealm,
    pub origin: EntityOrigin,
    pub subject_id: SubjectId,
    pub subject_version: u32,
    pub role: EntityRole,
    pub genome_hash: Hash32,
    pub genome_version: u16,
    pub genes: Genes,
    pub temperament: u8,
    pub cosmetic_seed: Hash32,
    pub stasis_genome: bool,
    pub level: u8,
    pub level_xp: u64,
    pub learned_moves: LearnedMoves,
    pub equipped_moves: EquippedMoves,
    pub current_cp: u32,
    pub max_cp: u32,
    pub cp_formula_version: u16,
    pub revision: u32,
    pub lock: Option<AssetLock<BlockNumber>>,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct EntityProfile {
    pub profile_id: u32,
    pub subject_id: SubjectId,
    pub subject_version: u32,
    pub rarity: CardRarity,
    pub role: EntityRole,
    pub base_combat_stats: [u16; 6],
    pub base_max_cp: u32,
    pub genetic_cp_span: u32,
    pub starter_moves: [MoveId; 2],
    pub formula_version: u16,
    pub definition_hash: Hash32,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct MoveDefinition {
    pub move_id: MoveId,
    pub element: Element,
    pub unlock_level: u8,
    pub essence_cost: u32,
    pub competitive_load: u16,
    pub tier: u8,
    pub tags: u32,
    pub cooldown_turns: u8,
    pub resource_cost: u16,
    pub rules_hash: Hash32,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct EntityLeagueFormat {
    pub format_id: u32,
    pub version: u32,
    pub min_max_cp: u32,
    pub max_max_cp: u32,
    pub current_cp_cap: u32,
    pub max_move_load: u16,
    pub maximum_ultimate_tier: u8,
    pub normalized: bool,
    pub rules_hash: Hash32,
}

pub fn calculate_max_cp(base_max_cp: u32, genetic_cp_span: u32, genes: &Genes) -> Option<u32> {
    base_max_cp.checked_add(genes.score().checked_mul(genetic_cp_span)? / 186)
}

pub fn calculate_current_cp(max_cp: u32, level_ratio_bps: u16) -> Option<u32> {
    max_cp
        .checked_mul(u32::from(level_ratio_bps))
        .map(|value| value / 10_000)
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum MagicAssetKind {
    #[codec(index = 0)]
    Essence,
    #[codec(index = 1)]
    SpellCharge,
    #[codec(index = 2)]
    PrismSpell,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PrismSpell<AccountId, BlockNumber> {
    pub spell_id: PrismSpellId,
    pub owner: AccountId,
    pub economic_realm: EconomicRealm,
    pub definition_id: u32,
    pub traits_seed: Hash32,
    pub level: u8,
    pub xp: u64,
    pub revision: u32,
    pub lock: Option<AssetLock<BlockNumber>>,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum PlayerXpTarget {
    #[codec(index = 0)]
    PlayerAdvancement,
    #[codec(index = 1)]
    PackTrack {
        pack_sku: u32,
        sku_version: u32,
        economy_version: u32,
    },
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum GameModeKind {
    #[codec(index = 0)]
    Legends,
    #[codec(index = 1)]
    AbilityDeathmatch,
    #[codec(index = 2)]
    Extraction,
    #[codec(index = 3)]
    ExtractionBattleRoyale,
    #[codec(index = 4)]
    NormalizedLegacy,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ResultHeaderV1<BlockNumber> {
    pub protocol_version: u16,
    pub genesis_hash: Hash32,
    pub game_id: u32,
    pub game_version: u32,
    pub mode_id: u32,
    pub policy_version: u32,
    pub session_id: SessionId,
    pub result_id: Hash32,
    pub authority_epoch: u32,
    pub roster_root: Hash32,
    pub expires_at: BlockNumber,
    pub telemetry_root: Hash32,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct EntityReforgeConfig {
    pub version: u32,
    pub pool_id: u32,
    pub pool_version: u32,
    pub pool_hash: Hash32,
    /// Reserved for a later reviewed runtime. Initial deployments must store false.
    pub enabled: bool,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct EntityReforgeReceipt<AccountId, BlockNumber> {
    pub reforge_id: u64,
    pub owner: AccountId,
    pub consumed_entity_id: EntityId,
    pub entity_snapshot_hash: Hash32,
    pub preserved_rarity: CardRarity,
    pub output_card_id: CardIdV2,
    pub pool_id: u32,
    pub pool_version: u32,
    pub reforged_at: BlockNumber,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rarity_wire_indices_and_loads_are_frozen() {
        let values = [
            CardRarity::Common,
            CardRarity::Rare,
            CardRarity::Epic,
            CardRarity::Legendary,
            CardRarity::Mythical,
        ];
        for (index, value) in values.iter().copied().enumerate() {
            assert_eq!(value.encode(), [index as u8]);
            assert_eq!(value.rarity_load(), index as u8 + 1);
        }
    }

    #[test]
    fn rank_profile_enforces_exact_rarity_total_and_apex_policy() {
        let valid = SubjectRarityProfile {
            profile_id: 1,
            subject_id: 1,
            subject_version: 1,
            rarity: CardRarity::Mythical,
            base_ranks: [10, 8, 7, 5],
            apex_side: Some(0),
            rarity_load: 5,
            profile_version: 1,
            profile_hash: [1; 32],
        };
        assert!(valid.validate());
        assert!(!SubjectRarityProfile {
            base_ranks: [10, 10, 5, 5],
            apex_side: Some(0),
            ..valid
        }
        .validate());
    }

    #[test]
    fn rank_profile_detects_directional_regression_between_rarities() {
        let common = SubjectRarityProfile {
            profile_id: 1,
            subject_id: 7,
            subject_version: 1,
            rarity: CardRarity::Common,
            base_ranks: [6, 5, 4, 3],
            apex_side: None,
            rarity_load: 1,
            profile_version: 1,
            profile_hash: [1; 32],
        };
        let rare_with_regression = SubjectRarityProfile {
            profile_id: 2,
            rarity: CardRarity::Rare,
            base_ranks: [5, 6, 5, 5],
            rarity_load: 2,
            profile_hash: [2; 32],
            ..common
        };
        assert!(common.validate());
        assert!(rare_with_regression.validate());
        assert!(!rare_with_regression.does_not_decrease_from(&common));

        let rare_monotonic = SubjectRarityProfile {
            base_ranks: [6, 6, 5, 4],
            ..rare_with_regression
        };
        assert!(rare_monotonic.validate());
        assert!(rare_monotonic.does_not_decrease_from(&common));
    }

    #[test]
    fn cp_is_integer_bounded_and_monotonic() {
        let genes = Genes {
            vitality: 31,
            attack: 20,
            defense: 15,
            agility: 12,
            focus: 8,
            resistance: 0,
        };
        let max_cp = calculate_max_cp(1_000, 600, &genes).unwrap();
        let level_one = calculate_current_cp(max_cp, 500).unwrap();
        let level_two = calculate_current_cp(max_cp, 650).unwrap();
        assert!(level_one <= level_two);
        assert!(level_two <= max_cp);
    }

    #[test]
    fn production_pack_table_is_exact() {
        let sku = PackSkuVersion {
            pack_sku: 1,
            version: 1,
            card_count: 6,
            set_id: 1,
            pool_id: 1,
            pool_version: 1,
            rarity_weights: [6_800, 2_200, 750, 200, 50],
            discovery_policy: DiscoveryPolicy::Earned,
            odds_metadata_hash: [1; 32],
            immutable_config_hash: [2; 32],
            active_from: 0u32,
            active_until: None,
        };
        assert!(sku.validates_weights());
    }

    #[test]
    fn public_scale_contract_matches_golden_v1() {
        assert_eq!(
            [
                CardOriginV2::Pack {
                    opening_id: [0; 32],
                    slot: 0,
                }
                .encode()[0],
                CardOriginV2::MythicalAscension {
                    ascension_id: [0; 32],
                }
                .encode()[0],
                CardOriginV2::Founder {
                    entitlement_id: [0; 32],
                }
                .encode()[0],
                CardOriginV2::Tutorial {
                    tutorial_id: [0; 32],
                }
                .encode()[0],
                CardOriginV2::FutureReforge { reforge_id: 0 }.encode()[0],
            ],
            [0, 1, 2, 3, 4]
        );
        assert_eq!(
            [
                CardStateV2::Active.encode()[0],
                CardStateV2::ConversionCommitted {
                    request_id: [0; 32],
                }
                .encode()[0],
                CardStateV2::Converted { entity_id: 0 }.encode()[0],
                CardStateV2::MythicalAscended { output_card_id: 0 }.encode()[0],
                CardStateV2::LegacyExchangeBurned.encode()[0],
            ],
            [0, 1, 2, 3, 4]
        );
        assert_eq!(
            [
                DiscoveryPolicy::Standard.encode()[0],
                DiscoveryPolicy::Earned.encode()[0],
                DiscoveryPolicy::PremiumCosmetic.encode()[0],
            ],
            [0, 1, 2]
        );
        assert_eq!(
            [
                AssetRole::Card.encode()[0],
                AssetRole::Entity.encode()[0],
                AssetRole::PrismSpell.encode()[0],
                AssetRole::SpellChargeReservation.encode()[0],
            ],
            [0, 1, 2, 3]
        );
        assert_eq!(
            [
                EntityOrigin::CardConversion {
                    source_card_id: 0,
                    source_rarity: CardRarity::Common,
                }
                .encode()[0],
                EntityOrigin::LegendsBond { bond_id: 0 }.encode()[0],
                EntityOrigin::ApprovedMigration {
                    provenance_hash: [0; 32],
                }
                .encode()[0],
            ],
            [0, 1, 2]
        );
        assert_eq!(
            [
                MagicAssetKind::Essence.encode()[0],
                MagicAssetKind::SpellCharge.encode()[0],
                MagicAssetKind::PrismSpell.encode()[0],
            ],
            [0, 1, 2]
        );
        assert_eq!(PlayerXpTarget::PlayerAdvancement.encode()[0], 0);

        let profile = SubjectRarityProfile {
            profile_id: 1,
            subject_id: 0x0102_0304,
            subject_version: 2,
            rarity: CardRarity::Epic,
            base_ranks: [3, 7, 8, 6],
            apex_side: None,
            rarity_load: 3,
            profile_version: 9,
            profile_hash: [0xaa; 32],
        };
        let mut expected_profile = vec![
            0x01, 0x00, 0x00, 0x00, 0x04, 0x03, 0x02, 0x01, 0x02, 0x00, 0x00, 0x00, 0x02, 0x03,
            0x07, 0x08, 0x06, 0x00, 0x03, 0x09, 0x00, 0x00, 0x00,
        ];
        expected_profile.extend([0xaa; 32]);
        assert_eq!(profile.encode(), expected_profile);

        let credit_source = PackCreditSource::FpsPackTrack {
            track_id: 17,
            economy_version: 4,
        };
        assert_eq!(
            credit_source.encode(),
            [0x02, 0x11, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00]
        );
        let arcade_source = PackCreditSource::ArcadePrize {
            policy_version: 9,
            redemption_id: [0xbb; 32],
        };
        let mut expected_arcade = vec![0x03, 0x09, 0x00, 0x00, 0x00];
        expected_arcade.extend([0xbb; 32]);
        assert_eq!(arcade_source.encode(), expected_arcade);
        assert_eq!(
            PackCreditSource::Founder {
                entitlement_id: [0xcc; 32]
            }
            .encode()[0],
            0x04
        );
        assert_eq!(
            PackCreditSource::TutorialTraining {
                tutorial_id: [0xdd; 32]
            }
            .encode()[0],
            0x05
        );

        let target = PlayerXpTarget::PackTrack {
            pack_sku: 7,
            sku_version: 2,
            economy_version: 4,
        };
        assert_eq!(
            target.encode(),
            [0x01, 0x07, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00]
        );
    }
}
