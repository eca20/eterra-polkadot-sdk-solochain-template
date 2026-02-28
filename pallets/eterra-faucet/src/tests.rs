use crate::{mock::*, Error, LastClaim, PayoutAmount, SponsoredClaimsUsed, SponsoredWindowStart};
use frame_support::{assert_noop, assert_ok, traits::Currency};

#[test]
fn claim_transfers_and_records_state() {
    new_test_ext().execute_with(|| {
        let faucet_before = Balances::free_balance(FAUCET);

        assert_ok!(EterraFaucet::claim(RuntimeOrigin::signed(BOB), BOB));

        assert_eq!(Balances::free_balance(BOB), PAYOUT);
        assert_eq!(Balances::free_balance(FAUCET), faucet_before - PAYOUT);
        assert_eq!(LastClaim::<Test>::get(BOB), Some(1));
        assert_eq!(SponsoredWindowStart::<Test>::get(BOB), Some(1));
        assert_eq!(SponsoredClaimsUsed::<Test>::get(BOB), 1);
    });
}

#[test]
fn claim_rejects_non_self_destination() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            EterraFaucet::claim(RuntimeOrigin::signed(BOB), CHARLIE),
            Error::<Test>::InvalidDestination
        );
    });
}

#[test]
fn claim_enforces_cooldown() {
    new_test_ext().execute_with(|| {
        assert_ok!(EterraFaucet::claim(RuntimeOrigin::signed(BOB), BOB));

        System::set_block_number(5);
        assert_noop!(
            EterraFaucet::claim(RuntimeOrigin::signed(BOB), BOB),
            Error::<Test>::TooFrequent
        );

        System::set_block_number(11);
        assert_ok!(EterraFaucet::claim(RuntimeOrigin::signed(BOB), BOB));
    });
}

#[test]
fn sponsored_quota_resets_after_window() {
    new_test_ext().execute_with(|| {
        SponsoredWindowStart::<Test>::insert(BOB, 1);
        SponsoredClaimsUsed::<Test>::insert(BOB, 2);

        assert!(!EterraFaucet::can_receive_sponsored_claim(&BOB, 10));
        assert!(EterraFaucet::can_receive_sponsored_claim(&BOB, 22));
    });
}

#[test]
fn pre_dispatch_requires_cooldown_and_liquidity() {
    new_test_ext().execute_with(|| {
        assert!(EterraFaucet::can_receive_sponsored_claim_pre_dispatch(
            &BOB, 1, 10
        ));

        LastClaim::<Test>::insert(BOB, 1);
        assert!(!EterraFaucet::can_receive_sponsored_claim_pre_dispatch(
            &BOB, 5, 10
        ));
        assert!(EterraFaucet::can_receive_sponsored_claim_pre_dispatch(
            &BOB, 11, 10
        ));

        let _ = <Balances as Currency<u64>>::deposit_creating(&BOB, 1_000);
        assert!(!EterraFaucet::can_receive_sponsored_claim_pre_dispatch(
            &BOB, 12, 10
        ));

        PayoutAmount::<Test>::put(2_000_000u128);
        assert!(!EterraFaucet::can_receive_sponsored_claim_pre_dispatch(
            &BOB, 30, 10
        ));
    });
}
