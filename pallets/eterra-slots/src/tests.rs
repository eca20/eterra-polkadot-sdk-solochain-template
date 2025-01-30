use crate::pallet::Config as EterraSlotsConfig;
use crate::{mock::*, Error, Event, PlayerPacks, ActiveCard};
use frame_support::{assert_noop, assert_ok};
use frame_support::traits::Get;
use log::{debug, Level, Metadata, Record};
use sp_runtime::traits::SaturatedConversion;
use std::sync::Once;

static INIT: Once = Once::new();

pub struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!(
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER: SimpleLogger = SimpleLogger;

pub fn init_logger() {
    INIT.call_once(|| {
        log::set_logger(&LOGGER).unwrap();
        log::set_max_level(log::LevelFilter::Debug);
    });
}

fn assert_event_found<F>(matcher: F, event_name: &str)
where
    F: Fn(&RuntimeEvent) -> bool,
{
    let events = frame_system::Pallet::<Test>::events();
    let found = events.iter().any(|record| matcher(&record.event));

    assert!(
        found,
        "Expected {} event but did not find it. Events seen: {:?}",
        event_name, events
    );
}

/// Advances the block number to `n` to ensure event processing occurs.
fn run_to_block(n: u64) {
    while frame_system::Pallet::<Test>::block_number() < n {
        frame_system::Pallet::<Test>::set_block_number(
            frame_system::Pallet::<Test>::block_number() + 1,
        );
        frame_system::Pallet::<Test>::finalize();
        frame_system::Pallet::<Test>::initialize(
            &frame_system::Pallet::<Test>::block_number(),
            &Default::default(),
            &Default::default(),
        );
    }
}

#[test]
fn test_mint_pack_simple_storage_check() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Clear any old data
        PlayerPacks::<Test>::remove(&player);
        ActiveCard::<Test>::remove(&player);
        System::reset_events();
        System::set_block_number(42); // or any number you prefer

        // Mint the pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // Verify the minted pack is in storage
        let packs = EterraSlots::player_packs(player);
        assert_eq!(packs.len(), 1, "Should have exactly 1 pack minted");

        // The newly minted pack should have ID = 42 (the current block)
        let minted_pack = &packs[0];
        assert_eq!(minted_pack.get_id(), 42);
    });
}

#[test]
fn test_mint_pack_check_event_directly() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Ensure a known block number
        System::set_block_number(100);
        System::reset_events();

        // Dispatch extrinsic
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // Check that PackMinted event with pack_id=100 was indeed emitted
        System::assert_has_event(
            RuntimeEvent::EterraSlots(Event::PackMinted {
                player,
                pack_id: 100,
            })
            .into(),
        );
    });
}

#[test]
fn test_mint_pack_inspect_events() {
    new_test_ext().execute_with(|| {
        let player = 1;
        System::set_block_number(7);
        System::reset_events();

        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        let all_events = System::events();
        assert!(!all_events.is_empty(), "No events were recorded!");

        let minted_event_found = all_events.iter().any(|r| match &r.event {
            RuntimeEvent::EterraSlots(Event::PackMinted {
                player: who,
                pack_id,
            }) => *who == player && *pack_id == 7,
            _ => false,
        });
        assert!(
            minted_event_found,
            "Expected PackMinted for player={}, pack_id=7, but not found.",
            player
        );
    });
}

#[test]
fn test_mint_pack_storage_and_events() {
    new_test_ext().execute_with(|| {
        let player = 1;
        System::set_block_number(8);
        System::reset_events();

        // 1) Mint the pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // 2) Check storage updated
        let packs = EterraSlots::player_packs(player);
        assert_eq!(packs.len(), 1, "Should have 1 pack minted now.");
        let minted_pack = &packs[0];
        assert_eq!(minted_pack.get_id(), 8);

        // 3) Check event with direct assertion
        System::assert_has_event(
            RuntimeEvent::EterraSlots(Event::PackMinted {
                player,
                pack_id: 8,
            })
            .into(),
        );
    });
}

#[test]
fn test_generate_slot_success() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Ensuring fresh state for player {}", player);
        PlayerPacks::<Test>::remove(&player);
        ActiveCard::<Test>::remove(&player);
        System::reset_events();
        assert!(
            EterraSlots::player_packs(player).is_empty(),
            "Player should start with no packs"
        );

        debug!("Minting a pack for player {} before generating a slot.", player);
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        debug!("Running to next block...");
        run_to_block(frame_system::Pallet::<Test>::block_number() + 1);

        // Check active card
        let active_card = ActiveCard::<Test>::get(player);
        assert!(active_card.is_some(), "Expected an active card but found None");

        debug!("Generate slot for the active card");
        System::reset_events();
        assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));

        run_to_block(frame_system::Pallet::<Test>::block_number() + 1);

        // We only have `SlotGenerated { card_id, values }` now
        // So let's confirm that event by checking it has the correct type:
        assert_event_found(
            |e| matches!(e, RuntimeEvent::EterraSlots(Event::SlotGenerated { card_id, values }) 
                if *card_id >= 0 && values.len() == 4),
            "SlotGenerated"
        );
    });
}

#[test]
fn test_accept_slot_success() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Minting a pack and generating a slot for player {}", player);
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));
        run_to_block(System::block_number() + 1);

        // Generate a slot
        assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));
        run_to_block(System::block_number() + 1);

        debug!("Accepting slot...");
        System::reset_events();
        assert_ok!(EterraSlots::accept_slot(RuntimeOrigin::signed(player)));
        run_to_block(System::block_number() + 1);

        // The event is now `SlotAccepted { card_id }`, no player field
        assert_event_found(
            |e| matches!(e, RuntimeEvent::EterraSlots(Event::SlotAccepted { card_id })
                if *card_id >= 0),
            "SlotAccepted"
        );
    });
}

#[test]
fn test_mint_pack_fail_when_max_packs_reached() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;
        debug!("Minting maximum allowed packs for player {}", player);

        for _ in 0..10 {
            assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));
            run_to_block(System::block_number() + 1);
        }

        debug!(
            "Attempting to mint an 11th pack for player {} (should fail).",
            player
        );
        assert_noop!(
            EterraSlots::mint_pack(RuntimeOrigin::signed(player)),
            Error::<Test>::MaxPacksReached
        );

        debug!("Correctly failed for exceeding max packs.");
    });
}

#[test]
fn test_generate_slot_fail_when_no_active_card() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Attempt to generate slot with no pack at all");
        assert_noop!(
            EterraSlots::generate_slot(RuntimeOrigin::signed(player)),
            Error::<Test>::NoPackFound
        );
    });
}

#[test]
fn test_accept_slot_fail_when_slot_not_rolled() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Minting pack but not generating a slot yet");
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        debug!("Try to accept slot before rolling one");
        let result = EterraSlots::accept_slot(RuntimeOrigin::signed(player));
        assert!(
            result == Err(Error::<Test>::NoActiveCard.into()),
            "Expected NoActiveCard but got {:?}",
            result
        );
    });
}

#[test]
fn test_attempts_removed_after_generating_max_times() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Mint a pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // We want to see which card_id was created.
        let packs = EterraSlots::player_packs(player);
        let last_pack = packs.last().expect("Pack should exist");
        let card_id = last_pack
            .get_card_ids()
            .first()
            .copied()
            .expect("Should have at least one card ID in the pack");

        // Check the MaxAttempts
        let max_attempts: u8 = <Test as EterraSlotsConfig>::MaxAttempts::get();

        // Generate slots until we hit max
        for _ in 0..max_attempts {
            assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));
        }

        // After final generation, that card should be finalized => attempts removed
        let attempts_after = EterraSlots::card_attempts(card_id);
        assert_eq!(
            attempts_after,
            0,
            "Expected attempts to be removed after finalization."
        );
    });
}

#[test]
fn test_attempts_removed_after_accept_slot() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Mint a pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // Grab the first card_id
        let packs = EterraSlots::player_packs(player);
        let last_pack = packs.last().unwrap();
        let card_id = *last_pack.get_card_ids().first().unwrap();

        // Generate one slot
        assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));

        // Should now have attempts = 1
        let attempts_before = EterraSlots::card_attempts(card_id);
        assert_eq!(attempts_before, 1);

        // Accept slot => finalize the card => attempts removed
        assert_ok!(EterraSlots::accept_slot(RuntimeOrigin::signed(player)));

        let attempts_after = EterraSlots::card_attempts(card_id);
        assert_eq!(
            attempts_after, 0,
            "Expected attempts to be removed after finalization."
        );
    });
}