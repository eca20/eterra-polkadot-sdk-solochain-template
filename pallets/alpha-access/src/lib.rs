#![cfg_attr(not(feature = "std"), no_std)]
// FRAME's generated pallet glue currently triggers this lint in macro expansion.
#![allow(clippy::manual_inspect)]
#![allow(clippy::duplicated_attributes)]

pub use pallet::*;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use frame_support::{dispatch::DispatchResult, pallet_prelude::*, traits::UnixTime};
use frame_system::pallet_prelude::*;
use parity_scale_codec::DecodeWithMemTracking;

pub trait AccessControl<AccountId> {
    fn ensure_whitelisted(account: &AccountId) -> DispatchResult;
}

impl<AccountId> AccessControl<AccountId> for () {
    fn ensure_whitelisted(_: &AccountId) -> DispatchResult {
        Ok(())
    }
}

#[derive(
    Clone,
    Copy,
    Decode,
    DecodeWithMemTracking,
    Encode,
    MaxEncodedLen,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
)]
pub enum AccessSourceKind {
    ContractEventRelayer,
    XcmMessage,
    ManualAdmin,
}

#[derive(
    Clone,
    Copy,
    Decode,
    DecodeWithMemTracking,
    Encode,
    MaxEncodedLen,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
)]
pub enum GateMode {
    Enforced,
    Open,
}

impl Default for GateMode {
    fn default() -> Self {
        Self::Enforced
    }
}

#[derive(
    Clone,
    Decode,
    DecodeWithMemTracking,
    Encode,
    MaxEncodedLen,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
)]
pub struct AccessSource<Hash> {
    pub source_kind: AccessSourceKind,
    pub source_chain_id: u64,
    pub source_contract: [u8; 20],
    pub source_event_id: Hash,
    pub source_tx_hash: Option<[u8; 32]>,
    pub source_log_index: Option<u32>,
    pub token_id: u128,
}

#[derive(
    Clone,
    Decode,
    DecodeWithMemTracking,
    Encode,
    MaxEncodedLen,
    PartialEq,
    Eq,
    RuntimeDebug,
    TypeInfo,
)]
pub struct AccessGrant<Hash, BlockNumber> {
    pub source_kind: AccessSourceKind,
    pub source_chain_id: u64,
    pub source_contract: [u8; 20],
    pub source_event_id: Hash,
    pub source_tx_hash: Option<[u8; 32]>,
    pub source_log_index: Option<u32>,
    pub token_id: u128,
    pub expires_at_unix: u64,
    pub granted_at_block: BlockNumber,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        type TimeProvider: UnixTime;
        #[pallet::constant]
        type MaxRevokeReasonLen: Get<u32>;
        type WeightInfo: WeightInfo;
    }

    pub type SourceOf<T> = AccessSource<<T as frame_system::Config>::Hash>;
    pub type GrantOf<T> = AccessGrant<<T as frame_system::Config>::Hash, BlockNumberFor<T>>;

    #[pallet::storage]
    #[pallet::getter(fn whitelist)]
    pub type Whitelist<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, GrantOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn processed_sources)]
    pub type ProcessedSources<T: Config> =
        StorageMap<_, Blake2_128Concat, T::Hash, (), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn managers)]
    pub type Managers<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, (), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn allowed_sources)]
    pub type AllowedSources<T: Config> =
        StorageMap<_, Blake2_128Concat, (AccessSourceKind, u64, [u8; 20]), (), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn access_mode)]
    pub type AccessMode<T: Config> = StorageValue<_, GateMode, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        AccessGranted {
            account: T::AccountId,
            source_event_id: T::Hash,
            source_kind: AccessSourceKind,
            source_chain_id: u64,
            source_contract: [u8; 20],
            token_id: u128,
            expires_at_unix: u64,
        },
        AccessRevoked {
            account: T::AccountId,
            reason: BoundedVec<u8, T::MaxRevokeReasonLen>,
        },
        ManagerSet {
            account: T::AccountId,
            enabled: bool,
        },
        AllowedSourceSet {
            source_kind: AccessSourceKind,
            source_chain_id: u64,
            source_contract: [u8; 20],
            enabled: bool,
        },
        AccessModeSet {
            mode: GateMode,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        NotAuthorized,
        SourceNotAllowed,
        SourceAlreadyProcessed,
        InvalidSource,
        NotWhitelisted,
        Expired,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::grant_access())]
        pub fn grant_access(
            origin: OriginFor<T>,
            account: T::AccountId,
            source: SourceOf<T>,
            expires_at_unix: u64,
        ) -> DispatchResult {
            let is_admin = T::AdminOrigin::ensure_origin(origin.clone()).is_ok();
            if !is_admin {
                let manager = ensure_signed(origin)?;
                ensure!(
                    Managers::<T>::contains_key(&manager),
                    Error::<T>::NotAuthorized
                );
                ensure!(
                    source.source_kind == AccessSourceKind::ContractEventRelayer,
                    Error::<T>::InvalidSource
                );
            }

            ensure!(
                AllowedSources::<T>::contains_key((
                    source.source_kind,
                    source.source_chain_id,
                    source.source_contract
                )),
                Error::<T>::SourceNotAllowed
            );
            ensure!(
                !ProcessedSources::<T>::contains_key(source.source_event_id),
                Error::<T>::SourceAlreadyProcessed
            );

            let grant = AccessGrant {
                source_kind: source.source_kind,
                source_chain_id: source.source_chain_id,
                source_contract: source.source_contract,
                source_event_id: source.source_event_id,
                source_tx_hash: source.source_tx_hash,
                source_log_index: source.source_log_index,
                token_id: source.token_id,
                expires_at_unix,
                granted_at_block: frame_system::Pallet::<T>::block_number(),
            };

            Whitelist::<T>::insert(&account, grant);
            ProcessedSources::<T>::insert(source.source_event_id, ());

            Self::deposit_event(Event::AccessGranted {
                account,
                source_event_id: source.source_event_id,
                source_kind: source.source_kind,
                source_chain_id: source.source_chain_id,
                source_contract: source.source_contract,
                token_id: source.token_id,
                expires_at_unix,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::revoke_access())]
        pub fn revoke_access(
            origin: OriginFor<T>,
            account: T::AccountId,
            reason: BoundedVec<u8, T::MaxRevokeReasonLen>,
        ) -> DispatchResult {
            Self::ensure_manager_or_admin(origin)?;
            Whitelist::<T>::remove(&account);
            Self::deposit_event(Event::AccessRevoked { account, reason });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::set_manager())]
        pub fn set_manager(
            origin: OriginFor<T>,
            manager: T::AccountId,
            enabled: bool,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            if enabled {
                Managers::<T>::insert(&manager, ());
            } else {
                Managers::<T>::remove(&manager);
            }
            Self::deposit_event(Event::ManagerSet {
                account: manager,
                enabled,
            });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::set_allowed_source())]
        pub fn set_allowed_source(
            origin: OriginFor<T>,
            source_kind: AccessSourceKind,
            source_chain_id: u64,
            source_contract: [u8; 20],
            enabled: bool,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            let key = (source_kind, source_chain_id, source_contract);
            if enabled {
                AllowedSources::<T>::insert(key, ());
            } else {
                AllowedSources::<T>::remove(key);
            }
            Self::deposit_event(Event::AllowedSourceSet {
                source_kind,
                source_chain_id,
                source_contract,
                enabled,
            });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::set_access_mode())]
        pub fn set_access_mode(origin: OriginFor<T>, mode: GateMode) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            AccessMode::<T>::put(mode);
            Self::deposit_event(Event::AccessModeSet { mode });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn is_whitelisted(account: &T::AccountId) -> bool {
            if AccessMode::<T>::get() == GateMode::Open {
                return true;
            }
            match Whitelist::<T>::get(account) {
                Some(grant) => {
                    grant.expires_at_unix == 0
                        || T::TimeProvider::now().as_secs() < grant.expires_at_unix
                }
                None => false,
            }
        }

        pub fn ensure_whitelisted(account: &T::AccountId) -> DispatchResult {
            if AccessMode::<T>::get() == GateMode::Open {
                return Ok(());
            }
            let grant = Whitelist::<T>::get(account).ok_or(Error::<T>::NotWhitelisted)?;
            ensure!(
                grant.expires_at_unix == 0
                    || T::TimeProvider::now().as_secs() < grant.expires_at_unix,
                Error::<T>::Expired
            );
            Ok(())
        }

        fn ensure_manager_or_admin(origin: OriginFor<T>) -> DispatchResult {
            if T::AdminOrigin::ensure_origin(origin.clone()).is_ok() {
                return Ok(());
            }
            let manager = ensure_signed(origin)?;
            ensure!(
                Managers::<T>::contains_key(manager),
                Error::<T>::NotAuthorized
            );
            Ok(())
        }
    }

    impl<T: Config> AccessControl<T::AccountId> for Pallet<T> {
        fn ensure_whitelisted(account: &T::AccountId) -> DispatchResult {
            Self::ensure_whitelisted(account)
        }
    }
}
