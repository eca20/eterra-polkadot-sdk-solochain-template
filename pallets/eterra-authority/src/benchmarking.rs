#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks};
use frame_support::{traits::Get, BoundedVec};
use frame_system::RawOrigin;
use sp_runtime::traits::Hash as HashT;

const BENCH_GAME: GameId = 1;
const BENCH_AUTHORITY: AuthorityId = 1;
const BENCH_EVENT: EventTypeId = 1;
const BENCH_VERSION: VersionId = 1;

fn metadata_hash<T: Config>() -> T::Hash {
    T::Hashing::hash(b"eterra-authority-benchmark")
}

fn allowed_events<T: Config>() -> BoundedVec<EventTypeId, T::MaxAllowedEventsPerAuthority> {
    let mut events = BoundedVec::default();
    let max = T::MaxAllowedEventsPerAuthority::get();
    for event_type in 0..max {
        let _ = events.try_push(event_type.saturating_add(BENCH_EVENT));
    }
    events
}

fn seed_authority<T: Config>(account: T::AccountId) {
    Authorities::<T>::insert(
        BENCH_GAME,
        BENCH_AUTHORITY,
        AuthorityRecord::<T> {
            account: account.clone(),
            kind: AuthorityKind::GameServer,
            status: AuthorityStatus::Active,
            version_id: Some(BENCH_VERSION),
            allowed_events: allowed_events::<T>(),
            expires_at: None,
            metadata_hash: metadata_hash::<T>(),
        },
    );
    AuthorityByAccount::<T>::insert(BENCH_GAME, account, BENCH_AUTHORITY);
}

benchmarks! {
    authorize_authority {
        let authority: T::AccountId = account("authority", 0, 0);
        let events = allowed_events::<T>();
    }: _(RawOrigin::Root, BENCH_GAME, BENCH_AUTHORITY, authority.clone(), AuthorityKind::GameServer, Some(BENCH_VERSION), events, None, metadata_hash::<T>())
    verify {
        assert!(Authorities::<T>::contains_key(BENCH_GAME, BENCH_AUTHORITY));
        assert_eq!(
            AuthorityByAccount::<T>::get(BENCH_GAME, &authority),
            Some(BENCH_AUTHORITY)
        );
    }

    set_authority_status {
        let authority: T::AccountId = account("authority", 0, 0);
        seed_authority::<T>(authority);
    }: _(RawOrigin::Root, BENCH_GAME, BENCH_AUTHORITY, AuthorityStatus::Suspended)
    verify {
        let record = Authorities::<T>::get(BENCH_GAME, BENCH_AUTHORITY).expect("authority exists");
        assert_eq!(record.status, AuthorityStatus::Suspended);
    }

    revoke_authority {
        let authority: T::AccountId = account("authority", 0, 0);
        seed_authority::<T>(authority.clone());
    }: _(RawOrigin::Root, BENCH_GAME, BENCH_AUTHORITY)
    verify {
        assert!(!Authorities::<T>::contains_key(BENCH_GAME, BENCH_AUTHORITY));
        assert_eq!(AuthorityByAccount::<T>::get(BENCH_GAME, &authority), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test);
}
