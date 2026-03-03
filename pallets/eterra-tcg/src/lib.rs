// TODO: Add limited card storage, 600 cards?
// TODO: Add ability to add storage for 50 tokens
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ExistenceRequirement, Get},
    BoundedVec,
};
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::traits::{Hash, SaturatedConversion};
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use frame_support::traits::ConstU32;
    use frame_support::transactional;
    use frame_system::pallet_prelude::BlockNumberFor;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    /// Balance type bound to the runtime currency.
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let mut weight = T::DbWeight::get().reads(1);
            if StorageVersion::get::<Pallet<T>>() < STORAGE_VERSION {
                STORAGE_VERSION.put::<Pallet<T>>();
                weight = weight.saturating_add(T::DbWeight::get().writes(1));
            }
            weight
        }
    }

    // ------------------
    // Pallet Config
    // ------------------

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Currency used to charge for minting packs.
        type Currency: Currency<Self::AccountId>;

        /// Fixed pack mint price (in native `COIN` base units).
        #[pallet::constant]
        type PackPrice: Get<BalanceOf<Self>>;

        /// Account that receives pack mint payments.
        #[pallet::constant]
        type PackPriceReceiver: Get<Self::AccountId>;

        /// Fixed "pro" mint price (in native `COIN` base units).
        #[pallet::constant]
        type ProPrice: Get<BalanceOf<Self>>;

        /// Account that receives "pro" mint payments.
        #[pallet::constant]
        type ProPriceReceiver: Get<Self::AccountId>;

        /// Maximum number of spins allowed for a "pro" card mint.
        #[pallet::constant]
        type MaxProSpins: Get<u8>;

        /// The maximum times a card can generate slots before it is forced to finalize.
        #[pallet::constant]
        type MaxAttempts: Get<u8>;

        /// How many cards are in each newly minted pack.
        #[pallet::constant]
        type CardsPerPack: Get<u8>;

        /// The maximum number of packs a single account can hold.
        #[pallet::constant]
        type MaxPacks: Get<u32>;

        /// Weight information for this pallet's extrinsics.
        type WeightInfo: WeightInfo;
    }

    // ------------------
    // Data Structures
    // ------------------

    /// The info stored about each card.
    #[derive(Clone, Encode, Decode, Default, PartialEq, TypeInfo, MaxEncodedLen, Debug)]
    pub struct CardInfo<AccountId> {
        owner: AccountId,
        finalized: bool,
        /// Directional ranks in `[north, east, south, west]` order.
        slot_values: Option<[u8; 4]>,
    }

    impl<AccountId> CardInfo<AccountId> {
        pub fn get_owner(&self) -> &AccountId {
            &self.owner
        }

        pub fn is_finalized(&self) -> bool {
            self.finalized
        }

        pub fn get_slot_values(&self) -> Option<[u8; 4]> {
            self.slot_values
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
    pub type Cards<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CardInfo<T::AccountId>, OptionQuery>;

    /// A map from account => list of packs
    #[pallet::storage]
    #[pallet::getter(fn player_packs)]
    pub type PlayerPacks<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BoundedVec<Pack, T::MaxPacks>, ValueQuery>;

    /// Tracks the currently “active” card index (within a pack) for each account
    #[pallet::storage]
    #[pallet::getter(fn active_card)]
    pub type ActiveCard<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, Option<u8>, ValueQuery>;

    /// Stores the attempt count for each card: `card_id => current attempts`.
    /// We omit the account ID here because the card can be traded to another owner.
    #[pallet::storage]
    #[pallet::getter(fn card_attempts)]
    pub type CardAttempts<T: Config> = StorageMap<_, Blake2_128Concat, u32, u8, ValueQuery>;

    /// Tracks the caller's currently in-progress "pro mint" card ID, if any.
    #[pallet::storage]
    #[pallet::getter(fn pro_in_progress)]
    pub type ProInProgress<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, OptionQuery>;

    // ------------------
    // Events
    // ------------------

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A new pack was minted for `player` with ID `pack_id`, containing multiple new cards.
        PackMinted { player: T::AccountId, pack_id: u32 },
        /// A card’s slot was generated.
        SlotGenerated { card_id: u32, values: [u8; 4] },
        /// A card’s slot was accepted (finalized).
        SlotAccepted { card_id: u32 },
        /// A card was finalized (forced finalize).
        SlotFinalized { card_id: u32 },
        /// A pack was completed (all cards finalized).
        PackCompleted { player: T::AccountId, pack_id: u32 },
        /// A card was transferred from `from` to `to`.
        CardTransferred {
            from: T::AccountId,
            to: T::AccountId,
            card_id: u32,
        },

        /// A new "pro" card was started for `player` with global `card_id`.
        ProMintStarted { player: T::AccountId, card_id: u32 },
        /// A "pro" card spin generated new directional ranks.
        ProSpin {
            card_id: u32,
            values: [u8; 4],
            spin: u8,
        },
        /// A "pro" card was accepted (finalized) with its current ranks.
        ProMintAccepted {
            player: T::AccountId,
            card_id: u32,
            values: [u8; 4],
        },
        /// A "pro" card hit the max spins and was finalized automatically.
        ProMintForcedFinalized {
            player: T::AccountId,
            card_id: u32,
            values: [u8; 4],
        },
    }

    // ------------------
    // Errors
    // ------------------

    #[pallet::error]
    pub enum Error<T> {
        /// Card attempts exceeded `MaxAttempts`.
        MaxAttemptsExceeded,
        /// No active card found for the user in the current pack context.
        NoActiveCard,
        /// Pack is already completed, no further changes allowed.
        PackAlreadyCompleted,
        /// The user has no pack to operate on.
        NoPackFound,
        /// The user’s pack limit is reached.
        MaxPacksReached,
        /// Card does not exist in storage.
        NoSuchCard,
        /// You do not own the card you’re trying to act upon.
        NotCardOwner,
        /// The card was already finalized and cannot be mutated.
        CardAlreadyFinalized,
        /// No more card IDs are available.
        CardIdExhausted,

        /// A "pro" mint is already in progress for this account.
        ProMintAlreadyInProgress,
        /// No "pro" mint is currently in progress for this account.
        NoProMintInProgress,
        /// Pro spins exceeded `MaxProSpins`.
        MaxProSpinsExceeded,
        /// Pro card has no spin values to accept yet.
        ProCardNotSpun,
    }

    // ------------------
    // Calls (Extrinsics)
    // ------------------

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Mint a new pack of cards for the caller, up to `MaxPacks`.
        ///
        /// Charges `PackPrice` (in native `COIN`) and mints `CardsPerPack` unique card IDs.
        /// Each card is stored globally in `Cards<T>`.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::mint_pack())]
        #[transactional]
        pub fn mint_pack(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            let mut packs = PlayerPacks::<T>::get(&player);
            ensure!(
                packs.len() < T::MaxPacks::get() as usize,
                Error::<T>::MaxPacksReached
            );

            // Charge the pack price up-front.
            let price = T::PackPrice::get();
            let receiver = T::PackPriceReceiver::get();
            T::Currency::transfer(&player, &receiver, price, ExistenceRequirement::KeepAlive)?;

            let pack_id = <frame_system::Pallet<T>>::block_number().saturated_into::<u32>();

            // Build a new pack with references to newly minted card IDs
            let mut card_ids: BoundedVec<u32, ConstU32<16>> = BoundedVec::default();

            for _ in 0..T::CardsPerPack::get() {
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
                completed: false,
            };

            packs
                .try_push(new_pack)
                .map_err(|_| Error::<T>::MaxPacksReached)?;

            PlayerPacks::<T>::insert(&player, packs);
            ActiveCard::<T>::insert(&player, Some(0));

            Self::deposit_event(Event::PackMinted { player, pack_id });
            Ok(())
        }

        /// Generate new slot values for the user’s current (active) card, up to `MaxAttempts`.
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::generate_slot())]
        #[transactional]
        pub fn generate_slot(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            // 1) Find the user’s last minted pack
            PlayerPacks::<T>::mutate(&player, |packs| -> DispatchResult {
                let pack = packs.last_mut().ok_or(Error::<T>::NoPackFound)?;
                ensure!(!pack.completed, Error::<T>::PackAlreadyCompleted);

                // 2) Get the active card index
                let active_card_idx =
                    ActiveCard::<T>::get(&player).ok_or(Error::<T>::NoActiveCard)?;
                let card_id = *pack
                    .card_ids
                    .get(active_card_idx as usize)
                    .ok_or(Error::<T>::NoActiveCard)?;

                // 3) Check ownership
                let mut card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == player, Error::<T>::NotCardOwner);
                ensure!(!card_info.finalized, Error::<T>::CardAlreadyFinalized);

                // 4) Check attempts
                let mut attempts = CardAttempts::<T>::get(card_id);
                ensure!(
                    attempts < T::MaxAttempts::get(),
                    Error::<T>::MaxAttemptsExceeded
                );

                // Derive deterministic ranks from on-chain entropy + (player, card_id, attempts).
                let values = Self::spin_values(&player, card_id, attempts, b"eterra-tcg/slot");

                // 6) Update card’s slot values
                card_info.slot_values = Some(values);

                // 7) Store back
                Cards::<T>::insert(card_id, card_info);

                // 8) Increment attempts
                attempts += 1;
                CardAttempts::<T>::insert(card_id, attempts);

                // 9) If attempts == max, finalize now
                if attempts == T::MaxAttempts::get() {
                    Self::finalize_card_and_advance(&player, card_id, pack, active_card_idx)?;
                }

                Self::deposit_event(Event::SlotGenerated { card_id, values });
                Ok(())
            })?;

            Ok(())
        }

        /// Accept (finalize) the user’s current card’s slot values immediately.
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::accept_slot())]
        #[transactional]
        pub fn accept_slot(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            PlayerPacks::<T>::mutate(&player, |packs| -> DispatchResult {
                let pack = packs.last_mut().ok_or(Error::<T>::NoPackFound)?;
                ensure!(!pack.completed, Error::<T>::PackAlreadyCompleted);
                let active_card_idx =
                    ActiveCard::<T>::get(&player).ok_or(Error::<T>::NoActiveCard)?;
                let card_id = *pack
                    .card_ids
                    .get(active_card_idx as usize)
                    .ok_or(Error::<T>::NoActiveCard)?;

                // Must have a card
                let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == player, Error::<T>::NotCardOwner);
                ensure!(!card_info.finalized, Error::<T>::CardAlreadyFinalized);

                // Must have generated at least once
                ensure!(card_info.slot_values.is_some(), Error::<T>::NoActiveCard);

                // Finalize
                Self::finalize_card_and_advance(&player, card_id, pack, active_card_idx)?;

                Self::deposit_event(Event::SlotAccepted { card_id });
                Ok(())
            })?;

            Ok(())
        }

        /// **New**: Transfer a single card from `origin` to `to`.
        /// If that card is also part of a pack, it still references it, but ownership
        /// changes to `to`.
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::transfer_card())]
        #[transactional]
        pub fn transfer_card(
            origin: OriginFor<T>,
            card_id: u32,
            to: T::AccountId,
        ) -> DispatchResult {
            let from = ensure_signed(origin)?;

            Cards::<T>::mutate(card_id, |maybe_card| -> DispatchResult {
                let card_info = maybe_card.as_mut().ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == from, Error::<T>::NotCardOwner);

                // ✅ Ensure the card is finalized before allowing transfer
                ensure!(card_info.finalized, Error::<T>::NoActiveCard); // Consider a better error name

                // Transfer ownership
                card_info.owner = to.clone();

                Ok(())
            })?;

            Self::deposit_event(Event::CardTransferred { from, to, card_id });
            Ok(())
        }

        /// Start a new "pro" mint: pay `ProPrice`, mint a single in-progress card,
        /// then use `spin_pro` (up to `MaxProSpins`) to generate ranks and `accept_pro` to finalize.
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::mint_pro())]
        #[transactional]
        pub fn mint_pro(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            ensure!(
                !ProInProgress::<T>::contains_key(&player),
                Error::<T>::ProMintAlreadyInProgress
            );

            // Charge the pro price up-front.
            let price = T::ProPrice::get();
            let receiver = T::ProPriceReceiver::get();
            T::Currency::transfer(&player, &receiver, price, ExistenceRequirement::KeepAlive)?;

            // Create the in-progress card.
            let card_id = Self::create_new_card(&player)?;
            ProInProgress::<T>::insert(&player, card_id);
            Self::deposit_event(Event::ProMintStarted {
                player: player.clone(),
                card_id,
            });

            Ok(())
        }

        /// Spin the "pro" card in progress, up to `MaxProSpins`.
        /// Updates the in-progress card's directional ranks.
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::spin_pro())]
        #[transactional]
        pub fn spin_pro(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            let card_id =
                ProInProgress::<T>::get(&player).ok_or(Error::<T>::NoProMintInProgress)?;

            let (values, spins_used, forced_finalized) = Self::do_pro_spin(&player, card_id)?;

            if forced_finalized {
                Self::deposit_event(Event::ProMintForcedFinalized {
                    player,
                    card_id,
                    values,
                });
            } else {
                Self::deposit_event(Event::ProSpin {
                    card_id,
                    values,
                    spin: spins_used,
                });
            }

            Ok(())
        }

        /// Accept (finalize) the current "pro" card with whatever values are currently set.
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::accept_pro())]
        #[transactional]
        pub fn accept_pro(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            let card_id =
                ProInProgress::<T>::get(&player).ok_or(Error::<T>::NoProMintInProgress)?;

            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == player, Error::<T>::NotCardOwner);
            ensure!(!card_info.finalized, Error::<T>::CardAlreadyFinalized);
            let values = card_info.slot_values.ok_or(Error::<T>::ProCardNotSpun)?;

            // Finalize the card and clear pro state.
            Self::finalize_pro_card(&player, card_id)?;

            Self::deposit_event(Event::ProMintAccepted {
                player,
                card_id,
                values,
            });
            Ok(())
        }
    }

    // ------------------
    // Pallet Internals
    // ------------------

    impl<T: Config> Pallet<T> {
        /// Create a brand-new card with `owner`.
        fn create_new_card(owner: &T::AccountId) -> Result<u32, DispatchError> {
            let card_id = NextCardId::<T>::get();
            let next_card_id = card_id.checked_add(1).ok_or(Error::<T>::CardIdExhausted)?;
            let new_card_info = CardInfo {
                owner: owner.clone(),
                finalized: false,
                slot_values: None,
            };

            Cards::<T>::insert(card_id, new_card_info);
            NextCardId::<T>::put(next_card_id);

            Ok(card_id)
        }

        /// Generate new ranks for a card based on on-chain entropy + (player, card_id, attempts).
        fn spin_values(
            player: &T::AccountId,
            card_id: u32,
            attempts: u8,
            domain: &'static [u8],
        ) -> [u8; 4] {
            let parent_hash = <frame_system::Pallet<T>>::parent_hash();
            let ext_index = <frame_system::Pallet<T>>::extrinsic_index().unwrap_or(0);
            let now = <frame_system::Pallet<T>>::block_number();

            let subject = (
                domain,
                now,
                parent_hash,
                ext_index,
                player,
                card_id,
                attempts,
            )
                .encode();
            let hash = T::Hashing::hash(&subject);
            let bytes = hash.as_ref();

            // Map bytes into a small "rank" range (1..=9) for game-friendly stats.
            let to_rank = |b: u8| -> u8 { (b % 9).saturating_add(1) };
            [
                to_rank(bytes.get(0).copied().unwrap_or(0)),
                to_rank(bytes.get(1).copied().unwrap_or(0)),
                to_rank(bytes.get(2).copied().unwrap_or(0)),
                to_rank(bytes.get(3).copied().unwrap_or(0)),
            ]
        }

        /// Execute a single pro spin, updating storage. Returns:
        /// - values (new ranks)
        /// - spins_used (after increment)
        /// - forced_finalized (true if this spin hit max and finalized)
        fn do_pro_spin(
            player: &T::AccountId,
            card_id: u32,
        ) -> Result<([u8; 4], u8, bool), DispatchError> {
            // Validate card ownership and in-progress state.
            let mut card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == *player, Error::<T>::NotCardOwner);
            ensure!(!card_info.finalized, Error::<T>::CardAlreadyFinalized);

            let mut spins_used = CardAttempts::<T>::get(card_id);
            ensure!(
                spins_used < T::MaxProSpins::get(),
                Error::<T>::MaxProSpinsExceeded
            );

            let values = Self::spin_values(player, card_id, spins_used, b"eterra-tcg/pro-spin");
            card_info.slot_values = Some(values);
            Cards::<T>::insert(card_id, card_info);

            spins_used = spins_used.saturating_add(1);
            CardAttempts::<T>::insert(card_id, spins_used);

            if spins_used == T::MaxProSpins::get() {
                // Auto-finalize on the last allowed spin.
                Self::finalize_pro_card(player, card_id)?;
                return Ok((values, spins_used, true));
            }

            Ok((values, spins_used, false))
        }

        fn finalize_pro_card(player: &T::AccountId, card_id: u32) -> DispatchResult {
            Cards::<T>::mutate(card_id, |maybe_card| -> DispatchResult {
                let card_info = maybe_card.as_mut().ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == *player, Error::<T>::NotCardOwner);
                ensure!(!card_info.finalized, Error::<T>::CardAlreadyFinalized);
                card_info.finalized = true;
                Ok(())
            })?;

            CardAttempts::<T>::remove(card_id);
            ProInProgress::<T>::remove(player);
            Ok(())
        }

        /// Finalize a card, remove attempts, possibly mark the pack completed, etc.
        fn internal_finalize_card(card_id: u32, pack: &mut Pack) -> DispatchResult {
            // 1) Mark card as finalized, remove attempts
            if let Some(mut card_info) = Cards::<T>::get(card_id) {
                card_info.finalized = true;
                Cards::<T>::insert(card_id, card_info);
            }

            CardAttempts::<T>::remove(card_id);

            // 2) Emit
            Self::deposit_event(Event::SlotFinalized { card_id });

            // 3) If this was the last card in the pack, set `pack.completed = true`.
            //    We'll check if all of them are finalized:
            let all_final = pack
                .card_ids
                .iter()
                .all(|id| Cards::<T>::get(*id).map(|c| c.finalized).unwrap_or(true));
            if all_final {
                pack.completed = true;
                // The user might be stored somewhere else, so we can’t easily remove
                // it here. But if the user minted the pack, they're the pack "owner".
                // If you want to store pack ownership, you'd embed that in `Pack` too.

                // For demonstration, we just say the pack is completed, but not
                // removed from the user’s `PlayerPacks`.
                // If you want an event:
                // Self::deposit_event(Event::PackCompleted {
                //   player: ???,
                //   pack_id: pack.id
                // });
            }

            Ok(())
        }

        /// Finalize the current card and advance the active card index (or complete the pack).
        fn finalize_card_and_advance(
            player: &T::AccountId,
            card_id: u32,
            pack: &mut Pack,
            active_card_idx: u8,
        ) -> DispatchResult {
            Self::internal_finalize_card(card_id, pack)?;

            let mut next_idx: Option<u8> = None;
            let start = (active_card_idx as usize).saturating_add(1);
            let len = pack.card_ids.len();

            for i in start..len {
                let cid = pack.card_ids[i];
                if let Some(info) = Cards::<T>::get(cid) {
                    if !info.finalized {
                        next_idx = Some(i as u8);
                        break;
                    }
                }
            }

            if next_idx.is_none() {
                for i in 0..start.min(len) {
                    let cid = pack.card_ids[i];
                    if let Some(info) = Cards::<T>::get(cid) {
                        if !info.finalized {
                            next_idx = Some(i as u8);
                            break;
                        }
                    }
                }
            }

            if let Some(idx) = next_idx {
                pack.active_card_index = idx;
                ActiveCard::<T>::insert(player, Some(idx));
            } else {
                pack.completed = true;
                ActiveCard::<T>::insert(player, Option::<u8>::None);
                Self::deposit_event(Event::PackCompleted {
                    player: player.clone(),
                    pack_id: pack.id,
                });
            }

            Ok(())
        }
    }
}
