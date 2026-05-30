//! Eterra Profile pallet MVP scaffold.
//!
//! Purpose: cross-game passport counters, badges, public facts, and capability
//! grants that allow one game to read approved state exposed by another.
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
    use super::weights::WeightInfo;
    use frame_support::{dispatch::DispatchResult, pallet_prelude::*, traits::StorageVersion};
    use frame_system::pallet_prelude::*;

    pub type GameId = u64;
    pub type BadgeId = u32;
    pub type CounterId = u32;
    pub type PublicFactId = u32;
    pub type CapabilityId = u64;

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum CapabilityPermission {
        Read,
        Write,
        ReadWrite,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum PublicValue {
        Bool(bool),
        U64(u64),
        Enum(u32),
        Hash([u8; 32]),
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct CapabilityRecord<T: Config> {
        pub source_game_id: GameId,
        pub target_game_id: GameId,
        pub fact_id: PublicFactId,
        pub permission: CapabilityPermission,
        pub expires_at: Option<BlockNumberFor<T>>,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type PassportCounters<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        CounterId,
        u64,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type Badges<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        BadgeId,
        bool,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type PublicFacts<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, T::AccountId>,
            NMapKey<Blake2_128Concat, PublicFactId>,
        ),
        PublicValue,
    >;

    #[pallet::storage]
    pub type Capabilities<T: Config> =
        StorageMap<_, Blake2_128Concat, CapabilityId, CapabilityRecord<T>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        CounterIncremented {
            account: T::AccountId,
            counter_id: CounterId,
            amount: u64,
        },
        BadgeGranted {
            account: T::AccountId,
            badge_id: BadgeId,
        },
        BadgeRevoked {
            account: T::AccountId,
            badge_id: BadgeId,
        },
        PublicFactSet {
            game_id: GameId,
            account: T::AccountId,
            fact_id: PublicFactId,
        },
        CapabilityGranted {
            capability_id: CapabilityId,
            source_game_id: GameId,
            target_game_id: GameId,
        },
        CapabilityRevoked {
            capability_id: CapabilityId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        ArithmeticOverflow,
        CapabilityAlreadyExists,
        CapabilityNotFound,
        CapabilityExpired,
        CapabilityNotAllowed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::increment_counter())]
        pub fn increment_counter(
            origin: OriginFor<T>,
            account: T::AccountId,
            counter_id: CounterId,
            amount: u64,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::try_increment_counter(&account, counter_id, amount)
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::grant_badge())]
        pub fn grant_badge(
            origin: OriginFor<T>,
            account: T::AccountId,
            badge_id: BadgeId,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::try_grant_badge(&account, badge_id)
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::revoke_badge())]
        pub fn revoke_badge(
            origin: OriginFor<T>,
            account: T::AccountId,
            badge_id: BadgeId,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::try_revoke_badge(&account, badge_id)
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::set_public_fact())]
        pub fn set_public_fact(
            origin: OriginFor<T>,
            game_id: GameId,
            account: T::AccountId,
            fact_id: PublicFactId,
            value: PublicValue,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::try_set_public_fact(game_id, &account, fact_id, value)
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::grant_capability())]
        pub fn grant_capability(
            origin: OriginFor<T>,
            capability_id: CapabilityId,
            source_game_id: GameId,
            target_game_id: GameId,
            fact_id: PublicFactId,
            permission: CapabilityPermission,
            expires_at: Option<BlockNumberFor<T>>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !Capabilities::<T>::contains_key(capability_id),
                Error::<T>::CapabilityAlreadyExists
            );
            Capabilities::<T>::insert(
                capability_id,
                CapabilityRecord::<T> {
                    source_game_id,
                    target_game_id,
                    fact_id,
                    permission,
                    expires_at,
                },
            );
            Self::deposit_event(Event::CapabilityGranted {
                capability_id,
                source_game_id,
                target_game_id,
            });
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::revoke_capability())]
        pub fn revoke_capability(
            origin: OriginFor<T>,
            capability_id: CapabilityId,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                Capabilities::<T>::contains_key(capability_id),
                Error::<T>::CapabilityNotFound
            );
            Capabilities::<T>::remove(capability_id);
            Self::deposit_event(Event::CapabilityRevoked { capability_id });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn has_badge(account: &T::AccountId, badge_id: BadgeId) -> bool {
            Badges::<T>::get(account, badge_id)
        }

        pub fn counter(account: &T::AccountId, counter_id: CounterId) -> u64 {
            PassportCounters::<T>::get(account, counter_id)
        }

        pub fn public_fact(
            game_id: GameId,
            account: &T::AccountId,
            fact_id: PublicFactId,
        ) -> Option<PublicValue> {
            PublicFacts::<T>::get((game_id, account, fact_id))
        }

        pub fn try_increment_counter(
            account: &T::AccountId,
            counter_id: CounterId,
            amount: u64,
        ) -> DispatchResult {
            PassportCounters::<T>::try_mutate(
                account.clone(),
                counter_id,
                |value| -> DispatchResult {
                    *value = value
                        .checked_add(amount)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            Self::deposit_event(Event::CounterIncremented {
                account: account.clone(),
                counter_id,
                amount,
            });
            Ok(())
        }

        pub fn try_grant_badge(account: &T::AccountId, badge_id: BadgeId) -> DispatchResult {
            Badges::<T>::insert(account.clone(), badge_id, true);
            Self::deposit_event(Event::BadgeGranted {
                account: account.clone(),
                badge_id,
            });
            Ok(())
        }

        pub fn try_revoke_badge(account: &T::AccountId, badge_id: BadgeId) -> DispatchResult {
            Badges::<T>::remove(account.clone(), badge_id);
            Self::deposit_event(Event::BadgeRevoked {
                account: account.clone(),
                badge_id,
            });
            Ok(())
        }

        pub fn try_set_public_fact(
            game_id: GameId,
            account: &T::AccountId,
            fact_id: PublicFactId,
            value: PublicValue,
        ) -> DispatchResult {
            PublicFacts::<T>::insert((game_id, account.clone(), fact_id), value);
            Self::deposit_event(Event::PublicFactSet {
                game_id,
                account: account.clone(),
                fact_id,
            });
            Ok(())
        }

        pub fn try_set_public_fact_with_capability(
            capability_id: CapabilityId,
            source_game_id: GameId,
            target_game_id: GameId,
            account: &T::AccountId,
            fact_id: PublicFactId,
            value: PublicValue,
        ) -> DispatchResult {
            Self::ensure_capability(capability_id, source_game_id, target_game_id, fact_id, true)?;
            Self::try_set_public_fact(source_game_id, account, fact_id, value)
        }

        pub fn can_read(
            capability_id: CapabilityId,
            source_game_id: GameId,
            target_game_id: GameId,
            fact_id: PublicFactId,
        ) -> bool {
            let Some(record) = Capabilities::<T>::get(capability_id) else {
                return false;
            };
            if record.source_game_id != source_game_id
                || record.target_game_id != target_game_id
                || record.fact_id != fact_id
            {
                return false;
            }
            if let Some(expires_at) = record.expires_at {
                if frame_system::Pallet::<T>::block_number() >= expires_at {
                    return false;
                }
            }
            matches!(
                record.permission,
                CapabilityPermission::Read | CapabilityPermission::ReadWrite
            )
        }

        pub fn can_write(
            capability_id: CapabilityId,
            source_game_id: GameId,
            target_game_id: GameId,
            fact_id: PublicFactId,
        ) -> bool {
            let Some(record) = Capabilities::<T>::get(capability_id) else {
                return false;
            };
            if record.source_game_id != source_game_id
                || record.target_game_id != target_game_id
                || record.fact_id != fact_id
            {
                return false;
            }
            if let Some(expires_at) = record.expires_at {
                if frame_system::Pallet::<T>::block_number() >= expires_at {
                    return false;
                }
            }
            matches!(
                record.permission,
                CapabilityPermission::Write | CapabilityPermission::ReadWrite
            )
        }

        fn ensure_capability(
            capability_id: CapabilityId,
            source_game_id: GameId,
            target_game_id: GameId,
            fact_id: PublicFactId,
            write: bool,
        ) -> DispatchResult {
            let record =
                Capabilities::<T>::get(capability_id).ok_or(Error::<T>::CapabilityNotFound)?;
            ensure!(
                record.source_game_id == source_game_id
                    && record.target_game_id == target_game_id
                    && record.fact_id == fact_id,
                Error::<T>::CapabilityNotAllowed
            );
            if let Some(expires_at) = record.expires_at {
                ensure!(
                    frame_system::Pallet::<T>::block_number() < expires_at,
                    Error::<T>::CapabilityExpired
                );
            }
            let allowed = if write {
                matches!(
                    record.permission,
                    CapabilityPermission::Write | CapabilityPermission::ReadWrite
                )
            } else {
                matches!(
                    record.permission,
                    CapabilityPermission::Read | CapabilityPermission::ReadWrite
                )
            };
            ensure!(allowed, Error::<T>::CapabilityNotAllowed);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_support::{
        assert_noop, assert_ok, construct_runtime, parameter_types,
        traits::{ConstU32, Everything, GetStorageVersion, StorageVersion},
    };
    use frame_system as system;
    use sp_core::H256;
    use sp_runtime::{
        traits::{BlakeTwo256, IdentityLookup},
        BuildStorage,
    };

    type AccountId = u64;
    type Block = system::mocking::MockBlock<Test>;

    construct_runtime!(
        pub enum Test {
            System: system,
            EterraProfile: crate,
        }
    );

    parameter_types! {
        pub const BlockHashCount: u64 = 250;
    }

    impl system::Config for Test {
        type BaseCallFilter = Everything;
        type BlockWeights = ();
        type BlockLength = ();
        type DbWeight = ();
        type RuntimeOrigin = RuntimeOrigin;
        type RuntimeCall = RuntimeCall;
        type RuntimeEvent = RuntimeEvent;
        type Block = Block;
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = AccountId;
        type Lookup = IdentityLookup<Self::AccountId>;
        type BlockHashCount = BlockHashCount;
        type Version = ();
        type PalletInfo = PalletInfo;
        type AccountData = ();
        type OnNewAccount = ();
        type OnKilledAccount = ();
        type SystemWeightInfo = ();
        type SS58Prefix = ();
        type OnSetCode = ();
        type MaxConsumers = ConstU32<16>;
        type Nonce = u64;
        type SingleBlockMigrations = ();
        type MultiBlockMigrator = ();
        type PreInherents = ();
        type PostInherents = ();
        type PostTransactions = ();
        type RuntimeTask = ();
    }

    impl Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type WeightInfo = ();
        type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    }

    pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
        let storage = system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("frame-system storage build should not fail");
        let mut ext = sp_io::TestExternalities::new(storage);
        ext.execute_with(|| System::set_block_number(1));
        ext
    }

    #[test]
    fn counters_and_badges_use_checked_storage() {
        new_test_ext().execute_with(|| {
            PassportCounters::<Test>::insert(42, 7, u64::MAX);
            assert_noop!(
                EterraProfile::try_increment_counter(&42, 7, 1),
                Error::<Test>::ArithmeticOverflow
            );

            assert_ok!(EterraProfile::try_grant_badge(&42, 9));
            assert!(EterraProfile::has_badge(&42, 9));
            assert_ok!(EterraProfile::try_revoke_badge(&42, 9));
            assert!(!EterraProfile::has_badge(&42, 9));
        });
    }

    #[test]
    fn capability_read_write_defaults_deny_and_grants_allow() {
        new_test_ext().execute_with(|| {
            assert!(!EterraProfile::can_read(1, 10, 20, 7));
            assert!(!EterraProfile::can_write(1, 10, 20, 7));

            assert_ok!(EterraProfile::grant_capability(
                RuntimeOrigin::root(),
                1,
                10,
                20,
                7,
                CapabilityPermission::Read,
                None,
            ));
            assert!(EterraProfile::can_read(1, 10, 20, 7));
            assert!(!EterraProfile::can_write(1, 10, 20, 7));
            assert_noop!(
                EterraProfile::try_set_public_fact_with_capability(
                    1,
                    10,
                    20,
                    &42,
                    7,
                    PublicValue::U64(3),
                ),
                Error::<Test>::CapabilityNotAllowed
            );

            assert_ok!(EterraProfile::grant_capability(
                RuntimeOrigin::root(),
                2,
                10,
                20,
                7,
                CapabilityPermission::ReadWrite,
                Some(3),
            ));
            assert_ok!(EterraProfile::try_set_public_fact_with_capability(
                2,
                10,
                20,
                &42,
                7,
                PublicValue::U64(3),
            ));
            assert_eq!(
                EterraProfile::public_fact(10, &42, 7),
                Some(PublicValue::U64(3))
            );

            System::set_block_number(3);
            assert!(!EterraProfile::can_read(2, 10, 20, 7));
            assert_noop!(
                EterraProfile::try_set_public_fact_with_capability(
                    2,
                    10,
                    20,
                    &42,
                    7,
                    PublicValue::U64(4),
                ),
                Error::<Test>::CapabilityExpired
            );
        });
    }

    #[test]
    fn storage_version_is_declared_without_migration_requirement() {
        new_test_ext().execute_with(|| {
            assert_eq!(
                Pallet::<Test>::in_code_storage_version(),
                StorageVersion::new(1)
            );
        });
    }

    #[test]
    fn dispatchables_emit_events_and_capability_lifecycle_is_checked() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraProfile::increment_counter(
                RuntimeOrigin::root(),
                42,
                7,
                2,
            ));
            System::assert_last_event(RuntimeEvent::EterraProfile(Event::CounterIncremented {
                account: 42,
                counter_id: 7,
                amount: 2,
            }));

            assert_ok!(EterraProfile::grant_badge(RuntimeOrigin::root(), 42, 9));
            assert_ok!(EterraProfile::grant_badge(RuntimeOrigin::root(), 42, 9));
            assert!(EterraProfile::has_badge(&42, 9));
            assert_ok!(EterraProfile::revoke_badge(RuntimeOrigin::root(), 42, 9));
            assert_ok!(EterraProfile::revoke_badge(RuntimeOrigin::root(), 42, 9));
            assert!(!EterraProfile::has_badge(&42, 9));

            assert_ok!(EterraProfile::set_public_fact(
                RuntimeOrigin::root(),
                10,
                42,
                3,
                PublicValue::Bool(true),
            ));
            assert_eq!(
                EterraProfile::public_fact(10, &42, 3),
                Some(PublicValue::Bool(true))
            );

            assert_ok!(EterraProfile::grant_capability(
                RuntimeOrigin::root(),
                1,
                10,
                20,
                3,
                CapabilityPermission::Write,
                None,
            ));
            assert_noop!(
                EterraProfile::grant_capability(
                    RuntimeOrigin::root(),
                    1,
                    10,
                    20,
                    3,
                    CapabilityPermission::Write,
                    None,
                ),
                Error::<Test>::CapabilityAlreadyExists
            );
            assert!(!EterraProfile::can_read(1, 10, 20, 3));
            assert!(EterraProfile::can_write(1, 10, 20, 3));
            assert_ok!(EterraProfile::try_set_public_fact_with_capability(
                1,
                10,
                20,
                &42,
                3,
                PublicValue::U64(4),
            ));
            assert_noop!(
                EterraProfile::revoke_capability(RuntimeOrigin::root(), 999),
                Error::<Test>::CapabilityNotFound
            );
            assert_ok!(EterraProfile::revoke_capability(RuntimeOrigin::root(), 1));
            assert!(!EterraProfile::can_write(1, 10, 20, 3));
        });
    }
}
