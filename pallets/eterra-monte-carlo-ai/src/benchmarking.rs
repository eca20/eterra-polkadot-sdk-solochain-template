#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_system::RawOrigin;

benchmarks! {
    suggest_move {
        let caller: T::AccountId = whitelisted_caller();
        let state = T::BenchmarkHelper::bench_state();
        let difficulty = T::BenchmarkHelper::bench_difficulty();
    }: _(RawOrigin::Signed(caller), state, difficulty)
    verify {
        assert!(Nonce::<T>::get() > 0);
    }
}
