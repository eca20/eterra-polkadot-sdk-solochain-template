#![cfg(test)]

use super::*;
use crate::mock::*;
use frame_support::{assert_noop, assert_ok, BoundedVec};
use sp_core::H256;

fn source(id: u8) -> AccessSource<H256> {
    AccessSource {
        source_kind: AccessSourceKind::ContractEventRelayer,
        source_chain_id: HUB_CHAIN_ID,
        source_contract: CONTRACT,
        source_event_id: H256::repeat_byte(id),
        source_tx_hash: Some([id; 32]),
        source_log_index: Some(id as u32),
        token_id: id as u128,
    }
}

fn setup_manager_and_source() {
    assert_ok!(AlphaAccess::set_manager(
        RuntimeOrigin::root(),
        MANAGER,
        true
    ));
    assert_ok!(AlphaAccess::set_allowed_source(
        RuntimeOrigin::root(),
        AccessSourceKind::ContractEventRelayer,
        HUB_CHAIN_ID,
        CONTRACT,
        true
    ));
}

#[test]
fn admin_can_set_manager_and_source() {
    new_test_ext().execute_with(|| {
        setup_manager_and_source();
        assert!(AlphaAccess::managers(MANAGER).is_some());
        assert!(AlphaAccess::allowed_sources((
            AccessSourceKind::ContractEventRelayer,
            HUB_CHAIN_ID,
            CONTRACT
        ))
        .is_some());
    });
}

#[test]
fn non_admin_cannot_set_manager() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            AlphaAccess::set_manager(RuntimeOrigin::signed(ALICE), MANAGER, true),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn manager_can_grant_access_and_duplicate_fails() {
    new_test_ext().execute_with(|| {
        setup_manager_and_source();
        let grant_source = source(1);
        assert_ok!(AlphaAccess::grant_access(
            RuntimeOrigin::signed(MANAGER),
            ALICE,
            grant_source.clone(),
            0
        ));
        assert!(AlphaAccess::is_whitelisted(&ALICE));
        assert_noop!(
            AlphaAccess::grant_access(RuntimeOrigin::signed(MANAGER), BOB, grant_source, 0),
            Error::<Test>::SourceAlreadyProcessed
        );
    });
}

#[test]
fn non_manager_cannot_grant_access() {
    new_test_ext().execute_with(|| {
        setup_manager_and_source();
        assert_noop!(
            AlphaAccess::grant_access(RuntimeOrigin::signed(ALICE), BOB, source(2), 0),
            Error::<Test>::NotAuthorized
        );
    });
}

#[test]
fn disabled_source_cannot_grant_access() {
    new_test_ext().execute_with(|| {
        assert_ok!(AlphaAccess::set_manager(
            RuntimeOrigin::root(),
            MANAGER,
            true
        ));
        assert_noop!(
            AlphaAccess::grant_access(RuntimeOrigin::signed(MANAGER), ALICE, source(3), 0),
            Error::<Test>::SourceNotAllowed
        );
    });
}

#[test]
fn revoke_removes_access() {
    new_test_ext().execute_with(|| {
        setup_manager_and_source();
        assert_ok!(AlphaAccess::grant_access(
            RuntimeOrigin::signed(MANAGER),
            ALICE,
            source(4),
            0
        ));
        let reason: BoundedVec<u8, <Test as Config>::MaxRevokeReasonLen> =
            b"refund".to_vec().try_into().unwrap();
        assert_ok!(AlphaAccess::revoke_access(
            RuntimeOrigin::signed(MANAGER),
            ALICE,
            reason
        ));
        assert!(!AlphaAccess::is_whitelisted(&ALICE));
        assert_noop!(
            AlphaAccess::ensure_whitelisted(&ALICE),
            Error::<Test>::NotWhitelisted
        );
    });
}

#[test]
fn expired_access_fails_but_lifetime_works() {
    new_test_ext().execute_with(|| {
        setup_manager_and_source();
        Timestamp::set_timestamp(10_000);
        assert_ok!(AlphaAccess::grant_access(
            RuntimeOrigin::signed(MANAGER),
            ALICE,
            source(5),
            9
        ));
        assert_noop!(
            AlphaAccess::ensure_whitelisted(&ALICE),
            Error::<Test>::Expired
        );

        assert_ok!(AlphaAccess::grant_access(
            RuntimeOrigin::signed(MANAGER),
            BOB,
            source(6),
            0
        ));
        assert_ok!(AlphaAccess::ensure_whitelisted(&BOB));
    });
}
