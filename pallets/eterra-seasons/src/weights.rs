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
        Weight::from_parts(20_000_000_000, 4_096)
    }
    fn remove_admin() -> Weight {
        Weight::from_parts(20_000_000_000, 4_096)
    }
    fn create_season() -> Weight {
        Weight::from_parts(50_000_000_000, 16_384)
    }
    fn activate_season() -> Weight {
        Weight::from_parts(400_000_000_000, 8_388_608)
    }
    fn close_season() -> Weight {
        Weight::from_parts(50_000_000_000, 16_384)
    }
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn add_admin() -> Weight {
        Weight::from_parts(20_000_000_000, 4_096)
            .saturating_add(T::DbWeight::get()
            .reads(0)
            .saturating_add(T::DbWeight::get().writes(1)))
    }
    fn remove_admin() -> Weight {
        Weight::from_parts(20_000_000_000, 4_096)
            .saturating_add(T::DbWeight::get()
            .reads(0)
            .saturating_add(T::DbWeight::get().writes(1)))
    }
    fn create_season() -> Weight {
        Weight::from_parts(50_000_000_000, 16_384)
            .saturating_add(T::DbWeight::get()
            .reads(1)
            .saturating_add(T::DbWeight::get().writes(2)))
    }
    fn activate_season() -> Weight {
        // The runtime validator scans as many as 32 published TCG
        // collections and their bounded asset/weight lists before activation.
        Weight::from_parts(400_000_000_000, 8_388_608)
            .saturating_add(T::DbWeight::get()
            .reads(128)
            .saturating_add(T::DbWeight::get().writes(16)))
    }
    fn close_season() -> Weight {
        Weight::from_parts(50_000_000_000, 16_384)
            .saturating_add(T::DbWeight::get()
            .reads(2)
            .saturating_add(T::DbWeight::get().writes(2)))
    }
}
