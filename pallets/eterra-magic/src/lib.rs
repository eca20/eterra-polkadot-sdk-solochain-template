#![cfg_attr(not(feature = "std"), no_std)]
// FRAME's generated hook glue currently triggers this lint in macro expansion.
#![allow(clippy::manual_inspect)]

pub use pallet::*;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use codec::{Decode, Encode, MaxEncodedLen};
use eterra_nexus_primitives::{
    AssetLock, EconomicRealm, Element, Hash32, PrismSpell, PrismSpellId, SessionId,
};
use frame_support::{
    dispatch::DispatchResult,
    pallet_prelude::*,
    traits::{Currency, ExistenceRequirement},
};
use scale_info::TypeInfo;
use sp_runtime::{
    traits::{CheckedMul, Zero},
    DispatchError, RuntimeDebug,
};

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct SpellChargeDefinition {
    pub definition_id: u32,
    pub element: Element,
    pub competitive_load: u16,
    pub max_per_session: u16,
    pub effect_hash: Hash32,
    pub transferable: bool,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PrismSpellDefinition {
    pub definition_id: u32,
    pub element: Element,
    pub competitive_load: u16,
    pub max_level: u8,
    pub deterministic_quest_available: bool,
    pub effect_hash: Hash32,
    pub transferable: bool,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ChargeAmount {
    pub definition_id: u32,
    pub amount: u32,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ChargeCraftingRecipe<Balance> {
    pub definition_id: u32,
    /// Opaque private-alpha formula commitment. Its derivation is provisional
    /// until a replacement content contract pins it.
    pub formula_hash: Hash32,
    /// Opaque private-alpha recipe commitment. V3 authoritatively binds the
    /// full Charge definition and catalog, not this separate derivation.
    pub recipe_hash: Hash32,
    pub essence_per_charge: u32,
    /// Fee in raw `Currency::Balance` base units. Whole-token scaling and the
    /// destination policy must be pinned before Production activation.
    pub eon_coin_fee_per_charge: Balance,
    /// Provisional private-alpha output bound; one unit currently mints one
    /// Charge and consumes one unit of each configured per-Charge cost.
    pub max_batch: u16,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ChargeCraftReceipt<Balance> {
    pub request_id: Hash32,
    pub commitment: Hash32,
    pub economic_realm: EconomicRealm,
    pub definition_id: u32,
    pub amount: u32,
    pub formula_hash: Hash32,
    pub recipe_hash: Hash32,
    pub essence_consumed: u32,
    pub eon_coin_fee: Balance,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ChargeReservation<AccountId, Charges> {
    pub session_id: SessionId,
    pub owner: AccountId,
    pub economic_realm: EconomicRealm,
    pub charges: Charges,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct MagicLoadoutLimits {
    pub max_magic_load: u16,
    pub max_prisms: u8,
    pub max_charge_definitions: u8,
    pub max_total_charges: u16,
}

pub trait MagicManager<AccountId, BlockNumber> {
    /// Proves every immutable definition referenced by a reward policy exists
    /// before publication. Definitions are immutable, so committed sessions
    /// cannot later be stranded by a mutable catalog toggle.
    fn validate_reward_definitions(
        charge_definition_id: Option<u32>,
        prism_definition_id: Option<u32>,
    ) -> DispatchResult;
    /// Validate ownership, revision, realm, reservation availability, and the
    /// published competitive load budget before a game session mutates state.
    fn validate_session_loadout(
        owner: &AccountId,
        realm: EconomicRealm,
        limits: MagicLoadoutLimits,
        prisms: &[(PrismSpellId, u32)],
        charges: &[(u32, u32)],
    ) -> DispatchResult;
    fn reserve_charges(
        session_id: SessionId,
        owner: &AccountId,
        realm: EconomicRealm,
        charges: &[(u32, u32)],
    ) -> DispatchResult;
    fn settle_charges(session_id: SessionId, used: &[(u32, u32)]) -> DispatchResult;
    fn release_charges(session_id: SessionId) -> DispatchResult;
    fn grant_essence(
        owner: &AccountId,
        realm: EconomicRealm,
        element: Element,
        amount: u32,
        result_id: Hash32,
    ) -> DispatchResult;
    fn grant_spell_charges(
        owner: &AccountId,
        realm: EconomicRealm,
        definition_id: u32,
        amount: u32,
        result_id: Hash32,
    ) -> DispatchResult;
    fn grant_prism_xp(
        owner: &AccountId,
        spell_id: PrismSpellId,
        amount: u64,
        result_id: Hash32,
    ) -> DispatchResult;
    fn create_prism_reward(
        owner: &AccountId,
        realm: EconomicRealm,
        definition_id: u32,
        traits_seed: Hash32,
        result_id: Hash32,
    ) -> DispatchResult;
    fn lock_prism(
        owner: &AccountId,
        spell_id: PrismSpellId,
        lock: AssetLock<BlockNumber>,
    ) -> DispatchResult;
    fn unlock_prism(session_id: SessionId, spell_id: PrismSpellId) -> DispatchResult;
    /// Recovery-only unlock for expired or governance-aborted sessions.
    /// Missing locks are accepted; a different session lock is never cleared.
    fn force_unlock_prism(session_id: SessionId, spell_id: PrismSpellId) -> DispatchResult;
}

impl<AccountId, BlockNumber> MagicManager<AccountId, BlockNumber> for () {
    fn validate_reward_definitions(
        charge_definition_id: Option<u32>,
        prism_definition_id: Option<u32>,
    ) -> DispatchResult {
        if charge_definition_id.is_none() && prism_definition_id.is_none() {
            Ok(())
        } else {
            Err(DispatchError::Other("magic provider unavailable"))
        }
    }
    fn validate_session_loadout(
        _owner: &AccountId,
        _realm: EconomicRealm,
        limits: MagicLoadoutLimits,
        prisms: &[(PrismSpellId, u32)],
        charges: &[(u32, u32)],
    ) -> DispatchResult {
        if limits.max_magic_load == 0
            && limits.max_prisms == 0
            && limits.max_charge_definitions == 0
            && limits.max_total_charges == 0
            && prisms.is_empty()
            && charges.is_empty()
        {
            Ok(())
        } else {
            Err(DispatchError::Other("magic provider unavailable"))
        }
    }

    fn reserve_charges(
        _session_id: SessionId,
        _owner: &AccountId,
        _realm: EconomicRealm,
        _charges: &[(u32, u32)],
    ) -> DispatchResult {
        Err(DispatchError::Other("magic provider unavailable"))
    }
    fn settle_charges(_session_id: SessionId, _used: &[(u32, u32)]) -> DispatchResult {
        Err(DispatchError::Other("magic provider unavailable"))
    }
    fn release_charges(_session_id: SessionId) -> DispatchResult {
        Err(DispatchError::Other("magic provider unavailable"))
    }
    fn grant_essence(
        _owner: &AccountId,
        _realm: EconomicRealm,
        _element: Element,
        _amount: u32,
        _result_id: Hash32,
    ) -> DispatchResult {
        Err(DispatchError::Other("magic provider unavailable"))
    }
    fn grant_spell_charges(
        _owner: &AccountId,
        _realm: EconomicRealm,
        _definition_id: u32,
        _amount: u32,
        _result_id: Hash32,
    ) -> DispatchResult {
        Err(DispatchError::Other("magic provider unavailable"))
    }
    fn grant_prism_xp(
        _owner: &AccountId,
        _spell_id: PrismSpellId,
        _amount: u64,
        _result_id: Hash32,
    ) -> DispatchResult {
        Err(DispatchError::Other("magic provider unavailable"))
    }
    fn create_prism_reward(
        _owner: &AccountId,
        _realm: EconomicRealm,
        _definition_id: u32,
        _traits_seed: Hash32,
        _result_id: Hash32,
    ) -> DispatchResult {
        Err(DispatchError::Other("magic provider unavailable"))
    }
    fn lock_prism(
        _owner: &AccountId,
        _spell_id: PrismSpellId,
        _lock: AssetLock<BlockNumber>,
    ) -> DispatchResult {
        Err(DispatchError::Other("magic provider unavailable"))
    }
    fn unlock_prism(_session_id: SessionId, _spell_id: PrismSpellId) -> DispatchResult {
        Err(DispatchError::Other("magic provider unavailable"))
    }
    fn force_unlock_prism(_session_id: SessionId, _spell_id: PrismSpellId) -> DispatchResult {
        Err(DispatchError::Other("magic provider unavailable"))
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use eterra_nexus_primitives::AssetRole;
    use frame_support::transactional;
    use frame_system::pallet_prelude::*;
    use pallet_alpha_access::AccessControl;
    use sp_std::{
        collections::{btree_map::BTreeMap, btree_set::BTreeSet},
        vec::Vec,
    };

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
    type ChargeListOf<T> = BoundedVec<ChargeAmount, <T as Config>::MaxChargeDefinitionsPerSession>;
    type ChargeReservationOf<T> =
        ChargeReservation<<T as frame_system::Config>::AccountId, ChargeListOf<T>>;
    type PrismSpellOf<T> = PrismSpell<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>;
    pub type ChargeCraftingRecipeOf<T> = ChargeCraftingRecipe<BalanceOf<T>>;
    type ChargeCraftReceiptOf<T> = ChargeCraftReceipt<BalanceOf<T>>;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        type AccessControl: pallet_alpha_access::AccessControl<Self::AccountId>;
        type Currency: Currency<Self::AccountId>;
        /// Provisional private-alpha fee sink. The V3 content contract does
        /// not specify treasury-versus-burn behavior.
        #[pallet::constant]
        type CraftingFeeDestination: Get<Self::AccountId>;
        /// Compile-time private-alpha gate. Production crafting remains false
        /// until the external economic review explicitly authorizes activation.
        #[pallet::constant]
        type ProductionCraftingEnabled: Get<bool>;
        #[pallet::constant]
        type MaxChargeDefinitionsPerSession: Get<u32>;
        #[pallet::constant]
        type MaxCraftBatch: Get<u32>;
        #[pallet::constant]
        type MaxPrismXpGrant: Get<u64>;
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn essence_balance)]
    pub type EssenceBalances<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        (EconomicRealm, Element),
        u128,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn spell_charge_balance)]
    pub type SpellChargeBalances<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        (EconomicRealm, u32),
        u32,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn reserved_spell_charge_balance)]
    pub type ReservedSpellChargeBalances<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        (EconomicRealm, u32),
        u32,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn charge_reservation)]
    pub type ChargeReservations<T: Config> =
        StorageMap<_, Blake2_128Concat, SessionId, ChargeReservationOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn spell_charge_definition)]
    pub type SpellChargeDefinitions<T> =
        StorageMap<_, Blake2_128Concat, u32, SpellChargeDefinition, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prism_spell_definition)]
    pub type PrismSpellDefinitions<T> =
        StorageMap<_, Blake2_128Concat, u32, PrismSpellDefinition, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_prism_spell_id)]
    pub type NextPrismSpellId<T> = StorageValue<_, PrismSpellId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prism_spell)]
    pub type PrismSpells<T: Config> =
        StorageMap<_, Blake2_128Concat, PrismSpellId, PrismSpellOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn processed_magic_result)]
    pub type ProcessedMagicResults<T> =
        StorageMap<_, Blake2_128Concat, (Hash32, u8), (), OptionQuery>;

    /// Immutable, content-hash-pinned recipes for the 12 fungible Charges.
    #[pallet::storage]
    #[pallet::getter(fn charge_crafting_recipe)]
    pub type ChargeCraftingRecipes<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, ChargeCraftingRecipeOf<T>, OptionQuery>;

    /// Idempotency and conservation receipt keyed by player-supplied request ID.
    #[pallet::storage]
    #[pallet::getter(fn processed_charge_craft)]
    pub type ProcessedChargeCrafts<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        Hash32,
        ChargeCraftReceiptOf<T>,
        OptionQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        SpellChargeDefinitionPublished {
            definition_id: u32,
            effect_hash: Hash32,
        },
        PrismSpellDefinitionPublished {
            definition_id: u32,
            effect_hash: Hash32,
        },
        EssenceGranted {
            owner: T::AccountId,
            economic_realm: EconomicRealm,
            element: Element,
            amount: u32,
            result_id: Hash32,
        },
        EssenceConsumed {
            owner: T::AccountId,
            economic_realm: EconomicRealm,
            element: Element,
            amount: u32,
        },
        SpellChargesGranted {
            owner: T::AccountId,
            economic_realm: EconomicRealm,
            definition_id: u32,
            amount: u32,
            result_id: Hash32,
        },
        SpellChargesReserved {
            owner: T::AccountId,
            session_id: SessionId,
            economic_realm: EconomicRealm,
            charges: Vec<ChargeAmount>,
        },
        SpellChargesConsumed {
            session_id: SessionId,
            used: Vec<ChargeAmount>,
        },
        SpellChargesReleased {
            session_id: SessionId,
        },
        PrismSpellCreated {
            owner: T::AccountId,
            spell_id: PrismSpellId,
            definition_id: u32,
            economic_realm: EconomicRealm,
        },
        PrismSpellExperienceGranted {
            owner: T::AccountId,
            spell_id: PrismSpellId,
            amount: u64,
            result_id: Hash32,
        },
        PrismSpellLeveled {
            spell_id: PrismSpellId,
            old_level: u8,
            new_level: u8,
        },
        PrismSpellLocked {
            spell_id: PrismSpellId,
            session_id: SessionId,
        },
        PrismSpellUnlocked {
            spell_id: PrismSpellId,
            session_id: SessionId,
            emergency: bool,
        },
        ChargeCraftingRecipePublished {
            definition_id: u32,
            formula_hash: Hash32,
            recipe_hash: Hash32,
        },
        SpellChargesCrafted {
            owner: T::AccountId,
            economic_realm: EconomicRealm,
            definition_id: u32,
            amount: u32,
            request_id: Hash32,
            formula_hash: Hash32,
            recipe_hash: Hash32,
            essence_consumed: u32,
            eon_coin_fee: BalanceOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        DefinitionAlreadyPublished,
        DefinitionMissing,
        InvalidDefinition,
        InsufficientEssence,
        InsufficientSpellCharges,
        ReservationAlreadyExists,
        ReservationMissing,
        ReservationOwnerMismatch,
        ReservationRealmMismatch,
        TooManyChargeDefinitions,
        DuplicateChargeDefinition,
        ChargeLimitExceeded,
        MagicLoadoutViolation,
        UsedChargeExceedsReservation,
        PrismSpellIdExhausted,
        PrismSpellMissing,
        NotPrismOwner,
        PrismLocked,
        PrismNotLocked,
        WrongSessionLock,
        PrismXpGrantTooLarge,
        ResultAlreadyProcessed,
        ArithmeticOverflow,
        TransferDisabled,
        TrainingOnlyHelper,
        CraftingRecipeAlreadyPublished,
        CraftingRecipeMissing,
        ProductionCraftingDisabled,
        CraftRequestConflict,
        CraftAmountInvalid,
        CraftFormulaMismatch,
        CraftRecipeMismatch,
        CraftFeePaymentFailed,
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
        #[pallet::weight(T::WeightInfo::publish_definition())]
        pub fn publish_spell_charge_definition(
            origin: OriginFor<T>,
            definition: SpellChargeDefinition,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !SpellChargeDefinitions::<T>::contains_key(definition.definition_id),
                Error::<T>::DefinitionAlreadyPublished
            );
            ensure!(
                definition.competitive_load > 0
                    && definition.max_per_session > 0
                    && definition.effect_hash.iter().any(|byte| *byte != 0)
                    && !definition.transferable,
                Error::<T>::InvalidDefinition
            );
            SpellChargeDefinitions::<T>::insert(definition.definition_id, definition);
            Self::deposit_event(Event::SpellChargeDefinitionPublished {
                definition_id: definition.definition_id,
                effect_hash: definition.effect_hash,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::publish_definition())]
        pub fn publish_prism_definition(
            origin: OriginFor<T>,
            definition: PrismSpellDefinition,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !PrismSpellDefinitions::<T>::contains_key(definition.definition_id),
                Error::<T>::DefinitionAlreadyPublished
            );
            ensure!(
                definition.competitive_load > 0
                    && definition.max_level > 0
                    && definition.deterministic_quest_available
                    && definition.effect_hash.iter().any(|byte| *byte != 0)
                    && !definition.transferable,
                Error::<T>::InvalidDefinition
            );
            PrismSpellDefinitions::<T>::insert(definition.definition_id, definition);
            Self::deposit_event(Event::PrismSpellDefinitionPublished {
                definition_id: definition.definition_id,
                effect_hash: definition.effect_hash,
            });
            Ok(())
        }

        /// Explicitly Training-only operator helper.
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::grant())]
        pub fn grant_training_essence(
            origin: OriginFor<T>,
            owner: T::AccountId,
            element: Element,
            amount: u32,
            result_id: Hash32,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::do_grant_essence(&owner, EconomicRealm::Training, element, amount, result_id)
        }

        /// Explicitly Training-only operator helper.
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::grant())]
        pub fn grant_training_spell_charges(
            origin: OriginFor<T>,
            owner: T::AccountId,
            definition_id: u32,
            amount: u32,
            result_id: Hash32,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::do_grant_spell_charges(
                &owner,
                EconomicRealm::Training,
                definition_id,
                amount,
                result_id,
            )
        }

        /// Creates only a Training fixture. Production Prism issuance is result-policy driven.
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::grant())]
        pub fn create_training_prism(
            origin: OriginFor<T>,
            owner: T::AccountId,
            definition_id: u32,
            traits_seed: Hash32,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::do_create_prism(owner, EconomicRealm::Training, definition_id, traits_seed)?;
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::emergency_unlock())]
        pub fn emergency_unlock_prism(
            origin: OriginFor<T>,
            spell_id: PrismSpellId,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            PrismSpells::<T>::try_mutate(spell_id, |maybe| -> DispatchResult {
                let spell = maybe.as_mut().ok_or(Error::<T>::PrismSpellMissing)?;
                let lock = spell.lock.take().ok_or(Error::<T>::PrismNotLocked)?;
                spell.revision = spell.revision.saturating_add(1);
                Self::deposit_event(Event::PrismSpellUnlocked {
                    spell_id,
                    session_id: lock.session_id,
                    emergency: true,
                });
                Ok(())
            })
        }

        /// Publishes one immutable private-alpha Charge crafting recipe.
        ///
        /// These opaque commitments are additive implementation contracts:
        /// V3 pins formula identifiers and full Charge definition/catalog
        /// hashes, but does not pin separate formula/recipe hash derivations.
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::publish_crafting_recipe())]
        pub fn publish_charge_crafting_recipe(
            origin: OriginFor<T>,
            recipe: ChargeCraftingRecipeOf<T>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                SpellChargeDefinitions::<T>::contains_key(recipe.definition_id),
                Error::<T>::DefinitionMissing
            );
            ensure!(
                !ChargeCraftingRecipes::<T>::contains_key(recipe.definition_id),
                Error::<T>::CraftingRecipeAlreadyPublished
            );
            ensure!(
                recipe.formula_hash.iter().any(|byte| *byte != 0)
                    && recipe.recipe_hash.iter().any(|byte| *byte != 0)
                    && recipe.essence_per_charge > 0
                    && !recipe.eon_coin_fee_per_charge.is_zero()
                    && recipe.max_batch > 0
                    && u32::from(recipe.max_batch) <= T::MaxCraftBatch::get(),
                Error::<T>::InvalidDefinition
            );
            ChargeCraftingRecipes::<T>::insert(recipe.definition_id, recipe);
            Self::deposit_event(Event::ChargeCraftingRecipePublished {
                definition_id: recipe.definition_id,
                formula_hash: recipe.formula_hash,
                recipe_hash: recipe.recipe_hash,
            });
            Ok(())
        }

        /// Deterministically crafts fungible Charges from the immutable
        /// private-alpha recipe. The request ID makes wallet retries
        /// idempotent; conflicting reuse fails before any essence or EON Coin
        /// moves. Formula-hash equality is not formula entitlement, so the
        /// runtime must keep Production disabled until entitlement and the
        /// remaining economic semantics are pinned.
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::craft_spell_charges())]
        #[transactional]
        pub fn craft_spell_charges(
            origin: OriginFor<T>,
            economic_realm: EconomicRealm,
            definition_id: u32,
            amount: u32,
            formula_hash: Hash32,
            recipe_hash: Hash32,
            request_id: Hash32,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&owner)?;
            ensure!(
                request_id.iter().any(|byte| *byte != 0),
                Error::<T>::CraftAmountInvalid
            );
            let commitment = sp_io::hashing::blake2_256(
                &(
                    b"ETERRA_MAGIC_CHARGE_CRAFT_V1".as_slice(),
                    &owner,
                    economic_realm,
                    definition_id,
                    amount,
                    formula_hash,
                    recipe_hash,
                    request_id,
                )
                    .encode(),
            );
            if let Some(processed) = ProcessedChargeCrafts::<T>::get(&owner, request_id) {
                ensure!(
                    processed.commitment == commitment,
                    Error::<T>::CraftRequestConflict
                );
                return Ok(());
            }
            if economic_realm == EconomicRealm::Production {
                ensure!(
                    T::ProductionCraftingEnabled::get(),
                    Error::<T>::ProductionCraftingDisabled
                );
            }
            let definition = SpellChargeDefinitions::<T>::get(definition_id)
                .ok_or(Error::<T>::DefinitionMissing)?;
            let recipe = ChargeCraftingRecipes::<T>::get(definition_id)
                .ok_or(Error::<T>::CraftingRecipeMissing)?;
            ensure!(
                formula_hash == recipe.formula_hash,
                Error::<T>::CraftFormulaMismatch
            );
            ensure!(
                recipe_hash == recipe.recipe_hash,
                Error::<T>::CraftRecipeMismatch
            );
            ensure!(
                amount > 0
                    && amount <= T::MaxCraftBatch::get()
                    && amount <= u32::from(recipe.max_batch),
                Error::<T>::CraftAmountInvalid
            );
            let essence_consumed = recipe
                .essence_per_charge
                .checked_mul(amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            // `Currency::Balance` is guaranteed to be at least 32 bits, so
            // this conversion is exact rather than saturating.
            let amount_balance: BalanceOf<T> = amount.into();
            let eon_coin_fee = recipe
                .eon_coin_fee_per_charge
                .checked_mul(&amount_balance)
                .ok_or(Error::<T>::ArithmeticOverflow)?;

            EssenceBalances::<T>::try_mutate(
                &owner,
                (economic_realm, definition.element),
                |balance| -> DispatchResult {
                    *balance = balance
                        .checked_sub(u128::from(essence_consumed))
                        .ok_or(Error::<T>::InsufficientEssence)?;
                    Ok(())
                },
            )?;
            Self::deposit_event(Event::EssenceConsumed {
                owner: owner.clone(),
                economic_realm,
                element: definition.element,
                amount: essence_consumed,
            });
            T::Currency::transfer(
                &owner,
                &T::CraftingFeeDestination::get(),
                eon_coin_fee,
                ExistenceRequirement::KeepAlive,
            )
            .map_err(|_| Error::<T>::CraftFeePaymentFailed)?;
            SpellChargeBalances::<T>::try_mutate(
                &owner,
                (economic_realm, definition_id),
                |balance| -> DispatchResult {
                    *balance = balance
                        .checked_add(amount)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            ProcessedChargeCrafts::<T>::insert(
                &owner,
                request_id,
                ChargeCraftReceipt {
                    request_id,
                    commitment,
                    economic_realm,
                    definition_id,
                    amount,
                    formula_hash,
                    recipe_hash,
                    essence_consumed,
                    eon_coin_fee,
                },
            );
            Self::deposit_event(Event::SpellChargesCrafted {
                owner,
                economic_realm,
                definition_id,
                amount,
                request_id,
                formula_hash,
                recipe_hash,
                essence_consumed,
                eon_coin_fee,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn marker(result_id: Hash32, domain: u8) -> (Hash32, u8) {
            (result_id, domain)
        }

        fn do_grant_essence(
            owner: &T::AccountId,
            realm: EconomicRealm,
            element: Element,
            amount: u32,
            result_id: Hash32,
        ) -> DispatchResult {
            let marker = Self::marker(result_id, 0);
            ensure!(
                !ProcessedMagicResults::<T>::contains_key(marker),
                Error::<T>::ResultAlreadyProcessed
            );
            EssenceBalances::<T>::try_mutate(
                owner,
                (realm, element),
                |balance| -> DispatchResult {
                    *balance = balance
                        .checked_add(u128::from(amount))
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            ProcessedMagicResults::<T>::insert(marker, ());
            Self::deposit_event(Event::EssenceGranted {
                owner: owner.clone(),
                economic_realm: realm,
                element,
                amount,
                result_id,
            });
            Ok(())
        }

        fn do_grant_spell_charges(
            owner: &T::AccountId,
            realm: EconomicRealm,
            definition_id: u32,
            amount: u32,
            result_id: Hash32,
        ) -> DispatchResult {
            ensure!(
                SpellChargeDefinitions::<T>::contains_key(definition_id),
                Error::<T>::DefinitionMissing
            );
            let marker = Self::marker(result_id, 1);
            ensure!(
                !ProcessedMagicResults::<T>::contains_key(marker),
                Error::<T>::ResultAlreadyProcessed
            );
            SpellChargeBalances::<T>::try_mutate(
                owner,
                (realm, definition_id),
                |balance| -> DispatchResult {
                    *balance = balance
                        .checked_add(amount)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            ProcessedMagicResults::<T>::insert(marker, ());
            Self::deposit_event(Event::SpellChargesGranted {
                owner: owner.clone(),
                economic_realm: realm,
                definition_id,
                amount,
                result_id,
            });
            Ok(())
        }

        fn do_create_prism(
            owner: T::AccountId,
            realm: EconomicRealm,
            definition_id: u32,
            traits_seed: Hash32,
        ) -> Result<PrismSpellId, DispatchError> {
            ensure!(
                PrismSpellDefinitions::<T>::contains_key(definition_id),
                Error::<T>::DefinitionMissing
            );
            let spell_id = NextPrismSpellId::<T>::get()
                .checked_add(1)
                .ok_or(Error::<T>::PrismSpellIdExhausted)?;
            NextPrismSpellId::<T>::put(spell_id);
            PrismSpells::<T>::insert(
                spell_id,
                PrismSpell {
                    spell_id,
                    owner: owner.clone(),
                    economic_realm: realm,
                    definition_id,
                    traits_seed,
                    level: 1,
                    xp: 0,
                    revision: 1,
                    lock: None,
                },
            );
            Self::deposit_event(Event::PrismSpellCreated {
                owner,
                spell_id,
                definition_id,
                economic_realm: realm,
            });
            Ok(spell_id)
        }

        #[transactional]
        fn do_reserve_charges(
            session_id: SessionId,
            owner: &T::AccountId,
            realm: EconomicRealm,
            charges: &[(u32, u32)],
        ) -> DispatchResult {
            ensure!(
                !ChargeReservations::<T>::contains_key(session_id),
                Error::<T>::ReservationAlreadyExists
            );
            let mut unique = BTreeMap::<u32, u32>::new();
            for (definition_id, amount) in charges {
                let definition = SpellChargeDefinitions::<T>::get(definition_id)
                    .ok_or(Error::<T>::DefinitionMissing)?;
                ensure!(*amount > 0, Error::<T>::InvalidDefinition);
                ensure!(
                    *amount <= u32::from(definition.max_per_session),
                    Error::<T>::ChargeLimitExceeded
                );
                ensure!(
                    unique.insert(*definition_id, *amount).is_none(),
                    Error::<T>::DuplicateChargeDefinition
                );
                let total = SpellChargeBalances::<T>::get(owner, (realm, *definition_id));
                let reserved =
                    ReservedSpellChargeBalances::<T>::get(owner, (realm, *definition_id));
                ensure!(
                    total.saturating_sub(reserved) >= *amount,
                    Error::<T>::InsufficientSpellCharges
                );
            }
            let list: Vec<_> = unique
                .iter()
                .map(|(definition_id, amount)| ChargeAmount {
                    definition_id: *definition_id,
                    amount: *amount,
                })
                .collect();
            let bounded: ChargeListOf<T> = list
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::TooManyChargeDefinitions)?;
            for charge in &bounded {
                ReservedSpellChargeBalances::<T>::try_mutate(
                    owner,
                    (realm, charge.definition_id),
                    |reserved| -> DispatchResult {
                        *reserved = reserved
                            .checked_add(charge.amount)
                            .ok_or(Error::<T>::ArithmeticOverflow)?;
                        Ok(())
                    },
                )?;
            }
            ChargeReservations::<T>::insert(
                session_id,
                ChargeReservation {
                    session_id,
                    owner: owner.clone(),
                    economic_realm: realm,
                    charges: bounded,
                },
            );
            Self::deposit_event(Event::SpellChargesReserved {
                owner: owner.clone(),
                session_id,
                economic_realm: realm,
                charges: list,
            });
            Ok(())
        }

        #[transactional]
        fn do_settle_charges(session_id: SessionId, used: &[(u32, u32)]) -> DispatchResult {
            let reservation =
                ChargeReservations::<T>::get(session_id).ok_or(Error::<T>::ReservationMissing)?;
            let used_map: BTreeMap<u32, u32> = used.iter().copied().collect();
            ensure!(
                used_map.len() == used.len(),
                Error::<T>::DuplicateChargeDefinition
            );
            for (definition_id, amount) in &used_map {
                let reserved = reservation
                    .charges
                    .iter()
                    .find(|charge| charge.definition_id == *definition_id)
                    .map(|charge| charge.amount)
                    .ok_or(Error::<T>::UsedChargeExceedsReservation)?;
                ensure!(
                    *amount <= reserved,
                    Error::<T>::UsedChargeExceedsReservation
                );
            }
            for charge in &reservation.charges {
                ReservedSpellChargeBalances::<T>::try_mutate(
                    &reservation.owner,
                    (reservation.economic_realm, charge.definition_id),
                    |reserved| -> DispatchResult {
                        *reserved = reserved
                            .checked_sub(charge.amount)
                            .ok_or(Error::<T>::ArithmeticOverflow)?;
                        Ok(())
                    },
                )?;
                let burned = used_map
                    .get(&charge.definition_id)
                    .copied()
                    .unwrap_or_default();
                if burned > 0 {
                    SpellChargeBalances::<T>::try_mutate(
                        &reservation.owner,
                        (reservation.economic_realm, charge.definition_id),
                        |balance| -> DispatchResult {
                            *balance = balance
                                .checked_sub(burned)
                                .ok_or(Error::<T>::ArithmeticOverflow)?;
                            Ok(())
                        },
                    )?;
                }
            }
            ChargeReservations::<T>::remove(session_id);
            Self::deposit_event(Event::SpellChargesConsumed {
                session_id,
                used: used_map
                    .into_iter()
                    .map(|(definition_id, amount)| ChargeAmount {
                        definition_id,
                        amount,
                    })
                    .collect(),
            });
            Ok(())
        }

        #[transactional]
        fn do_release_charges(session_id: SessionId) -> DispatchResult {
            let reservation =
                ChargeReservations::<T>::take(session_id).ok_or(Error::<T>::ReservationMissing)?;
            for charge in reservation.charges {
                ReservedSpellChargeBalances::<T>::try_mutate(
                    &reservation.owner,
                    (reservation.economic_realm, charge.definition_id),
                    |reserved| -> DispatchResult {
                        *reserved = reserved
                            .checked_sub(charge.amount)
                            .ok_or(Error::<T>::ArithmeticOverflow)?;
                        Ok(())
                    },
                )?;
            }
            Self::deposit_event(Event::SpellChargesReleased { session_id });
            Ok(())
        }

        fn do_grant_prism_xp(
            owner: &T::AccountId,
            spell_id: PrismSpellId,
            amount: u64,
            result_id: Hash32,
        ) -> DispatchResult {
            ensure!(
                amount <= T::MaxPrismXpGrant::get(),
                Error::<T>::PrismXpGrantTooLarge
            );
            let marker = Self::marker(result_id, 2);
            ensure!(
                !ProcessedMagicResults::<T>::contains_key(marker),
                Error::<T>::ResultAlreadyProcessed
            );
            PrismSpells::<T>::try_mutate(spell_id, |maybe| -> DispatchResult {
                let spell = maybe.as_mut().ok_or(Error::<T>::PrismSpellMissing)?;
                ensure!(&spell.owner == owner, Error::<T>::NotPrismOwner);
                ensure!(spell.lock.is_some(), Error::<T>::PrismNotLocked);
                let definition = PrismSpellDefinitions::<T>::get(spell.definition_id)
                    .ok_or(Error::<T>::DefinitionMissing)?;
                let old_level = spell.level;
                spell.xp = spell
                    .xp
                    .checked_add(amount)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                while spell.level < definition.max_level {
                    let threshold = u64::from(spell.level + 1)
                        .checked_mul(1_000)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    if spell.xp < threshold {
                        break;
                    }
                    spell.level = spell.level.saturating_add(1);
                }
                spell.revision = spell
                    .revision
                    .checked_add(1)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                if spell.level != old_level {
                    Self::deposit_event(Event::PrismSpellLeveled {
                        spell_id,
                        old_level,
                        new_level: spell.level,
                    });
                }
                Ok(())
            })?;
            ProcessedMagicResults::<T>::insert(marker, ());
            Self::deposit_event(Event::PrismSpellExperienceGranted {
                owner: owner.clone(),
                spell_id,
                amount,
                result_id,
            });
            Ok(())
        }
    }

    impl<T: Config> pallet_eterra_creatures::EssenceManager<T::AccountId> for Pallet<T> {
        fn consume(
            owner: &T::AccountId,
            realm: EconomicRealm,
            element_id: u8,
            amount: u32,
        ) -> DispatchResult {
            let element = match element_id {
                0 => Element::Neutral,
                1 => Element::Fire,
                2 => Element::Earth,
                3 => Element::Water,
                4 => Element::Wind,
                _ => return Err(Error::<T>::InvalidDefinition.into()),
            };
            EssenceBalances::<T>::try_mutate(
                owner,
                (realm, element),
                |balance| -> DispatchResult {
                    ensure!(
                        *balance >= u128::from(amount),
                        Error::<T>::InsufficientEssence
                    );
                    *balance -= u128::from(amount);
                    Ok(())
                },
            )?;
            Self::deposit_event(Event::EssenceConsumed {
                owner: owner.clone(),
                economic_realm: realm,
                element,
                amount,
            });
            Ok(())
        }
    }

    impl<T: Config> MagicManager<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
        fn validate_reward_definitions(
            charge_definition_id: Option<u32>,
            prism_definition_id: Option<u32>,
        ) -> DispatchResult {
            if let Some(definition_id) = charge_definition_id {
                ensure!(
                    SpellChargeDefinitions::<T>::contains_key(definition_id),
                    Error::<T>::DefinitionMissing
                );
            }
            if let Some(definition_id) = prism_definition_id {
                ensure!(
                    PrismSpellDefinitions::<T>::contains_key(definition_id),
                    Error::<T>::DefinitionMissing
                );
            }
            Ok(())
        }

        fn validate_session_loadout(
            owner: &T::AccountId,
            realm: EconomicRealm,
            limits: MagicLoadoutLimits,
            prisms: &[(PrismSpellId, u32)],
            charges: &[(u32, u32)],
        ) -> DispatchResult {
            ensure!(
                prisms.len() <= usize::from(limits.max_prisms)
                    && charges.len() <= usize::from(limits.max_charge_definitions),
                Error::<T>::MagicLoadoutViolation
            );
            let mut total_charge_count = 0u32;
            let mut total_load = 0u32;
            let mut charge_ids = BTreeSet::new();
            for (definition_id, amount) in charges {
                ensure!(
                    charge_ids.insert(*definition_id) && *amount > 0,
                    Error::<T>::MagicLoadoutViolation
                );
                let definition = SpellChargeDefinitions::<T>::get(definition_id)
                    .ok_or(Error::<T>::DefinitionMissing)?;
                ensure!(
                    *amount <= u32::from(definition.max_per_session),
                    Error::<T>::ChargeLimitExceeded
                );
                total_charge_count = total_charge_count
                    .checked_add(*amount)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                total_load = total_load
                    .checked_add(
                        u32::from(definition.competitive_load)
                            .checked_mul(*amount)
                            .ok_or(Error::<T>::ArithmeticOverflow)?,
                    )
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                let balance = SpellChargeBalances::<T>::get(owner, (realm, *definition_id));
                let reserved =
                    ReservedSpellChargeBalances::<T>::get(owner, (realm, *definition_id));
                ensure!(
                    balance.saturating_sub(reserved) >= *amount,
                    Error::<T>::InsufficientSpellCharges
                );
            }
            ensure!(
                total_charge_count <= u32::from(limits.max_total_charges),
                Error::<T>::MagicLoadoutViolation
            );
            let mut prism_ids = BTreeSet::new();
            for (spell_id, revision) in prisms {
                ensure!(
                    prism_ids.insert(*spell_id),
                    Error::<T>::MagicLoadoutViolation
                );
                let spell = PrismSpells::<T>::get(spell_id).ok_or(Error::<T>::PrismSpellMissing)?;
                ensure!(&spell.owner == owner, Error::<T>::NotPrismOwner);
                ensure!(
                    spell.economic_realm == realm,
                    Error::<T>::MagicLoadoutViolation
                );
                ensure!(
                    spell.revision == *revision && spell.lock.is_none(),
                    Error::<T>::MagicLoadoutViolation
                );
                let definition = PrismSpellDefinitions::<T>::get(spell.definition_id)
                    .ok_or(Error::<T>::DefinitionMissing)?;
                total_load = total_load
                    .checked_add(u32::from(definition.competitive_load))
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
            }
            ensure!(
                total_load <= u32::from(limits.max_magic_load),
                Error::<T>::MagicLoadoutViolation
            );
            Ok(())
        }

        fn reserve_charges(
            session_id: SessionId,
            owner: &T::AccountId,
            realm: EconomicRealm,
            charges: &[(u32, u32)],
        ) -> DispatchResult {
            Self::do_reserve_charges(session_id, owner, realm, charges)
        }

        fn settle_charges(session_id: SessionId, used: &[(u32, u32)]) -> DispatchResult {
            Self::do_settle_charges(session_id, used)
        }

        fn release_charges(session_id: SessionId) -> DispatchResult {
            Self::do_release_charges(session_id)
        }

        fn grant_essence(
            owner: &T::AccountId,
            realm: EconomicRealm,
            element: Element,
            amount: u32,
            result_id: Hash32,
        ) -> DispatchResult {
            Self::do_grant_essence(owner, realm, element, amount, result_id)
        }

        fn grant_spell_charges(
            owner: &T::AccountId,
            realm: EconomicRealm,
            definition_id: u32,
            amount: u32,
            result_id: Hash32,
        ) -> DispatchResult {
            Self::do_grant_spell_charges(owner, realm, definition_id, amount, result_id)
        }

        fn grant_prism_xp(
            owner: &T::AccountId,
            spell_id: PrismSpellId,
            amount: u64,
            result_id: Hash32,
        ) -> DispatchResult {
            Self::do_grant_prism_xp(owner, spell_id, amount, result_id)
        }

        fn create_prism_reward(
            owner: &T::AccountId,
            realm: EconomicRealm,
            definition_id: u32,
            traits_seed: Hash32,
            result_id: Hash32,
        ) -> DispatchResult {
            let marker = Self::marker(result_id, 3);
            ensure!(
                !ProcessedMagicResults::<T>::contains_key(marker),
                Error::<T>::ResultAlreadyProcessed
            );
            Self::do_create_prism(owner.clone(), realm, definition_id, traits_seed)?;
            ProcessedMagicResults::<T>::insert(marker, ());
            Ok(())
        }

        fn lock_prism(
            owner: &T::AccountId,
            spell_id: PrismSpellId,
            lock: AssetLock<BlockNumberFor<T>>,
        ) -> DispatchResult {
            ensure!(
                lock.role == AssetRole::PrismSpell,
                Error::<T>::InvalidDefinition
            );
            PrismSpells::<T>::try_mutate(spell_id, |maybe| -> DispatchResult {
                let spell = maybe.as_mut().ok_or(Error::<T>::PrismSpellMissing)?;
                ensure!(&spell.owner == owner, Error::<T>::NotPrismOwner);
                ensure!(spell.lock.is_none(), Error::<T>::PrismLocked);
                ensure!(
                    spell.revision == lock.revision_at_lock,
                    Error::<T>::InvalidDefinition
                );
                spell.lock = Some(lock);
                Self::deposit_event(Event::PrismSpellLocked {
                    spell_id,
                    session_id: lock.session_id,
                });
                Ok(())
            })
        }

        fn unlock_prism(session_id: SessionId, spell_id: PrismSpellId) -> DispatchResult {
            PrismSpells::<T>::try_mutate(spell_id, |maybe| -> DispatchResult {
                let spell = maybe.as_mut().ok_or(Error::<T>::PrismSpellMissing)?;
                let lock = spell.lock.ok_or(Error::<T>::PrismNotLocked)?;
                ensure!(lock.session_id == session_id, Error::<T>::WrongSessionLock);
                spell.lock = None;
                Self::deposit_event(Event::PrismSpellUnlocked {
                    spell_id,
                    session_id,
                    emergency: false,
                });
                Ok(())
            })
        }

        fn force_unlock_prism(session_id: SessionId, spell_id: PrismSpellId) -> DispatchResult {
            PrismSpells::<T>::try_mutate(spell_id, |maybe| -> DispatchResult {
                let spell = maybe.as_mut().ok_or(Error::<T>::PrismSpellMissing)?;
                if let Some(lock) = spell.lock {
                    ensure!(lock.session_id == session_id, Error::<T>::WrongSessionLock);
                    spell.lock = None;
                    Self::deposit_event(Event::PrismSpellUnlocked {
                        spell_id,
                        session_id,
                        emergency: true,
                    });
                }
                Ok(())
            })
        }
    }
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
