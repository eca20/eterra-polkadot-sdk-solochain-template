#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod types;

// Publicly re-export the Card and Color types for usage in other files
pub use crate::types::GameId;
pub use types::board::Board;
pub use types::card::{Card, Color};
pub use types::game::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Hash;
    use sp_std::vec::Vec;

    pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

    // Import Card and Color types from the crate
    use crate::types::board::Board;
    use crate::types::card::{Card, Color};
    use crate::types::game::*;
    use crate::types::GameId;
    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        // Maximum number of players that can join a single game
        #[pallet::constant]
        type NumPlayers: Get<u32>;
        #[pallet::constant]
        type MaxRounds: Get<u8>;
    }

    #[pallet::storage]
    #[pallet::getter(fn game_board)]
    pub type GameStorage<T: Config> = StorageMap<
        _, // Explicit prefix using the pallet type
        Blake2_128Concat,
        GameId<T>,
        (Board, T::AccountId, T::AccountId), // Store the board and both players
    >;

    #[pallet::storage]
    #[pallet::getter(fn player_colors)]
    pub type PlayerColors<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, (Color, Color)>;

    #[pallet::storage]
    #[pallet::getter(fn moves_played)]
    pub type MovesPlayed<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, u8>;

    #[pallet::storage]
    #[pallet::getter(fn current_turn)]
    pub type CurrentTurn<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, T::AccountId>;

    #[pallet::storage]
    #[pallet::getter(fn scores)]
    pub type Scores<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, (u8, u8)>;

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
    }

    #[pallet::error]
    pub enum Error<T> {
        GameNotFound,
        InvalidMove,
        NotYourTurn,
        CellOccupied,
        InvalidNumberOfPlayers,
        InternalError,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn create_game(origin: OriginFor<T>, players: Vec<AccountIdOf<T>>) -> DispatchResult {
            //let creator = ensure_signed(origin)?;
            let who: AccountIdOf<T> = ensure_signed(origin)?;

            // If you want to play, you need to specify yourself in the Vec as well
            let number_of_players = players.len();

            ensure!(
                number_of_players == T::NumPlayers::get() as usize,
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
            GameStorage::<T>::insert(&game_id, (initial_board, creator.clone(), opponent.clone()));

            // Randomly determine the first turn
            let first_turn = if sp_io::hashing::blake2_128(&creator.encode())[0] % 2 == 0 {
                creator.clone()
            } else {
                opponent.clone()
            };

            // Default Game Config
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
            };

            // Assign colors
            let player_colors = (Color::Blue, Color::Red);
            PlayerColors::<T>::insert(&game_id, player_colors);

            CurrentTurn::<T>::insert(&game_id, first_turn);

            Scores::<T>::insert(&game_id, (0, 0));

            //GameStorage::<T>::set(game_id, Some(game));

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

            ensure!(
                GameStorage::<T>::contains_key(&game_id),
                Error::<T>::GameNotFound
            );
            let (mut board, creator, opponent) = GameStorage::<T>::get(&game_id).unwrap();
            let current_turn = CurrentTurn::<T>::get(&game_id).unwrap();
            let player_colors = PlayerColors::<T>::get(&game_id).unwrap();

            log::debug!("Current turn belongs to: {:?}", current_turn);

            ensure!(current_turn == who, Error::<T>::NotYourTurn);
            ensure!(
                player_move.place_index_x < 4 && player_move.place_index_y < 4,
                Error::<T>::InvalidMove
            );
            ensure!(
                board[player_move.place_index_x as usize][player_move.place_index_y as usize]
                    .is_none(),
                Error::<T>::CellOccupied
            );

            let current_color = if who == creator {
                player_colors.0.clone()
            } else {
                player_colors.1.clone()
            };

            // Place the card with the current player's color
            let placed_card = player_move
                .place_card
                .clone()
                .with_color(current_color.clone());
            board[player_move.place_index_x as usize][player_move.place_index_y as usize] =
                Some(placed_card.clone());

            log::debug!("Board updated before capture: {:?}", board);

            for &(dx, dy, opposing_rank) in &[
                (0, -1, player_move.place_card.top),   // Top
                (1, 0, player_move.place_card.right),  // Right
                (0, 1, player_move.place_card.bottom), // Bottom
                (-1, 0, player_move.place_card.left),  // Left
            ] {
                let nx = player_move.place_index_x as isize + dx;
                let ny = player_move.place_index_y as isize + dy;
                if nx >= 0 && nx < 4 && ny >= 0 && ny < 4 {
                    if let Some(mut opposing_card) = board[nx as usize][ny as usize].clone() {
                        let rank = match (dx, dy) {
                            (0, -1) => opposing_card.bottom,
                            (1, 0) => opposing_card.left,
                            (0, 1) => opposing_card.top,
                            (-1, 0) => opposing_card.right,
                            _ => 0,
                        };
                        if opposing_rank > rank {
                            // Capture card
                            log::debug!("Captured card at ({}, {})", nx, ny);
                            opposing_card.color = Some(current_color.clone());
                            board[nx as usize][ny as usize] = Some(opposing_card);
                        }
                    }
                }
            }

            log::debug!("Board updated after capture: {:?}", board);

            // Save the updated board state
            GameStorage::<T>::insert(&game_id, (board.clone(), creator.clone(), opponent.clone()));

            // Update move counter
            let mut moves = MovesPlayed::<T>::get(&game_id).unwrap_or(0);
            moves += 1;
            MovesPlayed::<T>::insert(&game_id, moves);

            log::debug!("Total moves played: {:?}", moves);

            // Check if the game is won
            if let Some(winner) = Self::is_game_won(&game_id, &board, &creator, &opponent, moves) {
                // Emit game finished event
                Self::deposit_event(Event::GameFinished {
                    game_id,
                    winner: winner.clone(),
                });

                log::debug!("Game finished. Winner: {:?}", winner);

                // Remove game data
                GameStorage::<T>::remove(&game_id);
                CurrentTurn::<T>::remove(&game_id);
                Scores::<T>::remove(&game_id);
                MovesPlayed::<T>::remove(&game_id);

                return Ok(()); // Return early since the game has ended
            }

            // Update turn
            let next_turn = if current_turn == creator {
                opponent.clone()
            } else {
                creator.clone()
            };

            CurrentTurn::<T>::insert(&game_id, next_turn.clone());

            log::debug!("Next turn belongs to: {:?}", next_turn);

            // Emit event for the turn played
            Self::deposit_event(Event::MovePlayed {
                game_id,
                player: who.clone(),
                x: player_move.place_index_x,
                y: player_move.place_index_y,
            });

            Ok(())
        }
    }
}

// Helper methods
impl<T: Config> Pallet<T> {
    fn is_game_won(
        game_id: &GameId<T>,
        board: &Board,
        creator: &T::AccountId,
        opponent: &T::AccountId,
        moves: u8,
    ) -> Option<Option<T::AccountId>> {
        // Check if the game has reached the end condition
        if moves < 10 {
            return None; // Game is not yet finished
        }

        // Count cards of each color
        let mut blue_count = 0;
        let mut red_count = 0;

        for row in board {
            for cell in row {
                if let Some(card) = cell {
                    match card.color {
                        Some(Color::Blue) => blue_count += 1,
                        Some(Color::Red) => red_count += 1,
                        None => {}
                    }
                }
            }
        }

        // Determine the winner
        let winner = if blue_count > red_count {
            Some(creator.clone())
        } else if red_count > blue_count {
            Some(opponent.clone())
        } else {
            None // Draw
        };

        // Log game result
        log::debug!(
            "Game ID: {:?}, Blue Count: {}, Red Count: {}, Winner: {:?}",
            game_id,
            blue_count,
            red_count,
            winner
        );

        Some(winner) // Return wrapped winner to indicate game is finished
    }
}
