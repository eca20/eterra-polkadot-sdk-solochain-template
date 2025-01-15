use frame_system::Config;

pub mod board;
pub mod card;
pub mod game;

pub type GameId<T> = <T as Config>::Hash;
