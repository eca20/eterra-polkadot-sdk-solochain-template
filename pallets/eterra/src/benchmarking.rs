#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::{traits::Get, BoundedVec};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use sp_std::vec::Vec;

use pallet_eterra_tcg as cards;

/// Seed finalized cards directly so gameplay benchmarks do not depend on the
/// full card-art season minting pipeline.
fn seed_cards<T: Config>(owner: &T::AccountId, start_id: u32, count: u32) -> Vec<u32> {
    let mut ids = Vec::new();
    let mut next_candidate = start_id;

    for _ in 0..count {
        let next_global = cards::NextCardId::<T>::get();
        let card_id = next_candidate.max(next_global);
        cards::Pallet::<T>::benchmark_seed_finalized_card(owner, card_id, [1, 1, 1, 1])
            .expect("benchmark card seed succeeds");
        ids.push(card_id);
        next_candidate = card_id.checked_add(1).expect("benchmark card id fits");
    }

    ids
}

fn seed_current_hand<T: Config>(owner: &T::AccountId, start_id: u32) -> Vec<u32> {
    let count = HandLimit::get();
    let ids = seed_cards::<T>(owner, start_id, count);
    let bounded: BoundedVec<u32, HandLimit> = ids.clone().try_into().expect("hand size fits; qed");
    CurrentHandOf::<T>::insert(owner, bounded);
    ids
}

fn seed_hand_entries(card_ids: &[u32]) -> BoundedVec<HandEntry, HandLimit> {
    let mut hand: BoundedVec<HandEntry, HandLimit> = BoundedVec::default();
    for &card_id in card_ids.iter() {
        let entry = HandEntry {
            card_id,
            north: 1,
            east: 1,
            south: 1,
            west: 1,
            used: false,
        };
        let _ = hand.try_push(entry);
    }
    hand
}

fn seed_game<T: Config>(
    players: Vec<T::AccountId>,
    game_mode: GameMode,
    player_turn: u8,
) -> GameId<T> {
    let current_block = frame_system::Pallet::<T>::block_number();
    let game_id = T::Hashing::hash_of(&(players[0].clone(), players[1].clone(), current_block));
    let game: Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers> = Game {
        state: GameState::Playing,
        last_played_block: current_block,
        players: players.try_into().expect("player count fits; qed"),
        player_turn,
        round: 0,
        max_rounds: T::MaxRounds::get(),
        board: Default::default(),
        locked_mask: 0,
        scores: (5, 5),
    };
    GameStorage::<T>::insert(&game_id, game);
    GameModes::<T>::insert(&game_id, game_mode);
    game_id
}

benchmarks! {
    create_game {
        let who: T::AccountId = whitelisted_caller();
        let _ = seed_current_hand::<T>(&who, 0);
        let players: BoundedVec<T::AccountId, T::NumPlayers> = BoundedVec::default();
        let game_mode = GameMode::PvE;
    }: _(RawOrigin::Signed(who.clone()), players, game_mode)
    verify {
        let ai = T::AiAccount::get();
        let current_block = frame_system::Pallet::<T>::block_number();
        let game_id = T::Hashing::hash_of(&(who.clone(), ai, current_block));
        assert!(GameStorage::<T>::contains_key(&game_id));
    }

    submit_hand {
        let who: T::AccountId = whitelisted_caller();
        let ai = T::AiAccount::get();
        let _ = seed_current_hand::<T>(&who, 0);
        let game_id = seed_game::<T>(sp_std::vec![who.clone(), ai.clone()], GameMode::PvE, 1);
    }: _(RawOrigin::Signed(who.clone()), game_id, BoundedVec::<u32, HandLimit>::default())
    verify {
        assert!(HandsOfGame::<T>::get(&game_id, &who).is_some());
    }

    play_from_hand {
        let who: T::AccountId = whitelisted_caller();
        let ai = T::AiAccount::get();
        let card_ids = seed_cards::<T>(&who, 0, HandLimit::get());
        let human_hand = seed_hand_entries(&card_ids);
        let ai_card_ids = seed_cards::<T>(&ai, 1000, HandLimit::get());
        let ai_hand = seed_hand_entries(&ai_card_ids);
        let game_id = seed_game::<T>(sp_std::vec![who.clone(), ai.clone()], GameMode::PvE, 0);
        HandsOfGame::<T>::insert(&game_id, &who, human_hand);
        HandsOfGame::<T>::insert(&game_id, &ai, ai_hand);
        // Make this a worst-case path: ensure the move ends the game with a non-draw winner,
        // triggering win rewards (COIN + devCOIN + betaCOIN + XP).
        GameStorage::<T>::mutate(&game_id, |maybe_game| {
            if let Some(g) = maybe_game {
                g.round = g.max_rounds;
                g.scores = (6, 5); // player 0 (who) wins
            }
        });
    }: _(RawOrigin::Signed(who.clone()), game_id, 0u8, 0u8, 0u8)
    verify {
        let game = GameStorage::<T>::get(&game_id).expect("game exists");
        assert!(game.board[0][0].is_some());
    }

    force_finish_turn {
        let p0: T::AccountId = account("player", 0, 0);
        let p1: T::AccountId = account("player", 1, 0);
        let game_id = seed_game::<T>(sp_std::vec![p0.clone(), p1.clone()], GameMode::PvP, 0);

        let limit: BlockNumberFor<T> = T::BlocksToPlayLimit::get().into();
        let now = limit.saturating_add(1u32.into());
        frame_system::Pallet::<T>::set_block_number(now);
        GameStorage::<T>::mutate(&game_id, |maybe_game| {
            if let Some(game) = maybe_game {
                game.last_played_block = 0u32.into();
                // Ensure this path ends the game and triggers rewards.
                game.round = game.max_rounds;
                game.scores = (6, 5); // player 0 (p0) wins
            }
        });
    }: _(RawOrigin::Signed(p1.clone()), game_id)
    verify {
        let game = GameStorage::<T>::get(&game_id).expect("game exists");
        assert!(matches!(game.state, GameState::Finished { winner: Some(0) }));
    }

    set_current_hand {
        let who: T::AccountId = whitelisted_caller();
        let card_ids = seed_cards::<T>(&who, 0, HandLimit::get());
        let bounded: BoundedVec<u32, HandLimit> = card_ids.clone().try_into().expect("hand size fits; qed");
    }: _(RawOrigin::Signed(who.clone()), bounded)
    verify {
        assert!(CurrentHandOf::<T>::contains_key(&who));
    }

    set_preset_hand {
        let who: T::AccountId = whitelisted_caller();
        let card_ids = seed_cards::<T>(&who, 100, HandLimit::get());
        let bounded: BoundedVec<u32, HandLimit> = card_ids.clone().try_into().expect("hand size fits; qed");
    }: _(RawOrigin::Signed(who.clone()), bounded)
    verify {
        assert!(CurrentHandOf::<T>::contains_key(&who));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
