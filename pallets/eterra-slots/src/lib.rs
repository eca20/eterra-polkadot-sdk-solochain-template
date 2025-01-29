#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use frame_support::{pallet_prelude::*, traits::Get};
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
use sp_runtime::traits::Hash;
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        #[pallet::constant]
        type RandomnessSeed: Get<u64>;
    }

    #[pallet::storage]
    #[pallet::getter(fn player_attempts)]
    pub type PlayerAttempts<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u8, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        SlotGenerated {
            player: T::AccountId,
            values: [u8; 4],
        },
        SlotAccepted {
            player: T::AccountId,
        },
        SlotDenied {
            player: T::AccountId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        MaxAttemptsExceeded,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)] // Temporary until benchmarking is implemented
        pub fn generate_slot(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            let attempts = PlayerAttempts::<T>::get(&player);
            ensure!(attempts < 3, Error::<T>::MaxAttemptsExceeded);

            let current_block = <frame_system::Pallet<T>>::block_number();
            let seed = T::RandomnessSeed::get();
            let hash = T::Hashing::hash_of(&(current_block, &player, seed));
            let values = hash.as_ref()[..4].try_into().unwrap_or([0u8; 4]);

            Self::deposit_event(Event::SlotGenerated {
                player: player.clone(),
                values,
            });

            PlayerAttempts::<T>::insert(&player, attempts + 1);
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(10_000)] // Temporary until benchmarking is implemented
        pub fn accept_slot(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            PlayerAttempts::<T>::remove(&player);
            Self::deposit_event(Event::SlotAccepted { player });

            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(10_000)] // Temporary until benchmarking is implemented
        pub fn deny_slot(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            Self::deposit_event(Event::SlotDenied { player });

            Ok(())
        }
    }
}