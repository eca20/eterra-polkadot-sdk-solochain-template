use crate::{
    mock::*, CurrentMode, Error, LatestVerifiedAt, LatestVerifiedRound, NextRequestNonce, Outputs,
    RandomnessMode, RequestContexts, RequestStatus, Requests, VerifiableRandomness,
};
use codec::Encode;
use eterra_nexus_primitives::{EconomicRealm, DRAND_QUICKNET_CHAIN_HASH};
use frame_support::{assert_noop, assert_ok};

#[test]
fn randomness_enum_scale_indices_are_frozen() {
    assert_eq!(
        [
            RandomnessMode::Disabled.encode()[0],
            RandomnessMode::DeterministicPrivateAlpha.encode()[0],
            RandomnessMode::DrandQuicknet.encode()[0],
        ],
        [0, 1, 2]
    );
    assert_eq!(
        [
            RequestStatus::Pending.encode()[0],
            RequestStatus::Finalized.encode()[0],
            RequestStatus::TimedOut.encode()[0],
        ],
        [0, 1, 2]
    );
}

#[test]
fn unit_randomness_provider_is_never_production_ready() {
    assert!(!<() as VerifiableRandomness>::production_ready());
    assert_eq!(
        <() as VerifiableRandomness>::current_mode(),
        RandomnessMode::Disabled
    );
    assert!(<() as VerifiableRandomness>::request_for(
        EconomicRealm::Production,
        RandomnessMode::DrandQuicknet,
        [1; 32],
        [2; 32],
        [3; 32],
        0,
    )
    .is_err());
}

#[test]
fn alpha_request_is_delayed_and_domain_bound() {
    new_test_ext().execute_with(|| {
        assert_ok!(Randomness::set_mode(
            RuntimeOrigin::root(),
            RandomnessMode::DeterministicPrivateAlpha
        ));
        let request_id =
            <Randomness as VerifiableRandomness>::request([1; 32], [2; 32], [3; 32], 9).unwrap();
        assert_noop!(
            Randomness::finalize_alpha(RuntimeOrigin::signed(7), request_id),
            Error::<Test>::TooEarly
        );
        System::set_block_number(3);
        assert_ok!(Randomness::finalize_alpha(
            RuntimeOrigin::signed(7),
            request_id
        ));
        let output = Outputs::<Test>::get(request_id).unwrap();
        assert!(output.deterministic_alpha);
        assert_eq!(output.epoch, 9);
        assert_eq!(
            Requests::<Test>::get(request_id).unwrap().status,
            RequestStatus::Finalized
        );
    });
}

#[test]
fn drand_mode_cannot_activate_without_review_and_exact_round() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Randomness::set_mode(RuntimeOrigin::root(), RandomnessMode::DrandQuicknet),
            Error::<Test>::CryptographyReviewRequired
        );
        assert_ok!(Randomness::set_cryptography_review_status(
            RuntimeOrigin::root(),
            true
        ));
        assert_noop!(
            Randomness::set_mode(RuntimeOrigin::root(), RandomnessMode::DrandQuicknet),
            Error::<Test>::BeaconNotBootstrapped
        );
        assert_noop!(
            Randomness::submit_drand_checkpoint(RuntimeOrigin::signed(1), 1, vec![7; 32]),
            Error::<Test>::BeaconStale
        );
        let checkpoint = Randomness::current_drand_round().unwrap();
        assert_ok!(Randomness::submit_drand_checkpoint(
            RuntimeOrigin::signed(1),
            checkpoint,
            vec![7; 32]
        ));
        assert_ok!(Randomness::set_mode(
            RuntimeOrigin::root(),
            RandomnessMode::DrandQuicknet
        ));
        let request_id =
            <Randomness as VerifiableRandomness>::request([4; 32], [5; 32], [6; 32], 12).unwrap();
        let exact_round = Requests::<Test>::get(request_id).unwrap().exact_epoch;
        assert_eq!(exact_round, checkpoint + 4);
        assert_noop!(
            Randomness::submit_drand_quicknet(
                RuntimeOrigin::signed(1),
                request_id,
                exact_round + 1,
                vec![7; 32]
            ),
            Error::<Test>::WrongRound
        );
        assert_noop!(
            Randomness::submit_drand_quicknet(
                RuntimeOrigin::signed(1),
                request_id,
                exact_round,
                vec![7; 32]
            ),
            Error::<Test>::TooEarly
        );
        System::set_block_number(3);
        assert_ok!(Randomness::submit_drand_quicknet(
            RuntimeOrigin::signed(1),
            request_id,
            exact_round,
            vec![7; 32]
        ));
        assert_eq!(CurrentMode::<Test>::get(), RandomnessMode::DrandQuicknet);
        assert_eq!(LatestVerifiedRound::<Test>::get(), exact_round);
    });
}

#[test]
fn snapshotted_drand_request_can_finalize_after_global_pause() {
    new_test_ext().execute_with(|| {
        assert_ok!(Randomness::set_cryptography_review_status(
            RuntimeOrigin::root(),
            true
        ));
        let checkpoint = Randomness::current_drand_round().unwrap();
        assert_ok!(Randomness::submit_drand_checkpoint(
            RuntimeOrigin::signed(1),
            checkpoint,
            vec![7; 32]
        ));
        assert_ok!(Randomness::set_mode(
            RuntimeOrigin::root(),
            RandomnessMode::DrandQuicknet
        ));
        let request_id =
            <Randomness as VerifiableRandomness>::request([4; 32], [5; 32], [6; 32], 0).unwrap();
        let exact_round = Requests::<Test>::get(request_id).unwrap().exact_epoch;

        assert_ok!(Randomness::set_cryptography_review_status(
            RuntimeOrigin::root(),
            false
        ));
        assert_eq!(CurrentMode::<Test>::get(), RandomnessMode::Disabled);
        assert_noop!(
            <Randomness as VerifiableRandomness>::request([7; 32], [8; 32], [9; 32], 0),
            Error::<Test>::RandomnessDisabled
        );

        System::set_block_number(3);
        assert_ok!(Randomness::submit_drand_quicknet(
            RuntimeOrigin::signed(2),
            request_id,
            exact_round,
            vec![9; 32]
        ));
        assert_eq!(
            Requests::<Test>::get(request_id).unwrap().status,
            RequestStatus::Finalized
        );
        assert!(
            !Outputs::<Test>::get(request_id)
                .unwrap()
                .deterministic_alpha
        );
    });
}

#[test]
fn production_ready_requires_review_active_drand_and_fresh_checkpoint() {
    new_test_ext().execute_with(|| {
        assert!(!<Randomness as VerifiableRandomness>::production_ready());
        assert_ok!(Randomness::set_cryptography_review_status(
            RuntimeOrigin::root(),
            true
        ));
        assert!(!<Randomness as VerifiableRandomness>::production_ready());

        let first_round = Randomness::current_drand_round().unwrap();
        assert_ok!(Randomness::submit_drand_checkpoint(
            RuntimeOrigin::signed(1),
            first_round,
            vec![7; 32]
        ));
        assert!(!<Randomness as VerifiableRandomness>::production_ready());
        assert_ok!(Randomness::set_mode(
            RuntimeOrigin::root(),
            RandomnessMode::DrandQuicknet
        ));
        assert!(<Randomness as VerifiableRandomness>::production_ready());

        System::set_block_number(7);
        assert!(!<Randomness as VerifiableRandomness>::production_ready());
        let refreshed_round = Randomness::current_drand_round().unwrap();
        assert_ok!(Randomness::submit_drand_checkpoint(
            RuntimeOrigin::signed(2),
            refreshed_round,
            vec![8; 32]
        ));
        assert!(<Randomness as VerifiableRandomness>::production_ready());

        assert_ok!(Randomness::set_cryptography_review_status(
            RuntimeOrigin::root(),
            false
        ));
        assert!(!<Randomness as VerifiableRandomness>::production_ready());
    });
}

#[test]
fn checkpoint_rounds_are_monotonic_and_stale_bootstrap_fails_closed() {
    new_test_ext().execute_with(|| {
        assert_ok!(Randomness::set_cryptography_review_status(
            RuntimeOrigin::root(),
            true
        ));
        let first_round = Randomness::current_drand_round().unwrap();
        assert_ok!(Randomness::submit_drand_checkpoint(
            RuntimeOrigin::signed(1),
            first_round,
            vec![7; 32]
        ));
        assert_noop!(
            Randomness::submit_drand_checkpoint(RuntimeOrigin::signed(2), first_round, vec![8; 32]),
            Error::<Test>::BeaconRoundNotMonotonic
        );
        System::set_block_number(7);
        assert_noop!(
            Randomness::set_mode(RuntimeOrigin::root(), RandomnessMode::DrandQuicknet),
            Error::<Test>::BeaconStale
        );
        let current_round = Randomness::current_drand_round().unwrap();
        assert_ok!(Randomness::submit_drand_checkpoint(
            RuntimeOrigin::signed(2),
            current_round,
            vec![8; 32]
        ));
        assert_eq!(LatestVerifiedAt::<Test>::get(), Some(7));
        assert_ok!(Randomness::set_mode(
            RuntimeOrigin::root(),
            RandomnessMode::DrandQuicknet
        ));
    });
}

#[test]
fn older_request_finalization_never_moves_beacon_checkpoint_backwards() {
    new_test_ext().execute_with(|| {
        assert_ok!(Randomness::set_cryptography_review_status(
            RuntimeOrigin::root(),
            true
        ));
        let checkpoint = Randomness::current_drand_round().unwrap();
        assert_ok!(Randomness::submit_drand_checkpoint(
            RuntimeOrigin::signed(1),
            checkpoint,
            vec![7; 32]
        ));
        assert_ok!(Randomness::set_mode(
            RuntimeOrigin::root(),
            RandomnessMode::DrandQuicknet
        ));
        let older =
            <Randomness as VerifiableRandomness>::request([1; 32], [2; 32], [3; 32], 0).unwrap();
        let older_round = Requests::<Test>::get(older).unwrap().exact_epoch;
        let newer =
            <Randomness as VerifiableRandomness>::request([4; 32], [5; 32], [6; 32], 110).unwrap();
        let newer_round = Requests::<Test>::get(newer).unwrap().exact_epoch;
        assert!(newer_round > older_round);
        System::set_block_number(5);
        assert_ok!(Randomness::submit_drand_quicknet(
            RuntimeOrigin::signed(1),
            newer,
            newer_round,
            vec![8; 32]
        ));
        let checkpoint_at = LatestVerifiedAt::<Test>::get();
        assert_ok!(Randomness::submit_drand_quicknet(
            RuntimeOrigin::signed(1),
            older,
            older_round,
            vec![9; 32]
        ));
        assert_eq!(LatestVerifiedRound::<Test>::get(), newer_round);
        assert_eq!(LatestVerifiedAt::<Test>::get(), checkpoint_at);
    });
}

#[test]
fn timeout_has_no_randomness_fallback() {
    new_test_ext().execute_with(|| {
        assert_ok!(Randomness::set_mode(
            RuntimeOrigin::root(),
            RandomnessMode::DeterministicPrivateAlpha
        ));
        let request_id =
            <Randomness as VerifiableRandomness>::request([1; 32], [2; 32], [3; 32], 0).unwrap();
        System::set_block_number(21);
        assert_ok!(Randomness::mark_timed_out(
            RuntimeOrigin::signed(2),
            request_id
        ));
        assert!(Outputs::<Test>::get(request_id).is_none());
        assert!(<Randomness as VerifiableRandomness>::timed_out(request_id));
    });
}

#[test]
fn production_request_and_output_remain_drand_bound_across_mode_change() {
    new_test_ext().execute_with(|| {
        assert_ok!(Randomness::set_cryptography_review_status(
            RuntimeOrigin::root(),
            true
        ));
        let checkpoint = Randomness::current_drand_round().unwrap();
        assert_ok!(Randomness::submit_drand_checkpoint(
            RuntimeOrigin::signed(1),
            checkpoint,
            vec![7; 32]
        ));
        assert_ok!(Randomness::set_mode(
            RuntimeOrigin::root(),
            RandomnessMode::DrandQuicknet
        ));
        let request_id = <Randomness as VerifiableRandomness>::request_for(
            EconomicRealm::Production,
            RandomnessMode::DrandQuicknet,
            [11; 32],
            [12; 32],
            [13; 32],
            0,
        )
        .unwrap();
        let context = RequestContexts::<Test>::get(request_id).expect("request context");
        assert_eq!(context.economic_realm, EconomicRealm::Production);
        assert_eq!(context.expected_provenance, RandomnessMode::DrandQuicknet);
        assert_eq!(context.eterra_genesis_hash, [9; 32]);
        assert_eq!(context.pallet_instance_id, 35);
        assert_eq!(context.provider_chain_hash, DRAND_QUICKNET_CHAIN_HASH);
        let exact_round = Requests::<Test>::get(request_id).unwrap().exact_epoch;

        // Governance may pause or change the global mode, but it cannot
        // substitute alpha for the provenance authorized by this request.
        assert_ok!(Randomness::set_mode(
            RuntimeOrigin::root(),
            RandomnessMode::DeterministicPrivateAlpha
        ));
        assert_noop!(
            <Randomness as VerifiableRandomness>::request_for(
                EconomicRealm::Production,
                RandomnessMode::DrandQuicknet,
                [14; 32],
                [15; 32],
                [16; 32],
                0,
            ),
            Error::<Test>::ProvenanceMismatch
        );
        assert_noop!(
            <Randomness as VerifiableRandomness>::request_for(
                EconomicRealm::Production,
                RandomnessMode::DeterministicPrivateAlpha,
                [14; 32],
                [15; 32],
                [16; 32],
                0,
            ),
            Error::<Test>::ProductionRequiresDrandQuicknet
        );

        System::set_block_number(3);
        assert_ok!(Randomness::submit_drand_quicknet(
            RuntimeOrigin::signed(2),
            request_id,
            exact_round,
            vec![8; 32]
        ));
        let output = <Randomness as VerifiableRandomness>::output_for(
            request_id,
            EconomicRealm::Production,
            RandomnessMode::DrandQuicknet,
        )
        .expect("snapshotted Drand output remains consumable");
        assert_eq!(output.provenance, RandomnessMode::DrandQuicknet);
        assert_eq!(output.provider_chain_hash, DRAND_QUICKNET_CHAIN_HASH);
        assert!(<Randomness as VerifiableRandomness>::output_for(
            request_id,
            EconomicRealm::Training,
            RandomnessMode::DrandQuicknet,
        )
        .is_none());
        assert!(<Randomness as VerifiableRandomness>::output_for(
            request_id,
            EconomicRealm::Production,
            RandomnessMode::DeterministicPrivateAlpha,
        )
        .is_none());
        assert_noop!(
            Randomness::submit_drand_quicknet(
                RuntimeOrigin::signed(3),
                request_id,
                exact_round,
                vec![8; 32]
            ),
            Error::<Test>::RequestNotPending
        );
    });
}

#[test]
fn request_ids_are_separated_by_genesis_and_pallet_instance() {
    new_test_ext().execute_with(|| {
        assert_ok!(Randomness::set_mode(
            RuntimeOrigin::root(),
            RandomnessMode::DeterministicPrivateAlpha
        ));
        let request = || {
            <Randomness as VerifiableRandomness>::request_for(
                EconomicRealm::Training,
                RandomnessMode::DeterministicPrivateAlpha,
                [21; 32],
                [22; 32],
                [23; 32],
                9,
            )
            .unwrap()
        };
        let original = request();

        NextRequestNonce::<Test>::put(0);
        MOCK_GENESIS_HASH.with(|value| value.set([8; 32]));
        let other_genesis = request();

        NextRequestNonce::<Test>::put(0);
        MOCK_GENESIS_HASH.with(|value| value.set([9; 32]));
        MOCK_PALLET_INSTANCE_ID.with(|value| value.set(36));
        let other_instance = request();

        assert_ne!(original, other_genesis);
        assert_ne!(original, other_instance);
        assert_ne!(other_genesis, other_instance);
    });
}
