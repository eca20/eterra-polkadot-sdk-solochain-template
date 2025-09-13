// TODO: Add limited card storage, 600 cards?
// TODO: Add ability to add storage for 50 tokens
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use self::pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use frame_support::{pallet_prelude::*, traits::Get, BoundedVec};
// ===== New: utilities for in-pallet game logic =====

const GRID_DIM: usize = 4;
const BOARD_SIZE: usize = GRID_DIM * GRID_DIM; // 16

use core::array;
use frame_support::pallet_prelude::ConstU32;
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::traits::{Hash, SaturatedConversion};
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::traits::ConstU32;
    use frame_system::pallet_prelude::BlockNumberFor;

    /// Convenience type aliases for IDs/balance types used in cards.
    pub type CardId = u32;
    pub type Balance = u128;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    /// Which edition a card belongs to (extensible for future sets).
    #[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug)]
    pub enum CardEdition {
        Base,
        Genesis,
        Limited,
        Promo,
    }
    impl Default for CardEdition {
        fn default() -> Self {
            CardEdition::Base
        }
    }

    /// Rarity classification for cards.
    #[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug)]
    pub enum RarityType {
        Common,
        Uncommon,
        Rare,
        Epic,
        Legendary,
    }
    impl Default for RarityType {
        fn default() -> Self {
            RarityType::Common
        }
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // ------------------
    // Pallet Config
    // ------------------

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

    /// The info stored about each card.
    #[derive(Clone, Encode, Decode, Default, PartialEq, TypeInfo, MaxEncodedLen, Debug)]
    #[scale_info(skip_type_params(T))]
    pub struct CardInfo<T: Config> {
        /// Current on-chain owner of this card.
        pub owner: T::AccountId,
        /// Finalization status of generated stats.
        pub finalized: bool,
        /// Optional 4-side values (stats) prior to/after finalize.
        pub slot_values: Option<[u8; 4]>,

        /// Display name for the card (bounded).
        pub name: BoundedVec<u8, ConstU32<64>>,
        /// Directional values used by the front end.
        pub north: u8,
        pub east: u8,
        pub south: u8,
        pub west: u8,

        /// New: canonical id for this card (mirrors the storage key but stored for convenience).
        pub card_id: CardId,
        /// New: block number when this card was minted/created.
        pub minted_at: BlockNumberFor<T>,
        /// New: optional list/price field (can represent last sale price or a reserve).
        pub price: Balance,
        /// New: edition (set) this card belongs to.
        pub edition: CardEdition,
        /// New: rarity classification.
        pub rarity: RarityType,
    }

    impl<T: Config> CardInfo<T> {
        pub fn get_owner(&self) -> &T::AccountId {
            &self.owner
        }
    }

    /// A "Pack" just references existing cards by their IDs, rather than embedding them.
    #[derive(Clone, Encode, Decode, Default, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct Pack {
        id: u32,
        // Store the IDs of the cards that were originally minted in this pack
        card_ids: BoundedVec<u32, ConstU32<16>>,
        active_card_index: u8,
        completed: bool,
    }

    impl Pack {
        pub fn get_id(&self) -> u32 {
            self.id
        }

        pub fn get_card_ids(&self) -> &BoundedVec<u32, ConstU32<16>> {
            &self.card_ids
        }

        pub fn get_active_card_index(&self) -> u8 {
            self.active_card_index
        }

        pub fn get_completed(&self) -> bool {
            self.completed
        }
    }

    // ------------------
    // Storage
    // ------------------

    /// A global counter to assign unique IDs to cards.
    #[pallet::storage]
    #[pallet::getter(fn next_card_id)]
    pub type NextCardId<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// All cards in the system, by global `card_id` => `CardInfo`.
    #[pallet::storage]
    #[pallet::getter(fn cards)]
    pub type Cards<T: Config> = StorageMap<_, Blake2_128Concat, u32, CardInfo<T>, OptionQuery>;

    /// A map from account => list of packs
    #[pallet::storage]
    #[pallet::getter(fn player_packs)]
    pub type PlayerPacks<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BoundedVec<Pack, T::MaxPacks>, ValueQuery>;

    // (ActiveCard and CardAttempts storage removed)

    // ------------------
    // Events
    // ------------------

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A new pack was minted for `player` with ID `pack_id`, containing multiple new cards.
        PackMinted { player: T::AccountId, pack_id: u32 },
        /// A pack was completed (all cards finalized).
        PackCompleted { player: T::AccountId, pack_id: u32 },
        /// A card was transferred from `from` to `to`.
        CardTransferred {
            from: T::AccountId,
            to: T::AccountId,
            card_id: u32,
        },
    }

    // ------------------
    // Errors
    // ------------------

    #[pallet::error]
    pub enum Error<T> {
        /// The user has no pack to operate on.
        NoPackFound,
        /// The user’s pack limit is reached.
        MaxPacksReached,
        /// Card does not exist in storage.
        NoSuchCard,
        /// You do not own the card you’re trying to act upon.
        NotCardOwner,
        // --- Match errors ---
        CardNotFinalized,
    }

    // ------------------
    // Calls (Extrinsics)
    // ------------------

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Mint a new pack of cards for the caller, up to `MaxPacks`.
        /// Each card is stored globally in `Cards<T>`.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn mint_pack(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            let mut packs = PlayerPacks::<T>::get(&player);
            ensure!(
                packs.len() < T::MaxPacks::get() as usize,
                Error::<T>::MaxPacksReached
            );

            let pack_id = <frame_system::Pallet<T>>::block_number().saturated_into::<u32>();

            // Build a new pack with references to newly minted card IDs
            let mut card_ids: BoundedVec<u32, ConstU32<16>> = BoundedVec::default();

            for _ in 0..6u8 {
                let new_card_id = Self::create_new_card(&player)?;
                // Attach this card to the pack
                card_ids
                    .try_push(new_card_id)
                    .map_err(|_| Error::<T>::MaxPacksReached)?;
            }

            let new_pack = Pack {
                id: pack_id,
                card_ids,
                active_card_index: 0,
                completed: true,
            };

            packs
                .try_push(new_pack)
                .map_err(|_| Error::<T>::MaxPacksReached)?;

            PlayerPacks::<T>::insert(&player, packs);

            Self::deposit_event(Event::PackMinted {
                player: player.clone(),
                pack_id,
            });
            Self::deposit_event(Event::PackCompleted {
                player: player.clone(),
                pack_id,
            });
            Ok(())
        }

        /// **New**: Transfer a single card from `origin` to `to`.
        /// If that card is also part of a pack, it still references it, but ownership
        /// changes to `to`.
        #[pallet::call_index(3)]
        #[pallet::weight(10_000)]
        pub fn transfer_card(
            origin: OriginFor<T>,
            card_id: u32,
            to: T::AccountId,
        ) -> DispatchResult {
            let from = ensure_signed(origin)?;

            Cards::<T>::mutate(card_id, |maybe_card| -> DispatchResult {
                let card_info = maybe_card.as_mut().ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == from, Error::<T>::NotCardOwner);

                // Transfer ownership
                card_info.owner = to.clone();

                Ok(())
            })?;

            Self::deposit_event(Event::CardTransferred { from, to, card_id });
            Ok(())
        }
    }

    // ------------------
    // Pallet Internals (helpers; not dispatchables)
    // ------------------
    impl<T: Config> Pallet<T> {
        /// Create a brand-new card with `owner`.
        fn create_new_card(owner: &T::AccountId) -> Result<u32, DispatchError> {
            let card_id = NextCardId::<T>::get();

            // Derive pseudo-random bytes from block, owner, seed, and card_id
            let current_block = <frame_system::Pallet<T>>::block_number();
            let seed = T::RandomnessSeed::get();
            let hash = T::Hashing::hash_of(&(current_block, owner, seed, card_id));

            // Use the first 4 bytes for the four directions (1..=9)
            let bytes = hash.as_ref();
            let mut to_stat = |b: u8| -> u8 { (b % 9) + 1 };

            let n = to_stat(bytes.get(0).copied().unwrap_or(0));
            let e = to_stat(bytes.get(1).copied().unwrap_or(0));
            let s = to_stat(bytes.get(2).copied().unwrap_or(0));
            let w = to_stat(bytes.get(3).copied().unwrap_or(0));

            // Name: "Card-<id>"
            let name_string = alloc::format!("Card-{}", card_id);
            let name_bv: BoundedVec<u8, ConstU32<64>> =
                BoundedVec::try_from(name_string.into_bytes())
                    .map_err(|_| DispatchError::Other("NameTooLong"))?;

            let new_card_info = CardInfo {
                owner: owner.clone(),
                finalized: true,
                slot_values: Some([n, e, s, w]),
                // required by the front end
                name: name_bv,
                north: n,
                east: e,
                south: s,
                west: w,
                // existing fields
                card_id,
                minted_at: <frame_system::Pallet<T>>::block_number(),
                price: 0u128,
                edition: CardEdition::Base,
                rarity: RarityType::Common,
            };

            Cards::<T>::insert(card_id, new_card_info);
            NextCardId::<T>::put(card_id + 1);

            Ok(card_id)
        }
    }
}
