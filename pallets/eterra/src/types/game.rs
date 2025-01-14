use super::*;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use sp_runtime::SaturatedConversion;
use scale_info::TypeInfo;
use sp_runtime::traits::Get; // For the `Get` trait


#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct ParameterGet<const N: u32>;

impl<const N: u32> Get<u32> for ParameterGet<N> {
	fn get() -> u32 {
		N
	}
}
pub type MaxPlayers = ParameterGet<2>;

#[derive(Encode, Decode, TypeInfo, PartialEq, Clone, Debug)]
pub struct Move {
	pub place_index: u8,
}


#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, Copy, Clone, Debug)]
pub enum GameState {
	Matchmaking,
	Playing,
	Finished { winner: Option<u8> }, // Ready to reward players
}

pub trait GameProperties<Account, MaxPlayers> {
	// Player made a move
	// It is used for determining whether to generate a new selection
	fn get_played(&self) -> bool;
	fn set_played(&mut self, played: bool);

	fn get_round(&self) -> u8;
	fn set_round(&mut self, round: u8);

	fn get_player_turn(&self) -> u8;
	fn set_player_turn(&mut self, turn: u8);

	fn get_state(&self) -> GameState;
	fn set_state(&mut self, state: GameState);

	fn borrow_players(&self) -> &Players<Account, MaxPlayers>;


	fn next_turn(&mut self);
}


#[derive(Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct Game<Account, BlockNumber, MaxPlayers> {
	pub state: GameState,
	pub player_turn_and_played: u8,
	pub last_played_block: BlockNumber,
	pub players: Players<Account, MaxPlayers>, // Player AccountIds
	pub round: u8,
	pub max_rounds: u8,
}

impl<Account, BlockNumber, MaxPlayers> GameProperties<Account, MaxPlayers>
	for Game<Account, BlockNumber, MaxPlayers>
{
	fn get_played(&self) -> bool {
		((self.player_turn_and_played & 0x80) >> 7) == 1
	}

	fn set_played(&mut self, played: bool) {
		self.player_turn_and_played = (self.player_turn_and_played & 0x7F) | (played as u8) << 7
	}

	fn get_round(&self) -> u8 {
		self.round
	}

	fn set_round(&mut self, round: u8) {
		self.round = round;
	}

	fn get_player_turn(&self) -> u8 {
		self.player_turn_and_played & 0x7F
	}

	fn set_player_turn(&mut self, turn: u8) {
		self.player_turn_and_played = (self.player_turn_and_played & 0x80) | turn;
	}

	fn get_state(&self) -> GameState {
		self.state
	}

	fn set_state(&mut self, state: GameState) {
		self.state = state;
	}

	fn borrow_players(&self) -> &Players<Account, MaxPlayers> {
		&self.players
	}

	fn next_turn(&mut self) {
		let player_turn = self.get_player_turn();

		let next_player_turn =
			(player_turn + 1) % self.borrow_players().len().saturated_into::<u8>();

		self.set_player_turn(next_player_turn);

		if next_player_turn == 0 {
			let round = self.get_round() + 1;
			self.set_round(round);

			if round > self.max_rounds {
				self.set_state(GameState::Finished { winner: None });
			}
		}
	}
}
