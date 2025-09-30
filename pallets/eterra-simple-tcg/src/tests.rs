use super::*;
use crate::{mock::*, Event as TcgEvent};
use frame_support::{assert_noop, assert_ok, traits::OnInitialize};

fn last_event() -> RuntimeEvent {
    System::events().pop().expect("Event expected").event
}

#[test]
fn mint_card_charges_fee_and_mints() {
    new_test_ext().execute_with(|| {
        // Block 1 for deterministic hashing
        System::set_block_number(1);

        let faucet_before = Balances::free_balance(ALICE);
        let bob_before = Balances::free_balance(BOB);

        // Bob mints a card (fee 100 should go to Alice faucet)
        assert_ok!(EterraSimpleTCGConfig::mint_card(RuntimeOrigin::signed(BOB)));

        // Ownership & indices
        let owned = EterraSimpleTCGConfig::owned_cards(BOB);
        assert_eq!(owned.len(), 1);
        let card_id = owned[0];
        let card = EterraSimpleTCGConfig::cards(card_id).expect("card exists");
        assert_eq!(card.owner, BOB);

        // Fee accounting (Balances is u128 in mock)
        let faucet_after = Balances::free_balance(ALICE);
        let bob_after = Balances::free_balance(BOB);
        assert_eq!(faucet_after, faucet_before + 100);
        assert_eq!(bob_after, bob_before - 100);

        // Event
        System::assert_has_event(RuntimeEvent::EterraSimpleTCGConfig(TcgEvent::CardMinted {
            player: BOB,
            card_id,
        }));
    });
}

#[test]
fn set_and_remove_price_updates_storage_and_events() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        assert_ok!(EterraSimpleTCGConfig::mint_card(RuntimeOrigin::signed(BOB)));
        let id = EterraSimpleTCGConfig::owned_cards(BOB)[0];

        // List for sale
        assert_ok!(EterraSimpleTCGConfig::set_price(
            RuntimeOrigin::signed(BOB),
            id,
            500
        ));
        assert_eq!(EterraSimpleTCGConfig::card_prices(id), Some(500));
        assert!(EterraSimpleTCGConfig::listed_by_owner(BOB).contains(&id));
        System::assert_has_event(RuntimeEvent::EterraSimpleTCGConfig(TcgEvent::CardListed {
            owner: BOB,
            card_id: id,
            price: 500,
        }));

        // Unlist
        assert_ok!(EterraSimpleTCGConfig::remove_price(
            RuntimeOrigin::signed(BOB),
            id
        ));
        assert_eq!(EterraSimpleTCGConfig::card_prices(id), None);
        assert!(!EterraSimpleTCGConfig::listed_by_owner(BOB).contains(&id));
        System::assert_has_event(RuntimeEvent::EterraSimpleTCGConfig(
            TcgEvent::CardUnlisted {
                owner: BOB,
                card_id: id,
            },
        ));
    });
}

#[test]
fn transfer_card_auto_unlists() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        // Bob mints and lists
        assert_ok!(EterraSimpleTCGConfig::mint_card(RuntimeOrigin::signed(BOB)));
        let id = EterraSimpleTCGConfig::owned_cards(BOB)[0];
        assert_ok!(EterraSimpleTCGConfig::set_price(
            RuntimeOrigin::signed(BOB),
            id,
            777
        ));
        assert!(EterraSimpleTCGConfig::card_prices(id).is_some());

        // Transfer to Alice; should unlist
        assert_ok!(EterraSimpleTCGConfig::transfer_card(
            RuntimeOrigin::signed(BOB),
            id,
            ALICE
        ));
        let card = EterraSimpleTCGConfig::cards(id).unwrap();
        assert_eq!(card.owner, ALICE);
        assert!(EterraSimpleTCGConfig::owned_cards(ALICE).contains(&id));
        assert!(!EterraSimpleTCGConfig::owned_cards(BOB).contains(&id));

        // Listing removed
        assert_eq!(EterraSimpleTCGConfig::card_prices(id), None);
        assert!(!EterraSimpleTCGConfig::listed_by_owner(BOB).contains(&id));
    });
}

#[test]
fn buy_card_transfers_funds_and_ownership_then_unlists() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);

        // Seller Alice mints, lists at 200
        assert_ok!(EterraSimpleTCGConfig::mint_card(RuntimeOrigin::signed(
            ALICE
        )));
        let id = EterraSimpleTCGConfig::owned_cards(ALICE)[0];
        assert_ok!(EterraSimpleTCGConfig::set_price(
            RuntimeOrigin::signed(ALICE),
            id,
            200
        ));

        let alice_before = Balances::free_balance(ALICE);
        let bob_before = Balances::free_balance(BOB);

        // Bob buys
        assert_ok!(EterraSimpleTCGConfig::buy_card(
            RuntimeOrigin::signed(BOB),
            id
        ));

        // Ownership moved to Bob
        let card = EterraSimpleTCGConfig::cards(id).unwrap();
        assert_eq!(card.owner, BOB);
        assert!(EterraSimpleTCGConfig::owned_cards(BOB).contains(&id));
        assert!(!EterraSimpleTCGConfig::owned_cards(ALICE).contains(&id));

        // Listing removed
        assert_eq!(EterraSimpleTCGConfig::card_prices(id), None);
        assert!(!EterraSimpleTCGConfig::listed_by_owner(ALICE).contains(&id));

        // Funds moved: Bob -200, Alice +200
        let alice_after = Balances::free_balance(ALICE);
        let bob_after = Balances::free_balance(BOB);
        assert_eq!(alice_after, alice_before + 200);
        assert_eq!(bob_after, bob_before - 200);

        // Event emitted
        System::assert_has_event(RuntimeEvent::EterraSimpleTCGConfig(TcgEvent::CardBought {
            buyer: BOB,
            seller: ALICE,
            card_id: id,
            price: 200,
        }));
    });
}

#[test]
fn buy_card_fails_if_not_listed() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        assert_ok!(EterraSimpleTCGConfig::mint_card(RuntimeOrigin::signed(
            ALICE
        )));
        let id = EterraSimpleTCGConfig::owned_cards(ALICE)[0];
        assert_noop!(
            EterraSimpleTCGConfig::buy_card(RuntimeOrigin::signed(BOB), id),
            Error::<Test>::NotForSale
        );
    });
}
