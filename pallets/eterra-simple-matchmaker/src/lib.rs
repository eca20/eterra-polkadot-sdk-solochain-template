#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use frame_support::{dispatch::DispatchResult, pallet_prelude::*, traits::Get};
use frame_system::pallet_prelude::*;
use sp_std::prelude::*;

/// A lightweight bridge to verify that an account has configured a Current Hand
/// in the cards/game pallet. The runtime implements this by delegating to the
/// other pallet's storage (e.g. `eterra::CurrentHandOf`).
pub trait CurrentHandProvider<AccountId> {
    /// Returns true iff the account has a non-None current hand configured.
    fn has_current_hand(who: &AccountId) -> bool;
}

/// A callback interface the runtime/game pallet implements so the matchmaker
/// can create a game the moment two players are matched.
pub trait GameCreator<AccountId> {
    /// The concrete GameId type of the game pallet.
    type GameId;
    /// Create a new game for the given players. Implemented in the game pallet.
    fn create_from_matchmaking(
        p1: &AccountId,
        p2: &AccountId,
    ) -> Result<Self::GameId, sp_runtime::DispatchError>;
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Constant that indicates how many players are needed to create a match.
        #[pallet::constant]
        type PlayersPerMatch: Get<u8>;

        /// Capacity of the matchmaking queue.
        #[pallet::constant]
        type QueueCapacity: Get<u32>;
        /// A runtime hook used to check whether a player has a preset hand.
        /// Implement this in the runtime by delegating to your game/cards pallet.
        type HandProvider: super::CurrentHandProvider<Self::AccountId>;
        /// Hook to the game pallet that actually creates a game once two players are matched.
        type GameCreator: super::GameCreator<Self::AccountId>;
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    pub type QIndex = u32;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn head)]
    pub type Head<T: Config> = StorageValue<_, QIndex, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn tail)]
    pub type Tail<T: Config> = StorageValue<_, QIndex, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn ring)]
    pub type Ring<T: Config> = StorageMap<_, Blake2_128Concat, QIndex, T::AccountId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn in_queue)]
    pub type InQueue<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, (), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn live_size)]
    pub type LiveSize<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        Joined { who: T::AccountId },
        Left { who: T::AccountId },
        Matched { players: [T::AccountId; 2] },
        /// Emitted right after a join increases live size to at least the players-per-match threshold.
        TwoReadyToMatch { live_size: u32 },
        /// Emitted when `process_queue`/`join_queue` kicks off processing.
        ProcessingStarted { live_size: u32, head: QIndex, tail: QIndex },
        /// Emitted when we have popped two candidates to pair.
        PairFound { a: T::AccountId, b: T::AccountId },
        /// Emitted immediately before calling into the game pallet to create a game.
        GameCreateAttempt { a: T::AccountId, b: T::AccountId },
        /// Emitted when the second pop was unavailable and the first player was requeued.
        Requeued { who: T::AccountId },
        /// Emitted after processing finishes for this call.
        ProcessingCompleted { remaining_live: u32, head: QIndex, tail: QIndex },
    }

    #[pallet::error]
    pub enum Error<T> {
        QueueFull,
        AlreadyQueued,
        NotQueued,
        BadCapacity,
        /// Player attempted to queue without having a preset hand configured.
        NoPresetHand,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn join_queue(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let cap = T::QueueCapacity::get();
            ensure!(cap > 1, Error::<T>::BadCapacity);
            ensure!(
                InQueue::<T>::contains_key(&who) == false,
                Error::<T>::AlreadyQueued
            );
            // Require that the player has configured a Current Hand in the game/cards pallet.
            ensure!(
                T::HandProvider::has_current_hand(&who),
                Error::<T>::NoPresetHand
            );

            Head::<T>::mutate(|head| {
                Tail::<T>::mutate(|tail| -> DispatchResult {
                    let size = Self::ring_size(*head, *tail, cap);
                    ensure!(size < cap, Error::<T>::QueueFull);

                    let idx = *tail % cap;
                    Ring::<T>::insert(idx, &who);
                    *tail = tail.wrapping_add(1);

                    InQueue::<T>::insert(&who, ());
                    LiveSize::<T>::mutate(|n| *n = n.saturating_add(1));

                    // If we now have enough players to match, emit a signal.
                    let threshold = T::PlayersPerMatch::get() as u32;
                    let current = LiveSize::<T>::get();
                    if current >= threshold {
                        Self::deposit_event(Event::TwoReadyToMatch { live_size: current });
                    }

                    Self::deposit_event(Event::Joined { who: who.clone() });
                    Self::do_process(cap)?;
                    Ok(())
                })
            })?;

            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn leave_queue(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(InQueue::<T>::contains_key(&who), Error::<T>::NotQueued);

            InQueue::<T>::remove(&who);
            LiveSize::<T>::mutate(|n| *n = n.saturating_sub(1));
            Self::deposit_event(Event::Left { who });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn process_queue(origin: OriginFor<T>) -> DispatchResult {
            let _ = ensure_signed(origin).ok();
            let cap = T::QueueCapacity::get();
            ensure!(cap > 1, Error::<T>::BadCapacity);
            Self::deposit_event(Event::ProcessingStarted {
                live_size: LiveSize::<T>::get(),
                head: Head::<T>::get(),
                tail: Tail::<T>::get(),
            });
            Self::do_process(cap)
        }
    }

    impl<T: Config> Pallet<T> {
        fn ring_size(head: QIndex, tail: QIndex, _cap: QIndex) -> QIndex {
            tail.wrapping_sub(head)
        }

        fn pop_live(cap: QIndex) -> Option<T::AccountId> {
            Head::<T>::mutate(|head| {
                // We’ll search up to `cap` slots (one full cycle) to find a live account.
                // This makes the ring robust even if `head` previously advanced past older entries.
                let mut h = *head;
                let tail = Tail::<T>::get();

                for _ in 0..cap {
                    let idx = h % cap;
                    h = h.wrapping_add(1);

                    if let Some(acc) = Ring::<T>::take(idx) {
                        if InQueue::<T>::contains_key(&acc) {
                            *head = h;
                            InQueue::<T>::remove(&acc);
                            LiveSize::<T>::mutate(|n| *n = n.saturating_sub(1));
                            return Some(acc);
                        }
                    }

                    // If we made a full pass to the current tail without success and there are no
                    // gaps to consider, continue scanning; the `for` cap-guard prevents infinite loops.
                    if h == tail {
                        // continue scanning post-tail region in case of wrap-around entries
                    }
                }
                // Nothing live found in a full cycle.
                None
            })
        }

        fn do_process(cap: QIndex) -> DispatchResult {
            // Mirror the start event for calls coming from join_queue path.
            Self::deposit_event(Event::ProcessingStarted {
                live_size: LiveSize::<T>::get(),
                head: Head::<T>::get(),
                tail: Tail::<T>::get(),
            });
            loop {
                if LiveSize::<T>::get() < 2 {
                    break;
                }
                let a = match Self::pop_live(cap) {
                    Some(x) => x,
                    None => break,
                };
                let b = match Self::pop_live(cap) {
                    Some(x) => x,
                    None => {
                        Tail::<T>::mutate(|tail| {
                            let idx = *tail % cap;
                            Ring::<T>::insert(idx, &a);
                            *tail = tail.wrapping_add(1);
                        });
                        InQueue::<T>::insert(&a, ());
                        LiveSize::<T>::mutate(|n| *n = n.saturating_add(1));
                        Self::deposit_event(Event::Requeued { who: a.clone() });
                        break;
                    }
                };
                Self::deposit_event(Event::PairFound { a: a.clone(), b: b.clone() });

                if a == b {
                    // Extremely defensive: never match the same account with itself.
                    // Requeue `a` and stop this processing round.
                    Tail::<T>::mutate(|tail| {
                        let idx = *tail % cap;
                        Ring::<T>::insert(idx, &a);
                        *tail = tail.wrapping_add(1);
                    });
                    InQueue::<T>::insert(&a, ());
                    LiveSize::<T>::mutate(|n| *n = n.saturating_add(1));
                    Self::deposit_event(Event::Requeued { who: a.clone() });
                    break;
                }

                Self::deposit_event(Event::GameCreateAttempt { a: a.clone(), b: b.clone() });
                // Ask the game pallet to create a game for this pair. If it fails we still emit Matched.
                let _ = T::GameCreator::create_from_matchmaking(&a, &b);
                Self::deposit_event(Event::Matched {
                    players: [a.clone(), b.clone()],
                });
            }
            Self::deposit_event(Event::ProcessingCompleted {
                remaining_live: LiveSize::<T>::get(),
                head: Head::<T>::get(),
                tail: Tail::<T>::get(),
            });
            Ok(())
        }
    }
}
