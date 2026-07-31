use super::*;
use frame_benchmarking::{account, benchmarks};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use pallet_eterra_randomness::VerifiableRandomness;
use sp_runtime::traits::{One, Saturating};
use sp_std::vec;

const GAME_ID: u32 = FPS_GAME_ID;
const GAME_VERSION: u32 = 1;
const MODE_ID: u32 = ABILITY_DEATHMATCH_MODE_ID;
const POLICY_VERSION: u32 = 1;
const AUTHORITY_EPOCH: u32 = 1;
const POLICY_KEY: (u32, u32, u32, u32) = (GAME_ID, GAME_VERSION, MODE_ID, POLICY_VERSION);

fn session_epoch<T: Config>(session_id: SessionId) -> u64 {
    session_id.saturating_sub(1) / T::ResultEpochSize::get().max(1)
}

fn practice_policy() -> RewardPolicy {
    RewardPolicy {
        game_id: GAME_ID,
        game_version: GAME_VERSION,
        mode_id: MODE_ID,
        policy_version: POLICY_VERSION,
        mode_kind: GameModeKind::AbilityDeathmatch,
        economic_realm: EconomicRealm::Training,
        practice_only: true,
        normalized: false,
        loadout: PersistentLoadoutPolicy {
            entity_format: Some((1, 1)),
            allowed_entity_roles_mask: 0x7f,
            max_entities: 1,
            max_prisms: 1,
            max_charge_definitions: 1,
            max_total_charges: 1,
            max_magic_load: 4,
            rules_hash: [15; 32],
        },
        max_player_xp: 0,
        entity_xp: 0,
        base_essence: 0,
        essence_element: Element::Neutral,
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
        policy_hash: [2; 32],
    }
}

fn rewarded_fps_policy() -> RewardPolicy {
    let mut policy = practice_policy();
    policy.practice_only = false;
    policy.max_player_xp = 500;
    policy.elimination_weight_bps = 2_000;
    policy.participation_weight_bps = 3_000;
    policy.objective_weight_bps = 5_000;
    policy.maximum_xp_per_day = 10_000;
    policy.policy_hash = [13; 32];
    policy
}

fn authority<T: Config>() -> AuthorityEpoch<BlockNumberFor<T>> {
    let now = frame_system::Pallet::<T>::block_number();
    AuthorityEpoch {
        public_key: T::BenchmarkHelper::authority_public_key(),
        authority_config_hash: [14; 32],
        active_from: now,
        active_until: now
            .saturating_add(T::MaxSessionLifetime::get())
            .saturating_add(One::one()),
        revoked: false,
    }
}

fn seed_policy<T: Config>() {
    RewardPolicies::<T>::insert(POLICY_KEY, rewarded_fps_policy());
    RewardBudgets::<T>::insert(
        POLICY_KEY,
        RewardBudget {
            xp_total: 10_000,
            ..Default::default()
        },
    );
    RewardPolicyActivation::<T>::insert(POLICY_KEY, true);
}

fn seed_practice_policy<T: Config>() {
    RewardPolicies::<T>::insert(POLICY_KEY, practice_policy());
    RewardBudgets::<T>::insert(POLICY_KEY, RewardBudget::default());
    RewardPolicyActivation::<T>::insert(POLICY_KEY, true);
}

fn seed_authority<T: Config>() {
    AuthorityEpochs::<T>::insert(
        (GAME_ID, GAME_VERSION, MODE_ID, AUTHORITY_EPOCH),
        authority::<T>(),
    );
}

fn authorize_empty_session<T: Config>(owner: T::AccountId) -> SessionId {
    let now = frame_system::Pallet::<T>::block_number();
    let expires_at = now.saturating_add(T::MaxSessionLifetime::get());
    let entities = vec![];
    let prisms = vec![];
    let charges = vec![];
    let ticket = SessionAuthorizationTicket {
        protocol_version: 1,
        genesis_hash: T::GenesisHashProvider::genesis_hash(),
        pallet_instance_id: T::PalletInstanceId::get(),
        authorization_id: [42; 32],
        owner: owner.clone(),
        game_id: GAME_ID,
        game_version: GAME_VERSION,
        mode_id: MODE_ID,
        policy_version: POLICY_VERSION,
        policy_hash: rewarded_fps_policy().policy_hash,
        authority_epoch: AUTHORITY_EPOCH,
        authority_config_hash: [14; 32],
        economic_realm: EconomicRealm::Training,
        cohort_hash: [4; 32],
        encounter_id: None,
        roster_root: Pallet::<T>::session_roster_root(
            GAME_ID,
            GAME_VERSION,
            MODE_ID,
            POLICY_VERSION,
            EconomicRealm::Training,
            None,
            &entities,
            &prisms,
            &charges,
        ),
        expected_randomness_provenance: T::Randomness::current_mode(),
        expires_at,
    };
    let ticket_hash =
        Pallet::<T>::session_authorization_payload_hash(&ticket, &entities, &prisms, &charges);
    let signature = T::BenchmarkHelper::sign_result(&ticket_hash);
    Pallet::<T>::authorize_session_with_ticket(
        RawOrigin::Signed(owner).into(),
        ticket,
        entities,
        prisms,
        charges,
        signature,
    )
    .expect("benchmark session setup is valid");
    NextSessionId::<T>::get()
}

fn seed_pending_drop<T: Config>(owner: T::AccountId) -> (SessionId, Hash32, Hash32) {
    let session_id: SessionId = 1;
    let mut result_id = [9; 32];
    result_id[..8].copy_from_slice(&session_id.to_le_bytes());
    let request_id = [10; 32];
    let expires_at =
        frame_system::Pallet::<T>::block_number().saturating_add(T::MaxSessionLifetime::get());
    Sessions::<T>::insert(
        session_id,
        SessionRecord {
            session_id,
            owner: owner.clone(),
            game_id: GAME_ID,
            game_version: GAME_VERSION,
            mode_id: MODE_ID,
            policy_version: POLICY_VERSION,
            authority_epoch: AUTHORITY_EPOCH,
            economic_realm: EconomicRealm::Training,
            roster_root: [3; 32],
            cohort_hash: [4; 32],
            encounter_id: None,
            reward_day: 0,
            cohort_ordinal: 0,
            cohort_multiplier_bps: 10_000,
            reward_liability: RewardLiability::default(),
            pending_drop_slot_reserved: true,
            entities: BoundedVec::default(),
            prisms: BoundedVec::default(),
            charge_allowance: BoundedVec::default(),
            expires_at,
            status: SessionStatus::SettledPendingDrop,
            result_id: Some(result_id),
            randomness_provenance: RandomnessMode::DeterministicPrivateAlpha,
            deterministic_prism_quest: None,
        },
    );
    PendingDrops::<T>::insert(
        session_id,
        PendingDropResolution {
            session_id,
            owner: owner.clone(),
            economic_realm: EconomicRealm::Training,
            result_id,
            request_id,
            policy_key: POLICY_KEY,
            charge_definition_id: None,
            charge_drop_bps: 0,
            prism_definition_id: None,
            prism_drop_bps: 0,
            randomness_provenance: RandomnessMode::DeterministicPrivateAlpha,
        },
    );
    PendingDropLiabilityCount::<T>::insert(&owner, 1);
    RewardBudgets::<T>::insert(POLICY_KEY, RewardBudget::default());
    EpochSessionCount::<T>::insert(session_epoch::<T>(session_id), 1);
    (session_id, result_id, request_id)
}

benchmarks! {
    register_authority_epoch {
        let record = authority::<T>();
    }: _(RawOrigin::Root, GAME_ID, GAME_VERSION, MODE_ID, AUTHORITY_EPOCH, record)
    verify {
        assert!(AuthorityEpochs::<T>::contains_key((
            GAME_ID,
            GAME_VERSION,
            MODE_ID,
            AUTHORITY_EPOCH,
        )));
    }

    revoke_authority_epoch {
        seed_authority::<T>();
    }: _(
        RawOrigin::Root,
        GAME_ID,
        GAME_VERSION,
        MODE_ID,
        AUTHORITY_EPOCH
    )
    verify {
        assert!(
            AuthorityEpochs::<T>::get((
                GAME_ID,
                GAME_VERSION,
                MODE_ID,
                AUTHORITY_EPOCH,
            ))
            .expect("authority remains registered")
            .revoked
        );
    }

    publish_reward_policy {
        let policy = practice_policy();
        let budget = RewardBudget::default();
    }: _(RawOrigin::Root, policy, budget)
    verify {
        assert!(RewardPolicies::<T>::contains_key(POLICY_KEY));
        assert!(RewardBudgets::<T>::contains_key(POLICY_KEY));
    }

    set_reward_policy_activation {
        RewardPolicies::<T>::insert(POLICY_KEY, practice_policy());
    }: _(RawOrigin::Root, POLICY_KEY, true)
    verify {
        assert!(RewardPolicyActivation::<T>::get(POLICY_KEY));
    }

    authorize_session {
        let owner: T::AccountId = account("owner", 0, 0);
        let now = frame_system::Pallet::<T>::block_number();
        let expires_at = now.saturating_add(T::MaxSessionLifetime::get());
        seed_authority::<T>();
        seed_practice_policy::<T>();
    }: _(
        RawOrigin::Signed(owner.clone()),
        GAME_ID,
        GAME_VERSION,
        MODE_ID,
        POLICY_VERSION,
        AUTHORITY_EPOCH,
        EconomicRealm::Training,
        [4; 32],
        None,
        vec![],
        vec![],
        vec![],
        expires_at
    )
    verify {
        assert_eq!(
            Sessions::<T>::get(1).expect("session exists").owner,
            owner
        );
    }

    authorize_session_with_ticket {
        let owner: T::AccountId = account("owner", 0, 0);
        let now = frame_system::Pallet::<T>::block_number();
        let expires_at = now.saturating_add(T::MaxSessionLifetime::get());
        let entities = vec![];
        let prisms = vec![];
        let charges = vec![];
        seed_authority::<T>();
        seed_policy::<T>();
        let ticket = SessionAuthorizationTicket {
            protocol_version: 1,
            genesis_hash: T::GenesisHashProvider::genesis_hash(),
            pallet_instance_id: T::PalletInstanceId::get(),
            authorization_id: [43; 32],
            owner: owner.clone(),
            game_id: GAME_ID,
            game_version: GAME_VERSION,
            mode_id: MODE_ID,
            policy_version: POLICY_VERSION,
            policy_hash: rewarded_fps_policy().policy_hash,
            authority_epoch: AUTHORITY_EPOCH,
            authority_config_hash: [14; 32],
            economic_realm: EconomicRealm::Training,
            cohort_hash: [4; 32],
            encounter_id: None,
            roster_root: Pallet::<T>::session_roster_root(
                GAME_ID,
                GAME_VERSION,
                MODE_ID,
                POLICY_VERSION,
                EconomicRealm::Training,
                None,
                &entities,
                &prisms,
                &charges,
            ),
            expected_randomness_provenance: T::Randomness::current_mode(),
            expires_at,
        };
        let ticket_hash =
            Pallet::<T>::session_authorization_payload_hash(&ticket, &entities, &prisms, &charges);
        let server_signature = T::BenchmarkHelper::sign_result(&ticket_hash);
    }: _(
        RawOrigin::Signed(owner.clone()),
        ticket,
        entities,
        prisms,
        charges,
        server_signature
    )
    verify {
        assert_eq!(
            Sessions::<T>::get(1).expect("session exists").owner,
            owner
        );
        assert!(SessionAuthorizationReceipts::<T>::contains_key([43; 32]));
    }

    submit_result {
        let owner: T::AccountId = account("owner", 0, 0);
        let caller: T::AccountId = account("caller", 0, 0);
        seed_authority::<T>();
        seed_policy::<T>();
        let session_id = authorize_empty_session::<T>(owner.clone());
        let session = Sessions::<T>::get(session_id).expect("session exists");
        let mut result_id = [11; 32];
        result_id[..8].copy_from_slice(&session_id.to_le_bytes());
        let header = ResultHeaderV1 {
            protocol_version: 1,
            genesis_hash: T::GenesisHashProvider::genesis_hash(),
            game_id: GAME_ID,
            game_version: GAME_VERSION,
            mode_id: MODE_ID,
            policy_version: POLICY_VERSION,
            session_id,
            result_id,
            authority_epoch: AUTHORITY_EPOCH,
            roster_root: session.roster_root,
            expires_at: session.expires_at,
            telemetry_root: [12; 32],
        };
        let body = ResultBodyV1::FpsMatch(FpsMatchResultV1 {
            account: owner,
            cohort_hash: session.cohort_hash,
            active_seconds: 1,
            afk_seconds: 0,
            kills: 0,
            deaths: 0,
            assists: 0,
            damage: 0,
            objective_score: 0,
            outcome: 0,
            placement: 1,
            used_charges: BoundedVec::default(),
            used_prisms: BoundedVec::default(),
        });
        let payload_hash = Pallet::<T>::result_payload_hash(&header, &body);
        let server_signature = T::BenchmarkHelper::sign_result(&payload_hash)
            .try_into()
            .expect("benchmark signature respects the configured bound");
        let result = SignedResultV1 {
            header,
            body,
            server_signature,
        };
    }: _(RawOrigin::Signed(caller), result)
    verify {
        assert_eq!(
            Sessions::<T>::get(session_id).expect("session remains auditable").status,
            SessionStatus::Settled
        );
        assert_eq!(ProcessedResults::<T>::get(result_id), Some(payload_hash));
    }

    expire_session {
        let owner: T::AccountId = account("owner", 0, 0);
        let caller: T::AccountId = account("caller", 0, 0);
        seed_authority::<T>();
        seed_policy::<T>();
        let session_id = authorize_empty_session::<T>(owner);
        let expires_at = Sessions::<T>::get(session_id)
            .expect("session exists")
            .expires_at;
        frame_system::Pallet::<T>::set_block_number(
            expires_at.saturating_add(T::ExpiryGrace::get()),
        );
    }: _(RawOrigin::Signed(caller), session_id)
    verify {
        assert_eq!(
            Sessions::<T>::get(session_id)
                .expect("terminal session remains until epoch sealing")
                .status,
            SessionStatus::Expired
        );
    }

    emergency_abort_session {
        let owner: T::AccountId = account("owner", 0, 0);
        seed_authority::<T>();
        seed_policy::<T>();
        let session_id = authorize_empty_session::<T>(owner);
    }: _(RawOrigin::Root, session_id)
    verify {
        assert_eq!(
            Sessions::<T>::get(session_id)
                .expect("aborted session remains auditable")
                .status,
            SessionStatus::Aborted
        );
        assert_eq!(
            EpochTerminalCount::<T>::get(session_epoch::<T>(session_id)),
            1
        );
    }

    finalize_drop {
        let caller: T::AccountId = account("caller", 0, 0);
        let owner: T::AccountId = account("owner", 0, 0);
        let (session_id, result_id, request_id) = seed_pending_drop::<T>(owner);
        T::BenchmarkHelper::seed_finalized_randomness(request_id, [0; 32]);
    }: _(RawOrigin::Signed(caller), session_id)
    verify {
        assert!(!PendingDrops::<T>::contains_key(session_id));
        assert_eq!(
            Sessions::<T>::get(session_id).expect("session remains auditable").status,
            SessionStatus::Settled
        );
        assert_eq!(SettledSessions::<T>::get(session_id), None);
        assert_eq!(EpochTerminalCount::<T>::get(session_epoch::<T>(session_id)), 1);
        assert_eq!(
            Sessions::<T>::get(session_id).expect("session remains auditable").result_id,
            Some(result_id)
        );
    }

    finalize_drop_timeout {
        let caller: T::AccountId = account("caller", 0, 0);
        let owner: T::AccountId = account("owner", 0, 0);
        let (session_id, result_id, request_id) = seed_pending_drop::<T>(owner);
        T::BenchmarkHelper::seed_timed_out_randomness(request_id);
    }: _(RawOrigin::Signed(caller), session_id)
    verify {
        assert!(!PendingDrops::<T>::contains_key(session_id));
        assert_eq!(
            Sessions::<T>::get(session_id).expect("session remains auditable").status,
            SessionStatus::Settled
        );
        assert_eq!(
            Sessions::<T>::get(session_id).expect("session remains auditable").result_id,
            Some(result_id)
        );
    }

    seal_result_epoch {
        let caller: T::AccountId = account("caller", 0, 0);
        let owner: T::AccountId = account("owner", 0, 0);
        let epoch = 0u64;
        let now = frame_system::Pallet::<T>::block_number();
        let session_count = T::ResultEpochSize::get().max(1);
        NextSessionId::<T>::put(session_count);
        for session_id in 1..=session_count {
            Sessions::<T>::insert(
                session_id,
                SessionRecord {
                    session_id,
                    owner: owner.clone(),
                    game_id: GAME_ID,
                    game_version: GAME_VERSION,
                    mode_id: MODE_ID,
                    policy_version: POLICY_VERSION,
                    authority_epoch: AUTHORITY_EPOCH,
                    economic_realm: EconomicRealm::Training,
                    roster_root: [3; 32],
                    cohort_hash: [4; 32],
                    encounter_id: None,
                    reward_day: 0,
                    cohort_ordinal: 0,
                    cohort_multiplier_bps: 10_000,
                    reward_liability: RewardLiability::default(),
                    pending_drop_slot_reserved: false,
                    entities: BoundedVec::default(),
                    prisms: BoundedVec::default(),
                    charge_allowance: BoundedVec::default(),
                    expires_at: now,
                    status: SessionStatus::Settled,
                    result_id: None,
                    randomness_provenance: RandomnessMode::Disabled,
                    deterministic_prism_quest: None,
                },
            );
            SettledSessions::<T>::insert(session_id, [5; 32]);
        }
        let mut result_ids = BoundedVec::<Hash32, T::MaxResultsPerEpoch>::default();
        for index in 0..T::MaxResultsPerEpoch::get() {
            let mut result_id = [0; 32];
            result_id[..4].copy_from_slice(&index.to_le_bytes());
            result_ids
                .try_push(result_id)
                .expect("configured result bound accepts its own maximum");
            ProcessedResults::<T>::insert(result_id, [8; 32]);
        }
        EpochResultIds::<T>::insert(epoch, result_ids);
        let mut authorization_ids =
            BoundedVec::<Hash32, T::MaxSessionAuthorizationReceiptsPerEpoch>::default();
        for index in 0..T::MaxSessionAuthorizationReceiptsPerEpoch::get() {
            let mut authorization_id = [1; 32];
            authorization_id[..4].copy_from_slice(&index.to_le_bytes());
            authorization_ids
                .try_push(authorization_id)
                .expect("configured authorization bound accepts its own maximum");
            SessionAuthorizationReceipts::<T>::insert(
                authorization_id,
                SessionAuthorizationReceipt {
                    authorization_id,
                    ticket_hash: [6; 32],
                    session_id: 1,
                    session_epoch: epoch,
                },
            );
        }
        EpochAuthorizationIds::<T>::insert(epoch, authorization_ids);
        EpochAuthorizationMaxExpiry::<T>::insert(epoch, now);
        EpochSessionCount::<T>::insert(epoch, session_count as u32);
        EpochTerminalCount::<T>::insert(epoch, session_count as u32);
        EpochTerminalAccumulator::<T>::insert(epoch, [7; 32]);
        EpochLastTerminalAt::<T>::insert(epoch, now);
        frame_system::Pallet::<T>::set_block_number(
            now.saturating_add(T::ResultDisputeWindow::get()),
        );
    }: _(RawOrigin::Signed(caller), epoch)
    verify {
        let sealed = SealedResultEpochs::<T>::get(epoch).expect("epoch sealed");
        assert_eq!(sealed.terminal_root, [7; 32]);
        assert_eq!(sealed.session_count, session_count as u32);
        assert!(Sessions::<T>::iter().next().is_none());
        assert!(SettledSessions::<T>::iter().next().is_none());
        assert!(ProcessedResults::<T>::iter().next().is_none());
        assert!(SessionAuthorizationReceipts::<T>::iter().next().is_none());
        assert!(!EpochAuthorizationMaxExpiry::<T>::contains_key(epoch));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
