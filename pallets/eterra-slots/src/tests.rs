use crate::{mock::*, Error};
use frame_support::{assert_err, assert_ok};
use log::info;

#[test]
fn generate_slot_works() {
    new_test_ext().execute_with(|| {
        let player: u64 = 1;
        info!("Starting generate_slot_works test...");

        System::set_block_number(1);

        assert_eq!(SlotMachine::player_attempts(&player), 0);

        assert_ok!(SlotMachine::generate_slot(RuntimeOrigin::signed(player)));

        assert_eq!(SlotMachine::player_attempts(&player), 1);

        // Retrieve the generated event
        let all_events = System::events();
        info!("Total events: {}", all_events.len());

        let last_event = all_events.last();
        assert!(
            last_event.is_some(),
            "Expected an event to be emitted but found None."
        );

        if let RuntimeEvent::SlotMachine(crate::Event::SlotGenerated { player: event_player, values }) = &last_event.unwrap().event {
            assert_eq!(*event_player, player, "Player ID in event does not match expected.");
            assert_eq!(values.len(), 4, "Generated values should be exactly 4 bytes.");
        } else {
            panic!("Expected SlotGenerated event but got something else: {:?}", last_event);
        }
    });
}

#[test]
fn accept_slot_works() {
    new_test_ext().execute_with(|| {
        let player: u64 = 1;

        info!("Starting accept_slot_works test...");

        System::set_block_number(1);

        assert_ok!(SlotMachine::generate_slot(RuntimeOrigin::signed(player)));
        assert_eq!(SlotMachine::player_attempts(&player), 1);

        assert_ok!(SlotMachine::accept_slot(RuntimeOrigin::signed(player)));
        info!("Slot accepted by player {}", player);

        assert_eq!(SlotMachine::player_attempts(&player), 0);

        // Check event was emitted
        System::assert_has_event(RuntimeEvent::SlotMachine(crate::Event::SlotAccepted { player }));
    });
}

#[test]
fn deny_slot_works() {
    new_test_ext().execute_with(|| {
        let player: u64 = 1;

        info!("Starting deny_slot_works test...");

        System::set_block_number(1);

        assert_ok!(SlotMachine::generate_slot(RuntimeOrigin::signed(player)));
        assert_eq!(SlotMachine::player_attempts(&player), 1);

        assert_ok!(SlotMachine::deny_slot(RuntimeOrigin::signed(player)));
        info!("Slot denied by player {}", player);

        assert_eq!(SlotMachine::player_attempts(&player), 1);

        // Check event was emitted
        System::assert_has_event(RuntimeEvent::SlotMachine(crate::Event::SlotDenied { player }));
    });
}

#[test]
fn max_attempts_exceeded() {
    new_test_ext().execute_with(|| {
        let player: u64 = 1;

        info!("Starting max_attempts_exceeded test...");

        System::set_block_number(1);

        for _ in 0..3 {
            assert_ok!(SlotMachine::generate_slot(RuntimeOrigin::signed(player)));
        }

        // Expect failure when exceeding limit
        assert_err!(
            SlotMachine::generate_slot(RuntimeOrigin::signed(player)),
            Error::<Test>::MaxAttemptsExceeded
        );
    });
}