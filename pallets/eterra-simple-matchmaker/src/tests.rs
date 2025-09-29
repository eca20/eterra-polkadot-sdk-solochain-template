// pallets/eterra-simple-matchmaker/src/tests.rs
#![cfg(test)]

use super::*;

use frame_support::{assert_noop, assert_ok, traits::OnFinalize};
use sp_runtime::DispatchError;
use frame_system::pallet_prelude::BlockNumberFor;

use crate::mock::{
    new_test_ext, RuntimeEvent, RuntimeOrigin as SystemOrigin, Test, Matchmaker, set_has_hand, clear_all_hands
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
        // Event
        let ev = last_event();
        if let RuntimeEvent::Matchmaker(inner) = ev {
            let s = format!("{:?}", inner);
            assert!(s.contains("Queue") || s.contains("Join") || s.contains("Queued"), "unexpected matchmaker event: {:?}", inner);
        } else {
            panic!("unexpected event section: {:?}", ev);
        }
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
fn queue_capacity_enforced() {
    new_test_ext().execute_with(|| {
        // QueueCapacityConst is defined in mock.rs; fill it completely.
        for who in 1..=mock::QueueCapacityConst::get() as u64 {
            set_has_hand(who, true);
            assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(who)));
        }
        // One more should fail (ensure the overflow player also has a preset hand so we hit QueueFull, not NoPresetHand)
        let overflow = mock::QueueCapacityConst::get() as u64 + 1;
        set_has_hand(overflow, true);
        assert_noop!(
            Matchmaker::join_queue(SystemOrigin::signed(overflow)),
            Error::<Test>::QueueFull
        );
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
            assert!(s.contains("Left") || s.contains("Pop"), "unexpected matchmaker event: {:?}", inner);
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

#[cfg(any(feature = "dev_tests_with_try_match"))]
#[test]
fn try_match_noop_with_fewer_than_two() {
    new_test_ext().execute_with(|| {
        // 0 players
        assert_ok!(Matchmaker::try_match(SystemOrigin::signed(99)));
        // 1 player
        set_has_hand(1, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(1)));
        assert_ok!(Matchmaker::try_match(SystemOrigin::signed(99)));
        // No MatchFormed or Matched events
        let mm = filter_matchmaker(&take_events());
        assert!(mm.iter().all(|e| !matches!(
            e,
            RuntimeEvent::Matchmaker(Event::<Test>::MatchFormed { .. })
                | RuntimeEvent::Matchmaker(Event::<Test>::Matched { .. })
        )));
    });
}

#[cfg(any(feature = "dev_tests_with_try_match"))]
#[test]
fn try_match_forms_two_and_removes_from_head_fifo() {
    new_test_ext().execute_with(|| {
        // Join three to check FIFO (1,2 should be matched; 3 remains)
        set_has_hand(1, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(1)));
        set_has_hand(2, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(2)));
        set_has_hand(3, true);
        assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(3)));

        assert_ok!(Matchmaker::try_match(SystemOrigin::signed(99)));

        // Expect MatchFormed or Matched with [1,2]
        let evs = take_events();
        let formed = evs.into_iter().find_map(|ev| {
            match ev {
                RuntimeEvent::Matchmaker(Event::<Test>::MatchFormed { players })
                | RuntimeEvent::Matchmaker(Event::<Test>::Matched { players }) => Some(players),
                _ => None,
            }
        }).expect("match event expected");
        assert_eq!(formed, vec![1,2]);
    });
}

#[cfg(any(feature = "dev_tests_with_try_match"))]
#[test]
fn multiple_try_match_calls_form_multiple_pairs_in_fifo_order() {
    new_test_ext().execute_with(|| {
        // 1..=6 -> expect pairs (1,2), (3,4); then 5,6 remain until next call.
        for who in 1..=6 {
            set_has_hand(who, true);
            assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(who)));
        }

        assert_ok!(Matchmaker::try_match(SystemOrigin::signed(7)));
        // First formed: 1,2
        let first_pair = filter_matchmaker(&take_events()).into_iter().find_map(|ev| {
            match ev {
                RuntimeEvent::Matchmaker(Event::<Test>::MatchFormed { players })
                | RuntimeEvent::Matchmaker(Event::<Test>::Matched { players }) => Some(players),
                _ => None,
            }
        }).expect("first MatchFormed");
        assert_eq!(first_pair, vec![1,2]);

        assert_ok!(Matchmaker::try_match(SystemOrigin::signed(7)));
        let second_pair = filter_matchmaker(&take_events()).into_iter().find_map(|ev| {
            match ev {
                RuntimeEvent::Matchmaker(Event::<Test>::MatchFormed { players })
                | RuntimeEvent::Matchmaker(Event::<Test>::Matched { players }) => Some(players),
                _ => None,
            }
        }).expect("second MatchFormed");
        assert_eq!(second_pair, vec![3,4]);
    });
}

#[cfg(any(feature = "dev_tests_with_try_match"))]
#[test]
fn leaving_middle_preserves_order() {
    new_test_ext().execute_with(|| {
        // queue: [1,2,3,4]
        for who in 1..=4 {
            set_has_hand(who, true);
            assert_ok!(Matchmaker::join_queue(SystemOrigin::signed(who)));
        }
        // 2 leaves -> [1,3,4]
        assert_ok!(Matchmaker::leave_queue(SystemOrigin::signed(2)));
        // match -> should pair (1,3), leaving [4]
        assert_ok!(Matchmaker::try_match(SystemOrigin::signed(99)));

        let formed = filter_matchmaker(&take_events()).into_iter().find_map(|ev| {
            match ev {
                RuntimeEvent::Matchmaker(Event::<Test>::MatchFormed { players })
                | RuntimeEvent::Matchmaker(Event::<Test>::Matched { players }) => Some(players),
                _ => None,
            }
        }).expect("MatchFormed");
        assert_eq!(formed, vec![1,3]);
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
        // try_match
        #[cfg(feature = "dev_tests_with_try_match")]
        {
            assert!(matches!(
                Matchmaker::try_match(SystemOrigin::none()),
                Err(DispatchError::BadOrigin)
            ));
        }
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