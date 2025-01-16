use crate::types::board::Board;
use crate::types::card::Card;
use frame_support::BoundedVec;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen}; // For Encode, Decode, MaxEncodedLen
use scale_info::TypeInfo; // For TypeInfo

// Define `Players` type alias
pub type Players<Account, NumPlayers> = BoundedVec<Account, NumPlayers>;

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, Copy, Clone, Debug)]
pub enum GameState {
    Matchmaking,
    Playing,
    Finished { winner: Option<u8> }, // Ready to reward players
}

pub trait GameProperties<Account, NumPlayers> {
    fn get_round(&self) -> u8;
    fn set_round(&mut self, round: u8);

    fn get_player_turn(&self) -> u8;
    fn set_player_turn(&mut self, turn: u8);

    fn get_state(&self) -> GameState;
    fn set_state(&mut self, state: GameState);

    fn borrow_players(&self) -> &Players<Account, NumPlayers>;

    fn next_turn(&mut self);
}

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone)]
pub struct Game<Account, BlockNumber, NumPlayers>
where
    NumPlayers: Clone, // Add this bound
{
    pub state: GameState,
    pub last_played_block: BlockNumber,
    pub players: Players<Account, NumPlayers>, // Player AccountIds
    pub player_turn: u8,                       // Current player's turn (0 or 1)
    pub round: u8,                             // Current round number
    pub max_rounds: u8,                        // Maximum number of rounds
    pub board: Board,
}

impl<Account, BlockNumber, NumPlayers> GameProperties<Account, NumPlayers>
    for Game<Account, BlockNumber, NumPlayers>
where
    NumPlayers: Clone, // Add this bound
{
    fn get_round(&self) -> u8 {
        self.round
    }

    fn set_round(&mut self, round: u8) {
        self.round = round;
    }

    fn get_player_turn(&self) -> u8 {
        self.player_turn
    }

    fn set_player_turn(&mut self, turn: u8) {
        self.player_turn = turn;
    }

    fn get_state(&self) -> GameState {
        self.state
    }

    fn set_state(&mut self, state: GameState) {
        self.state = state;
    }

    fn borrow_players(&self) -> &Players<Account, NumPlayers> {
        &self.players
    }

    fn next_turn(&mut self) {
        // Switch turn
        self.player_turn = (self.player_turn + 1) % 2;

        // Increment round only when player_turn wraps back to 0
        if self.player_turn == 0 {
            self.round += 1;
        }
    }
}

#[derive(Encode, Decode, TypeInfo, PartialEq, Clone, Debug)]
pub struct Move {
    pub place_index_x: u8,
    pub place_index_y: u8,
    pub place_card: Card,
}
