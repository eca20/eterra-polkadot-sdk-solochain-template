use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;
use crate::Player;

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, Debug)]
pub struct Card {
    pub top: u8,
    pub right: u8,
    pub bottom: u8,
    pub left: u8,
    pub possession: Option<Player>, // None if not yet assigned
}

impl Card {
    pub fn new(top: u8, right: u8, bottom: u8, left: u8) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
            possession: None,
        }
    }

    pub fn with_possession(mut self, possession: Player) -> Self {
        self.possession = Some(possession);
        self
    }

    pub fn get_possession(&self) -> Option<&Player> {
        self.possession.as_ref()
    }
}

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, Debug)]
pub enum Possession { PlayerOne, PlayerTwo }