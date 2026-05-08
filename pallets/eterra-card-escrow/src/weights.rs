#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
    fn deposit_cards(n: u32) -> Weight;
    fn withdraw_cards(n: u32) -> Weight;
    fn record_enemy_defeat_with_event_id() -> Weight;
    fn record_enemy_elimination_with_event_id() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn deposit_cards(n: u32) -> Weight {
        Weight::from_parts(20_000_000, 0)
            .saturating_add(Weight::from_parts(8_000_000, 0).saturating_mul(n.into()))
            .saturating_add(T::DbWeight::get().reads((2_u64).saturating_mul(n.into())))
            .saturating_add(T::DbWeight::get().writes((6_u64).saturating_mul(n.into())))
    }

    fn withdraw_cards(n: u32) -> Weight {
        Weight::from_parts(18_000_000, 0)
            .saturating_add(Weight::from_parts(7_000_000, 0).saturating_mul(n.into()))
            .saturating_add(T::DbWeight::get().reads((2_u64).saturating_mul(n.into())))
            .saturating_add(T::DbWeight::get().writes((6_u64).saturating_mul(n.into())))
    }

    fn record_enemy_defeat_with_event_id() -> Weight {
        Weight::from_parts(25_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(3))
    }

    fn record_enemy_elimination_with_event_id() -> Weight {
        Weight::from_parts(22_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(2))
    }
}

impl WeightInfo for () {
    fn deposit_cards(n: u32) -> Weight {
        Weight::from_parts(20_000_000, 0)
            .saturating_add(Weight::from_parts(8_000_000, 0).saturating_mul(n.into()))
            .saturating_add(frame_support::weights::constants::RocksDbWeight::get().reads((2_u64).saturating_mul(n.into())))
            .saturating_add(frame_support::weights::constants::RocksDbWeight::get().writes((6_u64).saturating_mul(n.into())))
    }

    fn withdraw_cards(n: u32) -> Weight {
        Weight::from_parts(18_000_000, 0)
            .saturating_add(Weight::from_parts(7_000_000, 0).saturating_mul(n.into()))
            .saturating_add(frame_support::weights::constants::RocksDbWeight::get().reads((2_u64).saturating_mul(n.into())))
            .saturating_add(frame_support::weights::constants::RocksDbWeight::get().writes((6_u64).saturating_mul(n.into())))
    }

    fn record_enemy_defeat_with_event_id() -> Weight {
        Weight::from_parts(25_000_000, 0)
            .saturating_add(frame_support::weights::constants::RocksDbWeight::get().reads(4))
            .saturating_add(frame_support::weights::constants::RocksDbWeight::get().writes(3))
    }

    fn record_enemy_elimination_with_event_id() -> Weight {
        Weight::from_parts(22_000_000, 0)
            .saturating_add(frame_support::weights::constants::RocksDbWeight::get().reads(4))
            .saturating_add(frame_support::weights::constants::RocksDbWeight::get().writes(2))
    }
}
