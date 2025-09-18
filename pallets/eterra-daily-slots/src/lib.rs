// pallets/eterra-daily-slots/src/lib.rs

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*, traits::UnixTime};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::Hash;
use sp_std::vec;
use sp_std::vec::Vec;

use log::info;

const SECONDS_PER_DAY: u64 = 86_400;
const EVENING_THRESHOLD: u64 = 18 * 3600;

/// We target ~6 hours per window with 6s block time ⇒ 6h * 3600 / 6 = 3600 blocks.
const BLOCKS_PER_WINDOW: u64 = 3_600;

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
        #[pallet::constant]
        type MaxSlotLength: Get<u32>;
        /// How many symbols per reel
        #[pallet::constant]
        type MaxOptionsPerSlot: Get<u32>;
        /// Max rolls allowed per block
        #[pallet::constant]
        type MaxRollsPerRound: Get<u32>;
        /// Maximum number of roll results stored per account
        #[pallet::constant]
        type MaxRollHistoryLength: Get<u32>;
        /// Number of entries per reel
        #[pallet::constant]
        type MaxWeightEntries: Get<u32>;
    }

    // ─── STORAGE ────────────────────────────────────────────────────────────────

    #[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, MaxEncodedLen)]
    #[scale_info(skip_type_params(T))]
    pub struct RollResult<T: Config> {
        pub timestamp: u64,
        pub result: BoundedVec<u32, T::MaxSlotLength>,
    }

    /// (window_index, count_in_window)
    #[pallet::storage]
    #[pallet::getter(fn rolls_this_window_for)]
    /// Stores the number of rolls a user has performed in the current 6-hour window, keyed by account.
    /// The key value stores (window_index, count_in_window).
    pub type RollsThisWindow<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, (u64, u32), ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn last_roll_time)]
    /// Stores the timestamp of the last roll per user.
    pub type LastRollTime<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn rolls_this_block)]
    /// Tracks how many rolls a user has done in the current block.
    pub type RollsThisBlock<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        BlockNumberFor<T>,
        Blake2_128Concat,
        T::AccountId,
        u32,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn tickets_per_user)]
    /// Tracks the number of tickets each user has earned.
    pub type TicketsPerUser<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn total_tickets)]
    /// Total tickets accumulated across all users.
    pub type TotalTickets<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn last_drawing_time)]
    /// Timestamp of the last weekly drawing.
    pub type LastDrawingTime<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn roll_history)]
    /// Stores the roll history for each user as a bounded vector.
    pub type RollHistory<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<RollResult<T>, T::MaxRollHistoryLength>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn reel_weights)]
    /// Stores the weights for each reel (indexed by reel index).
    pub type ReelWeights<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u32,                                         // reel index
        BoundedVec<(u32, u32), T::MaxWeightEntries>, // (symbol, weight)
        OptionQuery,
    >;

    // ─── EVENTS & ERRORS ───────────────────────────────────────────────────────

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        SlotRolled {
            player: T::AccountId,
            result: Vec<u32>,
        },
        WeeklyWinner {
            winner: T::AccountId,
        },
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
        /// Roll the slot machine for the caller, producing a set of symbols.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn roll(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let slot_len = T::MaxSlotLength::get();
            let options = T::MaxOptionsPerSlot::get();
            let max_rolls = T::MaxRollsPerRound::get();
            ensure!(
                slot_len > 0 && options > 0 && max_rolls > 0,
                Error::<T>::InvalidConfiguration
            );

            // ─── ROLL CAP: 3 spins per ~6 hours (block-number based) ────────
            // We assume ~6s per block; 6 hours ≈ 3600 blocks.
            let bn_u64: u64 = TryInto::<u64>::try_into(frame_system::Pallet::<T>::block_number()).unwrap_or(0);
            let window_index = bn_u64 / BLOCKS_PER_WINDOW;
            let (stored_win, used) = Self::rolls_this_window_for(&who);
            let used = if stored_win == window_index { used } else { 0 };
            ensure!(used < max_rolls, Error::<T>::ExceedRollsPerRound);

            // Keep `now_secs` for entropy and history timestamps:
            let now_secs = T::TimeProvider::now().as_secs();

            // ─── DO THE SLOTS ───────────────────
            let mut result = Vec::with_capacity(slot_len as usize);
            for reel_index in 0..slot_len {
                // Fetch weights from storage for this reel
                let weights =
                    ReelWeights::<T>::get(reel_index).ok_or(Error::<T>::InvalidConfiguration)?;
                info!(
                    "[daily_slots] Using weights for reel {}: {:?}",
                    reel_index, weights
                );

                // Create unique input per reel
                let entropy = (
                    now_secs,
                    &who,
                    reel_index,
                    frame_system::Pallet::<T>::block_number(),
                    window_index,
                );
                let hash = T::Hashing::hash_of(&entropy);

                // Weighted selection logic
                let total_weight = weights.iter().map(|(_, w)| *w).sum::<u32>();
                ensure!(total_weight > 0, Error::<T>::InvalidConfiguration);

                let selection_threshold = {
                    let seed_bytes = &hash.as_ref()[0..4];
                    u32::from_le_bytes([seed_bytes[0], seed_bytes[1], seed_bytes[2], seed_bytes[3]])
                        % total_weight
                };

                let mut acc = 0;
                let chosen_symbol = weights
                    .iter()
                    .find_map(|(symbol, weight)| {
                        acc += *weight;
                        if selection_threshold < acc {
                            Some(*symbol)
                        } else {
                            None
                        }
                    })
                    .ok_or(Error::<T>::InvalidConfiguration)?;

                result.push(chosen_symbol);
            }

            // ─── UPDATE STATE ───────────────────
            // bump that user’s count for *this* window
            RollsThisWindow::<T>::insert(&who, (window_index, used + 1));
            LastRollTime::<T>::insert(&who, now_secs);

            // ─── AWARD TICKETS ──────────────────
            let ticket_symbol = 7u32;
            let tickets = result.iter().filter(|&&v| v == ticket_symbol).count() as u32;
            if tickets > 0 {
                TicketsPerUser::<T>::mutate(&who, |t| *t = t.saturating_add(tickets));
                TotalTickets::<T>::mutate(|t| *t = t.saturating_add(tickets));
            }

            Self::deposit_event(Event::SlotRolled {
                player: who.clone(),
                result: result.clone(),
            });

            // Save the roll result
            let bounded_result: BoundedVec<_, T::MaxSlotLength> = result
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::InvalidConfiguration)?;

            let roll_entry = RollResult::<T> {
                timestamp: now_secs,
                result: bounded_result,
            };

            RollHistory::<T>::mutate(&who, |history| {
                if history.len() as u32 >= T::MaxRollHistoryLength::get() {
                    history.remove(0);
                }
                let _ = history.try_push(roll_entry);
            });

            Ok(())
        }

        /// Set the weights for one reel (indexed by `reel`).
        /// To bias results, ensure all reels (from 0 to MaxSlotLength - 1) are updated.
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn set_reel_weights(
            origin: OriginFor<T>,
            reel: u32,
            weights: Vec<(u32, u32)>,
        ) -> DispatchResult {
            ensure_root(origin)?; // or ensure_signed(origin)? with checks

            Self::update_reel_weights(reel, weights)?;

            Ok(())
        }

        /// Allows a root origin to update multiple reels' weights in one call.
        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn set_all_reel_weights(
            origin: OriginFor<T>,
            all_weights: Vec<(u32, Vec<(u32, u32)>)>,
        ) -> DispatchResult {
            ensure_root(origin)?;

            for (reel, weights) in all_weights {
                Self::update_reel_weights(reel, weights)?;
            }

            Ok(())
        }
    }

    // ─── INTERNAL ───────────────────────────────────────────────────────────────

    impl<T: Config> Pallet<T> {
        /// Internal helper to update reel weights, converting and inserting into storage.
        fn update_reel_weights(reel: u32, weights: Vec<(u32, u32)>) -> Result<(), Error<T>> {
            // Reject empty weight lists
            if weights.is_empty() {
                return Err(Error::<T>::InvalidConfiguration);
            }

            // Clone weights for logging after move into BoundedVec
            let weights_for_log = weights.clone();
            let bounded: BoundedVec<_, T::MaxWeightEntries> = weights
                .try_into()
                .map_err(|_| Error::<T>::InvalidConfiguration)?;

            ReelWeights::<T>::insert(reel, bounded);
            info!(
                "[daily_slots] Set weights for reel {}: {:?}",
                reel, weights_for_log
            );
            Ok(())
        }

        fn perform_weekly_drawing() -> Result<(), Error<T>> {
            let total = TotalTickets::<T>::get();
            if total == 0 {
                return Err(Error::<T>::NoTicketsAvailable);
            }
            let now = T::TimeProvider::now().as_secs();
            let seed = T::Hashing::hash_of(&(now, frame_system::Pallet::<T>::block_number()));
            let pick = (seed.as_ref()[0] as u32) % total;

            let mut cum = 0;
            for (acct, share) in TicketsPerUser::<T>::iter() {
                cum += share;
                if pick < cum {
                    Self::deposit_event(Event::WeeklyWinner {
                        winner: acct.clone(),
                    });
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
            // Only on the first block
            if _n == 1u32.into() {
                let default_weights = vec![
                    (0, vec![(0, 5), (1, 3), (2, 2)]),
                    (1, vec![(0, 1), (1, 1), (2, 8)]),
                    (2, vec![(0, 4), (1, 4), (2, 2)]),
                ];

                for (reel, weights) in default_weights {
                    if !ReelWeights::<T>::contains_key(reel) {
                        let bounded: BoundedVec<_, T::MaxWeightEntries> =
                            weights.try_into().expect("Hardcoded weights are valid");
                        ReelWeights::<T>::insert(reel, bounded);
                    }
                }
            }

            // Grab “now” once:
            let now_secs = T::TimeProvider::now().as_secs();

            // How many seconds have elapsed since UNIX epoch in days:
            let days_since_epoch = now_secs / SECONDS_PER_DAY;
            // Adjust so that day_of_week == 0 means Sunday:
            let day_of_week = (days_since_epoch + 4) % 7;

            // How many seconds into *today* we are:
            let secs_today = now_secs % SECONDS_PER_DAY;

            // Only run the weekly drawing if *both*:
            //   1) it's Sunday (day_of_week == 0), and
            //   2) it's at or after 18:00 (EVENING_THRESHOLD)
            let is_sunday = day_of_week == 0;
            let is_after_6pm = secs_today >= EVENING_THRESHOLD;
            if !(is_sunday && is_after_6pm) {
                // bail out early, no drawing
                return Weight::from_parts(10_000, 0);
            }

            // If we’ve already done a drawing in the last 24 h, bail again:
            let last = LastDrawingTime::<T>::get();
            if now_secs.saturating_sub(last) < 24 * 3600 {
                return Weight::from_parts(10_000, 0);
            }

            // Now we really do a weekly drawing
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
