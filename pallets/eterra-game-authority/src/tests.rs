// pallets/eterra-game-authority/src/tests.rs

#![cfg(test)]

use crate::{self as pallet_eterra_game_authority, Error, Pallet as GamePallet};
use crate::mock::*;
use frame_support::{assert_noop, assert_ok};
use frame_support::traits::Get;
use frame_support::traits::Hooks;

#[test]
fn genesis_whitelist_allows_alice_to_create_game() {
    ExtBuilder::default()
        .with_servers(vec![ALICE]) // whitelist //Alice at genesis
        .build()
        .execute_with(|| {
            // Alice is whitelisted, should succeed
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));

            // Bob is not whitelisted, should fail
            assert_noop!(
                GamePallet::<Test>::create_game(RuntimeOrigin::signed(BOB)),
                Error::<Test>::NotWhitelistedServer
            );
        });
}

#[test]
fn player_cannot_be_in_two_active_games() {
    ExtBuilder::default()
        .with_servers(vec![ALICE]) // Alice is our server
        .build()
        .execute_with(|| {
            // Create Game #0 by Alice
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));
            // Add BOB to Game #0
            assert_ok!(GamePallet::<Test>::add_player(RuntimeOrigin::signed(ALICE), 0, BOB));

            // Create Game #1 by Alice
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));

            // Attempt to add BOB again to Game #1 while Game #0 is still active -> should fail
            assert_noop!(
                GamePallet::<Test>::add_player(RuntimeOrigin::signed(ALICE), 1, BOB),
                Error::<Test>::PlayerInAnotherActiveGame
            );

            // End Game #0
            assert_ok!(GamePallet::<Test>::end_game(RuntimeOrigin::signed(ALICE), 0));

            // Now BOB can be added to Game #1
            assert_ok!(GamePallet::<Test>::add_player(RuntimeOrigin::signed(ALICE), 1, BOB));
        });
}

#[test]
fn only_owning_server_can_record_elims() {
    ExtBuilder::default()
        .with_servers(vec![ALICE, CHARLIE]) // both whitelisted
        .build()
        .execute_with(|| {
            // Alice creates game #0
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));

            // Add Bob as player
            assert_ok!(GamePallet::<Test>::add_player(RuntimeOrigin::signed(ALICE), 0, BOB));

            // Owning server (Alice) can record eliminations
            assert_ok!(GamePallet::<Test>::record_eliminations(
                RuntimeOrigin::signed(ALICE),
                0,
                BOB,
                3
            ));

            // Non-owning server (Charlie) is whitelisted but not game owner -> should fail
            assert_noop!(
                GamePallet::<Test>::record_eliminations(
                    RuntimeOrigin::signed(CHARLIE),
                    0,
                    BOB,
                    1
                ),
                Error::<Test>::NotGameOwnerServer
            );

            // Verify the counter is 3
            let val = pallet_eterra_game_authority::Eliminations::<Test>::get(0, BOB);
            assert_eq!(val, 3);
        });
}

#[test]
fn end_game_clears_active_mapping() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            // Create and populate game #0
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));
            assert_ok!(GamePallet::<Test>::add_player(RuntimeOrigin::signed(ALICE), 0, BOB));

            // Active mapping should be set
            let active = pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(BOB);
            assert_eq!(active, Some(0));

            // End the game
            assert_ok!(GamePallet::<Test>::end_game(RuntimeOrigin::signed(ALICE), 0));

            // Mapping cleared -> None
            let active = pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(BOB);
            assert_eq!(active, None);
        });
}

#[test]
fn admin_can_add_and_remove_server() {
    ExtBuilder::default()
        .build()
        .execute_with(|| {
            // Initially, Alice is not whitelisted; creating a game fails
            assert_noop!(
                GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)),
                Error::<Test>::NotWhitelistedServer
            );

            // Root adds Alice to whitelist
            assert_ok!(GamePallet::<Test>::add_server(RuntimeOrigin::root(), ALICE));

            // Now Alice can create a game
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));

            // Root removes Alice from whitelist
            assert_ok!(GamePallet::<Test>::remove_server(RuntimeOrigin::root(), ALICE));

            // Alice can't create new games anymore
            assert_noop!(
                GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)),
                Error::<Test>::NotWhitelistedServer
            );
        });
}

#[test]
fn game_auto_ends_after_max_round_time_and_clears_players() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            // Create game #0 and add BOB.
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));
            assert_ok!(GamePallet::<Test>::add_player(RuntimeOrigin::signed(ALICE), 0, BOB));

            // Compute expire_at = now + MaxRoundBlocks (mock uses ConstU64<30>)
            let now = System::block_number();
            let expire_at = now + 30;

            // Fast-forward to the exact expiration block and run hooks.
            System::set_block_number(expire_at);
            GamePallet::<Test>::on_initialize(expire_at);

            // Game is ended…
            let game = pallet_eterra_game_authority::Games::<Test>::get(0).expect("game exists");
            assert!(game.ended);

            // …and ActiveGameByPlayer is cleared for BOB.
            let active = pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(BOB);
            assert_eq!(active, None);
        });
}

#[test]
fn player_can_join_new_game_after_auto_end() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            // Create game #0 and add BOB.
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));
            assert_ok!(GamePallet::<Test>::add_player(RuntimeOrigin::signed(ALICE), 0, BOB));

            // Let it auto-expire.
            let expire_at = System::block_number() + 30;
            System::set_block_number(expire_at);
            GamePallet::<Test>::on_initialize(expire_at);

            // Create a new game #1 and add BOB again — should succeed.
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));
            assert_ok!(GamePallet::<Test>::add_player(RuntimeOrigin::signed(ALICE), 1, BOB));
        });
}

#[test]
fn manual_end_before_expiration_is_idempotent() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            // Create game #0 and add BOB.
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));
            assert_ok!(GamePallet::<Test>::add_player(RuntimeOrigin::signed(ALICE), 0, BOB));

            // Manually end the game before expiration.
            assert_ok!(GamePallet::<Test>::end_game(RuntimeOrigin::signed(ALICE), 0));

            // Run on_initialize at the would-be expiration block; nothing should break.
            let expire_at = System::block_number() + 30;
            System::set_block_number(expire_at);
            GamePallet::<Test>::on_initialize(expire_at);

            // Still ended, and mapping is cleared.
            let game = pallet_eterra_game_authority::Games::<Test>::get(0).expect("game exists");
            assert!(game.ended);
            let active = pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(BOB);
            assert_eq!(active, None);
        });
}

#[test]
fn create_game_schedules_expiration() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));
            let expire_at = System::block_number() + 30;

            // The game id 0 should be present in the expirations list for expire_at.
            let scheduled = pallet_eterra_game_authority::Expirations::<Test>::get(expire_at);
            assert!(scheduled.contains(&0), "game 0 not scheduled for expiration at {}", expire_at);
        });
}

#[test]
fn create_game_fails_when_expiration_bucket_full() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            let max = <Test as pallet_eterra_game_authority::Config>::MaxExpirationsPerBlock::get();
            for _ in 0..max {
                assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));
            }

            assert_noop!(
                GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)),
                Error::<Test>::TooManyExpirations
            );
        });
}

use frame_support::BoundedVec;

#[test]
fn batch_add_happy_path_adds_multiple_players() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            // Create game #0 by Alice
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));

            // Batch add BOB and CHARLIE
            let players: BoundedVec<_, <Test as pallet_eterra_game_authority::Config>::MaxBatchAdd> =
                BoundedVec::try_from(vec![BOB, CHARLIE]).expect("within MaxBatchAdd");
            assert_ok!(GamePallet::<Test>::add_players_batch(RuntimeOrigin::signed(ALICE), 0, players));

            // Both are in the game and active mapping set
            let game = pallet_eterra_game_authority::Games::<Test>::get(0).expect("game exists");
            assert!(game.players.contains(&BOB));
            assert!(game.players.contains(&CHARLIE));
            assert_eq!(pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(BOB), Some(0));
            assert_eq!(pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(CHARLIE), Some(0));
        });
}

#[test]
fn batch_add_skips_players_already_in_other_active_game() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            // Game #0: add BOB as single add
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));
            assert_ok!(GamePallet::<Test>::add_player(RuntimeOrigin::signed(ALICE), 0, BOB));

            // Game #1: batch add tries to add BOB (already active) and CHARLIE (free)
            assert_ok!(GamePallet::<Test>::create_game(RuntimeOrigin::signed(ALICE)));
            let players: BoundedVec<_, <Test as pallet_eterra_game_authority::Config>::MaxBatchAdd> =
                BoundedVec::try_from(vec![BOB, CHARLIE]).unwrap();
            assert_ok!(GamePallet::<Test>::add_players_batch(RuntimeOrigin::signed(ALICE), 1, players));

            // BOB remains only in game #0
            let game0 = pallet_eterra_game_authority::Games::<Test>::get(0).unwrap();
            let game1 = pallet_eterra_game_authority::Games::<Test>::get(1).unwrap();
            assert!(game0.players.contains(&BOB));
            assert!(!game1.players.contains(&BOB));
            // CHARLIE added to game #1
            assert!(game1.players.contains(&CHARLIE));
            assert_eq!(pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(BOB), Some(0));
            assert_eq!(pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(CHARLIE), Some(1));
        });
}

#[test]
fn create_game_with_batch_add_creates_and_adds_players() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            let players: BoundedVec<_, <Test as pallet_eterra_game_authority::Config>::MaxBatchAdd> =
                BoundedVec::try_from(vec![BOB, CHARLIE]).unwrap();
            assert_ok!(GamePallet::<Test>::create_game_with_batch_add(RuntimeOrigin::signed(ALICE), players));

            // Game #0 exists, started, players present
            let game = pallet_eterra_game_authority::Games::<Test>::get(0).expect("game exists");
            assert!(game.started && !game.ended);
            assert!(game.players.contains(&BOB));
            assert!(game.players.contains(&CHARLIE));

            // Active mapping set
            assert_eq!(pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(BOB), Some(0));
            assert_eq!(pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(CHARLIE), Some(0));

            // Expiration scheduled at now + 30 (from mock MaxRoundBlocks)
            let expire_at = System::block_number() + 30;
            let scheduled = pallet_eterra_game_authority::Expirations::<Test>::get(expire_at);
            assert!(scheduled.contains(&0));
        });
}

#[test]
fn create_game_with_batch_add_skips_duplicates() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            let players: BoundedVec<_, <Test as pallet_eterra_game_authority::Config>::MaxBatchAdd> =
                BoundedVec::try_from(vec![BOB, BOB, CHARLIE]).unwrap();
            assert_ok!(GamePallet::<Test>::create_game_with_batch_add(RuntimeOrigin::signed(ALICE), players));

            let game = pallet_eterra_game_authority::Games::<Test>::get(0).unwrap();
            assert!(game.players.contains(&BOB));
            assert!(game.players.contains(&CHARLIE));
            // Only unique players should be present
            let count = game.players.len();
            assert_eq!(count, 2);
        });
}
