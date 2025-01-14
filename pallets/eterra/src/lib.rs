#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
use sp_io::hashing;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod types;

// Publicly re-export the Card and Color types for usage in other files
pub use types::card::{Card, Color};


#[frame_support::pallet]
pub mod pallet {
    use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Hash;

    // Import Card and Color types from the crate
    use crate::types::card::{Card, Color};
    
    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    pub type Board = [[Option<Card>; 4]; 4];

    #[pallet::storage]
    #[pallet::getter(fn game_board)]
    pub type GameBoard<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::Hash,
        (Board, T::AccountId, T::AccountId), // Store the board and both players
    >;

    #[pallet::storage]
    #[pallet::getter(fn player_colors)]
    pub type PlayerColors<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::Hash,
        (Color, Color)
    >;

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
            game_id: T::Hash,
        },
        TurnPlayed {
            game_id: T::Hash,
            player: T::AccountId,
            x: u8,
            y: u8,
        },
        GameFinished {
            game_id: T::Hash,
            winner: Option<T::AccountId>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        GameNotFound,
        InvalidMove,
        NotYourTurn,
        CellOccupied,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn create_game(origin: OriginFor<T>, opponent: T::AccountId) -> DispatchResult {
            let creator = ensure_signed(origin)?;

            // Prevent creating a game with oneself
            ensure!(creator != opponent, Error::<T>::InvalidMove);

            let game_id = T::Hashing::hash_of(&(creator.clone(), opponent.clone()));
            ensure!(
                !GameBoard::<T>::contains_key(&game_id),
                Error::<T>::GameNotFound
            );

            let initial_board: Board = Default::default();
            GameBoard::<T>::insert(&game_id, (initial_board, creator.clone(), opponent.clone()));

            // Randomly determine the first turn
            let first_turn = if sp_io::hashing::blake2_128(&creator.encode())[0] % 2 == 0 {
                creator.clone()
            } else {
                opponent.clone()
            };

            // Assign colors
            let player_colors = (Color::Blue, Color::Red);
            PlayerColors::<T>::insert(&game_id, player_colors);

            CurrentTurn::<T>::insert(&game_id, first_turn);

            Scores::<T>::insert(&game_id, (0, 0));

            Self::deposit_event(Event::GameCreated { game_id });

            Ok(())
        }
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)]
        pub fn play_turn(
            origin: OriginFor<T>,
            game_id: T::Hash,
            x: u8,
            y: u8,
            card: Card,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            log::debug!(
                "Player {:?} is attempting to play on game_id {:?}",
                who,
                game_id
            );

            ensure!(
                GameBoard::<T>::contains_key(&game_id),
                Error::<T>::GameNotFound
            );
            let (mut board, creator, opponent) = GameBoard::<T>::get(&game_id).unwrap();
            let current_turn = CurrentTurn::<T>::get(&game_id).unwrap();
            let player_colors = PlayerColors::<T>::get(&game_id).unwrap();

            log::debug!("Current turn belongs to: {:?}", current_turn);

            ensure!(current_turn == who, Error::<T>::NotYourTurn);
            ensure!(x < 4 && y < 4, Error::<T>::InvalidMove);
            ensure!(
                board[x as usize][y as usize].is_none(),
                Error::<T>::CellOccupied
            );

            let current_color = if who == creator {
                player_colors.0.clone()
            } else {
                player_colors.1.clone()
            };

            // Place the card with the current player's color
            let placed_card = card.clone().with_color(current_color.clone());
            board[x as usize][y as usize] = Some(placed_card.clone());

            log::debug!("Board updated before capture: {:?}", board);

            for &(dx, dy, opposing_rank) in &[
                (0, -1, card.top),   // Top
                (1, 0, card.right),  // Right
                (0, 1, card.bottom), // Bottom
                (-1, 0, card.left),  // Left
            ] {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
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
            GameBoard::<T>::insert(&game_id, (board.clone(), creator.clone(), opponent.clone()));

            // Update move counter
            let mut moves = MovesPlayed::<T>::get(&game_id).unwrap_or(0);
            moves += 1;
            MovesPlayed::<T>::insert(&game_id, moves);

            log::debug!("Total moves played: {:?}", moves);

            // Check for end-of-game condition (10 moves)
            if moves == 10 {
                // Count cards of each color
                let mut blue_count = 0;
                let mut red_count = 0;

                for row in &board {
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

                // Emit game finished event
                Self::deposit_event(Event::GameFinished {
                    game_id,
                    winner: winner.clone(), // Clone the winner to prevent move
                });

                log::debug!("Game finished. Winner: {:?}", winner);

                // Remove game data
                GameBoard::<T>::remove(&game_id);
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
            Self::deposit_event(Event::TurnPlayed {
                game_id,
                player: who.clone(),
                x,
                y,
            });

            Ok(())
        }
    }
}
