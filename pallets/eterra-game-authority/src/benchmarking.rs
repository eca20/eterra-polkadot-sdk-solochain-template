#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::{BoundedBTreeSet, BoundedVec};
use frame_support::traits::Get;
use frame_system::RawOrigin;

benchmarks! {
    add_server {
        let server: T::AccountId = account("server", 0, 0);
    }: _(RawOrigin::Root, server.clone())
    verify {
        assert!(WhitelistedServers::<T>::contains_key(&server));
    }

    remove_server {
        let server: T::AccountId = account("server", 0, 0);
        WhitelistedServers::<T>::insert(&server, ());
    }: _(RawOrigin::Root, server.clone())
    verify {
        assert!(!WhitelistedServers::<T>::contains_key(&server));
    }

    create_game {
        let server: T::AccountId = whitelisted_caller();
        WhitelistedServers::<T>::insert(&server, ());
    }: _(RawOrigin::Signed(server.clone()))
    verify {
        assert!(Games::<T>::contains_key(0));
    }

    add_player {
        let server: T::AccountId = whitelisted_caller();
        let player: T::AccountId = account("player", 0, 0);
        WhitelistedServers::<T>::insert(&server, ());
        let game_id = NextGameId::<T>::get();
        let info = GameInfo::<T::AccountId, T::MaxPlayersPerGame> {
            server: server.clone(),
            players: BoundedBTreeSet::new(),
            started: true,
            ended: false,
        };
        Games::<T>::insert(game_id, info);
    }: _(RawOrigin::Signed(server.clone()), game_id, player.clone())
    verify {
        assert_eq!(ActiveGameByPlayer::<T>::get(&player), Some(game_id));
    }

    add_players_batch {
        let server: T::AccountId = whitelisted_caller();
        let n in 1 .. T::MaxBatchAdd::get();
        WhitelistedServers::<T>::insert(&server, ());
        let game_id = NextGameId::<T>::get();
        let info = GameInfo::<T::AccountId, T::MaxPlayersPerGame> {
            server: server.clone(),
            players: BoundedBTreeSet::new(),
            started: true,
            ended: false,
        };
        Games::<T>::insert(game_id, info);

        let mut players: BoundedVec<T::AccountId, T::MaxBatchAdd> = BoundedVec::default();
        for i in 0..n {
            let p: T::AccountId = account("player", i, 0);
            let _ = players.try_push(p);
        }
    }: _(RawOrigin::Signed(server.clone()), game_id, players.clone())
    verify {
        for p in players {
            assert_eq!(ActiveGameByPlayer::<T>::get(&p), Some(game_id));
        }
    }

    create_game_with_batch_add {
        let server: T::AccountId = whitelisted_caller();
        let n in 1 .. T::MaxBatchAdd::get();
        WhitelistedServers::<T>::insert(&server, ());

        let mut players: BoundedVec<T::AccountId, T::MaxBatchAdd> = BoundedVec::default();
        for i in 0..n {
            let p: T::AccountId = account("player", i, 0);
            let _ = players.try_push(p);
        }

        let game_id = NextGameId::<T>::get();
    }: _(RawOrigin::Signed(server.clone()), players.clone())
    verify {
        assert!(Games::<T>::contains_key(game_id));
        for p in players {
            assert_eq!(ActiveGameByPlayer::<T>::get(&p), Some(game_id));
        }
    }

    record_eliminations {
        let server: T::AccountId = whitelisted_caller();
        let player: T::AccountId = account("player", 0, 0);
        WhitelistedServers::<T>::insert(&server, ());
        let game_id = NextGameId::<T>::get();
        let mut info = GameInfo::<T::AccountId, T::MaxPlayersPerGame> {
            server: server.clone(),
            players: BoundedBTreeSet::new(),
            started: true,
            ended: false,
        };
        let _ = info.players.try_insert(player.clone());
        Games::<T>::insert(game_id, info);
    }: _(RawOrigin::Signed(server.clone()), game_id, player.clone(), 1u32)
    verify {
        assert_eq!(Eliminations::<T>::get(game_id, &player), 1u32);
    }

    end_game {
        let server: T::AccountId = whitelisted_caller();
        WhitelistedServers::<T>::insert(&server, ());
        let game_id = NextGameId::<T>::get();
        let info = GameInfo::<T::AccountId, T::MaxPlayersPerGame> {
            server: server.clone(),
            players: BoundedBTreeSet::new(),
            started: true,
            ended: false,
        };
        Games::<T>::insert(game_id, info);
    }: _(RawOrigin::Signed(server.clone()), game_id)
    verify {
        let game = Games::<T>::get(game_id).expect("game exists");
        assert!(game.ended);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(
        Pallet,
        crate::mock::new_test_ext(),
        crate::mock::Test
    );
}
