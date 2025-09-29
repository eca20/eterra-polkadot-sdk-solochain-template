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
            ensure!(InQueue::<T>::contains_key(&who) == false, Error::<T>::AlreadyQueued);
            // Require that the player has configured a Current Hand in the game/cards pallet.
            ensure!(T::HandProvider::has_current_hand(&who), Error::<T>::NoPresetHand);

            Head::<T>::mutate(|head| {
                Tail::<T>::mutate(|tail| -> DispatchResult {
                    let size = Self::ring_size(*head, *tail, cap);
                    ensure!(size < cap, Error::<T>::QueueFull);

                    let idx = *tail % cap;
                    Ring::<T>::insert(idx, &who);
                    *tail = tail.wrapping_add(1);

                    InQueue::<T>::insert(&who, ());
                    LiveSize::<T>::mutate(|n| *n = n.saturating_add(1));

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
            Self::do_process(cap)
        }
    }

    impl<T: Config> Pallet<T> {
        fn ring_size(head: QIndex, tail: QIndex, _cap: QIndex) -> QIndex {
            tail.wrapping_sub(head)
        }

        fn pop_live(cap: QIndex) -> Option<T::AccountId> {
            Head::<T>::mutate(|head| {
                let tail = Tail::<T>::get();
                let mut h = *head;

                while h != tail {
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
                }
                *head = h;
                None
            })
        }

        fn do_process(cap: QIndex) -> DispatchResult {
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
                        break;
                    }
                };
                Self::deposit_event(Event::Matched { players: [a.clone(), b.clone()] });
            }
            Ok(())
        }
    }
}
