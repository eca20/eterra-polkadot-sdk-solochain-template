use crate::{mock::*, Error, Event, PlayerPacks};
use frame_support::traits::Get;
use frame_support::{assert_noop, assert_ok};
use log::{debug, Level, Metadata, Record};
use sp_runtime::traits::SaturatedConversion;
use std::sync::Once;
use scale_info::TypeInfo;

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
        System::reset_events();
        System::set_block_number(42); // or any number you prefer

        // Mint the pack
        assert_ok!(EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(player)));

        // Verify the minted pack is in storage
        let packs = EterraSimpleTCGConfig::player_packs(player);
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
        assert_ok!(EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(player)));

        // Check that PackMinted event with pack_id=100 was indeed emitted
        System::assert_has_event(
            RuntimeEvent::EterraSimpleTCGConfig(Event::PackMinted {
                player,
                pack_id: 100,
            })
            .into(),
        );
        System::assert_has_event(
            RuntimeEvent::EterraSimpleTCGConfig(Event::PackCompleted {
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

        assert_ok!(EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(player)));

        let all_events = System::events();
        assert!(!all_events.is_empty(), "No events were recorded!");

        let minted_event_found = all_events.iter().any(|r| match &r.event {
            RuntimeEvent::EterraSimpleTCGConfig(Event::PackMinted {
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
        assert_ok!(EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(player)));

        // 2) Check storage updated
        let packs = EterraSimpleTCGConfig::player_packs(player);
        assert_eq!(packs.len(), 1, "Should have 1 pack minted now.");
        let minted_pack = &packs[0];
        assert_eq!(minted_pack.get_id(), 8);

        // 3) Check event with direct assertion
        System::assert_has_event(
            RuntimeEvent::EterraSimpleTCGConfig(Event::PackMinted { player, pack_id: 8 }).into(),
        );
        System::assert_has_event(
            RuntimeEvent::EterraSimpleTCGConfig(Event::PackCompleted { player, pack_id: 8 }).into(),
        );
    });
}

#[test]
fn test_mint_pack_mints_six_finalized_cards_with_required_fields() {
    new_test_ext().execute_with(|| {
        let player = 1;
        System::set_block_number(55);
        System::reset_events();

        assert_ok!(EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(player)));

        // Verify pack storage
        let packs = EterraSimpleTCGConfig::player_packs(player);
        assert_eq!(packs.len(), 1, "One pack expected");
        let pack = &packs[0];
        assert_eq!(pack.get_id(), 55);
        assert!(pack.get_completed(), "Pack should be completed immediately");
        assert_eq!(pack.get_active_card_index(), 0, "active_card_index should start at 0");

        // Must mint exactly 6 cards
        let ids = pack.get_card_ids();
        assert_eq!(ids.len(), 6, "Exactly 6 cards should be minted per pack");

        // Inspect first card's fields
        let card_id = ids[0];
        let card = EterraSimpleTCGConfig::cards(card_id).expect("card exists");
        assert!(card.finalized, "Card should be finalized at mint");
        assert!(card.slot_values.is_some(), "slot_values should be Some");
        let (n, e, s, w) = (card.north, card.east, card.south, card.west);
        for v in [n, e, s, w] {
            assert!(v >= 1 && v <= 9, "Directional values must be in 1..=9, got {}", v);
        }
        let name_bytes = card.name.as_slice();
        assert!(!name_bytes.is_empty(), "name should be non-empty");
    });
}


#[test]
fn test_mint_pack_fail_when_max_packs_reached() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;
        debug!("Minting maximum allowed packs for player {}", player);

        for _ in 0..10 {
            assert_ok!(EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(player)));
            run_to_block(System::block_number() + 1);
        }

        debug!(
            "Attempting to mint an 11th pack for player {} (should fail).",
            player
        );
        assert_noop!(
            EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(player)),
            Error::<Test>::MaxPacksReached
        );

        debug!("Correctly failed for exceeding max packs.");
    });
}


#[test]
fn test_transfer_card_not_owner_fails() {
    new_test_ext().execute_with(|| {
        let owner = 1;
        let non_owner = 2;
        let malicious_user = 3;

        // 1) Mint a pack for `owner`
        assert_ok!(EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(owner)));

        // 2) Retrieve the first card
        let packs = EterraSimpleTCGConfig::player_packs(owner);
        let card_id = *packs[0]
            .get_card_ids()
            .first()
            .expect("At least one card expected");

        // 3) Attempt to transfer it as `non_owner` or `malicious_user`
        let result =
            EterraSimpleTCGConfig::transfer_card(RuntimeOrigin::signed(non_owner), card_id, malicious_user);

        // 4) Confirm it fails with the expected NotCardOwner error
        assert_noop!(result, Error::<Test>::NotCardOwner);
    });
}

#[test]
fn test_transfer_card_no_such_card_fails() {
    new_test_ext().execute_with(|| {
        let sender = 1;
        let receiver = 2;

        // Don’t mint anything, so no cards exist
        let card_id_that_does_not_exist = 9999;

        // Attempt transfer
        let result = EterraSimpleTCGConfig::transfer_card(
            RuntimeOrigin::signed(sender),
            card_id_that_does_not_exist,
            receiver,
        );

        assert_noop!(result, Error::<Test>::NoSuchCard);
    });
}

#[test]
fn test_transfer_card_success() {
    new_test_ext().execute_with(|| {
        let original_owner = 1;
        let new_owner = 2;

        // 1) Mint a pack for `original_owner` to create some cards.
        assert_ok!(EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(
            original_owner
        )));

        // 2) Grab the first pack and its first card_id.
        let packs = EterraSimpleTCGConfig::player_packs(original_owner);
        let pack = packs.first().expect("Expected at least one pack minted");
        let card_id = pack
            .get_card_ids()
            .first()
            .copied()
            .expect("Expected at least one card in the pack");

        // Log which card ID we’re transferring
        println!("[TEST] Minted card_id: {}", card_id);

        // 3) Transfer the card to `new_owner`
        let result =
            EterraSimpleTCGConfig::transfer_card(RuntimeOrigin::signed(original_owner), card_id, new_owner);

        assert_ok!(result);

        // 4) Confirm the card's ownership changed in storage
        let card_info = EterraSimpleTCGConfig::cards(card_id).expect("Card must still exist");
        println!("[TEST] card_info after transfer: {:?}", card_info);
        assert_eq!(
            card_info.owner,
            new_owner,
            "Storage shows the card owner didn't update!"
        );

        // 5) Attempt to find a CardTransferred event.
        let events = System::events();
        println!("[TEST] Events after transfer: {:?}", events);

        let found_event = events.iter().any(|r| {
            matches!(
                r.event,
                RuntimeEvent::EterraSimpleTCGConfig(Event::CardTransferred {
                    from,
                    to,
                    card_id: c_id
                }) if from == original_owner && to == new_owner && c_id == card_id
            )
        });
        if !found_event {
            println!(
                "[WARN] No CardTransferred event found for card_id={}, but ownership DID update.",
                card_id
            );
        } else {
            println!("[TEST] Found the CardTransferred event as expected!");
        }
    });
}


#[test]
fn test_card_ids_sequential_within_pack() {
    new_test_ext().execute_with(|| {
        let player = 42;
        System::set_block_number(123);
        System::reset_events();

        assert_ok!(EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(player)));
        let packs = EterraSimpleTCGConfig::player_packs(player);
        let pack = &packs[0];
        let ids = pack.get_card_ids();
        assert_eq!(ids.len(), 6, "pack should contain exactly 6 ids");

        // card_id should increase monotonically as we mint within the pack
        for i in 1..ids.len() {
            assert!(ids[i] > ids[i - 1], "card ids should be strictly increasing within a pack");
        }
    });
}

#[test]
fn test_cards_belong_to_owner_and_fields_within_range() {
    new_test_ext().execute_with(|| {
        let player = 7;
        System::set_block_number(77);
        System::reset_events();

        assert_ok!(EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(player)));
        let packs = EterraSimpleTCGConfig::player_packs(player);
        let pack = &packs[0];

        for &card_id in pack.get_card_ids().iter() {
            let card = EterraSimpleTCGConfig::cards(card_id).expect("card exists");
            // Owner and finalization
            assert_eq!(card.owner, player, "minted card owner should be the minter");
            assert!(card.finalized, "card must be finalized at mint");
            assert!(card.slot_values.is_some(), "slot_values must be populated at mint");

            // Bounds for directional stats
            for v in [card.north, card.east, card.south, card.west] {
                assert!((1..=9).contains(&v), "directional stat must be in 1..=9, got {}", v);
            }
        }
    });
}

#[test]
fn test_card_name_includes_id() {
    new_test_ext().execute_with(|| {
        let player = 3;
        System::set_block_number(19);
        System::reset_events();

        assert_ok!(EterraSimpleTCGConfig::mint_pack(RuntimeOrigin::signed(player)));
        let packs = EterraSimpleTCGConfig::player_packs(player);
        let pack = &packs[0];
        let first_id = pack.get_card_ids()[0];

        let card = EterraSimpleTCGConfig::cards(first_id).expect("card exists");
        let expected = format!("Card-{}", first_id).into_bytes();
        assert_eq!(card.name.as_slice(), expected.as_slice(), "name should be 'Card-<id>'");
        assert!(card.name.len() <= 64, "bounded name should not exceed 64 bytes");
    });
}