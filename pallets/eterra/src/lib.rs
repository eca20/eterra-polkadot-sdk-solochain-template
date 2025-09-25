#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod types;

pub use crate::types::GameId;
use frame_support::ensure;
use frame_support::traits::Get;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::traits::SaturatedConversion;
pub use types::board::Board;
pub use types::card::Color;
pub use types::card::Card;
pub use types::game::*;
use sp_std::vec::Vec;
use frame_support::pallet_prelude::ConstU32;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Hash;
    use sp_std::vec::Vec;

    pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

    use crate::types::board::Board;
    use crate::types::card::Color;
    use crate::types::game::*;
    use crate::types::GameId;
    use crate::types::card::Card;
    use crate::types::game::Move;
    // Alias the simple TCG pallet so we can read card ownership & stats
    use pallet_eterra_simple_tcg as cards;
    
    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config + cards::pallet::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        // Exact number of players that can join a single game
        #[pallet::constant]
        type NumPlayers: Get<u32> + Clone + TypeInfo;
        #[pallet::constant]
        type MaxRounds: Get<u8>;
        #[pallet::constant]
        type BlocksToPlayLimit: Get<u8>;
        /// Exactly how many cards a submitted hand must contain
        #[pallet::constant]
        type HandSize: Get<u32>;
    }

    #[pallet::storage]
    #[pallet::getter(fn game_board)]
    pub type GameStorage<T: Config> = StorageMap<
        _, // Explicit prefix using the pallet type
        Blake2_128Concat,
        GameId<T>,
        Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>, // Store the complete game struct
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        GameCreated {
            game_id: GameId<T>,
        },
        MovePlayed {
            game_id: GameId<T>,
            player: T::AccountId,
            x: u8,
            y: u8,
        },
        GameFinished {
            game_id: GameId<T>,
            winner: Option<T::AccountId>,
        },
        //New Turn
        NewTurn {
            game_id: GameId<T>,
            next_player: AccountIdOf<T>,
        },
        TurnForceFinished {
            game_id: GameId<T>,
            player: AccountIdOf<T>,
        },
        HandSubmitted {
            game_id: GameId<T>,
            player: AccountIdOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        GameNotFound,
        InvalidMove,
        NotYourTurn,
        CellOccupied,
        InvalidNumberOfPlayers,
        InternalError,
        BlocksToPlayLimitNotPassed,
        CurrentPlayerCannotForceFinishTurn,
        PlayerNotInGame,
        CreatorMustBeInGame,
        // Hand / deck errors
        HandSizeInvalid,
        DuplicateCardInHand,
        HandAlreadySubmitted,
        HandNotSubmitted,
        HandIndexOutOfRange,
        CardAlreadyUsed,
        CardDoesNotExist,
        CardNotOwned,
    }

    /// Limit of cards per hand (defaults to 5 via Config::HandSize)
    pub type HandLimit = ConstU32<5>;

    /// A single entry in a player's submitted hand
    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug)]
    pub struct HandEntry {
        pub card_id: u32,
        pub north: u8,
        pub east: u8,
        pub south: u8,
        pub west: u8,
        pub used: bool,
    }

    /// Stores each player's hand for a given game.
    /// Keyed by (game_id, account_id) -> bounded vec of exactly HandSize entries.
    #[pallet::storage]
    #[pallet::getter(fn game_hands)]
    pub type HandsOfGame<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat, GameId<T>,
        Blake2_128Concat, AccountIdOf<T>,
        BoundedVec<HandEntry, HandLimit>,
        OptionQuery
    >;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn create_game(origin: OriginFor<T>, players: Vec<AccountIdOf<T>>) -> DispatchResult {
            let who: AccountIdOf<T> = ensure_signed(origin)?;

            // If you want to play, you need to specify yourself in the Vec as well
            ensure!(players.contains(&who), Error::<T>::CreatorMustBeInGame);

            let number_of_players = players.len();

            ensure!(
                number_of_players
                    == <u32 as sp_runtime::traits::SaturatedConversion>::saturated_into::<usize>(
                        T::NumPlayers::get()
                    ),
                Error::<T>::InvalidNumberOfPlayers
            );

            let creator = who;
            let opponent = players[1].clone();
            // Prevent creating a game with oneself
            ensure!(creator != opponent, Error::<T>::InvalidMove);
            let current_block_number = <frame_system::Pallet<T>>::block_number();

            let game_id =
                T::Hashing::hash_of(&(creator.clone(), opponent.clone(), current_block_number));
            ensure!(
                !GameStorage::<T>::contains_key(&game_id),
                Error::<T>::GameNotFound
            );

            let initial_board: Board = Default::default();
            // Randomly determine the first turn
            // let first_turn = if sp_io::hashing::blake2_128(&creator.encode())[0] % 2 == 0 {
            //     creator.clone()
            // } else {
            //     opponent.clone()
            // };

            let initial_scores = (5, 5); // Each player starts with 5 points for their unplayed cards
            let player_colors = (Color::Blue, Color::Red);

            let mut game: Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers> = Game {
                state: GameState::Playing,
                last_played_block: current_block_number,
                players: players
                    .clone()
                    .try_into()
                    .map_err(|_| Error::<T>::InternalError)?,
                player_turn: 0,
                round: 0,
                max_rounds: T::MaxRounds::get(),
                board: initial_board.clone(),
                scores: initial_scores,
                player_colors,
            };

            GameStorage::<T>::insert(&game_id, game.clone());

            // Use set_player_turn instead
            game.set_player_turn(
                if sp_io::hashing::blake2_128(&creator.encode())[0] % 2 == 0 {
                    0 // Player 0 starts
                } else {
                    1 // Player 1 starts
                },
            );

            Self::deposit_event(Event::GameCreated { game_id });

            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn play(origin: OriginFor<T>, game_id: GameId<T>, player_move: Move) -> DispatchResult {
            let who = ensure_signed(origin)?;

            log::debug!(
                "Player {:?} is attempting to play on game_id {:?} at {:?}",
                who,
                game_id,
                player_move
            );

            let mut game = GameStorage::<T>::get(&game_id).ok_or(Error::<T>::GameNotFound)?;

            // Validate the current player's turn and move
            Self::validate_player_turn(&game, &who)?;
            Self::validate_move(&game, &player_move)?;

            // Get the player's color
            let current_color = Self::get_current_color(&game, &who);

            // Place the card on the board
            Self::place_card_on_board(&mut game, &player_move, current_color.clone());

            // Capture logic
            Self::apply_capture_logic(&mut game, &player_move, current_color.clone());

            // Update the last_played_block to the current block number
            let current_block = <frame_system::Pallet<T>>::block_number();
            game.last_played_block = current_block;

            // Check if the game is won
            // if let Some(winner) = Self::is_game_won(&game_id, &game) {
            //     Self::end_game(&game_id, winner);
            //     return Ok(());
            // }

            // Update to the next turn
            game.next_turn();

            log::debug!(
                "Saving game state after next_turn. Current round: {}, player_turn: {}",
                game.round,
                game.player_turn
            );

            // Emit a NewTurn event for the new current player
            let next_player = game.players[game.get_player_turn() as usize].clone();
            Self::deposit_event(Event::NewTurn {
                game_id,
                next_player,
            });

            // Save the updated game
            GameStorage::<T>::insert(&game_id, game.clone());

            // Check if the game is won after updating the round
            if let Some(winner) = Self::is_game_won(&game_id, &game) {
                Self::end_game(&game_id, winner);
                return Ok(());
            }

            log::debug!(
                "Next turn belongs to: {:?}",
                game.players[game.get_player_turn() as usize]
            );

            Self::deposit_event(Event::MovePlayed {
                game_id,
                player: who,
                x: player_move.place_index_x,
                y: player_move.place_index_y,
            });

            Ok(())
        }

        /// Submit a 5-card hand for this game. All cards must be owned by the caller.
        #[pallet::call_index(2)]
        #[pallet::weight(10_000)]
        pub fn submit_hand(origin: OriginFor<T>, game_id: GameId<T>, card_ids: Vec<u32>) -> DispatchResult {
            let who: AccountIdOf<T> = ensure_signed(origin)?;

            // Ensure the game exists and the caller is a player in it
            let game = GameStorage::<T>::get(&game_id).ok_or(Error::<T>::GameNotFound)?;
            ensure!(game.players.contains(&who), Error::<T>::PlayerNotInGame);

            // Enforce exact hand size and uniqueness
            ensure!(card_ids.len() as u32 == T::HandSize::get(), Error::<T>::HandSizeInvalid);
            // Check duplicates (O(n^2) but tiny n=5)
            for i in 0..card_ids.len() {
                for j in (i+1)..card_ids.len() {
                    ensure!(card_ids[i] != card_ids[j], Error::<T>::DuplicateCardInHand);
                }
            }

            // Prevent resubmission
            ensure!(HandsOfGame::<T>::get(&game_id, &who).is_none(), Error::<T>::HandAlreadySubmitted);

            // Build hand entries from the cards pallet; validate ownership & existence
            let mut hand: BoundedVec<HandEntry, HandLimit> = BoundedVec::default();
            for card_id in card_ids.into_iter() {
                let info = cards::pallet::Cards::<T>::get(card_id).ok_or(Error::<T>::CardDoesNotExist)?;
                ensure!(info.owner == who, Error::<T>::CardNotOwned);
                let entry = HandEntry { card_id, north: info.north, east: info.east, south: info.south, west: info.west, used: false };
                hand.try_push(entry).map_err(|_| Error::<T>::HandSizeInvalid)?;
            }

            HandsOfGame::<T>::insert(&game_id, &who, hand);
            Self::deposit_event(Event::HandSubmitted { game_id, player: who });
            Ok(())
        }

        /// Play a card by referencing its index in the submitted hand (0..HandSize-1).
        #[pallet::call_index(3)]
        #[pallet::weight(10_000)]
        pub fn play_from_hand(
            origin: OriginFor<T>,
            game_id: GameId<T>,
            hand_index: u8,
            x: u8,
            y: u8,
        ) -> DispatchResult {
            let who: AccountIdOf<T> = ensure_signed(origin)?;

            // Load game
            let mut game = GameStorage::<T>::get(&game_id).ok_or(Error::<T>::GameNotFound)?;

            // Validate it's the caller's turn and the target cell is open
            Self::validate_player_turn(&game, &who)?;
            ensure!(x < 4 && y < 4, Error::<T>::InvalidMove);
            ensure!(game.board[x as usize][y as usize].is_none(), Error::<T>::CellOccupied);

            // Get caller's hand
            let mut hand = HandsOfGame::<T>::get(&game_id, &who).ok_or(Error::<T>::HandNotSubmitted)?;
            let idx = hand_index as usize;
            ensure!(idx < hand.len(), Error::<T>::HandIndexOutOfRange);
            ensure!(!hand[idx].used, Error::<T>::CardAlreadyUsed);

            // Build the placed card from the saved stats
            let current_color = Self::get_current_color(&game, &who);
            let h = hand[idx].clone();
            let placed = Card { top: h.north, right: h.east, bottom: h.south, left: h.west, color: None };
            let mv = Move { place_card: placed, place_index_x: x, place_index_y: y };

            // Place the card and resolve capture logic (mirrors `play`)
            Self::place_card_on_board(&mut game, &mv, current_color.clone());
            Self::apply_capture_logic(&mut game, &mv, current_color.clone());

            // Mark card as used and persist the hand
            hand[idx].used = true;
            HandsOfGame::<T>::insert(&game_id, &who, hand);

            // Update timing and turn
            let current_block = <frame_system::Pallet<T>>::block_number();
            game.last_played_block = current_block;
            game.next_turn();

            // Emit events and save game
            let next_player = game.players[game.get_player_turn() as usize].clone();
            Self::deposit_event(Event::NewTurn { game_id, next_player });
            GameStorage::<T>::insert(&game_id, game.clone());

            // Check for win condition after saving
            if let Some(winner) = Self::is_game_won(&game_id, &game) {
                Self::end_game(&game_id, winner);
                return Ok(());
            }

            Self::deposit_event(Event::MovePlayed { game_id, player: who, x, y });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(10_000 + T::DbWeight::get().reads_writes(1, 1).ref_time())]
        pub fn force_finish_turn(origin: OriginFor<T>, game_id: GameId<T>) -> DispatchResult {
            let who: AccountIdOf<T> = ensure_signed(origin)?;

            // Retrieve the game from storage
            let mut game = GameStorage::<T>::get(&game_id).ok_or(Error::<T>::GameNotFound)?;

            // Ensure the caller is a player in the game
            ensure!(game.players.contains(&who), Error::<T>::PlayerNotInGame);

            // Ensure the caller is not the current player
            let current_player = game.players[game.player_turn as usize].clone();
            ensure!(
                current_player != who,
                Error::<T>::CurrentPlayerCannotForceFinishTurn
            );

            // Check if the BlocksToPlayLimit has passed
            let current_block = <frame_system::Pallet<T>>::block_number();
            ensure!(
                game.last_played_block + T::BlocksToPlayLimit::get().into() < current_block,
                Error::<T>::BlocksToPlayLimitNotPassed
            );

            // Force finish the current turn
            game.next_turn();
            game.last_played_block = current_block;

            log::debug!(
                "Force finish turn: game_id {:?}, current round: {}, max rounds: {}",
                game_id,
                game.round,
                game.max_rounds
            );

            // ✅ Check if game is won after forcing turn
            if let Some(winner) = Self::is_game_won(&game_id, &game) {
                Self::end_game(&game_id, winner);
                return Ok(()); // Stop execution if game is finished
            }

            // Save the updated game state
            GameStorage::<T>::insert(&game_id, game.clone());

            // Emit events
            let next_player = game.players[game.player_turn as usize].clone();
            Self::deposit_event(Event::TurnForceFinished {
                game_id,
                player: current_player,
            });
            Self::deposit_event(Event::NewTurn {
                game_id,
                next_player,
            });

            Ok(())
        }
    }
    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_finalize(_n: BlockNumberFor<T>) {
            Self::force_idle_turns();
        }
    }
}

// Helper methods
impl<T: Config> Pallet<T> {
    // Function to force idle turns to be played, preventing zombie games
    // from a case where both users are not taking turns
    pub fn force_idle_turns()
    where
        BlockNumberFor<T>: From<u32>,
    {
        let current_block: BlockNumberFor<T> = <frame_system::Pallet<T>>::block_number();

        // Convert BlocksToPlayLimit safely using saturated_into
        let blocks_to_wait =
            BlockNumberFor::<T>::from(T::BlocksToPlayLimit::get().saturated_into::<u32>())
                * 2u32.into();

        for (game_id, mut game) in GameStorage::<T>::iter() {
            if game.last_played_block + blocks_to_wait < current_block {
                let current_player = game.players[game.player_turn as usize].clone();
                game.next_turn();
                game.last_played_block = current_block;

                GameStorage::<T>::insert(&game_id, game.clone());

                let next_player = game.players[game.player_turn as usize].clone();
                Self::deposit_event(Event::TurnForceFinished {
                    game_id,
                    player: current_player,
                });
                Self::deposit_event(Event::NewTurn {
                    game_id,
                    next_player,
                });
            }
        }
    }

    fn is_game_won(
        game_id: &GameId<T>,
        game: &Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
    ) -> Option<Option<T::AccountId>> {
        log::debug!(
            "Checking if game is won. Current round: {}, Max rounds: {}",
            game.round,
            game.max_rounds
        );

        // Ensure game is still in storage before checking win conditions
        if !GameStorage::<T>::contains_key(game_id) {
            log::warn!(
                "Warning: Attempted to check is_game_won() on a removed game: {:?}",
                game_id
            );
            return None;
        }

        if game.round >= game.max_rounds {
            log::debug!("Max rounds reached. Determining winner...");
        } else {
            log::debug!("Game continues. Not at max rounds yet.");
            return None;
        }

        // Determine winner
        let (score_player_0, score_player_1) = game.scores;
        let winner = if score_player_0 > score_player_1 {
            Some(game.players[0].clone())
        } else if score_player_1 > score_player_0 {
            Some(game.players[1].clone())
        } else {
            None // Draw
        };

        log::debug!(
            "Game ID: {:?}, Scores: {:?}, Winner: {:?}",
            game_id,
            game.scores,
            winner
        );

        Some(winner)
    }
    fn validate_player_turn(
        game: &Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
        who: &AccountIdOf<T>,
    ) -> Result<(), Error<T>> {
        let current_turn_index = game.get_player_turn();
        let current_turn = game.players[current_turn_index as usize].clone();
        ensure!(current_turn == *who, Error::<T>::NotYourTurn);
        Ok(())
    }

    fn validate_move(
        game: &Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
        player_move: &Move,
    ) -> Result<(), Error<T>> {
        ensure!(
            player_move.place_index_x < 4 && player_move.place_index_y < 4,
            Error::<T>::InvalidMove
        );
        ensure!(
            game.board[player_move.place_index_x as usize][player_move.place_index_y as usize]
                .is_none(),
            Error::<T>::CellOccupied
        );
        Ok(())
    }

    fn get_current_color(
        game: &Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
        who: &AccountIdOf<T>,
    ) -> Color {
        if who == &game.players[0] {
            game.player_colors.0.clone()
        } else {
            game.player_colors.1.clone()
        }
    }

    fn place_card_on_board(
        game: &mut Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
        player_move: &Move,
        current_color: Color,
    ) {
        let placed_card = player_move
            .place_card
            .clone()
            .with_color(current_color.clone());
        game.board[player_move.place_index_x as usize][player_move.place_index_y as usize] =
            Some(placed_card);
    }

    fn apply_capture_logic(
        game: &mut Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
        player_move: &Move,
        current_color: Color,
    ) {
        for &(dx, dy, opposing_rank) in &[
            (0, -1, player_move.place_card.top),   // Top
            (1, 0, player_move.place_card.right),  // Right
            (0, 1, player_move.place_card.bottom), // Bottom
            (-1, 0, player_move.place_card.left),  // Left
        ] {
            let nx = player_move.place_index_x as isize + dx;
            let ny = player_move.place_index_y as isize + dy;
            if nx >= 0 && nx < 4 && ny >= 0 && ny < 4 {
                if let Some(mut opposing_card) = game.board[nx as usize][ny as usize].clone() {
                    let rank = match (dx, dy) {
                        (0, -1) => opposing_card.bottom,
                        (1, 0) => opposing_card.left,
                        (0, 1) => opposing_card.top,
                        (-1, 0) => opposing_card.right,
                        _ => 0,
                    };
                    if opposing_rank > rank {
                        log::debug!("Captured card at ({}, {})", nx, ny);

                        // Update scores
                        if let Some(color) = opposing_card.color {
                            if color == game.player_colors.0 {
                                game.scores.0 = game.scores.0.saturating_sub(1);
                            } else if color == game.player_colors.1 {
                                game.scores.1 = game.scores.1.saturating_sub(1);
                            }
                        }
                        if current_color == game.player_colors.0 {
                            game.scores.0 = game.scores.0.saturating_add(1);
                        } else {
                            game.scores.1 = game.scores.1.saturating_add(1);
                        }

                        // Change ownership of the card
                        opposing_card.color = Some(current_color.clone());
                        game.board[nx as usize][ny as usize] = Some(opposing_card);
                    }
                }
            }
        }
    }

    fn end_game(game_id: &GameId<T>, winner: Option<T::AccountId>) {
        Self::deposit_event(Event::GameFinished {
            game_id: *game_id,
            winner: winner.clone(),
        });

        log::debug!("Game finished. Winner: {:?}", winner);

        GameStorage::<T>::remove(game_id);
    }
}
