use super::*;
use codec::Encode;
use frame_benchmarking::{account, benchmarks};
use frame_support::storage::storage_prefix;
use frame_system::RawOrigin;

const DOMAIN: Hash32 = [1; 32];
const COMMITMENT: Hash32 = [2; 32];
const CONFIG_HASH: Hash32 = [3; 32];
const BENCHMARK_DRAND_ROUND: u64 = 123;
const BENCHMARK_DRAND_SIGNATURE: [u8; 48] = [
    0xb7, 0x5c, 0x69, 0xd0, 0xb7, 0x2a, 0x5d, 0x90, 0x6e, 0x85, 0x4e, 0x80, 0x8b, 0xa7, 0xe2, 0xac,
    0xcb, 0x15, 0x42, 0xac, 0x35, 0x5a, 0xe4, 0x86, 0xd5, 0x91, 0xaa, 0x9d, 0x43, 0x76, 0x54, 0x82,
    0xe2, 0x6c, 0xd0, 0x2d, 0xf8, 0x35, 0xd3, 0x54, 0x6d, 0x23, 0xc4, 0xb1, 0x3e, 0x0d, 0xfc, 0x92,
];
const BENCHMARK_DRAND_OUTPUT: Hash32 = [
    0xfb, 0x8f, 0x7b, 0xc2, 0x9b, 0xf2, 0x4d, 0xb5, 0x18, 0x71, 0xec, 0x8c, 0x79, 0xf3, 0xa1, 0xe4,
    0xbd, 0x05, 0x57, 0xbc, 0x0d, 0xfc, 0xee, 0x9e, 0xd1, 0xd9, 0x24, 0xe6, 0x9d, 0x1c, 0x60, 0xdc,
];

fn set_benchmark_drand_round<T: Config>(round: u64) {
    let unix_seconds = eterra_drand_quicknet::QUICKNET_GENESIS_UNIX_SECONDS
        .checked_add(
            round
                .checked_sub(1)
                .expect("benchmark round is non-zero")
                .checked_mul(eterra_drand_quicknet::QUICKNET_PERIOD_SECONDS)
                .expect("benchmark round fits Unix time"),
        )
        .expect("benchmark Unix time fits");
    let timestamp_millis = unix_seconds
        .checked_mul(1_000)
        .expect("benchmark timestamp fits");
    // The production runtime's UnixTime provider is the Timestamp pallet. The
    // mock mirrors this benchmark-only storage override.
    let key = storage_prefix(b"Timestamp", b"Now");
    sp_io::storage::set(&key, &timestamp_millis.encode());
    assert_eq!(
        Pallet::<T>::current_drand_round().expect("benchmark clock is valid"),
        round
    );
}

benchmarks! {
    set_mode {
    }: _(RawOrigin::Root, RandomnessMode::DeterministicPrivateAlpha)
    verify {
        assert_eq!(CurrentMode::<T>::get(), RandomnessMode::DeterministicPrivateAlpha);
    }

    set_cryptography_review_status {
    }: _(RawOrigin::Root, false)
    verify {
        assert!(!CryptographyReviewApproved::<T>::get());
    }

    request_alpha_fixture {
        CurrentMode::<T>::put(RandomnessMode::DeterministicPrivateAlpha);
    }: _(RawOrigin::Root, DOMAIN, COMMITMENT, CONFIG_HASH, 10)
    verify {
        assert_eq!(Requests::<T>::iter().count(), 1);
    }

    submit_drand_quicknet {
        let caller: T::AccountId = account("caller", 0, 0);
        let request_round = BENCHMARK_DRAND_ROUND
            .checked_sub(T::MinFutureEpochs::get())
            .expect("benchmark vector leaves room for the configured delay");
        set_benchmark_drand_round::<T>(request_round);
        CryptographyReviewApproved::<T>::put(true);
        CurrentMode::<T>::put(RandomnessMode::DrandQuicknet);
        LatestVerifiedRound::<T>::put(request_round);
        LatestVerifiedAt::<T>::put(frame_system::Pallet::<T>::block_number());
        LatestVerifiedProofHash::<T>::put([9; 32]);
        let request_id = Pallet::<T>::do_request(DOMAIN, COMMITMENT, CONFIG_HASH, 0)
            .expect("benchmark request");
        let request = Requests::<T>::get(request_id).expect("request exists");
        assert_eq!(request.exact_epoch, BENCHMARK_DRAND_ROUND);
        frame_system::Pallet::<T>::set_block_number(request.not_before);
        set_benchmark_drand_round::<T>(BENCHMARK_DRAND_ROUND);
        let raw_signature = BENCHMARK_DRAND_SIGNATURE.to_vec();
    }: _(
        RawOrigin::Signed(caller),
        request_id,
        BENCHMARK_DRAND_ROUND,
        raw_signature
    )
    verify {
        assert_eq!(
            Requests::<T>::get(request_id).expect("request exists").status,
            RequestStatus::Finalized
        );
        assert_eq!(
            Outputs::<T>::get(request_id).expect("output exists").output,
            BENCHMARK_DRAND_OUTPUT
        );
        assert!(ProofSignatures::<T>::contains_key(request_id));
    }

    finalize_alpha {
        let caller: T::AccountId = account("caller", 0, 0);
        CurrentMode::<T>::put(RandomnessMode::DeterministicPrivateAlpha);
        let request_id = Pallet::<T>::do_request(DOMAIN, COMMITMENT, CONFIG_HASH, 10)
            .expect("benchmark request");
        let request = Requests::<T>::get(request_id).expect("request exists");
        frame_system::Pallet::<T>::set_block_number(request.not_before);
    }: _(RawOrigin::Signed(caller), request_id)
    verify {
        assert!(Outputs::<T>::contains_key(request_id));
    }

    mark_timed_out {
        let caller: T::AccountId = account("caller", 0, 0);
        CurrentMode::<T>::put(RandomnessMode::DeterministicPrivateAlpha);
        let request_id = Pallet::<T>::do_request(DOMAIN, COMMITMENT, CONFIG_HASH, 10)
            .expect("benchmark request");
        let request = Requests::<T>::get(request_id).expect("request exists");
        frame_system::Pallet::<T>::set_block_number(request.timeout_at);
    }: _(RawOrigin::Signed(caller), request_id)
    verify {
        assert_eq!(
            Requests::<T>::get(request_id).expect("request exists").status,
            RequestStatus::TimedOut
        );
    }

    submit_drand_checkpoint {
        let caller: T::AccountId = account("caller", 0, 0);
        set_benchmark_drand_round::<T>(BENCHMARK_DRAND_ROUND);
        CryptographyReviewApproved::<T>::put(true);
        LatestVerifiedRound::<T>::put(BENCHMARK_DRAND_ROUND - 1);
        let raw_signature = BENCHMARK_DRAND_SIGNATURE.to_vec();
    }: _(
        RawOrigin::Signed(caller),
        BENCHMARK_DRAND_ROUND,
        raw_signature
    )
    verify {
        assert_eq!(LatestVerifiedRound::<T>::get(), BENCHMARK_DRAND_ROUND);
        assert_eq!(
            LatestVerifiedAt::<T>::get(),
            Some(frame_system::Pallet::<T>::block_number())
        );
        assert!(LatestVerifiedProofHash::<T>::get().is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
