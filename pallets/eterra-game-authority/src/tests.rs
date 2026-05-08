#![cfg(test)]

use crate::mock::*;
use crate::pallet::{OutcomeOf, RequestIdOf};
use crate::{self as pallet_eterra_game_authority, Error, Pallet as GamePallet};
use frame_support::traits::Get;
use frame_support::traits::Hooks;
use frame_support::{assert_noop, assert_ok, BoundedVec};

fn request_id(value: &str) -> RequestIdOf<Test> {
    BoundedVec::try_from(value.as_bytes().to_vec()).expect("request id within bounds")
}

fn outcome(value: &str) -> OutcomeOf<Test> {
    BoundedVec::try_from(value.as_bytes().to_vec()).expect("outcome within bounds")
}

fn players(
    values: Vec<AccountId>,
) -> BoundedVec<AccountId, <Test as pallet_eterra_game_authority::Config>::MaxBatchAdd> {
    BoundedVec::try_from(values).expect("players within bounds")
}

#[test]
fn duplicate_create_game_with_round_id_creates_one_game_only() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            let round_id = request_id("round-1");
            let game_players = players(vec![BOB, CHARLIE]);

            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                round_id.clone(),
                game_players.clone(),
            ));
            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                round_id.clone(),
                game_players,
            ));

            assert_eq!(pallet_eterra_game_authority::NextGameId::<Test>::get(), 1);
            assert_eq!(
                pallet_eterra_game_authority::GameIdByRoundId::<Test>::get(&round_id),
                Some(0)
            );

            let game = pallet_eterra_game_authority::Games::<Test>::get(0).expect("game exists");
            assert!(game.players.contains(&BOB));
            assert!(game.players.contains(&CHARLIE));
            assert_eq!(
                pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(BOB),
                Some(0)
            );
            assert_eq!(
                pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(CHARLIE),
                Some(0)
            );
        });
}

#[test]
fn duplicate_create_game_with_round_id_does_not_schedule_second_expiration() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            let round_id = request_id("round-expiration");
            let game_players = players(vec![BOB]);

            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                round_id.clone(),
                game_players.clone(),
            ));
            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                round_id,
                game_players,
            ));

            let expire_at = System::block_number() + 30;
            let scheduled = pallet_eterra_game_authority::Expirations::<Test>::get(expire_at);
            assert_eq!(scheduled.len(), 1);
            assert!(scheduled.contains(&0));
        });
}

#[test]
fn game_id_by_round_id_resolves_after_successful_create() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            let round_id = request_id("round-resolve");

            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                round_id.clone(),
                players(vec![BOB]),
            ));

            assert_eq!(
                pallet_eterra_game_authority::GameIdByRoundId::<Test>::get(&round_id),
                Some(0)
            );
        });
}

#[test]
fn different_round_ids_apply_independently() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            let round_a = request_id("round-a");
            let round_b = request_id("round-b");

            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                round_a.clone(),
                players(vec![BOB]),
            ));
            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                round_b.clone(),
                players(vec![CHARLIE]),
            ));

            assert_eq!(pallet_eterra_game_authority::NextGameId::<Test>::get(), 2);
            assert_eq!(
                pallet_eterra_game_authority::GameIdByRoundId::<Test>::get(&round_a),
                Some(0)
            );
            assert_eq!(
                pallet_eterra_game_authority::GameIdByRoundId::<Test>::get(&round_b),
                Some(1)
            );
        });
}

#[test]
fn failed_create_does_not_poison_round_id() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            let max = <Test as pallet_eterra_game_authority::Config>::MaxExpirationsPerBlock::get();
            for index in 0..max {
                assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                    RuntimeOrigin::signed(ALICE),
                    request_id(&format!("fill-{index}")),
                    players(vec![BOB]),
                ));
            }

            let failed_round = request_id("overflow-round");
            assert_noop!(
                GamePallet::<Test>::create_game_with_round_id(
                    RuntimeOrigin::signed(ALICE),
                    failed_round.clone(),
                    players(vec![CHARLIE]),
                ),
                Error::<Test>::TooManyExpirations
            );
            assert_eq!(
                pallet_eterra_game_authority::GameIdByRoundId::<Test>::get(&failed_round),
                None
            );
        });
}

#[test]
fn duplicate_end_game_with_command_id_ends_once_only() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                request_id("round-end"),
                players(vec![BOB]),
            ));

            let command_id = request_id("end:round-end:0:complete");
            let result = GamePallet::<Test>::end_game_with_command_id(
                RuntimeOrigin::signed(ALICE),
                0,
                command_id.clone(),
                outcome("round_complete"),
            );
            assert_ok!(result);
            assert_ok!(GamePallet::<Test>::end_game_with_command_id(
                RuntimeOrigin::signed(ALICE),
                0,
                command_id.clone(),
                outcome("round_complete"),
            ));

            let game = pallet_eterra_game_authority::Games::<Test>::get(0).expect("game exists");
            assert!(game.ended);
            assert_eq!(
                pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(BOB),
                None
            );
            assert!(
                pallet_eterra_game_authority::ProcessedEndCommands::<Test>::contains_key(
                    &command_id
                )
            );
        });
}

#[test]
fn processed_end_command_flips_only_after_successful_end() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                request_id("round-end-flip"),
                players(vec![BOB]),
            ));

            let failed_command = request_id("end:missing");
            assert_noop!(
                GamePallet::<Test>::end_game_with_command_id(
                    RuntimeOrigin::signed(ALICE),
                    99,
                    failed_command.clone(),
                    outcome("missing"),
                ),
                Error::<Test>::GameNotFound
            );
            assert!(
                pallet_eterra_game_authority::ProcessedEndCommands::<Test>::get(&failed_command)
                    .is_none()
            );

            let command_id = request_id("end:round-end-flip");
            assert!(
                pallet_eterra_game_authority::ProcessedEndCommands::<Test>::get(&command_id)
                    .is_none()
            );
            assert_ok!(GamePallet::<Test>::end_game_with_command_id(
                RuntimeOrigin::signed(ALICE),
                0,
                command_id.clone(),
                outcome("round_complete"),
            ));

            let processed =
                pallet_eterra_game_authority::ProcessedEndCommands::<Test>::get(&command_id)
                    .expect("processed end command exists");
            assert_eq!(processed.game_id, 0);
        });
}

#[test]
fn games_storage_ended_flag_matches_actual_state() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                request_id("round-ended-flag"),
                players(vec![BOB]),
            ));

            let before = pallet_eterra_game_authority::Games::<Test>::get(0).expect("game exists");
            assert!(!before.ended);

            assert_ok!(GamePallet::<Test>::end_game_with_command_id(
                RuntimeOrigin::signed(ALICE),
                0,
                request_id("end:round-ended-flag"),
                outcome("round_complete"),
            ));

            let after = pallet_eterra_game_authority::Games::<Test>::get(0).expect("game exists");
            assert!(after.ended);
        });
}

#[test]
fn end_command_after_auto_end_records_once_and_returns_success() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                request_id("round-auto-end"),
                players(vec![BOB]),
            ));

            let expire_at = System::block_number() + 30;
            System::set_block_number(expire_at);
            GamePallet::<Test>::on_initialize(expire_at);

            let command_id = request_id("end:auto-end");
            assert_ok!(GamePallet::<Test>::end_game_with_command_id(
                RuntimeOrigin::signed(ALICE),
                0,
                command_id.clone(),
                outcome("round_complete"),
            ));
            assert_ok!(GamePallet::<Test>::end_game_with_command_id(
                RuntimeOrigin::signed(ALICE),
                0,
                command_id.clone(),
                outcome("round_complete"),
            ));

            let game = pallet_eterra_game_authority::Games::<Test>::get(0).expect("game exists");
            assert!(game.ended);
            assert_eq!(
                pallet_eterra_game_authority::ActiveGameByPlayer::<Test>::get(BOB),
                None
            );
            assert!(
                pallet_eterra_game_authority::ProcessedEndCommands::<Test>::contains_key(
                    &command_id
                )
            );
        });
}

#[test]
fn duplicate_record_eliminations_with_event_id_increments_once_only() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                request_id("round-elim"),
                players(vec![BOB]),
            ));

            let event_id = request_id("elim:round-elim:0:2");
            assert_ok!(GamePallet::<Test>::record_eliminations_with_event_id(
                RuntimeOrigin::signed(ALICE),
                0,
                event_id.clone(),
                BOB,
                2,
            ));
            assert_ok!(GamePallet::<Test>::record_eliminations_with_event_id(
                RuntimeOrigin::signed(ALICE),
                0,
                event_id.clone(),
                BOB,
                2,
            ));

            assert_eq!(
                pallet_eterra_game_authority::Eliminations::<Test>::get(0, BOB),
                2
            );
            assert!(pallet_eterra_game_authority::ProcessedEliminationEvents::<
                Test,
            >::contains_key(&event_id));
        });
}

#[test]
fn different_event_ids_apply_independently() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                request_id("round-two-events"),
                players(vec![BOB]),
            ));

            assert_ok!(GamePallet::<Test>::record_eliminations_with_event_id(
                RuntimeOrigin::signed(ALICE),
                0,
                request_id("elim:1"),
                BOB,
                2,
            ));
            assert_ok!(GamePallet::<Test>::record_eliminations_with_event_id(
                RuntimeOrigin::signed(ALICE),
                0,
                request_id("elim:2"),
                BOB,
                1,
            ));

            assert_eq!(
                pallet_eterra_game_authority::Eliminations::<Test>::get(0, BOB),
                3
            );
        });
}

#[test]
fn processed_elimination_event_flips_only_after_successful_record() {
    ExtBuilder::default()
        .with_servers(vec![ALICE])
        .build()
        .execute_with(|| {
            assert_ok!(GamePallet::<Test>::create_game_with_round_id(
                RuntimeOrigin::signed(ALICE),
                request_id("round-elim-flip"),
                players(vec![BOB]),
            ));

            let failed_event = request_id("elim:missing-player");
            assert_noop!(
                GamePallet::<Test>::record_eliminations_with_event_id(
                    RuntimeOrigin::signed(ALICE),
                    0,
                    failed_event.clone(),
                    CHARLIE,
                    1,
                ),
                Error::<Test>::PlayerNotInGame
            );
            assert!(
                pallet_eterra_game_authority::ProcessedEliminationEvents::<Test>::get(
                    &failed_event
                )
                .is_none()
            );

            let event_id = request_id("elim:round-elim-flip");
            assert!(
                pallet_eterra_game_authority::ProcessedEliminationEvents::<Test>::get(&event_id)
                    .is_none()
            );
            assert_ok!(GamePallet::<Test>::record_eliminations_with_event_id(
                RuntimeOrigin::signed(ALICE),
                0,
                event_id.clone(),
                BOB,
                3,
            ));

            let processed =
                pallet_eterra_game_authority::ProcessedEliminationEvents::<Test>::get(&event_id)
                    .expect("processed elimination exists");
            assert_eq!(processed.game_id, 0);
            assert_eq!(processed.player, BOB);
            assert_eq!(processed.delta, 3);
        });
}
