use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use frame_support::storage::types::StorageMap;
use frame_support::Blake2_128Concat;

use crate::types::card::Card;
use crate::pallet::Config;

pub type Board = [[Option<Card>; 4]; 4];