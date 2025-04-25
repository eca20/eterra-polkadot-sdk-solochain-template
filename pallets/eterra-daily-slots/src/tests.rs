//! Tests for pallet-eterra-daily-slots

#![cfg(test)]

use crate::mock::*;
use crate::{Error, LastRollTime, Pallet, RollsThisBlock, SlotMachineConfig};
use frame_support::{assert_noop, assert_ok};
use frame_support::traits::Hooks;

// =====================================================
// 🛠 Helpers
// =====================================================

/// Set a basic valid slot machine config.
fn setup_valid_config() {
    SlotMachineConfig::<TestRuntime>::put((3, 5, 2));
}

/// Set MockTime to Sunday 6PM (correct drawing time).
fn set_mock_time_to_sunday_6pm() {
    MockTimeState::set_now(324_000);
}

// =====================================================
// 🎰 Basic Slot Roll Tests
// =====================================================

#[test]
fn test_roll_succeeds_with_valid_config() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));
        assert_eq!(LastRollTime::<TestRuntime>::get(1), 90_000);
    });
}

#[test]
fn test_roll_fails_if_not_enough_time_has_passed() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));
        let second_roll = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_noop!(second_roll, Error::<TestRuntime>::RollNotAvailableYet);
    });
}

#[test]
fn test_roll_fails_on_invalid_configuration() {
    new_test_ext().execute_with(|| {
        SlotMachineConfig::<TestRuntime>::put((0, 5, 2));
        let result = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_noop!(result, Error::<TestRuntime>::InvalidConfiguration);
    });
}

#[test]
fn test_roll_succeeds_after_24_hours() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));
        LastRollTime::<TestRuntime>::insert(1, 90_000 - 86_400);
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));
    });
}

// =====================================================
// 👥 Edge Case Roll Tests
// =====================================================

#[test]
fn test_different_accounts_can_roll_independently() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(2).into()));
    });
}

#[test]
fn test_only_one_successful_roll_per_block() {
    new_test_ext().execute_with(|| {
        SlotMachineConfig::<TestRuntime>::put((3, 5, 1));
        LastRollTime::<TestRuntime>::insert(1, 0);
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));
        let second_roll = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_noop!(second_roll, Error::<TestRuntime>::ExceedRollsPerRound);
    });
}

#[test]
fn test_roll_with_max_config() {
    new_test_ext().execute_with(|| {
        SlotMachineConfig::<TestRuntime>::put((1000, 10, 5));
        LastRollTime::<TestRuntime>::insert(1, 0);
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));
    });
}

// =====================================================
// 🔔 Slot Event Tests
// =====================================================

#[test]
fn test_slot_rolled_event_emitted() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        frame_system::Pallet::<TestRuntime>::reset_events();
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));

        let events = frame_system::Pallet::<TestRuntime>::events();
        assert_eq!(events.len(), 1);

        match &events[0].event {
            RuntimeEvent::EterraDailySlots(crate::Event::SlotRolled { player, result }) => {
                assert_eq!(*player, 1);
                assert_eq!(result.len(), 3);
            }
            _ => panic!("Unexpected event type"),
        }
    });
}

#[test]
fn test_slot_rolled_event_emitted_correctly() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        LastRollTime::<TestRuntime>::insert(1, 0);
        frame_system::Pallet::<TestRuntime>::reset_events();
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));

        let slot_rolled_found = frame_system::Pallet::<TestRuntime>::events().iter().any(|event_record| {
            matches!(event_record.event, RuntimeEvent::EterraDailySlots(crate::Event::SlotRolled { .. }))
        });
        assert!(slot_rolled_found, "SlotRolled event should have been emitted");
    });
}

// =====================================================
// 🎟️ Ticket Awarding Tests
// =====================================================

#[test]
fn test_ticket_awarded_on_special_symbol() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        LastRollTime::<TestRuntime>::insert(1, 0);
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));
        assert_eq!(crate::TicketsPerUser::<TestRuntime>::get(1), 0);
    });
}

// =====================================================
// 📅 Weekly Drawing Behavior Tests
// =====================================================

#[test]
fn test_no_weekly_drawing_if_not_sunday_6pm() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        crate::TicketsPerUser::<TestRuntime>::insert(1, 5);
        crate::TotalTickets::<TestRuntime>::put(5);
        crate::LastDrawingTime::<TestRuntime>::put(89_500);

        Pallet::<TestRuntime>::on_initialize(1);

        assert_eq!(crate::TotalTickets::<TestRuntime>::get(), 5);
        let winner_event_found = frame_system::Pallet::<TestRuntime>::events().iter().any(|event_record| {
            matches!(event_record.event, RuntimeEvent::EterraDailySlots(crate::Event::WeeklyWinner { .. }))
        });
        assert!(!winner_event_found, "Should NOT have emitted WeeklyWinner event");
    });
}

#[test]
fn test_no_weekly_drawing_with_no_tickets() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        set_mock_time_to_sunday_6pm();
        crate::TotalTickets::<TestRuntime>::put(0);
        crate::LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        assert_eq!(crate::TotalTickets::<TestRuntime>::get(), 0);
        let winner_event_found = frame_system::Pallet::<TestRuntime>::events().iter().any(|event_record| {
            matches!(event_record.event, RuntimeEvent::EterraDailySlots(crate::Event::WeeklyWinner { .. }))
        });
        assert!(!winner_event_found, "Should NOT have emitted WeeklyWinner event");
    });
}

#[test]
fn test_weekly_drawing_selects_winner() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        set_mock_time_to_sunday_6pm();
        crate::TicketsPerUser::<TestRuntime>::insert(1, 5);
        crate::TotalTickets::<TestRuntime>::put(5);
        crate::LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        assert_eq!(crate::TotalTickets::<TestRuntime>::get(), 0);
        let winner_event_found = frame_system::Pallet::<TestRuntime>::events().iter().any(|event_record| {
            matches!(event_record.event, RuntimeEvent::EterraDailySlots(crate::Event::WeeklyWinner { .. }))
        });
        assert!(winner_event_found, "WeeklyWinner event should have been emitted");
    });
}

#[test]
fn test_weekly_drawing_only_once_per_week() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        set_mock_time_to_sunday_6pm();
        crate::TicketsPerUser::<TestRuntime>::insert(1, 5);
        crate::TotalTickets::<TestRuntime>::put(5);
        crate::LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);
        Pallet::<TestRuntime>::on_initialize(1002);

        let winner_events = frame_system::Pallet::<TestRuntime>::events().iter().filter(|event_record| {
            matches!(event_record.event, RuntimeEvent::EterraDailySlots(crate::Event::WeeklyWinner { .. }))
        }).count();
        assert_eq!(winner_events, 1, "Should have exactly one WeeklyWinner event");
    });
}

// =====================================================
// 🏆 Weekly Winner Event Tests
// =====================================================

#[test]
fn test_weekly_winner_event_emitted_correctly() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        set_mock_time_to_sunday_6pm();
        crate::TicketsPerUser::<TestRuntime>::insert(1, 5);
        crate::TotalTickets::<TestRuntime>::put(5);
        crate::LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        let winner_found = frame_system::Pallet::<TestRuntime>::events().iter().any(|event_record| {
            matches!(event_record.event, RuntimeEvent::EterraDailySlots(crate::Event::WeeklyWinner { .. }))
        });
        assert!(winner_found, "WeeklyWinner event should have been emitted");
    });
}
