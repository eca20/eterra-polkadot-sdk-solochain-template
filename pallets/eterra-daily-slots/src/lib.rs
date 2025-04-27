// pallets/eterra-daily-slots/src/lib.rs

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*, traits::UnixTime};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::{Hash, SaturatedConversion};
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    /// Configuration trait for this pallet.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The outer event type
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// Time provider
        type TimeProvider: UnixTime;

        /// How many reels (slots)
        #[pallet::constant] type MaxSlotLength: Get<u32>;
        /// How many symbols per reel
        #[pallet::constant] type MaxOptionsPerSlot: Get<u32>;
        /// Max rolls allowed per block
        #[pallet::constant] type MaxRollsPerRound: Get<u32>;
    }

    // ─── STORAGE ────────────────────────────────────────────────────────────────

    #[pallet::storage]
    #[pallet::getter(fn last_roll_time)]
    pub type LastRollTime<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn rolls_this_block)]
    pub type RollsThisBlock<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat, BlockNumberFor<T>,
        Blake2_128Concat, T::AccountId,
        u32,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn slot_machine_config)]
    pub type SlotMachineConfig<T: Config> =
        StorageValue<_, (u32, u32, u32), ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn tickets_per_user)]
    pub type TicketsPerUser<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn total_tickets)]
    pub type TotalTickets<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn last_drawing_time)]
    pub type LastDrawingTime<T: Config> = StorageValue<_, u64, ValueQuery>;

    // ─── EVENTS & ERRORS ───────────────────────────────────────────────────────

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        SlotRolled { player: T::AccountId, result: Vec<u32> },
        WeeklyWinner { winner: T::AccountId },
    }

    #[pallet::error]
    pub enum Error<T> {
        RollNotAvailableYet,
        ExceedRollsPerRound,
        InvalidConfiguration,
        NoTicketsAvailable,
    }

    // ─── DISPATCHABLE CALLS ───────────────────────────────────────────────────

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Spin the slot machine.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn roll(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // read and validate config
            let (slot_length, options_per_slot, rolls_per_round) =
                SlotMachineConfig::<T>::get();
            ensure!(
                slot_length > 0 && options_per_slot > 0 && rolls_per_round > 0,
                Error::<T>::InvalidConfiguration
            );

            // per‐block limit
            let block = frame_system::Pallet::<T>::block_number();
            let used = RollsThisBlock::<T>::get(block, &who);
            ensure!(used < rolls_per_round, Error::<T>::ExceedRollsPerRound);

            // daily cooldown
            let now = T::TimeProvider::now().as_secs();
            let last = LastRollTime::<T>::get(&who);
            ensure!(now >= last + 86_400, Error::<T>::RollNotAvailableYet);

            // roll
            let mut result = Vec::with_capacity(slot_length as usize);
            for _ in 0..slot_length {
                let v = (now % options_per_slot.saturated_into::<u64>()) as u32;
                result.push(v);
            }

            // record
            RollsThisBlock::<T>::insert(block, &who, used + 1);
            LastRollTime::<T>::insert(&who, now);

            // award tickets for symbol == 7
            let count = result.iter().filter(|&&v| v == 7).count() as u32;
            if count > 0 {
                TicketsPerUser::<T>::mutate(&who, |t| *t += count);
                TotalTickets::<T>::mutate(|t| *t += count);
            }

            Self::deposit_event(Event::SlotRolled { player: who, result });
            Ok(())
        }
    }

    // ─── INTERNAL ───────────────────────────────────────────────────────────────

    impl<T: Config> Pallet<T> {
        fn perform_weekly_drawing() -> Result<(), Error<T>> {
            let total = TotalTickets::<T>::get();
            if total == 0 {
                return Err(Error::<T>::NoTicketsAvailable)
            }
            let now  = T::TimeProvider::now().as_secs();
            let seed = T::Hashing::hash_of(&(now, frame_system::Pallet::<T>::block_number()));
            let pick = (seed.as_ref()[0] as u32) % total;

            let mut cum = 0;
            for (acct, share) in TicketsPerUser::<T>::iter() {
                cum += share;
                if pick < cum {
                    Self::deposit_event(Event::WeeklyWinner { winner: acct.clone() });
                    break;
                }
            }

            // reset
            let _ = TicketsPerUser::<T>::clear(u32::MAX, None);
            TotalTickets::<T>::put(0);
            LastDrawingTime::<T>::put(now);
            Ok(())
        }
    }

    // ─── HOOKS ────────────────────────────────────────────────────────────────

use frame_support::weights::Weight;

#[pallet::hooks]
impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
        let now = T::TimeProvider::now().as_secs();
        let days_since_epoch = now / 86_400;
        let day_of_week = (days_since_epoch + 4) % 7;
        let seconds_today = now % 86_400;

        // bail out unless it's Sunday at/after 18:00
        if day_of_week != 0 || seconds_today < 64_800 {
            return Weight::from_parts(10_000, 0);
        }

        // bail out if we’ve already drawn in the last 24h
        let last_draw = LastDrawingTime::<T>::get();
        if now.saturating_sub(last_draw) < 86_400 {
            return Weight::from_parts(10_000, 0);
        }

        // perform the weekly drawing
        if let Err(e) = Self::perform_weekly_drawing() {
            log::warn!("(eterra-daily-slots) weekly drawing failed: {:?}", e);
        }

        Weight::from_parts(10_000, 0)
    }
}
}

pub use pallet::*;

/// Mock & tests live here
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;