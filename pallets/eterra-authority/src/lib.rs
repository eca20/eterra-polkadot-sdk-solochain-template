//! Eterra Authority pallet MVP scaffold.
//!
//! Purpose: register game servers, payment attestors, anti-cheat providers,
//! matchmakers, replay verifiers, and future committee authorities that are
//! allowed to submit bounded attested events into Eterra Flow.
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::too_many_arguments)]

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
    pub type VersionId = u32;
    pub type AuthorityId = u64;
    pub type EventTypeId = u32;

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum AuthorityKind {
        GameServer,
        Matchmaker,
        PaymentAttestor,
        AntiCheat,
        ReplayVerifier,
        TournamentAdmin,
        CommitteeMember,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum AuthorityStatus {
        Active,
        Suspended,
        Revoked,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct AuthorityRecord<T: Config> {
        pub account: T::AccountId,
        pub kind: AuthorityKind,
        pub status: AuthorityStatus,
        pub version_id: Option<VersionId>,
        pub allowed_events: BoundedVec<EventTypeId, T::MaxAllowedEventsPerAuthority>,
        pub expires_at: Option<BlockNumberFor<T>>,
        pub metadata_hash: T::Hash,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        #[pallet::constant]
        type MaxAllowedEventsPerAuthority: Get<u32>;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type Authorities<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GameId,
        Blake2_128Concat,
        AuthorityId,
        AuthorityRecord<T>,
    >;

    #[pallet::storage]
    pub type AuthorityByAccount<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, GameId, Blake2_128Concat, T::AccountId, AuthorityId>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        AuthorityAuthorized {
            game_id: GameId,
            authority_id: AuthorityId,
            account: T::AccountId,
        },
        AuthorityStatusChanged {
            game_id: GameId,
            authority_id: AuthorityId,
            status: AuthorityStatus,
        },
        AuthorityRevoked {
            game_id: GameId,
            authority_id: AuthorityId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        AuthorityAlreadyExists,
        AuthorityNotFound,
        EventListTooLarge,
        AuthorityExpired,
        AuthorityNotActive,
        EventNotAllowed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::authorize_authority())]
        pub fn authorize_authority(
            origin: OriginFor<T>,
            game_id: GameId,
            authority_id: AuthorityId,
            account: T::AccountId,
            kind: AuthorityKind,
            version_id: Option<VersionId>,
            allowed_events: BoundedVec<EventTypeId, T::MaxAllowedEventsPerAuthority>,
            expires_at: Option<BlockNumberFor<T>>,
            metadata_hash: T::Hash,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !Authorities::<T>::contains_key(game_id, authority_id),
                Error::<T>::AuthorityAlreadyExists
            );
            Authorities::<T>::insert(
                game_id,
                authority_id,
                AuthorityRecord::<T> {
                    account: account.clone(),
                    kind,
                    status: AuthorityStatus::Active,
                    version_id,
                    allowed_events,
                    expires_at,
                    metadata_hash,
                },
            );
            AuthorityByAccount::<T>::insert(game_id, account.clone(), authority_id);
            Self::deposit_event(Event::AuthorityAuthorized {
                game_id,
                authority_id,
                account,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::set_authority_status())]
        pub fn set_authority_status(
            origin: OriginFor<T>,
            game_id: GameId,
            authority_id: AuthorityId,
            status: AuthorityStatus,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Authorities::<T>::try_mutate(
                game_id,
                authority_id,
                |maybe_record| -> DispatchResult {
                    let record = maybe_record.as_mut().ok_or(Error::<T>::AuthorityNotFound)?;
                    record.status = status.clone();
                    Ok(())
                },
            )?;
            Self::deposit_event(Event::AuthorityStatusChanged {
                game_id,
                authority_id,
                status,
            });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::revoke_authority())]
        pub fn revoke_authority(
            origin: OriginFor<T>,
            game_id: GameId,
            authority_id: AuthorityId,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            let record = Authorities::<T>::take(game_id, authority_id)
                .ok_or(Error::<T>::AuthorityNotFound)?;
            AuthorityByAccount::<T>::remove(game_id, record.account);
            Self::deposit_event(Event::AuthorityRevoked {
                game_id,
                authority_id,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn can_submit(
            account: &T::AccountId,
            game_id: GameId,
            event_type: EventTypeId,
        ) -> bool {
            Self::resolve_authority(account, game_id, None, event_type).is_some()
        }

        pub fn resolve_authority(
            account: &T::AccountId,
            game_id: GameId,
            version_id: Option<VersionId>,
            event_type: EventTypeId,
        ) -> Option<AuthorityId> {
            let authority_id = AuthorityByAccount::<T>::get(game_id, account)?;
            let record = Authorities::<T>::get(game_id, authority_id)?;
            if record.status != AuthorityStatus::Active {
                return None;
            }
            if let Some(expires_at) = record.expires_at {
                if frame_system::Pallet::<T>::block_number() >= expires_at {
                    return None;
                }
            }
            if let Some(authority_version) = record.version_id {
                if version_id != Some(authority_version) {
                    return None;
                }
            }
            if !record.allowed_events.contains(&event_type) {
                return None;
            }
            Some(authority_id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_support::{
        assert_noop, assert_ok, construct_runtime, parameter_types,
        traits::{ConstU32, Everything, GetStorageVersion, StorageVersion},
        BoundedVec,
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
            EterraAuthority: crate,
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
        type MaxAllowedEventsPerAuthority = ConstU32<4>;
    }

    pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
        let storage = system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("frame-system storage build should not fail");
        let mut ext = sp_io::TestExternalities::new(storage);
        ext.execute_with(|| System::set_block_number(1));
        ext
    }

    fn allowed(events: &[EventTypeId]) -> BoundedVec<EventTypeId, ConstU32<4>> {
        BoundedVec::try_from(events.to_vec()).expect("test event list should fit")
    }

    #[test]
    fn resolve_authority_honors_version_and_event() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraAuthority::authorize_authority(
                RuntimeOrigin::root(),
                10,
                500,
                42,
                AuthorityKind::GameServer,
                Some(7),
                allowed(&[8]),
                None,
                H256::repeat_byte(1),
            ));

            assert_eq!(
                EterraAuthority::resolve_authority(&42, 10, Some(7), 8),
                Some(500)
            );
            assert_eq!(
                EterraAuthority::resolve_authority(&42, 10, Some(6), 8),
                None
            );
            assert_eq!(
                EterraAuthority::resolve_authority(&42, 10, Some(7), 9),
                None
            );
        });
    }

    #[test]
    fn suspended_revoked_and_expired_authorities_are_rejected() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraAuthority::authorize_authority(
                RuntimeOrigin::root(),
                10,
                500,
                42,
                AuthorityKind::GameServer,
                None,
                allowed(&[8]),
                Some(3),
                H256::repeat_byte(1),
            ));
            assert_eq!(
                EterraAuthority::resolve_authority(&42, 10, Some(1), 8),
                Some(500)
            );

            assert_ok!(EterraAuthority::set_authority_status(
                RuntimeOrigin::root(),
                10,
                500,
                AuthorityStatus::Suspended,
            ));
            assert_eq!(
                EterraAuthority::resolve_authority(&42, 10, Some(1), 8),
                None
            );

            assert_ok!(EterraAuthority::set_authority_status(
                RuntimeOrigin::root(),
                10,
                500,
                AuthorityStatus::Active,
            ));
            System::set_block_number(3);
            assert_eq!(
                EterraAuthority::resolve_authority(&42, 10, Some(1), 8),
                None
            );

            System::set_block_number(1);
            assert_ok!(EterraAuthority::revoke_authority(
                RuntimeOrigin::root(),
                10,
                500,
            ));
            assert_eq!(
                EterraAuthority::resolve_authority(&42, 10, Some(1), 8),
                None
            );
        });
    }

    #[test]
    fn storage_version_is_declared_without_migration_requirement() {
        new_test_ext().execute_with(|| {
            assert_eq!(
                Pallet::<Test>::in_code_storage_version(),
                StorageVersion::new(2)
            );
        });
    }

    #[test]
    fn duplicate_authority_and_revoke_cleanup_are_enforced() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraAuthority::authorize_authority(
                RuntimeOrigin::root(),
                10,
                500,
                42,
                AuthorityKind::GameServer,
                Some(7),
                allowed(&[8]),
                None,
                H256::repeat_byte(1),
            ));
            System::assert_last_event(RuntimeEvent::EterraAuthority(Event::AuthorityAuthorized {
                game_id: 10,
                authority_id: 500,
                account: 42,
            }));
            assert_noop!(
                EterraAuthority::authorize_authority(
                    RuntimeOrigin::root(),
                    10,
                    500,
                    99,
                    AuthorityKind::GameServer,
                    Some(7),
                    allowed(&[8]),
                    None,
                    H256::repeat_byte(1),
                ),
                Error::<Test>::AuthorityAlreadyExists
            );

            assert_ok!(EterraAuthority::set_authority_status(
                RuntimeOrigin::root(),
                10,
                500,
                AuthorityStatus::Revoked,
            ));
            assert_eq!(
                EterraAuthority::resolve_authority(&42, 10, Some(7), 8),
                None
            );
            assert_ok!(EterraAuthority::revoke_authority(
                RuntimeOrigin::root(),
                10,
                500,
            ));
            assert!(Authorities::<Test>::get(10, 500).is_none());
            assert_eq!(AuthorityByAccount::<Test>::get(10, 42), None);
            assert_noop!(
                EterraAuthority::set_authority_status(
                    RuntimeOrigin::root(),
                    10,
                    500,
                    AuthorityStatus::Active,
                ),
                Error::<Test>::AuthorityNotFound
            );
        });
    }
}
