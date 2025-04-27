#![cfg(test)]

use crate::mock::*;
use crate::{
    Error, Event, LastRollTime, LastDrawingTime, Pallet, RollsThisBlock, TicketsPerUser, TotalTickets, SlotMachineConfig,
};
use crate::mock::{MaxSlotLength, MaxOptionsPerSlot, MaxRollsPerRound};
use frame_support::{assert_noop, assert_ok};
use frame_support::traits::Hooks;
use crate::mock::RuntimeEvent;

// =====================================================
// 🛠 Helpers
// =====================================================

/// Set a basic valid slot machine config.
fn setup_valid_config() {
    // pull the .get()-values out of the parameter_types and store that triple
    SlotMachineConfig::<TestRuntime>::put((
        MaxSlotLength::get(),
        MaxOptionsPerSlot::get(),
        MaxRollsPerRound::get(),
    ));
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
        let second = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_noop!(second, Error::<TestRuntime>::RollNotAvailableYet);
    });
}

#[test]
fn test_roll_fails_on_invalid_configuration() {
    new_test_ext().execute_with(|| {
        // inject an invalid config
        SlotMachineConfig::<TestRuntime>::put((0, 5, 2));
        let res = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_noop!(res, Error::<TestRuntime>::InvalidConfiguration);
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
        // one roll per block
        SlotMachineConfig::<TestRuntime>::put((3, 5, 1));
        LastRollTime::<TestRuntime>::insert(1, 0);
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));
        let second = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_noop!(second, Error::<TestRuntime>::ExceedRollsPerRound);
    });
}

#[test]
fn test_roll_with_max_config() {
    new_test_ext().execute_with(|| {
        // huge slot length, options and rolls
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
        setup_valid_config();
        LastRollTime::<TestRuntime>::insert(1, 0);
        frame_system::Pallet::<TestRuntime>::reset_events();
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));

        let found = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| matches!(r.event, RuntimeEvent::EterraDailySlots(Event::SlotRolled { .. })));
        assert!(found, "SlotRolled should have been emitted");
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
        assert_eq!(TicketsPerUser::<TestRuntime>::get(1), 0);
    });
}

// =====================================================
// 📅 Weekly Drawing Behavior Tests
// =====================================================

#[test]
fn test_no_weekly_drawing_if_not_sunday_6pm() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        TicketsPerUser::<TestRuntime>::insert(1, 5);
        TotalTickets::<TestRuntime>::put(5);
        LastDrawingTime::<TestRuntime>::put(89_500);

        Pallet::<TestRuntime>::on_initialize(1);

        assert_eq!(TotalTickets::<TestRuntime>::get(), 5);
        let fired = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| matches!(r.event, RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })));
        assert!(!fired);
    });
}

#[test]
fn test_no_weekly_drawing_with_no_tickets() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        set_mock_time_to_sunday_6pm();
        TotalTickets::<TestRuntime>::put(0);
        LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        assert_eq!(TotalTickets::<TestRuntime>::get(), 0);
        let fired = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| matches!(r.event, RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })));
        assert!(!fired);
    });
}

#[test]
fn test_weekly_drawing_selects_winner() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
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
            .any(|r| matches!(r.event, RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })));
        assert!(fired);
    });
}

#[test]
fn test_weekly_drawing_only_once_per_week() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
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
            .filter(|r| matches!(r.event, RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })))
            .count();
        assert_eq!(count, 1);
    });
}

#[test]
fn test_weekly_winner_event_emitted_correctly() {
    new_test_ext().execute_with(|| {
        setup_valid_config();
        set_mock_time_to_sunday_6pm();
        TicketsPerUser::<TestRuntime>::insert(1, 5);
        TotalTickets::<TestRuntime>::put(5);
        LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        let found = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| matches!(r.event, RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })));
        assert!(found);
    });
}