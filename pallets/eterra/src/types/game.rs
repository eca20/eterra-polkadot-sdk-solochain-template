use frame_support::BoundedVec;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen}; // For Encode, Decode, MaxEncodedLen
use scale_info::TypeInfo; // For TypeInfo
use sp_runtime::SaturatedConversion; // For BoundedVec

// Define `Players` type alias
pub type Players<Account, NumPlayers> = BoundedVec<Account, NumPlayers>;

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, Copy, Clone, Debug)]
pub enum GameState {
    Matchmaking,
    Playing,
    Finished { winner: Option<u8> }, // Ready to reward players
}

pub trait GameProperties<Account, NumPlayers> {
    // Player made a move
    fn get_round(&self) -> u8;
    fn set_round(&mut self, round: u8);

    fn get_state(&self) -> GameState;
    fn set_state(&mut self, state: GameState);

    fn borrow_players(&self) -> &Players<Account, NumPlayers>;
}

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct Game<Account, BlockNumber, NumPlayers> {
    pub state: GameState,
    pub last_played_block: BlockNumber,
    pub players: Players<Account, NumPlayers>, // Player AccountIds
    pub round: u8,
    pub max_rounds: u8,
}

impl<Account, BlockNumber, NumPlayers> GameProperties<Account, NumPlayers>
    for Game<Account, BlockNumber, NumPlayers>
{
    fn get_round(&self) -> u8 {
        self.round
    }

    fn set_round(&mut self, round: u8) {
        self.round = round;
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
}

#[derive(Encode, Decode, TypeInfo, PartialEq, Clone, Debug)]
pub struct Move {
    pub place_index_x: u8,
    pub place_index_y: u8,
}
