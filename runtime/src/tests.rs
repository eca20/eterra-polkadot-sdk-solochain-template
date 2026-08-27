use super::*;

use codec::Encode;
use frame_support::{
    assert_noop, assert_ok,
    traits::{Contains, PalletInfoAccess},
};
use sp_core::{sr25519, Pair};
use sp_io::TestExternalities;
use sp_runtime::{
    generic::Era,
    traits::Header as _,
    transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidityError},
    BuildStorage,
};

fn account_id_from_pair(pair: &sr25519::Pair) -> AccountId {
    sp_runtime::AccountId32::from(pair.public())
}

#[test]
fn eterra_tcg_runtime_pallet_index_is_frozen_at_nine() {
    assert_eq!(<EterraTCG as PalletInfoAccess>::index(), 9);
}

#[test]
fn nexus_v2_new_pallet_indices_are_append_only() {
    assert_eq!(<EterraFlow as PalletInfoAccess>::index(), 29);
    assert_eq!(<EterraRandomness as PalletInfoAccess>::index(), 35);
    assert_eq!(<EterraCreatures as PalletInfoAccess>::index(), 36);
    assert_eq!(<EterraMagic as PalletInfoAccess>::index(), 37);
    assert_eq!(<EterraGameResults as PalletInfoAccess>::index(), 38);
    assert_eq!(<Utility as PalletInfoAccess>::index(), 39);
}

#[test]
fn legacy_escrow_classifier_uses_the_real_card_escrow_custodian() {
    use pallet_eterra_tcg::LegacyEscrowOwnerProvider as _;

    assert_eq!(
        configs::TcgLegacyEscrowOwnerProvider::custodian_account(),
        Some(EterraCardEscrow::account_id())
    );
}

#[test]
fn preserved_legacy_custody_paths_have_conservative_v16_weights() {
    use frame_support::dispatch::DispatchClass;
    use pallet_alpha_access::weights::WeightInfo as AlphaAccessWeightInfo;
    use pallet_cryptostrike::weights::WeightInfo as CryptoStrikeWeightInfo;
    use pallet_eterra_card_escrow::weights::WeightInfo as EscrowWeightInfo;
    use pallet_eterra_gamer::weights::WeightInfo as GamerWeightInfo;
    use pallet_eterra_seasons::weights::WeightInfo as SeasonsWeightInfo;
    use pallet_eterra_tcg::weights::WeightInfo as TcgWeightInfo;

    let convert = pallet_eterra_tcg::weights::SubstrateWeight::<Runtime>::convert_to_nft();
    let unwrap = pallet_eterra_tcg::weights::SubstrateWeight::<Runtime>::unwrap_from_nft();
    let wrapped_transfer =
        pallet_eterra_tcg::weights::SubstrateWeight::<Runtime>::transfer_wrapped_nft_v16();
    assert!(convert.ref_time() >= 1_200_000_000_000);
    assert!(convert.proof_size() >= 2_097_152);
    assert!(unwrap.ref_time() >= 1_200_000_000_000);
    assert!(unwrap.proof_size() >= 2_097_152);
    assert!(wrapped_transfer.ref_time() >= 1_200_000_000_000);
    assert!(wrapped_transfer.proof_size() >= 2_097_152);

    let deposit = pallet_eterra_card_escrow::weights::SubstrateWeight::<Runtime>::deposit_cards(1);
    let withdraw =
        pallet_eterra_card_escrow::weights::SubstrateWeight::<Runtime>::withdraw_cards(1);
    assert!(deposit.ref_time() >= 1_200_100_000_000);
    assert!(deposit.proof_size() >= 2_113_536);
    assert!(withdraw.ref_time() >= 1_200_100_000_000);
    assert!(withdraw.proof_size() >= 2_113_536);

    let grant = pallet_alpha_access::weights::SubstrateWeight::<Runtime>::grant_access();
    let steam_link = pallet_eterra_gamer::weights::SubstrateWeight::<Runtime>::link_steam();
    let settlement =
        pallet_cryptostrike::weights::SubstrateWeight::<Runtime>::submit_round_settlement();
    let finalize_pack =
        pallet_eterra_tcg::weights::SubstrateWeight::<Runtime>::finalize_v2_pack_open();
    let request_pack =
        pallet_eterra_tcg::weights::SubstrateWeight::<Runtime>::request_v2_pack_open();
    let allocate_pack_xp =
        pallet_eterra_gamer::weights::SubstrateWeight::<Runtime>::allocate_player_xp(16);
    let activate_season =
        pallet_eterra_seasons::weights::SubstrateWeight::<Runtime>::activate_season();
    assert!(grant.ref_time() >= 100_000_000_000);
    assert!(steam_link.ref_time() >= 300_000_000_000);
    assert!(settlement.ref_time() >= 1_200_000_000_000);
    assert!(settlement.proof_size() >= 2_000_000);
    assert!(finalize_pack.ref_time() >= 900_000_000_000);
    assert!(finalize_pack.proof_size() >= 16_777_216);
    assert!(request_pack.ref_time() >= 500_000_000_000);
    assert!(request_pack.proof_size() >= 16_777_216);
    assert!(allocate_pack_xp.ref_time() >= 370_000_000_000);
    assert!(allocate_pack_xp.proof_size() >= 1_114_112);
    assert!(activate_season.ref_time() >= 400_000_000_000);
    assert!(activate_season.proof_size() >= 8_388_608);

    let normal_max = configs::RuntimeBlockWeights::get()
        .get(DispatchClass::Normal)
        .max_extrinsic
        .expect("normal extrinsics have an explicit maximum");
    assert!(
        settlement.all_lte(normal_max),
        "the conservative settlement weight must remain dispatchable"
    );
    for (name, weight) in [
        ("convert", convert),
        ("unwrap", unwrap),
        ("wrapped_transfer", wrapped_transfer),
        ("single-card deposit", deposit),
        ("single-card withdraw", withdraw),
        ("tutorial pack request", request_pack),
        ("max PackTrack allocation", allocate_pack_xp),
        ("season activation", activate_season),
    ] {
        assert!(
            weight.all_lte(normal_max),
            "{name} conservative weight must remain dispatchable"
        );
    }
}

#[test]
fn session_authorization_ticket_matches_sdk_account_id32_golden_vector() {
    let entities = vec![
        pallet_eterra_game_results::AssetRevision {
            asset_id: 11,
            revision: 3,
        },
        pallet_eterra_game_results::AssetRevision {
            asset_id: 12,
            revision: 4,
        },
    ];
    let prisms = vec![pallet_eterra_game_results::AssetRevision {
        asset_id: 21,
        revision: 5,
    }];
    let charges = vec![
        pallet_eterra_game_results::ChargeUse {
            definition_id: 31,
            amount: 2,
        },
        pallet_eterra_game_results::ChargeUse {
            definition_id: 32,
            amount: 1,
        },
    ];
    let roster_root = EterraGameResults::session_roster_root(
        pallet_eterra_game_results::FPS_GAME_ID,
        1,
        pallet_eterra_game_results::ABILITY_DEATHMATCH_MODE_ID,
        1,
        eterra_nexus_primitives::EconomicRealm::Training,
        None,
        &entities,
        &prisms,
        &charges,
    );
    assert_eq!(
        roster_root,
        [
            0xaf, 0x28, 0xdf, 0x53, 0x77, 0x8f, 0x0e, 0x88, 0xc0, 0x47, 0x6d, 0x48, 0xb5, 0xf5,
            0x41, 0x0a, 0xd1, 0x53, 0xb0, 0x27, 0x46, 0x64, 0xb7, 0x43, 0x18, 0x17, 0x3e, 0x45,
            0x2b, 0xb2, 0x98, 0x3d,
        ]
    );

    let ticket = pallet_eterra_game_results::SessionAuthorizationTicket::<AccountId, BlockNumber> {
        protocol_version: 1,
        genesis_hash: [0x09; 32],
        pallet_instance_id: 38,
        authorization_id: [
            0x9c, 0xc4, 0x61, 0xe7, 0x31, 0xf2, 0xfb, 0x1c, 0x1f, 0x75, 0x6c, 0x91, 0x99, 0x11,
            0xbd, 0x65, 0xa4, 0x2e, 0x61, 0x0e, 0x1e, 0x2c, 0xb3, 0xf5, 0x93, 0x16, 0x82, 0xd7,
            0xa3, 0x86, 0x05, 0x4c,
        ],
        owner: AccountId::new([0x02; 32]),
        game_id: pallet_eterra_game_results::FPS_GAME_ID,
        game_version: 1,
        mode_id: pallet_eterra_game_results::ABILITY_DEATHMATCH_MODE_ID,
        policy_version: 1,
        policy_hash: [0x03; 32],
        authority_epoch: 7,
        authority_config_hash: [
            0xfb, 0x8a, 0xaf, 0x7b, 0xa6, 0x2c, 0xe6, 0x7c, 0xfd, 0x63, 0x93, 0x33, 0x0b, 0xdf,
            0xd0, 0xde, 0x96, 0x1e, 0xf6, 0xda, 0x1d, 0x89, 0xf2, 0xd0, 0x11, 0xad, 0x1b, 0x7c,
            0xd8, 0xd0, 0x26, 0x25,
        ],
        economic_realm: eterra_nexus_primitives::EconomicRealm::Training,
        cohort_hash: [0x05; 32],
        encounter_id: None,
        roster_root,
        expected_randomness_provenance:
            pallet_eterra_randomness::RandomnessMode::DeterministicPrivateAlpha,
        expires_at: 123_456,
    };
    let payload = (
        b"ETERRA_GAME_SESSION_AUTHORIZATION_V1".as_slice(),
        38u8,
        &ticket,
        entities.as_slice(),
        prisms.as_slice(),
        charges.as_slice(),
    )
        .encode();
    let to_hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    assert_eq!(
        to_hex(&payload),
        "904554455252415f47414d455f53455353494f4e5f415554484f52495a4154494f4e5f56312601000909090909090909090909090909090909090909090909090909090909090909269cc461e731f2fb1c1f756c919911bd65a42e610e1e2cb3f5931682d7a386054c0202020202020202020202020202020202020202020202020202020202020202ed030000010000000100000001000000030303030303030303030303030303030303030303030303030303030303030307000000fb8aaf7ba62ce67cfd6393330bdfd0de961ef6da1d89f2d011ad1b7cd8d0262500050505050505050505050505050505050505050505050505050505050505050500af28df53778f0e88c0476d48b5f5410ad153b0274664b74318173e452bb2983d0140e20100080b00000000000000030000000c000000000000000400000004150000000000000005000000081f000000020000002000000001000000"
    );
    assert_eq!(
        EterraGameResults::session_authorization_payload_hash(
            &ticket, &entities, &prisms, &charges,
        ),
        [
            0x27, 0xad, 0x16, 0xca, 0xe7, 0xd3, 0xfb, 0x38, 0xa2, 0x5a, 0x7a, 0x61, 0xca, 0x5d,
            0x40, 0xf4, 0x01, 0x17, 0xf6, 0x94, 0xf1, 0x4a, 0xdd, 0x4a, 0x31, 0x19, 0x69, 0xe4,
            0x34, 0xa3, 0xd0, 0xf5,
        ]
    );
}

#[test]
fn game_results_session_ticket_uses_runtime_sr25519_verifier() {
    let owner_pair = sr25519::Pair::from_string("//Alice", None).expect("dev key should parse");
    let owner = account_id_from_pair(&owner_pair);
    let authority_pair = sr25519::Pair::from_seed(&[0x77; 32]);

    new_test_ext_with_faucet(&owner, 1u128 << 60).execute_with(|| {
        System::set_block_number(1);
        pallet_alpha_access::AccessMode::<Runtime>::put(pallet_alpha_access::GateMode::Open);
        pallet_eterra_randomness::CurrentMode::<Runtime>::put(
            pallet_eterra_randomness::RandomnessMode::Disabled,
        );

        let policy = pallet_eterra_game_results::RewardPolicy {
            game_id: pallet_eterra_game_results::FPS_GAME_ID,
            game_version: 1,
            mode_id: pallet_eterra_game_results::ABILITY_DEATHMATCH_MODE_ID,
            policy_version: 1,
            mode_kind: eterra_nexus_primitives::GameModeKind::AbilityDeathmatch,
            economic_realm: eterra_nexus_primitives::EconomicRealm::Training,
            practice_only: true,
            normalized: false,
            loadout: pallet_eterra_game_results::PersistentLoadoutPolicy {
                entity_format: None,
                allowed_entity_roles_mask: 0,
                max_entities: 0,
                max_prisms: 0,
                max_charge_definitions: 0,
                max_total_charges: 0,
                max_magic_load: 0,
                rules_hash: [0x11; 32],
            },
            max_player_xp: 0,
            entity_xp: 0,
            base_essence: 0,
            essence_element: eterra_nexus_primitives::Element::Neutral,
            charge_definition_id: None,
            charge_drop_bps: 0,
            prism_definition_id: None,
            prism_drop_bps: 0,
            minimum_active_seconds: 0,
            maximum_afk_bps: 10_000,
            maximum_elapsed_seconds: 480,
            maximum_kills: 20,
            maximum_assists: 20,
            maximum_deaths: 20,
            maximum_damage: 20_000,
            maximum_objective_score: 5_000,
            maximum_outcome: 3,
            maximum_placement: 8,
            elimination_weight_bps: 0,
            participation_weight_bps: 0,
            objective_weight_bps: 0,
            maximum_xp_per_day: 0,
            repeat_cohort_multipliers_bps: [10_000, 7_500, 5_000, 2_500, 0],
            per_entity_encounter_rewards_per_day: 0,
            first_clear_markers_required: false,
            policy_hash: [0x22; 32],
        };
        assert_ok!(EterraGameResults::register_authority_epoch(
            RuntimeOrigin::root(),
            policy.game_id,
            policy.game_version,
            policy.mode_id,
            1,
            pallet_eterra_game_results::AuthorityEpoch {
                public_key: authority_pair.public().0,
                authority_config_hash: [0x33; 32],
                active_from: 1,
                active_until: 100,
                revoked: false,
            },
        ));
        assert_ok!(EterraGameResults::publish_reward_policy(
            RuntimeOrigin::root(),
            policy,
            pallet_eterra_game_results::RewardBudget::default(),
        ));
        assert_ok!(EterraGameResults::set_reward_policy_activation(
            RuntimeOrigin::root(),
            policy.key(),
            true,
        ));

        let entities = Vec::<pallet_eterra_game_results::AssetRevision>::new();
        let prisms = Vec::<pallet_eterra_game_results::AssetRevision>::new();
        let charges = Vec::<pallet_eterra_game_results::ChargeUse>::new();
        let roster_root = EterraGameResults::session_roster_root(
            policy.game_id,
            policy.game_version,
            policy.mode_id,
            policy.policy_version,
            policy.economic_realm,
            None,
            &entities,
            &prisms,
            &charges,
        );
        let ticket = pallet_eterra_game_results::SessionAuthorizationTicket {
            protocol_version: 1,
            genesis_hash: *System::block_hash(0).as_fixed_bytes(),
            pallet_instance_id: 38,
            authorization_id: [0x44; 32],
            owner: owner.clone(),
            game_id: policy.game_id,
            game_version: policy.game_version,
            mode_id: policy.mode_id,
            policy_version: policy.policy_version,
            policy_hash: policy.policy_hash,
            authority_epoch: 1,
            authority_config_hash: [0x33; 32],
            economic_realm: policy.economic_realm,
            cohort_hash: [0x55; 32],
            encounter_id: None,
            roster_root,
            expected_randomness_provenance: pallet_eterra_randomness::RandomnessMode::Disabled,
            expires_at: 50,
        };
        let payload_hash = EterraGameResults::session_authorization_payload_hash(
            &ticket, &entities, &prisms, &charges,
        );
        let signature = authority_pair.sign(&payload_hash).0.to_vec();
        assert_ok!(EterraGameResults::authorize_session_with_ticket(
            RuntimeOrigin::signed(owner.clone()),
            ticket.clone(),
            entities.clone(),
            prisms.clone(),
            charges.clone(),
            signature.clone(),
        ));
        let receipt = pallet_eterra_game_results::SessionAuthorizationReceipts::<Runtime>::get(
            ticket.authorization_id,
        )
        .expect("valid sr25519 ticket should create a replay receipt");
        assert_eq!(receipt.ticket_hash, payload_hash);
        assert_eq!(receipt.session_id, 1);

        let invalid_ticket = pallet_eterra_game_results::SessionAuthorizationTicket {
            authorization_id: [0x45; 32],
            cohort_hash: [0x56; 32],
            ..ticket
        };
        assert_noop!(
            EterraGameResults::authorize_session_with_ticket(
                RuntimeOrigin::signed(owner),
                invalid_ticket,
                entities,
                prisms,
                charges,
                signature,
            ),
            pallet_eterra_game_results::Error::<Runtime>::SessionAuthorizationSignatureInvalid
        );
    });
}

fn new_test_ext_with_faucet(faucet: &AccountId, faucet_balance: Balance) -> TestExternalities {
    let genesis = RuntimeGenesisConfig {
        balances: pallet_balances::GenesisConfig {
            balances: vec![(faucet.clone(), faucet_balance)],
        },
        eterra_faucet: pallet_eterra_faucet::GenesisConfig {
            faucet_account: Some(faucet.clone()),
            payout_amount: 1_000 * UNIT,
        },
        ..Default::default()
    };

    TestExternalities::new(
        genesis
            .build_storage()
            .expect("runtime genesis storage should build"),
    )
}

fn signed_extrinsic(
    call: RuntimeCall,
    signer: &sr25519::Pair,
    nonce: Nonce,
    genesis_hash: Hash,
    best_hash: Hash,
) -> UncheckedExtrinsic {
    // Keep this in sync with `node/src/benchmarking.rs` so we exercise the same SignedExtra set
    // that the node uses in production.
    let extra: SignedExtra = (
        frame_system::CheckNonZeroSender::<Runtime>::new(),
        frame_system::CheckSpecVersion::<Runtime>::new(),
        frame_system::CheckTxVersion::<Runtime>::new(),
        frame_system::CheckGenesis::<Runtime>::new(),
        frame_system::CheckEra::<Runtime>::from(Era::Immortal),
        CheckNonceWithFaucet::from(nonce),
        frame_system::CheckWeight::<Runtime>::new(),
        pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0),
        frame_metadata_hash_extension::CheckMetadataHash::<Runtime>::new(false),
    );

    let raw_payload = SignedPayload::from_raw(
        call.clone(),
        extra.clone(),
        (
            (),
            VERSION.spec_version,
            VERSION.transaction_version,
            genesis_hash,
            best_hash,
            (),
            (),
            (),
            None,
        ),
    );

    let signature = raw_payload.using_encoded(|e| signer.sign(e));

    UncheckedExtrinsic::new_signed(
        call,
        sp_runtime::AccountId32::from(signer.public()).into(),
        Signature::Sr25519(signature),
        extra,
    )
}

#[test]
fn faucet_claim_from_zero_balance_is_rejected_without_sponsorship() {
    let faucet_pair = sr25519::Pair::from_string("//Alice", None).expect("dev key should parse");
    let faucet = account_id_from_pair(&faucet_pair);

    let claimant_pair =
        sr25519::Pair::from_string("//Charlie", None).expect("dev key should parse");
    let claimant = account_id_from_pair(&claimant_pair);

    new_test_ext_with_faucet(&faucet, 1u128 << 60).execute_with(|| {
        System::set_block_number(1);
        let genesis_hash = System::block_hash(0);

        // Sanity: this account truly has no funds in our genesis.
        assert_eq!(Balances::free_balance(&claimant), 0);

        let call = RuntimeCall::EterraFaucet(pallet_eterra_faucet::Call::claim {
            dest: claimant.clone(),
        });
        let xt = signed_extrinsic(call, &claimant_pair, 0, genesis_hash, genesis_hash);

        let validity =
            Executive::validate_transaction(TransactionSource::External, xt.clone(), genesis_hash);
        assert_eq!(
            validity,
            Err(TransactionValidityError::Invalid(
                InvalidTransaction::Payment
            )),
            "disabled faucet claims must not receive the historical zero-balance exception"
        );

        let header = Header::new(
            1,
            Default::default(),
            Default::default(),
            genesis_hash,
            Default::default(),
        );
        Executive::initialize_block(&header);

        assert_eq!(
            Executive::apply_extrinsic(xt),
            Err(TransactionValidityError::Invalid(
                InvalidTransaction::Payment
            ))
        );
        assert_eq!(Balances::free_balance(&claimant), 0);
        assert_eq!(System::account(&claimant).nonce, 0);
        assert!(
            !System::events().iter().any(|record| matches!(
                record.event,
                RuntimeEvent::EterraFaucet(
                    pallet_eterra_faucet::Event::FeeSponsorshipApplied { .. }
                )
            )),
            "disabled faucet claims must never emit sponsorship evidence"
        );
    });
}

#[test]
fn non_faucet_call_from_zero_balance_is_rejected_for_payment() {
    let faucet_pair = sr25519::Pair::from_string("//Alice", None).expect("dev key should parse");
    let faucet = account_id_from_pair(&faucet_pair);

    let claimant_pair =
        sr25519::Pair::from_string("//Charlie", None).expect("dev key should parse");
    let claimant = account_id_from_pair(&claimant_pair);

    new_test_ext_with_faucet(&faucet, 1u128 << 60).execute_with(|| {
        System::set_block_number(1);
        let genesis_hash = System::block_hash(0);

        // Sanity: this account truly has no funds in our genesis.
        assert_eq!(Balances::free_balance(&claimant), 0);

        let call = RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
            dest: faucet.clone().into(),
            value: 1,
        });

        let xt = signed_extrinsic(call, &claimant_pair, 0, genesis_hash, genesis_hash);

        let validity =
            Executive::validate_transaction(TransactionSource::External, xt, genesis_hash);

        assert_eq!(
            validity,
            Err(TransactionValidityError::Invalid(
                InvalidTransaction::Payment
            )),
            "expected zero-balance account to be rejected for non-faucet call, got: {validity:?}"
        );
    });
}

#[test]
fn private_alpha_raw_asset_and_ticket_transfers_are_filtered() {
    let player_pair = sr25519::Pair::from_string("//Alice", None).expect("dev key should parse");
    let player = account_id_from_pair(&player_pair);
    let destination = account_id_from_pair(
        &sr25519::Pair::from_string("//Bob", None).expect("dev key should parse"),
    );

    new_test_ext_with_faucet(&player, 1u128 << 60).execute_with(|| {
        pallet_eterra_economy::TicketAsset::<Runtime>::put(
            pallet_eterra_economy::TicketAssetConfig {
                asset_id: 3,
                config_version: 1,
            },
        );

        let raw_transfer = RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: 3,
            target: destination.clone().into(),
            amount: 1,
        });
        let raw_approval = RuntimeCall::Assets(pallet_assets::Call::approve_transfer {
            id: 3,
            delegate: destination.clone().into(),
            amount: 1,
        });
        let non_ticket_transfer = RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: 2,
            target: destination.into(),
            amount: 1,
        });
        let economy_transfer =
            RuntimeCall::EterraEconomy(pallet_eterra_economy::Call::transfer_tickets {
                to: player,
                amount: 1,
            });

        assert!(!configs::EterraRuntimeCallFilter::contains(&raw_transfer));
        assert!(!configs::EterraRuntimeCallFilter::contains(&raw_approval));
        assert!(!configs::EterraRuntimeCallFilter::contains(
            &non_ticket_transfer
        ));
        assert!(!configs::EterraRuntimeCallFilter::contains(
            &economy_transfer
        ));
    });
}

#[test]
fn unbounded_public_node_authorization_calls_are_hard_disabled() {
    new_test_ext_with_faucet(&AccountId::from([1; 32]), 1u128 << 60).execute_with(|| {
        let node = sp_core::OpaquePeerId(vec![1, 2, 3]);
        let claim = RuntimeCall::NodeAuthorization(pallet_node_authorization::Call::claim_node {
            node: node.clone(),
        });
        let connections =
            RuntimeCall::NodeAuthorization(pallet_node_authorization::Call::add_connections {
                node: node.clone(),
                connections: vec![sp_core::OpaquePeerId(vec![4, 5, 6])],
            });
        let governed =
            RuntimeCall::NodeAuthorization(pallet_node_authorization::Call::add_well_known_node {
                node,
                owner: AccountId::from([2; 32]).into(),
            });

        assert!(!configs::EterraRuntimeCallFilter::contains(&claim));
        assert!(!configs::EterraRuntimeCallFilter::contains(&connections));
        assert!(configs::EterraRuntimeCallFilter::contains(&governed));

        pallet_eterra_economy::TicketAsset::<Runtime>::put(
            pallet_eterra_economy::TicketAssetConfig {
                asset_id: 3,
                config_version: 1,
            },
        );
        assert!(!configs::EterraRuntimeCallFilter::contains(&claim));
        assert!(!configs::EterraRuntimeCallFilter::contains(&connections));
    });
}

#[test]
fn unsafe_legacy_economy_and_raw_tcg_nft_mutations_are_hard_disabled() {
    new_test_ext_with_faucet(&AccountId::from([1; 32]), 1u128 << 60).execute_with(|| {
        pallet_eterra_tcg::CardNftCollectionId::<Runtime>::put(7);
        let owner = AccountId::from([1; 32]);
        let destination = AccountId::from([2; 32]);
        let raw_burn = RuntimeCall::Nfts(pallet_nfts::Call::burn {
            collection: 7,
            item: 9,
        });
        let raw_transfer = RuntimeCall::Nfts(pallet_nfts::Call::transfer {
            collection: 7,
            item: 9,
            dest: destination.clone().into(),
        });
        let wrapped_transfer =
            RuntimeCall::EterraTCG(pallet_eterra_tcg::Call::transfer_wrapped_card_nft_v16 {
                card_id: 9,
                new_owner: destination,
            });
        let legacy_settlement =
            RuntimeCall::CryptoStrike(pallet_cryptostrike::Call::claim_pending_guap {});
        let request_unstake =
            RuntimeCall::CryptoStrike(pallet_cryptostrike::Call::request_unstake { server_id: 1 });
        let finalize_unstake =
            RuntimeCall::CryptoStrike(pallet_cryptostrike::Call::finalize_unstake { server_id: 1 });
        let revoke_allowance =
            RuntimeCall::CryptoStrike(pallet_cryptostrike::Call::revoke_server_allowance {
                server_id: 1,
            });
        let legacy_reward = RuntimeCall::EterraCardEscrow(
            pallet_eterra_card_escrow::Call::record_enemy_elimination_with_event_id {
                game_id: 1,
                event_id: vec![1].try_into().expect("one byte fits"),
                victim: owner,
                card_id: 9,
            },
        );
        let lottery = RuntimeCall::EterraDailySlots(pallet_eterra_daily_slots::Call::roll {});

        assert!(!configs::EterraRuntimeCallFilter::contains(&raw_burn));
        assert!(!configs::EterraRuntimeCallFilter::contains(&raw_transfer));
        assert!(configs::EterraRuntimeCallFilter::contains(
            &wrapped_transfer
        ));
        assert!(!configs::EterraRuntimeCallFilter::contains(
            &legacy_settlement
        ));
        assert!(configs::EterraRuntimeCallFilter::contains(&request_unstake));
        assert!(configs::EterraRuntimeCallFilter::contains(
            &finalize_unstake
        ));
        assert!(configs::EterraRuntimeCallFilter::contains(
            &revoke_allowance
        ));
        assert!(!configs::EterraRuntimeCallFilter::contains(&legacy_reward));
        #[cfg(not(feature = "runtime-production"))]
        assert!(configs::EterraRuntimeCallFilter::contains(&lottery));
        #[cfg(feature = "runtime-production")]
        assert!(!configs::EterraRuntimeCallFilter::contains(&lottery));
    });
}

#[test]
fn training_calls_are_admitted_only_outside_production_while_paid_surfaces_stay_filtered() {
    new_test_ext_with_faucet(&AccountId::from([1; 32]), 1u128 << 60).execute_with(|| {
        let account = AccountId::from([2; 32]);
        let faucet = RuntimeCall::EterraFaucet(pallet_eterra_faucet::Call::claim {
            dest: account.clone(),
        });
        let list = RuntimeCall::EterraTCG(pallet_eterra_tcg::Call::set_price {
            card_id: 1,
            price: 10,
        });
        let buy = RuntimeCall::EterraTCG(pallet_eterra_tcg::Call::buy_card { card_id: 1 });
        let buy_capacity = RuntimeCall::EterraTCG(pallet_eterra_tcg::Call::buy_card_capacity {});
        let economy_faucet =
            RuntimeCall::EterraEconomy(pallet_eterra_economy::Call::claim_arcade_credit {});
        let economy_consume =
            RuntimeCall::EterraEconomy(pallet_eterra_economy::Call::consume_credit {
                game_id: 1_000,
                credit_type: 1,
                amount: 1,
            });
        let legacy_ticket_prize =
            RuntimeCall::EterraEconomy(pallet_eterra_economy::Call::redeem_prize_with_tickets {
                sku_id: 1,
                expected_version: 1,
            });
        let direct_core_start =
            RuntimeCall::EterraArcadeCore(pallet_eterra_arcade_core::Call::start_run {
                game_id: 1_006,
                ruleset_version: 1,
                client_run_id: vec![1].try_into().expect("one byte fits"),
                seed_commitment: Hash::default(),
            });
        let core_continue =
            RuntimeCall::EterraArcadeCore(pallet_eterra_arcade_core::Call::pay_continue {
                run_id: 1,
            });
        let wrapper_continue =
            RuntimeCall::EterraArcadeNovaRail(pallet_eterra_arcade_nova_rail::Call::pay_continue {
                run_id: 1,
            });
        let flow_create = RuntimeCall::EterraFlow(pallet_eterra_flow::Call::create_game {
            game_id: 1_000,
            metadata_hash: Hash::default(),
            metadata_uri: vec![].try_into().expect("empty URI fits"),
        });
        let media_create = RuntimeCall::EterraMedia(pallet_eterra_media::Call::create_collection {
            name: vec![].try_into().expect("empty name fits"),
            description: vec![].try_into().expect("empty description fits"),
        });
        let legacy_matchmaking = RuntimeCall::EterraSimpleMatchMaker(
            pallet_eterra_simple_matchmaker::Call::join_queue {},
        );
        let nft_create = RuntimeCall::Nfts(pallet_nfts::Call::create {
            admin: account.into(),
            config: Default::default(),
        });

        for call in [
            faucet,
            economy_faucet,
            direct_core_start,
            core_continue,
            wrapper_continue,
        ] {
            #[cfg(not(feature = "runtime-production"))]
            assert!(configs::EterraRuntimeCallFilter::contains(&call));
            #[cfg(feature = "runtime-production")]
            assert!(!configs::EterraRuntimeCallFilter::contains(&call));
        }

        for call in [
            list,
            buy,
            buy_capacity,
            economy_consume,
            legacy_ticket_prize,
            flow_create,
            media_create,
            legacy_matchmaking,
            nft_create,
        ] {
            assert!(!configs::EterraRuntimeCallFilter::contains(&call));
        }
    });
}

#[test]
fn flow_runtime_providers_cannot_mutate_economy_or_global_profile_state() {
    new_test_ext_with_faucet(&AccountId::from([1; 32]), 1u128 << 60).execute_with(|| {
        let account = AccountId::from([2; 32]);
        pallet_eterra_economy::Pallet::<Runtime>::try_grant_credit(&account, 1_000, 1, 5)
            .expect("test setup grants credit");
        pallet_eterra_economy::Pallet::<Runtime>::try_grant_entitlement(&account, 1_000, 7)
            .expect("test setup grants entitlement");
        pallet_eterra_profile::Pallet::<Runtime>::try_increment_counter(&account, 3, 11)
            .expect("test setup increments counter");
        pallet_eterra_profile::Pallet::<Runtime>::try_grant_badge(&account, 4)
            .expect("test setup grants badge");

        type FlowEconomy = configs::EterraFlowEconomyProvider;
        type FlowProfile = configs::EterraFlowProfileProvider;
        assert!(
            <FlowEconomy as pallet_eterra_flow::EconomyProvider<AccountId>>::grant_credit(
                &account, 1_000, 1, 99,
            )
            .is_err()
        );
        assert!(
            <FlowEconomy as pallet_eterra_flow::EconomyProvider<AccountId>>::consume_credit(
                &account, 1_000, 1, 1,
            )
            .is_err()
        );
        assert!(
            <FlowEconomy as pallet_eterra_flow::EconomyProvider<AccountId>>::revoke_entitlement(
                &account, 1_000, 7,
            )
            .is_err()
        );
        assert!(
            <FlowProfile as pallet_eterra_flow::ProfileProvider<AccountId>>::update_passport_counter(
                &account, 3, 9,
            )
            .is_err()
        );
        assert!(
            <FlowProfile as pallet_eterra_flow::ProfileProvider<AccountId>>::revoke_passport_badge(
                &account, 4,
            )
            .is_err()
        );

        assert_eq!(EterraEconomy::credit_balance(&account, 1_000, 1), 5);
        assert!(EterraEconomy::has_entitlement(&account, 1_000, 7));
        assert_eq!(
            pallet_eterra_profile::PassportCounters::<Runtime>::get(&account, 3),
            11
        );
        assert!(pallet_eterra_profile::Badges::<Runtime>::get(&account, 4));
    });
}

#[test]
fn game_authority_creation_rolls_back_while_v16_legacy_writes_are_paused() {
    new_test_ext_with_faucet(&AccountId::from([1; 32]), 1u128 << 60).execute_with(|| {
        let server = AccountId::from([1; 32]);
        let player = AccountId::from([2; 32]);
        assert_ok!(EterraGameAuthority::add_server(
            RuntimeOrigin::root(),
            server.clone(),
        ));
        pallet_eterra_tcg::LegacyWritesPausedV16::<Runtime>::put(true);

        assert_noop!(
            EterraGameAuthority::create_game_with_round_id(
                RuntimeOrigin::signed(server),
                b"paused-round".to_vec().try_into().expect("round ID fits"),
                vec![player].try_into().expect("one player fits"),
            ),
            sp_runtime::DispatchError::Other(
                "legacy game creation is paused during TCG V16 migration"
            )
        );
        assert!(pallet_eterra_game_authority::Games::<Runtime>::iter()
            .next()
            .is_none());
        assert!(
            pallet_eterra_game_authority::ActiveGameByPlayer::<Runtime>::iter()
                .next()
                .is_none()
        );
        assert!(pallet_eterra_game_authority::Expirations::<Runtime>::iter()
            .next()
            .is_none());
        assert!(
            pallet_eterra_card_escrow::GameEnemyAssignments::<Runtime>::iter()
                .next()
                .is_none()
        );
    });
}

#[test]
fn unity_prize_counter_ticket_redemption_issues_one_training_pack_credit() {
    let player_pair = sr25519::Pair::from_string("//Alice", None).expect("dev key should parse");
    let player = account_id_from_pair(&player_pair);
    let other = account_id_from_pair(
        &sr25519::Pair::from_string("//Bob", None).expect("dev key should parse"),
    );

    new_test_ext_with_faucet(&player, 1u128 << 60).execute_with(|| {
        System::set_block_number(1);
        pallet_alpha_access::AccessMode::<Runtime>::put(pallet_alpha_access::GateMode::Open);
        assert_ok!(Assets::force_create(
            RuntimeOrigin::root(),
            3,
            player.clone().into(),
            true,
            1,
        ));
        assert_ok!(Assets::mint(
            RuntimeOrigin::signed(player.clone()),
            3,
            player.clone().into(),
            100,
        ));
        assert_ok!(EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 1,));

        pallet_eterra_tcg::PackSkuVersionsV2::<Runtime>::insert(
            (1, 1),
            eterra_nexus_primitives::PackSkuVersion {
                pack_sku: 1,
                version: 1,
                card_count: eterra_nexus_primitives::PACK_CARD_COUNT,
                set_id: 1,
                pool_id: 1,
                pool_version: 1,
                rarity_weights: [6_800, 2_200, 750, 200, 50],
                discovery_policy: eterra_nexus_primitives::DiscoveryPolicy::Earned,
                odds_metadata_hash: [1u8; 32],
                immutable_config_hash: [2u8; 32],
                active_from: 1,
                active_until: None,
            },
        );
        assert_ok!(EterraEconomy::upsert_arcade_pack_credit_sku_v2(
            RuntimeOrigin::root(),
            7_001,
            pallet_eterra_economy::ArcadePackCreditSkuV2 {
                pack_sku: 1,
                pack_sku_version: 1,
                economic_realm: eterra_nexus_primitives::EconomicRealm::Training,
                ticket_price: 20,
                policy_version: 7,
                enabled: true,
                total_cap: Some(10),
                per_account_window_cap: 2,
                window_blocks: 100,
                config_version: 1,
            },
        ));
        assert_ok!(EterraEconomy::set_arcade_economy_pause(
            RuntimeOrigin::root(),
            pallet_eterra_economy::PauseDomain::PackCreditRedemptionV2,
            false,
        ));

        let redemption_id = [0xA7; 32];
        assert_ok!(EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
            RuntimeOrigin::signed(player.clone()),
            7_001,
            1,
            redemption_id,
        ));
        assert_eq!(EterraEconomy::ticket_balance(&player), 80);
        let credit_ids = pallet_eterra_tcg::AvailablePackCreditIdsV2::<Runtime>::get(
            &player,
            (1, 1, eterra_nexus_primitives::EconomicRealm::Training),
        );
        assert_eq!(credit_ids.as_slice(), &[1]);
        let credit = pallet_eterra_tcg::PackCreditsV2::<Runtime>::get(1)
            .expect("the bridge issued a Pack Credit");
        assert_eq!(credit.owner, player);
        assert_eq!(
            credit.source,
            eterra_nexus_primitives::PackCreditSource::ArcadePrize {
                policy_version: 7,
                redemption_id,
            }
        );

        // Finalized exact retries remain no-ops, while the same globally
        // reserved ID cannot be rebound to another account.
        assert_ok!(EterraEconomy::set_arcade_economy_pause(
            RuntimeOrigin::root(),
            pallet_eterra_economy::PauseDomain::PackCreditRedemptionV2,
            true,
        ));
        assert_ok!(EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
            RuntimeOrigin::signed(player),
            7_001,
            1,
            redemption_id,
        ));
        assert_noop!(
            EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                RuntimeOrigin::signed(other),
                7_001,
                1,
                redemption_id,
            ),
            pallet_eterra_economy::Error::<Runtime>::ArcadePackCreditRedemptionConflict
        );
        assert_eq!(pallet_eterra_tcg::NextPackCreditIdV2::<Runtime>::get(), 2);
    });
}
