use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
    fn publish_definition(items: u32) -> Weight;
    fn publish_league_move_policy(moves: u32) -> Weight;
    fn set_activation() -> Weight;
    fn learn_move() -> Weight;
    fn equip_moves(moves: u32) -> Weight;
    fn grant_training_experience() -> Weight;
    fn emergency_unlock() -> Weight;
}

impl WeightInfo for () {
    fn publish_definition(items: u32) -> Weight {
        Weight::from_parts(20_000_000 + u64::from(items) * 1_000_000, 0)
    }
    fn publish_league_move_policy(moves: u32) -> Weight {
        Weight::from_parts(20_000_000 + u64::from(moves) * 1_500_000, 0)
    }
    fn set_activation() -> Weight {
        Weight::from_parts(12_000_000, 0)
    }
    fn learn_move() -> Weight {
        Weight::from_parts(40_000_000, 0)
    }
    fn equip_moves(moves: u32) -> Weight {
        Weight::from_parts(25_000_000 + u64::from(moves) * 2_000_000, 0)
    }
    fn grant_training_experience() -> Weight {
        Weight::from_parts(30_000_000, 0)
    }
    fn emergency_unlock() -> Weight {
        Weight::from_parts(20_000_000, 0)
    }
}

/// Conservative private-alpha weights. Final production values must be
/// regenerated on the pinned benchmark host before economic activation.
pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn publish_definition(items: u32) -> Weight {
        Weight::from_parts(30_000_000, 3_500)
            .saturating_add(Weight::from_parts(
                u64::from(items).saturating_mul(1_500_000),
                u64::from(items).saturating_mul(40),
            ))
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }

    fn publish_league_move_policy(moves: u32) -> Weight {
        Weight::from_parts(28_000_000, 4_000)
            .saturating_add(Weight::from_parts(
                u64::from(moves).saturating_mul(2_000_000),
                u64::from(moves).saturating_mul(48),
            ))
            .saturating_add(T::DbWeight::get().reads(u64::from(moves).saturating_add(2)))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn set_activation() -> Weight {
        Weight::from_parts(18_000_000, 2_500)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn learn_move() -> Weight {
        Weight::from_parts(60_000_000, 6_500)
            .saturating_add(T::DbWeight::get().reads(10))
            .saturating_add(T::DbWeight::get().writes(3))
    }

    fn equip_moves(moves: u32) -> Weight {
        Weight::from_parts(40_000_000, 7_000)
            .saturating_add(Weight::from_parts(
                u64::from(moves).saturating_mul(3_000_000),
                u64::from(moves).saturating_mul(40),
            ))
            .saturating_add(T::DbWeight::get().reads(11))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn grant_training_experience() -> Weight {
        Weight::from_parts(50_000_000, 5_500)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(2))
    }

    fn emergency_unlock() -> Weight {
        Weight::from_parts(30_000_000, 4_000)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }
}
