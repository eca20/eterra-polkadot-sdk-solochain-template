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
    BoundedBTreeSet,
    BoundedVec,
    PalletId,
};
use frame_system::{ensure_signed, pallet_prelude::OriginFor};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::traits::{AccountIdConversion, Hash, SaturatedConversion};
use sp_std::prelude::*;

pub type MediaId = pallet_eterra_media::MediaId;
pub type SeasonId = pallet_eterra_seasons::SeasonId;

/// Provides a runtime-defined view of whether a given `card_id` is currently included
/// in `owner`'s configured "current hand".
///
/// This is used to prevent listing/selling/transferring a card that is actively in use
/// by gameplay, avoiding dangling card IDs in the player's current hand.
pub trait HandChecker<AccountId> {
    /// Returns `true` if `card_id` is present in `owner`'s current hand.
    fn is_card_in_current_hand(owner: &AccountId, card_id: u32) -> bool;
}

impl<AccountId> HandChecker<AccountId> for () {
    fn is_card_in_current_hand(_owner: &AccountId, _card_id: u32) -> bool {
        false
    }
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum AssetKind {
    Border,
    Background,
    Subject,
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct SeasonAssetsInfo<BBorders, BBackgrounds, BSubjects> {
    pub borders: BBorders,
    pub backgrounds: BBackgrounds,
    pub subjects: BSubjects,
}

impl<BBorders: Default, BBackgrounds: Default, BSubjects: Default> Default
    for SeasonAssetsInfo<BBorders, BBackgrounds, BSubjects>
{
    fn default() -> Self {
        Self {
            borders: Default::default(),
            backgrounds: Default::default(),
            subjects: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct CardArtworkInfo {
    pub season_id: SeasonId,
    pub border_media_id: MediaId,
    pub background_media_id: MediaId,
    pub subject_media_id: MediaId,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use frame_support::traits::ConstU32;
    use frame_support::transactional;
    use frame_system::pallet_prelude::BlockNumberFor;
    use sp_runtime::traits::StaticLookup;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);
    const ESCROW_PALLET_ID: PalletId = PalletId(*b"et/tcgsc");

    /// Balance type bound to the runtime currency.
    pub type BalanceOf<T> =
        <<T as Config>::PaymentCurrency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    type BoundedBorders<T> = BoundedVec<MediaId, <T as Config>::MaxBorders>;
    type BoundedBackgrounds<T> = BoundedVec<MediaId, <T as Config>::MaxBackgrounds>;
    type BoundedSubjects<T> = BoundedVec<MediaId, <T as Config>::MaxSubjects>;
    type SeasonAssetsInfoOf<T> =
        SeasonAssetsInfo<BoundedBorders<T>, BoundedBackgrounds<T>, BoundedSubjects<T>>;

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
    pub trait Config:
        frame_system::Config
        + pallet_eterra_seasons::Config
        + pallet_eterra_media::Config
        + pallet_nfts::Config<CollectionId = u32, ItemId = u32>
    {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Currency used to charge for minting packs.
        type PaymentCurrency: Currency<Self::AccountId>;

        /// A runtime-provided hook for checking whether a card is currently part of the owner's
        /// gameplay "current hand".
        type HandChecker: crate::HandChecker<Self::AccountId>;

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

        /// Fixed single-card mint price (in native `COIN` base units).
        #[pallet::constant]
        type MintCardPrice: Get<BalanceOf<Self>>;

        /// Account that receives single-card mint payments.
        #[pallet::constant]
        type MintCardPriceReceiver: Get<Self::AccountId>;

        /// Maximum number of spins allowed for a "pro" card mint.
        #[pallet::constant]
        type MaxProSpins: Get<u8>;

        /// The maximum times a card can generate slots before it is forced to finalize.
        #[pallet::constant]
        type MaxAttempts: Get<u8>;

        /// How many cards are in each newly minted pack.
        #[pallet::constant]
        type CardsPerPack: Get<u8>;

        /// The maximum number of cards a single account can own.
        ///
        /// This bounds storage reads for dashboards that list cards by owner.
        #[pallet::constant]
        type MaxOwnedCards: Get<u32>;

        /// Base card capacity available to every account before buying extra storage.
        #[pallet::constant]
        type BaseCardCapacity: Get<u32>;

        /// Slots added per storage upgrade purchase.
        #[pallet::constant]
        type CardCapacityUpgradeAmount: Get<u32>;

        /// Price charged for each storage upgrade purchase.
        #[pallet::constant]
        type CardCapacityUpgradePrice: Get<BalanceOf<Self>>;

        /// Account that receives storage upgrade payments.
        #[pallet::constant]
        type CardCapacityUpgradePriceReceiver: Get<Self::AccountId>;

        /// Maximum number of border layers per season.
        #[pallet::constant]
        type MaxBorders: Get<u32>;

        /// Maximum number of background layers per season.
        #[pallet::constant]
        type MaxBackgrounds: Get<u32>;

        /// Maximum number of subject layers per season.
        #[pallet::constant]
        type MaxSubjects: Get<u32>;

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

    /// Season-scoped lists of available artwork layer MediaIds.
    #[pallet::storage]
    #[pallet::getter(fn season_assets)]
    pub type SeasonAssets<T: Config> =
        StorageMap<_, Blake2_128Concat, SeasonId, SeasonAssetsInfoOf<T>, ValueQuery>;

    /// Immutable assigned artwork for each card: `card_id => CardArtworkInfo`.
    #[pallet::storage]
    #[pallet::getter(fn card_artwork)]
    pub type CardArtwork<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CardArtworkInfo, OptionQuery>;

    /// The NFT collection ID used for converted cards (single collection).
    #[pallet::storage]
    #[pallet::getter(fn card_nft_collection_id)]
    pub type CardNftCollectionId<T: Config> = StorageValue<_, u32, OptionQuery>;

    /// Tracks cards that have been converted to NFTs: `card_id => ()`.
    #[pallet::storage]
    #[pallet::getter(fn converted)]
    pub type Converted<T: Config> = StorageMap<_, Blake2_128Concat, u32, (), OptionQuery>;

    /// Additional card capacity purchased by each account.
    #[pallet::storage]
    #[pallet::getter(fn card_capacity_bonus)]
    pub type CardCapacityBonus<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    /// A map from account => list of currently in-progress packs.
    #[pallet::storage]
    #[pallet::getter(fn player_packs)]
    pub type PlayerPacks<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BoundedVec<Pack, T::MaxOwnedCards>, ValueQuery>;

    /// A map from account => set of owned card IDs.
    ///
    /// This is a secondary index to support efficient front-end queries like
    /// "show me all cards owned by this account", including cards minted via pro minting.
    #[pallet::storage]
    #[pallet::getter(fn cards_by_owner)]
    pub type CardsByOwner<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedBTreeSet<u32, T::MaxOwnedCards>,
        ValueQuery,
    >;

    /// A map of cards that are up for sale: `card_id => price`.
    #[pallet::storage]
    #[pallet::getter(fn card_prices)]
    pub type CardPrices<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, BalanceOf<T>, OptionQuery>;

    /// Index of cards a given owner has listed for sale.
    #[pallet::storage]
    #[pallet::getter(fn listed_by_owner)]
    pub type ListedByOwner<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedBTreeSet<u32, T::MaxOwnedCards>,
        ValueQuery,
    >;

    /// Tracks the currently “active” card index (within a pack) for each account
    #[pallet::storage]
    #[pallet::getter(fn active_card)]
    pub type ActiveCard<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, Option<u8>, ValueQuery>;

    /// Tracks the caller's currently in-progress pack mint, if any.
    ///
    /// This makes it easy for the front end to resume a minting flow after refresh.
    #[pallet::storage]
    #[pallet::getter(fn pack_in_progress)]
    pub type PackInProgress<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, OptionQuery>;

    /// Tracks the caller's currently active card ID within the pack mint in progress, if any.
    #[pallet::storage]
    #[pallet::getter(fn pack_card_in_progress)]
    pub type PackCardInProgress<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, OptionQuery>;

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
        /// A single card was minted for `player` with ID `card_id`.
        CardMinted { player: T::AccountId, card_id: u32 },
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
        /// A card was listed for sale by `owner` at `price`.
        CardListed {
            owner: T::AccountId,
            card_id: u32,
            price: BalanceOf<T>,
        },
        /// A card was unlisted (by owner or due to transfer).
        CardUnlisted { owner: T::AccountId, card_id: u32 },
        /// A listed card was bought by `buyer` from `seller` for `price`.
        CardBought {
            buyer: T::AccountId,
            seller: T::AccountId,
            card_id: u32,
            price: BalanceOf<T>,
        },
        /// A seasonal artwork layer was added.
        SeasonAssetAdded {
            season_id: SeasonId,
            kind: AssetKind,
            media_id: MediaId,
        },
        /// A seasonal artwork layer was removed.
        SeasonAssetRemoved {
            season_id: SeasonId,
            kind: AssetKind,
            media_id: MediaId,
        },
        /// The NFT collection used for converted cards was initialized.
        CardNftCollectionInitialized {
            collection_id: u32,
            admin: T::AccountId,
        },
        /// A card was converted to an NFT (card is escrowed in TCG, NFT is owned by player).
        CardConvertedToNft {
            card_id: u32,
            collection_id: u32,
            item_id: u32,
        },
        /// A card NFT was burned and the card was returned from escrow to the NFT owner.
        CardUnwrappedFromNft { card_id: u32 },
        /// An account bought additional card storage capacity.
        CardCapacityUpgraded {
            player: T::AccountId,
            added_slots: u32,
            new_capacity: u32,
            price_paid: BalanceOf<T>,
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
        /// Card does not exist in storage.
        NoSuchCard,
        /// You do not own the card you’re trying to act upon.
        NotCardOwner,
        /// Card must be finalized before it can be transferred or listed.
        CardNotFinalized,
        /// Card is currently part of the owner's configured "current hand" and cannot be listed,
        /// sold, or transferred until removed from that hand.
        CardInCurrentHand,
        /// The card was already finalized and cannot be mutated.
        CardAlreadyFinalized,
        /// No more card IDs are available.
        CardIdExhausted,
        /// This action would exceed the account's configured card capacity.
        CardCapacityExceeded,
        /// No more card capacity can be purchased because the hard storage ceiling was reached.
        CardCapacityMaxReached,
        /// The caller's owned-card limit is reached.
        MaxOwnedCardsReached,
        /// The caller's listed-card limit is reached.
        MaxListedCardsReached,
        /// Card is not listed for sale.
        NotForSale,
        /// Buyer cannot buy their own card.
        CannotBuyOwnCard,

        /// Caller is not an allowlisted season admin.
        NotSeasonAdmin,
        /// Season does not exist in the seasons pallet.
        UnknownSeason,
        /// Season is not in Draft status.
        SeasonNotDraft,
        /// MediaId not found in the media registry.
        UnknownMedia,
        /// MediaId is deprecated and cannot be used.
        MediaDeprecated,
        /// The seasonal asset list is full for this kind.
        AssetListFull,
        /// The specified MediaId is not present in the seasonal asset list.
        AssetNotFound,
        /// No active season is currently set.
        NoActiveSeason,
        /// One or more seasonal asset lists are empty.
        SeasonAssetsEmpty,
        /// Card artwork has not been assigned for this card.
        CardArtworkMissing,
        /// The card NFT collection has already been initialized.
        NftCollectionAlreadyInitialized,
        /// The card NFT collection is not initialized.
        NftCollectionNotInitialized,
        /// Card is already converted to an NFT.
        CardAlreadyConverted,
        /// Card is not converted to an NFT.
        CardNotConverted,
        /// Card is not held by escrow (unexpected state).
        CardNotEscrowed,
        /// Caller does not own the NFT item.
        NotNftOwner,

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
        /// Mint a new pack of cards for the caller.
        ///
        /// Charges `PackPrice` (in native `COIN`) and mints `CardsPerPack` unique card IDs.
        /// Each card is stored globally in `Cards<T>`.
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::mint_pack())]
        #[transactional]
        pub fn mint_pack(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            let mut packs = PlayerPacks::<T>::get(&player);
            Self::prune_completed_packs(&mut packs);
            Self::ensure_can_receive_cards(&player, u32::from(T::CardsPerPack::get()))?;

            // Charge the pack price up-front.
            let price = T::PackPrice::get();
            let receiver = T::PackPriceReceiver::get();
            T::PaymentCurrency::transfer(
                &player,
                &receiver,
                price,
                ExistenceRequirement::KeepAlive,
            )?;

            let pack_id = <frame_system::Pallet<T>>::block_number().saturated_into::<u32>();

            // Build a new pack with references to newly minted card IDs
            let mut card_ids: BoundedVec<u32, ConstU32<16>> = BoundedVec::default();

            for _ in 0..T::CardsPerPack::get() {
                let new_card_id = Self::create_new_card(&player)?;
                // Attach this card to the pack
                card_ids
                    .try_push(new_card_id)
                    .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;
            }

            let first_card_id = card_ids.get(0).copied();

            let new_pack = Pack {
                id: pack_id,
                card_ids,
                active_card_index: 0,
                completed: false,
            };

            packs
                .try_push(new_pack)
                .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;

            PlayerPacks::<T>::insert(&player, packs);
            ActiveCard::<T>::insert(&player, Some(0));
            PackInProgress::<T>::insert(&player, pack_id);
            // We just minted the pack, so index 0 must exist if `CardsPerPack > 0`.
            if let Some(first) = first_card_id {
                PackCardInProgress::<T>::insert(&player, first);
            }

            Self::deposit_event(Event::PackMinted { player, pack_id });
            Ok(())
        }

        /// Mint a single, immediately-finalized card for the caller.
        ///
        /// Charges `MintCardPrice` (in native `COIN`) and mints exactly one card ID with
        /// deterministic ranks based on on-chain entropy (consensus-safe, not cryptographic RNG).
        #[pallet::call_index(7)]
        #[pallet::weight(<T as Config>::WeightInfo::mint_card())]
        #[transactional]
        pub fn mint_card(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            Self::ensure_can_receive_cards(&player, 1)?;

            // Charge the mint price up-front.
            let price = T::MintCardPrice::get();
            let receiver = T::MintCardPriceReceiver::get();
            T::PaymentCurrency::transfer(
                &player,
                &receiver,
                price,
                ExistenceRequirement::KeepAlive,
            )?;

            let card_id = Self::create_new_finalized_card(&player)?;
            Self::deposit_event(Event::CardMinted { player, card_id });
            Ok(())
        }

        /// Generate new slot values for the user’s current (active) card, up to `MaxAttempts`.
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::generate_slot())]
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
                let pack_completed = if attempts == T::MaxAttempts::get() {
                    Self::finalize_card_and_advance(&player, card_id, pack, active_card_idx)?;
                    pack.completed
                } else {
                    false
                };

                if pack_completed {
                    Self::prune_completed_packs(packs);
                }

                Self::deposit_event(Event::SlotGenerated { card_id, values });
                Ok(())
            })?;

            Ok(())
        }

        /// Accept (finalize) the user’s current card’s slot values immediately.
        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::accept_slot())]
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
                let pack_completed = pack.completed;

                if pack_completed {
                    Self::prune_completed_packs(packs);
                }

                Self::deposit_event(Event::SlotAccepted { card_id });
                Ok(())
            })?;

            Ok(())
        }

        /// **New**: Transfer a single card from `origin` to `to`.
        /// If that card is also part of a pack, it still references it, but ownership
        /// changes to `to`.
        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::transfer_card())]
        #[transactional]
        pub fn transfer_card(
            origin: OriginFor<T>,
            card_id: u32,
            to: T::AccountId,
        ) -> DispatchResult {
            let from = ensure_signed(origin)?;

            // Ensure card exists, is owned, and is finalized before allowing transfer.
            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == from, Error::<T>::NotCardOwner);
            ensure!(card_info.finalized, Error::<T>::CardNotFinalized);
            ensure!(
                !T::HandChecker::is_card_in_current_hand(&from, card_id),
                Error::<T>::CardInCurrentHand
            );

            // If listed, unlist first so indices remain consistent.
            if CardPrices::<T>::contains_key(card_id) {
                Self::unlist(card_id, &from);
            }

            Self::do_transfer(&from, &to, card_id)?;

            Self::deposit_event(Event::CardTransferred { from, to, card_id });
            Ok(())
        }

        /// Start a new "pro" mint: pay `ProPrice`, mint a single in-progress card,
        /// then use `spin_pro` (up to `MaxProSpins`) to generate ranks and `accept_pro` to finalize.
        #[pallet::call_index(4)]
        #[pallet::weight(<T as Config>::WeightInfo::mint_pro())]
        #[transactional]
        pub fn mint_pro(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;
            ensure!(
                !ProInProgress::<T>::contains_key(&player),
                Error::<T>::ProMintAlreadyInProgress
            );
            Self::ensure_can_receive_cards(&player, 1)?;

            // Charge the pro price up-front.
            let price = T::ProPrice::get();
            let receiver = T::ProPriceReceiver::get();
            T::PaymentCurrency::transfer(
                &player,
                &receiver,
                price,
                ExistenceRequirement::KeepAlive,
            )?;

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
        #[pallet::weight(<T as Config>::WeightInfo::spin_pro())]
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
        #[pallet::weight(<T as Config>::WeightInfo::accept_pro())]
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

        /// List a finalized card for sale at a fixed `price` (in native balance units).
        #[pallet::call_index(8)]
        #[pallet::weight(<T as Config>::WeightInfo::set_price())]
        #[transactional]
        pub fn set_price(
            origin: OriginFor<T>,
            card_id: u32,
            price: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == who, Error::<T>::NotCardOwner);
            ensure!(card_info.finalized, Error::<T>::CardNotFinalized);
            ensure!(
                !T::HandChecker::is_card_in_current_hand(&who, card_id),
                Error::<T>::CardInCurrentHand
            );

            CardPrices::<T>::insert(card_id, price);
            ListedByOwner::<T>::try_mutate(&who, |set| -> DispatchResult {
                set.try_insert(card_id)
                    .map_err(|_| Error::<T>::MaxListedCardsReached)?;
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
        #[pallet::call_index(9)]
        #[pallet::weight(<T as Config>::WeightInfo::remove_price())]
        #[transactional]
        pub fn remove_price(origin: OriginFor<T>, card_id: u32) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == who, Error::<T>::NotCardOwner);

            ensure!(CardPrices::<T>::contains_key(card_id), Error::<T>::NotForSale);
            Self::unlist(card_id, &who);
            Ok(())
        }

        /// Buy a listed card at the asking price.
        #[pallet::call_index(10)]
        #[pallet::weight(<T as Config>::WeightInfo::buy_card())]
        #[transactional]
        pub fn buy_card(origin: OriginFor<T>, card_id: u32) -> DispatchResult {
            let buyer = ensure_signed(origin)?;

            let price = CardPrices::<T>::get(card_id).ok_or(Error::<T>::NotForSale)?;
            let seller = Cards::<T>::get(card_id)
                .map(|c| c.owner)
                .ok_or(Error::<T>::NoSuchCard)?;

            ensure!(seller != buyer, Error::<T>::CannotBuyOwnCard);
            ensure!(
                !T::HandChecker::is_card_in_current_hand(&seller, card_id),
                Error::<T>::CardInCurrentHand
            );

            // Transfer funds buyer -> seller.
            T::PaymentCurrency::transfer(
                &buyer,
                &seller,
                price,
                ExistenceRequirement::AllowDeath,
            )?;

            // Unlist before transfer (so indices are consistent).
            Self::unlist(card_id, &seller);

            // Transfer ownership seller -> buyer.
            Self::do_transfer(&seller, &buyer, card_id)?;

            Self::deposit_event(Event::CardBought {
                buyer,
                seller,
                card_id,
                price,
            });
            Ok(())
        }

        /// Add a seasonal artwork layer (border/background/subject) by MediaId.
        ///
        /// The season must exist and be in `Draft` status.
        #[pallet::call_index(11)]
        #[pallet::weight(<T as Config>::WeightInfo::add_season_asset())]
        #[transactional]
        pub fn add_season_asset(
            origin: OriginFor<T>,
            season_id: SeasonId,
            kind: AssetKind,
            media_id: MediaId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            Self::ensure_season_draft(season_id)?;
            Self::ensure_media_valid(media_id)?;

            let inserted = SeasonAssets::<T>::try_mutate(
                season_id,
                |assets| -> Result<bool, DispatchError> {
                    match kind {
                        AssetKind::Border => {
                            if assets.borders.contains(&media_id) {
                                return Ok(false);
                            }
                            assets
                                .borders
                                .try_push(media_id)
                                .map_err(|_| Error::<T>::AssetListFull)?;
                        }
                        AssetKind::Background => {
                            if assets.backgrounds.contains(&media_id) {
                                return Ok(false);
                            }
                            assets
                                .backgrounds
                                .try_push(media_id)
                                .map_err(|_| Error::<T>::AssetListFull)?;
                        }
                        AssetKind::Subject => {
                            if assets.subjects.contains(&media_id) {
                                return Ok(false);
                            }
                            assets
                                .subjects
                                .try_push(media_id)
                                .map_err(|_| Error::<T>::AssetListFull)?;
                        }
                    }
                    Ok(true)
                },
            )?;

            if inserted {
                Self::deposit_event(Event::SeasonAssetAdded {
                    season_id,
                    kind,
                    media_id,
                });
            }
            Ok(())
        }

        /// Remove a seasonal artwork layer by MediaId.
        ///
        /// The season must exist and be in `Draft` status.
        #[pallet::call_index(12)]
        #[pallet::weight(<T as Config>::WeightInfo::remove_season_asset())]
        #[transactional]
        pub fn remove_season_asset(
            origin: OriginFor<T>,
            season_id: SeasonId,
            kind: AssetKind,
            media_id: MediaId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            Self::ensure_season_draft(season_id)?;

            SeasonAssets::<T>::try_mutate(season_id, |assets| -> DispatchResult {
                let removed = match kind {
                    AssetKind::Border => Self::remove_asset_from_list(&mut assets.borders, media_id),
                    AssetKind::Background => {
                        Self::remove_asset_from_list(&mut assets.backgrounds, media_id)
                    }
                    AssetKind::Subject => {
                        Self::remove_asset_from_list(&mut assets.subjects, media_id)
                    }
                };
                ensure!(removed, Error::<T>::AssetNotFound);
                Ok(())
            })?;

            Self::deposit_event(Event::SeasonAssetRemoved {
                season_id,
                kind,
                media_id,
            });
            Ok(())
        }

        /// Buy one configured step of additional card storage capacity.
        #[pallet::call_index(13)]
        #[pallet::weight(<T as Config>::WeightInfo::buy_card_capacity())]
        #[transactional]
        pub fn buy_card_capacity(origin: OriginFor<T>) -> DispatchResult {
            let player = ensure_signed(origin)?;

            let added_slots = T::CardCapacityUpgradeAmount::get();
            let current_bonus = CardCapacityBonus::<T>::get(&player);
            let next_bonus = current_bonus.saturating_add(added_slots);
            let new_capacity = T::BaseCardCapacity::get().saturating_add(next_bonus);
            ensure!(
                new_capacity <= T::MaxOwnedCards::get(),
                Error::<T>::CardCapacityMaxReached
            );

            let price = T::CardCapacityUpgradePrice::get();
            let receiver = T::CardCapacityUpgradePriceReceiver::get();
            T::PaymentCurrency::transfer(
                &player,
                &receiver,
                price,
                ExistenceRequirement::KeepAlive,
            )?;

            CardCapacityBonus::<T>::insert(&player, next_bonus);
            Self::deposit_event(Event::CardCapacityUpgraded {
                player,
                added_slots,
                new_capacity,
                price_paid: price,
            });
            Ok(())
        }

        /// Assign seasonal artwork to previously-minted legacy cards that do not yet have
        /// `CardArtwork` set.
        ///
        /// Uses a separate hash domain to avoid matching the mint-time assignment.
        #[pallet::call_index(14)]
        #[pallet::weight(<T as Config>::WeightInfo::backfill_card_artwork())]
        #[transactional]
        pub fn backfill_card_artwork(
            origin: OriginFor<T>,
            start_card_id: u32,
            limit: u32,
            season_id: SeasonId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            Self::ensure_season_exists(season_id)?;

            // Ensure this season has renderable assets.
            let assets = SeasonAssets::<T>::get(season_id);
            Self::ensure_assets_non_empty(&assets)?;

            let end = start_card_id.saturating_add(limit);
            for card_id in start_card_id..end {
                if Cards::<T>::contains_key(card_id) && !CardArtwork::<T>::contains_key(card_id) {
                    Self::assign_artwork_for_card(
                        card_id,
                        season_id,
                        b"eterra-tcg/art-backfill",
                    )?;
                }
            }
            Ok(())
        }

        /// Initialize the single NFT collection used for converted cards.
        ///
        /// The `nft_admin` account becomes the NFT collection admin (intended to be the media
        /// service signer, so it can later call `nfts.set_metadata` on items).
        #[pallet::call_index(15)]
        #[pallet::weight(<T as Config>::WeightInfo::init_card_nft_collection())]
        #[transactional]
        pub fn init_card_nft_collection(
            origin: OriginFor<T>,
            nft_admin: T::AccountId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_season_admin(&who)?;
            ensure!(
                CardNftCollectionId::<T>::get().is_none(),
                Error::<T>::NftCollectionAlreadyInitialized
            );

            let collection_id = pallet_nfts::NextCollectionId::<T>::get().unwrap_or(0);
            let admin = T::Lookup::unlookup(nft_admin.clone());

            let config = pallet_nfts::CollectionConfig {
                settings: pallet_nfts::CollectionSettings::all_enabled(),
                max_supply: None,
                mint_settings: pallet_nfts::MintSettings::default(),
            };

            pallet_nfts::Pallet::<T>::create(
                frame_system::RawOrigin::Signed(who).into(),
                admin,
                config,
            )?;

            CardNftCollectionId::<T>::put(collection_id);
            Self::deposit_event(Event::CardNftCollectionInitialized {
                collection_id,
                admin: nft_admin,
            });
            Ok(())
        }

        /// Convert a finalized card to an NFT (withdraw model).
        ///
        /// The card is transferred to an escrow account controlled by this pallet, and an NFT
        /// item is minted with `item_id = card_id`.
        #[pallet::call_index(16)]
        #[pallet::weight(<T as Config>::WeightInfo::convert_to_nft())]
        #[transactional]
        pub fn convert_to_nft(origin: OriginFor<T>, card_id: u32) -> DispatchResult
        {
            let who = ensure_signed(origin)?;

            let collection_id =
                CardNftCollectionId::<T>::get().ok_or(Error::<T>::NftCollectionNotInitialized)?;
            ensure!(
                !Converted::<T>::contains_key(card_id),
                Error::<T>::CardAlreadyConverted
            );

            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == who, Error::<T>::NotCardOwner);
            ensure!(card_info.finalized, Error::<T>::CardNotFinalized);
            ensure!(
                !T::HandChecker::is_card_in_current_hand(&who, card_id),
                Error::<T>::CardInCurrentHand
            );
            ensure!(
                CardArtwork::<T>::contains_key(card_id),
                Error::<T>::CardArtworkMissing
            );

            // If listed, unlist first so indices remain consistent.
            if CardPrices::<T>::contains_key(card_id) {
                Self::unlist(card_id, &who);
            }

            let escrow = Self::escrow_account_id();
            Self::do_transfer(&who, &escrow, card_id)?;

            Converted::<T>::insert(card_id, ());

            pallet_nfts::Pallet::<T>::do_mint(
                collection_id,
                card_id,
                None,
                who.clone(),
                pallet_nfts::ItemConfig::default(),
                |_, _| Ok(()),
            )?;

            Self::deposit_event(Event::CardConvertedToNft {
                card_id,
                collection_id,
                item_id: card_id,
            });
            Ok(())
        }

        /// Unwrap a converted card NFT back into a playable TCG card.
        ///
        /// Burns the NFT item and transfers the card out of escrow to the NFT owner.
        #[pallet::call_index(17)]
        #[pallet::weight(<T as Config>::WeightInfo::unwrap_from_nft())]
        #[transactional]
        pub fn unwrap_from_nft(origin: OriginFor<T>, card_id: u32) -> DispatchResult
        {
            let who = ensure_signed(origin)?;

            let collection_id =
                CardNftCollectionId::<T>::get().ok_or(Error::<T>::NftCollectionNotInitialized)?;
            ensure!(
                Converted::<T>::contains_key(card_id),
                Error::<T>::CardNotConverted
            );

            let escrow = Self::escrow_account_id();
            let card_info = Cards::<T>::get(card_id).ok_or(Error::<T>::NoSuchCard)?;
            ensure!(card_info.owner == escrow, Error::<T>::CardNotEscrowed);

            let nft_owner =
                pallet_nfts::Pallet::<T>::owner(collection_id, card_id).ok_or(Error::<T>::NotNftOwner)?;
            ensure!(nft_owner == who, Error::<T>::NotNftOwner);

            pallet_nfts::Pallet::<T>::do_burn(collection_id, card_id, |_| Ok(()))?;
            Converted::<T>::remove(card_id);

            Self::do_transfer(&escrow, &who, card_id)?;

            Self::deposit_event(Event::CardUnwrappedFromNft { card_id });
            Ok(())
        }
    }

    // ------------------
    // Pallet Internals
    // ------------------

    impl<T: Config> Pallet<T> {
        fn escrow_account_id() -> T::AccountId {
            ESCROW_PALLET_ID.into_account_truncating()
        }

        fn owned_card_count(owner: &T::AccountId) -> u32 {
            CardsByOwner::<T>::get(owner).len().saturated_into::<u32>()
        }

        fn owned_card_capacity(owner: &T::AccountId) -> u32 {
            T::BaseCardCapacity::get().saturating_add(CardCapacityBonus::<T>::get(owner))
        }

        fn ensure_can_receive_cards(owner: &T::AccountId, additional_cards: u32) -> DispatchResult {
            let next_total = Self::owned_card_count(owner).saturating_add(additional_cards);
            ensure!(
                next_total <= Self::owned_card_capacity(owner),
                Error::<T>::CardCapacityExceeded
            );
            Ok(())
        }

        fn prune_completed_packs(packs: &mut BoundedVec<Pack, T::MaxOwnedCards>) {
            let retained: Vec<Pack> = packs
                .iter()
                .filter(|pack| !pack.completed)
                .cloned()
                .collect();
            *packs = match retained.try_into() {
                Ok(filtered) => filtered,
                Err(_) => unreachable!("filtered packs cannot exceed original bounded length"),
            };
        }

        fn ensure_season_admin(who: &T::AccountId) -> DispatchResult {
            ensure!(
                pallet_eterra_seasons::Admins::<T>::contains_key(who),
                Error::<T>::NotSeasonAdmin
            );
            Ok(())
        }

        fn ensure_season_exists(season_id: SeasonId) -> DispatchResult {
            ensure!(
                pallet_eterra_seasons::Seasons::<T>::contains_key(season_id),
                Error::<T>::UnknownSeason
            );
            Ok(())
        }

        fn ensure_season_draft(season_id: SeasonId) -> DispatchResult {
            let season =
                pallet_eterra_seasons::Seasons::<T>::get(season_id).ok_or(Error::<T>::UnknownSeason)?;
            ensure!(
                season.status == pallet_eterra_seasons::SeasonStatus::Draft,
                Error::<T>::SeasonNotDraft
            );
            Ok(())
        }

        fn ensure_media_valid(media_id: MediaId) -> DispatchResult {
            let meta =
                pallet_eterra_media::Media::<T>::get(media_id).ok_or(Error::<T>::UnknownMedia)?;
            ensure!(!meta.is_deprecated, Error::<T>::MediaDeprecated);
            Ok(())
        }

        fn ensure_assets_non_empty(assets: &SeasonAssetsInfoOf<T>) -> DispatchResult {
            ensure!(
                !assets.borders.is_empty()
                    && !assets.backgrounds.is_empty()
                    && !assets.subjects.is_empty(),
                Error::<T>::SeasonAssetsEmpty
            );
            Ok(())
        }

        fn remove_asset_from_list<ListLen: Get<u32>>(
            list: &mut BoundedVec<MediaId, ListLen>,
            media_id: MediaId,
        ) -> bool {
            if let Some(pos) = list.iter().position(|&id| id == media_id) {
                list.remove(pos);
                return true;
            }
            false
        }

        fn assign_artwork_from_active_season(card_id: u32) -> DispatchResult {
            let season_id = pallet_eterra_seasons::ActiveSeasonId::<T>::get()
                .ok_or(Error::<T>::NoActiveSeason)?;
            Self::assign_artwork_for_card(card_id, season_id, b"eterra-tcg/art")
        }

        fn assign_artwork_for_card(
            card_id: u32,
            season_id: SeasonId,
            domain: &'static [u8],
        ) -> DispatchResult {
            let assets = SeasonAssets::<T>::get(season_id);
            Self::ensure_assets_non_empty(&assets)?;

            let parent_hash = <frame_system::Pallet<T>>::parent_hash();
            let ext_index = <frame_system::Pallet<T>>::extrinsic_index().unwrap_or(0);
            let now = <frame_system::Pallet<T>>::block_number();

            let subject = (domain, season_id, card_id, now, parent_hash, ext_index).encode();
            let hash = T::Hashing::hash(&subject);
            let bytes = hash.as_ref();

            let border_ix = (bytes.get(0).copied().unwrap_or(0) as usize) % assets.borders.len();
            let bg_ix = (bytes.get(1).copied().unwrap_or(0) as usize) % assets.backgrounds.len();
            let subject_ix = (bytes.get(2).copied().unwrap_or(0) as usize) % assets.subjects.len();

            let border_media_id = *assets
                .borders
                .get(border_ix)
                .expect("borders is non-empty; modulo keeps index in range; qed");
            let background_media_id = *assets
                .backgrounds
                .get(bg_ix)
                .expect("backgrounds is non-empty; modulo keeps index in range; qed");
            let subject_media_id = *assets
                .subjects
                .get(subject_ix)
                .expect("subjects is non-empty; modulo keeps index in range; qed");

            CardArtwork::<T>::insert(
                card_id,
                CardArtworkInfo {
                    season_id,
                    border_media_id,
                    background_media_id,
                    subject_media_id,
                },
            );
            Ok(())
        }

        /// Create a brand-new card with `owner`.
        fn create_new_card(owner: &T::AccountId) -> Result<u32, DispatchError> {
            Self::ensure_can_receive_cards(owner, 1)?;
            let card_id = NextCardId::<T>::get();
            let next_card_id = card_id.checked_add(1).ok_or(Error::<T>::CardIdExhausted)?;
            let new_card_info = CardInfo {
                owner: owner.clone(),
                finalized: false,
                slot_values: None,
            };

            Cards::<T>::insert(card_id, new_card_info);
            CardsByOwner::<T>::try_mutate(owner, |set| -> Result<(), DispatchError> {
                set.try_insert(card_id)
                    .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;
                Ok(())
            })?;
            NextCardId::<T>::put(next_card_id);

            Self::assign_artwork_from_active_season(card_id)?;
            Ok(card_id)
        }

        /// Create a brand-new, immediately-finalized card with `owner`.
        fn create_new_finalized_card(owner: &T::AccountId) -> Result<u32, DispatchError> {
            Self::ensure_can_receive_cards(owner, 1)?;
            let card_id = NextCardId::<T>::get();
            let next_card_id = card_id.checked_add(1).ok_or(Error::<T>::CardIdExhausted)?;

            let values = Self::spin_values(owner, card_id, 0, b"eterra-tcg/mint-card");
            let new_card_info = CardInfo {
                owner: owner.clone(),
                finalized: true,
                slot_values: Some(values),
            };

            Cards::<T>::insert(card_id, new_card_info);
            CardsByOwner::<T>::try_mutate(owner, |set| -> Result<(), DispatchError> {
                set.try_insert(card_id)
                    .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;
                Ok(())
            })?;
            NextCardId::<T>::put(next_card_id);

            Self::assign_artwork_from_active_season(card_id)?;
            Ok(card_id)
        }

        /// Internal: remove a card from marketplace listings, updating indices.
        fn unlist(card_id: u32, owner: &T::AccountId) {
            CardPrices::<T>::remove(card_id);
            ListedByOwner::<T>::mutate(owner, |set| {
                set.remove(&card_id);
            });
            Self::deposit_event(Event::CardUnlisted {
                owner: owner.clone(),
                card_id,
            });
        }

        /// Internal: transfer ownership from `from` to `to` and ensure indices are updated.
        fn do_transfer(from: &T::AccountId, to: &T::AccountId, card_id: u32) -> DispatchResult {
            ensure!(
                !T::HandChecker::is_card_in_current_hand(from, card_id),
                Error::<T>::CardInCurrentHand
            );

            if from != to && *to != Self::escrow_account_id() {
                Self::ensure_can_receive_cards(to, 1)?;
            }

            // Update the card owner in main storage (ensures existence and ownership)
            Cards::<T>::try_mutate(card_id, |maybe_card| -> DispatchResult {
                let card_info = maybe_card.as_mut().ok_or(Error::<T>::NoSuchCard)?;
                ensure!(card_info.owner == *from, Error::<T>::NotCardOwner);
                ensure!(card_info.finalized, Error::<T>::CardNotFinalized);
                card_info.owner = to.clone();
                Ok(())
            })?;

            // Remove card_id from `from`'s CardsByOwner set, then insert into `to`'s.
            CardsByOwner::<T>::mutate(from, |set| {
                set.remove(&card_id);
            });
            CardsByOwner::<T>::try_mutate(to, |set| -> DispatchResult {
                set.try_insert(card_id)
                    .map_err(|_| Error::<T>::MaxOwnedCardsReached)?;
                Ok(())
            })?;

            Ok(())
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
                if let Some(cid) = pack.card_ids.get(idx as usize) {
                    PackCardInProgress::<T>::insert(player, *cid);
                }
            } else {
                pack.completed = true;
                ActiveCard::<T>::insert(player, Option::<u8>::None);
                PackInProgress::<T>::remove(player);
                PackCardInProgress::<T>::remove(player);
                Self::deposit_event(Event::PackCompleted {
                    player: player.clone(),
                    pack_id: pack.id,
                });
            }

            Ok(())
        }
    }
}
