// pallets/eterra-simple-matchmaker/src/tests.rs
#![cfg(test)]

use super::*;

use frame_support::{assert_noop, assert_ok, traits::OnFinalize};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::DispatchError;

use crate::mock::{
    clear_all_hands, created_games, new_test_ext, set_has_hand, Matchmaker, RuntimeEvent,
    RuntimeOrigin as SystemOrigin, Test,
};

fn last_event() -> RuntimeEvent {
    frame_system::Pallet::<Test>::events()
        .pop()
        .expect("Event expected")
        .event
}

fn take_events() -> Vec<RuntimeEvent> {
    frame_system::Pallet::<Test>::events()
        .into_iter()
        .map(|r| r.event)
        .collect()
}

fn filter_matchmaker(events: &[RuntimeEvent]) -> Vec<RuntimeEvent> {
    events
        .iter()
        .cloned()
        .filter(|ev| matches!(ev, RuntimeEvent::Matchmaker(_)))
        .collect()
}

#[test]
fn join_queue_emits_event_and_persists() {
    new_test_ext().execute_with(|| {
        set_has_hand(1, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(1)));

        // Collect all events and ensure a Joined{ who: 1 } was emitted,
        // ignoring any ProcessingStarted/ProcessingCompleted noise.
        let evs = take_events();
        let joined_seen = evs.iter().any(|ev| {
            matches!(
                ev,
                RuntimeEvent::Matchmaker(Event::<Test>::Joined { who }) if *who == 1
            )
        });
        assert!(
            joined_seen,
            "expected Joined event for who=1, got: {:?}",
            evs
        );

        // Also assert the state persisted: live size should be 1.
        assert_eq!(LiveSize::<Test>::get(), 1);
    });
}

#[test]
fn join_queue_rejects_duplicates() {
    new_test_ext().execute_with(|| {
        set_has_hand(1, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(1)));
        assert_noop!(
            Matchmaker::join_queue(SystemOrigin::signed(1)),
            Error::<Test>::AlreadyQueued
        );
    });
}

#[test]
fn queue_capacity_does_not_fill_due_to_ring_holes() {
    new_test_ext().execute_with(|| {
        // With auto-processing, live size stays low. Ensure repeated joins
        // don't hit QueueFull due to ring holes.
        let cap = mock::QueueCapacityConst::get() as u64;
        for who in 1..=(cap + 5) {
            set_has_hand(who, true);
            assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(who)));
        }
    });
}

#[test]
fn leave_queue_works_and_emits() {
    new_test_ext().execute_with(|| {
        set_has_hand(1, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(1)));
        assert_ok!(Matchmaker::leave_queue(SystemOrigin::signed(1)));

        // Event last should be QueueLeft
        let ev = last_event();
        if let RuntimeEvent::Matchmaker(inner) = ev {
            let s = format!("{:?}", inner);
            assert!(
                s.contains("Left") || s.contains("Pop"),
                "unexpected matchmaker event: {:?}",
                inner
            );
        } else {
            panic!("unexpected event section: {:?}", ev);
        }
    });
}

#[test]
fn leave_queue_when_not_queued_fails() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Matchmaker::leave_queue(SystemOrigin::signed(42)),
            Error::<Test>::NotQueued
        );
    });
}

#[test]
fn join_queue_requires_current_hand() {
    new_test_ext().execute_with(|| {
        // Ensure clean slate (no one has a hand)
        clear_all_hands();

        // Without a hand -> should fail
        assert_noop!(
            Matchmaker::join_queue(SystemOrigin::signed(1)),
            Error::<Test>::NoPresetHand
        );

        // Give account 1 a hand -> should succeed
        set_has_hand(1, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(1)));
    });
}

#[test]
fn process_queue_noops_with_fewer_than_two() {
    new_test_ext().execute_with(|| {
        // 0 players
        assert_ok!(Matchmaker::process_queue(SystemOrigin::signed(99)));
        // 1 player
        set_has_hand(1, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(1)));
        assert_ok!(Matchmaker::process_queue(SystemOrigin::signed(99)));
        // No game or matched event can be produced without a pair.
        assert!(created_games().is_empty());
        let mm = filter_matchmaker(&take_events());
        assert!(mm
            .iter()
            .all(|e| !matches!(e, RuntimeEvent::Matchmaker(Event::<Test>::Matched { .. }))));
    });
}

#[test]
fn second_join_forms_pair_and_preserves_fifo() {
    new_test_ext().execute_with(|| {
        // Matching is automatic on the second join. The third player remains queued.
        set_has_hand(1, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(1)));
        set_has_hand(2, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(2)));
        set_has_hand(3, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(3)));

        let matched = take_events()
            .into_iter()
            .find_map(|ev| match ev {
                RuntimeEvent::Matchmaker(Event::<Test>::Matched { players }) => Some(players),
                _ => None,
            })
            .expect("matched event expected");
        assert_eq!(matched, [1, 2]);
        assert_eq!(created_games(), vec![(1, 2)]);
        assert_eq!(LiveSize::<Test>::get(), 1);
        assert!(InQueue::<Test>::contains_key(3));
    });
}

#[test]
fn repeated_joins_form_multiple_pairs_in_fifo_order() {
    new_test_ext().execute_with(|| {
        // Each even-numbered join completes the next FIFO pair.
        for who in 1..=6 {
            set_has_hand(who, true);
            assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(who)));
        }

        assert_eq!(created_games(), vec![(1, 2), (3, 4), (5, 6)]);
        assert_eq!(LiveSize::<Test>::get(), 0);
    });
}

#[test]
fn leaving_queued_player_preserves_fifo_for_later_joins() {
    new_test_ext().execute_with(|| {
        set_has_hand(1, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(1)));
        assert_ok!(Matchmaker::leave_queue(SystemOrigin::signed(1)));

        for who in 2..=4 {
            set_has_hand(who, true);
            assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(who)));
        }

        assert_eq!(created_games(), vec![(2, 3)]);
        assert_eq!(LiveSize::<Test>::get(), 1);
        assert!(InQueue::<Test>::contains_key(4));
    });
}

#[test]
fn rejoin_after_leave_is_allowed() {
    new_test_ext().execute_with(|| {
        set_has_hand(10, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(10)));
        assert_ok!(Matchmaker::leave_queue(SystemOrigin::signed(10)));
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(10)));
    });
}

#[test]
fn calls_require_signed_origin() {
    new_test_ext().execute_with(|| {
        // join_queue
        assert!(matches!(
            Matchmaker::join_queue(SystemOrigin::none()),
            Err(DispatchError::BadOrigin)
        ));
        // leave_queue
        assert!(matches!(
            Matchmaker::leave_queue(SystemOrigin::none()),
            Err(DispatchError::BadOrigin)
        ));
        // process_queue
        assert!(matches!(
            Matchmaker::process_queue(SystemOrigin::none()),
            Err(DispatchError::BadOrigin)
        ));
    });
}

/// Sanity: multiple finalize blocks should not affect queue invariants
#[test]
fn finalize_blocks_does_not_break_queue() {
    new_test_ext().execute_with(|| {
        // Add some players
        for who in 1..=3 {
            set_has_hand(who, true);
            assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(who)));
        }
        // Simulate two blocks
        frame_system::Pallet::<Test>::set_block_number(1);
        <Matchmaker as frame_support::traits::Hooks<BlockNumberFor<Test>>>::on_finalize(1);
        frame_system::Pallet::<Test>::set_block_number(2);
        <Matchmaker as frame_support::traits::Hooks<BlockNumberFor<Test>>>::on_finalize(2);
    });
}
