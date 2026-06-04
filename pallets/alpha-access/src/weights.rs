#![allow(unused_parens)]

use frame_support::weights::Weight;

pub trait WeightInfo {
    fn grant_access() -> Weight;
    fn revoke_access() -> Weight;
    fn set_manager() -> Weight;
    fn set_allowed_source() -> Weight;
    fn set_access_mode() -> Weight;
}

impl WeightInfo for () {
    fn grant_access() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn revoke_access() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn set_manager() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn set_allowed_source() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn set_access_mode() -> Weight {
        Weight::from_parts(10_000, 0)
    }
}
