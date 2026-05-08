#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::traits::Currency;
use frame_system::RawOrigin;
use sp_runtime::traits::Saturating;

fn fund<T: Config>(who: &T::AccountId, amount: BalanceOf<T>) {
    let _ = T::Currency::deposit_creating(who, amount);
}

benchmarks! {
    claim {
        let caller: T::AccountId = whitelisted_caller();
        let faucet: T::AccountId = account("faucet", 0, 0);
        let amount: BalanceOf<T> = T::Currency::minimum_balance();
        let fund_amount = amount.saturating_add(amount);

        FaucetAccount::<T>::put(&faucet);
        PayoutAmount::<T>::put(amount);
        fund::<T>(&faucet, fund_amount);
    }: _(RawOrigin::Signed(caller.clone()), caller.clone())
    verify {
        assert!(LastClaim::<T>::get(&caller).is_some());
    }
}
