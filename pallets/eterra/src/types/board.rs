use frame_support::pallet_prelude::*;
use frame_support::storage::types::StorageMap;
use frame_support::Blake2_128Concat;
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;

use crate::pallet::Config;
use crate::types::card::Card;

pub type Board = [[Option<Card>; 4]; 4];
