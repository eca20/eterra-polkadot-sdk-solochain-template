#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::{Currency, Get};
use frame_system::RawOrigin;
use sp_runtime::traits::Saturating;
use sp_std::vec;
use sp_std::vec::Vec;

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

fn fund<T: Config>(who: &T::AccountId, amount: BalanceOf<T>) {
	let _ = T::Currency::deposit_creating(who, amount);
}

fn short_bytes(len: u32, byte: u8) -> Vec<u8> {
	let len = core::cmp::max(len, 1) as usize;
	vec![byte; len]
}

benchmarks! {
	set_gamer_tag {
		let caller: T::AccountId = whitelisted_caller();
		let tag = short_bytes(T::MaxTagLen::get(), b't');
		let existing: BoundedVec<u8, T::MaxTagLen> = tag.clone().try_into().expect("len ok");
		GamerTag::<T>::insert(&caller, existing);

		let fee = T::ChangeFee::get();
		let min = T::Currency::minimum_balance();
		// Ensure faucet account exists so a small fee transfer doesn't fail below ED.
		let faucet = T::FaucetAccount::get();
		fund::<T>(&faucet, min);
		let fund_amount = fee.saturating_add(min);
		fund::<T>(&caller, fund_amount);
	}: _(RawOrigin::Signed(caller.clone()), tag)
	verify {
		assert!(GamerTag::<T>::contains_key(&caller));
	}

	set_avatar {
		let caller: T::AccountId = whitelisted_caller();
		let cid = short_bytes(T::MaxAvatarCidLen::get(), b'Q');
		let existing: BoundedVec<u8, T::MaxAvatarCidLen> = cid.clone().try_into().expect("len ok");
		AvatarCid::<T>::insert(&caller, existing);

		let fee = T::ChangeFee::get();
		let min = T::Currency::minimum_balance();
		// Ensure faucet account exists so a small fee transfer doesn't fail below ED.
		let faucet = T::FaucetAccount::get();
		fund::<T>(&faucet, min);
		let fund_amount = fee.saturating_add(min);
		fund::<T>(&caller, fund_amount);
	}: _(RawOrigin::Signed(caller.clone()), cid)
	verify {
		assert!(AvatarCid::<T>::contains_key(&caller));
	}

	grant_experience {
		let target: T::AccountId = whitelisted_caller();
		let amount: u128 = 1_000;
	}: _(RawOrigin::Root, target.clone(), amount)
	verify {
		assert!(Experience::<T>::get(&target) >= amount);
	}

	redeem_levels {
		let caller: T::AccountId = whitelisted_caller();
		Level::<T>::insert(&caller, 0u8);
		Experience::<T>::insert(&caller, 1_000_000_000u128);
	}: _(RawOrigin::Signed(caller.clone()))
	verify {
		assert!(Level::<T>::get(&caller) > 0);
	}
}
