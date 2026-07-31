//! Blockchainia Flow pallet.
//!
//! This pallet owns game namespaces, immutable game versions, pinned instances,
//! player actions, actor nonces, and authority-attested event acceptance. The
//! v0 interpreter intentionally supports a small bounded rule set without
//! arbitrary scripts. All state changes remain runtime-authoritative.
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::too_many_arguments)]
// FRAME's pallet macro expands a transactional error path that triggers this
// lint even though the generated code is outside this crate's direct control.
#![allow(clippy::manual_inspect)]

pub use pallet::*;
pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(all(test, feature = "runtime-benchmarks"))]
mod mock;

#[frame_support::pallet]
pub mod pallet {
    use super::weights::WeightInfo;
    use codec::{Decode, Encode};
    use frame_support::{
        dispatch::DispatchResult, pallet_prelude::*, traits::StorageVersion, transactional,
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::{traits::Hash as HashT, DispatchError};

    pub type GameId = u64;
    pub type VersionId = u32;
    pub type InstanceId = u64;
    pub type ActorId = u64;
    pub type MachineId = u32;
    pub type StateId = u32;
    pub type ActionId = u32;
    pub type TransitionId = u64;
    pub type VariableId = u32;
    pub type ItemId = u32;
    pub type EntitlementId = u32;
    pub type CreditTypeId = u32;
    pub type EventTypeId = u32;
    pub type PassportFieldId = u32;
    pub type PassportBadgeId = u32;
    pub type AuthorityId = u64;

    pub trait AuthorityProvider<AccountId> {
        fn resolve_authority(
            account: &AccountId,
            game_id: GameId,
            version_id: Option<VersionId>,
            event_type: EventTypeId,
        ) -> Option<AuthorityId>;
    }

    impl<AccountId> AuthorityProvider<AccountId> for () {
        fn resolve_authority(
            _account: &AccountId,
            _game_id: GameId,
            _version_id: Option<VersionId>,
            _event_type: EventTypeId,
        ) -> Option<AuthorityId> {
            None
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    pub trait BenchmarkAuthorityProvider<AccountId> {
        fn authorize(
            account: &AccountId,
            game_id: GameId,
            version_id: VersionId,
            event_type: EventTypeId,
        ) -> DispatchResult;
    }

    #[cfg(feature = "runtime-benchmarks")]
    impl<AccountId> BenchmarkAuthorityProvider<AccountId> for () {
        fn authorize(
            _account: &AccountId,
            _game_id: GameId,
            _version_id: VersionId,
            _event_type: EventTypeId,
        ) -> DispatchResult {
            Ok(())
        }
    }

    pub trait EconomyProvider<AccountId> {
        fn has_entitlement(
            account: &AccountId,
            game_id: GameId,
            entitlement_id: EntitlementId,
        ) -> bool;
        fn credit_balance(account: &AccountId, game_id: GameId, credit_type: CreditTypeId) -> u64;
        fn consume_credit(
            account: &AccountId,
            game_id: GameId,
            credit_type: CreditTypeId,
            amount: u64,
        ) -> DispatchResult;
        fn grant_credit(
            account: &AccountId,
            game_id: GameId,
            credit_type: CreditTypeId,
            amount: u64,
        ) -> DispatchResult;
        fn grant_entitlement(
            account: &AccountId,
            game_id: GameId,
            entitlement_id: EntitlementId,
        ) -> DispatchResult;
        fn revoke_entitlement(
            account: &AccountId,
            game_id: GameId,
            entitlement_id: EntitlementId,
        ) -> DispatchResult;
        fn spend_sponsor_funds(game_id: GameId, amount: u128) -> DispatchResult;
    }

    impl<AccountId> EconomyProvider<AccountId> for () {
        fn has_entitlement(
            _account: &AccountId,
            _game_id: GameId,
            _entitlement_id: EntitlementId,
        ) -> bool {
            false
        }

        fn credit_balance(
            _account: &AccountId,
            _game_id: GameId,
            _credit_type: CreditTypeId,
        ) -> u64 {
            0
        }

        fn consume_credit(
            _account: &AccountId,
            _game_id: GameId,
            _credit_type: CreditTypeId,
            _amount: u64,
        ) -> DispatchResult {
            Err(DispatchError::Other("Eterra economy provider missing"))
        }

        fn grant_credit(
            _account: &AccountId,
            _game_id: GameId,
            _credit_type: CreditTypeId,
            _amount: u64,
        ) -> DispatchResult {
            Err(DispatchError::Other("Eterra economy provider missing"))
        }

        fn grant_entitlement(
            _account: &AccountId,
            _game_id: GameId,
            _entitlement_id: EntitlementId,
        ) -> DispatchResult {
            Err(DispatchError::Other("Eterra economy provider missing"))
        }

        fn revoke_entitlement(
            _account: &AccountId,
            _game_id: GameId,
            _entitlement_id: EntitlementId,
        ) -> DispatchResult {
            Err(DispatchError::Other("Eterra economy provider missing"))
        }

        fn spend_sponsor_funds(_game_id: GameId, _amount: u128) -> DispatchResult {
            Err(DispatchError::Other("Eterra economy provider missing"))
        }
    }

    pub trait ProfileProvider<AccountId> {
        fn update_passport_counter(
            account: &AccountId,
            field_id: PassportFieldId,
            amount: u64,
        ) -> DispatchResult;
        fn grant_passport_badge(account: &AccountId, badge_id: PassportBadgeId) -> DispatchResult;
        fn revoke_passport_badge(account: &AccountId, badge_id: PassportBadgeId) -> DispatchResult;
    }

    impl<AccountId> ProfileProvider<AccountId> for () {
        fn update_passport_counter(
            _account: &AccountId,
            _field_id: PassportFieldId,
            _amount: u64,
        ) -> DispatchResult {
            Err(DispatchError::Other("Eterra profile provider missing"))
        }

        fn grant_passport_badge(
            _account: &AccountId,
            _badge_id: PassportBadgeId,
        ) -> DispatchResult {
            Err(DispatchError::Other("Eterra profile provider missing"))
        }

        fn revoke_passport_badge(
            _account: &AccountId,
            _badge_id: PassportBadgeId,
        ) -> DispatchResult {
            Err(DispatchError::Other("Eterra profile provider missing"))
        }
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub enum GameStatus {
        Active,
        Paused,
        Retired,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub enum VersionStatus {
        Draft,
        Finalized,
        Active,
        Retired,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub enum InstanceStatus {
        Active,
        Paused,
        Finalized,
        Cancelled,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub enum Value {
        Bool(bool),
        U64(u64),
        I64(i64),
        Enum(u32),
    }

    impl Value {
        fn as_u64(self) -> Result<u64, DispatchError> {
            match self {
                Self::U64(value) => Ok(value),
                _ => Err(DispatchError::Other("Eterra value type mismatch")),
            }
        }

        fn checked_add<T: Config>(self, amount: u64) -> Result<Self, DispatchError> {
            Ok(Self::U64(
                self.as_u64()?
                    .checked_add(amount)
                    .ok_or(Error::<T>::ArithmeticOverflow)?,
            ))
        }

        fn checked_sub<T: Config>(self, amount: u64) -> Result<Self, DispatchError> {
            Ok(Self::U64(
                self.as_u64()?
                    .checked_sub(amount)
                    .ok_or(Error::<T>::Underflow)?,
            ))
        }
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub enum ValueType {
        Bool,
        U64,
        I64,
        Enum,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub enum VariableScope {
        Game,
        Instance,
        Actor,
        Entity,
        Passport,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub enum Scope {
        Game,
        Instance,
        Actor(ActorId),
        Entity(u64),
        Passport(ActorId),
    }

    impl Scope {
        fn variable_scope(self) -> VariableScope {
            match self {
                Self::Game => VariableScope::Game,
                Self::Instance => VariableScope::Instance,
                Self::Actor(_) => VariableScope::Actor,
                Self::Entity(_) => VariableScope::Entity,
                Self::Passport(_) => VariableScope::Passport,
            }
        }
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub struct VariableRef {
        pub scope: Scope,
        pub variable_id: VariableId,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub struct VariableDefinition {
        pub variable_id: VariableId,
        pub scope: VariableScope,
        pub value_type: ValueType,
        pub min: Option<i64>,
        pub max: Option<i64>,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum ConditionAtom {
        VarEquals(VariableRef, Value),
        VarGreaterOrEqual(VariableRef, u64),
        VarLessOrEqual(VariableRef, u64),
        HasItem {
            actor_id: ActorId,
            item_id: ItemId,
            amount: u64,
        },
        HasCredit {
            actor_id: ActorId,
            credit_type: CreditTypeId,
            amount: u64,
        },
        HasEntitlement {
            actor_id: ActorId,
            entitlement_id: EntitlementId,
        },
        MachineStateEquals {
            scope: Scope,
            machine_id: MachineId,
            state_id: StateId,
        },
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub enum Condition<T: Config> {
        All(BoundedVec<ConditionAtom, T::MaxConditionClauses>),
        Any(BoundedVec<ConditionAtom, T::MaxConditionClauses>),
        Not(ConditionAtom),
        Atom(ConditionAtom),
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum EconomyGateAtom {
        Free,
        DeveloperSponsored {
            amount: u128,
        },
        RequiresPayment {
            amount: u128,
        },
        RequiresEntitlement {
            entitlement_id: EntitlementId,
        },
        ConsumesCredit {
            credit_type: CreditTypeId,
            amount: u64,
        },
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub enum EconomyGate<T: Config> {
        Free,
        DeveloperSponsored {
            amount: u128,
        },
        RequiresPayment {
            amount: u128,
        },
        RequiresEntitlement {
            entitlement_id: EntitlementId,
        },
        ConsumesCredit {
            credit_type: CreditTypeId,
            amount: u64,
        },
        All(BoundedVec<EconomyGateAtom, T::MaxEconomyGateClauses>),
        Any(BoundedVec<EconomyGateAtom, T::MaxEconomyGateClauses>),
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum Effect {
        SetVar(VariableRef, Value),
        IncVar(VariableRef, u64),
        DecVar(VariableRef, u64),
        GrantItem {
            actor_id: ActorId,
            item_id: ItemId,
            amount: u64,
        },
        ConsumeItem {
            actor_id: ActorId,
            item_id: ItemId,
            amount: u64,
        },
        GrantCredit {
            actor_id: ActorId,
            credit_type: CreditTypeId,
            amount: u64,
        },
        GrantEntitlement {
            actor_id: ActorId,
            entitlement_id: EntitlementId,
        },
        RevokeEntitlement {
            actor_id: ActorId,
            entitlement_id: EntitlementId,
        },
        UpdatePassportCounter {
            actor_id: ActorId,
            field_id: PassportFieldId,
            amount: u64,
        },
        GrantPassportBadge {
            actor_id: ActorId,
            badge_id: PassportBadgeId,
        },
        RevokePassportBadge {
            actor_id: ActorId,
            badge_id: PassportBadgeId,
        },
        SetMachineState {
            scope: Scope,
            machine_id: MachineId,
            state_id: StateId,
        },
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum AttestedEffectPolicy {
        UpdatePassportCounter {
            field_id: PassportFieldId,
            amount: u64,
        },
        GrantPassportBadge {
            badge_id: PassportBadgeId,
        },
        RevokePassportBadge {
            badge_id: PassportBadgeId,
        },
        SetMachineState {
            scope: Scope,
            machine_id: MachineId,
            state_id: StateId,
        },
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct EventDefinition<T: Config> {
        pub event_type: EventTypeId,
        pub policies: BoundedVec<AttestedEffectPolicy, T::MaxEventEffectPolicies>,
    }

    #[derive(Encode, Decode, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound)]
    #[scale_info(skip_type_params(T))]
    pub enum AttestedEffect<T: Config> {
        UpdatePassportCounter {
            account: T::AccountId,
            field_id: PassportFieldId,
            amount: u64,
        },
        GrantPassportBadge {
            account: T::AccountId,
            badge_id: PassportBadgeId,
        },
        RevokePassportBadge {
            account: T::AccountId,
            badge_id: PassportBadgeId,
        },
        SetMachineState {
            scope: Scope,
            machine_id: MachineId,
            state_id: StateId,
        },
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct MachineDefinition<T: Config> {
        pub machine_id: MachineId,
        pub initial_state: StateId,
        pub states: BoundedVec<StateId, T::MaxStatesPerMachine>,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct Transition<T: Config> {
        pub transition_id: TransitionId,
        pub machine_id: MachineId,
        pub action_id: ActionId,
        pub from_state: Option<StateId>,
        pub to_state: Option<StateId>,
        pub priority: u16,
        pub conditions: BoundedVec<Condition<T>, T::MaxConditionsPerTransition>,
        pub economy_gate: EconomyGate<T>,
        pub effects: BoundedVec<Effect, T::MaxEffectsPerTransition>,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct Manifest<T: Config> {
        pub manifest_version: u16,
        pub game_id: GameId,
        pub version_id: VersionId,
        pub machines: BoundedVec<MachineDefinition<T>, T::MaxMachinesPerManifest>,
        pub variables: BoundedVec<VariableDefinition, T::MaxVariablesPerManifest>,
        pub actions: BoundedVec<ActionId, T::MaxActionsPerManifest>,
        pub transitions: BoundedVec<Transition<T>, T::MaxTransitionsPerManifest>,
        pub event_definitions: BoundedVec<EventDefinition<T>, T::MaxEventsPerManifest>,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct GameRecord<T: Config> {
        pub owner: T::AccountId,
        pub status: GameStatus,
        pub active_version: Option<VersionId>,
        pub metadata_hash: T::Hash,
        pub metadata_uri: BoundedVec<u8, T::MaxUriBytes>,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub struct VersionRecord<Hash> {
        pub status: VersionStatus,
        pub manifest_hash: Option<Hash>,
        pub chunk_count: u32,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct InstanceRecord<T: Config> {
        pub game_id: GameId,
        pub version_id: VersionId,
        pub creator: T::AccountId,
        pub status: InstanceStatus,
        pub config_hash: T::Hash,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;
        type AuthorityProvider: AuthorityProvider<Self::AccountId>;
        type EconomyProvider: EconomyProvider<Self::AccountId>;
        type ProfileProvider: ProfileProvider<Self::AccountId>;
        #[cfg(feature = "runtime-benchmarks")]
        type BenchmarkAuthorityProvider: BenchmarkAuthorityProvider<Self::AccountId>;

        #[pallet::constant]
        type MaxUriBytes: Get<u32>;
        #[pallet::constant]
        type MaxManifestChunkBytes: Get<u32>;
        #[pallet::constant]
        type MaxManifestChunks: Get<u32>;
        #[pallet::constant]
        type MaxManifestBytes: Get<u32>;
        #[pallet::constant]
        type MaxActionPayloadBytes: Get<u32>;
        #[pallet::constant]
        type MaxAttestedPayloadBytes: Get<u32>;
        #[pallet::constant]
        type MaxMachinesPerManifest: Get<u32>;
        #[pallet::constant]
        type MaxStatesPerMachine: Get<u32>;
        #[pallet::constant]
        type MaxVariablesPerManifest: Get<u32>;
        #[pallet::constant]
        type MaxActionsPerManifest: Get<u32>;
        #[pallet::constant]
        type MaxTransitionsPerManifest: Get<u32>;
        #[pallet::constant]
        type MaxConditionsPerTransition: Get<u32>;
        #[pallet::constant]
        type MaxConditionClauses: Get<u32>;
        #[pallet::constant]
        type MaxEconomyGateClauses: Get<u32>;
        #[pallet::constant]
        type MaxEffectsPerTransition: Get<u32>;
        #[pallet::constant]
        type MaxEventsPerManifest: Get<u32>;
        #[pallet::constant]
        type MaxAttestedEffectsPerEvent: Get<u32>;
        #[pallet::constant]
        type MaxEventEffectPolicies: Get<u32>;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type Games<T: Config> = StorageMap<_, Blake2_128Concat, GameId, GameRecord<T>>;

    #[pallet::storage]
    pub type Versions<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GameId,
        Blake2_128Concat,
        VersionId,
        VersionRecord<T::Hash>,
    >;

    #[pallet::storage]
    pub type VersionChunks<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, VersionId>,
            NMapKey<Blake2_128Concat, u32>,
        ),
        BoundedVec<u8, T::MaxManifestChunkBytes>,
    >;

    #[pallet::storage]
    pub type Manifests<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, GameId, Blake2_128Concat, VersionId, Manifest<T>>;

    #[pallet::storage]
    pub type Instances<T: Config> = StorageMap<_, Blake2_128Concat, InstanceId, InstanceRecord<T>>;

    #[pallet::storage]
    pub type ActorNonces<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, InstanceId>,
            NMapKey<Blake2_128Concat, ActorId>,
        ),
        u64,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type AttestedSequences<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, InstanceId>,
            NMapKey<Blake2_128Concat, AuthorityId>,
            NMapKey<Blake2_128Concat, EventTypeId>,
        ),
        u64,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type AttestedReplayHashes<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, InstanceId>,
            NMapKey<Blake2_128Concat, AuthorityId>,
            NMapKey<Blake2_128Concat, EventTypeId>,
            NMapKey<Blake2_128Concat, u64>,
        ),
        T::Hash,
    >;

    #[pallet::storage]
    pub type VariableValues<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, InstanceId>,
            NMapKey<Blake2_128Concat, Scope>,
            NMapKey<Blake2_128Concat, VariableId>,
        ),
        Value,
    >;

    #[pallet::storage]
    pub type MachineStates<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, InstanceId>,
            NMapKey<Blake2_128Concat, Scope>,
            NMapKey<Blake2_128Concat, MachineId>,
        ),
        StateId,
    >;

    #[pallet::storage]
    pub type Inventory<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, InstanceId>,
            NMapKey<Blake2_128Concat, ActorId>,
            NMapKey<Blake2_128Concat, ItemId>,
        ),
        u64,
        ValueQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        GameCreated {
            game_id: GameId,
            owner: T::AccountId,
        },
        VersionChunkUploaded {
            game_id: GameId,
            version_id: VersionId,
            chunk_index: u32,
        },
        VersionFinalized {
            game_id: GameId,
            version_id: VersionId,
            manifest_hash: T::Hash,
        },
        VersionActivated {
            game_id: GameId,
            version_id: VersionId,
        },
        InstanceCreated {
            game_id: GameId,
            instance_id: InstanceId,
            version_id: VersionId,
        },
        ActionSubmitted {
            game_id: GameId,
            instance_id: InstanceId,
            actor_id: ActorId,
            machine_id: MachineId,
            action_id: ActionId,
            transition_id: TransitionId,
            nonce: u64,
        },
        AttestedEventAccepted {
            game_id: GameId,
            instance_id: InstanceId,
            authority_id: AuthorityId,
            event_type: EventTypeId,
            next_sequence: u64,
            replay_hash: Option<T::Hash>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        GameAlreadyExists,
        GameNotFound,
        NotGameOwner,
        GamePausedOrRetired,
        VersionNotFound,
        VersionAlreadyFinalized,
        VersionNotFinalized,
        InvalidActiveVersion,
        InvalidChunkIndex,
        ManifestTooLarge,
        ManifestMissingChunk,
        ManifestHashMismatch,
        ManifestDecodeFailed,
        ManifestVersionUnsupported,
        ManifestGameMismatch,
        ManifestVersionMismatch,
        EmptyManifest,
        DuplicateMachine,
        DuplicateState,
        DuplicateVariable,
        DuplicateAction,
        DuplicateTransition,
        UnknownMachine,
        UnknownState,
        UnknownVariable,
        UnknownAction,
        InvalidCondition,
        InvalidEconomyGate,
        InvalidEffect,
        DuplicateEvent,
        UnknownEvent,
        InvalidAttestedEffect,
        ValueTypeMismatch,
        AmbiguousTransition,
        VersionManifestMissing,
        InstanceAlreadyExists,
        InstanceNotFound,
        InstanceNotActive,
        NonceMismatch,
        PayloadTooLarge,
        UnauthorizedAuthority,
        SequenceMismatch,
        NoMatchingTransition,
        PaymentRequired,
        MissingEntitlement,
        InvalidEffectActor,
        InsufficientItem,
        Underflow,
        ArithmeticOverflow,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::create_game())]
        pub fn create_game(
            origin: OriginFor<T>,
            game_id: GameId,
            metadata_hash: T::Hash,
            metadata_uri: BoundedVec<u8, T::MaxUriBytes>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                !Games::<T>::contains_key(game_id),
                Error::<T>::GameAlreadyExists
            );

            Games::<T>::insert(
                game_id,
                GameRecord::<T> {
                    owner: who.clone(),
                    status: GameStatus::Active,
                    active_version: None,
                    metadata_hash,
                    metadata_uri,
                },
            );
            Self::deposit_event(Event::GameCreated {
                game_id,
                owner: who,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::upload_version_chunk())]
        pub fn upload_version_chunk(
            origin: OriginFor<T>,
            game_id: GameId,
            version_id: VersionId,
            chunk_index: u32,
            chunk: BoundedVec<u8, T::MaxManifestChunkBytes>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner(game_id, &who)?;
            ensure!(
                chunk_index < T::MaxManifestChunks::get(),
                Error::<T>::InvalidChunkIndex
            );

            let mut version = Versions::<T>::get(game_id, version_id).unwrap_or(VersionRecord {
                status: VersionStatus::Draft,
                manifest_hash: None,
                chunk_count: 0,
            });
            ensure!(
                version.status == VersionStatus::Draft,
                Error::<T>::VersionAlreadyFinalized
            );
            ensure!(
                chunk_index <= version.chunk_count,
                Error::<T>::InvalidChunkIndex
            );

            let is_new_chunk =
                !VersionChunks::<T>::contains_key((game_id, version_id, chunk_index));
            VersionChunks::<T>::insert((game_id, version_id, chunk_index), chunk);
            if is_new_chunk && chunk_index == version.chunk_count {
                version.chunk_count = version
                    .chunk_count
                    .checked_add(1)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
            }
            Versions::<T>::insert(game_id, version_id, version);
            Self::deposit_event(Event::VersionChunkUploaded {
                game_id,
                version_id,
                chunk_index,
            });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::finalize_version())]
        pub fn finalize_version(
            origin: OriginFor<T>,
            game_id: GameId,
            version_id: VersionId,
            manifest_hash: T::Hash,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner(game_id, &who)?;
            let mut version =
                Versions::<T>::get(game_id, version_id).ok_or(Error::<T>::VersionNotFound)?;
            ensure!(
                version.status == VersionStatus::Draft,
                Error::<T>::VersionAlreadyFinalized
            );
            ensure!(version.chunk_count > 0, Error::<T>::ManifestMissingChunk);

            let encoded = Self::assembled_manifest_bytes(game_id, version_id, version.chunk_count)?;
            let computed_hash = T::Hashing::hash(encoded.as_slice());
            ensure!(
                computed_hash == manifest_hash,
                Error::<T>::ManifestHashMismatch
            );

            let mut input = encoded.as_slice();
            let manifest =
                Manifest::<T>::decode(&mut input).map_err(|_| Error::<T>::ManifestDecodeFailed)?;
            ensure!(input.is_empty(), Error::<T>::ManifestDecodeFailed);
            Self::validate_manifest(&manifest, game_id, version_id)?;

            Manifests::<T>::insert(game_id, version_id, manifest);
            version.status = VersionStatus::Finalized;
            version.manifest_hash = Some(manifest_hash);
            Versions::<T>::insert(game_id, version_id, version);
            Self::deposit_event(Event::VersionFinalized {
                game_id,
                version_id,
                manifest_hash,
            });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::activate_version())]
        pub fn activate_version(
            origin: OriginFor<T>,
            game_id: GameId,
            version_id: VersionId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner(game_id, &who)?;
            let version =
                Versions::<T>::get(game_id, version_id).ok_or(Error::<T>::VersionNotFound)?;
            ensure!(
                version.status == VersionStatus::Finalized
                    || version.status == VersionStatus::Active,
                Error::<T>::VersionNotFinalized
            );
            ensure!(
                Manifests::<T>::contains_key(game_id, version_id),
                Error::<T>::VersionManifestMissing
            );

            Versions::<T>::mutate(game_id, version_id, |maybe_version| {
                if let Some(version) = maybe_version {
                    version.status = VersionStatus::Active;
                }
            });
            Games::<T>::try_mutate(game_id, |maybe_game| -> DispatchResult {
                let game = maybe_game.as_mut().ok_or(Error::<T>::GameNotFound)?;
                game.active_version = Some(version_id);
                Ok(())
            })?;
            Self::deposit_event(Event::VersionActivated {
                game_id,
                version_id,
            });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::create_instance())]
        pub fn create_instance(
            origin: OriginFor<T>,
            game_id: GameId,
            instance_id: InstanceId,
            version_id: Option<VersionId>,
            config_hash: T::Hash,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                !Instances::<T>::contains_key(instance_id),
                Error::<T>::InstanceAlreadyExists
            );
            let game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;
            ensure!(
                game.status == GameStatus::Active,
                Error::<T>::GamePausedOrRetired
            );
            let version_id = version_id
                .or(game.active_version)
                .ok_or(Error::<T>::InvalidActiveVersion)?;
            let version =
                Versions::<T>::get(game_id, version_id).ok_or(Error::<T>::VersionNotFound)?;
            ensure!(
                version.status == VersionStatus::Active,
                Error::<T>::InvalidActiveVersion
            );
            ensure!(
                Manifests::<T>::contains_key(game_id, version_id),
                Error::<T>::VersionManifestMissing
            );

            Instances::<T>::insert(
                instance_id,
                InstanceRecord::<T> {
                    game_id,
                    version_id,
                    creator: who,
                    status: InstanceStatus::Active,
                    config_hash,
                },
            );
            Self::deposit_event(Event::InstanceCreated {
                game_id,
                instance_id,
                version_id,
            });
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::submit_action())]
        #[transactional]
        pub fn submit_action(
            origin: OriginFor<T>,
            game_id: GameId,
            instance_id: InstanceId,
            actor_id: ActorId,
            machine_id: MachineId,
            action_id: ActionId,
            expected_nonce: u64,
            _payload: BoundedVec<u8, T::MaxActionPayloadBytes>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;
            ensure!(
                game.status == GameStatus::Active,
                Error::<T>::GamePausedOrRetired
            );
            let instance = Instances::<T>::get(instance_id).ok_or(Error::<T>::InstanceNotFound)?;
            ensure!(instance.game_id == game_id, Error::<T>::InstanceNotFound);
            ensure!(
                instance.status == InstanceStatus::Active,
                Error::<T>::InstanceNotActive
            );
            let manifest = Manifests::<T>::get(game_id, instance.version_id)
                .ok_or(Error::<T>::VersionManifestMissing)?;

            let nonce = ActorNonces::<T>::get((game_id, instance_id, actor_id));
            ensure!(nonce == expected_nonce, Error::<T>::NonceMismatch);

            let current_state = Self::machine_state(
                &manifest,
                game_id,
                instance_id,
                Scope::Actor(actor_id),
                machine_id,
            );
            let transition = Self::select_transition(
                &manifest,
                game_id,
                instance_id,
                &who,
                actor_id,
                machine_id,
                action_id,
                current_state,
            )
            .ok_or(Error::<T>::NoMatchingTransition)?;

            Self::apply_economy_gate(&who, game_id, actor_id, &transition.economy_gate)?;
            Self::apply_effects(&who, game_id, instance_id, actor_id, &transition.effects)?;
            if let Some(to_state) = transition.to_state {
                MachineStates::<T>::insert(
                    (game_id, instance_id, Scope::Actor(actor_id), machine_id),
                    to_state,
                );
            }

            let next_nonce = expected_nonce
                .checked_add(1)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ActorNonces::<T>::insert((game_id, instance_id, actor_id), next_nonce);

            Self::deposit_event(Event::ActionSubmitted {
                game_id,
                instance_id,
                actor_id,
                machine_id,
                action_id,
                transition_id: transition.transition_id,
                nonce: next_nonce,
            });
            Ok(())
        }

        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::submit_attested_event())]
        #[transactional]
        pub fn submit_attested_event(
            origin: OriginFor<T>,
            game_id: GameId,
            instance_id: InstanceId,
            event_type: EventTypeId,
            sequence: u64,
            _payload: BoundedVec<u8, T::MaxAttestedPayloadBytes>,
            replay_hash: Option<T::Hash>,
            effects: BoundedVec<AttestedEffect<T>, T::MaxAttestedEffectsPerEvent>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;
            ensure!(
                game.status == GameStatus::Active,
                Error::<T>::GamePausedOrRetired
            );
            let instance = Instances::<T>::get(instance_id).ok_or(Error::<T>::InstanceNotFound)?;
            ensure!(instance.game_id == game_id, Error::<T>::InstanceNotFound);
            ensure!(
                instance.status == InstanceStatus::Active,
                Error::<T>::InstanceNotActive
            );
            let manifest = Manifests::<T>::get(game_id, instance.version_id)
                .ok_or(Error::<T>::VersionManifestMissing)?;
            let authority_id = T::AuthorityProvider::resolve_authority(
                &who,
                game_id,
                Some(instance.version_id),
                event_type,
            )
            .ok_or(Error::<T>::UnauthorizedAuthority)?;
            let event_definition =
                Self::event_definition(&manifest, event_type).ok_or(Error::<T>::UnknownEvent)?;

            let expected =
                AttestedSequences::<T>::get((game_id, instance_id, authority_id, event_type));
            ensure!(expected == sequence, Error::<T>::SequenceMismatch);
            Self::ensure_attested_effects_allowed(event_definition, &effects)?;
            Self::apply_attested_effects(game_id, instance_id, &effects)?;
            let next_sequence = expected
                .checked_add(1)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            AttestedSequences::<T>::insert(
                (game_id, instance_id, authority_id, event_type),
                next_sequence,
            );
            if let Some(hash) = replay_hash {
                AttestedReplayHashes::<T>::insert(
                    (game_id, instance_id, authority_id, event_type, sequence),
                    hash,
                );
            }
            Self::deposit_event(Event::AttestedEventAccepted {
                game_id,
                instance_id,
                authority_id,
                event_type,
                next_sequence,
                replay_hash,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn ensure_owner(game_id: GameId, who: &T::AccountId) -> Result<(), DispatchError> {
            let game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;
            ensure!(game.owner == *who, Error::<T>::NotGameOwner);
            Ok(())
        }

        fn assembled_manifest_bytes(
            game_id: GameId,
            version_id: VersionId,
            chunk_count: u32,
        ) -> Result<BoundedVec<u8, T::MaxManifestBytes>, DispatchError> {
            ensure!(
                chunk_count <= T::MaxManifestChunks::get(),
                Error::<T>::InvalidChunkIndex
            );
            let mut encoded = BoundedVec::<u8, T::MaxManifestBytes>::default();
            let mut index = 0;
            while index < chunk_count {
                let chunk = VersionChunks::<T>::get((game_id, version_id, index))
                    .ok_or(Error::<T>::ManifestMissingChunk)?;
                for byte in chunk {
                    encoded
                        .try_push(byte)
                        .map_err(|_| Error::<T>::ManifestTooLarge)?;
                }
                index = index.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
            }
            Ok(encoded)
        }

        pub fn validate_manifest(
            manifest: &Manifest<T>,
            game_id: GameId,
            version_id: VersionId,
        ) -> DispatchResult {
            ensure!(
                manifest.manifest_version == 0,
                Error::<T>::ManifestVersionUnsupported
            );
            ensure!(
                manifest.game_id == game_id,
                Error::<T>::ManifestGameMismatch
            );
            ensure!(
                manifest.version_id == version_id,
                Error::<T>::ManifestVersionMismatch
            );
            ensure!(!manifest.machines.is_empty(), Error::<T>::EmptyManifest);
            ensure!(!manifest.actions.is_empty(), Error::<T>::EmptyManifest);

            for (left_index, machine) in manifest.machines.iter().enumerate() {
                ensure!(
                    machine.states.contains(&machine.initial_state),
                    Error::<T>::UnknownState
                );
                for state_index in 0..machine.states.len() {
                    for other_state_index in (state_index + 1)..machine.states.len() {
                        ensure!(
                            machine.states[state_index] != machine.states[other_state_index],
                            Error::<T>::DuplicateState
                        );
                    }
                }
                for other in manifest.machines.iter().skip(left_index + 1) {
                    ensure!(
                        machine.machine_id != other.machine_id,
                        Error::<T>::DuplicateMachine
                    );
                }
            }

            for (left_index, variable) in manifest.variables.iter().enumerate() {
                for other in manifest.variables.iter().skip(left_index + 1) {
                    ensure!(
                        variable.variable_id != other.variable_id || variable.scope != other.scope,
                        Error::<T>::DuplicateVariable
                    );
                }
            }

            for (left_index, action_id) in manifest.actions.iter().enumerate() {
                for other_action_id in manifest.actions.iter().skip(left_index + 1) {
                    ensure!(action_id != other_action_id, Error::<T>::DuplicateAction);
                }
            }

            for (left_index, transition) in manifest.transitions.iter().enumerate() {
                ensure!(
                    Self::machine_definition(manifest, transition.machine_id).is_some(),
                    Error::<T>::UnknownMachine
                );
                ensure!(
                    manifest.actions.contains(&transition.action_id),
                    Error::<T>::UnknownAction
                );
                if let Some(state_id) = transition.from_state {
                    ensure!(
                        Self::state_exists(manifest, transition.machine_id, state_id),
                        Error::<T>::UnknownState
                    );
                }
                if let Some(state_id) = transition.to_state {
                    ensure!(
                        Self::state_exists(manifest, transition.machine_id, state_id),
                        Error::<T>::UnknownState
                    );
                }
                for condition in &transition.conditions {
                    Self::validate_condition(manifest, condition)?;
                }
                Self::validate_economy_gate(&transition.economy_gate)?;
                for effect in &transition.effects {
                    Self::validate_effect(manifest, effect)?;
                }
                for other in manifest.transitions.iter().skip(left_index + 1) {
                    ensure!(
                        transition.transition_id != other.transition_id,
                        Error::<T>::DuplicateTransition
                    );
                    ensure!(
                        transition.machine_id != other.machine_id
                            || transition.action_id != other.action_id
                            || transition.from_state != other.from_state
                            || transition.priority != other.priority,
                        Error::<T>::AmbiguousTransition
                    );
                }
            }

            for (left_index, event_definition) in manifest.event_definitions.iter().enumerate() {
                for other in manifest.event_definitions.iter().skip(left_index + 1) {
                    ensure!(
                        event_definition.event_type != other.event_type,
                        Error::<T>::DuplicateEvent
                    );
                }
                for policy in &event_definition.policies {
                    Self::validate_attested_effect_policy(manifest, policy)?;
                }
            }

            Ok(())
        }

        /// Return the canonical manifest hash accepted by `finalize_version`.
        ///
        /// The compiler contract is intentionally byte-level: the hash is over
        /// the exact SCALE encoding of `Manifest<T>` with no trailing bytes,
        /// wrapper fields, UI labels, or editor metadata.
        pub fn canonical_manifest_hash(manifest: &Manifest<T>) -> T::Hash {
            T::Hashing::hash(&manifest.encode())
        }

        fn machine_definition(
            manifest: &Manifest<T>,
            machine_id: MachineId,
        ) -> Option<&MachineDefinition<T>> {
            manifest
                .machines
                .iter()
                .find(|machine| machine.machine_id == machine_id)
        }

        fn event_definition(
            manifest: &Manifest<T>,
            event_type: EventTypeId,
        ) -> Option<&EventDefinition<T>> {
            manifest
                .event_definitions
                .iter()
                .find(|definition| definition.event_type == event_type)
        }

        fn state_exists(manifest: &Manifest<T>, machine_id: MachineId, state_id: StateId) -> bool {
            Self::machine_definition(manifest, machine_id)
                .is_some_and(|machine| machine.states.contains(&state_id))
        }

        fn variable_definition<'a>(
            manifest: &'a Manifest<T>,
            variable_ref: &VariableRef,
        ) -> Option<&'a VariableDefinition> {
            manifest.variables.iter().find(|variable| {
                variable.variable_id == variable_ref.variable_id
                    && variable.scope == variable_ref.scope.variable_scope()
            })
        }

        fn validate_variable_ref<'a>(
            manifest: &'a Manifest<T>,
            variable_ref: &VariableRef,
            expected_value_type: Option<ValueType>,
        ) -> Result<&'a VariableDefinition, DispatchError> {
            let variable = Self::variable_definition(manifest, variable_ref)
                .ok_or(Error::<T>::UnknownVariable)?;
            if let Some(value_type) = expected_value_type {
                ensure!(
                    variable.value_type == value_type,
                    Error::<T>::ValueTypeMismatch
                );
            }
            Ok(variable)
        }

        fn validate_condition(manifest: &Manifest<T>, condition: &Condition<T>) -> DispatchResult {
            match condition {
                Condition::All(atoms) | Condition::Any(atoms) => {
                    ensure!(!atoms.is_empty(), Error::<T>::InvalidCondition);
                    for atom in atoms {
                        Self::validate_condition_atom(manifest, atom)?;
                    }
                }
                Condition::Not(atom) | Condition::Atom(atom) => {
                    Self::validate_condition_atom(manifest, atom)?;
                }
            }
            Ok(())
        }

        fn validate_condition_atom(manifest: &Manifest<T>, atom: &ConditionAtom) -> DispatchResult {
            match atom {
                ConditionAtom::VarEquals(variable_ref, value) => {
                    let variable = Self::validate_variable_ref(manifest, variable_ref, None)?;
                    ensure!(
                        Self::value_matches_type(*value, variable.value_type)
                            && Self::value_within_bounds(variable, *value),
                        Error::<T>::ValueTypeMismatch
                    );
                }
                ConditionAtom::VarGreaterOrEqual(variable_ref, _)
                | ConditionAtom::VarLessOrEqual(variable_ref, _) => {
                    Self::validate_variable_ref(manifest, variable_ref, Some(ValueType::U64))?;
                }
                ConditionAtom::HasItem { amount, .. } | ConditionAtom::HasCredit { amount, .. } => {
                    ensure!(*amount > 0, Error::<T>::InvalidCondition);
                }
                ConditionAtom::HasEntitlement { .. } => {}
                ConditionAtom::MachineStateEquals {
                    machine_id,
                    state_id,
                    ..
                } => {
                    ensure!(
                        Self::state_exists(manifest, *machine_id, *state_id),
                        Error::<T>::UnknownState
                    );
                }
            }
            Ok(())
        }

        fn validate_economy_gate(gate: &EconomyGate<T>) -> DispatchResult {
            match gate {
                EconomyGate::Free => {}
                EconomyGate::DeveloperSponsored { amount }
                | EconomyGate::RequiresPayment { amount } => {
                    ensure!(*amount > 0, Error::<T>::InvalidEconomyGate);
                }
                EconomyGate::RequiresEntitlement { .. } => {}
                EconomyGate::ConsumesCredit { amount, .. } => {
                    ensure!(*amount > 0, Error::<T>::InvalidEconomyGate);
                }
                EconomyGate::All(gates) | EconomyGate::Any(gates) => {
                    ensure!(!gates.is_empty(), Error::<T>::InvalidEconomyGate);
                    for atom in gates {
                        Self::validate_economy_gate_atom(atom)?;
                    }
                }
            }
            Ok(())
        }

        fn validate_economy_gate_atom(gate: &EconomyGateAtom) -> DispatchResult {
            match gate {
                EconomyGateAtom::Free => {}
                EconomyGateAtom::DeveloperSponsored { amount }
                | EconomyGateAtom::RequiresPayment { amount } => {
                    ensure!(*amount > 0, Error::<T>::InvalidEconomyGate);
                }
                EconomyGateAtom::RequiresEntitlement { .. } => {}
                EconomyGateAtom::ConsumesCredit { amount, .. } => {
                    ensure!(*amount > 0, Error::<T>::InvalidEconomyGate);
                }
            }
            Ok(())
        }

        fn validate_effect(manifest: &Manifest<T>, effect: &Effect) -> DispatchResult {
            match effect {
                Effect::SetVar(variable_ref, value) => {
                    let variable = Self::validate_variable_ref(manifest, variable_ref, None)?;
                    ensure!(
                        Self::value_matches_type(*value, variable.value_type)
                            && Self::value_within_bounds(variable, *value),
                        Error::<T>::ValueTypeMismatch
                    );
                }
                Effect::IncVar(variable_ref, amount) | Effect::DecVar(variable_ref, amount) => {
                    Self::validate_variable_ref(manifest, variable_ref, Some(ValueType::U64))?;
                    ensure!(*amount > 0, Error::<T>::InvalidEffect);
                }
                Effect::GrantItem { amount, .. }
                | Effect::ConsumeItem { amount, .. }
                | Effect::GrantCredit { amount, .. }
                | Effect::UpdatePassportCounter { amount, .. } => {
                    ensure!(*amount > 0, Error::<T>::InvalidEffect);
                }
                Effect::GrantEntitlement { .. } | Effect::RevokeEntitlement { .. } => {}
                Effect::GrantPassportBadge { .. } | Effect::RevokePassportBadge { .. } => {}
                Effect::SetMachineState {
                    machine_id,
                    state_id,
                    ..
                } => {
                    ensure!(
                        Self::state_exists(manifest, *machine_id, *state_id),
                        Error::<T>::UnknownState
                    );
                }
            }
            Ok(())
        }

        fn validate_attested_effect_policy(
            manifest: &Manifest<T>,
            policy: &AttestedEffectPolicy,
        ) -> DispatchResult {
            match policy {
                AttestedEffectPolicy::UpdatePassportCounter { amount, .. } => {
                    ensure!(*amount > 0, Error::<T>::InvalidAttestedEffect);
                }
                AttestedEffectPolicy::GrantPassportBadge { .. }
                | AttestedEffectPolicy::RevokePassportBadge { .. } => {}
                AttestedEffectPolicy::SetMachineState {
                    machine_id,
                    state_id,
                    ..
                } => {
                    ensure!(
                        Self::state_exists(manifest, *machine_id, *state_id),
                        Error::<T>::UnknownState
                    );
                }
            }
            Ok(())
        }

        fn value_matches_type(value: Value, value_type: ValueType) -> bool {
            matches!(
                (value, value_type),
                (Value::Bool(_), ValueType::Bool)
                    | (Value::U64(_), ValueType::U64)
                    | (Value::I64(_), ValueType::I64)
                    | (Value::Enum(_), ValueType::Enum)
            )
        }

        fn value_within_bounds(variable: &VariableDefinition, value: Value) -> bool {
            match value {
                Value::Bool(_) => true,
                Value::U64(value) => {
                    let min_ok = variable
                        .min
                        .map_or(true, |min| min <= 0 || value >= min as u64);
                    let max_ok = variable
                        .max
                        .map_or(true, |max| max >= 0 && value <= max as u64);
                    min_ok && max_ok
                }
                Value::I64(value) => {
                    let min_ok = variable.min.map_or(true, |min| value >= min);
                    let max_ok = variable.max.map_or(true, |max| value <= max);
                    min_ok && max_ok
                }
                Value::Enum(value) => {
                    let min_ok = variable
                        .min
                        .map_or(true, |min| min <= 0 || value >= min as u32);
                    let max_ok = variable
                        .max
                        .map_or(true, |max| max >= 0 && value <= max as u32);
                    min_ok && max_ok
                }
            }
        }

        fn select_transition<'a>(
            manifest: &'a Manifest<T>,
            game_id: GameId,
            instance_id: InstanceId,
            account: &T::AccountId,
            actor_id: ActorId,
            machine_id: MachineId,
            action_id: ActionId,
            current_state: Option<StateId>,
        ) -> Option<&'a Transition<T>> {
            let mut selected: Option<&Transition<T>> = None;
            for transition in &manifest.transitions {
                if transition.machine_id != machine_id || transition.action_id != action_id {
                    continue;
                }
                if transition.from_state.is_some() && transition.from_state != current_state {
                    continue;
                }
                if !Self::conditions_hold(
                    &transition.conditions,
                    manifest,
                    game_id,
                    instance_id,
                    account,
                    actor_id,
                ) {
                    continue;
                }
                if selected.map_or(true, |current| {
                    (transition.priority, transition.transition_id)
                        < (current.priority, current.transition_id)
                }) {
                    selected = Some(transition);
                }
            }
            selected
        }

        fn conditions_hold(
            conditions: &BoundedVec<Condition<T>, T::MaxConditionsPerTransition>,
            manifest: &Manifest<T>,
            game_id: GameId,
            instance_id: InstanceId,
            account: &T::AccountId,
            actor_id: ActorId,
        ) -> bool {
            conditions.iter().all(|condition| {
                Self::condition_holds(manifest, game_id, instance_id, account, actor_id, condition)
            })
        }

        fn condition_holds(
            manifest: &Manifest<T>,
            game_id: GameId,
            instance_id: InstanceId,
            account: &T::AccountId,
            action_actor: ActorId,
            condition: &Condition<T>,
        ) -> bool {
            match condition {
                Condition::All(atoms) => atoms.iter().all(|atom| {
                    Self::condition_atom_holds(
                        manifest,
                        game_id,
                        instance_id,
                        account,
                        action_actor,
                        atom,
                    )
                }),
                Condition::Any(atoms) => atoms.iter().any(|atom| {
                    Self::condition_atom_holds(
                        manifest,
                        game_id,
                        instance_id,
                        account,
                        action_actor,
                        atom,
                    )
                }),
                Condition::Not(atom) => !Self::condition_atom_holds(
                    manifest,
                    game_id,
                    instance_id,
                    account,
                    action_actor,
                    atom,
                ),
                Condition::Atom(atom) => Self::condition_atom_holds(
                    manifest,
                    game_id,
                    instance_id,
                    account,
                    action_actor,
                    atom,
                ),
            }
        }

        fn condition_atom_holds(
            manifest: &Manifest<T>,
            game_id: GameId,
            instance_id: InstanceId,
            account: &T::AccountId,
            action_actor: ActorId,
            atom: &ConditionAtom,
        ) -> bool {
            match atom {
                ConditionAtom::VarEquals(variable_ref, value) => {
                    Self::value(game_id, instance_id, variable_ref) == Some(*value)
                }
                ConditionAtom::VarGreaterOrEqual(variable_ref, threshold) => {
                    Self::value(game_id, instance_id, variable_ref)
                        .and_then(|value| value.as_u64().ok())
                        .is_some_and(|value| value >= *threshold)
                }
                ConditionAtom::VarLessOrEqual(variable_ref, threshold) => {
                    Self::value(game_id, instance_id, variable_ref)
                        .and_then(|value| value.as_u64().ok())
                        .is_some_and(|value| value <= *threshold)
                }
                ConditionAtom::HasItem {
                    actor_id,
                    item_id,
                    amount,
                } => Self::item_balance(game_id, instance_id, *actor_id, *item_id) >= *amount,
                ConditionAtom::HasCredit {
                    actor_id,
                    credit_type,
                    amount,
                } => {
                    *actor_id == action_actor
                        && T::EconomyProvider::credit_balance(account, game_id, *credit_type)
                            >= *amount
                }
                ConditionAtom::HasEntitlement {
                    actor_id,
                    entitlement_id,
                } => {
                    *actor_id == action_actor
                        && T::EconomyProvider::has_entitlement(account, game_id, *entitlement_id)
                }
                ConditionAtom::MachineStateEquals {
                    scope,
                    machine_id,
                    state_id,
                } => {
                    Self::machine_state(manifest, game_id, instance_id, *scope, *machine_id)
                        == Some(*state_id)
                }
            }
        }

        pub fn value(
            game_id: GameId,
            instance_id: InstanceId,
            variable_ref: &VariableRef,
        ) -> Option<Value> {
            VariableValues::<T>::get((
                game_id,
                Self::storage_instance_for_scope(instance_id, variable_ref.scope),
                variable_ref.scope,
                variable_ref.variable_id,
            ))
        }

        pub fn item_balance(
            game_id: GameId,
            instance_id: InstanceId,
            actor_id: ActorId,
            item_id: ItemId,
        ) -> u64 {
            Inventory::<T>::get((game_id, instance_id, actor_id, item_id))
        }

        pub fn machine_state(
            manifest: &Manifest<T>,
            game_id: GameId,
            instance_id: InstanceId,
            scope: Scope,
            machine_id: MachineId,
        ) -> Option<StateId> {
            MachineStates::<T>::get((game_id, instance_id, scope, machine_id)).or_else(|| {
                Self::machine_definition(manifest, machine_id).map(|machine| machine.initial_state)
            })
        }

        fn storage_instance_for_scope(instance_id: InstanceId, scope: Scope) -> InstanceId {
            match scope {
                Scope::Game | Scope::Passport(_) => 0,
                Scope::Instance | Scope::Actor(_) | Scope::Entity(_) => instance_id,
            }
        }

        fn apply_economy_gate(
            account: &T::AccountId,
            game_id: GameId,
            _actor_id: ActorId,
            gate: &EconomyGate<T>,
        ) -> DispatchResult {
            match gate {
                EconomyGate::Free => Ok(()),
                EconomyGate::DeveloperSponsored { amount } => {
                    T::EconomyProvider::spend_sponsor_funds(game_id, *amount)
                }
                EconomyGate::RequiresPayment { .. } => Err(Error::<T>::PaymentRequired.into()),
                EconomyGate::RequiresEntitlement { entitlement_id } => {
                    ensure!(
                        T::EconomyProvider::has_entitlement(account, game_id, *entitlement_id),
                        Error::<T>::MissingEntitlement
                    );
                    Ok(())
                }
                EconomyGate::ConsumesCredit {
                    credit_type,
                    amount,
                } => T::EconomyProvider::consume_credit(account, game_id, *credit_type, *amount),
                EconomyGate::All(gates) => {
                    for gate in gates {
                        Self::apply_economy_gate_atom(account, game_id, gate)?;
                    }
                    Ok(())
                }
                EconomyGate::Any(gates) => {
                    for gate in gates {
                        if Self::apply_economy_gate_atom(account, game_id, gate).is_ok() {
                            return Ok(());
                        }
                    }
                    Err(Error::<T>::InvalidEconomyGate.into())
                }
            }
        }

        fn apply_economy_gate_atom(
            account: &T::AccountId,
            game_id: GameId,
            gate: &EconomyGateAtom,
        ) -> DispatchResult {
            match gate {
                EconomyGateAtom::Free => Ok(()),
                EconomyGateAtom::DeveloperSponsored { amount } => {
                    T::EconomyProvider::spend_sponsor_funds(game_id, *amount)
                }
                EconomyGateAtom::RequiresPayment { .. } => Err(Error::<T>::PaymentRequired.into()),
                EconomyGateAtom::RequiresEntitlement { entitlement_id } => {
                    ensure!(
                        T::EconomyProvider::has_entitlement(account, game_id, *entitlement_id),
                        Error::<T>::MissingEntitlement
                    );
                    Ok(())
                }
                EconomyGateAtom::ConsumesCredit {
                    credit_type,
                    amount,
                } => T::EconomyProvider::consume_credit(account, game_id, *credit_type, *amount),
            }
        }

        fn apply_effects(
            account: &T::AccountId,
            game_id: GameId,
            instance_id: InstanceId,
            action_actor: ActorId,
            effects: &BoundedVec<Effect, T::MaxEffectsPerTransition>,
        ) -> DispatchResult {
            for effect in effects {
                Self::apply_effect(account, game_id, instance_id, action_actor, effect)?;
            }
            Ok(())
        }

        fn ensure_attested_effects_allowed(
            event_definition: &EventDefinition<T>,
            effects: &BoundedVec<AttestedEffect<T>, T::MaxAttestedEffectsPerEvent>,
        ) -> DispatchResult {
            if !event_definition.policies.is_empty() {
                ensure!(!effects.is_empty(), Error::<T>::InvalidAttestedEffect);
            }
            for effect in effects {
                ensure!(
                    event_definition
                        .policies
                        .iter()
                        .any(|policy| Self::attested_policy_allows(policy, effect)),
                    Error::<T>::InvalidAttestedEffect
                );
            }
            Ok(())
        }

        fn attested_policy_allows(
            policy: &AttestedEffectPolicy,
            effect: &AttestedEffect<T>,
        ) -> bool {
            match (policy, effect) {
                (
                    AttestedEffectPolicy::UpdatePassportCounter { field_id, amount },
                    AttestedEffect::UpdatePassportCounter {
                        field_id: effect_field,
                        amount: effect_amount,
                        ..
                    },
                ) => field_id == effect_field && amount == effect_amount,
                (
                    AttestedEffectPolicy::GrantPassportBadge { badge_id },
                    AttestedEffect::GrantPassportBadge {
                        badge_id: effect_badge,
                        ..
                    },
                ) => badge_id == effect_badge,
                (
                    AttestedEffectPolicy::RevokePassportBadge { badge_id },
                    AttestedEffect::RevokePassportBadge {
                        badge_id: effect_badge,
                        ..
                    },
                ) => badge_id == effect_badge,
                (
                    AttestedEffectPolicy::SetMachineState {
                        scope,
                        machine_id,
                        state_id,
                    },
                    AttestedEffect::SetMachineState {
                        scope: effect_scope,
                        machine_id: effect_machine,
                        state_id: effect_state,
                    },
                ) => {
                    scope == effect_scope
                        && machine_id == effect_machine
                        && state_id == effect_state
                }
                _ => false,
            }
        }

        fn apply_attested_effects(
            game_id: GameId,
            instance_id: InstanceId,
            effects: &BoundedVec<AttestedEffect<T>, T::MaxAttestedEffectsPerEvent>,
        ) -> DispatchResult {
            for effect in effects {
                Self::apply_attested_effect(game_id, instance_id, effect)?;
            }
            Ok(())
        }

        fn apply_attested_effect(
            game_id: GameId,
            instance_id: InstanceId,
            effect: &AttestedEffect<T>,
        ) -> DispatchResult {
            match effect {
                AttestedEffect::UpdatePassportCounter {
                    account,
                    field_id,
                    amount,
                } => T::ProfileProvider::update_passport_counter(account, *field_id, *amount),
                AttestedEffect::GrantPassportBadge { account, badge_id } => {
                    T::ProfileProvider::grant_passport_badge(account, *badge_id)
                }
                AttestedEffect::RevokePassportBadge { account, badge_id } => {
                    T::ProfileProvider::revoke_passport_badge(account, *badge_id)
                }
                AttestedEffect::SetMachineState {
                    scope,
                    machine_id,
                    state_id,
                } => {
                    MachineStates::<T>::insert(
                        (game_id, instance_id, *scope, *machine_id),
                        *state_id,
                    );
                    Ok(())
                }
            }
        }

        fn apply_effect(
            account: &T::AccountId,
            game_id: GameId,
            instance_id: InstanceId,
            action_actor: ActorId,
            effect: &Effect,
        ) -> DispatchResult {
            match effect {
                Effect::SetVar(variable_ref, value) => {
                    Self::ensure_effect_scope_allowed(variable_ref.scope, action_actor)?;
                    VariableValues::<T>::insert(
                        (
                            game_id,
                            Self::storage_instance_for_scope(instance_id, variable_ref.scope),
                            variable_ref.scope,
                            variable_ref.variable_id,
                        ),
                        *value,
                    );
                    Ok(())
                }
                Effect::IncVar(variable_ref, amount) => {
                    Self::ensure_effect_scope_allowed(variable_ref.scope, action_actor)?;
                    let current =
                        Self::value(game_id, instance_id, variable_ref).unwrap_or(Value::U64(0));
                    VariableValues::<T>::insert(
                        (
                            game_id,
                            Self::storage_instance_for_scope(instance_id, variable_ref.scope),
                            variable_ref.scope,
                            variable_ref.variable_id,
                        ),
                        current.checked_add::<T>(*amount)?,
                    );
                    Ok(())
                }
                Effect::DecVar(variable_ref, amount) => {
                    Self::ensure_effect_scope_allowed(variable_ref.scope, action_actor)?;
                    let current =
                        Self::value(game_id, instance_id, variable_ref).unwrap_or(Value::U64(0));
                    VariableValues::<T>::insert(
                        (
                            game_id,
                            Self::storage_instance_for_scope(instance_id, variable_ref.scope),
                            variable_ref.scope,
                            variable_ref.variable_id,
                        ),
                        current.checked_sub::<T>(*amount)?,
                    );
                    Ok(())
                }
                Effect::GrantItem {
                    actor_id,
                    item_id,
                    amount,
                } => {
                    Self::ensure_effect_actor(*actor_id, action_actor)?;
                    Inventory::<T>::try_mutate(
                        (game_id, instance_id, *actor_id, *item_id),
                        |balance| -> DispatchResult {
                            *balance = (*balance)
                                .checked_add(*amount)
                                .ok_or(Error::<T>::ArithmeticOverflow)?;
                            Ok(())
                        },
                    )
                }
                Effect::ConsumeItem {
                    actor_id,
                    item_id,
                    amount,
                } => {
                    Self::ensure_effect_actor(*actor_id, action_actor)?;
                    Inventory::<T>::try_mutate(
                        (game_id, instance_id, *actor_id, *item_id),
                        |balance| -> DispatchResult {
                            ensure!(*balance >= *amount, Error::<T>::InsufficientItem);
                            *balance = (*balance)
                                .checked_sub(*amount)
                                .ok_or(Error::<T>::Underflow)?;
                            Ok(())
                        },
                    )
                }
                Effect::GrantCredit {
                    actor_id,
                    credit_type,
                    amount,
                } => {
                    Self::ensure_effect_actor(*actor_id, action_actor)?;
                    T::EconomyProvider::grant_credit(account, game_id, *credit_type, *amount)
                }
                Effect::GrantEntitlement {
                    actor_id,
                    entitlement_id,
                } => {
                    Self::ensure_effect_actor(*actor_id, action_actor)?;
                    T::EconomyProvider::grant_entitlement(account, game_id, *entitlement_id)
                }
                Effect::RevokeEntitlement {
                    actor_id,
                    entitlement_id,
                } => {
                    Self::ensure_effect_actor(*actor_id, action_actor)?;
                    T::EconomyProvider::revoke_entitlement(account, game_id, *entitlement_id)
                }
                Effect::UpdatePassportCounter {
                    actor_id,
                    field_id,
                    amount,
                } => {
                    Self::ensure_effect_actor(*actor_id, action_actor)?;
                    T::ProfileProvider::update_passport_counter(account, *field_id, *amount)
                }
                Effect::GrantPassportBadge { actor_id, badge_id } => {
                    Self::ensure_effect_actor(*actor_id, action_actor)?;
                    T::ProfileProvider::grant_passport_badge(account, *badge_id)
                }
                Effect::RevokePassportBadge { actor_id, badge_id } => {
                    Self::ensure_effect_actor(*actor_id, action_actor)?;
                    T::ProfileProvider::revoke_passport_badge(account, *badge_id)
                }
                Effect::SetMachineState {
                    scope,
                    machine_id,
                    state_id,
                } => {
                    Self::ensure_effect_scope_allowed(*scope, action_actor)?;
                    MachineStates::<T>::insert(
                        (game_id, instance_id, *scope, *machine_id),
                        *state_id,
                    );
                    Ok(())
                }
            }
        }

        fn ensure_effect_scope_allowed(scope: Scope, action_actor: ActorId) -> DispatchResult {
            match scope {
                Scope::Actor(actor_id) | Scope::Passport(actor_id) => {
                    Self::ensure_effect_actor(actor_id, action_actor)
                }
                Scope::Game | Scope::Instance | Scope::Entity(_) => Ok(()),
            }
        }

        fn ensure_effect_actor(actor_id: ActorId, action_actor: ActorId) -> DispatchResult {
            ensure!(actor_id == action_actor, Error::<T>::InvalidEffectActor);
            Ok(())
        }
    }
}

#[cfg(test)]
mod wire_contract_tests {
    use super::*;
    use codec::Encode;

    fn discriminant<T: Encode>(value: T) -> u8 {
        value.encode()[0]
    }

    #[test]
    fn manifest_v0_enum_discriminants_are_locked() {
        let variable = VariableRef {
            scope: Scope::Actor(42),
            variable_id: 7,
        };

        assert_eq!(discriminant(Value::Bool(false)), 0);
        assert_eq!(discriminant(Value::U64(0)), 1);
        assert_eq!(discriminant(Value::I64(0)), 2);
        assert_eq!(discriminant(Value::Enum(0)), 3);

        assert_eq!(
            discriminant(ConditionAtom::VarEquals(variable, Value::Bool(false))),
            0
        );
        assert_eq!(
            discriminant(ConditionAtom::VarGreaterOrEqual(variable, 1)),
            1
        );
        assert_eq!(discriminant(ConditionAtom::VarLessOrEqual(variable, 1)), 2);
        assert_eq!(
            discriminant(ConditionAtom::HasItem {
                actor_id: 1,
                item_id: 1,
                amount: 1,
            }),
            3
        );
        assert_eq!(
            discriminant(ConditionAtom::HasCredit {
                actor_id: 1,
                credit_type: 1,
                amount: 1,
            }),
            4
        );
        assert_eq!(
            discriminant(ConditionAtom::HasEntitlement {
                actor_id: 1,
                entitlement_id: 1,
            }),
            5
        );
        assert_eq!(
            discriminant(ConditionAtom::MachineStateEquals {
                scope: Scope::Instance,
                machine_id: 1,
                state_id: 1,
            }),
            6
        );

        assert_eq!(discriminant(EconomyGateAtom::Free), 0);
        assert_eq!(
            discriminant(EconomyGateAtom::DeveloperSponsored { amount: 1 }),
            1
        );
        assert_eq!(
            discriminant(EconomyGateAtom::RequiresPayment { amount: 1 }),
            2
        );
        assert_eq!(
            discriminant(EconomyGateAtom::RequiresEntitlement { entitlement_id: 1 }),
            3
        );
        assert_eq!(
            discriminant(EconomyGateAtom::ConsumesCredit {
                credit_type: 1,
                amount: 1,
            }),
            4
        );

        let effects = [
            Effect::SetVar(variable, Value::Bool(false)),
            Effect::IncVar(variable, 1),
            Effect::DecVar(variable, 1),
            Effect::GrantItem {
                actor_id: 1,
                item_id: 1,
                amount: 1,
            },
            Effect::ConsumeItem {
                actor_id: 1,
                item_id: 1,
                amount: 1,
            },
            Effect::GrantCredit {
                actor_id: 1,
                credit_type: 1,
                amount: 1,
            },
            Effect::GrantEntitlement {
                actor_id: 1,
                entitlement_id: 1,
            },
            Effect::RevokeEntitlement {
                actor_id: 1,
                entitlement_id: 1,
            },
            Effect::UpdatePassportCounter {
                actor_id: 1,
                field_id: 1,
                amount: 1,
            },
            Effect::GrantPassportBadge {
                actor_id: 1,
                badge_id: 1,
            },
            Effect::RevokePassportBadge {
                actor_id: 1,
                badge_id: 1,
            },
            Effect::SetMachineState {
                scope: Scope::Instance,
                machine_id: 1,
                state_id: 1,
            },
        ];
        for (expected, effect) in effects.into_iter().enumerate() {
            assert_eq!(discriminant(effect), expected as u8);
        }

        assert_eq!(
            discriminant(AttestedEffectPolicy::UpdatePassportCounter {
                field_id: 1,
                amount: 1,
            }),
            0
        );
        assert_eq!(
            discriminant(AttestedEffectPolicy::GrantPassportBadge { badge_id: 1 }),
            1
        );
        assert_eq!(
            discriminant(AttestedEffectPolicy::RevokePassportBadge { badge_id: 1 }),
            2
        );
        assert_eq!(
            discriminant(AttestedEffectPolicy::SetMachineState {
                scope: Scope::Instance,
                machine_id: 1,
                state_id: 1,
            }),
            3
        );
    }
}
