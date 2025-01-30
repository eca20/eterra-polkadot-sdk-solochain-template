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
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        #[pallet::constant]
        type RandomnessSeed: Get<u64>;

        #[pallet::constant]
        type MaxAttempts: Get<u8>;

        #[pallet::constant]
        type CardsPerPack: Get<u8>;

        #[pallet::constant]
        type MaxPacks: Get<u32>;
    }

    #[derive(Clone, Encode, Decode, Default, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct Card {
        id: u32,
        attempts: u8,
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

    #[pallet::storage]
    #[pallet::getter(fn player_packs)]
    pub type PlayerPacks<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BoundedVec<Pack, T::MaxPacks>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn active_card)]
    pub type ActiveCard<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, Option<u8>, ValueQuery>;

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

    #[pallet::error]
    pub enum Error<T> {
        MaxAttemptsExceeded,
        NoActiveCard,
        PackAlreadyCompleted,
        NoPackFound,
        MaxPacksReached,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn mint_pack(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            log::debug!(
                "mint_pack: Player {:?} is attempting to mint a pack.",
                player
            );

            let mut packs = PlayerPacks::<T>::get(&player);
            log::debug!(
                "mint_pack: Player {:?} currently has {} packs.",
                player,
                packs.len()
            );

            ensure!(
                packs.len() < T::MaxPacks::get() as usize,
                Error::<T>::MaxPacksReached
            );

            let pack_id = <frame_system::Pallet<T>>::block_number().saturated_into::<u32>();
            log::debug!("mint_pack: Assigning pack ID {:?}", pack_id);

            let mut cards: BoundedVec<Card, ConstU32<16>> = BoundedVec::default();

            for i in 0..T::CardsPerPack::get() {
                log::debug!("mint_pack: Adding card {} to pack.", i);
                cards
                    .try_push(Card {
                        id: i as u32,
                        attempts: 0,
                        finalized: false,
                        slot_values: None,
                    })
                    .map_err(|_| {
                        log::error!("mint_pack: Failed to add card {} to pack.", i);
                        Error::<T>::MaxPacksReached
                    })?;
            }

            let pack = Pack {
                id: pack_id,
                cards,
                active_card_index: 0,
                completed: false,
            };

            packs.try_push(pack).map_err(|_| {
                log::error!("mint_pack: Failed to add new pack for player {:?}", player);
                Error::<T>::MaxPacksReached
            })?;

            PlayerPacks::<T>::insert(&player, packs);
            ActiveCard::<T>::insert(&player, Some(0));

            log::debug!(
                "mint_pack: Successfully minted pack {:?} for player {:?}",
                pack_id,
                player
            );

            // 🔹 Ensure event is emitted
            Self::deposit_event(Event::PackMinted {
                player: player.clone(),
                pack_id,
            });

            log::debug!(
                "mint_pack: PackMinted event emitted for player {:?}, pack_id {:?}",
                player,
                pack_id
            );

            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn generate_slot(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            PlayerPacks::<T>::mutate(&player, |packs| {
                let pack_index = packs.len().saturating_sub(1);
                let pack = packs.get_mut(pack_index).ok_or(Error::<T>::NoPackFound)?;

                let active_card_idx =
                    ActiveCard::<T>::get(&player).ok_or(Error::<T>::NoActiveCard)?;
                let max_attempts = T::MaxAttempts::get();

                // 🔹 Extract reference to card ONCE
                let card = &mut pack.cards[active_card_idx as usize];

                ensure!(
                    card.attempts < max_attempts,
                    Error::<T>::MaxAttemptsExceeded
                );

                let current_block = <frame_system::Pallet<T>>::block_number();
                let seed = T::RandomnessSeed::get();
                let hash = T::Hashing::hash_of(&(current_block, &player, seed));
                let values = hash.as_ref()[..4].try_into().unwrap_or([0u8; 4]);

                card.slot_values = Some(values);
                card.attempts += 1;
                let card_id = card.id;

                if card.attempts == max_attempts {
                    card.finalized = true;
                    Self::finalize_card(&player, pack, active_card_idx)?;
                }

                // 🔹 Ensure event is emitted
                Self::deposit_event(Event::SlotGenerated {
                    player: player.clone(),
                    pack_id: pack.id,
                    card_id,
                    values,
                });

                Ok(())
            })
        }

        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn accept_slot(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            PlayerPacks::<T>::mutate(&player, |packs| {
                if packs.is_empty() {
                    return Err(Error::<T>::NoPackFound.into());
                }

                let pack_index = packs.len().saturating_sub(1);
                let pack = packs.get_mut(pack_index).ok_or(Error::<T>::NoPackFound)?;

                let active_card_idx =
                    ActiveCard::<T>::get(&player).ok_or(Error::<T>::NoActiveCard)?;

                // Extract mutable reference ONCE
                let card = &mut pack.cards[active_card_idx as usize];

                // Ensure the slot has been rolled before allowing acceptance
                if card.slot_values.is_none() {
                    return Err(Error::<T>::NoActiveCard.into());
                }

                let card_id = card.id; // Get ID before modifying

                card.finalized = true;

                Self::finalize_card(&player, pack, active_card_idx)?;

                Self::deposit_event(Event::SlotAccepted {
                    player: player.clone(),
                    pack_id: pack.id,
                    card_id,
                });

                Ok(())
            })
        }
    }

    impl<T: Config> Pallet<T> {
        fn finalize_card(
            player: &T::AccountId,
            pack: &mut Pack,
            current_idx: u8,
        ) -> DispatchResult {
            let card = &mut pack.cards[current_idx as usize];
            let card_id = card.id;

            // 🔹 Ensure `finalized` is properly set
            card.finalized = true;

            Self::deposit_event(Event::SlotFinalized {
                player: player.clone(),
                pack_id: pack.id,
                card_id,
            });

            if (current_idx as usize) < (pack.cards.len() - 1) {
                ActiveCard::<T>::insert(player, Some(current_idx + 1));
            } else {
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
