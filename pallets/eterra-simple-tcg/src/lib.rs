// TODO: Add limited card storage, 600 cards?
// TODO: Add ability to add storage for 50 tokens
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use self::pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use frame_support::traits::{Currency, ExistenceRequirement};
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
    use frame_system::pallet_prelude::BlockNumberFor;

    /// Convenience type aliases for IDs/balance types used in cards.
    pub type CardId = u32;
    pub type Balance = u128;

    /// Balance type bound to the runtime currency.
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    // Max number of cards we track per owner (bounded index)
    pub type OwnedLimit = ConstU32<600>;

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

        /// Currency used to charge the mint fee.
        type Currency: Currency<Self::AccountId>;

        /// Fixed fee to mint a new card (e.g., 100 tokens).
        #[pallet::constant]
        type MintFee: Get<<Self::Currency as Currency<Self::AccountId>>::Balance>;

        /// Faucet account that receives the mint fee.
        #[pallet::constant]
        type FaucetAccount: Get<Self::AccountId>;
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

    /// Index of cards owned by each account.
    #[pallet::storage]
    #[pallet::getter(fn owned_cards)]
    pub type OwnedCards<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BoundedVec<u32, OwnedLimit>, ValueQuery>;

    /// A map of cards that are up for sale: card_id => price.
    #[pallet::storage]
    #[pallet::getter(fn card_prices)]
    pub type CardPrices<T: Config> =
        StorageMap<_, Blake2_128Concat, CardId, BalanceOf<T>, OptionQuery>;

    /// Optional: index of cards a given owner has listed (bounded by OwnedLimit for simplicity).
    #[pallet::storage]
    #[pallet::getter(fn listed_by_owner)]
    pub type ListedByOwner<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BoundedVec<CardId, OwnedLimit>, ValueQuery>;

    // ------------------
    // Events
    // ------------------

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A card was minted for `player` with ID `card_id`.
        CardMinted { player: T::AccountId, card_id: u32 },
        /// A card was transferred from `from` to `to`.
        CardTransferred {
            from: T::AccountId,
            to: T::AccountId,
            card_id: u32,
        },
        /// A card was listed for sale by `owner` at `price`.
        CardListed {
            owner: T::AccountId,
            card_id: u32,
            price: BalanceOf<T>,
        },
        /// A card was unlisted (by owner or due to transfer).
        CardUnlisted { owner: T::AccountId, card_id: u32 },
        /// A card was bought by `buyer` from `seller` for `price`.
        CardBought {
            buyer: T::AccountId,
            seller: T::AccountId,
            card_id: u32,
            price: BalanceOf<T>,
        },
    }

    // ------------------
    // Errors
    // ------------------

    #[pallet::error]
    pub enum Error<T> {
        /// Card does not exist in storage.
        NoSuchCard,
        /// You do not own the card you’re trying to act upon.
        NotCardOwner,
        OwnedListFull,
        // --- Match errors ---
        CardNotFinalized,
        /// Card is not listed for sale.
        NotForSale,
        /// Only the current owner may list/unlist.
        NotOwner,
    }

    // ------------------
    // Calls (Extrinsics)
    // ------------------

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Mint a single card for the caller.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn mint_card(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            let card_id = Self::create_new_card(&player)?;
            Self::deposit_event(Event::CardMinted { player, card_id });
            Ok(())
        }

        /// **New**: Transfer a single card from `origin` to `to`.
        /// If that card is also part of a pack, it still references it, but ownership
        /// changes to `to`.
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn transfer_card(
            origin: OriginFor<T>,
            card_id: u32,
            to: T::AccountId,
        ) -> DispatchResult {
            let from = ensure_signed(origin)?;

            // Ensure card exists and belongs to `from`
            Cards::<T>::try_mutate(card_id, |maybe_card| -> DispatchResult {
                let card_info = maybe_card.as_mut().ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == from, Error::<T>::NotCardOwner);
                Ok(())
            })?;

            // Unlist if listed
            if CardPrices::<T>::contains_key(card_id) {
                Self::unlist(card_id, &from);
            }

            // Perform transfer
            Self::do_transfer(&from, &to, card_id)?;

            Self::deposit_event(Event::CardTransferred { from, to, card_id });
            Ok(())
        }

        /// List a card for sale at a fixed `price` (in chain base units).
        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn set_price(
            origin: OriginFor<T>,
            card_id: CardId,
            price: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // Verify ownership
            let is_owner = Cards::<T>::get(card_id)
                .map(|c| c.owner == who)
                .ok_or(Error::<T>::NoSuchCard)?;
            ensure!(is_owner, Error::<T>::NotOwner);

            CardPrices::<T>::insert(card_id, price);
            ListedByOwner::<T>::try_mutate(&who, |v| -> DispatchResult {
                if !v.iter().any(|&id| id == card_id) {
                    if v.len() as u32 >= <OwnedLimit as frame_support::traits::Get<u32>>::get() {
                        return Err(Error::<T>::OwnedListFull.into());
                    }
                    v.try_push(card_id).map_err(|_| Error::<T>::OwnedListFull)?;
                }
                Ok(())
            })?;

            Self::deposit_event(Event::CardListed {
                owner: who,
                card_id,
                price,
            });
            Ok(())
        }

        /// Remove a card from sale.
        #[pallet::call_index(3)]
        #[pallet::weight(10_000)]
        pub fn remove_price(origin: OriginFor<T>, card_id: CardId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // Verify ownership
            let is_owner = Cards::<T>::get(card_id)
                .map(|c| c.owner == who)
                .ok_or(Error::<T>::NoSuchCard)?;
            ensure!(is_owner, Error::<T>::NotOwner);

            // Ensure it was listed
            ensure!(
                CardPrices::<T>::contains_key(card_id),
                Error::<T>::NotForSale
            );

            Self::unlist(card_id, &who);
            Ok(())
        }

        /// Buy a listed card at the asking price.
        #[pallet::call_index(4)]
        #[pallet::weight(10_000)]
        pub fn buy_card(origin: OriginFor<T>, card_id: CardId) -> DispatchResult {
            let buyer = ensure_signed(origin)?;

            // Get price and current owner
            let price = CardPrices::<T>::get(card_id).ok_or(Error::<T>::NotForSale)?;
            let seller = Cards::<T>::get(card_id)
                .map(|c| c.owner)
                .ok_or(Error::<T>::NoSuchCard)?;

            // Prevent self-buy (optional)
            ensure!(seller != buyer, Error::<T>::NotOwner);

            // Transfer funds buyer -> seller
            <T as Config>::Currency::transfer(
                &buyer,
                &seller,
                price,
                ExistenceRequirement::AllowDeath,
            )?;

            // Unlist before transfer (so indices are consistent)
            Self::unlist(card_id, &seller);

            // Transfer ownership seller -> buyer
            Self::do_transfer(&seller, &buyer, card_id)?;

            Self::deposit_event(Event::CardBought {
                buyer,
                seller,
                card_id,
                price,
            });
            Ok(())
        }
    }

    // ------------------
    // Pallet Internals (helpers; not dispatchables)
    // ------------------
    impl<T: Config> Pallet<T> {
        /// Create a brand-new card with `owner`.
        fn create_new_card(owner: &T::AccountId) -> Result<u32, DispatchError> {
            // Charge the mint fee to the caller and send it to the faucet account.
            // This will fail with an error if the caller has insufficient funds.
            let fee: <<T as Config>::Currency as Currency<T::AccountId>>::Balance =
                T::MintFee::get();
            T::Currency::transfer(
                owner,
                &T::FaucetAccount::get(),
                fee,
                ExistenceRequirement::KeepAlive,
            )?;

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

            // Index the new card under the owner
            OwnedCards::<T>::try_mutate(owner, |list| -> Result<(), DispatchError> {
                if list.len() as u32 >= <OwnedLimit as frame_support::traits::Get<u32>>::get() {
                    return Err(Error::<T>::OwnedListFull.into());
                }
                list.try_push(card_id)
                    .map_err(|_| Error::<T>::OwnedListFull)?;
                Ok(())
            })?;

            NextCardId::<T>::put(card_id + 1);

            Ok(card_id)
        }

        /// Internal: remove a card from the marketplace listings, updating indices.
        fn unlist(card_id: CardId, owner: &T::AccountId) {
            // Remove price entry if any
            CardPrices::<T>::remove(card_id);
            // Remove from owner's listed index, if present
            ListedByOwner::<T>::mutate(owner, |v| {
                if let Some(pos) = v.iter().position(|&id| id == card_id) {
                    v.swap_remove(pos);
                }
            });
            // Emit event for UI (best-effort; no error path)
            Self::deposit_event(Event::CardUnlisted {
                owner: owner.clone(),
                card_id,
            });
        }

        /// Internal: transfer ownership from `from` to `to` and ensure indices are updated.
        fn do_transfer(
            from: &T::AccountId,
            to: &T::AccountId,
            card_id: CardId,
        ) -> Result<(), DispatchError> {
            // Update the card owner in main storage (ensures existence and ownership)
            Cards::<T>::try_mutate(card_id, |maybe_card| -> DispatchResult {
                let card_info = maybe_card.as_mut().ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == *from, Error::<T>::NotCardOwner);
                card_info.owner = to.clone();
                Ok(())
            })?;

            // Remove card_id from `from`'s OwnedCards list (if present)
            OwnedCards::<T>::mutate(from, |list| {
                if let Some(pos) = list.iter().position(|&id| id == card_id) {
                    list.swap_remove(pos);
                }
            });

            // Add card_id to `to`'s OwnedCards list (bounded)
            OwnedCards::<T>::try_mutate(to, |list| -> DispatchResult {
                if list.len() as u32 >= <OwnedLimit as frame_support::traits::Get<u32>>::get() {
                    return Err(Error::<T>::OwnedListFull.into());
                }
                list.try_push(card_id)
                    .map_err(|_| Error::<T>::OwnedListFull)?;
                Ok(())
            })?;

            Ok(())
        }
    }
}
