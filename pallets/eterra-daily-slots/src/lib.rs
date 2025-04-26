#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use sp_runtime::traits::BlockNumberProvider;
use frame_support::traits::{Get, UnixTime};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::pallet_prelude::*;
use sp_runtime::traits::SaturatedConversion;
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
    use frame_support::traits::UnixTime; // ✅
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::{BlockNumberProvider, Hash, SaturatedConversion};
    use sp_std::vec::Vec;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type TimeProvider: UnixTime;
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
    #[pallet::getter(fn rolls_this_block)]
    pub type RollsThisBlock<T: Config> = StorageDoubleMap<
        _, Blake2_128Concat, BlockNumberFor<T>, Blake2_128Concat, T::AccountId, u32, ValueQuery
    >;

    #[pallet::storage]
    #[pallet::getter(fn slot_machine_config)]
    pub type SlotMachineConfig<T: Config> = StorageValue<_, (u32, u32, u32), ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn tickets_per_user)]
    pub type TicketsPerUser<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn total_tickets)]
    pub type TotalTickets<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn last_drawing_time)]
    pub type LastDrawingTime<T: Config> = StorageValue<_, u64, ValueQuery>;

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
        DrawNotReadyYet,
        NoTicketsAvailable,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn roll(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            log::debug!("(pallet_eterra_daily_slots) roll called by user={:?}", who);

            let (slot_length, options_per_slot, rolls_per_round) = SlotMachineConfig::<T>::get();

            ensure!(
                slot_length > 0 && options_per_slot > 0 && rolls_per_round > 0,
                Error::<T>::InvalidConfiguration
            );

            let current_block = frame_system::Pallet::<T>::block_number();
            let roll_count = RollsThisBlock::<T>::get(current_block, &who);

            if roll_count >= rolls_per_round {
                return Err(Error::<T>::ExceedRollsPerRound.into());
            }

            let now = T::TimeProvider::now().as_secs();
            let last_roll = LastRollTime::<T>::get(&who);
            ensure!(now >= last_roll + 86_400, Error::<T>::RollNotAvailableYet);

            let mut result = Vec::new();
            for _ in 0..slot_length {
                let roll_value = (now % options_per_slot.saturated_into::<u64>()) as u32;
                result.push(roll_value);
            }

            RollsThisBlock::<T>::insert(current_block, &who, roll_count + 1);
            LastRollTime::<T>::insert(&who, now);

            let ticket_symbol: u32 = 7;
            let ticket_count = result.iter().filter(|&&v| v == ticket_symbol).count() as u32;
            if ticket_count > 0 {
                TicketsPerUser::<T>::mutate(&who, |t| *t += ticket_count);
                TotalTickets::<T>::mutate(|total| *total += ticket_count);
            }

            Self::deposit_event(Event::SlotRolled { player: who, result });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn perform_weekly_drawing() -> Result<(), Error<T>> {
            let total_tickets = TotalTickets::<T>::get();
            if total_tickets == 0 {
                return Err(Error::<T>::NoTicketsAvailable);
            }

            let now = T::TimeProvider::now().as_secs();
            let random_seed = T::Hashing::hash_of(&(
                now,
                frame_system::Pallet::<T>::block_number()
            ));

            let random_value = random_seed.as_ref()[0] as u32 % total_tickets;

            let mut cumulative = 0;
            for (account, tickets) in TicketsPerUser::<T>::iter() {
                cumulative += tickets;
                if random_value < cumulative {
                    Self::deposit_event(Event::WeeklyWinner { winner: account.clone() });
                    break;
                }
            }

            TicketsPerUser::<T>::remove_all(None);
            TotalTickets::<T>::put(0);
            LastDrawingTime::<T>::put(now);

            Ok(())
        }
    }

  #[pallet::hooks]
  impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
      fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
          let now = T::TimeProvider::now().as_secs();
          let days_since_epoch = now / 86_400;
          let day_of_week = (days_since_epoch + 4) % 7;
          let seconds_today = now % 86_400;

          if day_of_week == 0 && seconds_today >= 64_800 {
              let last_draw = LastDrawingTime::<T>::get();
              if (now - last_draw) >= 86_400 {
                  if let Err(e) = Self::perform_weekly_drawing() {
                      log::warn!("(pallet_eterra_daily_slots) => Weekly drawing failed: {:?}", e);
                  }
              }
          }

          frame_support::weights::Weight::from_parts(10_000, 0)
      }
  }
}
