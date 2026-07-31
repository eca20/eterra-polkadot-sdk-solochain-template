use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
    fn publish_definition() -> Weight;
    fn grant() -> Weight;
    fn reserve(items: u32) -> Weight;
    fn settle(items: u32) -> Weight;
    fn prism_xp() -> Weight;
    fn emergency_unlock() -> Weight;
    fn publish_crafting_recipe() -> Weight;
    fn craft_spell_charges() -> Weight;
}

impl WeightInfo for () {
    fn publish_definition() -> Weight {
        Weight::from_parts(18_000_000, 0)
    }
    fn grant() -> Weight {
        Weight::from_parts(22_000_000, 0)
    }
    fn reserve(items: u32) -> Weight {
        Weight::from_parts(30_000_000 + u64::from(items) * 4_000_000, 0)
    }
    fn settle(items: u32) -> Weight {
        Weight::from_parts(35_000_000 + u64::from(items) * 5_000_000, 0)
    }
    fn prism_xp() -> Weight {
        Weight::from_parts(28_000_000, 0)
    }
    fn emergency_unlock() -> Weight {
        Weight::from_parts(18_000_000, 0)
    }
    fn publish_crafting_recipe() -> Weight {
        Weight::from_parts(24_000_000, 0)
    }
    fn craft_spell_charges() -> Weight {
        Weight::from_parts(65_000_000, 0)
    }
}

/// Conservative private-alpha weights. Final production values must be
/// regenerated on the pinned benchmark host before economic activation.
pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn publish_definition() -> Weight {
        Weight::from_parts(28_000_000, 3_000)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn grant() -> Weight {
        Weight::from_parts(38_000_000, 4_500)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(2))
    }

    fn reserve(items: u32) -> Weight {
        Weight::from_parts(45_000_000, 6_000)
            .saturating_add(Weight::from_parts(
                u64::from(items).saturating_mul(6_000_000),
                u64::from(items).saturating_mul(160),
            ))
            .saturating_add(T::DbWeight::get().reads(2u64.saturating_add(u64::from(items))))
            .saturating_add(T::DbWeight::get().writes(1u64.saturating_add(u64::from(items))))
    }

    fn settle(items: u32) -> Weight {
        Weight::from_parts(55_000_000, 7_000)
            .saturating_add(Weight::from_parts(
                u64::from(items).saturating_mul(7_000_000),
                u64::from(items).saturating_mul(180),
            ))
            .saturating_add(T::DbWeight::get().reads(2u64.saturating_add(u64::from(items))))
            .saturating_add(T::DbWeight::get().writes(1u64.saturating_add(u64::from(items))))
    }

    fn prism_xp() -> Weight {
        Weight::from_parts(45_000_000, 5_000)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(2))
    }

    fn emergency_unlock() -> Weight {
        Weight::from_parts(28_000_000, 3_500)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn publish_crafting_recipe() -> Weight {
        Weight::from_parts(35_000_000, 4_000)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn craft_spell_charges() -> Weight {
        Weight::from_parts(85_000_000, 8_000)
            .saturating_add(T::DbWeight::get().reads(11))
            .saturating_add(T::DbWeight::get().writes(5))
    }
}
