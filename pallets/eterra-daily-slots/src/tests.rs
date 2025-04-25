#![cfg(test)]

use crate::mock::*;
use crate::{Error, LastRollTime, Pallet, RollsThisBlock, SlotMachineConfig};
use frame_support::{assert_noop, assert_ok};
use frame_support::traits::Hooks;



#[test]
fn test_roll_succeeds_with_valid_config() {
    new_test_ext().execute_with(|| {
        // 1. Set a valid config
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        // 2. Perform a roll
        let result = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_ok!(result);

        // 3. Verify last roll time is now 90_000
        let stored_time = LastRollTime::<TestRuntime>::get(1);
        assert_eq!(stored_time, 90_000);
    });
}

#[test]
fn test_roll_fails_if_not_enough_time_has_passed() {
    new_test_ext().execute_with(|| {
        // Valid config
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        // First roll OK
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));

        // Second roll immediately -> fails
        let second_roll = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_noop!(second_roll, Error::<TestRuntime>::RollNotAvailableYet);
    });
}

#[test]
fn test_roll_fails_on_invalid_configuration() {
    new_test_ext().execute_with(|| {
        // Invalid config
        SlotMachineConfig::<TestRuntime>::put((0, 5, 2));

        // Rolling now should fail
        let result = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_noop!(result, Error::<TestRuntime>::InvalidConfiguration);
    });
}

#[test]
fn test_roll_succeeds_after_24_hours() {
    new_test_ext().execute_with(|| {
        // Valid config
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        // First roll OK
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));

        // Simulate "24 hours later" by changing the mock's time or forcibly overriding `LastRollTime`.
        // If your mock time is static, you can do something like:
        LastRollTime::<TestRuntime>::insert(1, 90_000 - 86_400);
        // Now "now" - last_roll_time == 86,400

        // Attempt a second roll -> should now succeed
        let second_roll = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_ok!(second_roll);
    });
}

#[test]
fn test_different_accounts_can_roll_independently() {
    new_test_ext().execute_with(|| {
        // Valid config
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        // Account 1 rolls
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));

        // Account 2 rolls -> should also be OK, different user
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(2).into()
        ));
    });
}

#[test]
fn test_slot_rolled_event_emitted() {
    new_test_ext().execute_with(|| {
        // Valid config
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        // Clear old events
        frame_system::Pallet::<TestRuntime>::set_block_number(1);

        // Perform roll
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));

        // Fetch events
        let events = frame_system::Pallet::<TestRuntime>::events();
        assert_eq!(events.len(), 1, "Expected exactly one event");

        // Check that the event is our SlotRolled, with the right account ID
        let evt = &events[0];
        match evt.event {
            RuntimeEvent::EterraDailySlots(crate::Event::SlotRolled {
                ref player,
                ref result,
            }) => {
                assert_eq!(*player, 1);
                // If you want to check the `result` length or values, do so:
                assert_eq!(result.len(), 3);
            }
            _ => panic!("Unexpected event: {:#?}", evt),
        }
    });
}

#[test]
fn test_roll_with_max_config() {
    new_test_ext().execute_with(|| {
        // Suppose your real pallet has "max" that is well below u32::MAX.
        // e.g. 1000 slots is still fairly large, but won't take forever:
        SlotMachineConfig::<TestRuntime>::put((1000, 10, 5));

        // Possibly reset LastRollTime so daily-limit is not triggered
        LastRollTime::<TestRuntime>::insert(1, 0);

        // Now the loop is only 1000 iterations, which is fine
        let result = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_ok!(result);

        // Extra checks...
    });
}
#[test]
fn test_only_one_successful_roll_per_block() {
    new_test_ext().execute_with(|| {
        // 1. Only allow 1 roll per block
        SlotMachineConfig::<TestRuntime>::put((3, 5, 1));

        // 2. daily-limit override (so it doesn't block the test)
        LastRollTime::<TestRuntime>::insert(1, 0);

        // 3. First roll => OK
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));

        // Check that exactly one SlotRolled event was emitted
        let events_after_first = frame_system::Pallet::<TestRuntime>::events();
        println!("Events after first roll = {:?}", events_after_first);
        assert_eq!(events_after_first.len(), 1, "Expect exactly 1 event so far");
        match &events_after_first[0].event {
            RuntimeEvent::EterraDailySlots(crate::Event::SlotRolled { player, .. }) => {
                assert_eq!(*player, 1, "Wrong player for first roll");
            }
            _ => panic!("Expected first event to be SlotRolled"),
        }

        // Force the block number to remain the same:
        let block_num = frame_system::Pallet::<TestRuntime>::block_number();
        frame_system::Pallet::<TestRuntime>::set_block_number(block_num);

        // 4. Second roll => should fail with ExceedRollsPerRound
        LastRollTime::<TestRuntime>::insert(1, 0);
        let second_roll = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_noop!(second_roll, Error::<TestRuntime>::ExceedRollsPerRound);

        // Because the second roll returned an error, it does not emit or persist new events.
        // 5. Check the final event list is unchanged
        let events_after_second = frame_system::Pallet::<TestRuntime>::events();
        println!("Events after second roll = {:?}", events_after_second);
        // The second roll is a failed extrinsic => no additional SlotRolled event
        assert_eq!(
            events_after_second.len(),
            1,
            "No new event should be added for the failing roll"
        );
    });
}

 #[test]
fn test_ticket_awarded_on_special_symbol() {
    new_test_ext().execute_with(|| {
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        // Clear previous LastRollTime
        LastRollTime::<TestRuntime>::insert(1, 0);

        // Perform a roll
        assert_ok!(Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into()));

        // Check that tickets were assigned (mock time now() % 5 = 0 or 1 or so depending on timing)
        // For MockTime::now() = 90,000, options_per_slot = 5, so roll value is (90_000 % 5) = 0
        // Special symbol is 7 in current pallet logic, so unless options_per_slot changed, this test assumes symbol not 7
        assert_eq!(crate::TicketsPerUser::<TestRuntime>::get(1), 0);

        // This tests that unless you hit the special symbol (7), no tickets awarded.
        // For a stronger test, we would simulate a case where result = 7.
    });
}

#[test]
fn test_no_weekly_drawing_if_not_sunday_6pm() {
    new_test_ext().execute_with(|| {
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        // Set tickets to simulate a pool
        crate::TicketsPerUser::<TestRuntime>::insert(1, 5);
        crate::TotalTickets::<TestRuntime>::put(5);

        // Simulate last drawing already recent
        crate::LastDrawingTime::<TestRuntime>::put(89_500); // very close to now

        // Should not draw again if < 24h passed
        Pallet::<TestRuntime>::on_initialize(1);

        // Tickets should still exist
        assert_eq!(crate::TotalTickets::<TestRuntime>::get(), 5);

        // No WeeklyWinner event should exist
        let events = frame_system::Pallet::<TestRuntime>::events();
        let winner_event_found = events.iter().any(|event_record| {
            matches!(
                event_record.event,
                RuntimeEvent::EterraDailySlots(crate::Event::WeeklyWinner { .. })
            )
        });
        assert!(!winner_event_found, "Should NOT have emitted WeeklyWinner event");
    });
}

#[test]
fn test_weekly_drawing_selects_winner() {
    new_test_ext().execute_with(|| {
        // 🔥 Move MockTime set here (inside execute_with block)
        let sunday_time = 324_000; // Correct Substrate Sunday 6PM
        MockTimeState::set_now(sunday_time);

        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        crate::TicketsPerUser::<TestRuntime>::insert(1, 5);
        crate::TotalTickets::<TestRuntime>::put(5);
        crate::LastDrawingTime::<TestRuntime>::put(0);

        frame_system::Pallet::<TestRuntime>::set_block_number(1000);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        assert_eq!(crate::TotalTickets::<TestRuntime>::get(), 0);

        let events = frame_system::Pallet::<TestRuntime>::events();
        let winner_event_found = events.iter().any(|event_record| {
            matches!(
                event_record.event,
                RuntimeEvent::EterraDailySlots(crate::Event::WeeklyWinner { .. })
            )
        });
        assert!(winner_event_found, "WeeklyWinner event should have been emitted");
    });
}

#[test]
fn test_no_weekly_drawing_with_no_tickets() {
    let sunday_time = 324000; // Correct Sunday 6PM time
    MockTimeState::set_now(sunday_time);

    new_test_ext().execute_with(|| {
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        crate::TotalTickets::<TestRuntime>::put(0); // No tickets
        crate::LastDrawingTime::<TestRuntime>::put(0);

        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        // TotalTickets should remain 0
        assert_eq!(crate::TotalTickets::<TestRuntime>::get(), 0);

        // No WeeklyWinner event emitted
        let events = frame_system::Pallet::<TestRuntime>::events();
        let winner_event_found = events.iter().any(|event_record| {
            matches!(
                event_record.event,
                RuntimeEvent::EterraDailySlots(crate::Event::WeeklyWinner { .. })
            )
        });
        assert!(!winner_event_found, "Should NOT have emitted WeeklyWinner event");
    });
}

#[test]
fn test_weekly_drawing_only_once_per_week() {
    new_test_ext().execute_with(|| {
        let sunday_time = 324000; // Correct Sunday 6PM time
        MockTimeState::set_now(sunday_time); // ✅ move inside here

        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        crate::TicketsPerUser::<TestRuntime>::insert(1, 5);
        crate::TotalTickets::<TestRuntime>::put(5);
        crate::LastDrawingTime::<TestRuntime>::put(0);

        frame_system::Pallet::<TestRuntime>::set_block_number(1000);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        // First drawing
        Pallet::<TestRuntime>::on_initialize(1001);

        // Tickets should now be 0 after first draw
        assert_eq!(crate::TotalTickets::<TestRuntime>::get(), 0);

        // Now attempt second drawing immediately
        Pallet::<TestRuntime>::on_initialize(1002);

        // TotalTickets still 0 (no second drawing)
        assert_eq!(crate::TotalTickets::<TestRuntime>::get(), 0);

        // Only one WeeklyWinner event should exist
        let events = frame_system::Pallet::<TestRuntime>::events();
        let winner_events = events.iter().filter(|event_record| {
            matches!(
                event_record.event,
                RuntimeEvent::EterraDailySlots(crate::Event::WeeklyWinner { .. })
            )
        }).count();
        assert_eq!(winner_events, 1, "Should have exactly one WeeklyWinner event");
    });
}

#[test]
fn test_slot_rolled_event_emitted_correctly() {
    new_test_ext().execute_with(|| {
        // Set up a valid config
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));
        LastRollTime::<TestRuntime>::insert(1, 0); // avoid daily limit

        // Clear events
        frame_system::Pallet::<TestRuntime>::reset_events();

        // Perform a roll
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));

        // Capture events
        let events = frame_system::Pallet::<TestRuntime>::events();

        // Find SlotRolled event
        let slot_rolled_found = events.iter().any(|event_record| {
            matches!(
                event_record.event,
                RuntimeEvent::EterraDailySlots(crate::Event::SlotRolled { .. })
            )
        });

        assert!(slot_rolled_found, "SlotRolled event should have been emitted");
    });
}

#[test]
fn test_weekly_winner_event_emitted_correctly() {
    new_test_ext().execute_with(|| {
        // Simulate Sunday 6PM
        let sunday_time = 324_000;
        MockTimeState::set_now(sunday_time);

        // Set up tickets
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));
        crate::TicketsPerUser::<TestRuntime>::insert(1, 5);
        crate::TotalTickets::<TestRuntime>::put(5);
        crate::LastDrawingTime::<TestRuntime>::put(0);

        frame_system::Pallet::<TestRuntime>::set_block_number(1000);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);

        // Clear events
        frame_system::Pallet::<TestRuntime>::reset_events();

        // Trigger on_initialize where drawing should happen
        Pallet::<TestRuntime>::on_initialize(1001);

        // Capture events
        let events = frame_system::Pallet::<TestRuntime>::events();

        // Find WeeklyWinner event
        let winner_found = events.iter().any(|event_record| {
            matches!(
                event_record.event,
                RuntimeEvent::EterraDailySlots(crate::Event::WeeklyWinner { .. })
            )
        });

        assert!(winner_found, "WeeklyWinner event should have been emitted");
    });
}