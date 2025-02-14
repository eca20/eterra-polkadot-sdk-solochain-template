#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use frame_support::traits::{Get, UnixTime}; // Import UnixTime
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::traits::SaturatedConversion;
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::{Hash, SaturatedConversion};
    use sp_std::vec::Vec;
    use frame_support::traits::UnixTime; // Import UnixTime

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type TimeProvider: UnixTime; // ✅ Use UnixTime instead of `pallet_timestamp::Config`
        
        #[pallet::constant]
        type MaxSlotLength: Get<u32>;

        #[pallet::constant]
        type MaxOptionsPerSlot: Get<u32>;

        #[pallet::constant]
        type MaxRollsPerRound: Get<u32>;
    }

    #[pallet::storage]
    #[pallet::getter(fn last_roll_time)]
    pub type LastRollTime<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn slot_machine_config)]
    pub type SlotMachineConfig<T: Config> = StorageValue<_, (u32, u32, u32), ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        SlotRolled {
            player: T::AccountId,
            result: Vec<u32>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        RollNotAvailableYet,
        InvalidConfiguration,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
       #[pallet::call_index(0)]
    #[pallet::weight(10_000)] // or 0 for dev
    pub fn roll(origin: OriginFor<T>) -> DispatchResult {
        let who = ensure_signed(origin)?;
        
        // 1. Check that the config is valid first
        let (slot_length, options_per_slot, rolls_per_round) = SlotMachineConfig::<T>::get();
        ensure!(
            slot_length > 0 && options_per_slot > 0 && rolls_per_round > 0,
            Error::<T>::InvalidConfiguration
        );

        // 2. Now check the daily-limit logic
        let now = T::TimeProvider::now().as_secs();
        let last_roll = LastRollTime::<T>::get(&who);
        ensure!(now >= last_roll + 86_400, Error::<T>::RollNotAvailableYet);

        // 3. Perform the slot roll
        let mut result = Vec::new();
        for _ in 0..slot_length {
            let roll = (now % options_per_slot.saturated_into::<u64>()) as u32;
            result.push(roll);
        }

        LastRollTime::<T>::insert(&who, now);
        Self::deposit_event(Event::SlotRolled {
            player: who,
            result,
        });

        Ok(())
    }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_finalize(_n: BlockNumberFor<T>) {
            // Future functionality for periodic actions
        }
    }
}