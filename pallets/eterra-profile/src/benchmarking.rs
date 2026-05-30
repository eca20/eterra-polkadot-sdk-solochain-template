#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks};
use frame_system::RawOrigin;

const BENCH_GAME: GameId = 1;
const BENCH_TARGET_GAME: GameId = 2;
const BENCH_COUNTER: CounterId = 1;
const BENCH_BADGE: BadgeId = 1;
const BENCH_FACT: PublicFactId = 1;
const BENCH_CAPABILITY: CapabilityId = 1;
const BENCH_AMOUNT: u64 = 10;

benchmarks! {
    increment_counter {
        let player: T::AccountId = account("player", 0, 0);
    }: _(RawOrigin::Root, player.clone(), BENCH_COUNTER, BENCH_AMOUNT)
    verify {
        assert_eq!(PassportCounters::<T>::get(&player, BENCH_COUNTER), BENCH_AMOUNT);
    }

    grant_badge {
        let player: T::AccountId = account("player", 0, 0);
    }: _(RawOrigin::Root, player.clone(), BENCH_BADGE)
    verify {
        assert!(Badges::<T>::get(&player, BENCH_BADGE));
    }

    revoke_badge {
        let player: T::AccountId = account("player", 0, 0);
        Badges::<T>::insert(&player, BENCH_BADGE, true);
    }: _(RawOrigin::Root, player.clone(), BENCH_BADGE)
    verify {
        assert!(!Badges::<T>::get(&player, BENCH_BADGE));
    }

    set_public_fact {
        let player: T::AccountId = account("player", 0, 0);
    }: _(RawOrigin::Root, BENCH_GAME, player.clone(), BENCH_FACT, PublicValue::U64(BENCH_AMOUNT))
    verify {
        assert_eq!(
            PublicFacts::<T>::get((BENCH_GAME, &player, BENCH_FACT)),
            Some(PublicValue::U64(BENCH_AMOUNT))
        );
    }

    grant_capability {
    }: _(RawOrigin::Root, BENCH_CAPABILITY, BENCH_GAME, BENCH_TARGET_GAME, BENCH_FACT, CapabilityPermission::ReadWrite, None)
    verify {
        assert!(Capabilities::<T>::contains_key(BENCH_CAPABILITY));
    }

    revoke_capability {
        Capabilities::<T>::insert(
            BENCH_CAPABILITY,
            CapabilityRecord::<T> {
                source_game_id: BENCH_GAME,
                target_game_id: BENCH_TARGET_GAME,
                fact_id: BENCH_FACT,
                permission: CapabilityPermission::ReadWrite,
                expires_at: None,
            },
        );
    }: _(RawOrigin::Root, BENCH_CAPABILITY)
    verify {
        assert!(!Capabilities::<T>::contains_key(BENCH_CAPABILITY));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test);
}
