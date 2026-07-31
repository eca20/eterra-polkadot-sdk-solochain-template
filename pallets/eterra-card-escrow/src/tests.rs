#![cfg(test)]

use crate::mock::*;
use crate::{
    self as pallet_eterra_card_escrow, CardEscrowStats, CardLatestEscrowStats, CardLifetimeStats,
    Error, EventIdOf, GameEnemyAssignments,
};
use frame_support::{assert_noop, assert_ok, BoundedVec};

fn card_ids(
    values: Vec<u32>,
) -> BoundedVec<u32, <Test as pallet_eterra_card_escrow::Config>::MaxEscrowedPerOwner> {
    BoundedVec::try_from(values).expect("card ids within bounds")
}

fn event_id(value: &str) -> EventIdOf<Test> {
    BoundedVec::try_from(value.as_bytes().to_vec()).expect("event id within bounds")
}

fn genome(seed: u8) -> [u8; 32] {
    [seed; 32]
}

fn stats(games_placed: u32, eliminations: u32, total_earned: u128) -> CardEscrowStats<u128> {
    CardEscrowStats {
        games_placed,
        eliminations,
        total_earned,
    }
}

#[test]
fn deposit_and_immediate_withdraw_round_trips_card() {
    ExtBuilder::build().execute_with(|| {
        seed_card(10, ALICE, genome(7));

        assert_ok!(CardEscrow::deposit_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert_eq!(card_owner(10), Some(CardEscrow::account_id()));
        assert_eq!(CardEscrow::available_escrow_count(), 1);

        assert_ok!(CardEscrow::withdraw_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert_eq!(card_owner(10), Some(ALICE));
        assert_eq!(CardEscrow::available_escrow_count(), 0);
        assert!(CardEscrow::escrow_entry(10).is_none());
    });
}

#[test]
fn active_game_withdraw_becomes_unreserved_and_requires_weighted_direct_exit() {
    ExtBuilder::build().execute_with(|| {
        seed_card(10, ALICE, genome(7));
        seed_game(44, SERVER, vec![ALICE, BOB], true);

        assert_ok!(CardEscrow::deposit_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert_ok!(CardEscrow::handle_game_created(44));
        assert_eq!(CardEscrow::available_escrow_count(), 0);

        assert_ok!(CardEscrow::withdraw_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert!(
            CardEscrow::escrow_entry(10)
                .expect("entry")
                .withdraw_requested
        );

        CardEscrow::handle_game_ended(44);
        let entry = CardEscrow::escrow_entry(10).expect("queued exit remains in escrow");
        assert_eq!(entry.owner, ALICE);
        assert_eq!(entry.reserved_by, None);
        assert!(entry.withdraw_requested);
        assert_eq!(card_owner(10), Some(CardEscrow::account_id()));
        assert!(CardEscrow::escrowed_by_owner(ALICE).contains(&10));
        assert!(GameEnemyAssignments::<Test>::get(44).is_empty());
        assert_eq!(CardEscrow::available_escrow_count(), 0);

        assert_ok!(CardEscrow::withdraw_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert_eq!(card_owner(10), Some(ALICE));
        assert!(CardEscrow::escrow_entry(10).is_none());
        assert!(!CardEscrow::escrowed_by_owner(ALICE).contains(&10));
    });
}

#[test]
fn failed_direct_exit_after_game_end_preserves_entry_and_owner_index_for_retry() {
    ExtBuilder::build().execute_with(|| {
        seed_card(10, ALICE, genome(7));
        seed_game(44, SERVER, vec![ALICE, BOB], true);
        assert_ok!(CardEscrow::deposit_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert_ok!(CardEscrow::handle_game_created(44));
        assert_ok!(CardEscrow::withdraw_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));

        CardEscrow::handle_game_ended(44);

        set_withdraw_failure(true);
        assert!(
            CardEscrow::withdraw_cards(RuntimeOrigin::signed(ALICE), card_ids(vec![10]),).is_err()
        );

        let entry = CardEscrow::escrow_entry(10).expect("failed direct exit remains recoverable");
        assert_eq!(entry.owner, ALICE);
        assert_eq!(entry.reserved_by, None);
        assert!(entry.withdraw_requested);
        assert!(CardEscrow::escrowed_by_owner(ALICE).contains(&10));
        assert_eq!(card_owner(10), Some(CardEscrow::account_id()));
        assert!(GameEnemyAssignments::<Test>::get(44).is_empty());

        set_withdraw_failure(false);
        assert_ok!(CardEscrow::withdraw_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert_eq!(card_owner(10), Some(ALICE));
        assert!(CardEscrow::escrow_entry(10).is_none());
        assert!(!CardEscrow::escrowed_by_owner(ALICE).contains(&10));
    });
}

#[test]
fn game_reserves_up_to_available_cards() {
    ExtBuilder::build().execute_with(|| {
        for (card_id, owner, seed) in [(10, ALICE, 1u8), (11, ALICE, 2u8), (12, BOB, 3u8)] {
            seed_card(card_id, owner, genome(seed));
            assert_ok!(CardEscrow::deposit_cards(
                RuntimeOrigin::signed(owner),
                card_ids(vec![card_id]),
            ));
        }

        assert_ok!(CardEscrow::handle_game_created(55));
        let assignments = CardEscrow::game_enemy_assignments(55);
        assert_eq!(assignments.len(), 3);
        assert_eq!(CardEscrow::available_escrow_count(), 0);
    });
}

#[test]
fn deposit_resets_latest_stats_only() {
    ExtBuilder::build().execute_with(|| {
        seed_card(10, ALICE, genome(7));
        CardLatestEscrowStats::<Test>::insert(10, stats(3, 2, 250));
        CardLifetimeStats::<Test>::insert(10, stats(8, 5, 900));

        assert_ok!(CardEscrow::deposit_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));

        assert_eq!(CardEscrow::card_latest_escrow_stats(10), stats(0, 0, 0));
        assert_eq!(CardEscrow::card_lifetime_stats(10), stats(8, 5, 900));
    });
}

#[test]
fn game_placement_and_owner_elimination_update_both_stat_buckets() {
    ExtBuilder::build().execute_with(|| {
        seed_card(10, ALICE, genome(7));
        seed_game(44, SERVER, vec![ALICE, BOB], true);

        assert_ok!(CardEscrow::deposit_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert_ok!(CardEscrow::handle_game_created(44));
        assert_eq!(CardEscrow::card_latest_escrow_stats(10), stats(1, 0, 0));
        assert_eq!(CardEscrow::card_lifetime_stats(10), stats(1, 0, 0));

        let alice_before = Balances::free_balance(ALICE);
        assert_ok!(CardEscrow::record_enemy_elimination_with_event_id(
            RuntimeOrigin::signed(SERVER),
            44,
            event_id("elim-1"),
            10,
            BOB,
        ));
        assert_eq!(Balances::free_balance(ALICE), alice_before + 100);
        assert_eq!(CardEscrow::card_latest_escrow_stats(10), stats(1, 1, 100));
        assert_eq!(CardEscrow::card_lifetime_stats(10), stats(1, 1, 100));

        assert_ok!(CardEscrow::record_enemy_elimination_with_event_id(
            RuntimeOrigin::signed(SERVER),
            44,
            event_id("elim-1"),
            10,
            BOB,
        ));
        assert_eq!(Balances::free_balance(ALICE), alice_before + 100);
        assert_eq!(CardEscrow::card_latest_escrow_stats(10), stats(1, 1, 100));
        assert_eq!(CardEscrow::card_lifetime_stats(10), stats(1, 1, 100));
    });
}

#[test]
fn defeat_rewards_do_not_change_owner_stat_buckets() {
    ExtBuilder::build().execute_with(|| {
        seed_card(10, ALICE, genome(7));
        seed_game(44, SERVER, vec![ALICE, BOB], true);

        assert_ok!(CardEscrow::deposit_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert_ok!(CardEscrow::handle_game_created(44));
        assert_eq!(CardEscrow::card_latest_escrow_stats(10), stats(1, 0, 0));
        assert_eq!(CardEscrow::card_lifetime_stats(10), stats(1, 0, 0));

        let bob_before = Balances::free_balance(BOB);
        assert_ok!(CardEscrow::record_enemy_defeat_with_event_id(
            RuntimeOrigin::signed(SERVER),
            44,
            event_id("defeat-1"),
            BOB,
            10,
        ));
        assert_eq!(Balances::free_balance(BOB), bob_before + 100);
        assert_eq!(CardEscrow::card_latest_escrow_stats(10), stats(1, 0, 0));
        assert_eq!(CardEscrow::card_lifetime_stats(10), stats(1, 0, 0));
        assert_ok!(CardEscrow::record_enemy_defeat_with_event_id(
            RuntimeOrigin::signed(SERVER),
            44,
            event_id("defeat-1"),
            BOB,
            10,
        ));
        assert_eq!(Balances::free_balance(BOB), bob_before + 100);
        assert_eq!(CardEscrow::card_latest_escrow_stats(10), stats(1, 0, 0));
        assert_eq!(CardEscrow::card_lifetime_stats(10), stats(1, 0, 0));

        assert_noop!(
            CardEscrow::record_enemy_elimination_with_event_id(
                RuntimeOrigin::signed(SERVER),
                44,
                event_id("elim-after-defeat"),
                10,
                BOB,
            ),
            Error::<Test>::EnemyAlreadyDefeated
        );
    });
}

#[test]
fn withdraw_preserves_latest_stats_and_redeposit_resets_only_latest() {
    ExtBuilder::build().execute_with(|| {
        seed_card(10, ALICE, genome(7));
        seed_game(44, SERVER, vec![ALICE, BOB], true);

        assert_ok!(CardEscrow::deposit_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert_ok!(CardEscrow::handle_game_created(44));
        assert_ok!(CardEscrow::record_enemy_elimination_with_event_id(
            RuntimeOrigin::signed(SERVER),
            44,
            event_id("elim-1"),
            10,
            BOB,
        ));
        assert_eq!(CardEscrow::card_latest_escrow_stats(10), stats(1, 1, 100));
        assert_eq!(CardEscrow::card_lifetime_stats(10), stats(1, 1, 100));

        CardEscrow::handle_game_ended(44);
        assert_ok!(CardEscrow::withdraw_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert_eq!(card_owner(10), Some(ALICE));
        assert_eq!(CardEscrow::card_latest_escrow_stats(10), stats(1, 1, 100));
        assert_eq!(CardEscrow::card_lifetime_stats(10), stats(1, 1, 100));

        assert_ok!(CardEscrow::deposit_cards(
            RuntimeOrigin::signed(ALICE),
            card_ids(vec![10]),
        ));
        assert_eq!(CardEscrow::card_latest_escrow_stats(10), stats(0, 0, 0));
        assert_eq!(CardEscrow::card_lifetime_stats(10), stats(1, 1, 100));
    });
}
