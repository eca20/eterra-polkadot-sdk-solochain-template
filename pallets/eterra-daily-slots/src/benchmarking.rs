#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::Get;
use frame_system::RawOrigin;

fn seed_reels<T: Config>() {
    let slot_len = T::MaxSlotLength::get();
    let max_entries = T::MaxWeightEntries::get();
    assert!(slot_len > 0);
    assert!(max_entries > 0);

    let count = max_entries.min(3);
    for reel in 0..slot_len {
        let mut weights: Vec<(u32, u32)> = Vec::new();
        for i in 0..count {
            weights.push((i, 1));
        }
        let bounded: BoundedVec<_, T::MaxWeightEntries> = weights
            .try_into()
            .expect("weights length within bounds; qed");
        ReelWeights::<T>::insert(reel, bounded);
    }
}

benchmarks! {
    roll {
        let caller: T::AccountId = whitelisted_caller();
        seed_reels::<T>();
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        // RollHistory should have at least one entry after a successful roll.
        assert!(!RollHistory::<T>::get(&caller).is_empty());
    }

    set_reel_weights {
        let reel: u32 = 0;
        let max_entries = T::MaxWeightEntries::get();
        let count = max_entries.max(1).min(3);
        let mut weights: Vec<(u32, u32)> = Vec::new();
        for i in 0..count {
            weights.push((i, 1));
        }
    }: _(RawOrigin::Root, reel, weights)
    verify {
        assert!(ReelWeights::<T>::get(reel).is_some());
    }

    set_all_reel_weights {
        let slot_len = T::MaxSlotLength::get();
        let max_entries = T::MaxWeightEntries::get();
        let count = max_entries.max(1).min(3);

        let mut all_weights: Vec<(u32, Vec<(u32, u32)>)> = Vec::new();
        for reel in 0..slot_len {
            let mut weights: Vec<(u32, u32)> = Vec::new();
            for i in 0..count {
                weights.push((i, 1));
            }
            all_weights.push((reel, weights));
        }
    }: _(RawOrigin::Root, all_weights)
    verify {
        if slot_len > 0 {
            assert!(ReelWeights::<T>::get(0).is_some());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(
        Pallet,
        crate::mock::new_test_ext(),
        crate::mock::TestRuntime
    );
}
