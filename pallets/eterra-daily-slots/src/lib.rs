#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

// Added to use `BlockNumberProvider` explicitly:
use sp_runtime::traits::BlockNumberProvider;

use frame_support::traits::{Get, UnixTime};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::pallet_prelude::*;
use sp_runtime::traits::SaturatedConversion;
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::traits::UnixTime;
    use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::{BlockNumberProvider, Hash, SaturatedConversion};
    use sp_std::vec::Vec;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The event type that is used in the runtime.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Provides the current timestamp (in seconds).
        type TimeProvider: UnixTime;

        /// The maximum slot length (number of reels) we allow per spin.
        #[pallet::constant]
        type MaxSlotLength: Get<u32>;

        /// The maximum number of different symbols per reel.
        #[pallet::constant]
        type MaxOptionsPerSlot: Get<u32>;

        /// The maximum rolls allowed in a single "round" (per block).
        #[pallet::constant]
        type MaxRollsPerRound: Get<u32>;
    }

    /// Maps each account to the last time (in seconds) they successfully rolled.
    #[pallet::storage]
    #[pallet::getter(fn last_roll_time)]
    pub type LastRollTime<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    /// Track how many rolls an account has done **in the current block**.
    #[pallet::storage]
    #[pallet::getter(fn rolls_this_block)]
    pub type RollsThisBlock<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        BlockNumberFor<T>,
        Blake2_128Concat,
        T::AccountId,
        u32,
        ValueQuery,
    >;

    /// Our configurable slot machine parameters: (slot_length, options_per_slot, rolls_per_round)
    #[pallet::storage]
    #[pallet::getter(fn slot_machine_config)]
    pub type SlotMachineConfig<T: Config> = StorageValue<_, (u32, u32, u32), ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A successful slot roll occurred.
        SlotRolled {
            player: T::AccountId,
            result: Vec<u32>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// The user hasn't waited 24 hours since their last roll.
        RollNotAvailableYet,
        /// Tried to roll more than allowed in a single block/round.
        ExceedRollsPerRound,
        /// The slot machine configuration was invalid (0, 0, or 0).
        InvalidConfiguration,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// The main entry point for a "roll" of the slot machine.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)] // or 0 for dev
        pub fn roll(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // 1. Log the user & config
            log::debug!("(pallet_eterra_daily_slots) roll called by user={:?}", who);

            let (slot_length, options_per_slot, rolls_per_round) = SlotMachineConfig::<T>::get();
            log::debug!(
        "(pallet_eterra_daily_slots) config => slot_length={}, options_per_slot={}, rolls_per_round={}",
        slot_length, options_per_slot, rolls_per_round
    );

            ensure!(
                slot_length > 0 && options_per_slot > 0 && rolls_per_round > 0,
                Error::<T>::InvalidConfiguration
            );

            // 2. Check how many times this user has rolled in this block
            let current_block = frame_system::Pallet::<T>::block_number();
            let roll_count = RollsThisBlock::<T>::get(current_block, &who);

            log::debug!(
                "(pallet_eterra_daily_slots) current_block={:?} => existing roll_count={:?}",
                current_block,
                roll_count
            );

            if roll_count >= rolls_per_round {
                log::debug!(
            "(pallet_eterra_daily_slots) ExceedRollsPerRound => user={:?}, roll_count={}, rolls_per_round={}",
            who, roll_count, rolls_per_round
        );
                return Err(Error::<T>::ExceedRollsPerRound.into());
            }

            // 3. Daily-limit logic
            let now = T::TimeProvider::now().as_secs();
            let last_roll = LastRollTime::<T>::get(&who);

            log::debug!(
                "(pallet_eterra_daily_slots) daily-limit => now={}, last_roll={}, min_required={}",
                now,
                last_roll,
                last_roll + 86_400
            );
            ensure!(now >= last_roll + 86_400, Error::<T>::RollNotAvailableYet);

            // 4. Perform the slot roll
            let mut result = Vec::new();
            for _ in 0..slot_length {
                let roll_value = (now % options_per_slot.saturated_into::<u64>()) as u32;
                result.push(roll_value);
            }

            // 5. Store updated info
            RollsThisBlock::<T>::insert(current_block, &who, roll_count + 1);
            log::debug!(
                "(pallet_eterra_daily_slots) => incremented roll_count => {}",
                roll_count + 1
            );

            LastRollTime::<T>::insert(&who, now);

            // 6. Emit success event
            log::debug!(
                "(pallet_eterra_daily_slots) => success: SlotRolled(user={:?}, result={:?})",
                who,
                result
            );
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
