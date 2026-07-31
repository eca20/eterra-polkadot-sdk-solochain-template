use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

/// Provisional private-alpha charge for one native BLS12-381 verification.
///
/// This intentionally permits no more than one proof-verification call in the
/// runtime's 1.5-second Normal dispatch allowance. Production activation still
/// requires generated weights from the pinned deployment hardware.
const DRAND_PROOF_VERIFICATION_REF_TIME: u64 = 1_250_000_000_000;

pub trait WeightInfo {
    fn set_mode() -> Weight;
    fn request() -> Weight;
    fn submit_drand_quicknet(signature_bytes: u32) -> Weight;
    fn submit_drand_checkpoint(signature_bytes: u32) -> Weight;
    fn finalize_alpha() -> Weight;
    fn timeout() -> Weight;
}

impl WeightInfo for () {
    fn set_mode() -> Weight {
        Weight::from_parts(10_000_000, 0)
    }
    fn request() -> Weight {
        Weight::from_parts(25_000_000, 0)
    }
    fn submit_drand_quicknet(signature_bytes: u32) -> Weight {
        Weight::from_parts(
            DRAND_PROOF_VERIFICATION_REF_TIME
                .saturating_add(u64::from(signature_bytes).saturating_mul(25_000)),
            0,
        )
    }
    fn submit_drand_checkpoint(signature_bytes: u32) -> Weight {
        Weight::from_parts(
            DRAND_PROOF_VERIFICATION_REF_TIME
                .saturating_add(u64::from(signature_bytes).saturating_mul(25_000)),
            0,
        )
    }
    fn finalize_alpha() -> Weight {
        Weight::from_parts(35_000_000, 0)
    }
    fn timeout() -> Weight {
        Weight::from_parts(20_000_000, 0)
    }
}

/// Conservative private-alpha weights pending final production-hardware
/// benchmarking. Proof submission reserves 1.25 seconds for the no-std
/// BLS12-381 pairing verifier, so at most one fits in the runtime's Normal
/// dispatch allowance.
pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn set_mode() -> Weight {
        Weight::from_parts(18_000_000, 1_500)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }

    fn request() -> Weight {
        Weight::from_parts(45_000_000, 4_500)
            .saturating_add(T::DbWeight::get().reads(5))
            .saturating_add(T::DbWeight::get().writes(2))
    }

    fn submit_drand_quicknet(signature_bytes: u32) -> Weight {
        Weight::from_parts(DRAND_PROOF_VERIFICATION_REF_TIME, 7_000)
            .saturating_add(Weight::from_parts(
                u64::from(signature_bytes).saturating_mul(25_000),
                0,
            ))
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(6))
    }

    fn submit_drand_checkpoint(signature_bytes: u32) -> Weight {
        Weight::from_parts(DRAND_PROOF_VERIFICATION_REF_TIME, 3_500)
            .saturating_add(Weight::from_parts(
                u64::from(signature_bytes).saturating_mul(25_000),
                0,
            ))
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(3))
    }

    fn finalize_alpha() -> Weight {
        Weight::from_parts(55_000_000, 4_500)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(2))
    }

    fn timeout() -> Weight {
        Weight::from_parts(30_000_000, 3_000)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_verification_weight_uses_substrate_ref_time_units() {
        let quicknet = <() as WeightInfo>::submit_drand_quicknet(48);
        let checkpoint = <() as WeightInfo>::submit_drand_checkpoint(48);
        assert!(quicknet.ref_time() >= 1_000_000_000_000);
        assert!(checkpoint.ref_time() >= 1_000_000_000_000);
        assert!(quicknet.ref_time().saturating_mul(2) > 1_500_000_000_000);
        assert!(checkpoint.ref_time().saturating_mul(2) > 1_500_000_000_000);
    }
}
