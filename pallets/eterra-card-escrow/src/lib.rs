#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::duplicated_attributes, clippy::missing_const_for_thread_local)]

pub use pallet::*;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use frame_support::dispatch::DispatchResult;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;

pub type CardId = u32;
pub type GameId = u64;
pub type CardGenomeHash = [u8; 32];

pub trait CardCustodian<AccountId> {
    fn move_card_to_escrow(
        owner: &AccountId,
        escrow_account: &AccountId,
        card_id: CardId,
    ) -> Result<CardGenomeHash, DispatchError>;

    fn move_card_from_escrow(
        escrow_account: &AccountId,
        owner: &AccountId,
        card_id: CardId,
    ) -> DispatchResult;
}

pub trait GameAuthority<AccountId> {
    fn ensure_game_owned_by(game_id: GameId, caller: &AccountId) -> DispatchResult;
    fn ensure_active_game_owned_by(game_id: GameId, caller: &AccountId) -> DispatchResult;
    fn ensure_player_in_game(game_id: GameId, player: &AccountId) -> DispatchResult;
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use frame_support::pallet_prelude::*;
    use frame_support::traits::{Currency, StorageVersion};
    use frame_support::transactional;
    use frame_support::{BoundedBTreeSet, BoundedVec, PalletId};
    use frame_system::pallet_prelude::*;
    use pallet_alpha_access::AccessControl;
    use sp_runtime::traits::{AccountIdConversion, Hash, Saturating};

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);
    const ESCROW_PALLET_ID: PalletId = PalletId(*b"et/cdesc");

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
    pub type EventIdOf<T> = BoundedVec<u8, <T as Config>::MaxEventIdLen>;

    #[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct EscrowEntry<AccountId> {
        pub owner: AccountId,
        pub genome: CardGenomeHash,
        pub reserved_by: Option<GameId>,
        pub withdraw_requested: bool,
    }

    #[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct GameEnemyAssignment<AccountId> {
        pub card_id: CardId,
        pub owner: AccountId,
        pub genome: CardGenomeHash,
        pub enemy_hp: u16,
        pub enemy_color_rgb: [u8; 3],
        pub defeated: bool,
    }

    #[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct ProcessedEnemyDefeatEvent<AccountId, BlockNumber> {
        pub game_id: GameId,
        pub card_id: CardId,
        pub killer: AccountId,
        pub block_number: BlockNumber,
    }

    #[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct ProcessedEnemyEliminationEvent<AccountId, BlockNumber> {
        pub game_id: GameId,
        pub card_id: CardId,
        pub owner: AccountId,
        pub victim: AccountId,
        pub block_number: BlockNumber,
    }

    #[derive(
        Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen, Default,
    )]
    pub struct CardEscrowStats<Balance> {
        pub games_placed: u32,
        pub eliminations: u32,
        pub total_earned: Balance,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type AccessControl: pallet_alpha_access::AccessControl<Self::AccountId>;

        type Currency: Currency<Self::AccountId>;

        #[pallet::constant]
        type RewardAmount: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type MaxEscrowedPerOwner: Get<u32>;

        #[pallet::constant]
        type MaxReservedPerGame: Get<u32>;

        #[pallet::constant]
        type MaxEventIdLen: Get<u32>;

        type CardCustodian: crate::CardCustodian<Self::AccountId>;
        type GameAuthority: crate::GameAuthority<Self::AccountId>;
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn escrowed_by_owner)]
    pub type EscrowedByOwner<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedBTreeSet<CardId, T::MaxEscrowedPerOwner>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn escrow_entry)]
    pub type EscrowEntries<T: Config> =
        StorageMap<_, Blake2_128Concat, CardId, EscrowEntry<T::AccountId>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn card_lifetime_stats)]
    pub type CardLifetimeStats<T: Config> =
        StorageMap<_, Blake2_128Concat, CardId, CardEscrowStats<BalanceOf<T>>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn card_latest_escrow_stats)]
    pub type CardLatestEscrowStats<T: Config> =
        StorageMap<_, Blake2_128Concat, CardId, CardEscrowStats<BalanceOf<T>>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn available_escrow_count)]
    pub type AvailableEscrowCount<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    pub type AvailableCardByIndex<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CardId, OptionQuery>;

    #[pallet::storage]
    pub type AvailableIndexByCard<T: Config> =
        StorageMap<_, Blake2_128Concat, CardId, u32, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn game_enemy_assignments)]
    pub type GameEnemyAssignments<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        GameId,
        BoundedVec<GameEnemyAssignment<T::AccountId>, T::MaxReservedPerGame>,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type ProcessedEnemyDefeatEvents<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        EventIdOf<T>,
        ProcessedEnemyDefeatEvent<T::AccountId, BlockNumberFor<T>>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type ProcessedEnemyEliminationEvents<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        EventIdOf<T>,
        ProcessedEnemyEliminationEvent<T::AccountId, BlockNumberFor<T>>,
        OptionQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        CardDeposited {
            owner: T::AccountId,
            card_id: CardId,
        },
        CardWithdrawn {
            owner: T::AccountId,
            card_id: CardId,
        },
        CardWithdrawQueued {
            owner: T::AccountId,
            card_id: CardId,
            game_id: GameId,
        },
        GameCardsReserved {
            game_id: GameId,
            reserved: u32,
        },
        GameReservationsReleased {
            game_id: GameId,
            released: u32,
            withdrawn: u32,
        },
        EnemyDefeatRewarded {
            game_id: GameId,
            card_id: CardId,
            killer: T::AccountId,
            amount: BalanceOf<T>,
        },
        EnemyEliminationRewarded {
            game_id: GameId,
            card_id: CardId,
            owner: T::AccountId,
            victim: T::AccountId,
            amount: BalanceOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        TooManyEscrowedCards,
        DuplicateCardInRequest,
        CardAlreadyEscrowed,
        CardNotEscrowed,
        NotEscrowOwner,
        CardAlreadyPendingWithdrawal,
        CardNotAssignedToGame,
        EnemyAlreadyDefeated,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::deposit_cards(card_ids.len() as u32))]
        #[transactional]
        pub fn deposit_cards(
            origin: OriginFor<T>,
            card_ids: BoundedVec<CardId, T::MaxEscrowedPerOwner>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&owner)?;
            Self::ensure_distinct_cards(&card_ids)?;

            let existing = EscrowedByOwner::<T>::get(&owner).len() as u32;
            ensure!(
                existing.saturating_add(card_ids.len() as u32) <= T::MaxEscrowedPerOwner::get(),
                Error::<T>::TooManyEscrowedCards
            );

            for card_id in card_ids.into_inner().into_iter() {
                ensure!(
                    !EscrowEntries::<T>::contains_key(card_id),
                    Error::<T>::CardAlreadyEscrowed
                );
                let genome =
                    T::CardCustodian::move_card_to_escrow(&owner, &Self::account_id(), card_id)?;
                EscrowEntries::<T>::insert(
                    card_id,
                    EscrowEntry {
                        owner: owner.clone(),
                        genome,
                        reserved_by: None,
                        withdraw_requested: false,
                    },
                );
                Self::reset_latest_escrow_stats(card_id);
                EscrowedByOwner::<T>::try_mutate(&owner, |cards| -> DispatchResult {
                    cards
                        .try_insert(card_id)
                        .map_err(|_| Error::<T>::TooManyEscrowedCards)?;
                    Ok(())
                })?;
                Self::insert_available_card(card_id);
                Self::deposit_event(Event::CardDeposited {
                    owner: owner.clone(),
                    card_id,
                });
            }

            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::withdraw_cards(card_ids.len() as u32))]
        #[transactional]
        pub fn withdraw_cards(
            origin: OriginFor<T>,
            card_ids: BoundedVec<CardId, T::MaxEscrowedPerOwner>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            T::AccessControl::ensure_whitelisted(&owner)?;
            Self::ensure_distinct_cards(&card_ids)?;

            for card_id in card_ids.into_inner().into_iter() {
                let mut entry =
                    EscrowEntries::<T>::take(card_id).ok_or(Error::<T>::CardNotEscrowed)?;
                ensure!(entry.owner == owner, Error::<T>::NotEscrowOwner);

                if let Some(game_id) = entry.reserved_by {
                    ensure!(
                        !entry.withdraw_requested,
                        Error::<T>::CardAlreadyPendingWithdrawal
                    );
                    entry.withdraw_requested = true;
                    EscrowEntries::<T>::insert(card_id, entry);
                    Self::deposit_event(Event::CardWithdrawQueued {
                        owner: owner.clone(),
                        card_id,
                        game_id,
                    });
                    continue;
                }

                Self::remove_available_card(card_id);
                Self::remove_owner_card(&owner, card_id);
                T::CardCustodian::move_card_from_escrow(&Self::account_id(), &owner, card_id)?;
                Self::deposit_event(Event::CardWithdrawn {
                    owner: owner.clone(),
                    card_id,
                });
            }

            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::record_enemy_defeat_with_event_id())]
        pub fn record_enemy_defeat_with_event_id(
            origin: OriginFor<T>,
            game_id: GameId,
            event_id: EventIdOf<T>,
            killer: T::AccountId,
            card_id: CardId,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            if let Some(processed) = ProcessedEnemyDefeatEvents::<T>::get(&event_id) {
                T::GameAuthority::ensure_game_owned_by(processed.game_id, &caller)?;
                return Ok(());
            }

            T::GameAuthority::ensure_active_game_owned_by(game_id, &caller)?;
            T::GameAuthority::ensure_player_in_game(game_id, &killer)?;

            GameEnemyAssignments::<T>::try_mutate(
                game_id,
                |assignments| -> Result<(), DispatchError> {
                    let assignment = assignments
                        .iter_mut()
                        .find(|assignment| assignment.card_id == card_id)
                        .ok_or(Error::<T>::CardNotAssignedToGame)?;
                    ensure!(!assignment.defeated, Error::<T>::EnemyAlreadyDefeated);
                    assignment.defeated = true;
                    Ok(())
                },
            )?;

            let reward = T::RewardAmount::get();
            let _ = T::Currency::deposit_creating(&killer, reward);
            ProcessedEnemyDefeatEvents::<T>::insert(
                event_id,
                ProcessedEnemyDefeatEvent {
                    game_id,
                    card_id,
                    killer: killer.clone(),
                    block_number: <frame_system::Pallet<T>>::block_number(),
                },
            );
            Self::deposit_event(Event::EnemyDefeatRewarded {
                game_id,
                card_id,
                killer,
                amount: reward,
            });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::record_enemy_elimination_with_event_id())]
        pub fn record_enemy_elimination_with_event_id(
            origin: OriginFor<T>,
            game_id: GameId,
            event_id: EventIdOf<T>,
            card_id: CardId,
            victim: T::AccountId,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            if let Some(processed) = ProcessedEnemyEliminationEvents::<T>::get(&event_id) {
                T::GameAuthority::ensure_game_owned_by(processed.game_id, &caller)?;
                return Ok(());
            }

            T::GameAuthority::ensure_active_game_owned_by(game_id, &caller)?;
            T::GameAuthority::ensure_player_in_game(game_id, &victim)?;

            let assignments = GameEnemyAssignments::<T>::get(game_id);
            let Some(assignment) = assignments
                .iter()
                .find(|assignment| assignment.card_id == card_id)
            else {
                return Err(Error::<T>::CardNotAssignedToGame.into());
            };
            ensure!(!assignment.defeated, Error::<T>::EnemyAlreadyDefeated);
            let owner = assignment.owner.clone();

            let reward = T::RewardAmount::get();
            let _ = T::Currency::deposit_creating(&owner, reward);
            Self::record_owner_elimination_stats(card_id, reward);
            ProcessedEnemyEliminationEvents::<T>::insert(
                event_id,
                ProcessedEnemyEliminationEvent {
                    game_id,
                    card_id,
                    owner: owner.clone(),
                    victim: victim.clone(),
                    block_number: <frame_system::Pallet<T>>::block_number(),
                },
            );
            Self::deposit_event(Event::EnemyEliminationRewarded {
                game_id,
                card_id,
                owner,
                victim,
                amount: reward,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn account_id() -> T::AccountId {
            ESCROW_PALLET_ID.into_account_truncating()
        }

        pub fn handle_game_created(game_id: GameId) -> DispatchResult {
            let reserve_count = AvailableEscrowCount::<T>::get().min(T::MaxReservedPerGame::get());
            let mut assignments: BoundedVec<
                GameEnemyAssignment<T::AccountId>,
                T::MaxReservedPerGame,
            > = BoundedVec::default();

            for nonce in 0..reserve_count {
                let available_count = AvailableEscrowCount::<T>::get();
                if available_count == 0 {
                    break;
                }
                let index = Self::random_available_index(game_id, nonce, available_count);
                let card_id = match Self::take_available_card_at(index) {
                    Some(card_id) => card_id,
                    None => continue,
                };
                if let Some(mut entry) = EscrowEntries::<T>::get(card_id) {
                    entry.reserved_by = Some(game_id);
                    let assignment = Self::build_assignment(card_id, &entry);
                    EscrowEntries::<T>::insert(card_id, entry);
                    if assignments.try_push(assignment).is_ok() {
                        Self::record_game_placement_stats(card_id);
                    }
                }
            }

            GameEnemyAssignments::<T>::insert(game_id, assignments.clone());
            Self::deposit_event(Event::GameCardsReserved {
                game_id,
                reserved: assignments.len() as u32,
            });
            Ok(())
        }

        pub fn handle_game_ended(game_id: GameId) {
            let assignments = GameEnemyAssignments::<T>::take(game_id);
            let mut released: u32 = 0;
            let mut withdrawn: u32 = 0;

            for assignment in assignments.into_inner().into_iter() {
                let Some(mut entry) = EscrowEntries::<T>::take(assignment.card_id) else {
                    continue;
                };
                entry.reserved_by = None;
                if entry.withdraw_requested {
                    Self::remove_owner_card(&entry.owner, assignment.card_id);
                    if T::CardCustodian::move_card_from_escrow(
                        &Self::account_id(),
                        &entry.owner,
                        assignment.card_id,
                    )
                    .is_ok()
                    {
                        withdrawn = withdrawn.saturating_add(1);
                        Self::deposit_event(Event::CardWithdrawn {
                            owner: entry.owner.clone(),
                            card_id: assignment.card_id,
                        });
                    } else {
                        EscrowEntries::<T>::insert(assignment.card_id, entry);
                    }
                    continue;
                }

                EscrowEntries::<T>::insert(assignment.card_id, entry);
                Self::insert_available_card(assignment.card_id);
                released = released.saturating_add(1);
            }

            Self::deposit_event(Event::GameReservationsReleased {
                game_id,
                released,
                withdrawn,
            });
        }

        fn ensure_distinct_cards(
            card_ids: &BoundedVec<CardId, T::MaxEscrowedPerOwner>,
        ) -> DispatchResult {
            let mut seen: BoundedBTreeSet<CardId, T::MaxEscrowedPerOwner> = BoundedBTreeSet::new();
            for card_id in card_ids.iter().copied() {
                ensure!(
                    seen.try_insert(card_id).is_ok(),
                    Error::<T>::DuplicateCardInRequest
                );
            }
            Ok(())
        }

        fn build_assignment(
            card_id: CardId,
            entry: &EscrowEntry<T::AccountId>,
        ) -> GameEnemyAssignment<T::AccountId> {
            let hp_seed = u16::from_le_bytes([entry.genome[0], entry.genome[1]]);
            GameEnemyAssignment {
                card_id,
                owner: entry.owner.clone(),
                genome: entry.genome,
                enemy_hp: 100u16.saturating_add(hp_seed % 401),
                enemy_color_rgb: [entry.genome[2], entry.genome[3], entry.genome[4]],
                defeated: false,
            }
        }

        fn reset_latest_escrow_stats(card_id: CardId) {
            CardLatestEscrowStats::<T>::insert(card_id, CardEscrowStats::<BalanceOf<T>>::default());
        }

        fn record_game_placement_stats(card_id: CardId) {
            CardLatestEscrowStats::<T>::mutate(card_id, |stats| {
                stats.games_placed = stats.games_placed.saturating_add(1);
            });
            CardLifetimeStats::<T>::mutate(card_id, |stats| {
                stats.games_placed = stats.games_placed.saturating_add(1);
            });
        }

        fn record_owner_elimination_stats(card_id: CardId, reward: BalanceOf<T>) {
            CardLatestEscrowStats::<T>::mutate(card_id, |stats| {
                stats.eliminations = stats.eliminations.saturating_add(1);
                stats.total_earned = stats.total_earned.saturating_add(reward);
            });
            CardLifetimeStats::<T>::mutate(card_id, |stats| {
                stats.eliminations = stats.eliminations.saturating_add(1);
                stats.total_earned = stats.total_earned.saturating_add(reward);
            });
        }

        fn random_available_index(game_id: GameId, nonce: u32, available_count: u32) -> u32 {
            let subject = (
                b"eterra-card-escrow/reserve",
                game_id,
                nonce,
                <frame_system::Pallet<T>>::block_number(),
                <frame_system::Pallet<T>>::parent_hash(),
            )
                .encode();
            let hash = T::Hashing::hash(&subject);
            let bytes = hash.as_ref();
            let random = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if available_count == 0 {
                0
            } else {
                random % available_count
            }
        }

        fn insert_available_card(card_id: CardId) {
            let index = AvailableEscrowCount::<T>::get();
            AvailableCardByIndex::<T>::insert(index, card_id);
            AvailableIndexByCard::<T>::insert(card_id, index);
            AvailableEscrowCount::<T>::put(index.saturating_add(1));
        }

        fn remove_available_card(card_id: CardId) {
            if let Some(index) = AvailableIndexByCard::<T>::get(card_id) {
                let _ = Self::take_available_card_at(index);
            }
        }

        fn take_available_card_at(index: u32) -> Option<CardId> {
            let count = AvailableEscrowCount::<T>::get();
            if count == 0 || index >= count {
                return None;
            }

            let last_index = count.saturating_sub(1);
            let card_id = AvailableCardByIndex::<T>::take(index)?;

            if index != last_index {
                if let Some(last_card) = AvailableCardByIndex::<T>::take(last_index) {
                    AvailableCardByIndex::<T>::insert(index, last_card);
                    AvailableIndexByCard::<T>::insert(last_card, index);
                }
            }

            AvailableIndexByCard::<T>::remove(card_id);
            AvailableEscrowCount::<T>::put(last_index);
            Some(card_id)
        }

        fn remove_owner_card(owner: &T::AccountId, card_id: CardId) {
            EscrowedByOwner::<T>::mutate(owner, |cards| {
                cards.remove(&card_id);
            });
        }
    }
}
