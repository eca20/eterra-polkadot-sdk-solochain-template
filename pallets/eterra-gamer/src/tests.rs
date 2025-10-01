//! Unit tests for pallet-eterra-gamer.
#![cfg(test)]

use super::*;
use crate::pallet::{Experience, GamerTag, Level, AvatarCid, Error as GamerError};
use frame_support::{assert_ok, assert_noop};
use crate::mock::*;

#[test]
fn first_set_tag_is_free() {
    new_test_ext().execute_with(|| {
        // Record starting faucet balance (may be set to ED in the mock)
        let faucet_before = pallet_balances::Pallet::<Test>::free_balance(FAUCET);

        // First set by ALICE should be free
        let tag = b"AliceTheBrave".to_vec();
        assert_ok!(EterraGamer::set_gamer_tag(RuntimeOrigin::signed(ALICE), tag.clone()));
        assert_eq!(GamerTag::<Test>::get(ALICE).unwrap().to_vec(), tag);

        // Faucet balance unchanged (no fee on first set)
        assert_eq!(pallet_balances::Pallet::<Test>::free_balance(FAUCET), faucet_before);
    });
}

#[test]
fn second_set_tag_charges_fee() {
    new_test_ext().execute_with(|| {
        let tag1 = b"Alice1".to_vec();
        let tag2 = b"Alice2".to_vec();

        assert_ok!(EterraGamer::set_gamer_tag(RuntimeOrigin::signed(ALICE), tag1));
        let before_faucet = pallet_balances::Pallet::<Test>::free_balance(FAUCET);
        let before_alice = pallet_balances::Pallet::<Test>::free_balance(ALICE);

        assert_ok!(EterraGamer::set_gamer_tag(RuntimeOrigin::signed(ALICE), tag2));
        // Fee moved
        let fee = ChangeFee::get();
        assert_eq!(pallet_balances::Pallet::<Test>::free_balance(FAUCET), before_faucet + fee);
        assert_eq!(pallet_balances::Pallet::<Test>::free_balance(ALICE), before_alice - fee);
    });
}

#[test]
fn set_avatar_valid_ascii_and_length() {
    new_test_ext().execute_with(|| {
        let cid = b"bafybeigdyrztvz3kvis4cdwq5lq6eqyqf7x7v2gd3h3b7l5jv2w7".to_vec();
        assert_ok!(EterraGamer::set_avatar(RuntimeOrigin::signed(ALICE), cid.clone()));
        assert_eq!(AvatarCid::<Test>::get(ALICE).unwrap().to_vec(), cid);
    });
}

#[test]
fn set_avatar_rejects_invalid_ascii() {
    new_test_ext().execute_with(|| {
        let mut cid = b"bafy..ok".to_vec();
        cid[4] = b' '; // space is invalid per validate_ascii_cid (must be 33..=126)
        assert_noop!(
            EterraGamer::set_avatar(RuntimeOrigin::signed(ALICE), cid),
            GamerError::<Test>::AvatarCidInvalidAscii
        );
    });
}

#[test]
fn second_set_avatar_charges_fee_and_fails_if_insufficient() {
    new_test_ext().execute_with(|| {
        let cid1 = b"bafy1".to_vec();
        let cid2 = b"bafy2".to_vec();

        // Give BOB a tiny balance so second change fails
        assert_ok!(EterraGamer::set_avatar(RuntimeOrigin::signed(BOB), cid1));
        // Drain BOB so change fee cannot be paid
        pallet_balances::Pallet::<Test>::make_free_balance_be(&BOB, 0);

        assert_noop!(
            EterraGamer::set_avatar(RuntimeOrigin::signed(BOB), cid2),
            GamerError::<Test>::InsufficientBalanceForChange
        );
    });
}

#[test]
fn grant_exp_and_redeem_levels_progresses() {
    new_test_ext().execute_with(|| {
        // Grant enough exp for a few levels
        let l1 = EterraGamer::exp_required_for_level(1);
        let l2 = EterraGamer::exp_required_for_level(2);
        let l3 = EterraGamer::exp_required_for_level(3);
        let total = l1 + l2 + l3 + 10; // a bit extra

        // Only privileged origin can grant
        assert_ok!(EterraGamer::grant_experience(RuntimeOrigin::root(), ALICE, total));
        assert_eq!(Experience::<Test>::get(ALICE), total);

        // Redeem
        assert_ok!(EterraGamer::redeem_levels(RuntimeOrigin::signed(ALICE)));
        // Expect to be at least level 3
        assert!(Level::<Test>::get(ALICE) >= 3);
        // Unredeemed exp dropped
        assert!(Experience::<Test>::get(ALICE) < total);
    });
}

#[test]
fn redeem_without_enough_exp_fails() {
    new_test_ext().execute_with(|| {
        // No exp
        assert_noop!(
            EterraGamer::redeem_levels(RuntimeOrigin::signed(ALICE)),
            GamerError::<Test>::NotEnoughExperience
        );
    });
}

#[test]
fn already_max_level_fails() {
    new_test_ext().execute_with(|| {
        // Force ALICE to level 99 and some exp
        Level::<Test>::insert(ALICE, 99u8);
        Experience::<Test>::insert(ALICE, 1_000);
        assert_noop!(
            EterraGamer::redeem_levels(RuntimeOrigin::signed(ALICE)),
            GamerError::<Test>::AlreadyMaxLevel
        );
    });
}
