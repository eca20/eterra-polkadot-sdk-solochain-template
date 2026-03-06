#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::traits::Get;
use frame_support::weights::Weight;

/// Weight functions for `pallet-eterra-seasons`.
pub trait WeightInfo {
    fn add_admin() -> Weight;
    fn remove_admin() -> Weight;
    fn create_season() -> Weight;
    fn activate_season() -> Weight;
    fn close_season() -> Weight;
}

impl WeightInfo for () {
    fn add_admin() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn remove_admin() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn create_season() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn activate_season() -> Weight {
        Weight::from_parts(10_000, 0)
    }
    fn close_season() -> Weight {
        Weight::from_parts(10_000, 0)
    }
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn add_admin() -> Weight {
        T::DbWeight::get()
            .reads(0)
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn remove_admin() -> Weight {
        T::DbWeight::get()
            .reads(0)
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn create_season() -> Weight {
        T::DbWeight::get()
            .reads(1)
            .saturating_add(T::DbWeight::get().writes(2))
    }
    fn activate_season() -> Weight {
        T::DbWeight::get()
            .reads(3)
            .saturating_add(T::DbWeight::get().writes(3))
    }
    fn close_season() -> Weight {
        T::DbWeight::get()
            .reads(2)
            .saturating_add(T::DbWeight::get().writes(2))
    }
}
