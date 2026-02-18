#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::traits::Get;
use frame_support::BoundedVec;
use frame_system::RawOrigin;

fn short_bytes(max: u32, fill: u8) -> Vec<u8> {
    let len = max.min(8) as usize;
    vec![fill; len]
}

fn create_collection_for_bench<T: Config>(owner: &T::AccountId) -> MediaCollectionId {
    let collection_id = NextCollectionId::<T>::get();
    let name: BoundedVec<u8, <T as Config>::MaxNameLen> = short_bytes(T::MaxNameLen::get(), b'n')
        .try_into()
        .expect("within max len");
    let desc: BoundedVec<u8, <T as Config>::MaxDescriptionLen> =
        short_bytes(T::MaxDescriptionLen::get(), b'd')
            .try_into()
            .expect("within max len");
    let _ = Pallet::<T>::create_collection(RawOrigin::Signed(owner.clone()).into(), name, desc);
    collection_id
}

benchmarks! {
    create_collection {
        let caller: T::AccountId = whitelisted_caller();
        let name: BoundedVec<u8, <T as Config>::MaxNameLen> = short_bytes(T::MaxNameLen::get(), b'n')
            .try_into()
            .expect("within max len");
        let desc: BoundedVec<u8, <T as Config>::MaxDescriptionLen> = short_bytes(T::MaxDescriptionLen::get(), b'd')
            .try_into()
            .expect("within max len");
    }: _(RawOrigin::Signed(caller.clone()), name, desc)
    verify {
        let id = NextCollectionId::<T>::get().saturating_sub(1);
        assert!(Collections::<T>::get(id).is_some());
    }

    set_collection_role {
        let caller: T::AccountId = whitelisted_caller();
        let collection_id = create_collection_for_bench::<T>(&caller);
        let target: T::AccountId = account("user", 0, 0);
    }: _(RawOrigin::Signed(caller.clone()), collection_id, target.clone(), CollectionRole::Uploader, true)
    verify {
        let roles = CollectionRoles::<T>::get(collection_id, &target);
        assert!(roles.contains(&CollectionRole::Uploader));
    }

    register_media {
        let caller: T::AccountId = whitelisted_caller();
        let collection_id = create_collection_for_bench::<T>(&caller);
        let uri: BoundedVec<u8, <T as Config>::MaxUriLen> = short_bytes(T::MaxUriLen::get(), b'u')
            .try_into()
            .expect("within max len");
        let content_type: BoundedVec<u8, <T as Config>::MaxContentTypeLen> =
            short_bytes(T::MaxContentTypeLen::get(), b'c')
                .try_into()
                .expect("within max len");
        let class = MediaClass::CoreAsset;
        let delivery = Delivery::RemoteIpfs;
        let size_bytes = Some(123u64);
    }: _(RawOrigin::Signed(caller.clone()), Some(collection_id), uri, content_type, class, delivery, size_bytes)
    verify {
        let media_id = NextMediaId::<T>::get().saturating_sub(1);
        assert!(Media::<T>::get(media_id).is_some());
    }

    freeze_collection {
        let caller: T::AccountId = whitelisted_caller();
        let collection_id = create_collection_for_bench::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()), collection_id)
    verify {
        let info = Collections::<T>::get(collection_id).expect("collection exists");
        assert!(info.frozen);
    }

    deprecate_media {
        let caller: T::AccountId = whitelisted_caller();
        let collection_id = create_collection_for_bench::<T>(&caller);
        let uri: BoundedVec<u8, <T as Config>::MaxUriLen> = short_bytes(T::MaxUriLen::get(), b'u')
            .try_into()
            .expect("within max len");
        let content_type: BoundedVec<u8, <T as Config>::MaxContentTypeLen> =
            short_bytes(T::MaxContentTypeLen::get(), b'c')
                .try_into()
                .expect("within max len");
        let class = MediaClass::CoreAsset;
        let delivery = Delivery::RemoteIpfs;
        let size_bytes = Some(123u64);
        let media_id = NextMediaId::<T>::get();
        let _ = Pallet::<T>::register_media(
            RawOrigin::Signed(caller.clone()).into(),
            Some(collection_id),
            uri,
            content_type,
            class,
            delivery,
            size_bytes,
        );
    }: _(RawOrigin::Signed(caller.clone()), media_id)
    verify {
        let meta = Media::<T>::get(media_id).expect("media exists");
        assert!(meta.is_deprecated);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
