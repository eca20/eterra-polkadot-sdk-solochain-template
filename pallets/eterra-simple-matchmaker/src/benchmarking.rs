#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::traits::Get;
use frame_system::RawOrigin;

benchmarks! {
    join_queue {
        let caller: T::AccountId = whitelisted_caller();
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert!(InQueue::<T>::contains_key(&caller));
    }

    leave_queue {
        let caller: T::AccountId = whitelisted_caller();
        InQueue::<T>::insert(&caller, ());
        LiveSize::<T>::put(1);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert!(!InQueue::<T>::contains_key(&caller));
    }

    process_queue {
        let cap = T::QueueCapacity::get();
        let a: T::AccountId = account("a", 0, 0);
        let b: T::AccountId = account("b", 1, 0);

        Head::<T>::put(0);
        Tail::<T>::put(0);
        LiveSize::<T>::put(0);

        let idx0 = 0 % cap;
        Ring::<T>::insert(idx0, &a);
        Tail::<T>::put(1);
        InQueue::<T>::insert(&a, ());
        LiveSize::<T>::put(1);

        let idx1 = 1 % cap;
        Ring::<T>::insert(idx1, &b);
        Tail::<T>::put(2);
        InQueue::<T>::insert(&b, ());
        LiveSize::<T>::put(2);
    }: _(RawOrigin::Signed(a.clone()))
    verify {
        assert!(LiveSize::<T>::get() < 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
