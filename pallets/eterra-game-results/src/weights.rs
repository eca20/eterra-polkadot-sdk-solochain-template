use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
    fn publish_policy() -> Weight;
    fn authorize_session(assets: u32) -> Weight;
    fn authorize_session_with_ticket(assets: u32, signature_bytes: u32) -> Weight;
    fn submit_result(metrics: u32) -> Weight;
    fn expire_session(assets: u32) -> Weight;
    fn finalize_drop() -> Weight;
    fn seal_epoch(sessions: u32) -> Weight;
}

impl WeightInfo for () {
    fn publish_policy() -> Weight {
        Weight::from_parts(25_000_000, 0)
    }
    fn authorize_session(assets: u32) -> Weight {
        Weight::from_parts(75_000_000 + u64::from(assets) * 12_000_000, 0)
    }
    fn authorize_session_with_ticket(assets: u32, signature_bytes: u32) -> Weight {
        Weight::from_parts(
            105_000_000 + u64::from(assets) * 12_000_000 + u64::from(signature_bytes) * 150_000,
            0,
        )
    }
    fn submit_result(metrics: u32) -> Weight {
        Weight::from_parts(80_000_000 + u64::from(metrics) * 3_000_000, 0)
    }
    fn expire_session(assets: u32) -> Weight {
        Weight::from_parts(35_000_000 + u64::from(assets) * 5_000_000, 0)
    }
    fn finalize_drop() -> Weight {
        Weight::from_parts(50_000_000, 0)
    }
    fn seal_epoch(sessions: u32) -> Weight {
        Weight::from_parts(30_000_000 + u64::from(sessions) * 3_000_000, 0)
    }
}

/// Conservative private-alpha weights. Final production values must be
/// regenerated on the pinned benchmark host before economic activation.
pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn publish_policy() -> Weight {
        Weight::from_parts(40_000_000, 4_000)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(2))
    }

    fn authorize_session(assets: u32) -> Weight {
        Weight::from_parts(350_000_000, 40_000)
            .saturating_add(Weight::from_parts(
                u64::from(assets).saturating_mul(60_000_000),
                u64::from(assets).saturating_mul(4_096),
            ))
            .saturating_add(
                T::DbWeight::get()
                    .reads(23u64.saturating_add(u64::from(assets).saturating_mul(12))),
            )
            .saturating_add(
                T::DbWeight::get()
                    .writes(14u64.saturating_add(u64::from(assets).saturating_mul(5))),
            )
    }

    fn authorize_session_with_ticket(assets: u32, signature_bytes: u32) -> Weight {
        Weight::from_parts(500_000_000, 64_000)
            .saturating_add(Weight::from_parts(
                u64::from(assets).saturating_mul(60_000_000),
                u64::from(assets).saturating_mul(4_096),
            ))
            .saturating_add(Weight::from_parts(
                u64::from(signature_bytes).saturating_mul(500_000),
                u64::from(signature_bytes),
            ))
            .saturating_add(
                T::DbWeight::get()
                    .reads(31u64.saturating_add(u64::from(assets).saturating_mul(12))),
            )
            .saturating_add(
                T::DbWeight::get()
                    .writes(20u64.saturating_add(u64::from(assets).saturating_mul(5))),
            )
    }

    fn submit_result(metrics: u32) -> Weight {
        Weight::from_parts(750_000_000, 96_000)
            .saturating_add(Weight::from_parts(
                u64::from(metrics).saturating_mul(75_000_000),
                u64::from(metrics).saturating_mul(4_096),
            ))
            .saturating_add(
                T::DbWeight::get()
                    .reads(30u64.saturating_add(u64::from(metrics).saturating_mul(4))),
            )
            .saturating_add(
                T::DbWeight::get()
                    .writes(25u64.saturating_add(u64::from(metrics).saturating_mul(4))),
            )
    }

    fn expire_session(assets: u32) -> Weight {
        Weight::from_parts(350_000_000, 48_000)
            .saturating_add(Weight::from_parts(
                u64::from(assets).saturating_mul(50_000_000),
                u64::from(assets).saturating_mul(2_048),
            ))
            .saturating_add(
                T::DbWeight::get().reads(12u64.saturating_add(u64::from(assets).saturating_mul(3))),
            )
            .saturating_add(
                T::DbWeight::get()
                    .writes(13u64.saturating_add(u64::from(assets).saturating_mul(3))),
            )
    }

    fn finalize_drop() -> Weight {
        Weight::from_parts(500_000_000, 64_000)
            .saturating_add(T::DbWeight::get().reads(24))
            .saturating_add(T::DbWeight::get().writes(20))
    }

    fn seal_epoch(sessions: u32) -> Weight {
        Weight::from_parts(200_000_000, 32_000)
            .saturating_add(Weight::from_parts(
                u64::from(sessions).saturating_mul(20_000_000),
                u64::from(sessions).saturating_mul(512),
            ))
            .saturating_add(T::DbWeight::get().reads(8u64.saturating_add(u64::from(sessions))))
            .saturating_add(
                T::DbWeight::get()
                    .writes(8u64.saturating_add(u64::from(sessions).saturating_mul(2))),
            )
    }
}
