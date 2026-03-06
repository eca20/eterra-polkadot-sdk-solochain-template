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

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::traits::Zero;
use sp_std::prelude::*;

pub type SeasonId = u32;

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum SeasonStatus {
    Draft,
    Active,
    Closed,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct SeasonInfo<BStrName, BStrDesc, BlockNumber> {
    pub name: BStrName,
    pub description: BStrDesc,
    pub status: SeasonStatus,
    pub created_at: BlockNumber,
    pub activated_at: Option<BlockNumber>,
    pub closed_at: Option<BlockNumber>,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::traits::{BuildGenesisConfig, StorageVersion};
    use frame_system::pallet_prelude::*;
    use sp_runtime::ArithmeticError;

    type BoundedStrName<T> = BoundedVec<u8, <T as Config>::MaxSeasonNameLen>;
    type BoundedStrDesc<T> = BoundedVec<u8, <T as Config>::MaxSeasonDescLen>;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The aggregated event type of the runtime.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Root/privileged origin that can manage the admin allowlist.
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Maximum name length.
        #[pallet::constant]
        type MaxSeasonNameLen: Get<u32>;

        /// Maximum description length.
        #[pallet::constant]
        type MaxSeasonDescLen: Get<u32>;

        /// Weight information for this pallet.
        type WeightInfo: WeightInfo;
    }

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub admins: Vec<T::AccountId>,
        pub initial_draft_season: Option<(Vec<u8>, Vec<u8>)>,
        pub initial_active_season: Option<(Vec<u8>, Vec<u8>)>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                admins: Vec::new(),
                initial_draft_season: None,
                initial_active_season: None,
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for admin in &self.admins {
                Admins::<T>::insert(admin, ());
            }

            // Start SeasonId at 1 so the first season created is "Season 1".
            let mut next_id: SeasonId = 1;

            if let Some((ref name, ref desc)) = self.initial_active_season {
                let bounded_name: BoundedStrName<T> = name.clone().try_into().unwrap_or_default();
                let bounded_desc: BoundedStrDesc<T> = desc.clone().try_into().unwrap_or_default();

                let season_id: SeasonId = 1;
                let info = SeasonInfo {
                    name: bounded_name,
                    description: bounded_desc,
                    status: SeasonStatus::Active,
                    created_at: Zero::zero(),
                    activated_at: Some(Zero::zero()),
                    closed_at: None,
                };

                Seasons::<T>::insert(season_id, info);
                ActiveSeasonId::<T>::put(Some(season_id));
                next_id = 2;
            } else if let Some((ref name, ref desc)) = self.initial_draft_season {
                let bounded_name: BoundedStrName<T> = name.clone().try_into().unwrap_or_default();
                let bounded_desc: BoundedStrDesc<T> = desc.clone().try_into().unwrap_or_default();

                let season_id: SeasonId = 1;
                let info = SeasonInfo {
                    name: bounded_name,
                    description: bounded_desc,
                    status: SeasonStatus::Draft,
                    created_at: Zero::zero(),
                    activated_at: None,
                    closed_at: None,
                };

                Seasons::<T>::insert(season_id, info);
                ActiveSeasonId::<T>::put(None::<SeasonId>);
                next_id = 2;
            }

            NextSeasonId::<T>::put(next_id);
        }
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let mut weight = T::DbWeight::get().reads(1);
            if StorageVersion::get::<Pallet<T>>() < STORAGE_VERSION {
                STORAGE_VERSION.put::<Pallet<T>>();
                weight = weight.saturating_add(T::DbWeight::get().writes(1));
            }
            weight
        }
    }

    #[pallet::storage]
    #[pallet::getter(fn admins)]
    pub type Admins<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, (), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_season_id)]
    pub type NextSeasonId<T: Config> = StorageValue<_, SeasonId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn active_season_id)]
    pub type ActiveSeasonId<T: Config> = StorageValue<_, Option<SeasonId>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn seasons)]
    pub type Seasons<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        SeasonId,
        SeasonInfo<BoundedStrName<T>, BoundedStrDesc<T>, BlockNumberFor<T>>,
        OptionQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        AdminAdded { account: T::AccountId },
        AdminRemoved { account: T::AccountId },
        SeasonCreated { season_id: SeasonId },
        SeasonActivated { season_id: SeasonId },
        SeasonClosed { season_id: SeasonId },
    }

    #[pallet::error]
    pub enum Error<T> {
        NotAdmin,
        UnknownSeason,
        AlreadyClosed,
        SeasonNotDraft,
        SeasonNotActive,
        NoActiveSeason,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::add_admin())]
        pub fn add_admin(origin: OriginFor<T>, account: T::AccountId) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Admins::<T>::insert(&account, ());
            Self::deposit_event(Event::AdminAdded { account });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::remove_admin())]
        pub fn remove_admin(origin: OriginFor<T>, account: T::AccountId) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Admins::<T>::remove(&account);
            Self::deposit_event(Event::AdminRemoved { account });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::create_season())]
        pub fn create_season(
            origin: OriginFor<T>,
            name: BoundedStrName<T>,
            description: BoundedStrDesc<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(Admins::<T>::contains_key(&who), Error::<T>::NotAdmin);

            let season_id = NextSeasonId::<T>::get();
            let next = season_id.checked_add(1).ok_or(ArithmeticError::Overflow)?;

            let now = <frame_system::Pallet<T>>::block_number();
            let info = SeasonInfo {
                name,
                description,
                status: SeasonStatus::Draft,
                created_at: now,
                activated_at: None,
                closed_at: None,
            };

            Seasons::<T>::insert(season_id, info);
            NextSeasonId::<T>::put(next);
            Self::deposit_event(Event::SeasonCreated { season_id });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::activate_season())]
        pub fn activate_season(origin: OriginFor<T>, season_id: SeasonId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(Admins::<T>::contains_key(&who), Error::<T>::NotAdmin);

            Seasons::<T>::try_mutate(season_id, |maybe_info| -> DispatchResult {
                let info = maybe_info.as_mut().ok_or(Error::<T>::UnknownSeason)?;

                match info.status {
                    SeasonStatus::Draft => {}
                    SeasonStatus::Active => return Err(Error::<T>::SeasonNotDraft.into()),
                    SeasonStatus::Closed => return Err(Error::<T>::AlreadyClosed.into()),
                }

                let now = <frame_system::Pallet<T>>::block_number();

                // Close any currently-active season.
                if let Some(prev_id) = ActiveSeasonId::<T>::get() {
                    if prev_id != season_id {
                        Seasons::<T>::mutate(prev_id, |prev| {
                            if let Some(prev) = prev {
                                if prev.status == SeasonStatus::Active {
                                    prev.status = SeasonStatus::Closed;
                                    prev.closed_at = Some(now);
                                }
                            }
                        });
                        Self::deposit_event(Event::SeasonClosed { season_id: prev_id });
                    }
                }

                info.status = SeasonStatus::Active;
                info.activated_at = Some(now);
                ActiveSeasonId::<T>::put(Some(season_id));
                Ok(())
            })?;

            Self::deposit_event(Event::SeasonActivated { season_id });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::close_season())]
        pub fn close_season(origin: OriginFor<T>, season_id: SeasonId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(Admins::<T>::contains_key(&who), Error::<T>::NotAdmin);

            let now = <frame_system::Pallet<T>>::block_number();
            Seasons::<T>::try_mutate(season_id, |maybe_info| -> DispatchResult {
                let info = maybe_info.as_mut().ok_or(Error::<T>::UnknownSeason)?;
                match info.status {
                    SeasonStatus::Draft => return Err(Error::<T>::SeasonNotActive.into()),
                    SeasonStatus::Active => {}
                    SeasonStatus::Closed => return Err(Error::<T>::AlreadyClosed.into()),
                }

                info.status = SeasonStatus::Closed;
                info.closed_at = Some(now);
                Ok(())
            })?;

            if ActiveSeasonId::<T>::get() == Some(season_id) {
                ActiveSeasonId::<T>::put(None::<SeasonId>);
            }

            Self::deposit_event(Event::SeasonClosed { season_id });
            Ok(())
        }
    }
}
