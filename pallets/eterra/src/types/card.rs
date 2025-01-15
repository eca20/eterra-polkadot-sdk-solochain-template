use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, Debug)]
pub struct Card {
    pub top: u8,
    pub right: u8,
    pub bottom: u8,
    pub left: u8,
    pub color: Option<Color>, // None if not yet assigned
}

impl Card {
    pub fn new(top: u8, right: u8, bottom: u8, left: u8) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
            color: None,
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn get_color(&self) -> Option<&Color> {
        self.color.as_ref()
    }
}

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, Debug)]
pub enum Color {
    Blue,
    Red,
}
