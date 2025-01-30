#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use frame_support::{pallet_prelude::*, traits::Get, BoundedVec};
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::traits::{Hash, SaturatedConversion};
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::traits::ConstU32;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// A numeric seed for our randomness.
        #[pallet::constant]
        type RandomnessSeed: Get<u64>;

        /// The maximum times a card can generate slots before it is forced to finalize.
        #[pallet::constant]
        type MaxAttempts: Get<u8>;

        /// How many cards are in each newly minted pack.
        #[pallet::constant]
        type CardsPerPack: Get<u8>;

        /// The maximum number of packs a single account can hold.
        #[pallet::constant]
        type MaxPacks: Get<u32>;
    }

    // ------------------
    // Data Structures
    // ------------------

    #[derive(Clone, Encode, Decode, Default, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct Card {
        id: u32,
        finalized: bool,
        slot_values: Option<[u8; 4]>,
    }

    #[derive(Clone, Encode, Decode, Default, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct Pack {
        id: u32,
        cards: BoundedVec<Card, ConstU32<16>>,
        active_card_index: u8,
        completed: bool,
    }

    impl Pack {
        pub fn get_cards(&self) -> &BoundedVec<Card, ConstU32<16>> {
            &self.cards
        }

        pub fn get_id(&self) -> u32 {
            self.id
        }

        pub fn get_active_card_index(&self) -> u8 {
            self.active_card_index
        }

        pub fn get_completed(&self) -> bool {
            self.completed
        }
    }

    impl Card {
        pub fn get_slot_values(&self) -> Option<[u8; 4]> {
            self.slot_values
        }

        pub fn get_id(&self) -> u32 {
            self.id
        }

        pub fn get_finalized(&self) -> bool {
            self.finalized
        }
    }

    // ------------------
    // Storage Items
    // ------------------

    /// A map from account -> list of packs
    #[pallet::storage]
    #[pallet::getter(fn player_packs)]
    pub type PlayerPacks<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BoundedVec<Pack, T::MaxPacks>, ValueQuery>;

    /// Tracks the currently “active” card index for each account
    #[pallet::storage]
    #[pallet::getter(fn active_card)]
    pub type ActiveCard<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, Option<u8>, ValueQuery>;

    /// Stores the "attempts" count for each individual card:
    /// Key is `(account, pack_id, card_id)` => how many times they've generated so far
    #[pallet::storage]
    #[pallet::getter(fn card_attempts)]
    pub type CardAttempts<T: Config> =
        StorageMap<_, Blake2_128Concat, (T::AccountId, u32, u32), u8, ValueQuery>;

    // ------------------
    // Events
    // ------------------

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PackMinted {
            player: T::AccountId,
            pack_id: u32,
        },
        SlotGenerated {
            player: T::AccountId,
            pack_id: u32,
            card_id: u32,
            values: [u8; 4],
        },
        SlotAccepted {
            player: T::AccountId,
            pack_id: u32,
            card_id: u32,
        },
        SlotFinalized {
            player: T::AccountId,
            pack_id: u32,
            card_id: u32,
        },
        PackCompleted {
            player: T::AccountId,
            pack_id: u32,
        },
    }

    // ------------------
    // Errors
    // ------------------

    #[pallet::error]
    pub enum Error<T> {
        MaxAttemptsExceeded,
        NoActiveCard,
        PackAlreadyCompleted,
        NoPackFound,
        MaxPacksReached,
    }

    // ------------------
    // Calls (Extrinsics)
    // ------------------

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Mint a new pack of cards for the caller, up to `MaxPacks`.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn mint_pack(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            log::debug!(
                "mint_pack: Player {:?} is attempting to mint a pack.",
                player
            );

            let mut packs = PlayerPacks::<T>::get(&player);
            ensure!(
                packs.len() < T::MaxPacks::get() as usize,
                Error::<T>::MaxPacksReached
            );

            let pack_id = <frame_system::Pallet<T>>::block_number().saturated_into::<u32>();
            log::debug!("mint_pack: Assigning pack ID {:?}", pack_id);

            let mut cards: BoundedVec<Card, ConstU32<16>> = BoundedVec::default();

            // Create each card with no attempts stored in the struct
            for i in 0..T::CardsPerPack::get() {
                cards
                    .try_push(Card {
                        id: i as u32,
                        finalized: false,
                        slot_values: None,
                    })
                    .map_err(|_| Error::<T>::MaxPacksReached)?;
            }

            let pack = Pack {
                id: pack_id,
                cards,
                active_card_index: 0,
                completed: false,
            };

            packs
                .try_push(pack)
                .map_err(|_| Error::<T>::MaxPacksReached)?;

            // Update storage
            PlayerPacks::<T>::insert(&player, packs);
            ActiveCard::<T>::insert(&player, Some(0));

            Self::deposit_event(Event::PackMinted {
                player: player.clone(),
                pack_id,
            });
            Ok(())
        }

        /// Generate new slot values for the current (active) card, up to `MaxAttempts`.
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn generate_slot(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            PlayerPacks::<T>::mutate(&player, |packs| {
                let pack = packs.last_mut().ok_or(Error::<T>::NoPackFound)?; // use the last pack minted

                let active_card_idx =
                    ActiveCard::<T>::get(&player).ok_or(Error::<T>::NoActiveCard)?;

                let card = &mut pack.cards[active_card_idx as usize];
                let card_id = card.id;

                // Check how many attempts so far
                let mut attempts = CardAttempts::<T>::get((player.clone(), pack.id, card_id));

                ensure!(
                    attempts < T::MaxAttempts::get(),
                    Error::<T>::MaxAttemptsExceeded
                );

                // Generate random values
                let current_block = <frame_system::Pallet<T>>::block_number();
                let seed = T::RandomnessSeed::get();
                let hash = T::Hashing::hash_of(&(current_block, &player, seed));
                let values = hash.as_ref()[..4].try_into().unwrap_or([0u8; 4]);

                // Update the card
                card.slot_values = Some(values);

                // Increment attempts
                attempts += 1;
                CardAttempts::<T>::insert((player.clone(), pack.id, card_id), attempts);

                // If we've hit max attempts, finalize the card now
                if attempts == T::MaxAttempts::get() {
                    card.finalized = true;
                    Self::finalize_card(&player, pack, active_card_idx)?;
                }

                // Emit
                Self::deposit_event(Event::SlotGenerated {
                    player: player.clone(),
                    pack_id: pack.id,
                    card_id,
                    values,
                });

                Ok(())
            })
        }

        /// Accept the current card's slot values (finalize it immediately).
        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn accept_slot(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            PlayerPacks::<T>::mutate(&player, |packs| {
                let pack = packs.last_mut().ok_or(Error::<T>::NoPackFound)?;

                let active_card_idx =
                    ActiveCard::<T>::get(&player).ok_or(Error::<T>::NoActiveCard)?;

                let card = &mut pack.cards[active_card_idx as usize];

                // Must have generated at least once
                if card.slot_values.is_none() {
                    return Err(Error::<T>::NoActiveCard.into());
                }

                let card_id = card.id;
                // Mark the card as finalized
                card.finalized = true;

                // Also finalize from the pallet perspective
                Self::finalize_card(&player, pack, active_card_idx)?;

                // Emit
                Self::deposit_event(Event::SlotAccepted {
                    player: player.clone(),
                    pack_id: pack.id,
                    card_id,
                });

                Ok(())
            })
        }
    }

    // ------------------
    // Pallet Internals
    // ------------------

    impl<T: Config> Pallet<T> {
        /// Helper to finalize a card (mark it done, advance or complete the pack)
        fn finalize_card(
            player: &T::AccountId,
            pack: &mut Pack,
            current_idx: u8,
        ) -> DispatchResult {
            let card = &mut pack.cards[current_idx as usize];
            let card_id = card.id;

            // Mark final
            card.finalized = true;
            // Remove attempts data now that it's finalized
            CardAttempts::<T>::remove((player.clone(), pack.id, card_id));

            // Emit event
            Self::deposit_event(Event::SlotFinalized {
                player: player.clone(),
                pack_id: pack.id,
                card_id,
            });

            // If not at the last card, increment the active index
            if (current_idx as usize) < (pack.cards.len() - 1) {
                ActiveCard::<T>::insert(player, Some(current_idx + 1));
            } else {
                // If that was the last card, the pack is now complete
                pack.completed = true;
                ActiveCard::<T>::remove(player);

                Self::deposit_event(Event::PackCompleted {
                    player: player.clone(),
                    pack_id: pack.id,
                });
            }

            Ok(())
        }
    }
}
