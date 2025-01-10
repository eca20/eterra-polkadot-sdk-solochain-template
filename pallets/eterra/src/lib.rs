#![cfg_attr(not(feature = "std"), no_std)]

use sp_io::hashing;
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::Hash;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	pub type Board = [[Option<Card>; 4]; 4];

	#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, Debug)]
	pub struct Card {
		top: u8,
		right: u8,
		bottom: u8,
		left: u8,
	}

  impl Card {
    pub fn new(top: u8, right: u8, bottom: u8, left: u8) -> Self {
        Self { top, right, bottom, left }
    }
}

	#[pallet::storage]
	#[pallet::getter(fn game_board)]
  pub type GameBoard<T: Config> = StorageMap<
      _, 
      Blake2_128Concat, 
      T::Hash, 
      (Board, T::AccountId, T::AccountId) // Store the board and both players
  >;

	#[pallet::storage]
	#[pallet::getter(fn current_turn)]
	pub type CurrentTurn<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, T::AccountId>;

	#[pallet::storage]
	#[pallet::getter(fn scores)]
	pub type Scores<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, (u8, u8)>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		GameCreated { game_id: T::Hash },
		TurnPlayed { game_id: T::Hash, player: T::AccountId, x: u8, y: u8 },
		GameFinished { game_id: T::Hash, winner: Option<T::AccountId> },
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
		pub fn create_game(
			origin: OriginFor<T>,
			opponent: T::AccountId,
		) -> DispatchResult {
			let creator = ensure_signed(origin)?;

			let game_id = T::Hashing::hash_of(&(creator.clone(), opponent.clone()));
			ensure!(!GameBoard::<T>::contains_key(&game_id), Error::<T>::GameNotFound);

			let initial_board: Board = Default::default();
      GameBoard::<T>::insert(&game_id, (initial_board, creator.clone(), opponent.clone()));

			// Randomly determine the first turn
			let first_turn = if sp_io::hashing::blake2_128(&creator.encode())[0] % 2 == 0 {
				creator.clone()
			} else {
				opponent.clone()
			};
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

    log::debug!("Player {:?} is attempting to play on game_id {:?}", who, game_id);

    ensure!(GameBoard::<T>::contains_key(&game_id), Error::<T>::GameNotFound);
    let (mut board, creator, opponent) = GameBoard::<T>::get(&game_id).unwrap();
    let current_turn = CurrentTurn::<T>::get(&game_id).unwrap();

    log::debug!("Current turn belongs to: {:?}", current_turn);

    ensure!(current_turn == who, Error::<T>::NotYourTurn);
    ensure!(x < 4 && y < 4, Error::<T>::InvalidMove);
    ensure!(board[x as usize][y as usize].is_none(), Error::<T>::CellOccupied);

    board[x as usize][y as usize] = Some(card.clone());

    log::debug!("Board updated before capture: {:?}", board);

    // Capture logic
    let mut scores = Scores::<T>::get(&game_id).unwrap_or((0, 0));
    for &(dx, dy, opposing_rank) in &[
        (0, -1, card.top),    // Top
        (1, 0, card.right),   // Right
        (0, 1, card.bottom),  // Bottom
        (-1, 0, card.left),   // Left
    ] {
        let nx = x as isize + dx;
        let ny = y as isize + dy;
        if nx >= 0 && nx < 4 && ny >= 0 && ny < 4 {
            if let Some(opposing_card) = &board[nx as usize][ny as usize] {
                let rank = match (dx, dy) {
                    (0, -1) => opposing_card.bottom,
                    (1, 0) => opposing_card.left,
                    (0, 1) => opposing_card.top,
                    (-1, 0) => opposing_card.right,
                    _ => 0,
                };
                if opposing_rank > rank {
                    // Capture card
                    scores.0 += 1; // Increment current player's score
                    scores.1 -= 1; // Decrement opponent's score
                    board[nx as usize][ny as usize] = Some(card.clone());
                    log::debug!("Captured card at ({}, {})", nx, ny);
                }
            }
        }
    }

    log::debug!("Board updated after capture: {:?}", board);

    // Save the updated board state
    GameBoard::<T>::insert(&game_id, (board.clone(), creator.clone(), opponent.clone()));

    Scores::<T>::insert(&game_id, scores);

    // Update turn
    let next_turn = if current_turn == creator {
        opponent.clone()
    } else {
        creator.clone()
    };

    // Clone `next_turn` for insertion and logging
    CurrentTurn::<T>::insert(&game_id, next_turn.clone());

    log::debug!("Next turn belongs to: {:?}", next_turn);

    // Emit event
    Self::deposit_event(Event::TurnPlayed {
        game_id,
        player: who.clone(),
        x,
        y,
    });

    // Check if game ends
    let total_cells = board.iter().flat_map(|row| row.iter()).filter(|cell| cell.is_some()).count();
    if total_cells == 16 {
        let winner = if scores.0 > scores.1 {
            Some(creator)
        } else if scores.1 > scores.0 {
            Some(opponent)
        } else {
            None
        };
        GameBoard::<T>::remove(&game_id);
        CurrentTurn::<T>::remove(&game_id);
        Scores::<T>::remove(&game_id);

        log::debug!("Game finished. Winner: {:?}", winner);

        Self::deposit_event(Event::GameFinished { game_id, winner });
    }

    Ok(())
}
	}
}