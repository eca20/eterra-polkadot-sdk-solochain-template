#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_system::RawOrigin;

benchmarks! {
    add_admin {
        let admin: T::AccountId = whitelisted_caller();
        let new_admin: T::AccountId = account("admin", 0, 0);
    }: _(RawOrigin::Root, new_admin.clone())
    verify {
        assert!(Admins::<T>::contains_key(&new_admin));
    }

    create_season {
        let admin: T::AccountId = whitelisted_caller();
        Admins::<T>::insert(&admin, ());
        let name: BoundedVec<u8, T::MaxSeasonNameLen> = b"Season".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, T::MaxSeasonDescLen> = b"Desc".to_vec().try_into().unwrap();
    }: _(RawOrigin::Signed(admin), name, desc)
    verify {
        assert!(Seasons::<T>::get(NextSeasonId::<T>::get().saturating_sub(1)).is_some());
    }
}
