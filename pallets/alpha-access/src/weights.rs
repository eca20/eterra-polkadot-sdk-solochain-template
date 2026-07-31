#![allow(unused_parens)]

use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
    fn grant_access() -> Weight;
    fn revoke_access() -> Weight;
    fn set_manager() -> Weight;
    fn set_allowed_source() -> Weight;
    fn set_access_mode() -> Weight;
}

/// Provisional conservative V16 weights. These remain subject to the frozen
/// production-hardware benchmark gate.
pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn grant_access() -> Weight {
        Weight::from_parts(100_000_000_000, 32_768)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(2))
    }

    fn revoke_access() -> Weight {
        Weight::from_parts(30_000_000_000, 8_192)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn set_manager() -> Weight {
        Weight::from_parts(20_000_000_000, 4_096).saturating_add(T::DbWeight::get().writes(1))
    }

    fn set_allowed_source() -> Weight {
        Weight::from_parts(20_000_000_000, 4_096).saturating_add(T::DbWeight::get().writes(1))
    }

    fn set_access_mode() -> Weight {
        Weight::from_parts(20_000_000_000, 4_096).saturating_add(T::DbWeight::get().writes(1))
    }
}

impl WeightInfo for () {
    fn grant_access() -> Weight {
        Weight::from_parts(100_000_000_000, 32_768)
    }

    fn revoke_access() -> Weight {
        Weight::from_parts(30_000_000_000, 8_192)
    }

    fn set_manager() -> Weight {
        Weight::from_parts(20_000_000_000, 4_096)
    }

    fn set_allowed_source() -> Weight {
        Weight::from_parts(20_000_000_000, 4_096)
    }

    fn set_access_mode() -> Weight {
        Weight::from_parts(20_000_000_000, 4_096)
    }
}
