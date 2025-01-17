#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod types;

// Publicly re-export the Card and Color types for usage in other files
pub use crate::types::GameId;
use frame_support::ensure;
use frame_system::pallet_prelude::BlockNumberFor;
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
        type NumPlayers: Get<u32> + Clone + TypeInfo;
        #[pallet::constant]
        type MaxRounds: Get<u8>;
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
            // Randomly determine the first turn
            let first_turn = if sp_io::hashing::blake2_128(&creator.encode())[0] % 2 == 0 {
                creator.clone()
            } else {
                opponent.clone()
            };

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
            // Assign colors
            let player_colors = (Color::Blue, Color::Red);

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

            Self::validate_player_turn(&game, &who)?;
            Self::validate_move(&game, &player_move)?;

            let current_color = Self::get_current_color(&game, &who);

            // Place the card on the board
            Self::place_card_on_board(&mut game, &player_move, current_color.clone());

            // Capture logic
            Self::apply_capture_logic(&mut game, &player_move, current_color.clone());

            // Check if the game is won
            if let Some(winner) = Self::is_game_won(&game_id, &game) {
                Self::end_game(&game_id, winner);
                return Ok(());
            }

            // Update to the next turn
            game.next_turn();

            // Check if the game is won after updating the round
            if let Some(winner) = Self::is_game_won(&game_id, &game) {
                Self::end_game(&game_id, winner);
                return Ok(());
            }

            // Save the updated game
            GameStorage::<T>::insert(&game_id, game.clone());

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
    }
}

// Helper methods
impl<T: Config> Pallet<T> {
    fn is_game_won(
        game_id: &GameId<T>,
        game: &Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
    ) -> Option<Option<T::AccountId>> {
        if game.round < game.max_rounds {
            return None; // Game is not yet finished
        }

        // Determine the winner using scores
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
