#![cfg_attr(not(feature = "std"), no_std)]
// FRAME's generated hook glue currently triggers this lint in macro expansion.
#![allow(clippy::manual_inspect)]

pub use pallet::*;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use codec::{Decode, Encode, MaxEncodedLen};
use eterra_nexus_primitives::{
    AssetLock, CardIdV2, CardRarity, EconomicRealm, EntityId, EntityInstance, EntityLeagueFormat,
    EntityOrigin, EntityProfile, Genes, Hash32, MoveDefinition, MoveId, SubjectId,
};
use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
use scale_info::TypeInfo;
use sp_runtime::{DispatchError, RuntimeDebug};

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ConversionEntityInput<AccountId> {
    pub entity_id: EntityId,
    pub owner: AccountId,
    pub economic_realm: EconomicRealm,
    pub source_card_id: CardIdV2,
    pub source_rarity: CardRarity,
    pub subject_id: SubjectId,
    pub subject_version: u32,
    pub genome_seed: Hash32,
    pub stasis_genome: bool,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct CpLevelCurve {
    pub version: u16,
    pub ratios_bps: [u16; 50],
    pub curve_hash: Hash32,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct LeagueMovePolicy<Moves> {
    pub allowed_moves: Moves,
    /// Per-bit limits for the immutable `MoveDefinition.tags` bitset.
    /// A zero limit forbids that tag in this format.
    pub per_tag_limits: [u8; 32],
    /// Must equal the owning `EntityLeagueFormat.rules_hash`.
    pub rules_hash: Hash32,
}

impl CpLevelCurve {
    pub fn validate(&self) -> bool {
        self.ratios_bps[0] > 0
            && self.ratios_bps[49] == 10_000
            && self
                .ratios_bps
                .windows(2)
                .all(|pair| pair[0] <= pair[1] && pair[1] <= 10_000)
    }
}

pub trait EssenceManager<AccountId> {
    fn consume(
        owner: &AccountId,
        realm: EconomicRealm,
        element_id: u8,
        amount: u32,
    ) -> DispatchResult;
}

impl<AccountId> EssenceManager<AccountId> for () {
    fn consume(
        _owner: &AccountId,
        _realm: EconomicRealm,
        _element_id: u8,
        _amount: u32,
    ) -> DispatchResult {
        Err(DispatchError::Other("essence provider unavailable"))
    }
}

pub trait EntityManager<AccountId, BlockNumber> {
    fn reserve_entity_id() -> Result<EntityId, DispatchError>;
    /// Validate mutable profile activation and immutable subject/version
    /// compatibility before the source card becomes non-cancellable.
    fn ensure_conversion_profile_active(
        subject_id: SubjectId,
        subject_version: u32,
        rarity: CardRarity,
    ) -> DispatchResult;
    /// Create from an already committed conversion. Implementations must retain
    /// immutable profile/version validation but must not reapply activation.
    fn create_from_conversion(input: ConversionEntityInput<AccountId>) -> DispatchResult;
    /// Validate the complete persistent entity side of a session loadout
    /// before any lock or reward reservation is committed.
    fn validate_session_entity(
        owner: &AccountId,
        economic_realm: EconomicRealm,
        entity_id: EntityId,
        revision: u32,
        format_id: u32,
        format_version: u32,
        allowed_roles_mask: u8,
    ) -> DispatchResult;
    fn lock_entity(
        owner: &AccountId,
        entity_id: EntityId,
        lock: AssetLock<BlockNumber>,
    ) -> DispatchResult;
    fn unlock_entity(session_id: u64, entity_id: EntityId) -> DispatchResult;
    /// Recovery-only unlock used when a session is expired or governance-aborted.
    ///
    /// Missing locks are accepted so a prior audited emergency unlock cannot
    /// strand the owning session's reward liability forever. A lock belonging
    /// to a different session still fails closed.
    fn force_unlock_entity(session_id: u64, entity_id: EntityId) -> DispatchResult;
    fn grant_experience(
        owner: &AccountId,
        entity_id: EntityId,
        amount: u64,
        result_id: Hash32,
    ) -> DispatchResult;
}

impl<AccountId, BlockNumber> EntityManager<AccountId, BlockNumber> for () {
    fn reserve_entity_id() -> Result<EntityId, DispatchError> {
        Err(DispatchError::Other("entity provider unavailable"))
    }
    fn ensure_conversion_profile_active(
        _subject_id: SubjectId,
        _subject_version: u32,
        _rarity: CardRarity,
    ) -> DispatchResult {
        Err(DispatchError::Other("entity provider unavailable"))
    }
    fn create_from_conversion(_input: ConversionEntityInput<AccountId>) -> DispatchResult {
        Err(DispatchError::Other("entity provider unavailable"))
    }
    fn validate_session_entity(
        _owner: &AccountId,
        _economic_realm: EconomicRealm,
        _entity_id: EntityId,
        _revision: u32,
        _format_id: u32,
        _format_version: u32,
        _allowed_roles_mask: u8,
    ) -> DispatchResult {
        Err(DispatchError::Other("entity provider unavailable"))
    }
    fn lock_entity(
        _owner: &AccountId,
        _entity_id: EntityId,
        _lock: AssetLock<BlockNumber>,
    ) -> DispatchResult {
        Err(DispatchError::Other("entity provider unavailable"))
    }
    fn unlock_entity(_session_id: u64, _entity_id: EntityId) -> DispatchResult {
        Err(DispatchError::Other("entity provider unavailable"))
    }
    fn force_unlock_entity(_session_id: u64, _entity_id: EntityId) -> DispatchResult {
        Err(DispatchError::Other("entity provider unavailable"))
    }
    fn grant_experience(
        _owner: &AccountId,
        _entity_id: EntityId,
        _amount: u64,
        _result_id: Hash32,
    ) -> DispatchResult {
        Err(DispatchError::Other("entity provider unavailable"))
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use eterra_nexus_primitives::{
        calculate_current_cp, calculate_max_cp, AssetRole, EntityReforgeConfig, MAX_ENTITY_LEVEL,
    };
    use frame_support::transactional;
    use frame_system::pallet_prelude::*;
    use pallet_alpha_access::AccessControl;
    use sp_std::collections::btree_set::BTreeSet;
    use sp_std::vec::Vec;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);
    const GENOME_DOMAIN: &[u8] = b"ETERRA_ENTITY_GENOME_V1";

    type LearnedMovesOf<T> = BoundedVec<MoveId, <T as Config>::MaxLearnedMoves>;
    type EquippedMovesOf<T> = BoundedVec<MoveId, <T as Config>::MaxEquippedMoves>;
    type EntityOf<T> = EntityInstance<
        <T as frame_system::Config>::AccountId,
        BlockNumberFor<T>,
        LearnedMovesOf<T>,
        EquippedMovesOf<T>,
    >;
    type LearnsetOf<T> = BoundedVec<MoveId, <T as Config>::MaxProfileMoves>;
    type LeagueMovePolicyOf<T> =
        LeagueMovePolicy<BoundedVec<MoveId, <T as Config>::MaxLeagueAllowedMoves>>;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        type ResultOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        type AccessControl: pallet_alpha_access::AccessControl<Self::AccountId>;
        type Essence: EssenceManager<Self::AccountId>;
        #[pallet::constant]
        type MaxLearnedMoves: Get<u32>;
        #[pallet::constant]
        type MaxEquippedMoves: Get<u32>;
        #[pallet::constant]
        type MaxProfileMoves: Get<u32>;
        #[pallet::constant]
        type MaxLeagueAllowedMoves: Get<u32>;
        #[pallet::constant]
        type MaxExperienceGrant: Get<u64>;
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn next_entity_id)]
    pub type NextEntityId<T> = StorageValue<_, EntityId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn entity)]
    pub type Entities<T: Config> =
        StorageMap<_, Blake2_128Concat, EntityId, EntityOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn owner_entity_count)]
    pub type OwnerEntityCount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn entity_profile)]
    pub type EntityProfiles<T> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        SubjectId,
        Blake2_128Concat,
        CardRarity,
        EntityProfile,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn entity_profile_key)]
    pub type EntityProfileKeys<T> =
        StorageMap<_, Blake2_128Concat, u32, (SubjectId, CardRarity), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn entity_profile_active)]
    pub type EntityProfileActivation<T> = StorageMap<_, Blake2_128Concat, u32, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn move_definition)]
    pub type MoveDefinitions<T> =
        StorageMap<_, Blake2_128Concat, MoveId, MoveDefinition, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn profile_learnset)]
    pub type ProfileLearnsets<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, LearnsetOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn cp_level_curve)]
    pub type CpLevelCurves<T> = StorageMap<_, Blake2_128Concat, u16, CpLevelCurve, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn league_format)]
    pub type LeagueFormats<T> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u32,
        Blake2_128Concat,
        u32,
        EntityLeagueFormat,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn league_move_policy)]
    pub type LeagueMovePolicies<T: Config> =
        StorageMap<_, Blake2_128Concat, (u32, u32), LeagueMovePolicyOf<T>, OptionQuery>;

    /// Schema reservation only. No player-callable reforge exists.
    #[pallet::storage]
    #[pallet::getter(fn reserved_reforge_config)]
    pub type ReservedReforgeConfigs<T> =
        StorageMap<_, Blake2_128Concat, u32, EntityReforgeConfig, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn processed_entity_result)]
    pub type ProcessedEntityResults<T> = StorageMap<_, Blake2_128Concat, Hash32, (), OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        CpLevelCurvePublished {
            version: u16,
            curve_hash: Hash32,
        },
        MoveDefinitionPublished {
            move_id: MoveId,
            rules_hash: Hash32,
        },
        EntityProfilePublished {
            profile_id: u32,
            subject_id: SubjectId,
            rarity: CardRarity,
            definition_hash: Hash32,
        },
        EntityProfileActivationChanged {
            profile_id: u32,
            active: bool,
        },
        EntityLeagueFormatPublished {
            format_id: u32,
            version: u32,
            rules_hash: Hash32,
        },
        LeagueMovePolicyPublished {
            format_id: u32,
            version: u32,
            allowed_move_count: u32,
            rules_hash: Hash32,
        },
        EntityCreated {
            owner: T::AccountId,
            entity_id: EntityId,
            subject_id: SubjectId,
            rarity: CardRarity,
            genome_hash: Hash32,
            stasis_genome: bool,
        },
        EntityExperienceGranted {
            owner: T::AccountId,
            entity_id: EntityId,
            amount: u64,
            result_id: Hash32,
        },
        EntityLeveled {
            entity_id: EntityId,
            old_level: u8,
            new_level: u8,
            current_cp: u32,
        },
        EntityMoveLearned {
            owner: T::AccountId,
            entity_id: EntityId,
            move_id: MoveId,
        },
        EntityMoveLoadoutChanged {
            owner: T::AccountId,
            entity_id: EntityId,
            moves: Vec<MoveId>,
            revision: u32,
        },
        EntityLocked {
            entity_id: EntityId,
            session_id: u64,
        },
        EntityUnlocked {
            entity_id: EntityId,
            session_id: u64,
            emergency: bool,
        },
        FutureReforgeSchemaReserved {
            version: u32,
            pool_hash: Hash32,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        EntityIdExhausted,
        EntityAlreadyExists,
        EntityNotFound,
        NotEntityOwner,
        EntityLocked,
        EntityNotLocked,
        WrongSessionLock,
        ProfileAlreadyPublished,
        ProfileMissing,
        ProfileInactive,
        ProfileInvalid,
        MoveAlreadyPublished,
        MoveMissing,
        InvalidMoveDefinition,
        LearnsetAlreadyPublished,
        InvalidLearnset,
        MoveNotCompatible,
        MoveLevelLocked,
        MoveAlreadyLearned,
        LearnedMoveLimit,
        EquippedMoveLimit,
        DuplicateEquippedMove,
        MoveNotLearned,
        CurveAlreadyPublished,
        CurveMissing,
        InvalidCurve,
        LeagueAlreadyPublished,
        LeagueMovePolicyAlreadyPublished,
        LeagueMovePolicyMissing,
        InvalidLeague,
        LeagueCpViolation,
        LeagueMoveLoadViolation,
        EntityRealmMismatch,
        EntityRoleViolation,
        ExperienceGrantTooLarge,
        ResultAlreadyProcessed,
        ArithmeticOverflow,
        ReservedReforgeMustRemainDisabled,
        ReservedReforgeAlreadyPublished,
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
        #[pallet::weight(T::WeightInfo::publish_definition(50))]
        pub fn publish_cp_level_curve(origin: OriginFor<T>, curve: CpLevelCurve) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                curve.validate() && curve.curve_hash.iter().any(|byte| *byte != 0),
                Error::<T>::InvalidCurve
            );
            ensure!(
                !CpLevelCurves::<T>::contains_key(curve.version),
                Error::<T>::CurveAlreadyPublished
            );
            CpLevelCurves::<T>::insert(curve.version, curve);
            Self::deposit_event(Event::CpLevelCurvePublished {
                version: curve.version,
                curve_hash: curve.curve_hash,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::publish_definition(1))]
        pub fn publish_move(origin: OriginFor<T>, definition: MoveDefinition) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !MoveDefinitions::<T>::contains_key(definition.move_id),
                Error::<T>::MoveAlreadyPublished
            );
            ensure!(
                definition.unlock_level >= 1
                    && definition.unlock_level <= MAX_ENTITY_LEVEL
                    && definition.competitive_load > 0
                    && definition.rules_hash.iter().any(|byte| *byte != 0),
                Error::<T>::InvalidMoveDefinition
            );
            MoveDefinitions::<T>::insert(definition.move_id, definition);
            Self::deposit_event(Event::MoveDefinitionPublished {
                move_id: definition.move_id,
                rules_hash: definition.rules_hash,
            });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::publish_definition(learnset.len() as u32))]
        pub fn publish_entity_profile(
            origin: OriginFor<T>,
            profile: EntityProfile,
            learnset: Vec<MoveId>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !EntityProfiles::<T>::contains_key(profile.subject_id, profile.rarity),
                Error::<T>::ProfileAlreadyPublished
            );
            ensure!(
                !ProfileLearnsets::<T>::contains_key(profile.profile_id),
                Error::<T>::LearnsetAlreadyPublished
            );
            ensure!(
                profile.base_max_cp > 0
                    && profile.genetic_cp_span > 0
                    && profile.starter_moves[0] != profile.starter_moves[1]
                    && CpLevelCurves::<T>::contains_key(profile.formula_version)
                    && profile.definition_hash.iter().any(|byte| *byte != 0),
                Error::<T>::ProfileInvalid
            );
            let mut seen = BTreeSet::new();
            for move_id in &learnset {
                ensure!(seen.insert(*move_id), Error::<T>::InvalidLearnset);
                ensure!(
                    MoveDefinitions::<T>::contains_key(move_id),
                    Error::<T>::MoveMissing
                );
            }
            ensure!(
                seen.contains(&profile.starter_moves[0])
                    && seen.contains(&profile.starter_moves[1]),
                Error::<T>::InvalidLearnset
            );
            let bounded: LearnsetOf<T> = learnset
                .try_into()
                .map_err(|_| Error::<T>::InvalidLearnset)?;
            EntityProfiles::<T>::insert(profile.subject_id, profile.rarity, profile);
            EntityProfileKeys::<T>::insert(
                profile.profile_id,
                (profile.subject_id, profile.rarity),
            );
            ProfileLearnsets::<T>::insert(profile.profile_id, bounded);
            Self::deposit_event(Event::EntityProfilePublished {
                profile_id: profile.profile_id,
                subject_id: profile.subject_id,
                rarity: profile.rarity,
                definition_hash: profile.definition_hash,
            });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::set_activation())]
        pub fn set_profile_activation(
            origin: OriginFor<T>,
            profile_id: u32,
            active: bool,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                EntityProfileKeys::<T>::contains_key(profile_id),
                Error::<T>::ProfileMissing
            );
            EntityProfileActivation::<T>::insert(profile_id, active);
            Self::deposit_event(Event::EntityProfileActivationChanged { profile_id, active });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::publish_definition(1))]
        pub fn publish_league_format(
            origin: OriginFor<T>,
            format: EntityLeagueFormat,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !LeagueFormats::<T>::contains_key(format.format_id, format.version),
                Error::<T>::LeagueAlreadyPublished
            );
            ensure!(
                format.min_max_cp <= format.max_max_cp
                    && format.current_cp_cap <= format.max_max_cp
                    && format.max_move_load > 0
                    && format.rules_hash.iter().any(|byte| *byte != 0),
                Error::<T>::InvalidLeague
            );
            LeagueFormats::<T>::insert(format.format_id, format.version, format);
            Self::deposit_event(Event::EntityLeagueFormatPublished {
                format_id: format.format_id,
                version: format.version,
                rules_hash: format.rules_hash,
            });
            Ok(())
        }

        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::publish_league_move_policy(allowed_moves.len() as u32))]
        pub fn publish_league_move_policy(
            origin: OriginFor<T>,
            format_id: u32,
            version: u32,
            allowed_moves: Vec<MoveId>,
            per_tag_limits: [u8; 32],
            rules_hash: Hash32,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !LeagueMovePolicies::<T>::contains_key((format_id, version)),
                Error::<T>::LeagueMovePolicyAlreadyPublished
            );
            let format =
                LeagueFormats::<T>::get(format_id, version).ok_or(Error::<T>::InvalidLeague)?;
            ensure!(
                rules_hash == format.rules_hash
                    && rules_hash.iter().any(|byte| *byte != 0)
                    && (format.normalized
                        || (!allowed_moves.is_empty()
                            && per_tag_limits.iter().any(|limit| *limit > 0))),
                Error::<T>::InvalidLeague
            );
            let mut seen = BTreeSet::new();
            for move_id in &allowed_moves {
                ensure!(seen.insert(*move_id), Error::<T>::InvalidLeague);
                ensure!(
                    MoveDefinitions::<T>::contains_key(move_id),
                    Error::<T>::MoveMissing
                );
            }
            let allowed_move_count = allowed_moves.len() as u32;
            let bounded = allowed_moves
                .try_into()
                .map_err(|_| Error::<T>::InvalidLeague)?;
            LeagueMovePolicies::<T>::insert(
                (format_id, version),
                LeagueMovePolicy {
                    allowed_moves: bounded,
                    per_tag_limits,
                    rules_hash,
                },
            );
            Self::deposit_event(Event::LeagueMovePolicyPublished {
                format_id,
                version,
                allowed_move_count,
                rules_hash,
            });
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::learn_move())]
        #[transactional]
        pub fn learn_move(
            origin: OriginFor<T>,
            entity_id: EntityId,
            move_id: MoveId,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&owner)?;
            Entities::<T>::try_mutate(entity_id, |maybe| -> DispatchResult {
                let entity = maybe.as_mut().ok_or(Error::<T>::EntityNotFound)?;
                ensure!(entity.owner == owner, Error::<T>::NotEntityOwner);
                ensure!(entity.lock.is_none(), Error::<T>::EntityLocked);
                ensure!(
                    !entity.learned_moves.contains(&move_id),
                    Error::<T>::MoveAlreadyLearned
                );
                let profile =
                    EntityProfiles::<T>::get(entity.subject_id, Self::origin_rarity(entity))
                        .ok_or(Error::<T>::ProfileMissing)?;
                let learnset = ProfileLearnsets::<T>::get(profile.profile_id)
                    .ok_or(Error::<T>::InvalidLearnset)?;
                ensure!(learnset.contains(&move_id), Error::<T>::MoveNotCompatible);
                let definition =
                    MoveDefinitions::<T>::get(move_id).ok_or(Error::<T>::MoveMissing)?;
                ensure!(
                    entity.level >= definition.unlock_level,
                    Error::<T>::MoveLevelLocked
                );
                T::Essence::consume(
                    &owner,
                    entity.economic_realm,
                    definition.element as u8,
                    definition.essence_cost,
                )?;
                entity
                    .learned_moves
                    .try_push(move_id)
                    .map_err(|_| Error::<T>::LearnedMoveLimit)?;
                entity.revision = entity
                    .revision
                    .checked_add(1)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                Self::deposit_event(Event::EntityMoveLearned {
                    owner: owner.clone(),
                    entity_id,
                    move_id,
                });
                Ok(())
            })
        }

        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::equip_moves(moves.len() as u32))]
        pub fn equip_moves(
            origin: OriginFor<T>,
            entity_id: EntityId,
            moves: Vec<MoveId>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&owner)?;
            let bounded: EquippedMovesOf<T> = moves
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::EquippedMoveLimit)?;
            let distinct: BTreeSet<_> = moves.iter().copied().collect();
            ensure!(
                distinct.len() == moves.len(),
                Error::<T>::DuplicateEquippedMove
            );
            Entities::<T>::try_mutate(entity_id, |maybe| -> DispatchResult {
                let entity = maybe.as_mut().ok_or(Error::<T>::EntityNotFound)?;
                ensure!(entity.owner == owner, Error::<T>::NotEntityOwner);
                ensure!(entity.lock.is_none(), Error::<T>::EntityLocked);
                ensure!(
                    bounded
                        .iter()
                        .all(|move_id| entity.learned_moves.contains(move_id)),
                    Error::<T>::MoveNotLearned
                );
                entity.equipped_moves = bounded;
                entity.revision = entity
                    .revision
                    .checked_add(1)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                Self::deposit_event(Event::EntityMoveLoadoutChanged {
                    owner: owner.clone(),
                    entity_id,
                    moves,
                    revision: entity.revision,
                });
                Ok(())
            })
        }

        /// Explicitly Training-only helper for private-alpha encounter validation.
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::grant_training_experience())]
        #[transactional]
        pub fn grant_training_experience(
            origin: OriginFor<T>,
            owner: T::AccountId,
            entity_id: EntityId,
            amount: u64,
            result_id: Hash32,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                amount <= T::MaxExperienceGrant::get(),
                Error::<T>::ExperienceGrantTooLarge
            );
            let entity = Entities::<T>::get(entity_id).ok_or(Error::<T>::EntityNotFound)?;
            ensure!(
                entity.economic_realm == EconomicRealm::Training,
                Error::<T>::ProfileInvalid
            );
            Self::do_grant_experience(&owner, entity_id, amount, result_id)
        }

        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::emergency_unlock())]
        pub fn emergency_unlock(origin: OriginFor<T>, entity_id: EntityId) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Entities::<T>::try_mutate(entity_id, |maybe| -> DispatchResult {
                let entity = maybe.as_mut().ok_or(Error::<T>::EntityNotFound)?;
                let lock = entity.lock.take().ok_or(Error::<T>::EntityNotLocked)?;
                entity.revision = entity.revision.saturating_add(1);
                Self::deposit_event(Event::EntityUnlocked {
                    entity_id,
                    session_id: lock.session_id,
                    emergency: true,
                });
                Ok(())
            })
        }

        /// Schema-only reservation. `enabled=true` is deliberately impossible in V1.
        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::publish_definition(1))]
        pub fn reserve_future_reforge_schema(
            origin: OriginFor<T>,
            config: EntityReforgeConfig,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !config.enabled,
                Error::<T>::ReservedReforgeMustRemainDisabled
            );
            ensure!(
                !ReservedReforgeConfigs::<T>::contains_key(config.version),
                Error::<T>::ReservedReforgeAlreadyPublished
            );
            ReservedReforgeConfigs::<T>::insert(config.version, config);
            Self::deposit_event(Event::FutureReforgeSchemaReserved {
                version: config.version,
                pool_hash: config.pool_hash,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn origin_rarity(entity: &EntityOf<T>) -> CardRarity {
            match entity.origin {
                EntityOrigin::CardConversion { source_rarity, .. } => source_rarity,
                EntityOrigin::LegendsBond { .. } | EntityOrigin::ApprovedMigration { .. } => {
                    CardRarity::Common
                }
            }
        }

        fn genes_from_seed(seed: &Hash32, stasis: bool) -> Genes {
            if stasis {
                return Genes {
                    vitality: 15,
                    attack: 15,
                    defense: 15,
                    agility: 15,
                    focus: 15,
                    resistance: 15,
                };
            }
            Genes {
                vitality: seed[0] & 31,
                attack: seed[1] & 31,
                defense: seed[2] & 31,
                agility: seed[3] & 31,
                focus: seed[4] & 31,
                resistance: seed[5] & 31,
            }
        }

        #[transactional]
        fn do_create_from_conversion(input: ConversionEntityInput<T::AccountId>) -> DispatchResult {
            ensure!(
                !Entities::<T>::contains_key(input.entity_id),
                Error::<T>::EntityAlreadyExists
            );
            let profile = EntityProfiles::<T>::get(input.subject_id, input.source_rarity)
                .ok_or(Error::<T>::ProfileMissing)?;
            ensure!(
                profile.subject_version == input.subject_version,
                Error::<T>::ProfileInvalid
            );
            let curve =
                CpLevelCurves::<T>::get(profile.formula_version).ok_or(Error::<T>::CurveMissing)?;
            let genome_hash = sp_io::hashing::blake2_256(
                &(
                    GENOME_DOMAIN,
                    input.entity_id,
                    input.source_card_id,
                    input.subject_id,
                    input.subject_version,
                    input.genome_seed,
                    input.stasis_genome,
                )
                    .encode(),
            );
            let genes = Self::genes_from_seed(&input.genome_seed, input.stasis_genome);
            let max_cp = calculate_max_cp(profile.base_max_cp, profile.genetic_cp_span, &genes)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let current_cp = calculate_current_cp(max_cp, curve.ratios_bps[0])
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let mut learned_moves = LearnedMovesOf::<T>::default();
            learned_moves
                .try_push(profile.starter_moves[0])
                .map_err(|_| Error::<T>::LearnedMoveLimit)?;
            learned_moves
                .try_push(profile.starter_moves[1])
                .map_err(|_| Error::<T>::LearnedMoveLimit)?;
            let mut equipped_moves = EquippedMovesOf::<T>::default();
            equipped_moves
                .try_push(profile.starter_moves[0])
                .map_err(|_| Error::<T>::EquippedMoveLimit)?;
            equipped_moves
                .try_push(profile.starter_moves[1])
                .map_err(|_| Error::<T>::EquippedMoveLimit)?;
            let entity = EntityInstance {
                entity_id: input.entity_id,
                owner: input.owner.clone(),
                economic_realm: input.economic_realm,
                origin: EntityOrigin::CardConversion {
                    source_card_id: input.source_card_id,
                    source_rarity: input.source_rarity,
                },
                subject_id: input.subject_id,
                subject_version: input.subject_version,
                role: profile.role,
                genome_hash,
                genome_version: 1,
                genes,
                temperament: input.genome_seed[6],
                cosmetic_seed: sp_io::hashing::blake2_256(
                    &(b"ETERRA_ENTITY_COSMETIC_V1", genome_hash).encode(),
                ),
                stasis_genome: input.stasis_genome,
                level: 1,
                level_xp: 0,
                learned_moves,
                equipped_moves,
                current_cp,
                max_cp,
                cp_formula_version: profile.formula_version,
                revision: 1,
                lock: None,
            };
            Entities::<T>::insert(input.entity_id, entity);
            OwnerEntityCount::<T>::try_mutate(&input.owner, |count| -> DispatchResult {
                *count = count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })?;
            Self::deposit_event(Event::EntityCreated {
                owner: input.owner,
                entity_id: input.entity_id,
                subject_id: input.subject_id,
                rarity: input.source_rarity,
                genome_hash,
                stasis_genome: input.stasis_genome,
            });
            Ok(())
        }

        fn do_grant_experience(
            owner: &T::AccountId,
            entity_id: EntityId,
            amount: u64,
            result_id: Hash32,
        ) -> DispatchResult {
            ensure!(
                amount <= T::MaxExperienceGrant::get(),
                Error::<T>::ExperienceGrantTooLarge
            );
            ensure!(
                !ProcessedEntityResults::<T>::contains_key(result_id),
                Error::<T>::ResultAlreadyProcessed
            );
            Entities::<T>::try_mutate(entity_id, |maybe| -> DispatchResult {
                let entity = maybe.as_mut().ok_or(Error::<T>::EntityNotFound)?;
                ensure!(&entity.owner == owner, Error::<T>::NotEntityOwner);
                let old_level = entity.level;
                entity.level_xp = entity
                    .level_xp
                    .checked_add(amount)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                while entity.level < MAX_ENTITY_LEVEL {
                    let next_level = u64::from(entity.level) + 1;
                    let threshold = next_level
                        .checked_mul(next_level)
                        .and_then(|value| value.checked_mul(100))
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    if entity.level_xp < threshold {
                        break;
                    }
                    entity.level = entity.level.saturating_add(1);
                }
                let curve = CpLevelCurves::<T>::get(entity.cp_formula_version)
                    .ok_or(Error::<T>::CurveMissing)?;
                entity.current_cp = calculate_current_cp(
                    entity.max_cp,
                    curve.ratios_bps[usize::from(entity.level - 1)],
                )
                .ok_or(Error::<T>::ArithmeticOverflow)?;
                entity.revision = entity
                    .revision
                    .checked_add(1)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                if entity.level != old_level {
                    Self::deposit_event(Event::EntityLeveled {
                        entity_id,
                        old_level,
                        new_level: entity.level,
                        current_cp: entity.current_cp,
                    });
                }
                Ok(())
            })?;
            ProcessedEntityResults::<T>::insert(result_id, ());
            Self::deposit_event(Event::EntityExperienceGranted {
                owner: owner.clone(),
                entity_id,
                amount,
                result_id,
            });
            Ok(())
        }

        pub fn validate_for_league(
            owner: &T::AccountId,
            entity_id: EntityId,
            format_id: u32,
            version: u32,
        ) -> DispatchResult {
            let entity = Entities::<T>::get(entity_id).ok_or(Error::<T>::EntityNotFound)?;
            ensure!(&entity.owner == owner, Error::<T>::NotEntityOwner);
            let format =
                LeagueFormats::<T>::get(format_id, version).ok_or(Error::<T>::InvalidLeague)?;
            let move_policy = LeagueMovePolicies::<T>::get((format_id, version))
                .ok_or(Error::<T>::LeagueMovePolicyMissing)?;
            ensure!(
                move_policy.rules_hash == format.rules_hash,
                Error::<T>::InvalidLeague
            );
            ensure!(
                entity.current_cp <= format.current_cp_cap
                    && entity.max_cp >= format.min_max_cp
                    && entity.max_cp <= format.max_max_cp,
                Error::<T>::LeagueCpViolation
            );
            if !format.normalized {
                let mut load = 0u16;
                let mut tag_counts = [0u8; 32];
                for move_id in entity.equipped_moves {
                    let definition =
                        MoveDefinitions::<T>::get(move_id).ok_or(Error::<T>::MoveMissing)?;
                    ensure!(
                        move_policy.allowed_moves.contains(&move_id),
                        Error::<T>::LeagueMoveLoadViolation
                    );
                    ensure!(
                        definition.tier <= format.maximum_ultimate_tier,
                        Error::<T>::LeagueMoveLoadViolation
                    );
                    for (tag_index, count) in tag_counts.iter_mut().enumerate() {
                        if definition.tags & (1u32 << tag_index) != 0 {
                            *count = count.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
                            ensure!(
                                *count <= move_policy.per_tag_limits[tag_index],
                                Error::<T>::LeagueMoveLoadViolation
                            );
                        }
                    }
                    load = load
                        .checked_add(definition.competitive_load)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                }
                ensure!(
                    load <= format.max_move_load,
                    Error::<T>::LeagueMoveLoadViolation
                );
            }
            Ok(())
        }
    }

    impl<T: Config> EntityManager<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
        fn reserve_entity_id() -> Result<EntityId, DispatchError> {
            let id = NextEntityId::<T>::get()
                .checked_add(1)
                .ok_or(Error::<T>::EntityIdExhausted)?;
            NextEntityId::<T>::put(id);
            Ok(id)
        }

        fn ensure_conversion_profile_active(
            subject_id: SubjectId,
            subject_version: u32,
            rarity: CardRarity,
        ) -> DispatchResult {
            let profile =
                EntityProfiles::<T>::get(subject_id, rarity).ok_or(Error::<T>::ProfileMissing)?;
            ensure!(
                profile.subject_version == subject_version,
                Error::<T>::ProfileInvalid
            );
            ensure!(
                EntityProfileActivation::<T>::get(profile.profile_id),
                Error::<T>::ProfileInactive
            );
            Ok(())
        }

        fn create_from_conversion(input: ConversionEntityInput<T::AccountId>) -> DispatchResult {
            Self::do_create_from_conversion(input)
        }

        fn validate_session_entity(
            owner: &T::AccountId,
            economic_realm: EconomicRealm,
            entity_id: EntityId,
            revision: u32,
            format_id: u32,
            format_version: u32,
            allowed_roles_mask: u8,
        ) -> DispatchResult {
            let entity = Entities::<T>::get(entity_id).ok_or(Error::<T>::EntityNotFound)?;
            ensure!(&entity.owner == owner, Error::<T>::NotEntityOwner);
            ensure!(
                entity.economic_realm == economic_realm,
                Error::<T>::EntityRealmMismatch
            );
            ensure!(entity.revision == revision, Error::<T>::ProfileInvalid);
            ensure!(entity.lock.is_none(), Error::<T>::EntityLocked);
            let role_bit = 1u8
                .checked_shl(entity.role as u32)
                .ok_or(Error::<T>::EntityRoleViolation)?;
            ensure!(
                allowed_roles_mask & role_bit != 0,
                Error::<T>::EntityRoleViolation
            );
            Self::validate_for_league(owner, entity_id, format_id, format_version)
        }

        fn lock_entity(
            owner: &T::AccountId,
            entity_id: EntityId,
            lock: AssetLock<BlockNumberFor<T>>,
        ) -> DispatchResult {
            ensure!(lock.role == AssetRole::Entity, Error::<T>::ProfileInvalid);
            Entities::<T>::try_mutate(entity_id, |maybe| -> DispatchResult {
                let entity = maybe.as_mut().ok_or(Error::<T>::EntityNotFound)?;
                ensure!(&entity.owner == owner, Error::<T>::NotEntityOwner);
                ensure!(entity.lock.is_none(), Error::<T>::EntityLocked);
                ensure!(
                    entity.revision == lock.revision_at_lock,
                    Error::<T>::ProfileInvalid
                );
                entity.lock = Some(lock);
                Self::deposit_event(Event::EntityLocked {
                    entity_id,
                    session_id: lock.session_id,
                });
                Ok(())
            })
        }

        fn unlock_entity(session_id: u64, entity_id: EntityId) -> DispatchResult {
            Entities::<T>::try_mutate(entity_id, |maybe| -> DispatchResult {
                let entity = maybe.as_mut().ok_or(Error::<T>::EntityNotFound)?;
                let lock = entity.lock.ok_or(Error::<T>::EntityNotLocked)?;
                ensure!(lock.session_id == session_id, Error::<T>::WrongSessionLock);
                entity.lock = None;
                Self::deposit_event(Event::EntityUnlocked {
                    entity_id,
                    session_id,
                    emergency: false,
                });
                Ok(())
            })
        }

        fn force_unlock_entity(session_id: u64, entity_id: EntityId) -> DispatchResult {
            Entities::<T>::try_mutate(entity_id, |maybe| -> DispatchResult {
                let entity = maybe.as_mut().ok_or(Error::<T>::EntityNotFound)?;
                if let Some(lock) = entity.lock {
                    ensure!(lock.session_id == session_id, Error::<T>::WrongSessionLock);
                    entity.lock = None;
                    Self::deposit_event(Event::EntityUnlocked {
                        entity_id,
                        session_id,
                        emergency: true,
                    });
                }
                Ok(())
            })
        }

        fn grant_experience(
            owner: &T::AccountId,
            entity_id: EntityId,
            amount: u64,
            result_id: Hash32,
        ) -> DispatchResult {
            Self::do_grant_experience(owner, entity_id, amount, result_id)
        }
    }
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
