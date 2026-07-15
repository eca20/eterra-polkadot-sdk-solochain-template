//! Unit tests for pallet-eterra-gamer.
#![cfg(test)]

use super::*;
use crate::mock::*;
use crate::pallet::{
    AccountToSteam, ArcadeInitials, AvatarCid, Error as GamerError, Experience, GamerProfiles,
    GamerTag, Level, RegionCode, SteamLinkAuthority, SteamToAccount, UsedSteamLinkNonces,
};
use frame_support::BoundedVec;
use frame_support::{assert_noop, assert_ok};
use sp_core::{sr25519, Pair};
use sp_runtime::codec::Encode;

type TestBlockNumber = frame_system::pallet_prelude::BlockNumberFor<Test>;

fn steam_hash(seed: u8) -> SteamHash {
    [seed; 32]
}

fn reason_hash(seed: u8) -> ReasonHash {
    [seed; 32]
}

fn nonce(seed: u8) -> SteamLinkNonce {
    [seed; 32]
}

fn authority_pair() -> sr25519::Pair {
    sr25519::Pair::from_seed(&[42u8; 32])
}

fn steam_link_payload(
    account: AccountId,
    steam_hash: SteamHash,
    nonce: SteamLinkNonce,
    expires_at: TestBlockNumber,
) -> Vec<u8> {
    let mut payload = b"eterra:gamer:steam-link:v1".to_vec();
    account.encode_to(&mut payload);
    steam_hash.encode_to(&mut payload);
    nonce.encode_to(&mut payload);
    expires_at.encode_to(&mut payload);
    payload
}

fn steam_link_signature(
    account: AccountId,
    steam_hash: SteamHash,
    nonce: SteamLinkNonce,
    expires_at: TestBlockNumber,
) -> BoundedVec<u8, <Test as crate::Config>::MaxSteamLinkSignatureLen> {
    authority_pair()
        .sign(&steam_link_payload(account, steam_hash, nonce, expires_at))
        .0
        .to_vec()
        .try_into()
        .expect("sr25519 signature fits")
}

fn install_authority() {
    assert_ok!(EterraGamer::set_steam_link_authority(
        RuntimeOrigin::root(),
        authority_pair().public().0,
    ));
}

fn arcade_initials(value: &[u8]) -> BoundedVec<u8, <Test as crate::Config>::MaxInitialsLen> {
    value.to_vec().try_into().expect("within max initials len")
}

#[test]
fn first_set_tag_is_free() {
    new_test_ext().execute_with(|| {
        // Record starting faucet balance (may be set to ED in the mock)
        let faucet_before = pallet_balances::Pallet::<Test>::free_balance(FAUCET);

        // First set by ALICE should be free
        let tag = b"AliceTheBrave".to_vec();
        let bounded: BoundedVec<u8, <Test as crate::Config>::MaxTagLen> =
            tag.clone().try_into().expect("within max tag len");
        assert_ok!(EterraGamer::set_gamer_tag(
            RuntimeOrigin::signed(ALICE),
            bounded
        ));
        assert_eq!(GamerTag::<Test>::get(ALICE).unwrap().to_vec(), tag);

        // Faucet balance unchanged (no fee on first set)
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(FAUCET),
            faucet_before
        );
    });
}

#[test]
fn second_set_tag_charges_fee() {
    new_test_ext().execute_with(|| {
        let tag1 = b"Alice1".to_vec();
        let tag2 = b"Alice2".to_vec();

        let tag1: BoundedVec<u8, <Test as crate::Config>::MaxTagLen> =
            tag1.try_into().expect("within max tag len");
        assert_ok!(EterraGamer::set_gamer_tag(
            RuntimeOrigin::signed(ALICE),
            tag1
        ));
        let before_faucet = pallet_balances::Pallet::<Test>::free_balance(FAUCET);
        let before_alice = pallet_balances::Pallet::<Test>::free_balance(ALICE);

        let tag2: BoundedVec<u8, <Test as crate::Config>::MaxTagLen> =
            tag2.try_into().expect("within max tag len");
        assert_ok!(EterraGamer::set_gamer_tag(
            RuntimeOrigin::signed(ALICE),
            tag2
        ));
        // Fee moved
        let fee = ChangeFee::get();
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(FAUCET),
            before_faucet + fee
        );
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(ALICE),
            before_alice - fee
        );
    });
}

#[test]
fn first_set_arcade_initials_is_free() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let faucet_before = pallet_balances::Pallet::<Test>::free_balance(FAUCET);

        assert_ok!(EterraGamer::set_arcade_initials(
            RuntimeOrigin::signed(ALICE),
            arcade_initials(b"AB_1")
        ));
        assert_eq!(
            ArcadeInitials::<Test>::get(ALICE).unwrap().to_vec(),
            b"AB_1"
        );
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(FAUCET),
            faucet_before
        );
        System::assert_last_event(RuntimeEvent::EterraGamer(Event::InitialsSet {
            who: ALICE,
            initials: b"AB_1".to_vec(),
            charged: false,
        }));
    });
}

#[test]
fn second_set_arcade_initials_charges_fee() {
    new_test_ext().execute_with(|| {
        assert_ok!(EterraGamer::set_arcade_initials(
            RuntimeOrigin::signed(ALICE),
            arcade_initials(b"ABC")
        ));

        let before_faucet = pallet_balances::Pallet::<Test>::free_balance(FAUCET);
        let before_alice = pallet_balances::Pallet::<Test>::free_balance(ALICE);

        assert_ok!(EterraGamer::set_arcade_initials(
            RuntimeOrigin::signed(ALICE),
            arcade_initials(b"A-1")
        ));

        let fee = ChangeFee::get();
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(FAUCET),
            before_faucet + fee
        );
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(ALICE),
            before_alice - fee
        );
    });
}

#[test]
fn arcade_initials_accept_classic_machine_characters() {
    new_test_ext().execute_with(|| {
        assert_ok!(EterraGamer::set_arcade_initials(
            RuntimeOrigin::signed(ALICE),
            arcade_initials(b"A._-")
        ));
        assert_eq!(
            ArcadeInitials::<Test>::get(ALICE).unwrap().to_vec(),
            b"A._-"
        );
    });
}

#[test]
fn arcade_initials_reject_lowercase_invalid_and_all_space_values() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            EterraGamer::set_arcade_initials(RuntimeOrigin::signed(ALICE), arcade_initials(b"abc")),
            GamerError::<Test>::InvalidInitials
        );
        assert_noop!(
            EterraGamer::set_arcade_initials(RuntimeOrigin::signed(ALICE), arcade_initials(b"A@1")),
            GamerError::<Test>::InvalidInitials
        );
        assert_noop!(
            EterraGamer::set_arcade_initials(
                RuntimeOrigin::signed(ALICE),
                arcade_initials(b"    ")
            ),
            GamerError::<Test>::InvalidInitials
        );
    });
}

#[test]
fn arcade_initials_max_length_is_enforced_by_bounded_vec() {
    new_test_ext().execute_with(|| {
        let too_long: Result<BoundedVec<u8, <Test as crate::Config>::MaxInitialsLen>, _> =
            b"ABCDE".to_vec().try_into();
        assert!(too_long.is_err());
    });
}

#[test]
fn set_avatar_valid_ascii_and_length() {
    new_test_ext().execute_with(|| {
        let cid = b"bafybeigdyrztvz3kvis4cdwq5lq6eqyqf7x7v2gd3h3b7l5jv2w7".to_vec();
        let bounded: BoundedVec<u8, <Test as crate::Config>::MaxAvatarCidLen> =
            cid.clone().try_into().expect("within max avatar len");
        assert_ok!(EterraGamer::set_avatar(
            RuntimeOrigin::signed(ALICE),
            bounded
        ));
        assert_eq!(AvatarCid::<Test>::get(ALICE).unwrap().to_vec(), cid);
    });
}

#[test]
fn set_avatar_rejects_invalid_ascii() {
    new_test_ext().execute_with(|| {
        let mut cid = b"bafy..ok".to_vec();
        cid[4] = b' '; // space is invalid per validate_ascii_cid (must be 33..=126)
        let bounded: BoundedVec<u8, <Test as crate::Config>::MaxAvatarCidLen> =
            cid.try_into().expect("within max avatar len");
        assert_noop!(
            EterraGamer::set_avatar(RuntimeOrigin::signed(ALICE), bounded),
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
        let cid1: BoundedVec<u8, <Test as crate::Config>::MaxAvatarCidLen> =
            cid1.try_into().expect("within max avatar len");
        assert_ok!(EterraGamer::set_avatar(RuntimeOrigin::signed(BOB), cid1));
        // Drain BOB so change fee cannot be paid
        pallet_balances::Pallet::<Test>::make_free_balance_be(&BOB, 0);

        let cid2: BoundedVec<u8, <Test as crate::Config>::MaxAvatarCidLen> =
            cid2.try_into().expect("within max avatar len");
        assert_noop!(
            EterraGamer::set_avatar(RuntimeOrigin::signed(BOB), cid2),
            GamerError::<Test>::InsufficientBalanceForChange
        );
    });
}

#[test]
fn set_region_stores_updates_and_clears_country_code() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let us: BoundedVec<u8, <Test as crate::Config>::MaxRegionCodeLen> =
            b"US".to_vec().try_into().expect("two-byte region fits");
        assert_ok!(EterraGamer::set_region(
            RuntimeOrigin::signed(ALICE),
            Some(us)
        ));
        assert_eq!(RegionCode::<Test>::get(ALICE).unwrap().to_vec(), b"US");

        let jp: BoundedVec<u8, <Test as crate::Config>::MaxRegionCodeLen> =
            b"JP".to_vec().try_into().expect("two-byte region fits");
        assert_ok!(EterraGamer::set_region(
            RuntimeOrigin::signed(ALICE),
            Some(jp)
        ));
        assert_eq!(RegionCode::<Test>::get(ALICE).unwrap().to_vec(), b"JP");
        System::assert_last_event(RuntimeEvent::EterraGamer(Event::RegionSet {
            who: ALICE,
            country_code: Some(b"JP".to_vec()),
        }));

        assert_ok!(EterraGamer::set_region(RuntimeOrigin::signed(ALICE), None));
        assert_eq!(RegionCode::<Test>::get(ALICE), None);
        System::assert_last_event(RuntimeEvent::EterraGamer(Event::RegionSet {
            who: ALICE,
            country_code: None,
        }));
    });
}

#[test]
fn set_region_rejects_non_uppercase_alpha2_codes() {
    new_test_ext().execute_with(|| {
        let lower: BoundedVec<u8, <Test as crate::Config>::MaxRegionCodeLen> =
            b"us".to_vec().try_into().expect("two-byte region fits");
        assert_noop!(
            EterraGamer::set_region(RuntimeOrigin::signed(ALICE), Some(lower)),
            GamerError::<Test>::InvalidRegionCode
        );

        let numeric: BoundedVec<u8, <Test as crate::Config>::MaxRegionCodeLen> =
            b"U1".to_vec().try_into().expect("two-byte region fits");
        assert_noop!(
            EterraGamer::set_region(RuntimeOrigin::signed(ALICE), Some(numeric)),
            GamerError::<Test>::InvalidRegionCode
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
        assert_ok!(EterraGamer::grant_experience(
            RuntimeOrigin::root(),
            ALICE,
            total
        ));
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

#[test]
fn exp_required_is_monotonic() {
    // Ensure required EXP strictly increases per level.
    let mut prev = 0u128;
    for lvl in 1u8..=99u8 {
        let need = EterraGamer::exp_required_for_level(lvl);
        assert!(need > prev, "level {} not increasing", lvl);
        prev = need;
    }
}

#[test]
fn redeem_caps_at_99() {
    new_test_ext().execute_with(|| {
        Level::<Test>::insert(ALICE, 98u8);
        Experience::<Test>::insert(ALICE, u128::MAX);

        assert_ok!(EterraGamer::redeem_levels(RuntimeOrigin::signed(ALICE)));

        assert_eq!(Level::<Test>::get(ALICE), 99u8);
    });
}

#[test]
fn set_steam_link_authority_rejects_zero_and_stores_key() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            EterraGamer::set_steam_link_authority(RuntimeOrigin::root(), [0; 32]),
            GamerError::<Test>::InvalidSteamLinkAuthority
        );

        install_authority();
        assert_eq!(
            SteamLinkAuthority::<Test>::get(),
            Some(authority_pair().public().0)
        );
    });
}

#[test]
fn link_steam_stores_bidirectional_link_profile_and_nonce() {
    new_test_ext().execute_with(|| {
        install_authority();
        System::set_block_number(1);
        let hash = steam_hash(7);
        let nonce = nonce(9);
        let expires_at: TestBlockNumber = 10;

        assert_ok!(EterraGamer::link_steam(
            RuntimeOrigin::signed(ALICE),
            hash,
            nonce,
            expires_at,
            steam_link_signature(ALICE, hash, nonce, expires_at),
        ));

        assert_eq!(SteamToAccount::<Test>::get(hash), Some(ALICE));
        assert_eq!(AccountToSteam::<Test>::get(ALICE), Some(hash));
        assert!(UsedSteamLinkNonces::<Test>::contains_key(nonce));
        assert_eq!(
            GamerProfiles::<Test>::get(ALICE),
            Some(GamerProfile {
                linked_at: 1,
                frozen: false,
                freeze_reason: None,
            })
        );
        System::assert_last_event(RuntimeEvent::EterraGamer(Event::SteamLinked {
            steam_hash: hash,
            account: ALICE,
        }));
    });
}

#[test]
fn link_steam_rejects_expired_replayed_duplicate_and_bad_signatures() {
    new_test_ext().execute_with(|| {
        install_authority();
        System::set_block_number(5);
        let hash = steam_hash(7);
        let nonce_a = nonce(9);

        assert_noop!(
            EterraGamer::link_steam(
                RuntimeOrigin::signed(ALICE),
                hash,
                nonce_a,
                5,
                steam_link_signature(ALICE, hash, nonce_a, 5),
            ),
            GamerError::<Test>::SteamLinkExpired
        );

        assert_noop!(
            EterraGamer::link_steam(
                RuntimeOrigin::signed(ALICE),
                hash,
                nonce_a,
                10,
                steam_link_signature(BOB, hash, nonce_a, 10),
            ),
            GamerError::<Test>::InvalidSteamLinkSignature
        );

        assert_ok!(EterraGamer::link_steam(
            RuntimeOrigin::signed(ALICE),
            hash,
            nonce_a,
            10,
            steam_link_signature(ALICE, hash, nonce_a, 10),
        ));
        assert_noop!(
            EterraGamer::link_steam(
                RuntimeOrigin::signed(BOB),
                steam_hash(8),
                nonce_a,
                10,
                steam_link_signature(BOB, steam_hash(8), nonce_a, 10),
            ),
            GamerError::<Test>::SteamLinkNonceUsed
        );
        assert_noop!(
            EterraGamer::link_steam(
                RuntimeOrigin::signed(ALICE),
                steam_hash(8),
                nonce(10),
                10,
                steam_link_signature(ALICE, steam_hash(8), nonce(10), 10),
            ),
            GamerError::<Test>::AlreadyLinked
        );
        assert_noop!(
            EterraGamer::link_steam(
                RuntimeOrigin::signed(BOB),
                hash,
                nonce(11),
                10,
                steam_link_signature(BOB, hash, nonce(11), 10),
            ),
            GamerError::<Test>::SteamHashAlreadyLinked
        );
    });
}

#[test]
fn unlink_steam_removes_link_and_profile() {
    new_test_ext().execute_with(|| {
        install_authority();
        System::set_block_number(1);
        let hash = steam_hash(12);
        let nonce = nonce(12);

        assert_noop!(
            EterraGamer::unlink_steam(RuntimeOrigin::signed(ALICE)),
            GamerError::<Test>::SteamHashNotLinked
        );
        assert_ok!(EterraGamer::link_steam(
            RuntimeOrigin::signed(ALICE),
            hash,
            nonce,
            10,
            steam_link_signature(ALICE, hash, nonce, 10),
        ));
        assert_ok!(EterraGamer::unlink_steam(RuntimeOrigin::signed(ALICE)));

        assert_eq!(SteamToAccount::<Test>::get(hash), None);
        assert_eq!(AccountToSteam::<Test>::get(ALICE), None);
        assert_eq!(GamerProfiles::<Test>::get(ALICE), None);
        System::assert_last_event(RuntimeEvent::EterraGamer(Event::SteamUnlinked {
            steam_hash: hash,
            account: ALICE,
        }));
    });
}

#[test]
fn freeze_and_unfreeze_player_control_profile_actions() {
    new_test_ext().execute_with(|| {
        install_authority();
        let hash = steam_hash(13);
        let nonce = nonce(13);
        assert_ok!(EterraGamer::link_steam(
            RuntimeOrigin::signed(ALICE),
            hash,
            nonce,
            10,
            steam_link_signature(ALICE, hash, nonce, 10),
        ));

        assert_ok!(EterraGamer::freeze_player(
            RuntimeOrigin::root(),
            ALICE,
            reason_hash(3),
        ));
        assert_noop!(
            EterraGamer::set_gamer_tag(
                RuntimeOrigin::signed(ALICE),
                b"FrozenAlice".to_vec().try_into().unwrap(),
            ),
            GamerError::<Test>::PlayerFrozen
        );
        assert_noop!(
            EterraGamer::set_region(
                RuntimeOrigin::signed(ALICE),
                Some(b"US".to_vec().try_into().unwrap()),
            ),
            GamerError::<Test>::PlayerFrozen
        );
        assert_ok!(EterraGamer::unfreeze_player(RuntimeOrigin::root(), ALICE));
        assert_ok!(EterraGamer::set_gamer_tag(
            RuntimeOrigin::signed(ALICE),
            b"AliceAgain".to_vec().try_into().unwrap(),
        ));
    });
}
