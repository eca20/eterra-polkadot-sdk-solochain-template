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
use sp_runtime::traits::Hash;
pub use types::board::Board;
pub use types::card::Color;
pub use types::card::Card;
pub use types::game::*;
use sp_std::vec::Vec;
use frame_support::pallet_prelude::ConstU32;
use frame_support::BoundedVec;
use eterra_card_ai_adapter::eterra_adapter as ai;
use pallet_eterra_monte_carlo_ai as mc_ai; // reserved for future use

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
    use frame_support::BoundedVec;
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Hash;
    use sp_runtime::Saturating;
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
    use eterra_card_ai_adapter::eterra_adapter as ai;
    use pallet_eterra_monte_carlo_ai as mc_ai; // reserved for future use
    
    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config + cards::pallet::Config + mc_ai::pallet::Config {
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
        /// Special account representing the AI opponent in PvE games
        type AiAccount: Get<Self::AccountId>;
        /// Default AI difficulty (0..=100)
        type AiDifficulty: Get<u8>;
    }

    #[pallet::storage]
    #[pallet::getter(fn game_board)]
    pub type GameStorage<T: Config> = StorageMap<
        _, // Explicit prefix using the pallet type
        Blake2_128Concat,
        GameId<T>,
        Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>, // Store the complete game struct
    >;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug)]
    pub enum GameMode { PvP, PvE }

    #[pallet::storage]
    #[pallet::getter(fn game_mode_of)]
    pub type GameModes<T: Config> = StorageMap<_, Blake2_128Concat, GameId<T>, GameMode, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn active_game_of)]
    /// Tracks if an account is currently in an active game. A player may have at most one.
    pub type ActiveGameOf<T: Config> = StorageMap<_, Blake2_128Concat, AccountIdOf<T>, GameId<T>, OptionQuery>;

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
        PlayerAlreadyInGame,
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
        pub fn create_game(
            origin: OriginFor<T>,
            mut players: Vec<AccountIdOf<T>>,
            game_mode: GameMode,
        ) -> DispatchResult {
            let who: AccountIdOf<T> = ensure_signed(origin)?;

            // Normalize players vector depending on mode
            match game_mode {
                GameMode::PvP => {
                    // For PvP, the caller must have included themselves and exactly one opponent.
                    ensure!(players.contains(&who), Error::<T>::CreatorMustBeInGame);
                    ensure!(
                        players.len()
                            == <u32 as sp_runtime::traits::SaturatedConversion>::saturated_into::<usize>(
                                T::NumPlayers::get()
                            ),
                        Error::<T>::InvalidNumberOfPlayers
                    );
                    // Ensure distinct players; also normalize order to [creator, opponent]
                    // so downstream logic is predictable.
                    ensure!(players[0] != players[1], Error::<T>::InvalidMove);
                    if players[0] != who {
                        // Put creator in slot 0
                        if players[1] == who {
                            players.swap(0, 1);
                        } else {
                            // Shouldn’t happen because of the contains() check, but be safe.
                            return Err(Error::<T>::CreatorMustBeInGame.into());
                        }
                    }
                }
                GameMode::PvE => {
                    // For PvE, ignore whatever was passed and force [creator, AI].
                    let ai_acc = T::AiAccount::get();
                    // Also guard against creator == AI account (shouldn’t happen for sane config).
                    ensure!(who != ai_acc, Error::<T>::InvalidMove);
                    players = sp_std::vec![who.clone(), ai_acc];
                }
            }

            // From here on, `players` is normalized for both modes.
            let number_of_players = players.len();
            ensure!(
                number_of_players
                    == <u32 as sp_runtime::traits::SaturatedConversion>::saturated_into::<usize>(
                        T::NumPlayers::get()
                    ),
                Error::<T>::InvalidNumberOfPlayers
            );

            let creator = players[0].clone();
            let opponent = players[1].clone();

            // Redundant after normalization, but keep as a safety net.
            ensure!(creator != opponent, Error::<T>::InvalidMove);

            // Enforce: a wallet may participate in at most one active game, scoped by mode.
            match game_mode {
                GameMode::PvP => {
                    ensure!(ActiveGameOf::<T>::get(&creator).is_none(), Error::<T>::PlayerAlreadyInGame);
                    ensure!(ActiveGameOf::<T>::get(&opponent).is_none(), Error::<T>::PlayerAlreadyInGame);
                }
                GameMode::PvE => {
                    // Only the human creator is restricted in PvE; the AI may participate in many games.
                    ensure!(ActiveGameOf::<T>::get(&creator).is_none(), Error::<T>::PlayerAlreadyInGame);
                }
            }

            let current_block_number = <frame_system::Pallet<T>>::block_number();
            let game_id = T::Hashing::hash_of(&(creator.clone(), opponent.clone(), current_block_number));

            // Ensure the game_id isn’t already in use (collision check)
            ensure!(
                !GameStorage::<T>::contains_key(&game_id),
                Error::<T>::GameNotFound
            );

            let initial_board: Board = Default::default();
            let initial_scores = (5, 5);
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

            GameModes::<T>::insert(&game_id, game_mode.clone());
            // Mark participants as busy with this game
            match game_mode {
                GameMode::PvP => {
                    ActiveGameOf::<T>::insert(&creator, game_id);
                    ActiveGameOf::<T>::insert(&opponent, game_id);
                }
                GameMode::PvE => {
                    // Only mark the human creator as active; AI is allowed to be in many games simultaneously.
                    ActiveGameOf::<T>::insert(&creator, game_id);
                }
            }

            // If PvE, create AI hand immediately so UI can render it.
            if matches!(game_mode, GameMode::PvE) {
                let ai_acc = T::AiAccount::get();
                if HandsOfGame::<T>::get(&game_id, &ai_acc).is_none() {
                    if let Some(ai_hand) = Self::generate_ai_hand_default(&game_id) {
                        HandsOfGame::<T>::insert(&game_id, &ai_acc, ai_hand);
                    }
                }
            }

            // Set starting player: PvE -> creator always starts; PvP -> keep randomized start
            if matches!(game_mode, GameMode::PvE) {
                // players[0] is guaranteed to be the creator after normalization above
                game.set_player_turn(0);
            } else {
                // PvP: randomize starting player based on creator hash
                game.set_player_turn(
                    if sp_io::hashing::blake2_128(&creator.encode())[0] % 2 == 0 { 0 } else { 1 }
                );
            }

            GameStorage::<T>::insert(&game_id, game.clone());
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

            // If this is a PvE game and it's now the AI's turn, let the AI act immediately.
            if matches!(GameModes::<T>::get(&game_id), Some(GameMode::PvE)) {
                if let Some(mut g) = GameStorage::<T>::get(&game_id) {
                    Self::maybe_ai_take_turn(&game_id, &mut g);
                }
            }

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
            Self::deposit_event(Event::HandSubmitted { game_id, player: who.clone() });

            // PvE: submitting player is always the human. Generate AI hand right away,
            // and if it's AI's turn (e.g., AI won first move), let it act immediately.
            if matches!(GameModes::<T>::get(&game_id), Some(GameMode::PvE)) {
                let ai_acc = T::AiAccount::get();
                if HandsOfGame::<T>::get(&game_id, &ai_acc).is_none() {
                    if let Some(ai_hand) = Self::generate_ai_hand_for_game(&game_id, &who) {
                        HandsOfGame::<T>::insert(&game_id, &ai_acc, ai_hand);
                    }
                }
                // If AI is up next, take its turn now that it has a hand.
                if let Some(mut game) = GameStorage::<T>::get(&game_id) {
                    Self::maybe_ai_take_turn(&game_id, &mut game);
                }
            }
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

            // If this is a PvE game and it's now the AI's turn, let the AI act immediately.
            if matches!(GameModes::<T>::get(&game_id), Some(GameMode::PvE)) {
                if let Some(mut g) = GameStorage::<T>::get(&game_id) {
                    Self::maybe_ai_take_turn(&game_id, &mut g);
                }
            }

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

            // Check if the BlocksToPlayLimit has passed (use saturating math and inclusive deadline)
            let current_block = <frame_system::Pallet<T>>::block_number();
            let limit: BlockNumberFor<T> = T::BlocksToPlayLimit::get().into();
            let deadline = game.last_played_block.saturating_add(limit);
            ensure!(current_block >= deadline, Error::<T>::BlocksToPlayLimitNotPassed);

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
                // End game clears storage and ActiveGameOf markers; early return is fine.
                Self::end_game(&game_id, winner);
                return Ok(());
            }

            // Persist updated game state before emitting events
            GameStorage::<T>::insert(&game_id, game.clone());

            // Emit events
            let next_player = game.players[game.player_turn as usize].clone();
            Self::deposit_event(Event::TurnForceFinished {
                game_id,
                player: current_player,
            });
            Self::deposit_event(Event::NewTurn { game_id, next_player });

            Ok(())
        }
    }
}

// Helper methods
impl<T: Config> Pallet<T> {
    fn map_card_to_ai(c: &Card) -> ai::Card {
        let color = match c.color {
            Some(Color::Blue) => Some(ai::Color::Blue),
            Some(Color::Red)  => Some(ai::Color::Red),
            None => None,
        };
        ai::Card { top: c.top, right: c.right, bottom: c.bottom, left: c.left, color }
    }
    /// If the next player is the AI in a PvE game, let the AI take its move immediately.
    fn maybe_ai_take_turn(
        game_id: &GameId<T>,
        game: &mut Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
    ) {
        // Only PvE
        if !matches!(GameModes::<T>::get(game_id), Some(GameMode::PvE)) { return; }
        let ai_acc = T::AiAccount::get();
        let turn_acc = game.players[game.get_player_turn() as usize].clone();
        if turn_acc != ai_acc { return; }

        // Build AI adapter state from on-chain state
        let state = match Self::build_ai_state(game_id, game) { Some(s) => s, None => return };
        let diff = T::AiDifficulty::get();

        if let Some(action) = mc_ai::pallet::Pallet::<T>::suggest::<ai::Adapter>(&state, diff) {
            let x = action.x;
            let y = action.y;
            let idx = action.hand_index as usize;

            // Play as AI (mirror play_from_hand)
            if let Some(mut ai_hand) = HandsOfGame::<T>::get(game_id, &ai_acc) {
                // Safely access the chosen hand entry without using idx < len comparisons
                if let Some(slot) = ai_hand.get_mut(idx) {
                    let xi: usize = x as usize;
                    let yi: usize = y as usize;
                    if !slot.used {
                        if let Some(col) = game.board.get(xi) {
                            if let Some(cell) = col.get(yi) {
                                if cell.is_none() {
                                    let h = slot.clone();
                                    let placed = Card { top: h.north, right: h.east, bottom: h.south, left: h.west, color: None };
                                    let mv = Move { place_card: placed, place_index_x: x, place_index_y: y };

                                    let current_color = Self::get_current_color(game, &ai_acc);
                                    Self::place_card_on_board(game, &mv, current_color.clone());
                                    Self::apply_capture_logic(game, &mv, current_color.clone());

                                    slot.used = true;
                                    HandsOfGame::<T>::insert(game_id, &ai_acc, ai_hand);

                                    let current_block = <frame_system::Pallet<T>>::block_number();
                                    game.last_played_block = current_block;
                                    game.next_turn();

                                    let next_player = game.players[game.get_player_turn() as usize].clone();
                                    Self::deposit_event(Event::NewTurn { game_id: *game_id, next_player });
                                    GameStorage::<T>::insert(game_id, game.clone());

                                    if let Some(winner) = Self::is_game_won(game_id, game) {
                                        Self::end_game(game_id, winner);
                                        return;
                                    }

                                    Self::deposit_event(Event::MovePlayed { game_id: *game_id, player: ai_acc, x, y });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn build_ai_state(
        game_id: &GameId<T>,
        game: &Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
    ) -> Option<ai::State> {
        let p0 = game.players.get(0)?.clone();
        let p1 = game.players.get(1)?.clone();
        let hand0 = HandsOfGame::<T>::get(game_id, &p0)?;
        let hand1 = HandsOfGame::<T>::get(game_id, &p1)?;

        let map_hand = |h: &BoundedVec<HandEntry, HandLimit>| -> ai::Hand {
            let mut arr: [ai::HandEntry; 5] = core::array::from_fn(|_| ai::HandEntry { north: 1, east: 1, south: 1, west: 1, used: true });
            for (i, he) in h.iter().enumerate().take(5) {
                arr[i] = ai::HandEntry { north: he.north, east: he.east, south: he.south, west: he.west, used: he.used };
            }
            ai::Hand { entries: arr }
        };

        let hands = [map_hand(&hand0), map_hand(&hand1)];

        // Map on-chain board (card::Card) to adapter board (ai::Card)
        let mut board_ai: [[Option<ai::Card>; 4]; 4] = core::array::from_fn(|_| core::array::from_fn(|_| None));
        for x in 0..4 { for y in 0..4 {
            if let Some(ref c) = game.board[x][y] { board_ai[x][y] = Some(Self::map_card_to_ai(c)); }
        }}

        Some(ai::State {
            board: board_ai,
            scores: game.scores,
            player_turn: game.player_turn,
            round: game.round,
            max_rounds: game.max_rounds,
            hands,
        })
    }

    /// Build an AI hand whose average ranks are slightly below the human's submitted hand.
    fn generate_ai_hand_for_game(
        game_id: &GameId<T>,
        human: &T::AccountId,
    ) -> Option<BoundedVec<HandEntry, HandLimit>> {
        let human_hand = HandsOfGame::<T>::get(game_id, human)?;
        let mut sum: u32 = 0;
        for h in human_hand.iter() {
            sum += h.north as u32 + h.east as u32 + h.south as u32 + h.west as u32;
        }
        let avg = (sum as f32) / ((human_hand.len() as f32) * 4.0);
        let target = (avg - 0.5).max(1.0); // slightly easier than human

        // Deterministic pseudo-randomization from (game_id, human)
        let seed_hash = <T as frame_system::Config>::Hashing::hash_of(&(game_id, human));
        let bytes = seed_hash.as_ref();

        let mut mk_val = |i: usize| -> u8 {
            // clamp to [1.0, 9.0] then round without relying on FloatCore/round
            let jitter = ((bytes.get(i % bytes.len()).copied().unwrap_or(0) as i8 % 3) - 1) as f32;
            let mut base = target + jitter;
            if base < 1.0 { base = 1.0; }
            if base > 9.0 { base = 9.0; }
            let b_int = base as i32;
            let frac = base - (b_int as f32);
            let mut clamped = if frac >= 0.5 { (b_int + 1) as u8 } else { b_int as u8 };
            if clamped < 1 { clamped = 1; }
            if clamped > 9 { clamped = 9; }
            clamped
        };

        let mut out: BoundedVec<HandEntry, HandLimit> = BoundedVec::default();
        for i in 0..HandLimit::get() {
            let e = HandEntry { card_id: 0, north: mk_val(i as usize), east: mk_val(i as usize + 1), south: mk_val(i as usize + 2), west: mk_val(i as usize + 3), used: false };
            let _ = out.try_push(e);
        }
        Some(out)
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

    /// Create a default AI hand at game creation time so UI can display it even before human submits.
    /// This hand uses deterministic pseudo-random stats (1..=9) derived from the game_id seed.
    fn generate_ai_hand_default(game_id: &GameId<T>) -> Option<BoundedVec<HandEntry, HandLimit>> {
        // Derive bytes from the game_id itself for reproducible pseudo-randomness
        let h = <T as frame_system::Config>::Hashing::hash_of(game_id);
        let bytes = h.as_ref();
        if bytes.is_empty() { return None; }

        let mut at = 0usize;
        let mut next = || -> u8 {
            let b = bytes[at % bytes.len()];
            at = at.wrapping_add(1);
            // Map 0..=255 -> 1..=9
            (b % 9).saturating_add(1)
        };

        let mut out: BoundedVec<HandEntry, HandLimit> = BoundedVec::default();
        for _ in 0..HandLimit::get() {
            let e = HandEntry { card_id: 0, north: next(), east: next(), south: next(), west: next(), used: false };
            let _ = out.try_push(e);
        }
        Some(out)
    }

    fn end_game(game_id: &GameId<T>, winner: Option<T::AccountId>) {
        // Try to read players before we wipe storage
        let participants = GameStorage::<T>::get(game_id).map(|g| (
            g.players.get(0).cloned(),
            g.players.get(1).cloned(),
        ));

        Self::deposit_event(Event::GameFinished {
            game_id: *game_id,
            winner: winner.clone(),
        });

        // Clear active-game markers for both players if present
        if let Some((p0, p1)) = participants {
            if let Some(a) = p0 { ActiveGameOf::<T>::remove(&a); }
            if let Some(b) = p1 { ActiveGameOf::<T>::remove(&b); }
        }

        // Finally remove the game itself
        GameStorage::<T>::remove(game_id);
    }
}
