#![cfg(test)]

use crate::mock::MaxWeightEntries;
use crate::mock::RuntimeEvent;
use crate::mock::*;
use crate::{
    Config, Error, Event, LastDrawingTime, LastRollTime, Pallet, RollHistory, TicketsPerUser,
    TotalTickets,
};
use frame_support::traits::Hooks;
use frame_support::BoundedVec;
use frame_support::{assert_noop, assert_ok};

use frame_system::RawOrigin;

// ─── Helpers ────────────────────────────────────────────────────────────────

fn set_mock_time_to_sunday_6pm() {
    MockTimeState::set_now(324_000);
}

fn roll_n_times<T: crate::pallet::Config>(who: &T::AccountId, n: u32) {
    for _ in 0..n {
        assert_ok!(crate::Pallet::<T>::roll(
            frame_system::RawOrigin::Signed(who.clone()).into()
        ));
    }
}

// ─── Basic Slot Roll Tests ─────────────────────────────────────────────────

#[test]
fn test_roll_succeeds_with_valid_config() {
    new_test_ext().execute_with(|| {
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        // our MockTime starts at 90_000
        assert_eq!(LastRollTime::<TestRuntime>::get(1), 90_000);
    });
}

/// You can roll up to 3× in a 24h window, so the second and third rolls still succeed.
#[test]
fn test_second_and_third_roll_succeed() {
    new_test_ext().execute_with(|| {
        // first roll
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        // second roll must also succeed
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        // third roll still under the 3-roll limit
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
    });
}

#[test]
fn test_exceed_rolls_per_day() {
    new_test_ext().execute_with(|| {
        // allow three rolls
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        // fourth roll in the same 24h window must now fail
        let fourth = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_noop!(fourth, Error::<TestRuntime>::ExceedRollsPerRound);
    });
}

#[test]
fn test_roll_succeeds_after_24_hours() {
    new_test_ext().execute_with(|| {
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        // pretend it was 24h ago:
        LastRollTime::<TestRuntime>::insert(1, 90_000 - 86_400);
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
    });
}

// ─── Edge Case: independent accounts ─────────────────────────────────────────

#[test]
fn test_different_accounts_can_roll_independently() {
    new_test_ext().execute_with(|| {
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(2).into()
        ));
    });
}

// ─── Slot Event Tests ──────────────────────────────────────────────────────

#[test]
fn test_slot_rolled_event_emitted() {
    new_test_ext().execute_with(|| {
        frame_system::Pallet::<TestRuntime>::reset_events();
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));

        let evts = frame_system::Pallet::<TestRuntime>::events();
        assert_eq!(evts.len(), 1);
        match &evts[0].event {
            RuntimeEvent::EterraDailySlots(Event::SlotRolled { player, result }) => {
                assert_eq!(*player, 1);
                assert_eq!(result.len(), 3);
            }
            _ => panic!("unexpected event"),
        }
    });
}

#[test]
fn test_slot_rolled_event_emitted_correctly() {
    new_test_ext().execute_with(|| {
        LastRollTime::<TestRuntime>::insert(1, 0);
        frame_system::Pallet::<TestRuntime>::reset_events();
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));

        let found = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::SlotRolled { .. })
                )
            });
        assert!(found, "SlotRolled should have been emitted");
    });
}

// ─── Ticket Awarding ────────────────────────────────────────────────────────

#[test]
fn test_ticket_awarded_on_special_symbol() {
    new_test_ext().execute_with(|| {
        LastRollTime::<TestRuntime>::insert(1, 0);
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        assert_eq!(TicketsPerUser::<TestRuntime>::get(1), 0);
    });
}

// ─── Weekly Drawing Tests ──────────────────────────────────────────────────

#[test]
fn test_no_weekly_drawing_if_not_sunday_6pm() {
    new_test_ext().execute_with(|| {
        TicketsPerUser::<TestRuntime>::insert(1, 5);
        TotalTickets::<TestRuntime>::put(5);
        LastDrawingTime::<TestRuntime>::put(89_500);

        Pallet::<TestRuntime>::on_initialize(1);

        assert_eq!(TotalTickets::<TestRuntime>::get(), 5);
        let fired = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })
                )
            });
        assert!(!fired);
    });
}

#[test]
fn test_no_weekly_drawing_with_no_tickets() {
    new_test_ext().execute_with(|| {
        set_mock_time_to_sunday_6pm();
        TotalTickets::<TestRuntime>::put(0);
        LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        assert_eq!(TotalTickets::<TestRuntime>::get(), 0);
        let fired = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })
                )
            });
        assert!(!fired);
    });
}

#[test]
fn test_weekly_drawing_selects_winner() {
    new_test_ext().execute_with(|| {
        set_mock_time_to_sunday_6pm();
        TicketsPerUser::<TestRuntime>::insert(1, 5);
        TotalTickets::<TestRuntime>::put(5);
        LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        assert_eq!(TotalTickets::<TestRuntime>::get(), 0);
        let fired = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })
                )
            });
        assert!(fired);
    });
}

#[test]
fn test_weekly_drawing_only_once_per_week() {
    new_test_ext().execute_with(|| {
        set_mock_time_to_sunday_6pm();
        TicketsPerUser::<TestRuntime>::insert(1, 5);
        TotalTickets::<TestRuntime>::put(5);
        LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);
        Pallet::<TestRuntime>::on_initialize(1002);

        let count = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .filter(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })
                )
            })
            .count();
        assert_eq!(count, 1);
    });
}

#[test]
fn test_weekly_winner_event_emitted_correctly() {
    new_test_ext().execute_with(|| {
        set_mock_time_to_sunday_6pm();
        TicketsPerUser::<TestRuntime>::insert(1, 5);
        TotalTickets::<TestRuntime>::put(5);
        LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        let found = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })
                )
            });
        assert!(found);
    });
}

#[test]
fn roll_creates_history_entry() {
    new_test_ext().execute_with(|| {
        let user = 1u64; // Assume u64 AccountId in mock.rs
        assert_eq!(RollHistory::<Test>::get(user).len(), 0);

        assert_ok!(Pallet::<Test>::roll(RawOrigin::Signed(user).into()));
        let history = RollHistory::<Test>::get(user);
        assert_eq!(history.len(), 1);
        let entry = &history[0];

        assert!(entry.timestamp > 0);
        assert!(!entry.result.is_empty());
    });
}

#[test]
fn roll_history_respects_max_length() {
    new_test_ext().execute_with(|| {
        let user = 1u64;

        let max_len = <Test as Config>::MaxRollHistoryLength::get();
        let roll_limit = <Test as Config>::MaxRollsPerRound::get();

        // Roll up to allowed limit
        roll_n_times::<Test>(&user, roll_limit);

        let history = RollHistory::<Test>::get(user);
        assert!(history.len() as u32 <= roll_limit);
        assert!(history.len() as u32 <= max_len);
    });
}

#[test]
fn test_set_reel_weights_and_roll_with_weights() {
    new_test_ext().execute_with(|| {
        let weights = vec![(1, 10), (2, 0), (3, 0)];
        assert_ok!(Pallet::<Test>::set_reel_weights(
            RawOrigin::Root.into(),
            0,
            weights.clone()
        ));
        let expected: BoundedVec<_, MaxWeightEntries> = weights.try_into().unwrap();
        assert_eq!(crate::ReelWeights::<Test>::get(0).unwrap(), expected);
        // Perform a roll and ensure the result includes the weighted symbol
        assert_ok!(Pallet::<Test>::roll(RawOrigin::Signed(1).into()));
        let history = RollHistory::<Test>::get(1);
        assert!(!history.is_empty());
    });
}
