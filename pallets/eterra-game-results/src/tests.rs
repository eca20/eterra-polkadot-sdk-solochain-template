use crate::{
    mock::*, AssetRevision, AuthorityEpoch, ChargeUse, DeterministicPrismQuestPolicy, Error,
    FpsMatchResultV1, PersistentLoadoutPolicy, ResultBodyV1, RewardBudget, RewardBudgets,
    RewardPolicy, RpgBattleResultV1, SessionAuthorizationTicket, SessionStatus, Sessions,
    SignedResultV1, ABILITY_DEATHMATCH_MODE_ID, FPS_GAME_ID, LEGENDS_GAME_ID,
    NORMALIZED_LEGACY_MODE_ID,
};
use codec::Encode;
use eterra_nexus_primitives::{EconomicRealm, Element, GameModeKind, ResultHeaderV1};
use frame_support::{assert_noop, assert_ok, BoundedVec};
use pallet_eterra_randomness::RandomnessMode;

fn result_id_for(session_id: u64, fill: u8) -> [u8; 32] {
    let mut result_id = [fill; 32];
    result_id[..8].copy_from_slice(&session_id.to_le_bytes());
    result_id
}

fn policy(normalized: bool) -> RewardPolicy {
    let loadout = if normalized {
        PersistentLoadoutPolicy {
            entity_format: None,
            allowed_entity_roles_mask: 0,
            max_entities: 0,
            max_prisms: 0,
            max_charge_definitions: 0,
            max_total_charges: 0,
            max_magic_load: 0,
            rules_hash: [2; 32],
        }
    } else {
        PersistentLoadoutPolicy {
            entity_format: Some((1, 1)),
            allowed_entity_roles_mask: 0x7f,
            max_entities: 1,
            max_prisms: 2,
            max_charge_definitions: 2,
            max_total_charges: 4,
            max_magic_load: 8,
            rules_hash: [2; 32],
        }
    };
    RewardPolicy {
        game_id: FPS_GAME_ID,
        game_version: 1,
        mode_id: if normalized {
            NORMALIZED_LEGACY_MODE_ID
        } else {
            ABILITY_DEATHMATCH_MODE_ID
        },
        policy_version: 1,
        mode_kind: if normalized {
            GameModeKind::NormalizedLegacy
        } else {
            GameModeKind::AbilityDeathmatch
        },
        economic_realm: EconomicRealm::Training,
        practice_only: false,
        normalized,
        loadout,
        max_player_xp: 500,
        entity_xp: 0,
        base_essence: 0,
        essence_element: Element::Neutral,
        charge_definition_id: None,
        charge_drop_bps: 0,
        prism_definition_id: None,
        prism_drop_bps: 0,
        minimum_active_seconds: 60,
        maximum_afk_bps: 2_000,
        maximum_elapsed_seconds: 480,
        maximum_kills: 20,
        maximum_assists: 20,
        maximum_deaths: 20,
        maximum_damage: 20_000,
        maximum_objective_score: 5_000,
        maximum_outcome: 3,
        maximum_placement: 8,
        elimination_weight_bps: 2_000,
        participation_weight_bps: 3_000,
        objective_weight_bps: 5_000,
        maximum_xp_per_day: 3_600,
        repeat_cohort_multipliers_bps: [10_000, 7_500, 5_000, 2_500, 0],
        per_entity_encounter_rewards_per_day: 0,
        first_clear_markers_required: false,
        policy_hash: [3; 32],
    }
}

fn production_fps_policy() -> RewardPolicy {
    let mut production = policy(false);
    production.economic_realm = EconomicRealm::Production;
    production.max_player_xp = crate::PRODUCTION_MAX_PLAYER_XP_V1;
    production.maximum_xp_per_day = crate::PRODUCTION_MAX_XP_PER_DAY_V1;
    production.minimum_active_seconds = crate::PRODUCTION_MIN_ACTIVE_SECONDS_V1;
    production.maximum_afk_bps = crate::PRODUCTION_MAX_AFK_BPS_V1;
    production.elimination_weight_bps = crate::PRODUCTION_REWARD_WEIGHTS_BPS_V1[0];
    production.participation_weight_bps = crate::PRODUCTION_REWARD_WEIGHTS_BPS_V1[1];
    production.objective_weight_bps = crate::PRODUCTION_REWARD_WEIGHTS_BPS_V1[2];
    production.repeat_cohort_multipliers_bps = crate::PRODUCTION_REPEAT_COHORT_MULTIPLIERS_BPS_V1;
    production
}

fn budget() -> RewardBudget {
    RewardBudget {
        xp_total: 10_000,
        essence_total: 0,
        charge_slots_total: 0,
        prism_slots_total: 0,
        ..Default::default()
    }
}

fn configure_with_policy(policy: RewardPolicy) {
    let mode_id = policy.mode_id;
    assert_ok!(GameResults::register_authority_epoch(
        RuntimeOrigin::root(),
        FPS_GAME_ID,
        1,
        mode_id,
        1,
        AuthorityEpoch {
            public_key: [1; 32],
            authority_config_hash: [2; 32],
            active_from: 1,
            active_until: 1_000,
            revoked: false,
        }
    ));
    assert_ok!(GameResults::publish_reward_policy(
        RuntimeOrigin::root(),
        policy,
        budget()
    ));
    assert_ok!(GameResults::set_reward_policy_activation(
        RuntimeOrigin::root(),
        (FPS_GAME_ID, 1, mode_id, 1),
        true
    ));
}

fn configure(normalized: bool) {
    configure_with_policy(policy(normalized));
}

#[allow(clippy::too_many_arguments)]
fn authorize_session_with_authority_ticket(
    origin: RuntimeOrigin,
    game_id: u32,
    game_version: u32,
    mode_id: u32,
    policy_version: u32,
    authority_epoch: u32,
    economic_realm: EconomicRealm,
    cohort_hash: [u8; 32],
    encounter_id: Option<u32>,
    entities: Vec<AssetRevision>,
    prisms: Vec<AssetRevision>,
    charges: Vec<ChargeUse>,
    expires_at: u64,
) -> frame_support::dispatch::DispatchResult {
    let owner = frame_system::ensure_signed(origin)?;
    let policy_key = (game_id, game_version, mode_id, policy_version);
    let reward_policy =
        crate::RewardPolicies::<Test>::get(policy_key).ok_or(Error::<Test>::PolicyMissing)?;
    let authority =
        crate::AuthorityEpochs::<Test>::get((game_id, game_version, mode_id, authority_epoch))
            .ok_or(Error::<Test>::AuthorityMissing)?;
    let roster_root = GameResults::session_roster_root(
        game_id,
        game_version,
        mode_id,
        policy_version,
        economic_realm,
        encounter_id,
        &entities,
        &prisms,
        &charges,
    );
    let authorization_id = sp_io::hashing::blake2_256(
        &(
            b"ETERRA_TEST_SESSION_AUTHORIZATION".as_slice(),
            crate::NextSessionId::<Test>::get().saturating_add(1),
            owner,
            game_id,
            game_version,
            mode_id,
            policy_version,
            cohort_hash,
            encounter_id,
            roster_root,
            expires_at,
        )
            .encode(),
    );
    let ticket = SessionAuthorizationTicket {
        protocol_version: 1,
        genesis_hash: GenesisHash::get(),
        pallet_instance_id: PalletInstanceId::get(),
        authorization_id,
        owner,
        game_id,
        game_version,
        mode_id,
        policy_version,
        policy_hash: reward_policy.policy_hash,
        authority_epoch,
        authority_config_hash: authority.authority_config_hash,
        economic_realm,
        cohort_hash,
        encounter_id,
        roster_root,
        expected_randomness_provenance: RANDOMNESS_MODE.with(|mode| mode.get()),
        expires_at,
    };
    let payload_hash =
        GameResults::session_authorization_payload_hash(&ticket, &entities, &prisms, &charges);
    GameResults::authorize_session_with_ticket(
        RuntimeOrigin::signed(owner),
        ticket,
        entities,
        prisms,
        charges,
        payload_hash.to_vec(),
    )
}

fn default_fps_authorization_ticket(
    owner: u64,
    authorization_id: [u8; 32],
    cohort_hash: [u8; 32],
    expires_at: u64,
    entities: &[AssetRevision],
    prisms: &[AssetRevision],
    charges: &[ChargeUse],
) -> crate::pallet::SessionAuthorizationTicketOf<Test> {
    let reward_policy =
        crate::RewardPolicies::<Test>::get((FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1))
            .expect("configured reward policy");
    let authority =
        crate::AuthorityEpochs::<Test>::get((FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1))
            .expect("configured authority");
    SessionAuthorizationTicket {
        protocol_version: 1,
        genesis_hash: GenesisHash::get(),
        pallet_instance_id: PalletInstanceId::get(),
        authorization_id,
        owner,
        game_id: FPS_GAME_ID,
        game_version: 1,
        mode_id: ABILITY_DEATHMATCH_MODE_ID,
        policy_version: 1,
        policy_hash: reward_policy.policy_hash,
        authority_epoch: 1,
        authority_config_hash: authority.authority_config_hash,
        economic_realm: EconomicRealm::Training,
        cohort_hash,
        encounter_id: None,
        roster_root: GameResults::session_roster_root(
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            EconomicRealm::Training,
            None,
            entities,
            prisms,
            charges,
        ),
        expected_randomness_provenance: RANDOMNESS_MODE.with(|mode| mode.get()),
        expires_at,
    }
}

fn submit_authorization_ticket(
    ticket: crate::pallet::SessionAuthorizationTicketOf<Test>,
    entities: Vec<AssetRevision>,
    prisms: Vec<AssetRevision>,
    charges: Vec<ChargeUse>,
) -> frame_support::dispatch::DispatchResult {
    let payload_hash =
        GameResults::session_authorization_payload_hash(&ticket, &entities, &prisms, &charges);
    GameResults::authorize_session_with_ticket(
        RuntimeOrigin::signed(ticket.owner),
        ticket,
        entities,
        prisms,
        charges,
        payload_hash.to_vec(),
    )
}

fn authorize_fps(owner: u64, cohort_hash: [u8; 32], expires_at: u64) {
    assert_ok!(authorize_session_with_authority_ticket(
        RuntimeOrigin::signed(owner),
        FPS_GAME_ID,
        1,
        ABILITY_DEATHMATCH_MODE_ID,
        1,
        1,
        EconomicRealm::Training,
        cohort_hash,
        None,
        vec![],
        vec![],
        vec![],
        expires_at,
    ));
}

fn fps_body(
    account: u64,
    cohort_hash: [u8; 32],
    kills: u16,
) -> ResultBodyV1<
    u64,
    BoundedVec<u64, MaxSessionEntities>,
    BoundedVec<ChargeUse, MaxChargeDefinitions>,
    BoundedVec<u64, MaxSessionPrisms>,
> {
    ResultBodyV1::FpsMatch(FpsMatchResultV1 {
        account,
        cohort_hash,
        active_seconds: 400,
        afk_seconds: 20,
        kills,
        deaths: 3,
        assists: 4,
        damage: 4_000,
        objective_score: 200,
        outcome: 1,
        placement: 1,
        used_charges: BoundedVec::default(),
        used_prisms: BoundedVec::default(),
    })
}

fn signed_result_for(
    session_id: u64,
    result_fill: u8,
    kills: u16,
) -> crate::pallet::ResultOf<Test> {
    let session = Sessions::<Test>::get(session_id).expect("authorized test session");
    let result_id = result_id_for(session_id, result_fill);
    let header = ResultHeaderV1 {
        protocol_version: 1,
        genesis_hash: [9; 32],
        game_id: session.game_id,
        game_version: session.game_version,
        mode_id: session.mode_id,
        policy_version: session.policy_version,
        session_id,
        result_id,
        authority_epoch: session.authority_epoch,
        roster_root: session.roster_root,
        expires_at: session.expires_at,
        telemetry_root: [4; 32],
    };
    let body = fps_body(session.owner, session.cohort_hash, kills);
    let payload_hash = GameResults::result_payload_hash(&header, &body);
    SignedResultV1 {
        header,
        body,
        server_signature: payload_hash.to_vec().try_into().unwrap(),
    }
}

fn signed_result(result_id: [u8; 32], kills: u16) -> crate::pallet::ResultOf<Test> {
    signed_result_for(1, result_id[31], kills)
}

fn legends_policy() -> RewardPolicy {
    RewardPolicy {
        game_id: LEGENDS_GAME_ID,
        game_version: 1,
        mode_id: 1,
        policy_version: 1,
        mode_kind: GameModeKind::Legends,
        economic_realm: EconomicRealm::Training,
        practice_only: false,
        normalized: false,
        loadout: PersistentLoadoutPolicy {
            entity_format: Some((1, 1)),
            allowed_entity_roles_mask: 0x7f,
            max_entities: 3,
            max_prisms: 2,
            max_charge_definitions: 2,
            max_total_charges: 4,
            max_magic_load: 8,
            rules_hash: [9; 32],
        },
        max_player_xp: 0,
        entity_xp: 100,
        base_essence: 5,
        essence_element: Element::Fire,
        charge_definition_id: Some(10),
        charge_drop_bps: 2_500,
        prism_definition_id: Some(20),
        prism_drop_bps: 500,
        minimum_active_seconds: 0,
        maximum_afk_bps: 10_000,
        maximum_elapsed_seconds: 600,
        maximum_kills: 0,
        maximum_assists: 0,
        maximum_deaths: 0,
        maximum_damage: 10_000,
        maximum_objective_score: 0,
        maximum_outcome: 0,
        maximum_placement: 0,
        elimination_weight_bps: 0,
        participation_weight_bps: 0,
        objective_weight_bps: 0,
        maximum_xp_per_day: 0,
        repeat_cohort_multipliers_bps: [10_000, 7_500, 5_000, 2_500, 0],
        per_entity_encounter_rewards_per_day: 6,
        first_clear_markers_required: true,
        policy_hash: [8; 32],
    }
}

fn configure_legends_with_policy(policy: RewardPolicy) {
    assert_ok!(GameResults::register_authority_epoch(
        RuntimeOrigin::root(),
        LEGENDS_GAME_ID,
        1,
        1,
        1,
        AuthorityEpoch {
            public_key: [1; 32],
            authority_config_hash: [2; 32],
            active_from: 1,
            active_until: 1_000,
            revoked: false,
        }
    ));
    assert_ok!(GameResults::publish_reward_policy(
        RuntimeOrigin::root(),
        policy,
        RewardBudget {
            xp_total: 10_000,
            essence_total: 1_000,
            charge_slots_total: 10,
            prism_slots_total: 10,
            ..Default::default()
        }
    ));
    assert_ok!(GameResults::set_reward_policy_activation(
        RuntimeOrigin::root(),
        (LEGENDS_GAME_ID, 1, 1, 1),
        true
    ));
}

fn configure_legends() {
    configure_legends_with_policy(legends_policy());
}

#[test]
fn reward_policy_rejects_incoherent_or_missing_drop_definitions() {
    new_test_ext().execute_with(|| {
        let mut incoherent = legends_policy();
        incoherent.charge_definition_id = None;
        assert_noop!(
            GameResults::publish_reward_policy(RuntimeOrigin::root(), incoherent, budget()),
            Error::<Test>::InvalidPolicy
        );

        let mut cross_mode = policy(false);
        cross_mode.charge_definition_id = Some(10);
        cross_mode.charge_drop_bps = 100;
        assert_noop!(
            GameResults::publish_reward_policy(RuntimeOrigin::root(), cross_mode, budget()),
            Error::<Test>::InvalidPolicy
        );

        let mut missing = legends_policy();
        missing.charge_definition_id = Some(999);
        assert_noop!(
            GameResults::publish_reward_policy(RuntimeOrigin::root(), missing, budget()),
            Error::<Test>::RewardDefinitionMissing
        );
    });
}

#[test]
fn essence_only_legends_rewards_are_still_encounter_rate_limited() {
    new_test_ext().execute_with(|| {
        let mut essence_only = legends_policy();
        essence_only.entity_xp = 0;
        essence_only.charge_definition_id = None;
        essence_only.charge_drop_bps = 0;
        essence_only.prism_definition_id = None;
        essence_only.prism_drop_bps = 0;
        essence_only.per_entity_encounter_rewards_per_day = 1;
        configure_legends_with_policy(essence_only);

        authorize_legends(1, 7, 44, 50);
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_legends_result_for(1, 41, true),
        ));
        assert!(crate::FirstClearMarkers::<Test>::contains_key(
            1,
            (EconomicRealm::Training, LEGENDS_GAME_ID, 44)
        ));

        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(1),
                LEGENDS_GAME_ID,
                1,
                1,
                1,
                1,
                EconomicRealm::Training,
                [22; 32],
                Some(44),
                vec![AssetRevision {
                    asset_id: 7,
                    revision: 2,
                }],
                vec![],
                vec![],
                60,
            ),
            Error::<Test>::AntiFarmLimitReached
        );
    });
}

fn authorize_legends(owner: u64, entity_id: u64, encounter_id: u32, expires_at: u64) {
    assert_ok!(authorize_session_with_authority_ticket(
        RuntimeOrigin::signed(owner),
        LEGENDS_GAME_ID,
        1,
        1,
        1,
        1,
        EconomicRealm::Training,
        [22; 32],
        Some(encounter_id),
        vec![AssetRevision {
            asset_id: entity_id,
            revision: 1,
        }],
        vec![],
        vec![],
        expires_at,
    ));
}

fn signed_legends_result_with_outcome(
    result_id: [u8; 32],
    owner_won: bool,
) -> crate::pallet::ResultOf<Test> {
    signed_legends_result_for(1, result_id[31], owner_won)
}

fn signed_legends_result_for(
    session_id: u64,
    result_fill: u8,
    owner_won: bool,
) -> crate::pallet::ResultOf<Test> {
    let session = Sessions::<Test>::get(session_id).expect("authorized Legends test session");
    let result_id = result_id_for(session_id, result_fill);
    let header = ResultHeaderV1 {
        protocol_version: 1,
        genesis_hash: [9; 32],
        game_id: session.game_id,
        game_version: session.game_version,
        mode_id: session.mode_id,
        policy_version: session.policy_version,
        session_id,
        result_id,
        authority_epoch: session.authority_epoch,
        roster_root: session.roster_root,
        expires_at: session.expires_at,
        telemetry_root: [4; 32],
    };
    let body = ResultBodyV1::RpgBattle(RpgBattleResultV1 {
        owner_won,
        encounter_id: session.encounter_id.expect("Legends encounter"),
        entity_ids: session
            .entities
            .iter()
            .map(|asset| asset.asset_id)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
        elapsed_seconds: 180,
        turn_count: 12,
        combat_metric: 1_000,
        transcript_hash: [6; 32],
    });
    let payload_hash = GameResults::result_payload_hash(&header, &body);
    SignedResultV1 {
        header,
        body,
        server_signature: payload_hash.to_vec().try_into().unwrap(),
    }
}

fn signed_legends_result(result_id: [u8; 32]) -> crate::pallet::ResultOf<Test> {
    signed_legends_result_with_outcome(result_id, true)
}

#[test]
fn signed_result_settles_once_and_grants_runtime_derived_xp() {
    new_test_ext().execute_with(|| {
        configure(false);
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(1),
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            1,
            EconomicRealm::Training,
            [12; 32],
            None,
            vec![],
            vec![],
            vec![],
            50
        ));
        let result = signed_result([5; 32], 10);
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            result.clone()
        ));
        assert_eq!(
            Sessions::<Test>::get(1).unwrap().status,
            SessionStatus::Settled
        );
        PLAYER_XP.with(|xp| {
            let values = xp.borrow();
            assert_eq!(values.len(), 1);
            assert_eq!(values[0].0, 1);
            assert!(values[0].2 > 0);
        });
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(98),
            result
        ));
        PLAYER_XP.with(|xp| assert_eq!(xp.borrow().len(), 1));
    });
}

#[test]
fn practice_only_results_settle_without_creating_or_spending_xp() {
    new_test_ext().execute_with(|| {
        assert_ok!(GameResults::register_authority_epoch(
            RuntimeOrigin::root(),
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            AuthorityEpoch {
                public_key: [1; 32],
                authority_config_hash: [2; 32],
                active_from: 1,
                active_until: 1_000,
                revoked: false,
            }
        ));
        let mut practice_policy = policy(false);
        practice_policy.practice_only = true;
        assert_ok!(GameResults::publish_reward_policy(
            RuntimeOrigin::root(),
            practice_policy,
            budget()
        ));
        assert_ok!(GameResults::set_reward_policy_activation(
            RuntimeOrigin::root(),
            (FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1),
            true
        ));
        assert_ok!(GameResults::authorize_session(
            RuntimeOrigin::signed(1),
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            1,
            EconomicRealm::Training,
            [12; 32],
            None,
            vec![],
            vec![],
            vec![],
            50
        ));

        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_result([6; 32], 20)
        ));

        PLAYER_XP.with(|xp| assert!(xp.borrow().is_empty()));
        let remaining =
            RewardBudgets::<Test>::get((FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1)).unwrap();
        assert_eq!(remaining.xp_reserved, 0);
        assert_eq!(remaining.xp_spent, 0);
        assert_eq!(
            Sessions::<Test>::get(1).unwrap().status,
            SessionStatus::Settled
        );
    });
}

#[test]
fn legacy_session_authorization_is_training_practice_only() {
    new_test_ext().execute_with(|| {
        configure(false);
        assert_noop!(
            GameResults::authorize_session(
                RuntimeOrigin::signed(1),
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
                1,
                EconomicRealm::Training,
                [12; 32],
                None,
                vec![],
                vec![],
                vec![],
                50,
            ),
            Error::<Test>::LegacySessionRequiresPractice
        );
        assert_eq!(crate::NextSessionId::<Test>::get(), 0);
        assert_eq!(
            RewardBudgets::<Test>::get((FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1))
                .expect("configured budget")
                .xp_reserved,
            0
        );
    });
}

#[test]
fn authority_ticket_is_single_use_idempotent_and_conflict_safe() {
    new_test_ext().execute_with(|| {
        configure(false);
        let ticket = default_fps_authorization_ticket(1, [41; 32], [12; 32], 50, &[], &[], &[]);
        assert_ok!(submit_authorization_ticket(
            ticket.clone(),
            vec![],
            vec![],
            vec![]
        ));
        let reserved = RewardBudgets::<Test>::get((FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1))
            .expect("budget remains");
        assert_eq!(reserved.xp_reserved, 500);
        assert_eq!(crate::NextSessionId::<Test>::get(), 1);
        assert_eq!(crate::ActiveSessionCount::<Test>::get(1), 1);
        assert_eq!(
            crate::ActiveSessionCountByAuthority::<Test>::get((
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
            )),
            1
        );

        // Exact replay returns the original receipt without reserving,
        // locking, or incrementing anything again.
        assert_ok!(GameResults::authorize_session_with_ticket(
            RuntimeOrigin::signed(1),
            ticket.clone(),
            vec![],
            vec![],
            vec![],
            vec![],
        ));
        assert_eq!(crate::NextSessionId::<Test>::get(), 1);
        assert_eq!(
            RewardBudgets::<Test>::get((FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1,))
                .expect("budget remains")
                .xp_reserved,
            500
        );

        let conflicting = SessionAuthorizationTicket {
            cohort_hash: [13; 32],
            ..ticket
        };
        assert_noop!(
            submit_authorization_ticket(conflicting, vec![], vec![], vec![]),
            Error::<Test>::SessionAuthorizationTicketConflict
        );
    });
}

#[test]
fn authority_ticket_binds_chain_policy_authority_roster_and_signature() {
    new_test_ext().execute_with(|| {
        configure(false);

        let mut wrong_chain =
            default_fps_authorization_ticket(1, [42; 32], [12; 32], 50, &[], &[], &[]);
        wrong_chain.genesis_hash = [8; 32];
        assert_noop!(
            submit_authorization_ticket(wrong_chain, vec![], vec![], vec![]),
            Error::<Test>::SessionAuthorizationTicketInvalid
        );

        let mut wrong_policy =
            default_fps_authorization_ticket(1, [43; 32], [12; 32], 50, &[], &[], &[]);
        wrong_policy.policy_hash = [99; 32];
        assert_noop!(
            submit_authorization_ticket(wrong_policy, vec![], vec![], vec![]),
            Error::<Test>::SessionAuthorizationTicketInvalid
        );

        let mut wrong_authority =
            default_fps_authorization_ticket(1, [44; 32], [12; 32], 50, &[], &[], &[]);
        wrong_authority.authority_config_hash = [98; 32];
        assert_noop!(
            submit_authorization_ticket(wrong_authority, vec![], vec![], vec![]),
            Error::<Test>::SessionAuthorizationTicketInvalid
        );

        let entity = AssetRevision {
            asset_id: 7,
            revision: 1,
        };
        let ticket =
            default_fps_authorization_ticket(1, [45; 32], [12; 32], 50, &[entity], &[], &[]);
        assert_noop!(
            submit_authorization_ticket(ticket.clone(), vec![], vec![], vec![]),
            Error::<Test>::SessionAuthorizationTicketInvalid
        );

        assert_noop!(
            GameResults::authorize_session_with_ticket(
                RuntimeOrigin::signed(1),
                ticket,
                vec![entity],
                vec![],
                vec![],
                vec![1; 32],
            ),
            Error::<Test>::SessionAuthorizationSignatureInvalid
        );
        assert_eq!(crate::NextSessionId::<Test>::get(), 0);
    });
}

#[test]
fn authority_ticket_rejects_invalid_owner_expiry_and_randomness() {
    new_test_ext().execute_with(|| {
        configure(false);

        let wrong_owner =
            default_fps_authorization_ticket(1, [46; 32], [12; 32], 50, &[], &[], &[]);
        let wrong_owner_hash =
            GameResults::session_authorization_payload_hash(&wrong_owner, &[], &[], &[]);
        assert_noop!(
            GameResults::authorize_session_with_ticket(
                RuntimeOrigin::signed(2),
                wrong_owner,
                vec![],
                vec![],
                vec![],
                wrong_owner_hash.to_vec(),
            ),
            Error::<Test>::SessionAuthorizationTicketInvalid
        );

        let expired = default_fps_authorization_ticket(1, [47; 32], [12; 32], 1, &[], &[], &[]);
        assert_noop!(
            submit_authorization_ticket(expired, vec![], vec![], vec![]),
            Error::<Test>::InvalidExpiry
        );

        let mut wrong_randomness =
            default_fps_authorization_ticket(1, [48; 32], [12; 32], 50, &[], &[], &[]);
        wrong_randomness.expected_randomness_provenance = RandomnessMode::DeterministicPrivateAlpha;
        assert_noop!(
            submit_authorization_ticket(wrong_randomness, vec![], vec![], vec![]),
            Error::<Test>::SessionAuthorizationTicketInvalid
        );

        assert_eq!(crate::NextSessionId::<Test>::get(), 0);
        assert_eq!(
            RewardBudgets::<Test>::get((FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1))
                .expect("configured budget")
                .xp_reserved,
            0
        );
    });
}

#[test]
fn authority_ticket_receipt_cap_accepts_boundary_then_fails_closed() {
    new_test_ext().execute_with(|| {
        configure(false);
        let existing: BoundedVec<_, MaxSessionAuthorizationReceiptsPerEpoch> =
            vec![[61; 32], [62; 32], [63; 32]].try_into().unwrap();
        crate::EpochAuthorizationIds::<Test>::insert(0, existing);

        let boundary = default_fps_authorization_ticket(1, [64; 32], [12; 32], 50, &[], &[], &[]);
        assert_ok!(submit_authorization_ticket(
            boundary,
            vec![],
            vec![],
            vec![]
        ));
        assert_eq!(crate::EpochAuthorizationIds::<Test>::get(0).len(), 4);
        assert_eq!(crate::NextSessionId::<Test>::get(), 1);

        let over_cap = default_fps_authorization_ticket(2, [65; 32], [13; 32], 50, &[], &[], &[]);
        assert_noop!(
            submit_authorization_ticket(over_cap, vec![], vec![], vec![]),
            Error::<Test>::SessionAuthorizationReceiptLimit
        );
        assert_eq!(crate::NextSessionId::<Test>::get(), 1);
        assert_eq!(crate::ActiveSessionCount::<Test>::get(2), 0);
        assert_eq!(
            RewardBudgets::<Test>::get((FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1))
                .expect("configured budget")
                .xp_reserved,
            500
        );
    });
}

#[test]
fn epoch_sealing_waits_for_ticket_expiry_then_prunes_replay_receipts() {
    new_test_ext().execute_with(|| {
        configure(false);
        let first = default_fps_authorization_ticket(1, [51; 32], [1; 32], 100, &[], &[], &[]);
        assert_ok!(submit_authorization_ticket(
            first.clone(),
            vec![],
            vec![],
            vec![]
        ));
        for owner in 2..=4 {
            authorize_fps(owner, [owner as u8; 32], 100);
        }
        for session_id in 1..=4 {
            assert_ok!(GameResults::submit_result(
                RuntimeOrigin::signed(99),
                signed_result_for(session_id, 50 + session_id as u8, 10),
            ));
        }

        System::set_block_number(6);
        assert_noop!(
            GameResults::seal_result_epoch(RuntimeOrigin::signed(99), 0),
            Error::<Test>::EpochAuthorizationTicketsLive
        );
        assert!(crate::SessionAuthorizationReceipts::<Test>::contains_key(
            first.authorization_id
        ));

        System::set_block_number(100);
        assert_ok!(GameResults::seal_result_epoch(RuntimeOrigin::signed(99), 0));
        assert!(!crate::SessionAuthorizationReceipts::<Test>::contains_key(
            first.authorization_id
        ));
        assert_noop!(
            submit_authorization_ticket(first, vec![], vec![], vec![]),
            Error::<Test>::InvalidExpiry
        );
    });
}

#[test]
fn alpha_access_gates_new_sessions_but_not_settlement_or_recovery() {
    new_test_ext().execute_with(|| {
        configure(false);
        ACCESS_ALLOWED.with(|allowed| allowed.set(false));
        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(1),
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
                1,
                EconomicRealm::Training,
                [12; 32],
                None,
                vec![],
                vec![],
                vec![],
                50,
            ),
            sp_runtime::DispatchError::Other("not whitelisted")
        );
        assert_eq!(crate::NextSessionId::<Test>::get(), 0);

        ACCESS_ALLOWED.with(|allowed| allowed.set(true));
        authorize_fps(1, [12; 32], 50);
        ACCESS_ALLOWED.with(|allowed| allowed.set(false));
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_result_for(1, 46, 10),
        ));
        assert_eq!(
            Sessions::<Test>::get(1).expect("settled session").status,
            SessionStatus::Settled
        );
    });
}

#[test]
fn authority_active_session_cap_is_released_on_every_terminal_path() {
    new_test_ext().execute_with(|| {
        configure(false);
        for owner in 1..=4 {
            authorize_fps(owner, [owner as u8; 32], 50);
        }
        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(5),
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
                1,
                EconomicRealm::Training,
                [5; 32],
                None,
                vec![],
                vec![],
                vec![],
                50,
            ),
            Error::<Test>::TooManyActiveSessionsForAuthority
        );

        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_result_for(1, 47, 10),
        ));
        assert_eq!(
            crate::ActiveSessionCountByAuthority::<Test>::get((
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
            )),
            3
        );
        authorize_fps(5, [5; 32], 50);

        ACCESS_ALLOWED.with(|allowed| allowed.set(false));
        System::set_block_number(52);
        assert_ok!(GameResults::expire_session(RuntimeOrigin::signed(99), 2));
        assert_ok!(GameResults::emergency_abort_session(
            RuntimeOrigin::root(),
            3
        ));
        assert_ok!(GameResults::expire_session(RuntimeOrigin::signed(99), 4));
        assert_ok!(GameResults::expire_session(RuntimeOrigin::signed(99), 5));
        assert_eq!(
            crate::ActiveSessionCountByAuthority::<Test>::get((
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
            )),
            0
        );
        for owner in 1..=5 {
            assert_eq!(crate::ActiveSessionCount::<Test>::get(owner), 0);
        }
    });
}

#[test]
fn fps_result_rejects_duplicate_charge_and_prism_uses() {
    new_test_ext().execute_with(|| {
        configure(false);
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(1),
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            1,
            EconomicRealm::Training,
            [12; 32],
            None,
            vec![],
            vec![AssetRevision {
                asset_id: 77,
                revision: 1,
            }],
            vec![ChargeUse {
                definition_id: 10,
                amount: 2,
            }],
            50,
        ));

        let mut duplicate_charges = signed_result_for(1, 48, 10);
        let ResultBodyV1::FpsMatch(body) = &mut duplicate_charges.body else {
            unreachable!()
        };
        body.used_charges = vec![
            ChargeUse {
                definition_id: 10,
                amount: 1,
            },
            ChargeUse {
                definition_id: 10,
                amount: 1,
            },
        ]
        .try_into()
        .expect("two uses fit the test bound");
        let payload_hash =
            GameResults::result_payload_hash(&duplicate_charges.header, &duplicate_charges.body);
        duplicate_charges.server_signature = payload_hash.to_vec().try_into().unwrap();
        assert_noop!(
            GameResults::submit_result(RuntimeOrigin::signed(99), duplicate_charges),
            Error::<Test>::DuplicateResultAssetUse
        );

        let mut duplicate_prisms = signed_result_for(1, 49, 10);
        let ResultBodyV1::FpsMatch(body) = &mut duplicate_prisms.body else {
            unreachable!()
        };
        body.used_prisms = vec![77, 77]
            .try_into()
            .expect("two uses fit the test bound");
        let payload_hash =
            GameResults::result_payload_hash(&duplicate_prisms.header, &duplicate_prisms.body);
        duplicate_prisms.server_signature = payload_hash.to_vec().try_into().unwrap();
        assert_noop!(
            GameResults::submit_result(RuntimeOrigin::signed(99), duplicate_prisms),
            Error::<Test>::DuplicateResultAssetUse
        );
        assert_eq!(
            Sessions::<Test>::get(1)
                .expect("session remains active")
                .status,
            SessionStatus::Active
        );
    });
}

#[test]
fn result_id_conflict_and_implausible_metrics_are_rejected() {
    new_test_ext().execute_with(|| {
        configure(false);
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(1),
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            1,
            EconomicRealm::Training,
            [12; 32],
            None,
            vec![],
            vec![],
            vec![],
            50
        ));
        let good = signed_result([5; 32], 10);
        assert_ok!(GameResults::submit_result(RuntimeOrigin::signed(2), good));
        let conflict = signed_result([5; 32], 11);
        assert_noop!(
            GameResults::submit_result(RuntimeOrigin::signed(2), conflict),
            Error::<Test>::ResultIdConflict
        );
    });
}

#[test]
fn normalized_mode_rejects_every_persistent_power_reference() {
    new_test_ext().execute_with(|| {
        configure(true);
        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(1),
                FPS_GAME_ID,
                1,
                NORMALIZED_LEGACY_MODE_ID,
                1,
                1,
                EconomicRealm::Training,
                [12; 32],
                None,
                vec![AssetRevision {
                    asset_id: 7,
                    revision: 1,
                }],
                vec![],
                vec![],
                50
            ),
            Error::<Test>::NormalizedPersistentAssetRejected
        );
    });
}

#[test]
fn session_roster_root_is_runtime_derived_and_loadout_bounds_fail_before_locking() {
    new_test_ext().execute_with(|| {
        configure(false);
        let entity = AssetRevision {
            asset_id: 7,
            revision: 1,
        };
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(1),
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            1,
            EconomicRealm::Training,
            [12; 32],
            None,
            vec![entity],
            vec![],
            vec![],
            50,
        ));
        let expected = GameResults::session_roster_root(
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            EconomicRealm::Training,
            None,
            &[entity],
            &[],
            &[],
        );
        assert_eq!(Sessions::<Test>::get(1).unwrap().roster_root, expected);

        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(2),
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
                1,
                EconomicRealm::Training,
                [13; 32],
                None,
                vec![
                    AssetRevision {
                        asset_id: 8,
                        revision: 1,
                    },
                    AssetRevision {
                        asset_id: 9,
                        revision: 1,
                    },
                ],
                vec![],
                vec![],
                50,
            ),
            Error::<Test>::PersistentLoadoutRejected
        );
        ENTITY_LOCKS.with(|locks| {
            assert!(locks.borrow().contains_key(&7));
            assert!(!locks.borrow().contains_key(&8));
            assert!(!locks.borrow().contains_key(&9));
        });
    });
}

#[test]
fn authority_epoch_requires_an_immutable_nonzero_server_rules_hash() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GameResults::register_authority_epoch(
                RuntimeOrigin::root(),
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
                AuthorityEpoch {
                    public_key: [1; 32],
                    authority_config_hash: [0; 32],
                    active_from: 1,
                    active_until: 1_000,
                    revoked: false,
                },
            ),
            Error::<Test>::InvalidPolicy
        );
    });
}

#[test]
fn expiry_releases_locks_and_grants_nothing() {
    new_test_ext().execute_with(|| {
        configure(false);
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(1),
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            1,
            EconomicRealm::Training,
            [12; 32],
            None,
            vec![AssetRevision {
                asset_id: 7,
                revision: 1,
            }],
            vec![],
            vec![],
            20
        ));
        ENTITY_LOCKS.with(|locks| assert!(locks.borrow().contains_key(&7)));
        System::set_block_number(22);
        assert_ok!(GameResults::expire_session(RuntimeOrigin::signed(5), 1));
        ENTITY_LOCKS.with(|locks| assert!(locks.borrow().is_empty()));
        PLAYER_XP.with(|xp| assert!(xp.borrow().is_empty()));
        assert_eq!(
            Sessions::<Test>::get(1).unwrap().status,
            SessionStatus::Expired
        );
    });
}

#[test]
fn authority_revocation_is_fail_closed_for_new_sessions() {
    new_test_ext().execute_with(|| {
        configure(false);
        assert_ok!(GameResults::revoke_authority_epoch(
            RuntimeOrigin::root(),
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
        ));
        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(1),
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
                1,
                EconomicRealm::Training,
                [12; 32],
                None,
                vec![],
                vec![],
                vec![],
                50
            ),
            Error::<Test>::AuthorityRevoked
        );
    });
}

#[test]
fn reward_budget_is_reserved_before_play_and_released_on_expiry() {
    new_test_ext().execute_with(|| {
        configure(false);
        RewardBudgets::<Test>::mutate((FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1), |maybe| {
            maybe.as_mut().unwrap().xp_total = 500
        });
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(1),
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            1,
            EconomicRealm::Training,
            [12; 32],
            None,
            vec![],
            vec![],
            vec![],
            20
        ));
        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(2),
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
                1,
                EconomicRealm::Training,
                [13; 32],
                None,
                vec![],
                vec![],
                vec![],
                20
            ),
            Error::<Test>::RewardBudgetInsufficient
        );
        System::set_block_number(22);
        assert_ok!(GameResults::expire_session(RuntimeOrigin::signed(9), 1));
        assert_eq!(
            RewardBudgets::<Test>::get((FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1))
                .unwrap()
                .xp_reserved,
            0
        );
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(2),
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            1,
            EconomicRealm::Training,
            [13; 32],
            None,
            vec![],
            vec![],
            vec![],
            30
        ));
    });
}

#[test]
fn legends_loot_is_delayed_runtime_derived_and_idempotent() {
    new_test_ext().execute_with(|| {
        configure_legends();
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(1),
            LEGENDS_GAME_ID,
            1,
            1,
            1,
            1,
            EconomicRealm::Training,
            [22; 32],
            Some(1),
            vec![AssetRevision {
                asset_id: 7,
                revision: 1,
            }],
            vec![],
            vec![],
            50
        ));
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_legends_result([11; 32])
        ));
        assert_eq!(
            Sessions::<Test>::get(1).unwrap().status,
            SessionStatus::SettledPendingDrop
        );
        ENTITY_XP.with(|xp| {
            let grants = xp.borrow();
            assert_eq!(grants.len(), 1);
            assert_eq!((grants[0].0, grants[0].1, grants[0].2), (1, 7, 100));
        });
        let pending = crate::PendingDrops::<Test>::get(1).expect("drop pending");
        let reserved = RewardBudgets::<Test>::get((LEGENDS_GAME_ID, 1, 1, 1)).unwrap();
        assert_eq!(reserved.charge_slots_reserved, 1);
        assert_eq!(reserved.prism_slots_reserved, 1);
        RANDOM_OUTPUTS.with(|outputs| {
            outputs.borrow_mut().insert(pending.request_id, [0; 32]);
        });
        assert_ok!(GameResults::finalize_drop(RuntimeOrigin::signed(3), 1));
        PRISM_REWARDS.with(|rewards| {
            let rewards = rewards.borrow();
            assert_eq!(rewards.len(), 1);
            let expected_traits_seed = sp_io::hashing::blake2_256(
                &(
                    *b"ETERRA_RPG_DROP_V1______________",
                    result_id_for(1, 11),
                    1u64,
                    (LEGENDS_GAME_ID, 1u32, 1u32, 1u32),
                    [0u8; 32],
                    b"PRISM_TRAITS_V1",
                )
                    .encode(),
            );
            assert_eq!(rewards[0].3, expected_traits_seed);
            assert_ne!(rewards[0].3, [0; 32]);
        });
        assert_eq!(
            Sessions::<Test>::get(1).unwrap().status,
            SessionStatus::Settled
        );
        assert!(crate::PendingDrops::<Test>::get(1).is_none());
        let settled = RewardBudgets::<Test>::get((LEGENDS_GAME_ID, 1, 1, 1)).unwrap();
        assert_eq!(settled.charge_slots_reserved, 0);
        assert_eq!(settled.prism_slots_reserved, 0);
        assert_eq!(settled.charge_slots_spent, 0);
        assert_eq!(settled.prism_slots_spent, 1);
        assert_ok!(GameResults::finalize_drop(RuntimeOrigin::signed(4), 1));
        assert_ok!(GameResults::finalize_drop_timeout(
            RuntimeOrigin::signed(5),
            1
        ));
    });
}

#[test]
fn legends_drop_timeout_awards_no_rare_asset_and_releases_liability() {
    new_test_ext().execute_with(|| {
        configure_legends();
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(1),
            LEGENDS_GAME_ID,
            1,
            1,
            1,
            1,
            EconomicRealm::Training,
            [22; 32],
            Some(1),
            vec![AssetRevision {
                asset_id: 7,
                revision: 1,
            }],
            vec![],
            vec![],
            50
        ));
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_legends_result([12; 32])
        ));
        let pending = crate::PendingDrops::<Test>::get(1).expect("drop pending");
        RANDOM_TIMEOUTS.with(|timeouts| {
            timeouts.borrow_mut().insert(pending.request_id, true);
        });
        assert_ok!(GameResults::finalize_drop_timeout(
            RuntimeOrigin::signed(3),
            1
        ));
        let settled = RewardBudgets::<Test>::get((LEGENDS_GAME_ID, 1, 1, 1)).unwrap();
        assert_eq!(settled.charge_slots_reserved, 0);
        assert_eq!(settled.prism_slots_reserved, 0);
        assert_eq!(settled.charge_slots_spent, 0);
        assert_eq!(settled.prism_slots_spent, 0);
        assert_ok!(GameResults::finalize_drop_timeout(
            RuntimeOrigin::signed(4),
            1
        ));
    });
}

#[test]
fn lost_legends_battle_releases_without_spending_unawarded_rewards() {
    new_test_ext().execute_with(|| {
        configure_legends();
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(1),
            LEGENDS_GAME_ID,
            1,
            1,
            1,
            1,
            EconomicRealm::Training,
            [22; 32],
            Some(1),
            vec![AssetRevision {
                asset_id: 7,
                revision: 1,
            }],
            vec![],
            vec![],
            50
        ));
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_legends_result_with_outcome([14; 32], false)
        ));
        let budget = RewardBudgets::<Test>::get((LEGENDS_GAME_ID, 1, 1, 1)).unwrap();
        assert_eq!(budget.xp_reserved, 0);
        assert_eq!(budget.essence_reserved, 0);
        assert_eq!(budget.xp_spent, 0);
        assert_eq!(budget.essence_spent, 0);
        assert_eq!(budget.charge_slots_reserved, 0);
        assert_eq!(budget.prism_slots_reserved, 0);
        assert!(!crate::PendingDrops::<Test>::contains_key(1));
        ENTITY_XP.with(|grants| assert!(grants.borrow().is_empty()));
    });
}

#[test]
fn sealed_epoch_prunes_live_records_and_rejects_result_replay() {
    new_test_ext().execute_with(|| {
        configure(false);
        for owner in 1..=4 {
            assert_ok!(authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(owner),
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
                1,
                EconomicRealm::Training,
                [12; 32],
                None,
                vec![],
                vec![],
                vec![],
                50
            ));
        }
        let result = signed_result([13; 32], 10);
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            result.clone()
        ));
        System::set_block_number(52);
        for session_id in 2..=4 {
            assert_ok!(GameResults::expire_session(
                RuntimeOrigin::signed(9),
                session_id
            ));
        }
        System::set_block_number(57);
        assert_ok!(GameResults::seal_result_epoch(RuntimeOrigin::signed(9), 0));
        assert!(crate::SealedResultEpochs::<Test>::contains_key(0));
        assert!(Sessions::<Test>::get(1).is_none());
        assert!(crate::SettledSessions::<Test>::get(1).is_none());
        assert!(crate::ProcessedResults::<Test>::get(result_id_for(1, 13)).is_none());
        assert_noop!(
            GameResults::submit_result(RuntimeOrigin::signed(99), result),
            Error::<Test>::SealedEpochReplay
        );
    });
}

#[test]
fn result_ids_are_bound_to_the_chain_assigned_session_namespace() {
    new_test_ext().execute_with(|| {
        configure(false);
        authorize_fps(1, [12; 32], 50);
        let mut result = signed_result_for(1, 31, 10);
        result.header.result_id = [31; 32];
        let payload_hash = GameResults::result_payload_hash(&result.header, &result.body);
        result.server_signature = payload_hash.to_vec().try_into().unwrap();
        assert_noop!(
            GameResults::submit_result(RuntimeOrigin::signed(9), result),
            Error::<Test>::ResultNamespaceMismatch
        );
        assert_eq!(
            Sessions::<Test>::get(1).unwrap().status,
            SessionStatus::Active
        );
    });
}

#[test]
fn cohort_multipliers_are_reserved_before_play_and_settlement_order_independent() {
    new_test_ext().execute_with(|| {
        configure(false);
        authorize_fps(1, [44; 32], 50);
        authorize_fps(1, [44; 32], 50);
        let first = Sessions::<Test>::get(1).unwrap();
        let second = Sessions::<Test>::get(2).unwrap();
        assert_eq!(
            (first.cohort_ordinal, first.cohort_multiplier_bps),
            (0, 10_000)
        );
        assert_eq!(
            (second.cohort_ordinal, second.cohort_multiplier_bps),
            (1, 7_500)
        );
        assert_eq!(
            (first.reward_liability.xp, second.reward_liability.xp),
            (500, 375)
        );

        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(9),
            signed_result_for(2, 42, 10),
        ));
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(9),
            signed_result_for(1, 41, 10),
        ));
        PLAYER_XP.with(|grants| {
            let grants = grants.borrow();
            assert_eq!(grants.len(), 2);
            assert!(grants[0].2 < grants[1].2);
        });

        authorize_fps(1, [44; 32], 50);
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(9),
            signed_result_for(3, 43, 10),
        ));
        authorize_fps(1, [44; 32], 50);
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(9),
            signed_result_for(4, 44, 10),
        ));
        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(1),
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
                1,
                EconomicRealm::Training,
                [44; 32],
                None,
                vec![],
                vec![],
                vec![],
                50,
            ),
            Error::<Test>::AntiFarmLimitReached
        );
    });
}

#[test]
fn daily_xp_cap_reserves_full_liability_and_resets_by_block_day() {
    new_test_ext().execute_with(|| {
        let mut capped = policy(false);
        capped.maximum_xp_per_day = 500;
        configure_with_policy(capped);
        authorize_fps(1, [51; 32], 50);
        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(1),
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
                1,
                EconomicRealm::Training,
                [52; 32],
                None,
                vec![],
                vec![],
                vec![],
                50,
            ),
            Error::<Test>::AntiFarmLimitReached
        );
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(9),
            signed_result_for(1, 51, 10),
        ));
        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(1),
                FPS_GAME_ID,
                1,
                ABILITY_DEATHMATCH_MODE_ID,
                1,
                1,
                EconomicRealm::Training,
                [52; 32],
                None,
                vec![],
                vec![],
                vec![],
                50,
            ),
            Error::<Test>::AntiFarmLimitReached
        );
        System::set_block_number(100);
        authorize_fps(1, [52; 32], 150);
        assert_eq!(Sessions::<Test>::get(2).unwrap().reward_day, 1);
    });
}

#[test]
fn legends_first_clear_and_entity_encounter_cap_are_chain_enforced() {
    new_test_ext().execute_with(|| {
        let mut one_reward = legends_policy();
        one_reward.per_entity_encounter_rewards_per_day = 1;
        configure_legends_with_policy(one_reward);
        authorize_legends(1, 7, 77, 50);
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(9),
            signed_legends_result_for(1, 61, true),
        ));
        assert_eq!(
            crate::FirstClearMarkers::<Test>::get(
                1,
                (EconomicRealm::Training, LEGENDS_GAME_ID, 77)
            ),
            Some(result_id_for(1, 61))
        );
        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(1),
                LEGENDS_GAME_ID,
                1,
                1,
                1,
                1,
                EconomicRealm::Training,
                [22; 32],
                Some(77),
                vec![AssetRevision {
                    asset_id: 7,
                    revision: 1,
                }],
                vec![],
                vec![],
                50,
            ),
            Error::<Test>::AntiFarmLimitReached
        );
    });
}

#[test]
fn legends_requires_an_entity_and_bounds_unresolved_drop_liability() {
    new_test_ext().execute_with(|| {
        configure_legends();
        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(1),
                LEGENDS_GAME_ID,
                1,
                1,
                1,
                1,
                EconomicRealm::Training,
                [22; 32],
                Some(1),
                vec![],
                vec![],
                vec![],
                50,
            ),
            Error::<Test>::EmptyEntityRoster
        );
        authorize_legends(1, 7, 1, 50);
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(9),
            signed_legends_result_for(1, 71, true),
        ));
        authorize_legends(1, 7, 1, 50);
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(9),
            signed_legends_result_for(2, 72, true),
        ));
        assert_eq!(crate::PendingDropLiabilityCount::<Test>::get(1), 2);
        assert_noop!(
            authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(1),
                LEGENDS_GAME_ID,
                1,
                1,
                1,
                1,
                EconomicRealm::Training,
                [22; 32],
                Some(1),
                vec![AssetRevision {
                    asset_id: 7,
                    revision: 1,
                }],
                vec![],
                vec![],
                50,
            ),
            Error::<Test>::TooManyPendingDrops
        );
    });
}

#[test]
fn randomness_outage_keeps_base_legends_rewards_and_skips_rare_drop() {
    new_test_ext().execute_with(|| {
        configure_legends();
        authorize_legends(1, 7, 1, 50);
        RANDOM_REQUEST_AVAILABLE.with(|available| available.set(false));
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(9),
            signed_legends_result_for(1, 81, true),
        ));
        assert_eq!(
            Sessions::<Test>::get(1).unwrap().status,
            SessionStatus::Settled
        );
        assert!(!crate::PendingDrops::<Test>::contains_key(1));
        assert_eq!(crate::PendingDropLiabilityCount::<Test>::get(1), 0);
        ENTITY_XP.with(|grants| assert_eq!(grants.borrow().len(), 1));
        let budget = RewardBudgets::<Test>::get((LEGENDS_GAME_ID, 1, 1, 1)).unwrap();
        assert_eq!((budget.xp_spent, budget.essence_spent), (100, 5));
        assert_eq!(
            (budget.charge_slots_reserved, budget.prism_slots_reserved),
            (0, 0)
        );
    });
}

#[test]
fn emergency_abort_recovers_previously_unlocked_assets_and_all_liability() {
    new_test_ext().execute_with(|| {
        configure_legends();
        authorize_legends(1, 7, 1, 50);
        ENTITY_LOCKS.with(|locks| {
            locks.borrow_mut().remove(&7);
        });
        assert_ok!(GameResults::emergency_abort_session(
            RuntimeOrigin::root(),
            1,
        ));
        assert_eq!(
            Sessions::<Test>::get(1).unwrap().status,
            SessionStatus::Aborted
        );
        assert_eq!(crate::PendingDropLiabilityCount::<Test>::get(1), 0);
        let budget = RewardBudgets::<Test>::get((LEGENDS_GAME_ID, 1, 1, 1)).unwrap();
        assert_eq!(
            (
                budget.xp_reserved,
                budget.essence_reserved,
                budget.charge_slots_reserved,
                budget.prism_slots_reserved,
            ),
            (0, 0, 0, 0)
        );
    });
}

#[test]
fn production_policy_activation_requires_reviewed_live_randomness() {
    new_test_ext().execute_with(|| {
        let production = production_fps_policy();
        assert_ok!(GameResults::register_authority_epoch(
            RuntimeOrigin::root(),
            FPS_GAME_ID,
            1,
            ABILITY_DEATHMATCH_MODE_ID,
            1,
            AuthorityEpoch {
                public_key: [1; 32],
                authority_config_hash: [2; 32],
                active_from: 1,
                active_until: 1_000,
                revoked: false,
            }
        ));
        assert_ok!(GameResults::publish_reward_policy(
            RuntimeOrigin::root(),
            production,
            budget(),
        ));
        PRODUCTION_RANDOMNESS_READY.with(|ready| ready.set(false));
        assert_noop!(
            GameResults::set_reward_policy_activation(
                RuntimeOrigin::root(),
                (FPS_GAME_ID, 1, ABILITY_DEATHMATCH_MODE_ID, 1),
                true,
            ),
            Error::<Test>::ProductionRandomnessUnavailable
        );
    });
}

#[test]
fn production_policy_rejects_any_noncanonical_v1_anti_farm_baseline() {
    new_test_ext().execute_with(|| {
        assert!(policy(false).validate(), "Training remains configurable");
        let canonical = production_fps_policy();
        assert!(canonical.validate());

        let mut candidates = Vec::new();
        let mut wrong = canonical;
        wrong.minimum_active_seconds = 299;
        candidates.push(wrong);
        let mut wrong = canonical;
        wrong.maximum_afk_bps = 2_499;
        candidates.push(wrong);
        let mut wrong = canonical;
        wrong.max_player_xp = 599;
        candidates.push(wrong);
        let mut wrong = canonical;
        wrong.maximum_xp_per_day = 3_599;
        candidates.push(wrong);
        let mut wrong = canonical;
        wrong.elimination_weight_bps = 1_999;
        wrong.participation_weight_bps = 3_001;
        candidates.push(wrong);
        let mut wrong = canonical;
        wrong.repeat_cohort_multipliers_bps = [10_000, 7_500, 5_000, 2_501, 0];
        candidates.push(wrong);

        for candidate in candidates {
            assert_noop!(
                GameResults::publish_reward_policy(RuntimeOrigin::root(), candidate, budget()),
                Error::<Test>::InvalidPolicy
            );
        }
    });
}

#[test]
fn production_session_rejects_alpha_substitution_after_authorization() {
    new_test_ext().execute_with(|| {
        let mut production = legends_policy();
        production.economic_realm = EconomicRealm::Production;
        configure_legends_with_policy(production);
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(1),
            LEGENDS_GAME_ID,
            1,
            1,
            1,
            1,
            EconomicRealm::Production,
            [22; 32],
            Some(1),
            vec![AssetRevision {
                asset_id: 7,
                revision: 1,
            }],
            vec![],
            vec![],
            50
        ));
        assert_eq!(
            Sessions::<Test>::get(1).unwrap().randomness_provenance,
            RandomnessMode::DrandQuicknet
        );

        RANDOMNESS_MODE.with(|mode| mode.set(RandomnessMode::DeterministicPrivateAlpha));
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_legends_result_for(1, 92, true)
        ));
        assert!(!crate::PendingDrops::<Test>::contains_key(1));
        assert_eq!(
            Sessions::<Test>::get(1).unwrap().status,
            SessionStatus::Settled
        );
        PRISM_REWARDS.with(|rewards| assert!(rewards.borrow().is_empty()));
    });
}

#[test]
fn production_drop_consumes_only_the_bound_drand_output_after_mode_switch() {
    new_test_ext().execute_with(|| {
        let mut production = legends_policy();
        production.economic_realm = EconomicRealm::Production;
        configure_legends_with_policy(production);
        assert_ok!(authorize_session_with_authority_ticket(
            RuntimeOrigin::signed(1),
            LEGENDS_GAME_ID,
            1,
            1,
            1,
            1,
            EconomicRealm::Production,
            [22; 32],
            Some(1),
            vec![AssetRevision {
                asset_id: 7,
                revision: 1,
            }],
            vec![],
            vec![],
            50
        ));
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_legends_result_for(1, 93, true)
        ));
        let pending = crate::PendingDrops::<Test>::get(1).expect("drop pending");
        assert_eq!(pending.randomness_provenance, RandomnessMode::DrandQuicknet);
        RANDOMNESS_MODE.with(|mode| mode.set(RandomnessMode::DeterministicPrivateAlpha));
        RANDOM_OUTPUTS.with(|outputs| {
            outputs.borrow_mut().insert(pending.request_id, [0; 32]);
        });
        RANDOM_OUTPUT_PROVENANCE.with(|modes| {
            modes.borrow_mut().insert(
                pending.request_id,
                RandomnessMode::DeterministicPrivateAlpha,
            );
        });
        assert_noop!(
            GameResults::finalize_drop(RuntimeOrigin::signed(2), 1),
            Error::<Test>::DropNotReady
        );
        RANDOM_OUTPUT_PROVENANCE.with(|modes| {
            modes
                .borrow_mut()
                .insert(pending.request_id, RandomnessMode::DrandQuicknet);
        });
        assert_ok!(GameResults::finalize_drop(RuntimeOrigin::signed(2), 1));
        assert_ok!(GameResults::finalize_drop(RuntimeOrigin::signed(3), 1));
    });
}

#[test]
fn verified_production_quest_awards_one_deterministic_prism_per_account() {
    new_test_ext().execute_with(|| {
        let mut production = legends_policy();
        production.economic_realm = EconomicRealm::Production;
        production.charge_definition_id = None;
        production.charge_drop_bps = 0;
        production.prism_definition_id = None;
        production.prism_drop_bps = 0;
        let policy_key = (LEGENDS_GAME_ID, 1, 1, 1);
        assert_ok!(GameResults::register_authority_epoch(
            RuntimeOrigin::root(),
            LEGENDS_GAME_ID,
            1,
            1,
            1,
            AuthorityEpoch {
                public_key: [1; 32],
                authority_config_hash: [2; 32],
                active_from: 1,
                active_until: 1_000,
                revoked: false,
            }
        ));
        assert_ok!(GameResults::publish_reward_policy(
            RuntimeOrigin::root(),
            production,
            RewardBudget {
                xp_total: 10_000,
                essence_total: 1_000,
                prism_slots_total: 10,
                ..Default::default()
            }
        ));
        let quest = DeterministicPrismQuestPolicy {
            quest_hash: [77; 32],
            encounter_id: 1,
            prism_definition_id: 20,
            economic_realm: EconomicRealm::Production,
        };
        assert_ok!(GameResults::publish_deterministic_prism_quest_policy(
            RuntimeOrigin::root(),
            policy_key,
            quest,
        ));
        assert_ok!(GameResults::set_reward_policy_activation(
            RuntimeOrigin::root(),
            policy_key,
            true,
        ));
        assert_noop!(
            GameResults::publish_deterministic_prism_quest_policy(
                RuntimeOrigin::root(),
                policy_key,
                DeterministicPrismQuestPolicy {
                    encounter_id: 2,
                    ..quest
                },
            ),
            Error::<Test>::QuestPolicyRequiresInactiveRewardPolicy
        );
        assert_ok!(GameResults::set_reward_policy_activation(
            RuntimeOrigin::root(),
            policy_key,
            false,
        ));
        assert_noop!(
            GameResults::publish_deterministic_prism_quest_policy(
                RuntimeOrigin::root(),
                policy_key,
                DeterministicPrismQuestPolicy {
                    encounter_id: 2,
                    ..quest
                },
            ),
            Error::<Test>::QuestPolicyRequiresInactiveRewardPolicy
        );
        assert_ok!(GameResults::set_reward_policy_activation(
            RuntimeOrigin::root(),
            policy_key,
            true,
        ));

        for entity_id in [7, 8] {
            assert_ok!(authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(1),
                LEGENDS_GAME_ID,
                1,
                1,
                1,
                1,
                EconomicRealm::Production,
                [22; 32],
                Some(1),
                vec![AssetRevision {
                    asset_id: entity_id,
                    revision: 1,
                }],
                vec![],
                vec![],
                50
            ));
        }
        assert_eq!(
            Sessions::<Test>::get(1).unwrap().deterministic_prism_quest,
            Some(quest)
        );
        assert_eq!(
            RewardBudgets::<Test>::get(policy_key)
                .unwrap()
                .prism_slots_reserved,
            2
        );

        let first = signed_legends_result_for(1, 94, true);
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            first.clone()
        ));
        // Exact-result replay is idempotent and cannot mint a second Prism.
        assert_ok!(GameResults::submit_result(RuntimeOrigin::signed(99), first));
        PRISM_REWARDS.with(|rewards| {
            let rewards = rewards.borrow();
            assert_eq!(rewards.len(), 1);
            let result_id = result_id_for(1, 94);
            let (expected_traits, expected_result) =
                GameResults::deterministic_prism_quest_award_ids(&1, 1, result_id, quest);
            assert_eq!(
                rewards[0],
                (
                    1,
                    EconomicRealm::Production,
                    20,
                    expected_traits,
                    expected_result,
                )
            );
        });
        assert_eq!(
            crate::DeterministicPrismQuestClaims::<Test>::get(1, quest.quest_hash),
            Some(result_id_for(1, 94))
        );

        // A second already-authorized session for the same account releases
        // its reserved liability without granting the one-per-account reward.
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_legends_result_for(2, 95, true)
        ));
        PRISM_REWARDS.with(|rewards| assert_eq!(rewards.borrow().len(), 1));
        let settled_budget = RewardBudgets::<Test>::get(policy_key).unwrap();
        assert_eq!(settled_budget.prism_slots_reserved, 0);
        assert_eq!(settled_budget.prism_slots_spent, 1);
        assert!(!crate::PendingDrops::<Test>::contains_key(1));
        assert!(!crate::PendingDrops::<Test>::contains_key(2));

        // A loss proves no completion, and a win in a different verified
        // encounter cannot claim this encounter-bound quest.
        for (owner, entity_id, encounter_id) in [(2, 9, 1), (3, 10, 2)] {
            assert_ok!(authorize_session_with_authority_ticket(
                RuntimeOrigin::signed(owner),
                LEGENDS_GAME_ID,
                1,
                1,
                1,
                1,
                EconomicRealm::Production,
                [22; 32],
                Some(encounter_id),
                vec![AssetRevision {
                    asset_id: entity_id,
                    revision: 1,
                }],
                vec![],
                vec![],
                50
            ));
        }
        assert_eq!(
            Sessions::<Test>::get(3).unwrap().deterministic_prism_quest,
            Some(quest)
        );
        assert_eq!(
            Sessions::<Test>::get(4).unwrap().deterministic_prism_quest,
            None
        );
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_legends_result_for(3, 96, false)
        ));
        assert_ok!(GameResults::submit_result(
            RuntimeOrigin::signed(99),
            signed_legends_result_for(4, 97, true)
        ));
        assert!(crate::DeterministicPrismQuestClaims::<Test>::get(2, quest.quest_hash).is_none());
        assert!(crate::DeterministicPrismQuestClaims::<Test>::get(3, quest.quest_hash).is_none());
        PRISM_REWARDS.with(|rewards| assert_eq!(rewards.borrow().len(), 1));
        let final_budget = RewardBudgets::<Test>::get(policy_key).unwrap();
        assert_eq!(final_budget.prism_slots_reserved, 0);
        assert_eq!(final_budget.prism_slots_spent, 1);
    });
}

#[test]
fn fps_assists_and_placement_are_bounded_signed_facts() {
    new_test_ext().execute_with(|| {
        configure(false);
        authorize_fps(1, [12; 32], 50);
        let mut result = signed_result_for(1, 91, 10);
        let ResultBodyV1::FpsMatch(body) = &mut result.body else {
            unreachable!()
        };
        body.assists = 21;
        body.placement = 9;
        let payload_hash = GameResults::result_payload_hash(&result.header, &result.body);
        result.server_signature = payload_hash.to_vec().try_into().unwrap();
        assert_noop!(
            GameResults::submit_result(RuntimeOrigin::signed(9), result),
            Error::<Test>::ResultMetricsInvalid
        );
    });
}

#[test]
fn ticket_additions_preserve_legacy_scale_discriminants() {
    let ticket = SessionAuthorizationTicket {
        protocol_version: 1,
        genesis_hash: [9; 32],
        pallet_instance_id: 38,
        authorization_id: [1; 32],
        owner: 1,
        game_id: FPS_GAME_ID,
        game_version: 1,
        mode_id: ABILITY_DEATHMATCH_MODE_ID,
        policy_version: 1,
        policy_hash: [3; 32],
        authority_epoch: 1,
        authority_config_hash: [2; 32],
        economic_realm: EconomicRealm::Training,
        cohort_hash: [12; 32],
        encounter_id: None,
        roster_root: [4; 32],
        expected_randomness_provenance: RandomnessMode::DrandQuicknet,
        expires_at: 50,
    };
    let calls = [
        (
            crate::Call::<Test>::authorize_session {
                game_id: FPS_GAME_ID,
                game_version: 1,
                mode_id: ABILITY_DEATHMATCH_MODE_ID,
                policy_version: 1,
                authority_epoch: 1,
                economic_realm: EconomicRealm::Training,
                cohort_hash: [12; 32],
                encounter_id: None,
                entities: vec![],
                prisms: vec![],
                charges: vec![],
                expires_at: 50,
            },
            4,
        ),
        (
            crate::Call::<Test>::publish_deterministic_prism_quest_policy {
                policy_key: (LEGENDS_GAME_ID, 1, 1, 1),
                quest: DeterministicPrismQuestPolicy {
                    quest_hash: [7; 32],
                    encounter_id: 1,
                    prism_definition_id: 20,
                    economic_realm: EconomicRealm::Training,
                },
            },
            11,
        ),
        (
            crate::Call::<Test>::authorize_session_with_ticket {
                ticket,
                entities: vec![],
                prisms: vec![],
                charges: vec![],
                server_signature: vec![1; 32],
            },
            12,
        ),
    ];
    for (call, expected) in calls {
        assert_eq!(call.encode()[0], expected);
    }

    assert_eq!(
        crate::Event::<Test>::DeterministicPrismQuestRewardClaimed {
            owner: 1,
            session_id: 1,
            result_id: [1; 32],
            quest_hash: [2; 32],
            prism_definition_id: 20,
        }
        .encode()[0],
        16
    );
    assert_eq!(
        crate::Event::<Test>::SessionAuthorizationTicketConsumed {
            owner: 1,
            authorization_id: [1; 32],
            ticket_hash: [2; 32],
            session_id: 1,
        }
        .encode()[0],
        17
    );
    assert_eq!(
        Error::<Test>::QuestPolicyRequiresInactiveRewardPolicy
            .encode()
            .as_slice(),
        &[54]
    );
    assert_eq!(
        Error::<Test>::DuplicateResultAssetUse.encode().as_slice(),
        &[62]
    );
}

#[test]
fn scale_session_authorization_ticket_contract_matches_golden_vector() {
    let entities = vec![
        AssetRevision {
            asset_id: 101,
            revision: 3,
        },
        AssetRevision {
            asset_id: 202,
            revision: 5,
        },
    ];
    let prisms = vec![AssetRevision {
        asset_id: 303,
        revision: 2,
    }];
    let charges = vec![
        ChargeUse {
            definition_id: 11,
            amount: 2,
        },
        ChargeUse {
            definition_id: 12,
            amount: 1,
        },
    ];
    let ticket: crate::pallet::SessionAuthorizationTicketOf<Test> = SessionAuthorizationTicket {
        protocol_version: 1,
        genesis_hash: [0xaa; 32],
        pallet_instance_id: 38,
        authorization_id: [0xbb; 32],
        owner: 0x0102_0304_0506_0708,
        game_id: 1006,
        game_version: 1,
        mode_id: 2,
        policy_version: 7,
        policy_hash: [0xcc; 32],
        authority_epoch: 9,
        authority_config_hash: [0xdd; 32],
        economic_realm: EconomicRealm::Training,
        cohort_hash: [0xee; 32],
        encounter_id: Some(42),
        roster_root: [0x44; 32],
        expected_randomness_provenance: RandomnessMode::DrandQuicknet,
        expires_at: 123_456,
    };
    // The pallet ID is deliberately encoded both inside the signed ticket and
    // immediately after the domain. This golden vector freezes that redundant
    // cross-instance replay binding as part of the public wire contract.
    let payload = (
        b"ETERRA_GAME_SESSION_AUTHORIZATION_V1".as_slice(),
        38u8,
        &ticket,
        entities.as_slice(),
        prisms.as_slice(),
        charges.as_slice(),
    )
        .encode();
    let payload_hash =
        GameResults::session_authorization_payload_hash(&ticket, &entities, &prisms, &charges);
    let to_hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    assert_eq!(
        to_hex(&ticket.encode()),
        "0100aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa26bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0807060504030201ee030000010000000200000007000000cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc09000000dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd00eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee012a00000044444444444444444444444444444444444444444444444444444444444444440240e2010000000000"
    );
    assert_eq!(
        to_hex(&payload),
        "904554455252415f47414d455f53455353494f4e5f415554484f52495a4154494f4e5f5631260100aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa26bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0807060504030201ee030000010000000200000007000000cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc09000000dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd00eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee012a00000044444444444444444444444444444444444444444444444444444444444444440240e201000000000008650000000000000003000000ca0000000000000005000000042f0100000000000002000000080b000000020000000c00000001000000"
    );
    assert_eq!(
        to_hex(&payload_hash),
        "c1d88ecd69b6435f818b0f90adf4f70b63d255b8f345599efeb7a6426045d967"
    );
}

#[test]
fn scale_result_and_roster_contract_matches_golden_vector() {
    let entities = vec![
        AssetRevision {
            asset_id: 101,
            revision: 3,
        },
        AssetRevision {
            asset_id: 202,
            revision: 5,
        },
    ];
    let prisms = vec![AssetRevision {
        asset_id: 303,
        revision: 2,
    }];
    let charges = vec![
        ChargeUse {
            definition_id: 11,
            amount: 2,
        },
        ChargeUse {
            definition_id: 12,
            amount: 1,
        },
    ];
    let roster_payload = (
        b"ETERRA_SESSION_ROSTER_V1".as_slice(),
        38u8,
        1006u32,
        1u32,
        1u32,
        7u32,
        EconomicRealm::Training,
        Some(42u32),
        entities.as_slice(),
        prisms.as_slice(),
        charges.as_slice(),
    )
        .encode();
    let roster_root = sp_io::hashing::blake2_256(&roster_payload);
    let mut result_id = [0xbb; 32];
    let session_id = 0x0102_0304_0506_0708u64;
    result_id[..8].copy_from_slice(&session_id.to_le_bytes());
    let header = ResultHeaderV1 {
        protocol_version: 1,
        genesis_hash: [0xaa; 32],
        game_id: 1006,
        game_version: 1,
        mode_id: 1,
        policy_version: 7,
        session_id,
        result_id,
        authority_epoch: 9,
        roster_root,
        expires_at: 123_456u32,
        telemetry_root: [0xcc; 32],
    };
    let body: ResultBodyV1<[u8; 32], Vec<u64>, Vec<ChargeUse>, Vec<u64>> =
        ResultBodyV1::RpgBattle(RpgBattleResultV1 {
            owner_won: true,
            encounter_id: 42,
            entity_ids: vec![101, 202],
            elapsed_seconds: 321,
            turn_count: 17,
            combat_metric: 777,
            transcript_hash: [0xdd; 32],
        });
    let payload = (b"ETERRA_GAME_RESULT_V1".as_slice(), 38u8, &header, &body).encode();
    let payload_hash = sp_io::hashing::blake2_256(&payload);
    let signed: SignedResultV1<u32, [u8; 32], Vec<u64>, Vec<ChargeUse>, Vec<u64>, Vec<u8>> =
        SignedResultV1 {
            header,
            body: body.clone(),
            server_signature: vec![0xeeu8; 64],
        };
    let to_hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    assert_eq!(
        to_hex(&roster_payload),
        "604554455252415f53455353494f4e5f524f535445525f563126ee03000001000000010000000700000000012a00000008650000000000000003000000ca0000000000000005000000042f0100000000000002000000080b000000020000000c00000001000000"
    );
    assert_eq!(
        to_hex(&roster_root),
        "69cd03b6f68b3557a315653a79cee4add79500a52c66251101516088901c209e"
    );
    assert_eq!(
        to_hex(&header.encode()),
        "0100aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaee03000001000000010000000700000008070605040302010807060504030201bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0900000069cd03b6f68b3557a315653a79cee4add79500a52c66251101516088901c209e40e20100cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    );
    assert_eq!(
        to_hex(&body.encode()),
        "00012a000000086500000000000000ca0000000000000041010000110009030000dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    );
    assert_eq!(
        to_hex(&payload),
        "544554455252415f47414d455f524553554c545f5631260100aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaee03000001000000010000000700000008070605040302010807060504030201bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0900000069cd03b6f68b3557a315653a79cee4add79500a52c66251101516088901c209e40e20100cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc00012a000000086500000000000000ca0000000000000041010000110009030000dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    );
    assert_eq!(
        to_hex(&payload_hash),
        "53406f39eaf9e32b15639cc571bccf1147ef399124d462e5670e58573da12f68"
    );
    assert_eq!(
        to_hex(&signed.encode()),
        "0100aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaee03000001000000010000000700000008070605040302010807060504030201bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0900000069cd03b6f68b3557a315653a79cee4add79500a52c66251101516088901c209e40e20100cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc00012a000000086500000000000000ca0000000000000041010000110009030000dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd0101eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    );
}
