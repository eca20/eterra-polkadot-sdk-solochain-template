#![cfg_attr(not(feature = "std"), no_std)]
// FRAME's generated hook glue currently triggers this lint in macro expansion.
#![allow(clippy::manual_inspect)]
// SCALE-stable result/session contracts intentionally expose bounded generic
// bodies and append-only extrinsic arguments instead of wrapper migrations.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;

use codec::{Decode, Encode, MaxEncodedLen};
use eterra_nexus_primitives::{
    EconomicRealm, Element, EntityId, GameModeKind, Hash32, PrismSpellId, ResultHeaderV1, SessionId,
};
use frame_support::pallet_prelude::*;
use pallet_eterra_randomness::RandomnessMode;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

pub const FPS_GAME_ID: u32 = 1005;
pub const LEGENDS_GAME_ID: u32 = 1006;
pub const NORMALIZED_LEGACY_MODE_ID: u32 = 0;
pub const ABILITY_DEATHMATCH_MODE_ID: u32 = 1;
pub const EXTRACTION_MODE_ID: u32 = 2;
pub const EXTRACTION_BATTLE_ROYALE_MODE_ID: u32 = 3;
pub const PRODUCTION_MAX_PLAYER_XP_V1: u128 = 600;
pub const PRODUCTION_MAX_XP_PER_DAY_V1: u128 = 3_600;
pub const PRODUCTION_MIN_ACTIVE_SECONDS_V1: u32 = 300;
pub const PRODUCTION_MAX_AFK_BPS_V1: u16 = 2_500;
pub const PRODUCTION_REWARD_WEIGHTS_BPS_V1: [u16; 3] = [2_000, 3_000, 5_000];
pub const PRODUCTION_REPEAT_COHORT_MULTIPLIERS_BPS_V1: [u16; 5] = [10_000, 7_500, 5_000, 2_500, 0];
pub const PRODUCTION_ENTITY_REWARDS_PER_DAY_V1: u16 = 6;

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct AssetRevision {
    pub asset_id: u64,
    pub revision: u32,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ChargeUse {
    pub definition_id: u32,
    pub amount: u32,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct AuthorityEpoch<BlockNumber> {
    pub public_key: [u8; 32],
    /// Immutable hash of the complete authority-side rules artifact used for
    /// roster initialization, deterministic encounter AI, and bounded result
    /// validation. A key rotation or rules change requires a new epoch.
    pub authority_config_hash: Hash32,
    pub active_from: BlockNumber,
    pub active_until: BlockNumber,
    pub revoked: bool,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PersistentLoadoutPolicy {
    /// Immutable creature format that enforces CP, potential, move tier/load,
    /// allowed definitions, and per-tag limits.
    pub entity_format: Option<(u32, u32)>,
    /// Bitset over `EntityRole` codec indices. Zero forbids entities.
    pub allowed_entity_roles_mask: u8,
    pub max_entities: u8,
    pub max_prisms: u8,
    pub max_charge_definitions: u8,
    pub max_total_charges: u16,
    pub max_magic_load: u16,
    pub rules_hash: Hash32,
}

impl PersistentLoadoutPolicy {
    pub fn validate(&self, normalized: bool, mode_kind: GameModeKind) -> bool {
        let has_entities = self.max_entities > 0;
        let has_magic = self.max_prisms > 0 || self.max_charge_definitions > 0;
        let entity_shape_valid =
            has_entities == (self.entity_format.is_some() && self.allowed_entity_roles_mask != 0);
        let magic_shape_valid = has_magic == (self.max_magic_load > 0)
            && (self.max_charge_definitions > 0) == (self.max_total_charges > 0);
        let normalized_shape_valid = !normalized
            || (!has_entities
                && !has_magic
                && self.max_total_charges == 0
                && self.max_magic_load == 0);
        let legends_shape_valid =
            mode_kind != GameModeKind::Legends || (has_entities && self.max_entities <= 3);
        self.rules_hash.iter().any(|byte| *byte != 0)
            && entity_shape_valid
            && magic_shape_valid
            && normalized_shape_valid
            && legends_shape_valid
    }
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct RewardPolicy {
    pub game_id: u32,
    pub game_version: u32,
    pub mode_id: u32,
    pub policy_version: u32,
    pub mode_kind: GameModeKind,
    pub economic_realm: EconomicRealm,
    pub practice_only: bool,
    pub normalized: bool,
    pub loadout: PersistentLoadoutPolicy,
    pub max_player_xp: u128,
    pub entity_xp: u64,
    pub base_essence: u32,
    pub essence_element: Element,
    pub charge_definition_id: Option<u32>,
    pub charge_drop_bps: u16,
    pub prism_definition_id: Option<u32>,
    pub prism_drop_bps: u16,
    pub minimum_active_seconds: u32,
    pub maximum_afk_bps: u16,
    pub maximum_elapsed_seconds: u32,
    pub maximum_kills: u16,
    pub maximum_assists: u16,
    pub maximum_deaths: u16,
    pub maximum_damage: u32,
    pub maximum_objective_score: u32,
    pub maximum_outcome: u8,
    pub maximum_placement: u8,
    pub elimination_weight_bps: u16,
    pub participation_weight_bps: u16,
    pub objective_weight_bps: u16,
    pub maximum_xp_per_day: u128,
    pub repeat_cohort_multipliers_bps: [u16; 5],
    pub per_entity_encounter_rewards_per_day: u16,
    pub first_clear_markers_required: bool,
    pub policy_hash: Hash32,
}

/// Immutable, non-random Prism reward attached to one verified RPG encounter
/// under a specific reward-policy version.
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct DeterministicPrismQuestPolicy {
    pub quest_hash: Hash32,
    pub encounter_id: u32,
    pub prism_definition_id: u32,
    pub economic_realm: EconomicRealm,
}

impl RewardPolicy {
    pub fn key(&self) -> (u32, u32, u32, u32) {
        (
            self.game_id,
            self.game_version,
            self.mode_id,
            self.policy_version,
        )
    }

    pub fn validate(&self) -> bool {
        let charge_drop_shape_valid =
            (self.charge_drop_bps > 0) == self.charge_definition_id.is_some();
        let prism_drop_shape_valid =
            (self.prism_drop_bps > 0) == self.prism_definition_id.is_some();
        let reward_weights = u32::from(self.elimination_weight_bps)
            + u32::from(self.participation_weight_bps)
            + u32::from(self.objective_weight_bps);
        let repeat_multipliers_valid = self.repeat_cohort_multipliers_bps[0] == 10_000
            && self
                .repeat_cohort_multipliers_bps
                .windows(2)
                .all(|pair| pair[0] >= pair[1] && pair[0] <= 10_000)
            && self.repeat_cohort_multipliers_bps[4] <= 10_000;
        let fps_reward_shape_valid = if self.max_player_xp == 0 {
            true
        } else {
            reward_weights == 10_000
                && self.maximum_xp_per_day >= self.max_player_xp
                && self.maximum_placement > 0
                && repeat_multipliers_valid
        };
        let legends_has_rewards = self.mode_kind == GameModeKind::Legends
            && !self.practice_only
            && (self.entity_xp > 0
                || self.base_essence > 0
                || self.charge_drop_bps > 0
                || self.prism_drop_bps > 0);
        let entity_reward_shape_valid = !legends_has_rewards
            || (self.per_entity_encounter_rewards_per_day > 0 && self.first_clear_markers_required);
        let mode_reward_shape_valid = if self.mode_kind == GameModeKind::Legends {
            self.max_player_xp == 0
        } else {
            self.entity_xp == 0
                && self.base_essence == 0
                && self.charge_definition_id.is_none()
                && self.charge_drop_bps == 0
                && self.prism_definition_id.is_none()
                && self.prism_drop_bps == 0
        };
        let mode_identity_valid = match self.mode_kind {
            GameModeKind::Legends => {
                self.game_id == LEGENDS_GAME_ID && self.mode_id == 1 && !self.normalized
            }
            GameModeKind::AbilityDeathmatch => {
                self.game_id == FPS_GAME_ID
                    && self.mode_id == ABILITY_DEATHMATCH_MODE_ID
                    && !self.normalized
                    && self.maximum_elapsed_seconds == 480
                    && self.maximum_kills == 20
            }
            GameModeKind::Extraction => {
                self.game_id == FPS_GAME_ID
                    && self.mode_id == EXTRACTION_MODE_ID
                    && !self.normalized
                    && self.maximum_elapsed_seconds == 720
            }
            GameModeKind::ExtractionBattleRoyale => {
                self.game_id == FPS_GAME_ID
                    && self.mode_id == EXTRACTION_BATTLE_ROYALE_MODE_ID
                    && !self.normalized
                    && self.maximum_elapsed_seconds == 900
            }
            GameModeKind::NormalizedLegacy => {
                self.game_id == FPS_GAME_ID
                    && self.mode_id == NORMALIZED_LEGACY_MODE_ID
                    && self.normalized
            }
        };
        let production_v1_baseline_valid =
            self.economic_realm != EconomicRealm::Production || self.practice_only || {
                if self.mode_kind == GameModeKind::Legends {
                    self.per_entity_encounter_rewards_per_day
                        == PRODUCTION_ENTITY_REWARDS_PER_DAY_V1
                        && self.first_clear_markers_required
                        && self.repeat_cohort_multipliers_bps
                            == PRODUCTION_REPEAT_COHORT_MULTIPLIERS_BPS_V1
                } else {
                    self.minimum_active_seconds == PRODUCTION_MIN_ACTIVE_SECONDS_V1
                        && self.maximum_afk_bps == PRODUCTION_MAX_AFK_BPS_V1
                        && self.max_player_xp == PRODUCTION_MAX_PLAYER_XP_V1
                        && self.maximum_xp_per_day == PRODUCTION_MAX_XP_PER_DAY_V1
                        && [
                            self.elimination_weight_bps,
                            self.participation_weight_bps,
                            self.objective_weight_bps,
                        ] == PRODUCTION_REWARD_WEIGHTS_BPS_V1
                        && self.repeat_cohort_multipliers_bps
                            == PRODUCTION_REPEAT_COHORT_MULTIPLIERS_BPS_V1
                }
            };
        u32::from(self.charge_drop_bps) + u32::from(self.prism_drop_bps) <= 10_000
            && self.policy_hash.iter().any(|byte| *byte != 0)
            && charge_drop_shape_valid
            && prism_drop_shape_valid
            && self.maximum_afk_bps <= 10_000
            && self.maximum_elapsed_seconds > 0
            && self.minimum_active_seconds <= self.maximum_elapsed_seconds
            && (self.practice_only
                || self.max_player_xp > 0
                || self.entity_xp > 0
                || self.base_essence > 0
                || self.charge_drop_bps > 0
                || self.prism_drop_bps > 0)
            && mode_identity_valid
            && self.loadout.validate(self.normalized, self.mode_kind)
            && fps_reward_shape_valid
            && entity_reward_shape_valid
            && mode_reward_shape_valid
            && production_v1_baseline_valid
    }
}

#[derive(
    Encode, Decode, Clone, Copy, Default, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct RewardBudget {
    pub xp_total: u128,
    pub xp_reserved: u128,
    pub xp_spent: u128,
    pub essence_total: u128,
    pub essence_reserved: u128,
    pub essence_spent: u128,
    pub charge_slots_total: u64,
    pub charge_slots_reserved: u64,
    pub charge_slots_spent: u64,
    pub prism_slots_total: u64,
    pub prism_slots_reserved: u64,
    pub prism_slots_spent: u64,
}

#[derive(
    Encode, Decode, Clone, Copy, Default, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct RewardLiability {
    pub xp: u128,
    pub essence: u128,
    pub charge_slots: u64,
    pub prism_slots: u64,
}

#[derive(
    Encode, Decode, Clone, Copy, Default, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct DailyXpLedger {
    pub reserved: u128,
    pub awarded: u128,
}

#[derive(
    Encode, Decode, Clone, Copy, Default, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct EntityEncounterLedger {
    pub reserved: u16,
    pub rewarded: u16,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum SessionStatus {
    #[codec(index = 0)]
    Active,
    #[codec(index = 1)]
    SettledPendingDrop,
    #[codec(index = 2)]
    Settled,
    #[codec(index = 3)]
    Expired,
    #[codec(index = 4)]
    Aborted,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct SessionRecord<AccountId, BlockNumber, Entities, Prisms, Charges> {
    pub session_id: SessionId,
    pub owner: AccountId,
    pub game_id: u32,
    pub game_version: u32,
    pub mode_id: u32,
    pub policy_version: u32,
    pub authority_epoch: u32,
    pub economic_realm: EconomicRealm,
    pub roster_root: Hash32,
    pub cohort_hash: Hash32,
    pub encounter_id: Option<u32>,
    pub reward_day: u64,
    pub cohort_ordinal: u8,
    pub cohort_multiplier_bps: u16,
    pub reward_liability: RewardLiability,
    pub pending_drop_slot_reserved: bool,
    pub entities: Entities,
    pub prisms: Prisms,
    pub charge_allowance: Charges,
    pub expires_at: BlockNumber,
    pub status: SessionStatus,
    pub result_id: Option<Hash32>,
    /// Immutable source expectation captured at authorization. Appended to
    /// preserve the ordering of every existing session field.
    pub randomness_provenance: RandomnessMode,
    /// Snapshotted at authorization so a later governance publication cannot
    /// alter this session's maximum liability or reward facts.
    pub deterministic_prism_quest: Option<DeterministicPrismQuestPolicy>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct RpgBattleResultV1<Entities> {
    pub owner_won: bool,
    pub encounter_id: u32,
    pub entity_ids: Entities,
    pub elapsed_seconds: u32,
    pub turn_count: u16,
    pub combat_metric: u32,
    pub transcript_hash: Hash32,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct FpsMatchResultV1<AccountId, Charges, Prisms> {
    pub account: AccountId,
    pub cohort_hash: Hash32,
    pub active_seconds: u32,
    pub afk_seconds: u32,
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    pub damage: u32,
    pub objective_score: u32,
    pub outcome: u8,
    pub placement: u8,
    pub used_charges: Charges,
    pub used_prisms: Prisms,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum ResultBodyV1<AccountId, Entities, Charges, Prisms> {
    #[codec(index = 0)]
    RpgBattle(RpgBattleResultV1<Entities>),
    #[codec(index = 1)]
    FpsMatch(FpsMatchResultV1<AccountId, Charges, Prisms>),
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct SignedResultV1<BlockNumber, AccountId, Entities, Charges, Prisms, Signature> {
    pub header: ResultHeaderV1<BlockNumber>,
    pub body: ResultBodyV1<AccountId, Entities, Charges, Prisms>,
    pub server_signature: Signature,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PendingDropResolution<AccountId> {
    pub session_id: SessionId,
    pub owner: AccountId,
    pub economic_realm: EconomicRealm,
    pub result_id: Hash32,
    pub request_id: Hash32,
    pub policy_key: (u32, u32, u32, u32),
    pub charge_definition_id: Option<u32>,
    pub charge_drop_bps: u16,
    pub prism_definition_id: Option<u32>,
    pub prism_drop_bps: u16,
    /// Must match both the session authorization and the provider's bound
    /// request/output provenance.
    pub randomness_provenance: RandomnessMode,
}

/// Authority-approved, player-submitted session start contract. The authority
/// signs this closed payload before the runtime reserves any reward liability
/// or locks any persistent asset.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct SessionAuthorizationTicket<AccountId, BlockNumber> {
    pub protocol_version: u16,
    pub genesis_hash: Hash32,
    pub pallet_instance_id: u8,
    pub authorization_id: Hash32,
    pub owner: AccountId,
    pub game_id: u32,
    pub game_version: u32,
    pub mode_id: u32,
    pub policy_version: u32,
    pub policy_hash: Hash32,
    pub authority_epoch: u32,
    pub authority_config_hash: Hash32,
    pub economic_realm: EconomicRealm,
    pub cohort_hash: Hash32,
    pub encounter_id: Option<u32>,
    pub roster_root: Hash32,
    pub expected_randomness_provenance: RandomnessMode,
    pub expires_at: BlockNumber,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct SessionAuthorizationReceipt {
    pub authorization_id: Hash32,
    pub ticket_hash: Hash32,
    pub session_id: SessionId,
    pub session_epoch: u64,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct SealedResultEpoch {
    pub epoch: u64,
    pub terminal_root: Hash32,
    pub session_count: u32,
}

pub trait ServerSignatureVerifier {
    fn verify(public_key: &[u8; 32], payload_hash: &Hash32, signature: &[u8]) -> bool;
}

pub trait GenesisHashProvider {
    fn genesis_hash() -> Hash32;
}

#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper {
    fn authority_public_key() -> [u8; 32];
    fn sign_result(payload_hash: &Hash32) -> sp_std::vec::Vec<u8>;
    fn seed_finalized_randomness(request_id: Hash32, output: Hash32);
    fn seed_timed_out_randomness(request_id: Hash32);
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use eterra_nexus_primitives::{AssetLock, AssetRole};
    use frame_support::{dispatch::DispatchResult, transactional};
    use frame_system::pallet_prelude::*;
    use pallet_alpha_access::AccessControl;
    use pallet_eterra_creatures::EntityManager;
    use pallet_eterra_gamer::V2PlayerProgressionManager;
    use pallet_eterra_magic::{MagicLoadoutLimits, MagicManager};
    use pallet_eterra_randomness::VerifiableRandomness;
    use sp_runtime::traits::{SaturatedConversion, Saturating};
    use sp_runtime::DispatchError;
    use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);
    const RESULT_DOMAIN: &[u8] = b"ETERRA_GAME_RESULT_V1";
    const ROSTER_DOMAIN: &[u8] = b"ETERRA_SESSION_ROSTER_V1";
    const SESSION_AUTHORIZATION_DOMAIN: &[u8] = b"ETERRA_GAME_SESSION_AUTHORIZATION_V1";
    const DROP_DOMAIN: Hash32 = *b"ETERRA_RPG_DROP_V1______________";
    const DETERMINISTIC_PRISM_QUEST_DOMAIN: &[u8] = b"ETERRA_DETERMINISTIC_PRISM_QUEST_V1";

    type EntityListOf<T> = BoundedVec<AssetRevision, <T as Config>::MaxSessionEntities>;
    type PrismListOf<T> = BoundedVec<AssetRevision, <T as Config>::MaxSessionPrisms>;
    type ChargeListOf<T> = BoundedVec<ChargeUse, <T as Config>::MaxChargeDefinitions>;
    type SignatureOf<T> = BoundedVec<u8, <T as Config>::MaxSignatureBytes>;
    type SessionOf<T> = SessionRecord<
        <T as frame_system::Config>::AccountId,
        BlockNumberFor<T>,
        EntityListOf<T>,
        PrismListOf<T>,
        ChargeListOf<T>,
    >;
    pub type ResultOf<T> = SignedResultV1<
        BlockNumberFor<T>,
        <T as frame_system::Config>::AccountId,
        BoundedVec<EntityId, <T as Config>::MaxSessionEntities>,
        ChargeListOf<T>,
        BoundedVec<PrismSpellId, <T as Config>::MaxSessionPrisms>,
        SignatureOf<T>,
    >;
    type ResultIdsOf<T> = BoundedVec<Hash32, <T as Config>::MaxResultsPerEpoch>;
    type AuthorizationIdsOf<T> =
        BoundedVec<Hash32, <T as Config>::MaxSessionAuthorizationReceiptsPerEpoch>;
    pub type SessionAuthorizationTicketOf<T> =
        SessionAuthorizationTicket<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        type AccessControl: pallet_alpha_access::AccessControl<Self::AccountId>;
        type SignatureVerifier: ServerSignatureVerifier;
        type Entities: EntityManager<Self::AccountId, BlockNumberFor<Self>>;
        type Magic: MagicManager<Self::AccountId, BlockNumberFor<Self>>;
        type PlayerProgression: V2PlayerProgressionManager<Self::AccountId>;
        type Randomness: VerifiableRandomness;
        type GenesisHashProvider: crate::GenesisHashProvider;
        #[cfg(feature = "runtime-benchmarks")]
        type BenchmarkHelper: crate::BenchmarkHelper;
        #[pallet::constant]
        type PalletInstanceId: Get<u8>;
        #[pallet::constant]
        type MaxSessionEntities: Get<u32>;
        #[pallet::constant]
        type MaxSessionPrisms: Get<u32>;
        #[pallet::constant]
        type MaxChargeDefinitions: Get<u32>;
        #[pallet::constant]
        type MaxSignatureBytes: Get<u32>;
        #[pallet::constant]
        type MaxActiveSessionsPerAccount: Get<u32>;
        #[pallet::constant]
        type MaxActiveSessionsPerAuthority: Get<u32>;
        #[pallet::constant]
        type MaxSessionAuthorizationReceiptsPerEpoch: Get<u32>;
        #[pallet::constant]
        type MaxPendingDropsPerAccount: Get<u32>;
        #[pallet::constant]
        type MaxSessionLifetime: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type ExpiryGrace: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type ResultEpochSize: Get<u64>;
        #[pallet::constant]
        type MaxResultsPerEpoch: Get<u32>;
        #[pallet::constant]
        type ResultDisputeWindow: Get<BlockNumberFor<Self>>;
        /// Number of chain blocks in the anti-farming reward day.
        #[pallet::constant]
        type RewardDayBlocks: Get<u64>;
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn authority_epoch)]
    pub type AuthorityEpochs<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        (u32, u32, u32, u32),
        AuthorityEpoch<BlockNumberFor<T>>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn reward_policy)]
    pub type RewardPolicies<T> =
        StorageMap<_, Blake2_128Concat, (u32, u32, u32, u32), RewardPolicy, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn reward_policy_active)]
    pub type RewardPolicyActivation<T> =
        StorageMap<_, Blake2_128Concat, (u32, u32, u32, u32), bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn reward_budget)]
    pub type RewardBudgets<T> =
        StorageMap<_, Blake2_128Concat, (u32, u32, u32, u32), RewardBudget, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_session_id)]
    pub type NextSessionId<T> = StorageValue<_, SessionId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn session)]
    pub type Sessions<T: Config> =
        StorageMap<_, Blake2_128Concat, SessionId, SessionOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn active_session_count)]
    pub type ActiveSessionCount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn processed_result)]
    pub type ProcessedResults<T> = StorageMap<_, Blake2_128Concat, Hash32, Hash32, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn settled_session)]
    pub type SettledSessions<T> = StorageMap<_, Blake2_128Concat, SessionId, Hash32, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pending_drop)]
    pub type PendingDrops<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        SessionId,
        PendingDropResolution<T::AccountId>,
        OptionQuery,
    >;

    /// Includes both authorized RPG drop liabilities and unresolved drops.
    #[pallet::storage]
    #[pallet::getter(fn pending_drop_liability_count)]
    pub type PendingDropLiabilityCount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn daily_xp_ledger)]
    pub type DailyXpLedgers<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        (EconomicRealm, u64),
        DailyXpLedger,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn cohort_attempt_count)]
    pub type CohortAttemptCounts<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        ((u32, u32, u32, u32), u64, Hash32),
        u32,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn entity_encounter_ledger)]
    pub type EntityEncounterLedgers<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        (T::AccountId, EntityId),
        Blake2_128Concat,
        (EconomicRealm, u32, u32, u64),
        EntityEncounterLedger,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn first_clear_marker)]
    pub type FirstClearMarkers<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        (EconomicRealm, u32, u32),
        Hash32,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn epoch_session_count)]
    pub type EpochSessionCount<T> = StorageMap<_, Blake2_128Concat, u64, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn epoch_terminal_count)]
    pub type EpochTerminalCount<T> = StorageMap<_, Blake2_128Concat, u64, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn epoch_terminal_accumulator)]
    pub type EpochTerminalAccumulator<T> = StorageMap<_, Blake2_128Concat, u64, Hash32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn epoch_last_terminal_at)]
    pub type EpochLastTerminalAt<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BlockNumberFor<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn epoch_result_ids)]
    pub type EpochResultIds<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, ResultIdsOf<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn sealed_result_epoch)]
    pub type SealedResultEpochs<T> =
        StorageMap<_, Blake2_128Concat, u64, SealedResultEpoch, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn deterministic_prism_quest_policy)]
    pub type DeterministicPrismQuestPolicies<T> = StorageMap<
        _,
        Blake2_128Concat,
        ((u32, u32, u32, u32), u32),
        DeterministicPrismQuestPolicy,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn deterministic_prism_quest_definition)]
    pub type DeterministicPrismQuestDefinitions<T> =
        StorageMap<_, Blake2_128Concat, Hash32, u32, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn deterministic_prism_quest_claim)]
    pub type DeterministicPrismQuestClaims<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        Hash32,
        Hash32,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn reward_policy_ever_activated)]
    pub type RewardPolicyEverActivated<T> =
        StorageMap<_, Blake2_128Concat, (u32, u32, u32, u32), bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn active_session_count_by_authority)]
    pub type ActiveSessionCountByAuthority<T> =
        StorageMap<_, Blake2_128Concat, (u32, u32, u32, u32), u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn session_authorization_receipt)]
    pub type SessionAuthorizationReceipts<T> =
        StorageMap<_, Blake2_128Concat, Hash32, SessionAuthorizationReceipt, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn epoch_authorization_ids)]
    pub type EpochAuthorizationIds<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, AuthorizationIdsOf<T>, ValueQuery>;

    /// Sealing waits until every ticket in the epoch has expired, so pruning
    /// its replay receipt cannot make an old ticket usable again.
    #[pallet::storage]
    #[pallet::getter(fn epoch_authorization_max_expiry)]
    pub type EpochAuthorizationMaxExpiry<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BlockNumberFor<T>, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        AuthorityEpochRegistered {
            game_id: u32,
            game_version: u32,
            mode_id: u32,
            authority_epoch: u32,
            public_key: [u8; 32],
            authority_config_hash: Hash32,
        },
        AuthorityEpochRevoked {
            game_id: u32,
            game_version: u32,
            mode_id: u32,
            authority_epoch: u32,
        },
        RewardPolicyPublished {
            key: (u32, u32, u32, u32),
            policy_hash: Hash32,
            economic_realm: EconomicRealm,
            practice_only: bool,
        },
        RewardPolicyActivationChanged {
            key: (u32, u32, u32, u32),
            active: bool,
        },
        GameSessionAuthorized {
            owner: T::AccountId,
            session_id: SessionId,
            game_id: u32,
            mode_id: u32,
            policy_version: u32,
            economic_realm: EconomicRealm,
            roster_root: Hash32,
            cohort_hash: Hash32,
            encounter_id: Option<u32>,
            reward_day: u64,
            cohort_ordinal: u8,
            cohort_multiplier_bps: u16,
            expires_at: BlockNumberFor<T>,
        },
        GameResultAccepted {
            owner: T::AccountId,
            session_id: SessionId,
            result_id: Hash32,
            payload_hash: Hash32,
            xp_awarded: u128,
        },
        RandomDropRequested {
            session_id: SessionId,
            result_id: Hash32,
            request_id: Hash32,
        },
        RandomDropFinalized {
            session_id: SessionId,
            result_id: Hash32,
            charge_awarded: bool,
            prism_awarded: bool,
        },
        RandomDropTimedOut {
            session_id: SessionId,
            result_id: Hash32,
        },
        RandomDropUnavailable {
            session_id: SessionId,
            result_id: Hash32,
        },
        FirstEncounterClearRecorded {
            owner: T::AccountId,
            game_id: u32,
            encounter_id: u32,
            result_id: Hash32,
        },
        GameSessionExpired {
            owner: T::AccountId,
            session_id: SessionId,
        },
        GameSessionEmergencyAborted {
            owner: T::AccountId,
            session_id: SessionId,
        },
        RewardBudgetExhausted {
            key: (u32, u32, u32, u32),
        },
        ResultEpochSealed {
            epoch: u64,
            terminal_root: Hash32,
            session_count: u32,
        },
        DeterministicPrismQuestPolicyPublished {
            policy_key: (u32, u32, u32, u32),
            encounter_id: u32,
            quest_hash: Hash32,
            prism_definition_id: u32,
            economic_realm: EconomicRealm,
        },
        DeterministicPrismQuestRewardClaimed {
            owner: T::AccountId,
            session_id: SessionId,
            result_id: Hash32,
            quest_hash: Hash32,
            prism_definition_id: u32,
        },
        SessionAuthorizationTicketConsumed {
            owner: T::AccountId,
            authorization_id: Hash32,
            ticket_hash: Hash32,
            session_id: SessionId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        AuthorityAlreadyRegistered,
        AuthorityMissing,
        AuthorityRevoked,
        AuthorityNotActive,
        InvalidSignature,
        SignatureTooLong,
        PolicyAlreadyPublished,
        PolicyMissing,
        PolicyInactive,
        InvalidPolicy,
        RewardDefinitionMissing,
        PolicyRealmMismatch,
        BudgetMissing,
        RewardBudgetInsufficient,
        SessionIdExhausted,
        TooManyActiveSessions,
        TooManyPendingDrops,
        InvalidExpiry,
        InvalidEncounter,
        EmptyEntityRoster,
        AntiFarmLimitReached,
        TooManyEntities,
        TooManyPrisms,
        TooManyChargeDefinitions,
        DuplicateAsset,
        NormalizedPersistentAssetRejected,
        PersistentLoadoutRejected,
        SessionMissing,
        SessionNotActive,
        SessionExpired,
        SessionNotExpired,
        HeaderMismatch,
        ResultNamespaceMismatch,
        BodyModeMismatch,
        ResultAccountMismatch,
        ResultIdConflict,
        SecondFinalResult,
        ResultMetricsInvalid,
        ChargeUseInvalid,
        PrismUseInvalid,
        DropMissing,
        DropNotReady,
        DropNotTimedOut,
        EpochStillOpen,
        EpochNotTerminal,
        EpochDisputeWindowOpen,
        EpochAlreadySealed,
        EpochResultLimit,
        SealedEpochReplay,
        ProductionRandomnessUnavailable,
        ArithmeticOverflow,
        QuestPolicyAlreadyPublished,
        InvalidQuestPolicy,
        QuestDefinitionConflict,
        QuestPolicyRequiresInactiveRewardPolicy,
        LegacySessionRequiresPractice,
        SessionAuthorizationTicketInvalid,
        SessionAuthorizationTicketConflict,
        SessionAuthorizationSignatureInvalid,
        SessionAuthorizationReceiptLimit,
        TooManyActiveSessionsForAuthority,
        EpochAuthorizationTicketsLive,
        DuplicateResultAssetUse,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let on_chain = StorageVersion::get::<Pallet<T>>();
            if on_chain < STORAGE_VERSION {
                STORAGE_VERSION.put::<Pallet<T>>();
                T::DbWeight::get().reads_writes(1, 1)
            } else {
                T::DbWeight::get().reads(1)
            }
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::publish_policy())]
        pub fn register_authority_epoch(
            origin: OriginFor<T>,
            game_id: u32,
            game_version: u32,
            mode_id: u32,
            authority_epoch: u32,
            record: AuthorityEpoch<BlockNumberFor<T>>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            let key = (game_id, game_version, mode_id, authority_epoch);
            ensure!(
                !AuthorityEpochs::<T>::contains_key(key),
                Error::<T>::AuthorityAlreadyRegistered
            );
            ensure!(
                record.public_key.iter().any(|byte| *byte != 0)
                    && record.authority_config_hash.iter().any(|byte| *byte != 0)
                    && record.active_from < record.active_until
                    && !record.revoked,
                Error::<T>::InvalidPolicy
            );
            AuthorityEpochs::<T>::insert(key, record);
            Self::deposit_event(Event::AuthorityEpochRegistered {
                game_id,
                game_version,
                mode_id,
                authority_epoch,
                public_key: record.public_key,
                authority_config_hash: record.authority_config_hash,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::publish_policy())]
        pub fn revoke_authority_epoch(
            origin: OriginFor<T>,
            game_id: u32,
            game_version: u32,
            mode_id: u32,
            authority_epoch: u32,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            let key = (game_id, game_version, mode_id, authority_epoch);
            AuthorityEpochs::<T>::try_mutate(key, |maybe| -> DispatchResult {
                let record = maybe.as_mut().ok_or(Error::<T>::AuthorityMissing)?;
                record.revoked = true;
                Ok(())
            })?;
            Self::deposit_event(Event::AuthorityEpochRevoked {
                game_id,
                game_version,
                mode_id,
                authority_epoch,
            });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::publish_policy())]
        pub fn publish_reward_policy(
            origin: OriginFor<T>,
            policy: RewardPolicy,
            budget: RewardBudget,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(policy.validate(), Error::<T>::InvalidPolicy);
            ensure!(
                u32::from(policy.loadout.max_entities) <= T::MaxSessionEntities::get()
                    && u32::from(policy.loadout.max_prisms) <= T::MaxSessionPrisms::get()
                    && u32::from(policy.loadout.max_charge_definitions)
                        <= T::MaxChargeDefinitions::get(),
                Error::<T>::InvalidPolicy
            );
            T::Magic::validate_reward_definitions(
                policy.charge_definition_id,
                policy.prism_definition_id,
            )
            .map_err(|_| Error::<T>::RewardDefinitionMissing)?;
            let key = policy.key();
            ensure!(
                !RewardPolicies::<T>::contains_key(key),
                Error::<T>::PolicyAlreadyPublished
            );
            ensure!(
                budget.xp_reserved == 0
                    && budget.xp_spent == 0
                    && budget.essence_reserved == 0
                    && budget.essence_spent == 0
                    && budget.charge_slots_reserved == 0
                    && budget.charge_slots_spent == 0
                    && budget.prism_slots_reserved == 0
                    && budget.prism_slots_spent == 0,
                Error::<T>::InvalidPolicy
            );
            RewardPolicies::<T>::insert(key, policy);
            RewardBudgets::<T>::insert(key, budget);
            Self::deposit_event(Event::RewardPolicyPublished {
                key,
                policy_hash: policy.policy_hash,
                economic_realm: policy.economic_realm,
                practice_only: policy.practice_only,
            });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::publish_policy())]
        pub fn set_reward_policy_activation(
            origin: OriginFor<T>,
            key: (u32, u32, u32, u32),
            active: bool,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                RewardPolicies::<T>::contains_key(key),
                Error::<T>::PolicyMissing
            );
            if active {
                let policy = RewardPolicies::<T>::get(key).ok_or(Error::<T>::PolicyMissing)?;
                if policy.economic_realm == EconomicRealm::Production && !policy.practice_only {
                    ensure!(
                        T::Randomness::current_mode() == RandomnessMode::DrandQuicknet
                            && T::Randomness::production_ready(),
                        Error::<T>::ProductionRandomnessUnavailable
                    );
                }
            }
            RewardPolicyActivation::<T>::insert(key, active);
            if active {
                RewardPolicyEverActivated::<T>::insert(key, true);
            }
            Self::deposit_event(Event::RewardPolicyActivationChanged { key, active });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::authorize_session(
            entities.len().saturating_add(prisms.len()).saturating_add(charges.len()) as u32
        ))]
        #[transactional]
        pub fn authorize_session(
            origin: OriginFor<T>,
            game_id: u32,
            game_version: u32,
            mode_id: u32,
            policy_version: u32,
            authority_epoch: u32,
            economic_realm: EconomicRealm,
            cohort_hash: Hash32,
            encounter_id: Option<u32>,
            entities: Vec<AssetRevision>,
            prisms: Vec<AssetRevision>,
            charges: Vec<ChargeUse>,
            expires_at: BlockNumberFor<T>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&owner)?;
            Self::do_authorize_session(
                owner,
                game_id,
                game_version,
                mode_id,
                policy_version,
                authority_epoch,
                economic_realm,
                cohort_hash,
                encounter_id,
                entities,
                prisms,
                charges,
                expires_at,
                T::Randomness::current_mode(),
                true,
            )?;
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::submit_result(
            T::MaxSessionEntities::get()
                .saturating_add(T::MaxSessionPrisms::get())
                .saturating_add(T::MaxChargeDefinitions::get())
        ))]
        #[transactional]
        pub fn submit_result(origin: OriginFor<T>, result: ResultOf<T>) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            Self::validate_result_namespace(result.header.session_id, &result.header.result_id)?;
            let payload_hash = Self::result_payload_hash(&result.header, &result.body);
            if let Some(existing) = ProcessedResults::<T>::get(result.header.result_id) {
                ensure!(existing == payload_hash, Error::<T>::ResultIdConflict);
                return Ok(());
            }
            let epoch = Self::session_epoch(result.header.session_id);
            ensure!(
                !SealedResultEpochs::<T>::contains_key(epoch),
                Error::<T>::SealedEpochReplay
            );
            let mut session =
                Sessions::<T>::get(result.header.session_id).ok_or(Error::<T>::SessionMissing)?;
            ensure!(
                session.status == SessionStatus::Active,
                Error::<T>::SecondFinalResult
            );
            let now = frame_system::Pallet::<T>::block_number();
            ensure!(now <= session.expires_at, Error::<T>::SessionExpired);
            Self::validate_header(&session, &result.header)?;
            let authority = AuthorityEpochs::<T>::get((
                session.game_id,
                session.game_version,
                session.mode_id,
                session.authority_epoch,
            ))
            .ok_or(Error::<T>::AuthorityMissing)?;
            ensure!(!authority.revoked, Error::<T>::AuthorityRevoked);
            ensure!(
                T::SignatureVerifier::verify(
                    &authority.public_key,
                    &payload_hash,
                    result.server_signature.as_slice(),
                ),
                Error::<T>::InvalidSignature
            );
            let key = (
                session.game_id,
                session.game_version,
                session.mode_id,
                session.policy_version,
            );
            let policy = RewardPolicies::<T>::get(key).ok_or(Error::<T>::PolicyMissing)?;
            let xp_awarded = match &result.body {
                ResultBodyV1::RpgBattle(body) => {
                    ensure!(
                        policy.mode_kind == GameModeKind::Legends,
                        Error::<T>::BodyModeMismatch
                    );
                    Self::validate_rpg_body(&session, &policy, body)?;
                    Self::settle_rpg(&session, &policy, &result.header, body)?
                }
                ResultBodyV1::FpsMatch(body) => {
                    ensure!(
                        policy.mode_kind != GameModeKind::Legends,
                        Error::<T>::BodyModeMismatch
                    );
                    Self::validate_fps_body(&session, &policy, body)?;
                    Self::settle_fps(&session, &policy, &result.header, body)?
                }
            };
            let pending_drop = PendingDrops::<T>::contains_key(session.session_id);
            session.pending_drop_slot_reserved = pending_drop;
            session.status = if pending_drop {
                SessionStatus::SettledPendingDrop
            } else {
                SessionStatus::Settled
            };
            session.result_id = Some(result.header.result_id);
            Sessions::<T>::insert(session.session_id, &session);
            SettledSessions::<T>::insert(session.session_id, result.header.result_id);
            ProcessedResults::<T>::insert(result.header.result_id, payload_hash);
            EpochResultIds::<T>::try_mutate(epoch, |ids| -> DispatchResult {
                ids.try_push(result.header.result_id)
                    .map_err(|_| Error::<T>::EpochResultLimit)?;
                Ok(())
            })?;
            Self::decrement_active_session_counts(&session)?;
            Self::release_asset_locks(&session, &result.body)?;
            if !pending_drop {
                Self::mark_terminal(session.session_id, result.header.result_id)?;
            }
            Self::deposit_event(Event::GameResultAccepted {
                owner: session.owner,
                session_id: session.session_id,
                result_id: result.header.result_id,
                payload_hash,
                xp_awarded,
            });
            Ok(())
        }

        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::expire_session(
            T::MaxSessionEntities::get().saturating_add(T::MaxSessionPrisms::get())
        ))]
        #[transactional]
        pub fn expire_session(origin: OriginFor<T>, session_id: SessionId) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            let mut session = Sessions::<T>::get(session_id).ok_or(Error::<T>::SessionMissing)?;
            ensure!(
                session.status == SessionStatus::Active,
                Error::<T>::SessionNotActive
            );
            ensure!(
                frame_system::Pallet::<T>::block_number()
                    >= session.expires_at.saturating_add(T::ExpiryGrace::get()),
                Error::<T>::SessionNotExpired
            );
            let key = (
                session.game_id,
                session.game_version,
                session.mode_id,
                session.policy_version,
            );
            let policy = RewardPolicies::<T>::get(key).ok_or(Error::<T>::PolicyMissing)?;
            Self::release_reward_budget(key, session.reward_liability)?;
            Self::release_anti_farm_reservations(&session, &policy)?;
            if session.pending_drop_slot_reserved {
                Self::release_pending_drop_liability(&session.owner)?;
                session.pending_drop_slot_reserved = false;
            }
            Self::release_all_assets_force(&session)?;
            session.status = SessionStatus::Expired;
            Sessions::<T>::insert(session_id, &session);
            Self::decrement_active_session_counts(&session)?;
            Self::mark_terminal(
                session_id,
                sp_io::hashing::blake2_256(&(b"EXPIRED", session_id).encode()),
            )?;
            Self::deposit_event(Event::GameSessionExpired {
                owner: session.owner,
                session_id,
            });
            Ok(())
        }

        /// Governance recovery path for a compromised or externally unlocked
        /// active session. It grants nothing, releases every reservation, and
        /// makes the session terminal immediately.
        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::expire_session(
            T::MaxSessionEntities::get().saturating_add(T::MaxSessionPrisms::get())
        ))]
        #[transactional]
        pub fn emergency_abort_session(
            origin: OriginFor<T>,
            session_id: SessionId,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            let mut session = Sessions::<T>::get(session_id).ok_or(Error::<T>::SessionMissing)?;
            ensure!(
                session.status == SessionStatus::Active,
                Error::<T>::SessionNotActive
            );
            let key = (
                session.game_id,
                session.game_version,
                session.mode_id,
                session.policy_version,
            );
            let policy = RewardPolicies::<T>::get(key).ok_or(Error::<T>::PolicyMissing)?;
            Self::release_reward_budget(key, session.reward_liability)?;
            Self::release_anti_farm_reservations(&session, &policy)?;
            if session.pending_drop_slot_reserved {
                Self::release_pending_drop_liability(&session.owner)?;
                session.pending_drop_slot_reserved = false;
            }
            Self::release_all_assets_force(&session)?;
            session.status = SessionStatus::Aborted;
            Sessions::<T>::insert(session_id, &session);
            Self::decrement_active_session_counts(&session)?;
            let terminal_hash =
                sp_io::hashing::blake2_256(&(b"EMERGENCY_ABORTED", session_id).encode());
            Self::mark_terminal(session_id, terminal_hash)?;
            Self::deposit_event(Event::GameSessionEmergencyAborted {
                owner: session.owner,
                session_id,
            });
            Ok(())
        }

        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::finalize_drop())]
        #[transactional]
        pub fn finalize_drop(origin: OriginFor<T>, session_id: SessionId) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            if PendingDrops::<T>::get(session_id).is_none() {
                let session = Sessions::<T>::get(session_id).ok_or(Error::<T>::DropMissing)?;
                ensure!(
                    session.status == SessionStatus::Settled && session.result_id.is_some(),
                    Error::<T>::DropMissing
                );
                return Ok(());
            }
            let pending = PendingDrops::<T>::get(session_id).ok_or(Error::<T>::DropMissing)?;
            let output = T::Randomness::output_for(
                pending.request_id,
                pending.economic_realm,
                pending.randomness_provenance,
            )
            .ok_or(Error::<T>::DropNotReady)?;
            Self::resolve_drop(pending, output.output, false)
        }

        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::finalize_drop())]
        #[transactional]
        pub fn finalize_drop_timeout(
            origin: OriginFor<T>,
            session_id: SessionId,
        ) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            if PendingDrops::<T>::get(session_id).is_none() {
                let session = Sessions::<T>::get(session_id).ok_or(Error::<T>::DropMissing)?;
                ensure!(
                    session.status == SessionStatus::Settled && session.result_id.is_some(),
                    Error::<T>::DropMissing
                );
                return Ok(());
            }
            let pending = PendingDrops::<T>::get(session_id).ok_or(Error::<T>::DropMissing)?;
            ensure!(
                T::Randomness::timed_out(pending.request_id),
                Error::<T>::DropNotTimedOut
            );
            Self::resolve_drop(pending, [0; 32], true)
        }

        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::seal_epoch(
            (T::ResultEpochSize::get() as u32)
                .saturating_add(T::MaxResultsPerEpoch::get())
                .saturating_add(T::MaxSessionAuthorizationReceiptsPerEpoch::get())
        ))]
        #[transactional]
        pub fn seal_result_epoch(origin: OriginFor<T>, epoch: u64) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            ensure!(
                !SealedResultEpochs::<T>::contains_key(epoch),
                Error::<T>::EpochAlreadySealed
            );
            let current_epoch = Self::session_epoch(NextSessionId::<T>::get().saturating_add(1));
            ensure!(epoch < current_epoch, Error::<T>::EpochStillOpen);
            let count = EpochSessionCount::<T>::get(epoch);
            ensure!(
                count > 0 && count == EpochTerminalCount::<T>::get(epoch),
                Error::<T>::EpochNotTerminal
            );
            let last = EpochLastTerminalAt::<T>::get(epoch).ok_or(Error::<T>::EpochNotTerminal)?;
            ensure!(
                frame_system::Pallet::<T>::block_number()
                    >= last.saturating_add(T::ResultDisputeWindow::get()),
                Error::<T>::EpochDisputeWindowOpen
            );
            if let Some(max_expiry) = EpochAuthorizationMaxExpiry::<T>::get(epoch) {
                ensure!(
                    frame_system::Pallet::<T>::block_number() >= max_expiry,
                    Error::<T>::EpochAuthorizationTicketsLive
                );
            }
            let terminal_root = EpochTerminalAccumulator::<T>::get(epoch);
            for result_id in EpochResultIds::<T>::take(epoch) {
                ProcessedResults::<T>::remove(result_id);
            }
            for authorization_id in EpochAuthorizationIds::<T>::take(epoch) {
                SessionAuthorizationReceipts::<T>::remove(authorization_id);
            }
            EpochAuthorizationMaxExpiry::<T>::remove(epoch);
            let start = epoch
                .saturating_mul(T::ResultEpochSize::get())
                .saturating_add(1);
            let end = start.saturating_add(T::ResultEpochSize::get());
            for session_id in start..end {
                Sessions::<T>::remove(session_id);
                SettledSessions::<T>::remove(session_id);
            }
            let sealed = SealedResultEpoch {
                epoch,
                terminal_root,
                session_count: count,
            };
            SealedResultEpochs::<T>::insert(epoch, sealed);
            Self::deposit_event(Event::ResultEpochSealed {
                epoch,
                terminal_root,
                session_count: count,
            });
            Ok(())
        }

        /// Publish the immutable no-purchase Prism path for one encounter.
        /// This must happen before the parent reward policy is activated, so
        /// every authorized session snapshots the same maximum liability.
        #[pallet::call_index(11)]
        #[pallet::weight(T::WeightInfo::publish_policy())]
        pub fn publish_deterministic_prism_quest_policy(
            origin: OriginFor<T>,
            policy_key: (u32, u32, u32, u32),
            quest: DeterministicPrismQuestPolicy,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !RewardPolicyActivation::<T>::get(policy_key)
                    && !RewardPolicyEverActivated::<T>::get(policy_key),
                Error::<T>::QuestPolicyRequiresInactiveRewardPolicy
            );
            let reward_policy =
                RewardPolicies::<T>::get(policy_key).ok_or(Error::<T>::PolicyMissing)?;
            ensure!(
                reward_policy.mode_kind == GameModeKind::Legends
                    && !reward_policy.practice_only
                    && reward_policy.economic_realm == quest.economic_realm
                    && quest.encounter_id > 0
                    && quest.prism_definition_id > 0
                    && quest.quest_hash.iter().any(|byte| *byte != 0),
                Error::<T>::InvalidQuestPolicy
            );
            let storage_key = (policy_key, quest.encounter_id);
            ensure!(
                !DeterministicPrismQuestPolicies::<T>::contains_key(storage_key),
                Error::<T>::QuestPolicyAlreadyPublished
            );
            if let Some(definition_id) =
                DeterministicPrismQuestDefinitions::<T>::get(quest.quest_hash)
            {
                ensure!(
                    definition_id == quest.prism_definition_id,
                    Error::<T>::QuestDefinitionConflict
                );
            }
            T::Magic::validate_reward_definitions(None, Some(quest.prism_definition_id))
                .map_err(|_| Error::<T>::RewardDefinitionMissing)?;
            DeterministicPrismQuestPolicies::<T>::insert(storage_key, quest);
            DeterministicPrismQuestDefinitions::<T>::insert(
                quest.quest_hash,
                quest.prism_definition_id,
            );
            Self::deposit_event(Event::DeterministicPrismQuestPolicyPublished {
                policy_key,
                encounter_id: quest.encounter_id,
                quest_hash: quest.quest_hash,
                prism_definition_id: quest.prism_definition_id,
                economic_realm: quest.economic_realm,
            });
            Ok(())
        }

        /// Consume an authority-signed, single-use session-start ticket.
        ///
        /// Exact replays are idempotent. Reusing an authorization ID with any
        /// other ticket payload is rejected, and receipts remain live until
        /// the result epoch can be sealed after the ticket's expiry.
        #[pallet::call_index(12)]
        #[pallet::weight(T::WeightInfo::authorize_session_with_ticket(
            entities.len().saturating_add(prisms.len()).saturating_add(charges.len()) as u32,
            server_signature.len() as u32,
        ))]
        #[transactional]
        pub fn authorize_session_with_ticket(
            origin: OriginFor<T>,
            ticket: SessionAuthorizationTicketOf<T>,
            entities: Vec<AssetRevision>,
            prisms: Vec<AssetRevision>,
            charges: Vec<ChargeUse>,
            server_signature: Vec<u8>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            ensure!(
                ticket.authorization_id != [0; 32] && ticket.owner == owner,
                Error::<T>::SessionAuthorizationTicketInvalid
            );
            let ticket_hash =
                Self::session_authorization_payload_hash(&ticket, &entities, &prisms, &charges);
            if let Some(receipt) = SessionAuthorizationReceipts::<T>::get(ticket.authorization_id) {
                ensure!(
                    receipt.ticket_hash == ticket_hash,
                    Error::<T>::SessionAuthorizationTicketConflict
                );
                return Ok(());
            }
            T::AccessControl::ensure_whitelisted(&owner)?;
            ensure!(
                ticket.protocol_version == 1
                    && ticket.genesis_hash == T::GenesisHashProvider::genesis_hash()
                    && ticket.pallet_instance_id == T::PalletInstanceId::get()
                    && ticket.owner == owner,
                Error::<T>::SessionAuthorizationTicketInvalid
            );
            ensure!(
                u32::try_from(entities.len()).unwrap_or(u32::MAX) <= T::MaxSessionEntities::get()
                    && u32::try_from(prisms.len()).unwrap_or(u32::MAX)
                        <= T::MaxSessionPrisms::get()
                    && u32::try_from(charges.len()).unwrap_or(u32::MAX)
                        <= T::MaxChargeDefinitions::get(),
                Error::<T>::SessionAuthorizationTicketInvalid
            );
            let roster_root = Self::session_roster_root(
                ticket.game_id,
                ticket.game_version,
                ticket.mode_id,
                ticket.policy_version,
                ticket.economic_realm,
                ticket.encounter_id,
                &entities,
                &prisms,
                &charges,
            );
            ensure!(
                ticket.roster_root == roster_root
                    && ticket.expected_randomness_provenance == T::Randomness::current_mode(),
                Error::<T>::SessionAuthorizationTicketInvalid
            );
            let policy_key = (
                ticket.game_id,
                ticket.game_version,
                ticket.mode_id,
                ticket.policy_version,
            );
            let policy = RewardPolicies::<T>::get(policy_key).ok_or(Error::<T>::PolicyMissing)?;
            ensure!(
                policy.policy_hash == ticket.policy_hash,
                Error::<T>::SessionAuthorizationTicketInvalid
            );
            let authority_key = (
                ticket.game_id,
                ticket.game_version,
                ticket.mode_id,
                ticket.authority_epoch,
            );
            let authority =
                AuthorityEpochs::<T>::get(authority_key).ok_or(Error::<T>::AuthorityMissing)?;
            ensure!(
                authority.authority_config_hash == ticket.authority_config_hash,
                Error::<T>::SessionAuthorizationTicketInvalid
            );
            let signature: SignatureOf<T> = server_signature
                .try_into()
                .map_err(|_| Error::<T>::SignatureTooLong)?;
            ensure!(
                T::SignatureVerifier::verify(
                    &authority.public_key,
                    &ticket_hash,
                    signature.as_slice(),
                ),
                Error::<T>::SessionAuthorizationSignatureInvalid
            );

            let anticipated_session_id = NextSessionId::<T>::get()
                .checked_add(1)
                .ok_or(Error::<T>::SessionIdExhausted)?;
            let anticipated_epoch = Self::session_epoch(anticipated_session_id);
            ensure!(
                (EpochAuthorizationIds::<T>::get(anticipated_epoch).len() as u32)
                    < T::MaxSessionAuthorizationReceiptsPerEpoch::get(),
                Error::<T>::SessionAuthorizationReceiptLimit
            );

            let session_id = Self::do_authorize_session(
                owner.clone(),
                ticket.game_id,
                ticket.game_version,
                ticket.mode_id,
                ticket.policy_version,
                ticket.authority_epoch,
                ticket.economic_realm,
                ticket.cohort_hash,
                ticket.encounter_id,
                entities,
                prisms,
                charges,
                ticket.expires_at,
                ticket.expected_randomness_provenance,
                false,
            )?;
            ensure!(
                session_id == anticipated_session_id,
                Error::<T>::ArithmeticOverflow
            );
            EpochAuthorizationIds::<T>::try_mutate(
                anticipated_epoch,
                |authorization_ids| -> DispatchResult {
                    authorization_ids
                        .try_push(ticket.authorization_id)
                        .map_err(|_| Error::<T>::SessionAuthorizationReceiptLimit)?;
                    Ok(())
                },
            )?;
            EpochAuthorizationMaxExpiry::<T>::mutate(anticipated_epoch, |max_expiry| {
                if match max_expiry.as_ref() {
                    Some(current) => ticket.expires_at > *current,
                    None => true,
                } {
                    *max_expiry = Some(ticket.expires_at);
                }
            });
            SessionAuthorizationReceipts::<T>::insert(
                ticket.authorization_id,
                SessionAuthorizationReceipt {
                    authorization_id: ticket.authorization_id,
                    ticket_hash,
                    session_id,
                    session_epoch: anticipated_epoch,
                },
            );
            Self::deposit_event(Event::SessionAuthorizationTicketConsumed {
                owner,
                authorization_id: ticket.authorization_id,
                ticket_hash,
                session_id,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn session_authorization_payload_hash(
            ticket: &SessionAuthorizationTicketOf<T>,
            entities: &[AssetRevision],
            prisms: &[AssetRevision],
            charges: &[ChargeUse],
        ) -> Hash32 {
            sp_io::hashing::blake2_256(
                &(
                    SESSION_AUTHORIZATION_DOMAIN,
                    T::PalletInstanceId::get(),
                    ticket,
                    entities,
                    prisms,
                    charges,
                )
                    .encode(),
            )
        }

        fn do_authorize_session(
            owner: T::AccountId,
            game_id: u32,
            game_version: u32,
            mode_id: u32,
            policy_version: u32,
            authority_epoch: u32,
            economic_realm: EconomicRealm,
            cohort_hash: Hash32,
            encounter_id: Option<u32>,
            entities: Vec<AssetRevision>,
            prisms: Vec<AssetRevision>,
            charges: Vec<ChargeUse>,
            expires_at: BlockNumberFor<T>,
            expected_randomness_provenance: RandomnessMode,
            legacy_practice_only: bool,
        ) -> Result<SessionId, DispatchError> {
            let policy_key = (game_id, game_version, mode_id, policy_version);
            let policy = RewardPolicies::<T>::get(policy_key).ok_or(Error::<T>::PolicyMissing)?;
            ensure!(
                RewardPolicyActivation::<T>::get(policy_key),
                Error::<T>::PolicyInactive
            );
            ensure!(
                policy.economic_realm == economic_realm,
                Error::<T>::PolicyRealmMismatch
            );
            if legacy_practice_only {
                ensure!(
                    economic_realm == EconomicRealm::Training && policy.practice_only,
                    Error::<T>::LegacySessionRequiresPractice
                );
            }

            let authority_key = (game_id, game_version, mode_id, authority_epoch);
            let authority =
                AuthorityEpochs::<T>::get(authority_key).ok_or(Error::<T>::AuthorityMissing)?;
            ensure!(!authority.revoked, Error::<T>::AuthorityRevoked);
            let now = frame_system::Pallet::<T>::block_number();
            ensure!(
                now >= authority.active_from && now <= authority.active_until,
                Error::<T>::AuthorityNotActive
            );
            ensure!(
                expires_at > now
                    && expires_at <= now.saturating_add(T::MaxSessionLifetime::get())
                    && expires_at <= authority.active_until,
                Error::<T>::InvalidExpiry
            );
            ensure!(
                expected_randomness_provenance == T::Randomness::current_mode(),
                Error::<T>::SessionAuthorizationTicketInvalid
            );
            if economic_realm == EconomicRealm::Production {
                ensure!(
                    expected_randomness_provenance == RandomnessMode::DrandQuicknet
                        && T::Randomness::production_ready(),
                    Error::<T>::ProductionRandomnessUnavailable
                );
            }

            if policy.normalized {
                ensure!(
                    entities.is_empty() && prisms.is_empty() && charges.is_empty(),
                    Error::<T>::NormalizedPersistentAssetRejected
                );
            }
            ensure!(
                entities.len() <= policy.loadout.max_entities as usize
                    && prisms.len() <= policy.loadout.max_prisms as usize
                    && charges.len() <= policy.loadout.max_charge_definitions as usize,
                Error::<T>::PersistentLoadoutRejected
            );
            Self::ensure_distinct_assets(&entities, &prisms, &charges)?;
            if policy.mode_kind == GameModeKind::Legends {
                ensure!(!entities.is_empty(), Error::<T>::EmptyEntityRoster);
                ensure!(
                    encounter_id.is_some_and(|value| value > 0),
                    Error::<T>::InvalidEncounter
                );
            } else {
                ensure!(encounter_id.is_none(), Error::<T>::InvalidEncounter);
            }
            if let Some((format_id, format_version)) = policy.loadout.entity_format {
                for entity in &entities {
                    T::Entities::validate_session_entity(
                        &owner,
                        economic_realm,
                        entity.asset_id,
                        entity.revision,
                        format_id,
                        format_version,
                        policy.loadout.allowed_entity_roles_mask,
                    )?;
                }
            } else {
                ensure!(entities.is_empty(), Error::<T>::PersistentLoadoutRejected);
            }
            let prism_pairs: Vec<_> = prisms
                .iter()
                .map(|asset| (asset.asset_id, asset.revision))
                .collect();
            let charge_pairs: Vec<_> = charges
                .iter()
                .map(|charge| (charge.definition_id, charge.amount))
                .collect();
            T::Magic::validate_session_loadout(
                &owner,
                economic_realm,
                MagicLoadoutLimits {
                    max_magic_load: policy.loadout.max_magic_load,
                    max_prisms: policy.loadout.max_prisms,
                    max_charge_definitions: policy.loadout.max_charge_definitions,
                    max_total_charges: policy.loadout.max_total_charges,
                },
                prism_pairs.as_slice(),
                charge_pairs.as_slice(),
            )?;

            ensure!(
                ActiveSessionCount::<T>::get(&owner) < T::MaxActiveSessionsPerAccount::get(),
                Error::<T>::TooManyActiveSessions
            );
            ensure!(
                ActiveSessionCountByAuthority::<T>::get(authority_key)
                    < T::MaxActiveSessionsPerAuthority::get(),
                Error::<T>::TooManyActiveSessionsForAuthority
            );
            let pending_drop_slot_reserved = Self::policy_may_drop(&policy);
            if pending_drop_slot_reserved {
                ensure!(
                    PendingDropLiabilityCount::<T>::get(&owner)
                        < T::MaxPendingDropsPerAccount::get(),
                    Error::<T>::TooManyPendingDrops
                );
            }

            let entities: EntityListOf<T> = entities
                .try_into()
                .map_err(|_| Error::<T>::TooManyEntities)?;
            let prisms: PrismListOf<T> =
                prisms.try_into().map_err(|_| Error::<T>::TooManyPrisms)?;
            let charge_allowance: ChargeListOf<T> = charges
                .try_into()
                .map_err(|_| Error::<T>::TooManyChargeDefinitions)?;
            let reward_day = Self::reward_day(now);
            let (cohort_ordinal, cohort_multiplier_bps, fps_xp_liability) =
                Self::reserve_fps_anti_farm(
                    &owner,
                    policy_key,
                    &policy,
                    economic_realm,
                    reward_day,
                    cohort_hash,
                )?;
            if let Some(encounter) = encounter_id {
                Self::reserve_entity_encounter_rewards(
                    &owner,
                    economic_realm,
                    game_id,
                    encounter,
                    reward_day,
                    &entities,
                    &policy,
                )?;
            }
            let deterministic_prism_quest = if policy.practice_only {
                None
            } else {
                encounter_id.and_then(|encounter| {
                    DeterministicPrismQuestPolicies::<T>::get((policy_key, encounter))
                })
            };
            let reward_liability = Self::reward_liability(
                &policy,
                entities.len() as u32,
                fps_xp_liability,
                deterministic_prism_quest.is_some(),
            )?;
            if legacy_practice_only {
                ensure!(
                    reward_liability == RewardLiability::default()
                        && deterministic_prism_quest.is_none(),
                    Error::<T>::LegacySessionRequiresPractice
                );
            }
            Self::reserve_reward_budget(policy_key, reward_liability)?;

            let session_id = NextSessionId::<T>::get()
                .checked_add(1)
                .ok_or(Error::<T>::SessionIdExhausted)?;
            let roster_root = Self::session_roster_root(
                game_id,
                game_version,
                mode_id,
                policy_version,
                economic_realm,
                encounter_id,
                entities.as_slice(),
                prisms.as_slice(),
                charge_allowance.as_slice(),
            );
            let lock = |role, revision_at_lock| AssetLock {
                session_id,
                role,
                revision_at_lock,
                expires_at,
            };
            for entity in &entities {
                T::Entities::lock_entity(
                    &owner,
                    entity.asset_id,
                    lock(AssetRole::Entity, entity.revision),
                )?;
            }
            for prism in &prisms {
                T::Magic::lock_prism(
                    &owner,
                    prism.asset_id,
                    lock(AssetRole::PrismSpell, prism.revision),
                )?;
            }
            if !charge_allowance.is_empty() {
                T::Magic::reserve_charges(
                    session_id,
                    &owner,
                    economic_realm,
                    charge_pairs.as_slice(),
                )?;
            }
            if pending_drop_slot_reserved {
                PendingDropLiabilityCount::<T>::try_mutate(&owner, |count| -> DispatchResult {
                    *count = count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                })?;
            }
            NextSessionId::<T>::put(session_id);
            Sessions::<T>::insert(
                session_id,
                SessionRecord {
                    session_id,
                    owner: owner.clone(),
                    game_id,
                    game_version,
                    mode_id,
                    policy_version,
                    authority_epoch,
                    economic_realm,
                    roster_root,
                    cohort_hash,
                    encounter_id,
                    reward_day,
                    cohort_ordinal,
                    cohort_multiplier_bps,
                    reward_liability,
                    pending_drop_slot_reserved,
                    entities,
                    prisms,
                    charge_allowance,
                    expires_at,
                    status: SessionStatus::Active,
                    result_id: None,
                    randomness_provenance: expected_randomness_provenance,
                    deterministic_prism_quest,
                },
            );
            ActiveSessionCount::<T>::try_mutate(&owner, |count| -> DispatchResult {
                *count = count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })?;
            ActiveSessionCountByAuthority::<T>::try_mutate(
                authority_key,
                |count| -> DispatchResult {
                    *count = count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            let epoch = Self::session_epoch(session_id);
            EpochSessionCount::<T>::try_mutate(epoch, |count| -> DispatchResult {
                *count = count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })?;
            Self::deposit_event(Event::GameSessionAuthorized {
                owner,
                session_id,
                game_id,
                mode_id,
                policy_version,
                economic_realm,
                roster_root,
                cohort_hash,
                encounter_id,
                reward_day,
                cohort_ordinal,
                cohort_multiplier_bps,
                expires_at,
            });
            Ok(session_id)
        }

        pub fn deterministic_prism_quest_award_ids(
            owner: &T::AccountId,
            session_id: SessionId,
            verified_result_id: Hash32,
            quest: DeterministicPrismQuestPolicy,
        ) -> (Hash32, Hash32) {
            let award_preimage = (
                DETERMINISTIC_PRISM_QUEST_DOMAIN,
                T::GenesisHashProvider::genesis_hash(),
                T::PalletInstanceId::get(),
                session_id,
                verified_result_id,
                owner,
                quest.quest_hash,
                quest.encounter_id,
                quest.prism_definition_id,
                quest.economic_realm,
            );
            let traits_seed =
                sp_io::hashing::blake2_256(&(award_preimage, b"TRAITS_SEED").encode());
            let quest_result_id =
                sp_io::hashing::blake2_256(&(award_preimage, b"QUEST_RESULT").encode());
            (traits_seed, quest_result_id)
        }

        pub fn session_roster_root(
            game_id: u32,
            game_version: u32,
            mode_id: u32,
            policy_version: u32,
            economic_realm: EconomicRealm,
            encounter_id: Option<u32>,
            entities: &[AssetRevision],
            prisms: &[AssetRevision],
            charges: &[ChargeUse],
        ) -> Hash32 {
            sp_io::hashing::blake2_256(
                &(
                    ROSTER_DOMAIN,
                    T::PalletInstanceId::get(),
                    game_id,
                    game_version,
                    mode_id,
                    policy_version,
                    economic_realm,
                    encounter_id,
                    entities,
                    prisms,
                    charges,
                )
                    .encode(),
            )
        }

        fn session_epoch(session_id: SessionId) -> u64 {
            session_id.saturating_sub(1) / T::ResultEpochSize::get().max(1)
        }

        fn decrement_active_session_counts(session: &SessionOf<T>) -> DispatchResult {
            ActiveSessionCount::<T>::try_mutate(&session.owner, |count| -> DispatchResult {
                *count = count.checked_sub(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })?;
            ActiveSessionCountByAuthority::<T>::try_mutate(
                (
                    session.game_id,
                    session.game_version,
                    session.mode_id,
                    session.authority_epoch,
                ),
                |count| -> DispatchResult {
                    *count = count.checked_sub(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                },
            )
        }

        fn reward_day(now: BlockNumberFor<T>) -> u64 {
            now.saturated_into::<u64>() / T::RewardDayBlocks::get().max(1)
        }

        fn validate_result_namespace(session_id: SessionId, result_id: &Hash32) -> DispatchResult {
            ensure!(
                result_id[..8] == session_id.to_le_bytes(),
                Error::<T>::ResultNamespaceMismatch
            );
            Ok(())
        }

        fn policy_may_drop(policy: &RewardPolicy) -> bool {
            !policy.practice_only
                && ((policy.charge_definition_id.is_some() && policy.charge_drop_bps > 0)
                    || (policy.prism_definition_id.is_some() && policy.prism_drop_bps > 0))
        }

        fn policy_has_entity_encounter_rewards(policy: &RewardPolicy) -> bool {
            !policy.practice_only
                && policy.mode_kind == GameModeKind::Legends
                && (policy.entity_xp > 0
                    || policy.base_essence > 0
                    || Self::policy_may_drop(policy))
        }

        fn reward_liability(
            policy: &RewardPolicy,
            entity_count: u32,
            fps_xp_liability: u128,
            deterministic_prism_quest: bool,
        ) -> Result<RewardLiability, DispatchError> {
            if policy.practice_only {
                return Ok(RewardLiability::default());
            }
            let xp = if policy.mode_kind == GameModeKind::Legends {
                u128::from(policy.entity_xp)
                    .checked_mul(u128::from(entity_count))
                    .ok_or(Error::<T>::ArithmeticOverflow)?
            } else {
                fps_xp_liability
            };
            Ok(RewardLiability {
                xp,
                essence: if policy.mode_kind == GameModeKind::Legends {
                    u128::from(policy.base_essence)
                } else {
                    0
                },
                charge_slots: u64::from(
                    policy.charge_definition_id.is_some() && policy.charge_drop_bps > 0,
                ),
                prism_slots: u64::from(
                    policy.prism_definition_id.is_some() && policy.prism_drop_bps > 0,
                )
                .checked_add(u64::from(deterministic_prism_quest))
                .ok_or(Error::<T>::ArithmeticOverflow)?,
            })
        }

        fn reserve_fps_anti_farm(
            owner: &T::AccountId,
            key: (u32, u32, u32, u32),
            policy: &RewardPolicy,
            realm: EconomicRealm,
            reward_day: u64,
            cohort_hash: Hash32,
        ) -> Result<(u8, u16, u128), DispatchError> {
            if policy.practice_only
                || policy.mode_kind == GameModeKind::Legends
                || policy.max_player_xp == 0
            {
                return Ok((0, 10_000, 0));
            }
            let cohort_key = (key, reward_day, cohort_hash);
            let attempts = CohortAttemptCounts::<T>::get(owner, cohort_key);
            let multiplier = policy
                .repeat_cohort_multipliers_bps
                .get(attempts as usize)
                .copied()
                .unwrap_or(0);
            ensure!(multiplier > 0, Error::<T>::AntiFarmLimitReached);
            let liability = policy
                .max_player_xp
                .checked_mul(u128::from(multiplier))
                .ok_or(Error::<T>::ArithmeticOverflow)?
                / 10_000;
            ensure!(liability > 0, Error::<T>::AntiFarmLimitReached);
            DailyXpLedgers::<T>::try_mutate(
                owner,
                (realm, reward_day),
                |ledger| -> DispatchResult {
                    let committed = ledger
                        .awarded
                        .checked_add(ledger.reserved)
                        .and_then(|value| value.checked_add(liability))
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    ensure!(
                        committed <= policy.maximum_xp_per_day,
                        Error::<T>::AntiFarmLimitReached
                    );
                    ledger.reserved = ledger
                        .reserved
                        .checked_add(liability)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            CohortAttemptCounts::<T>::insert(
                owner,
                cohort_key,
                attempts
                    .checked_add(1)
                    .ok_or(Error::<T>::ArithmeticOverflow)?,
            );
            Ok((
                attempts.min(u32::from(u8::MAX)) as u8,
                multiplier,
                liability,
            ))
        }

        fn reserve_entity_encounter_rewards(
            owner: &T::AccountId,
            realm: EconomicRealm,
            game_id: u32,
            encounter_id: u32,
            reward_day: u64,
            entities: &EntityListOf<T>,
            policy: &RewardPolicy,
        ) -> DispatchResult {
            if !Self::policy_has_entity_encounter_rewards(policy) {
                return Ok(());
            }
            for entity in entities {
                EntityEncounterLedgers::<T>::try_mutate(
                    (owner.clone(), entity.asset_id),
                    (realm, game_id, encounter_id, reward_day),
                    |ledger| -> DispatchResult {
                        let committed = ledger
                            .rewarded
                            .checked_add(ledger.reserved)
                            .ok_or(Error::<T>::ArithmeticOverflow)?;
                        ensure!(
                            committed < policy.per_entity_encounter_rewards_per_day,
                            Error::<T>::AntiFarmLimitReached
                        );
                        ledger.reserved = ledger
                            .reserved
                            .checked_add(1)
                            .ok_or(Error::<T>::ArithmeticOverflow)?;
                        Ok(())
                    },
                )?;
            }
            Ok(())
        }

        fn ensure_distinct_assets(
            entities: &[AssetRevision],
            prisms: &[AssetRevision],
            charges: &[ChargeUse],
        ) -> DispatchResult {
            let entity_count = entities
                .iter()
                .map(|item| (item.asset_id, ()))
                .collect::<BTreeMap<_, _>>()
                .len();
            ensure!(entity_count == entities.len(), Error::<T>::DuplicateAsset);
            let prism_count = prisms
                .iter()
                .map(|item| (item.asset_id, ()))
                .collect::<BTreeMap<_, _>>()
                .len();
            ensure!(prism_count == prisms.len(), Error::<T>::DuplicateAsset);
            let charge_count = charges
                .iter()
                .map(|item| (item.definition_id, ()))
                .collect::<BTreeMap<_, _>>()
                .len();
            ensure!(charge_count == charges.len(), Error::<T>::DuplicateAsset);
            Ok(())
        }

        fn reserve_reward_budget(
            key: (u32, u32, u32, u32),
            liability: RewardLiability,
        ) -> DispatchResult {
            RewardBudgets::<T>::try_mutate(key, |maybe| -> DispatchResult {
                let budget = maybe.as_mut().ok_or(Error::<T>::BudgetMissing)?;
                let xp_committed = budget
                    .xp_spent
                    .checked_add(budget.xp_reserved)
                    .and_then(|value| value.checked_add(liability.xp))
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                let essence_committed = budget
                    .essence_spent
                    .checked_add(budget.essence_reserved)
                    .and_then(|value| value.checked_add(liability.essence))
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                let charge_committed = budget
                    .charge_slots_spent
                    .checked_add(budget.charge_slots_reserved)
                    .and_then(|value| value.checked_add(liability.charge_slots))
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                let prism_committed = budget
                    .prism_slots_spent
                    .checked_add(budget.prism_slots_reserved)
                    .and_then(|value| value.checked_add(liability.prism_slots))
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                let enough = xp_committed <= budget.xp_total
                    && essence_committed <= budget.essence_total
                    && charge_committed <= budget.charge_slots_total
                    && prism_committed <= budget.prism_slots_total;
                ensure!(enough, Error::<T>::RewardBudgetInsufficient);
                budget.xp_reserved = budget
                    .xp_reserved
                    .checked_add(liability.xp)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.essence_reserved = budget
                    .essence_reserved
                    .checked_add(liability.essence)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.charge_slots_reserved = budget
                    .charge_slots_reserved
                    .checked_add(liability.charge_slots)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.prism_slots_reserved = budget
                    .prism_slots_reserved
                    .checked_add(liability.prism_slots)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })
        }

        fn release_reward_budget(
            key: (u32, u32, u32, u32),
            liability: RewardLiability,
        ) -> DispatchResult {
            RewardBudgets::<T>::try_mutate(key, |maybe| -> DispatchResult {
                let budget = maybe.as_mut().ok_or(Error::<T>::BudgetMissing)?;
                budget.xp_reserved = budget
                    .xp_reserved
                    .checked_sub(liability.xp)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.essence_reserved = budget
                    .essence_reserved
                    .checked_sub(liability.essence)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.charge_slots_reserved = budget
                    .charge_slots_reserved
                    .checked_sub(liability.charge_slots)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.prism_slots_reserved = budget
                    .prism_slots_reserved
                    .checked_sub(liability.prism_slots)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })
        }

        fn spend_actual_and_release(
            key: (u32, u32, u32, u32),
            liability: RewardLiability,
            actual_xp: u128,
            actual_essence: u128,
            retained_charge_slots: u64,
            retained_prism_slots: u64,
            actual_prism_slots: u64,
        ) -> DispatchResult {
            ensure!(
                actual_xp <= liability.xp
                    && actual_essence <= liability.essence
                    && retained_charge_slots <= liability.charge_slots
                    && retained_prism_slots <= liability.prism_slots
                    && actual_prism_slots
                        <= liability.prism_slots.saturating_sub(retained_prism_slots),
                Error::<T>::ArithmeticOverflow
            );
            RewardBudgets::<T>::try_mutate(key, |maybe| -> DispatchResult {
                let budget = maybe.as_mut().ok_or(Error::<T>::BudgetMissing)?;
                budget.xp_reserved = budget
                    .xp_reserved
                    .checked_sub(liability.xp)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.essence_reserved = budget
                    .essence_reserved
                    .checked_sub(liability.essence)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.xp_spent = budget
                    .xp_spent
                    .checked_add(actual_xp)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.essence_spent = budget
                    .essence_spent
                    .checked_add(actual_essence)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.charge_slots_reserved = budget
                    .charge_slots_reserved
                    .checked_sub(
                        liability
                            .charge_slots
                            .checked_sub(retained_charge_slots)
                            .ok_or(Error::<T>::ArithmeticOverflow)?,
                    )
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.prism_slots_reserved = budget
                    .prism_slots_reserved
                    .checked_sub(
                        liability
                            .prism_slots
                            .checked_sub(retained_prism_slots)
                            .ok_or(Error::<T>::ArithmeticOverflow)?,
                    )
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                budget.prism_slots_spent = budget
                    .prism_slots_spent
                    .checked_add(actual_prism_slots)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })
        }

        fn settle_daily_xp(
            session: &SessionOf<T>,
            policy: &RewardPolicy,
            actual_xp: u128,
        ) -> DispatchResult {
            if policy.practice_only
                || policy.mode_kind == GameModeKind::Legends
                || session.reward_liability.xp == 0
            {
                ensure!(actual_xp == 0, Error::<T>::ArithmeticOverflow);
                return Ok(());
            }
            ensure!(
                actual_xp <= session.reward_liability.xp,
                Error::<T>::ArithmeticOverflow
            );
            DailyXpLedgers::<T>::try_mutate(
                &session.owner,
                (session.economic_realm, session.reward_day),
                |ledger| -> DispatchResult {
                    ledger.reserved = ledger
                        .reserved
                        .checked_sub(session.reward_liability.xp)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    ledger.awarded = ledger
                        .awarded
                        .checked_add(actual_xp)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    ensure!(
                        ledger.awarded <= policy.maximum_xp_per_day,
                        Error::<T>::AntiFarmLimitReached
                    );
                    Ok(())
                },
            )
        }

        fn settle_entity_encounter_reservations(
            session: &SessionOf<T>,
            policy: &RewardPolicy,
            rewarded: bool,
            result_id: Hash32,
        ) -> DispatchResult {
            if !Self::policy_has_entity_encounter_rewards(policy) {
                return Ok(());
            }
            let encounter_id = session.encounter_id.ok_or(Error::<T>::InvalidEncounter)?;
            for entity in &session.entities {
                EntityEncounterLedgers::<T>::try_mutate(
                    (session.owner.clone(), entity.asset_id),
                    (
                        session.economic_realm,
                        session.game_id,
                        encounter_id,
                        session.reward_day,
                    ),
                    |ledger| -> DispatchResult {
                        ledger.reserved = ledger
                            .reserved
                            .checked_sub(1)
                            .ok_or(Error::<T>::ArithmeticOverflow)?;
                        if rewarded {
                            ledger.rewarded = ledger
                                .rewarded
                                .checked_add(1)
                                .ok_or(Error::<T>::ArithmeticOverflow)?;
                        }
                        Ok(())
                    },
                )?;
            }
            if rewarded && policy.first_clear_markers_required {
                let marker_key = (session.economic_realm, session.game_id, encounter_id);
                if !FirstClearMarkers::<T>::contains_key(&session.owner, marker_key) {
                    FirstClearMarkers::<T>::insert(&session.owner, marker_key, result_id);
                    Self::deposit_event(Event::FirstEncounterClearRecorded {
                        owner: session.owner.clone(),
                        game_id: session.game_id,
                        encounter_id,
                        result_id,
                    });
                }
            }
            Ok(())
        }

        fn release_anti_farm_reservations(
            session: &SessionOf<T>,
            policy: &RewardPolicy,
        ) -> DispatchResult {
            if !policy.practice_only
                && policy.mode_kind != GameModeKind::Legends
                && session.reward_liability.xp > 0
            {
                DailyXpLedgers::<T>::try_mutate(
                    &session.owner,
                    (session.economic_realm, session.reward_day),
                    |ledger| -> DispatchResult {
                        ledger.reserved = ledger
                            .reserved
                            .checked_sub(session.reward_liability.xp)
                            .ok_or(Error::<T>::ArithmeticOverflow)?;
                        Ok(())
                    },
                )?;
            }
            Self::settle_entity_encounter_reservations(session, policy, false, [0; 32])
        }

        fn release_pending_drop_liability(owner: &T::AccountId) -> DispatchResult {
            PendingDropLiabilityCount::<T>::try_mutate(owner, |count| -> DispatchResult {
                *count = count.checked_sub(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })
        }

        fn validate_header(
            session: &SessionOf<T>,
            header: &ResultHeaderV1<BlockNumberFor<T>>,
        ) -> DispatchResult {
            ensure!(
                header.protocol_version == 1
                    && header.genesis_hash == T::GenesisHashProvider::genesis_hash()
                    && header.game_id == session.game_id
                    && header.game_version == session.game_version
                    && header.mode_id == session.mode_id
                    && header.policy_version == session.policy_version
                    && header.session_id == session.session_id
                    && header.authority_epoch == session.authority_epoch
                    && header.roster_root == session.roster_root
                    && header.expires_at == session.expires_at,
                Error::<T>::HeaderMismatch
            );
            Ok(())
        }

        pub fn result_payload_hash(
            header: &ResultHeaderV1<BlockNumberFor<T>>,
            body: &ResultBodyV1<
                T::AccountId,
                BoundedVec<EntityId, T::MaxSessionEntities>,
                ChargeListOf<T>,
                BoundedVec<PrismSpellId, T::MaxSessionPrisms>,
            >,
        ) -> Hash32 {
            sp_io::hashing::blake2_256(
                &(RESULT_DOMAIN, T::PalletInstanceId::get(), header, body).encode(),
            )
        }

        fn validate_rpg_body(
            session: &SessionOf<T>,
            policy: &RewardPolicy,
            body: &RpgBattleResultV1<BoundedVec<EntityId, T::MaxSessionEntities>>,
        ) -> DispatchResult {
            ensure!(
                body.elapsed_seconds <= policy.maximum_elapsed_seconds
                    && body.turn_count > 0
                    && body.combat_metric <= policy.maximum_damage,
                Error::<T>::ResultMetricsInvalid
            );
            ensure!(
                session.encounter_id == Some(body.encounter_id) && !body.entity_ids.is_empty(),
                Error::<T>::InvalidEncounter
            );
            let session_ids: Vec<_> = session.entities.iter().map(|item| item.asset_id).collect();
            ensure!(
                body.entity_ids.as_slice() == session_ids.as_slice(),
                Error::<T>::HeaderMismatch
            );
            Ok(())
        }

        fn validate_fps_body(
            session: &SessionOf<T>,
            policy: &RewardPolicy,
            body: &FpsMatchResultV1<
                T::AccountId,
                ChargeListOf<T>,
                BoundedVec<PrismSpellId, T::MaxSessionPrisms>,
            >,
        ) -> DispatchResult {
            ensure!(
                body.account == session.owner && body.cohort_hash == session.cohort_hash,
                Error::<T>::ResultAccountMismatch
            );
            let total = body.active_seconds.saturating_add(body.afk_seconds);
            let afk_bps = if total == 0 {
                10_000
            } else {
                body.afk_seconds.saturating_mul(10_000) / total
            };
            ensure!(
                body.active_seconds >= policy.minimum_active_seconds
                    && total <= policy.maximum_elapsed_seconds
                    && afk_bps <= u32::from(policy.maximum_afk_bps)
                    && body.kills <= policy.maximum_kills
                    && body.assists <= policy.maximum_assists
                    && body.deaths <= policy.maximum_deaths
                    && body.damage <= policy.maximum_damage
                    && body.objective_score <= policy.maximum_objective_score
                    && body.outcome <= policy.maximum_outcome
                    && body.placement > 0
                    && body.placement <= policy.maximum_placement,
                Error::<T>::ResultMetricsInvalid
            );
            let used_charge_count = body
                .used_charges
                .iter()
                .map(|used| (used.definition_id, ()))
                .collect::<BTreeMap<_, _>>()
                .len();
            let used_prism_count = body
                .used_prisms
                .iter()
                .map(|spell_id| (*spell_id, ()))
                .collect::<BTreeMap<_, _>>()
                .len();
            ensure!(
                used_charge_count == body.used_charges.len()
                    && used_prism_count == body.used_prisms.len(),
                Error::<T>::DuplicateResultAssetUse
            );
            for used in &body.used_charges {
                let allowed = session
                    .charge_allowance
                    .iter()
                    .find(|charge| charge.definition_id == used.definition_id)
                    .map(|charge| charge.amount)
                    .unwrap_or_default();
                ensure!(used.amount <= allowed, Error::<T>::ChargeUseInvalid);
            }
            for spell_id in &body.used_prisms {
                ensure!(
                    session
                        .prisms
                        .iter()
                        .any(|asset| asset.asset_id == *spell_id),
                    Error::<T>::PrismUseInvalid
                );
            }
            Ok(())
        }

        fn award_deterministic_prism_quest(
            session: &SessionOf<T>,
            verified_result_id: Hash32,
        ) -> Result<bool, DispatchError> {
            let Some(quest) = session.deterministic_prism_quest else {
                return Ok(false);
            };
            ensure!(
                session.encounter_id == Some(quest.encounter_id)
                    && session.economic_realm == quest.economic_realm
                    && DeterministicPrismQuestDefinitions::<T>::get(quest.quest_hash)
                        == Some(quest.prism_definition_id),
                Error::<T>::InvalidQuestPolicy
            );
            if DeterministicPrismQuestClaims::<T>::contains_key(&session.owner, quest.quest_hash) {
                return Ok(false);
            }
            let (traits_seed, quest_result_id) = Self::deterministic_prism_quest_award_ids(
                &session.owner,
                session.session_id,
                verified_result_id,
                quest,
            );
            T::Magic::create_prism_reward(
                &session.owner,
                quest.economic_realm,
                quest.prism_definition_id,
                traits_seed,
                quest_result_id,
            )?;
            DeterministicPrismQuestClaims::<T>::insert(
                &session.owner,
                quest.quest_hash,
                verified_result_id,
            );
            Self::deposit_event(Event::DeterministicPrismQuestRewardClaimed {
                owner: session.owner.clone(),
                session_id: session.session_id,
                result_id: verified_result_id,
                quest_hash: quest.quest_hash,
                prism_definition_id: quest.prism_definition_id,
            });
            Ok(true)
        }

        fn settle_rpg(
            session: &SessionOf<T>,
            policy: &RewardPolicy,
            header: &ResultHeaderV1<BlockNumberFor<T>>,
            body: &RpgBattleResultV1<BoundedVec<EntityId, T::MaxSessionEntities>>,
        ) -> Result<u128, DispatchError> {
            let key = policy.key();
            let actual_entity_xp = if body.owner_won && !policy.practice_only {
                policy.entity_xp
            } else {
                0
            };
            for entity_id in &body.entity_ids {
                if actual_entity_xp > 0 {
                    let derived = sp_io::hashing::blake2_256(
                        &(header.result_id, b"ENTITY_XP", entity_id).encode(),
                    );
                    T::Entities::grant_experience(
                        &session.owner,
                        *entity_id,
                        actual_entity_xp,
                        derived,
                    )?;
                }
            }
            if body.owner_won && !policy.practice_only && policy.base_essence > 0 {
                T::Magic::grant_essence(
                    &session.owner,
                    session.economic_realm,
                    policy.essence_element,
                    policy.base_essence,
                    sp_io::hashing::blake2_256(&(header.result_id, b"ESSENCE").encode()),
                )?;
            }
            Self::settle_entity_encounter_reservations(
                session,
                policy,
                body.owner_won && !policy.practice_only,
                header.result_id,
            )?;
            let deterministic_prism_awarded = if body.owner_won && !policy.practice_only {
                Self::award_deterministic_prism_quest(session, header.result_id)?
            } else {
                false
            };
            let has_drop = body.owner_won && !policy.practice_only && Self::policy_may_drop(policy);
            let mut pending_drop = false;
            if has_drop {
                if let Ok(request_id) = T::Randomness::request_for(
                    session.economic_realm,
                    session.randomness_provenance,
                    DROP_DOMAIN,
                    header.result_id,
                    policy.policy_hash,
                    0,
                ) {
                    PendingDrops::<T>::insert(
                        session.session_id,
                        PendingDropResolution {
                            session_id: session.session_id,
                            owner: session.owner.clone(),
                            economic_realm: session.economic_realm,
                            result_id: header.result_id,
                            request_id,
                            policy_key: key,
                            charge_definition_id: policy.charge_definition_id,
                            charge_drop_bps: policy.charge_drop_bps,
                            prism_definition_id: policy.prism_definition_id,
                            prism_drop_bps: policy.prism_drop_bps,
                            randomness_provenance: session.randomness_provenance,
                        },
                    );
                    pending_drop = true;
                    Self::deposit_event(Event::RandomDropRequested {
                        session_id: session.session_id,
                        result_id: header.result_id,
                        request_id,
                    });
                } else {
                    Self::deposit_event(Event::RandomDropUnavailable {
                        session_id: session.session_id,
                        result_id: header.result_id,
                    });
                }
            }
            let total_xp =
                u128::from(actual_entity_xp).saturating_mul(body.entity_ids.len() as u128);
            let actual_essence = if body.owner_won && !policy.practice_only {
                u128::from(policy.base_essence)
            } else {
                0
            };
            Self::spend_actual_and_release(
                key,
                session.reward_liability,
                total_xp,
                actual_essence,
                u64::from(
                    pending_drop
                        && policy.charge_definition_id.is_some()
                        && policy.charge_drop_bps > 0,
                ),
                u64::from(
                    pending_drop
                        && policy.prism_definition_id.is_some()
                        && policy.prism_drop_bps > 0,
                ),
                u64::from(deterministic_prism_awarded),
            )?;
            if session.pending_drop_slot_reserved && !pending_drop {
                Self::release_pending_drop_liability(&session.owner)?;
            }
            Ok(total_xp)
        }

        fn settle_fps(
            session: &SessionOf<T>,
            policy: &RewardPolicy,
            header: &ResultHeaderV1<BlockNumberFor<T>>,
            body: &FpsMatchResultV1<
                T::AccountId,
                ChargeListOf<T>,
                BoundedVec<PrismSpellId, T::MaxSessionPrisms>,
            >,
        ) -> Result<u128, DispatchError> {
            let xp = if policy.practice_only {
                0
            } else {
                let participation_bps = u128::from(body.active_seconds).saturating_mul(10_000)
                    / u128::from(policy.maximum_elapsed_seconds.max(1));
                let elimination_value =
                    u128::from(body.kills).saturating_mul(2) + u128::from(body.assists);
                let elimination_max = u128::from(policy.maximum_kills).saturating_mul(2)
                    + u128::from(policy.maximum_assists);
                let elimination_bps = if elimination_max == 0 {
                    0
                } else {
                    elimination_value.saturating_mul(10_000) / elimination_max
                };
                let objective_bps = if policy.maximum_objective_score == 0 {
                    0
                } else {
                    u128::from(body.objective_score).saturating_mul(10_000)
                        / u128::from(policy.maximum_objective_score)
                };
                let weighted_bps = participation_bps
                    .min(10_000)
                    .saturating_mul(u128::from(policy.participation_weight_bps))
                    .saturating_add(
                        elimination_bps
                            .min(10_000)
                            .saturating_mul(u128::from(policy.elimination_weight_bps)),
                    )
                    .saturating_add(
                        objective_bps
                            .min(10_000)
                            .saturating_mul(u128::from(policy.objective_weight_bps)),
                    )
                    / 10_000;
                policy
                    .max_player_xp
                    .saturating_mul(weighted_bps)
                    .saturating_mul(u128::from(session.cohort_multiplier_bps))
                    / 100_000_000
            };
            ensure!(
                xp <= session.reward_liability.xp,
                Error::<T>::AntiFarmLimitReached
            );
            if xp > 0 {
                T::PlayerProgression::grant_settled_fps_xp(
                    &session.owner,
                    session.economic_realm,
                    xp,
                    header.result_id,
                )?;
            }
            let used: Vec<_> = body
                .used_charges
                .iter()
                .map(|charge| (charge.definition_id, charge.amount))
                .collect();
            if !session.charge_allowance.is_empty() {
                if policy.practice_only {
                    T::Magic::release_charges(session.session_id)?;
                } else {
                    T::Magic::settle_charges(session.session_id, used.as_slice())?;
                }
            }
            if !policy.practice_only {
                for spell_id in &body.used_prisms {
                    T::Magic::grant_prism_xp(
                        &session.owner,
                        *spell_id,
                        100,
                        sp_io::hashing::blake2_256(
                            &(header.result_id, b"PRISM_XP", spell_id).encode(),
                        ),
                    )?;
                }
            }
            Self::settle_daily_xp(session, policy, xp)?;
            Self::spend_actual_and_release(policy.key(), session.reward_liability, xp, 0, 0, 0, 0)?;
            Ok(xp)
        }

        fn release_asset_locks(
            session: &SessionOf<T>,
            body: &ResultBodyV1<
                T::AccountId,
                BoundedVec<EntityId, T::MaxSessionEntities>,
                ChargeListOf<T>,
                BoundedVec<PrismSpellId, T::MaxSessionPrisms>,
            >,
        ) -> DispatchResult {
            for asset in &session.entities {
                T::Entities::unlock_entity(session.session_id, asset.asset_id)?;
            }
            for asset in &session.prisms {
                T::Magic::unlock_prism(session.session_id, asset.asset_id)?;
            }
            if matches!(body, ResultBodyV1::RpgBattle(_)) && !session.charge_allowance.is_empty() {
                T::Magic::release_charges(session.session_id)?;
            }
            Ok(())
        }

        fn release_all_assets_force(session: &SessionOf<T>) -> DispatchResult {
            for asset in &session.entities {
                T::Entities::force_unlock_entity(session.session_id, asset.asset_id)?;
            }
            for asset in &session.prisms {
                T::Magic::force_unlock_prism(session.session_id, asset.asset_id)?;
            }
            if !session.charge_allowance.is_empty() {
                T::Magic::release_charges(session.session_id)?;
            }
            Ok(())
        }

        /// Domain-separate the public beacon output per result before drawing.
        /// Rejection sampling avoids the modulo bias of a 16-bit roll. The
        /// rejection probability for a 64-bit word and a 10,000-wide range is
        /// negligible; exhausting four independent draws fails closed.
        fn drop_roll_bps(
            pending: &PendingDropResolution<T::AccountId>,
            output: Hash32,
        ) -> Option<u16> {
            const RANGE: u64 = 10_000;
            const ACCEPT_BELOW: u64 = u64::MAX - (u64::MAX % RANGE);
            for draw_index in 0u8..4 {
                let derived = sp_io::hashing::blake2_256(
                    &(
                        DROP_DOMAIN,
                        pending.result_id,
                        pending.session_id,
                        pending.policy_key,
                        output,
                        draw_index,
                    )
                        .encode(),
                );
                let value = u64::from_le_bytes(
                    derived[..8]
                        .try_into()
                        .expect("an eight-byte hash prefix always converts"),
                );
                if value < ACCEPT_BELOW {
                    return Some((value % RANGE) as u16);
                }
            }
            None
        }

        fn resolve_drop(
            pending: PendingDropResolution<T::AccountId>,
            output: Hash32,
            timed_out: bool,
        ) -> DispatchResult {
            let mut charge_awarded = false;
            let mut prism_awarded = false;
            if !timed_out {
                if let Some(roll) = Self::drop_roll_bps(&pending, output) {
                    if roll < pending.prism_drop_bps {
                        if let Some(definition_id) = pending.prism_definition_id {
                            let traits_seed = sp_io::hashing::blake2_256(
                                &(
                                    DROP_DOMAIN,
                                    pending.result_id,
                                    pending.session_id,
                                    pending.policy_key,
                                    output,
                                    b"PRISM_TRAITS_V1",
                                )
                                    .encode(),
                            );
                            T::Magic::create_prism_reward(
                                &pending.owner,
                                pending.economic_realm,
                                definition_id,
                                traits_seed,
                                sp_io::hashing::blake2_256(
                                    &(pending.result_id, b"PRISM_DROP").encode(),
                                ),
                            )?;
                            prism_awarded = true;
                        }
                    } else if roll
                        < pending
                            .prism_drop_bps
                            .saturating_add(pending.charge_drop_bps)
                    {
                        if let Some(definition_id) = pending.charge_definition_id {
                            T::Magic::grant_spell_charges(
                                &pending.owner,
                                pending.economic_realm,
                                definition_id,
                                1,
                                sp_io::hashing::blake2_256(
                                    &(pending.result_id, b"CHARGE_DROP").encode(),
                                ),
                            )?;
                            charge_awarded = true;
                        }
                    }
                }
            }
            RewardBudgets::<T>::try_mutate(pending.policy_key, |maybe| -> DispatchResult {
                let budget = maybe.as_mut().ok_or(Error::<T>::BudgetMissing)?;
                if pending.charge_definition_id.is_some() && pending.charge_drop_bps > 0 {
                    budget.charge_slots_reserved = budget
                        .charge_slots_reserved
                        .checked_sub(1)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    if charge_awarded {
                        budget.charge_slots_spent = budget.charge_slots_spent.saturating_add(1);
                    }
                }
                if pending.prism_definition_id.is_some() && pending.prism_drop_bps > 0 {
                    budget.prism_slots_reserved = budget
                        .prism_slots_reserved
                        .checked_sub(1)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    if prism_awarded {
                        budget.prism_slots_spent = budget.prism_slots_spent.saturating_add(1);
                    }
                }
                Ok(())
            })?;
            PendingDrops::<T>::remove(pending.session_id);
            Self::release_pending_drop_liability(&pending.owner)?;
            Sessions::<T>::try_mutate(pending.session_id, |maybe| -> DispatchResult {
                let session = maybe.as_mut().ok_or(Error::<T>::SessionMissing)?;
                ensure!(
                    session.status == SessionStatus::SettledPendingDrop,
                    Error::<T>::SessionNotActive
                );
                session.status = SessionStatus::Settled;
                session.pending_drop_slot_reserved = false;
                Ok(())
            })?;
            Self::mark_terminal(pending.session_id, pending.result_id)?;
            if timed_out {
                Self::deposit_event(Event::RandomDropTimedOut {
                    session_id: pending.session_id,
                    result_id: pending.result_id,
                });
            } else {
                Self::deposit_event(Event::RandomDropFinalized {
                    session_id: pending.session_id,
                    result_id: pending.result_id,
                    charge_awarded,
                    prism_awarded,
                });
            }
            Ok(())
        }

        fn mark_terminal(session_id: SessionId, terminal_hash: Hash32) -> DispatchResult {
            let epoch = Self::session_epoch(session_id);
            EpochTerminalCount::<T>::mutate(epoch, |count| *count = count.saturating_add(1));
            let previous = EpochTerminalAccumulator::<T>::get(epoch);
            let root = sp_io::hashing::blake2_256(&(previous, session_id, terminal_hash).encode());
            EpochTerminalAccumulator::<T>::insert(epoch, root);
            EpochLastTerminalAt::<T>::insert(epoch, frame_system::Pallet::<T>::block_number());
            Ok(())
        }
    }
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
