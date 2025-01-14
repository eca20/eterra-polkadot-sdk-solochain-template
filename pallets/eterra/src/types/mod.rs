use frame_support::BoundedVec;

pub mod card;
pub mod board;
pub mod game;

pub type GameId = [u8; 32];
pub type Players<Account, N> = BoundedVec<Account, N>;