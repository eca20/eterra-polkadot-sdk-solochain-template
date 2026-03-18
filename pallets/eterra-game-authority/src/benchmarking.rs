#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::{BoundedBTreeSet, BoundedVec};
use frame_support::traits::Get;
use frame_system::RawOrigin;
use sp_std::vec::Vec;

fn request_id<T: Config>(value: &[u8]) -> RequestIdOf<T> {
    BoundedVec::try_from(value.to_vec()).expect("request id within bounds")
}

fn outcome<T: Config>(value: &[u8]) -> OutcomeOf<T> {
    BoundedVec::try_from(value.to_vec()).expect("outcome within bounds")
}

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

    create_game_with_round_id {
        let server: T::AccountId = whitelisted_caller();
        let n in 1 .. T::MaxBatchAdd::get();
        WhitelistedServers::<T>::insert(&server, ());

        let mut players: BoundedVec<T::AccountId, T::MaxBatchAdd> = BoundedVec::default();
        for i in 0..n {
            let player: T::AccountId = account("player", i, 0);
            let _ = players.try_push(player);
        }

        let round_id = request_id::<T>(b"bench-round");
        let game_id = NextGameId::<T>::get();
    }: _(RawOrigin::Signed(server.clone()), round_id.clone(), players.clone())
    verify {
        assert_eq!(GameIdByRoundId::<T>::get(&round_id), Some(game_id));
        assert!(Games::<T>::contains_key(game_id));
        for player in players {
            assert_eq!(ActiveGameByPlayer::<T>::get(&player), Some(game_id));
        }
    }

    end_game_with_command_id {
        let server: T::AccountId = whitelisted_caller();
        WhitelistedServers::<T>::insert(&server, ());
        let game_id = NextGameId::<T>::get();
        let mut info = GameInfo::<T::AccountId, T::MaxPlayersPerGame> {
            server: server.clone(),
            players: BoundedBTreeSet::new(),
            started: true,
            ended: false,
        };
        let max_players = T::MaxPlayersPerGame::get();
        let mut players = Vec::new();
        for i in 0..max_players {
            let player: T::AccountId = account("player", i, 0);
            let _ = info.players.try_insert(player.clone());
            ActiveGameByPlayer::<T>::insert(&player, game_id);
            players.push(player);
        }
        Games::<T>::insert(game_id, info);
        let command_id = request_id::<T>(b"bench-end-command");
        let outcome = outcome::<T>(b"round_complete");
    }: _(RawOrigin::Signed(server.clone()), game_id, command_id.clone(), outcome)
    verify {
        let game = Games::<T>::get(game_id).expect("game exists");
        assert!(game.ended);
        for player in players {
            assert_eq!(ActiveGameByPlayer::<T>::get(&player), None);
        }
        assert!(ProcessedEndCommands::<T>::contains_key(&command_id));
    }

    record_eliminations_with_event_id {
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
        ActiveGameByPlayer::<T>::insert(&player, game_id);
        let event_id = request_id::<T>(b"bench-elim-event");
    }: _(RawOrigin::Signed(server.clone()), game_id, event_id.clone(), player.clone(), 1u32)
    verify {
        assert_eq!(Eliminations::<T>::get(game_id, &player), 1u32);
        assert!(ProcessedEliminationEvents::<T>::contains_key(&event_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
