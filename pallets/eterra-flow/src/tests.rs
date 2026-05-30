use super::*;
use crate::mock::{
    new_test_ext, EterraAuthority, EterraEconomy, EterraFlow, EterraProfile, RuntimeEvent,
    RuntimeOrigin, System, Test,
};
use codec::{Decode, Encode};
use frame_support::{
    assert_noop, assert_ok,
    traits::{Get, GetStorageVersion, StorageVersion},
    BoundedVec,
};
use sp_core::H256;
use sp_runtime::traits::Hash as HashT;
use sp_std::vec;
use sp_std::vec::Vec;

const OWNER: u64 = 1;
const GAME: GameId = 10;
const VERSION: VersionId = 1;
const INSTANCE: InstanceId = 100;
const ACTOR: ActorId = 42;
const MACHINE_DOOR: MachineId = 7;
const ACTION_OPEN_DOOR: ActionId = 9;
const STATE_CLOSED: StateId = 1;
const STATE_OPEN: StateId = 2;
const VAR_HAS_KEY: VariableId = 10;
const VAR_STARS: VariableId = 11;
const VAR_DOOR_OPEN: VariableId = 12;

type MaxUriBytes = <Test as Config>::MaxUriBytes;
type MaxManifestChunkBytes = <Test as Config>::MaxManifestChunkBytes;
type MaxActionPayloadBytes = <Test as Config>::MaxActionPayloadBytes;
type MaxAttestedPayloadBytes = <Test as Config>::MaxAttestedPayloadBytes;
type MaxStatesPerMachine = <Test as Config>::MaxStatesPerMachine;
type MaxMachinesPerManifest = <Test as Config>::MaxMachinesPerManifest;
type MaxVariablesPerManifest = <Test as Config>::MaxVariablesPerManifest;
type MaxActionsPerManifest = <Test as Config>::MaxActionsPerManifest;
type MaxTransitionsPerManifest = <Test as Config>::MaxTransitionsPerManifest;
type MaxConditionClauses = <Test as Config>::MaxConditionClauses;
type MaxConditionsPerTransition = <Test as Config>::MaxConditionsPerTransition;
type MaxEconomyGateClauses = <Test as Config>::MaxEconomyGateClauses;
type MaxEffectsPerTransition = <Test as Config>::MaxEffectsPerTransition;
type MaxEventsPerManifest = <Test as Config>::MaxEventsPerManifest;
type MaxEventEffectPolicies = <Test as Config>::MaxEventEffectPolicies;
type MaxAttestedEffectsPerEvent = <Test as Config>::MaxAttestedEffectsPerEvent;
type MaxAuthorityEvents = <Test as pallet_eterra_authority::Config>::MaxAllowedEventsPerAuthority;

fn bvec<Value, Limit: Get<u32>>(values: Vec<Value>) -> BoundedVec<Value, Limit> {
    BoundedVec::try_from(values).unwrap_or_else(|_| panic!("test vector should fit bound"))
}

fn uri(bytes: &[u8]) -> BoundedVec<u8, MaxUriBytes> {
    bvec(bytes.to_vec())
}

fn empty_payload() -> BoundedVec<u8, MaxActionPayloadBytes> {
    BoundedVec::default()
}

fn empty_attested_payload() -> BoundedVec<u8, MaxAttestedPayloadBytes> {
    BoundedVec::default()
}

fn empty_attested_effects() -> BoundedVec<AttestedEffect<Test>, MaxAttestedEffectsPerEvent> {
    BoundedVec::default()
}

fn states(values: Vec<StateId>) -> BoundedVec<StateId, MaxStatesPerMachine> {
    bvec(values)
}

fn machine(
    machine_id: MachineId,
    initial_state: StateId,
    states: Vec<StateId>,
) -> MachineDefinition<Test> {
    MachineDefinition::<Test> {
        machine_id,
        initial_state,
        states: self::states(states),
    }
}

fn variable(
    variable_id: VariableId,
    scope: VariableScope,
    value_type: ValueType,
) -> VariableDefinition {
    VariableDefinition {
        variable_id,
        scope,
        value_type,
        min: None,
        max: None,
    }
}

fn variable_ref(scope: Scope, variable_id: VariableId) -> VariableRef {
    VariableRef { scope, variable_id }
}

fn manifest(
    game_id: GameId,
    version_id: VersionId,
    machines: Vec<MachineDefinition<Test>>,
    variables: Vec<VariableDefinition>,
    actions: Vec<ActionId>,
    transitions: Vec<Transition<Test>>,
) -> Manifest<Test> {
    manifest_with_events(
        game_id,
        version_id,
        machines,
        variables,
        actions,
        transitions,
        vec![],
    )
}

fn manifest_with_events(
    game_id: GameId,
    version_id: VersionId,
    machines: Vec<MachineDefinition<Test>>,
    variables: Vec<VariableDefinition>,
    actions: Vec<ActionId>,
    transitions: Vec<Transition<Test>>,
    event_definitions: Vec<EventDefinition<Test>>,
) -> Manifest<Test> {
    Manifest::<Test> {
        manifest_version: 0,
        game_id,
        version_id,
        machines: bvec::<_, MaxMachinesPerManifest>(machines),
        variables: bvec::<_, MaxVariablesPerManifest>(variables),
        actions: bvec::<_, MaxActionsPerManifest>(actions),
        transitions: bvec::<_, MaxTransitionsPerManifest>(transitions),
        event_definitions: bvec::<_, MaxEventsPerManifest>(event_definitions),
    }
}

fn all_atoms(atoms: Vec<ConditionAtom>) -> Condition<Test> {
    Condition::All(bvec::<_, MaxConditionClauses>(atoms))
}

fn any_atoms(atoms: Vec<ConditionAtom>) -> Condition<Test> {
    Condition::Any(bvec::<_, MaxConditionClauses>(atoms))
}

fn conditions(
    values: Vec<Condition<Test>>,
) -> BoundedVec<Condition<Test>, MaxConditionsPerTransition> {
    bvec(values)
}

fn effects(values: Vec<Effect>) -> BoundedVec<Effect, MaxEffectsPerTransition> {
    bvec(values)
}

fn event_definition(
    event_type: EventTypeId,
    policies: Vec<AttestedEffectPolicy>,
) -> EventDefinition<Test> {
    EventDefinition::<Test> {
        event_type,
        policies: bvec::<_, MaxEventEffectPolicies>(policies),
    }
}

fn attested_effects(
    values: Vec<AttestedEffect<Test>>,
) -> BoundedVec<AttestedEffect<Test>, MaxAttestedEffectsPerEvent> {
    bvec(values)
}

fn authority_events(values: Vec<EventTypeId>) -> BoundedVec<EventTypeId, MaxAuthorityEvents> {
    bvec(values)
}

fn transition(
    transition_id: TransitionId,
    machine_id: MachineId,
    action_id: ActionId,
    from_state: Option<StateId>,
    to_state: Option<StateId>,
    priority: u16,
    conditions: Vec<Condition<Test>>,
    economy_gate: EconomyGate<Test>,
    effects: Vec<Effect>,
) -> Transition<Test> {
    Transition::<Test> {
        transition_id,
        machine_id,
        action_id,
        from_state,
        to_state,
        priority,
        conditions: self::conditions(conditions),
        economy_gate,
        effects: self::effects(effects),
    }
}

fn upload_manifest(game_id: GameId, version_id: VersionId, manifest: Manifest<Test>) -> H256 {
    upload_manifest_bytes(game_id, version_id, manifest.encode())
}

fn upload_manifest_bytes(game_id: GameId, version_id: VersionId, encoded: Vec<u8>) -> H256 {
    let manifest_hash = <Test as frame_system::Config>::Hashing::hash(&encoded);
    assert_ok!(EterraFlow::upload_version_chunk(
        RuntimeOrigin::signed(OWNER),
        game_id,
        version_id,
        0,
        bvec::<_, MaxManifestChunkBytes>(encoded),
    ));
    manifest_hash
}

fn create_game(game_id: GameId) {
    assert_ok!(EterraFlow::create_game(
        RuntimeOrigin::signed(OWNER),
        game_id,
        H256::repeat_byte(1),
        uri(b"ipfs://game"),
    ));
}

fn setup_active_instance(manifest: Manifest<Test>) {
    create_game(manifest.game_id);
    let game_id = manifest.game_id;
    let version_id = manifest.version_id;
    let manifest_hash = upload_manifest(game_id, version_id, manifest);
    assert_ok!(EterraFlow::finalize_version(
        RuntimeOrigin::signed(OWNER),
        game_id,
        version_id,
        manifest_hash,
    ));
    assert_ok!(EterraFlow::activate_version(
        RuntimeOrigin::signed(OWNER),
        game_id,
        version_id,
    ));
    assert_ok!(EterraFlow::create_instance(
        RuntimeOrigin::signed(ACTOR),
        game_id,
        INSTANCE,
        Some(version_id),
        H256::repeat_byte(2),
    ));
}

fn raw_submit_action_call_with_payload(payload_len: usize) -> Vec<u8> {
    let mut encoded = 5u8.encode();
    encoded.extend(GAME.encode());
    encoded.extend(INSTANCE.encode());
    encoded.extend(ACTOR.encode());
    encoded.extend(MACHINE_DOOR.encode());
    encoded.extend(ACTION_OPEN_DOOR.encode());
    encoded.extend(0u64.encode());
    encoded.extend(vec![0u8; payload_len].encode());
    encoded
}

fn raw_submit_attested_event_call_with_payload(payload_len: usize) -> Vec<u8> {
    let mut encoded = 6u8.encode();
    encoded.extend(GAME.encode());
    encoded.extend(INSTANCE.encode());
    encoded.extend(EVENT_MATCH_FINALIZED.encode());
    encoded.extend(0u64.encode());
    encoded.extend(vec![0u8; payload_len].encode());
    encoded.extend(Option::<H256>::None.encode());
    encoded.extend(Vec::<AttestedEffect<Test>>::new().encode());
    encoded
}

fn zelda_manifest() -> Manifest<Test> {
    let has_key = variable_ref(Scope::Actor(ACTOR), VAR_HAS_KEY);
    let stars = variable_ref(Scope::Actor(ACTOR), VAR_STARS);
    let door_open = variable_ref(Scope::Instance, VAR_DOOR_OPEN);

    manifest(
        GAME,
        VERSION,
        vec![machine(
            MACHINE_DOOR,
            STATE_CLOSED,
            vec![STATE_CLOSED, STATE_OPEN],
        )],
        vec![
            variable(VAR_HAS_KEY, VariableScope::Actor, ValueType::Bool),
            variable(VAR_STARS, VariableScope::Actor, ValueType::U64),
            variable(VAR_DOOR_OPEN, VariableScope::Instance, ValueType::Bool),
        ],
        vec![ACTION_OPEN_DOOR],
        vec![transition(
            1,
            MACHINE_DOOR,
            ACTION_OPEN_DOOR,
            Some(STATE_CLOSED),
            Some(STATE_OPEN),
            0,
            vec![any_atoms(vec![
                ConditionAtom::VarEquals(has_key, Value::Bool(true)),
                ConditionAtom::VarGreaterOrEqual(stars, 25),
            ])],
            EconomyGate::Free,
            vec![Effect::SetVar(door_open, Value::Bool(true))],
        )],
    )
}

const SERVER_ACCOUNT: u64 = 500;
const FPS_AUTHORITY: AuthorityId = 900;
const EVENT_MATCH_FINALIZED: EventTypeId = 8;
const PASSPORT_FPS_WINS: PassportFieldId = 123;
const PASSPORT_FPS_BADGE: PassportBadgeId = 321;
const MACHINE_MATCH: MachineId = 44;
const STATE_MATCH_ACTIVE: StateId = 1;
const STATE_MATCH_FINALIZED: StateId = 2;

fn fps_attested_manifest() -> Manifest<Test> {
    manifest_with_events(
        GAME,
        VERSION,
        vec![machine(
            MACHINE_MATCH,
            STATE_MATCH_ACTIVE,
            vec![STATE_MATCH_ACTIVE, STATE_MATCH_FINALIZED],
        )],
        vec![],
        vec![1],
        vec![],
        vec![event_definition(
            EVENT_MATCH_FINALIZED,
            vec![
                AttestedEffectPolicy::UpdatePassportCounter {
                    field_id: PASSPORT_FPS_WINS,
                    amount: 1,
                },
                AttestedEffectPolicy::GrantPassportBadge {
                    badge_id: PASSPORT_FPS_BADGE,
                },
                AttestedEffectPolicy::SetMachineState {
                    scope: Scope::Instance,
                    machine_id: MACHINE_MATCH,
                    state_id: STATE_MATCH_FINALIZED,
                },
            ],
        )],
    )
}

fn authorize_server(
    event_type: EventTypeId,
    version_id: Option<VersionId>,
    expires_at: Option<u64>,
) {
    assert_ok!(EterraAuthority::authorize_authority(
        RuntimeOrigin::root(),
        GAME,
        FPS_AUTHORITY,
        SERVER_ACCOUNT,
        pallet_eterra_authority::AuthorityKind::GameServer,
        version_id,
        authority_events(vec![event_type]),
        expires_at,
        H256::repeat_byte(9),
    ));
}

#[test]
fn zelda_key_or_star_door_flow_opens() {
    new_test_ext().execute_with(|| {
        setup_active_instance(zelda_manifest());
        let has_key = variable_ref(Scope::Actor(ACTOR), VAR_HAS_KEY);
        let stars = variable_ref(Scope::Actor(ACTOR), VAR_STARS);
        let door_open = variable_ref(Scope::Instance, VAR_DOOR_OPEN);

        VariableValues::<Test>::insert(
            (GAME, INSTANCE, has_key.scope, has_key.variable_id),
            Value::Bool(true),
        );
        VariableValues::<Test>::insert(
            (GAME, INSTANCE, stars.scope, stars.variable_id),
            Value::U64(0),
        );

        assert_ok!(EterraFlow::submit_action(
            RuntimeOrigin::signed(ACTOR),
            GAME,
            INSTANCE,
            ACTOR,
            MACHINE_DOOR,
            ACTION_OPEN_DOOR,
            0,
            empty_payload(),
        ));

        assert_eq!(ActorNonces::<Test>::get((GAME, INSTANCE, ACTOR)), 1);
        assert_eq!(
            EterraFlow::value(GAME, INSTANCE, &door_open),
            Some(Value::Bool(true))
        );
        let stored_manifest = Manifests::<Test>::get(GAME, VERSION).expect("manifest stored");
        assert_eq!(
            EterraFlow::machine_state(
                &stored_manifest,
                GAME,
                INSTANCE,
                Scope::Actor(ACTOR),
                MACHINE_DOOR,
            ),
            Some(STATE_OPEN)
        );
    });
}

#[test]
fn storage_version_is_declared_without_migration_requirement() {
    new_test_ext().execute_with(|| {
        assert_eq!(
            Pallet::<Test>::in_code_storage_version(),
            StorageVersion::new(2)
        );
    });
}

#[test]
fn zelda_door_without_key_or_stars_does_not_increment_nonce() {
    new_test_ext().execute_with(|| {
        setup_active_instance(zelda_manifest());
        let stars = variable_ref(Scope::Actor(ACTOR), VAR_STARS);
        VariableValues::<Test>::insert(
            (GAME, INSTANCE, stars.scope, stars.variable_id),
            Value::U64(0),
        );

        assert_noop!(
            EterraFlow::submit_action(
                RuntimeOrigin::signed(ACTOR),
                GAME,
                INSTANCE,
                ACTOR,
                MACHINE_DOOR,
                ACTION_OPEN_DOOR,
                0,
                empty_payload(),
            ),
            Error::<Test>::NoMatchingTransition
        );
        assert_eq!(ActorNonces::<Test>::get((GAME, INSTANCE, ACTOR)), 0);
    });
}

#[test]
fn season_pass_reward_claims_once() {
    new_test_ext().execute_with(|| {
        const ACTION_CLAIM_REWARD: ActionId = 50;
        const MACHINE_SEASON: MachineId = 5;
        const STATE_ACTIVE: StateId = 1;
        const PASS: EntitlementId = 77;
        const VAR_REWARD_CLAIMED: VariableId = 88;
        const ITEM_RARE_SKIN: ItemId = 99;

        let claimed = variable_ref(Scope::Actor(ACTOR), VAR_REWARD_CLAIMED);
        let manifest = manifest(
            GAME,
            VERSION,
            vec![machine(MACHINE_SEASON, STATE_ACTIVE, vec![STATE_ACTIVE])],
            vec![variable(
                VAR_REWARD_CLAIMED,
                VariableScope::Actor,
                ValueType::Bool,
            )],
            vec![ACTION_CLAIM_REWARD],
            vec![transition(
                3,
                MACHINE_SEASON,
                ACTION_CLAIM_REWARD,
                Some(STATE_ACTIVE),
                Some(STATE_ACTIVE),
                0,
                vec![Condition::Not(ConditionAtom::VarEquals(
                    claimed,
                    Value::Bool(true),
                ))],
                EconomyGate::RequiresEntitlement {
                    entitlement_id: PASS,
                },
                vec![
                    Effect::SetVar(claimed, Value::Bool(true)),
                    Effect::GrantItem {
                        actor_id: ACTOR,
                        item_id: ITEM_RARE_SKIN,
                        amount: 1,
                    },
                ],
            )],
        );

        setup_active_instance(manifest);
        assert_ok!(EterraEconomy::create_product(
            RuntimeOrigin::root(),
            GAME,
            700,
            pallet_eterra_economy::ProductType::SeasonPass,
            500,
            Some(PASS),
            None,
            H256::repeat_byte(7),
        ));
        assert_ok!(EterraEconomy::set_product_status(
            RuntimeOrigin::root(),
            GAME,
            700,
            pallet_eterra_economy::ProductStatus::Active,
        ));
        assert_ok!(EterraEconomy::fulfill_product(
            RuntimeOrigin::root(),
            GAME,
            700,
            ACTOR,
            H256::repeat_byte(8),
        ));
        assert!(EterraEconomy::has_entitlement(&ACTOR, GAME, PASS));
        assert_ok!(EterraFlow::submit_action(
            RuntimeOrigin::signed(ACTOR),
            GAME,
            INSTANCE,
            ACTOR,
            MACHINE_SEASON,
            ACTION_CLAIM_REWARD,
            0,
            empty_payload(),
        ));
        assert_eq!(
            EterraFlow::item_balance(GAME, INSTANCE, ACTOR, ITEM_RARE_SKIN),
            1
        );

        assert_noop!(
            EterraFlow::submit_action(
                RuntimeOrigin::signed(ACTOR),
                GAME,
                INSTANCE,
                ACTOR,
                MACHINE_SEASON,
                ACTION_CLAIM_REWARD,
                1,
                empty_payload(),
            ),
            Error::<Test>::NoMatchingTransition
        );
        assert_eq!(
            EterraFlow::item_balance(GAME, INSTANCE, ACTOR, ITEM_RARE_SKIN),
            1
        );
        assert_eq!(ActorNonces::<Test>::get((GAME, INSTANCE, ACTOR)), 1);
    });
}

#[test]
fn sponsored_action_spends_pool_and_exhaustion_rolls_back() {
    new_test_ext().execute_with(|| {
        const ACTION_START_MATCH: ActionId = 31;
        const MACHINE_MATCH: MachineId = 2;
        const STATE_READY: StateId = 1;
        const STATE_ACTIVE: StateId = 2;

        let manifest = manifest(
            GAME,
            VERSION,
            vec![machine(
                MACHINE_MATCH,
                STATE_READY,
                vec![STATE_READY, STATE_ACTIVE],
            )],
            vec![],
            vec![ACTION_START_MATCH],
            vec![transition(
                2,
                MACHINE_MATCH,
                ACTION_START_MATCH,
                Some(STATE_READY),
                Some(STATE_ACTIVE),
                0,
                vec![],
                EconomyGate::DeveloperSponsored { amount: 3 },
                vec![],
            )],
        );

        setup_active_instance(manifest);
        assert_ok!(EterraEconomy::try_deposit_sponsor_funds(GAME, 5));
        assert_ok!(EterraFlow::submit_action(
            RuntimeOrigin::signed(ACTOR),
            GAME,
            INSTANCE,
            ACTOR,
            MACHINE_MATCH,
            ACTION_START_MATCH,
            0,
            empty_payload(),
        ));
        assert_eq!(pallet_eterra_economy::SponsorPools::<Test>::get(GAME), 2);
        assert_eq!(ActorNonces::<Test>::get((GAME, INSTANCE, ACTOR)), 1);

        assert_noop!(
            EterraFlow::submit_action(
                RuntimeOrigin::signed(ACTOR),
                GAME,
                INSTANCE,
                ACTOR,
                MACHINE_MATCH,
                ACTION_START_MATCH,
                1,
                empty_payload(),
            ),
            Error::<Test>::NoMatchingTransition
        );
        assert_eq!(pallet_eterra_economy::SponsorPools::<Test>::get(GAME), 2);
        assert_eq!(ActorNonces::<Test>::get((GAME, INSTANCE, ACTOR)), 1);
    });

    new_test_ext().execute_with(|| {
        const ACTION_START_MATCH: ActionId = 31;
        const MACHINE_MATCH: MachineId = 2;
        const STATE_READY: StateId = 1;
        const STATE_ACTIVE: StateId = 2;

        let manifest = manifest(
            GAME,
            VERSION,
            vec![machine(
                MACHINE_MATCH,
                STATE_READY,
                vec![STATE_READY, STATE_ACTIVE],
            )],
            vec![],
            vec![ACTION_START_MATCH],
            vec![transition(
                2,
                MACHINE_MATCH,
                ACTION_START_MATCH,
                Some(STATE_READY),
                Some(STATE_ACTIVE),
                0,
                vec![],
                EconomyGate::DeveloperSponsored { amount: 3 },
                vec![],
            )],
        );

        setup_active_instance(manifest);
        assert_ok!(EterraEconomy::try_deposit_sponsor_funds(GAME, 2));
        assert_noop!(
            EterraFlow::submit_action(
                RuntimeOrigin::signed(ACTOR),
                GAME,
                INSTANCE,
                ACTOR,
                MACHINE_MATCH,
                ACTION_START_MATCH,
                0,
                empty_payload(),
            ),
            pallet_eterra_economy::Error::<Test>::InsufficientSponsorFunds
        );
        assert_eq!(pallet_eterra_economy::SponsorPools::<Test>::get(GAME), 2);
        assert_eq!(ActorNonces::<Test>::get((GAME, INSTANCE, ACTOR)), 0);
    });
}

#[test]
fn arcade_credit_success_consumes_credit() {
    new_test_ext().execute_with(|| {
        const ACTION_START_RUN: ActionId = 70;
        const MACHINE_ARCADE: MachineId = 12;
        const STATE_IDLE: StateId = 1;
        const STATE_RUNNING: StateId = 2;
        const ARCADE_CREDIT: CreditTypeId = 55;

        let manifest = manifest(
            GAME,
            VERSION,
            vec![machine(
                MACHINE_ARCADE,
                STATE_IDLE,
                vec![STATE_IDLE, STATE_RUNNING],
            )],
            vec![],
            vec![ACTION_START_RUN],
            vec![transition(
                4,
                MACHINE_ARCADE,
                ACTION_START_RUN,
                Some(STATE_IDLE),
                Some(STATE_RUNNING),
                0,
                vec![],
                EconomyGate::ConsumesCredit {
                    credit_type: ARCADE_CREDIT,
                    amount: 1,
                },
                vec![],
            )],
        );

        setup_active_instance(manifest);
        assert_ok!(EterraEconomy::try_grant_credit(
            &ACTOR,
            GAME,
            ARCADE_CREDIT,
            1
        ));
        assert_ok!(EterraFlow::submit_action(
            RuntimeOrigin::signed(ACTOR),
            GAME,
            INSTANCE,
            ACTOR,
            MACHINE_ARCADE,
            ACTION_START_RUN,
            0,
            empty_payload(),
        ));
        assert_eq!(
            EterraEconomy::credit_balance(&ACTOR, GAME, ARCADE_CREDIT),
            0
        );
        assert_eq!(ActorNonces::<Test>::get((GAME, INSTANCE, ACTOR)), 1);
    });
}

#[test]
fn failed_effect_rolls_back_consumed_credit_and_nonce() {
    new_test_ext().execute_with(|| {
        const ACTION_START_RUN: ActionId = 70;
        const MACHINE_ARCADE: MachineId = 12;
        const STATE_IDLE: StateId = 1;
        const STATE_RUNNING: StateId = 2;
        const ARCADE_CREDIT: CreditTypeId = 55;
        const VAR_LIVES: VariableId = 66;

        let lives = variable_ref(Scope::Actor(ACTOR), VAR_LIVES);
        let manifest = manifest(
            GAME,
            VERSION,
            vec![machine(
                MACHINE_ARCADE,
                STATE_IDLE,
                vec![STATE_IDLE, STATE_RUNNING],
            )],
            vec![variable(VAR_LIVES, VariableScope::Actor, ValueType::U64)],
            vec![ACTION_START_RUN],
            vec![transition(
                4,
                MACHINE_ARCADE,
                ACTION_START_RUN,
                Some(STATE_IDLE),
                Some(STATE_RUNNING),
                0,
                vec![],
                EconomyGate::ConsumesCredit {
                    credit_type: ARCADE_CREDIT,
                    amount: 1,
                },
                vec![Effect::DecVar(lives, 1)],
            )],
        );

        setup_active_instance(manifest);
        assert_ok!(EterraEconomy::try_grant_credit(
            &ACTOR,
            GAME,
            ARCADE_CREDIT,
            1
        ));

        assert_noop!(
            EterraFlow::submit_action(
                RuntimeOrigin::signed(ACTOR),
                GAME,
                INSTANCE,
                ACTOR,
                MACHINE_ARCADE,
                ACTION_START_RUN,
                0,
                empty_payload(),
            ),
            Error::<Test>::Underflow
        );

        assert_eq!(
            EterraEconomy::credit_balance(&ACTOR, GAME, ARCADE_CREDIT),
            1
        );
        assert_eq!(ActorNonces::<Test>::get((GAME, INSTANCE, ACTOR)), 0);
    });
}

#[test]
fn failed_effect_rolls_back_sponsor_funds() {
    new_test_ext().execute_with(|| {
        const ACTION_START_RUN: ActionId = 71;
        const MACHINE_RUN: MachineId = 13;
        const STATE_IDLE: StateId = 1;
        const STATE_RUNNING: StateId = 2;
        const VAR_LIVES: VariableId = 67;

        let lives = variable_ref(Scope::Actor(ACTOR), VAR_LIVES);
        let manifest = manifest(
            GAME,
            VERSION,
            vec![machine(
                MACHINE_RUN,
                STATE_IDLE,
                vec![STATE_IDLE, STATE_RUNNING],
            )],
            vec![variable(VAR_LIVES, VariableScope::Actor, ValueType::U64)],
            vec![ACTION_START_RUN],
            vec![transition(
                5,
                MACHINE_RUN,
                ACTION_START_RUN,
                Some(STATE_IDLE),
                Some(STATE_RUNNING),
                0,
                vec![],
                EconomyGate::DeveloperSponsored { amount: 3 },
                vec![Effect::DecVar(lives, 1)],
            )],
        );

        setup_active_instance(manifest);
        assert_ok!(EterraEconomy::try_deposit_sponsor_funds(GAME, 10));

        assert_noop!(
            EterraFlow::submit_action(
                RuntimeOrigin::signed(ACTOR),
                GAME,
                INSTANCE,
                ACTOR,
                MACHINE_RUN,
                ACTION_START_RUN,
                0,
                empty_payload(),
            ),
            Error::<Test>::Underflow
        );

        assert_eq!(pallet_eterra_economy::SponsorPools::<Test>::get(GAME), 10);
        assert_eq!(ActorNonces::<Test>::get((GAME, INSTANCE, ACTOR)), 0);
    });
}

#[test]
fn ambiguous_manifest_is_rejected_at_finalization() {
    new_test_ext().execute_with(|| {
        let manifest = manifest(
            GAME,
            VERSION,
            vec![machine(MACHINE_DOOR, STATE_CLOSED, vec![STATE_CLOSED])],
            vec![],
            vec![ACTION_OPEN_DOOR],
            vec![
                transition(
                    1,
                    MACHINE_DOOR,
                    ACTION_OPEN_DOOR,
                    Some(STATE_CLOSED),
                    None,
                    0,
                    vec![],
                    EconomyGate::Free,
                    vec![],
                ),
                transition(
                    2,
                    MACHINE_DOOR,
                    ACTION_OPEN_DOOR,
                    Some(STATE_CLOSED),
                    None,
                    0,
                    vec![],
                    EconomyGate::Free,
                    vec![],
                ),
            ],
        );

        create_game(GAME);
        let manifest_hash = upload_manifest(GAME, VERSION, manifest);
        assert_noop!(
            EterraFlow::finalize_version(
                RuntimeOrigin::signed(OWNER),
                GAME,
                VERSION,
                manifest_hash,
            ),
            Error::<Test>::AmbiguousTransition
        );
    });
}

#[test]
fn manifest_with_unknown_variable_is_rejected_at_finalization() {
    new_test_ext().execute_with(|| {
        let missing_var = variable_ref(Scope::Actor(ACTOR), VAR_HAS_KEY);
        let manifest = manifest(
            GAME,
            VERSION,
            vec![machine(MACHINE_DOOR, STATE_CLOSED, vec![STATE_CLOSED])],
            vec![],
            vec![ACTION_OPEN_DOOR],
            vec![transition(
                1,
                MACHINE_DOOR,
                ACTION_OPEN_DOOR,
                Some(STATE_CLOSED),
                None,
                0,
                vec![all_atoms(vec![ConditionAtom::VarEquals(
                    missing_var,
                    Value::Bool(true),
                )])],
                EconomyGate::Free,
                vec![],
            )],
        );

        create_game(GAME);
        let manifest_hash = upload_manifest(GAME, VERSION, manifest);
        assert_noop!(
            EterraFlow::finalize_version(
                RuntimeOrigin::signed(OWNER),
                GAME,
                VERSION,
                manifest_hash,
            ),
            Error::<Test>::UnknownVariable
        );
    });
}

#[derive(Encode)]
struct RawManifest {
    manifest_version: u16,
    game_id: GameId,
    version_id: VersionId,
    machines: Vec<MachineDefinition<Test>>,
    variables: Vec<VariableDefinition>,
    actions: Vec<ActionId>,
    transitions: Vec<Transition<Test>>,
    event_definitions: Vec<EventDefinition<Test>>,
}

#[derive(Encode)]
struct RawMachineDefinition {
    machine_id: MachineId,
    initial_state: StateId,
    states: Vec<StateId>,
}

#[derive(Encode)]
#[allow(dead_code)]
enum RawCondition {
    All(Vec<ConditionAtom>),
    Any(Vec<ConditionAtom>),
    Not(ConditionAtom),
    Atom(ConditionAtom),
}

#[derive(Encode)]
#[allow(dead_code)]
enum RawEconomyGate {
    Free,
    DeveloperSponsored {
        amount: u128,
    },
    RequiresPayment {
        amount: u128,
    },
    RequiresEntitlement {
        entitlement_id: EntitlementId,
    },
    ConsumesCredit {
        credit_type: CreditTypeId,
        amount: u64,
    },
    All(Vec<EconomyGateAtom>),
    Any(Vec<EconomyGateAtom>),
}

#[derive(Encode)]
struct RawTransition {
    transition_id: TransitionId,
    machine_id: MachineId,
    action_id: ActionId,
    from_state: Option<StateId>,
    to_state: Option<StateId>,
    priority: u16,
    conditions: Vec<RawCondition>,
    economy_gate: RawEconomyGate,
    effects: Vec<Effect>,
}

#[derive(Encode)]
struct RawEventDefinition {
    event_type: EventTypeId,
    policies: Vec<AttestedEffectPolicy>,
}

#[derive(Encode)]
struct RawBoundedManifest {
    manifest_version: u16,
    game_id: GameId,
    version_id: VersionId,
    machines: Vec<RawMachineDefinition>,
    variables: Vec<VariableDefinition>,
    actions: Vec<ActionId>,
    transitions: Vec<RawTransition>,
    event_definitions: Vec<RawEventDefinition>,
}

fn raw_machine(
    machine_id: MachineId,
    initial_state: StateId,
    states: Vec<StateId>,
) -> RawMachineDefinition {
    RawMachineDefinition {
        machine_id,
        initial_state,
        states,
    }
}

fn raw_transition(
    transition_id: TransitionId,
    conditions: Vec<RawCondition>,
    economy_gate: RawEconomyGate,
    effects: Vec<Effect>,
) -> RawTransition {
    RawTransition {
        transition_id,
        machine_id: MACHINE_DOOR,
        action_id: ACTION_OPEN_DOOR,
        from_state: Some(STATE_CLOSED),
        to_state: Some(STATE_OPEN),
        priority: transition_id as u16,
        conditions,
        economy_gate,
        effects,
    }
}

fn raw_bounded_manifest(
    machines: Vec<RawMachineDefinition>,
    variables: Vec<VariableDefinition>,
    actions: Vec<ActionId>,
    transitions: Vec<RawTransition>,
    event_definitions: Vec<RawEventDefinition>,
) -> RawBoundedManifest {
    RawBoundedManifest {
        manifest_version: 0,
        game_id: GAME,
        version_id: VERSION,
        machines,
        variables,
        actions,
        transitions,
        event_definitions,
    }
}

fn assert_manifest_decode_fails(raw: RawBoundedManifest) {
    let encoded = raw.encode();
    let mut input = encoded.as_slice();
    assert!(Manifest::<Test>::decode(&mut input).is_err());
}

#[test]
fn over_limit_manifest_fails_decode_during_finalization() {
    new_test_ext().execute_with(|| {
        let raw = RawManifest {
            manifest_version: 0,
            game_id: GAME,
            version_id: VERSION,
            machines: vec![machine(MACHINE_DOOR, STATE_CLOSED, vec![STATE_CLOSED])],
            variables: vec![],
            actions: vec![ACTION_OPEN_DOOR],
            transitions: vec![
                transition(
                    1,
                    MACHINE_DOOR,
                    ACTION_OPEN_DOOR,
                    Some(STATE_CLOSED),
                    None,
                    0,
                    vec![],
                    EconomyGate::Free,
                    vec![],
                ),
                transition(
                    2,
                    MACHINE_DOOR,
                    ACTION_OPEN_DOOR,
                    Some(STATE_CLOSED),
                    None,
                    1,
                    vec![],
                    EconomyGate::Free,
                    vec![],
                ),
                transition(
                    3,
                    MACHINE_DOOR,
                    ACTION_OPEN_DOOR,
                    Some(STATE_CLOSED),
                    None,
                    2,
                    vec![],
                    EconomyGate::Free,
                    vec![],
                ),
            ],
            event_definitions: vec![],
        };

        create_game(GAME);
        let manifest_hash = upload_manifest_bytes(GAME, VERSION, raw.encode());
        assert_noop!(
            EterraFlow::finalize_version(
                RuntimeOrigin::signed(OWNER),
                GAME,
                VERSION,
                manifest_hash,
            ),
            Error::<Test>::ManifestDecodeFailed
        );
    });
}

#[test]
fn canonical_manifest_hash_is_exact_scale_bytes() {
    new_test_ext().execute_with(|| {
        let manifest = zelda_manifest();
        let encoded = manifest.encode();
        let canonical_hash = EterraFlow::canonical_manifest_hash(&manifest);
        assert_eq!(
            canonical_hash,
            <Test as frame_system::Config>::Hashing::hash(&encoded)
        );

        create_game(GAME);
        let manifest_hash = upload_manifest_bytes(GAME, VERSION, encoded);
        assert_eq!(manifest_hash, canonical_hash);
        assert_ok!(EterraFlow::finalize_version(
            RuntimeOrigin::signed(OWNER),
            GAME,
            VERSION,
            canonical_hash,
        ));
    });
}

#[test]
fn sdk_generated_manifest_templates_finalize() {
    new_test_ext().execute_with(|| {
        let templates = [
            eterra_manifest_builder::templates::zelda_door(),
            eterra_manifest_builder::templates::arcade_credit_run(),
            eterra_manifest_builder::templates::season_pass_reward(),
            eterra_manifest_builder::templates::fps_attested_result(),
        ];

        for template in templates {
            let compiled = eterra_manifest_builder::compile_manifest(
                template,
                eterra_manifest_builder::CompilerLimits::default(),
            )
            .expect("SDK template should compile");
            let mut input = compiled.scale_bytes.as_slice();
            Manifest::<Test>::decode(&mut input).expect("SDK SCALE bytes decode as Flow manifest");
            assert!(input.is_empty());

            create_game(compiled.runtime_manifest.game_id);
            let manifest_hash = upload_manifest_bytes(
                compiled.runtime_manifest.game_id,
                compiled.runtime_manifest.version_id,
                compiled.scale_bytes.clone(),
            );
            assert_eq!(
                manifest_hash,
                compiled.manifest_hash_with(<Test as frame_system::Config>::Hashing::hash)
            );
            assert_ok!(EterraFlow::finalize_version(
                RuntimeOrigin::signed(OWNER),
                compiled.runtime_manifest.game_id,
                compiled.runtime_manifest.version_id,
                manifest_hash,
            ));
        }
    });
}

#[test]
fn finalize_rejects_wrong_hash_trailing_bytes_and_malformed_scale() {
    new_test_ext().execute_with(|| {
        create_game(GAME);
        let manifest_hash = upload_manifest(GAME, VERSION, zelda_manifest());
        assert_ne!(manifest_hash, H256::repeat_byte(99));
        assert_noop!(
            EterraFlow::finalize_version(
                RuntimeOrigin::signed(OWNER),
                GAME,
                VERSION,
                H256::repeat_byte(99),
            ),
            Error::<Test>::ManifestHashMismatch
        );
    });

    new_test_ext().execute_with(|| {
        create_game(GAME);
        let mut encoded = zelda_manifest().encode();
        encoded.push(0);
        let manifest_hash = upload_manifest_bytes(GAME, VERSION, encoded);
        assert_noop!(
            EterraFlow::finalize_version(
                RuntimeOrigin::signed(OWNER),
                GAME,
                VERSION,
                manifest_hash,
            ),
            Error::<Test>::ManifestDecodeFailed
        );
    });

    new_test_ext().execute_with(|| {
        create_game(GAME);
        let manifest_hash = upload_manifest_bytes(GAME, VERSION, vec![0xff]);
        assert_noop!(
            EterraFlow::finalize_version(
                RuntimeOrigin::signed(OWNER),
                GAME,
                VERSION,
                manifest_hash,
            ),
            Error::<Test>::ManifestDecodeFailed
        );
    });
}

#[test]
fn finalize_rejects_manifest_game_and_version_mismatches() {
    new_test_ext().execute_with(|| {
        let mut manifest = zelda_manifest();
        manifest.game_id = GAME + 1;
        create_game(GAME);
        let manifest_hash = upload_manifest(GAME, VERSION, manifest);
        assert_noop!(
            EterraFlow::finalize_version(
                RuntimeOrigin::signed(OWNER),
                GAME,
                VERSION,
                manifest_hash,
            ),
            Error::<Test>::ManifestGameMismatch
        );
    });

    new_test_ext().execute_with(|| {
        let mut manifest = zelda_manifest();
        manifest.version_id = VERSION + 1;
        create_game(GAME);
        let manifest_hash = upload_manifest(GAME, VERSION, manifest);
        assert_noop!(
            EterraFlow::finalize_version(
                RuntimeOrigin::signed(OWNER),
                GAME,
                VERSION,
                manifest_hash,
            ),
            Error::<Test>::ManifestVersionMismatch
        );
    });
}

#[test]
fn manifest_validator_rejects_contract_violations() {
    new_test_ext().execute_with(|| {
        let valid_machine = machine(MACHINE_DOOR, STATE_CLOSED, vec![STATE_CLOSED, STATE_OPEN]);
        let valid_var = variable(VAR_HAS_KEY, VariableScope::Actor, ValueType::Bool);
        let u64_var = variable(VAR_STARS, VariableScope::Actor, ValueType::U64);
        let bool_ref = variable_ref(Scope::Actor(ACTOR), VAR_HAS_KEY);
        let u64_ref = variable_ref(Scope::Actor(ACTOR), VAR_STARS);

        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![
                        valid_machine.clone(),
                        machine(MACHINE_DOOR, STATE_CLOSED, vec![STATE_CLOSED])
                    ],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::DuplicateMachine
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![machine(
                        MACHINE_DOOR,
                        STATE_CLOSED,
                        vec![STATE_CLOSED, STATE_CLOSED]
                    )],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::DuplicateState
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![
                        valid_var,
                        variable(VAR_HAS_KEY, VariableScope::Actor, ValueType::Bool)
                    ],
                    vec![ACTION_OPEN_DOOR],
                    vec![],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::DuplicateVariable
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![],
                    vec![ACTION_OPEN_DOOR, ACTION_OPEN_DOOR],
                    vec![],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::DuplicateAction
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![
                        transition(
                            1,
                            MACHINE_DOOR,
                            ACTION_OPEN_DOOR,
                            Some(STATE_CLOSED),
                            None,
                            0,
                            vec![],
                            EconomyGate::Free,
                            vec![],
                        ),
                        transition(
                            1,
                            MACHINE_DOOR,
                            ACTION_OPEN_DOOR,
                            Some(STATE_CLOSED),
                            None,
                            1,
                            vec![],
                            EconomyGate::Free,
                            vec![],
                        ),
                    ],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::DuplicateTransition
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![transition(
                        1,
                        MACHINE_DOOR + 1,
                        ACTION_OPEN_DOOR,
                        Some(STATE_CLOSED),
                        None,
                        0,
                        vec![],
                        EconomyGate::Free,
                        vec![],
                    )],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::UnknownMachine
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![transition(
                        1,
                        MACHINE_DOOR,
                        ACTION_OPEN_DOOR + 1,
                        Some(STATE_CLOSED),
                        None,
                        0,
                        vec![],
                        EconomyGate::Free,
                        vec![],
                    )],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::UnknownAction
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![transition(
                        1,
                        MACHINE_DOOR,
                        ACTION_OPEN_DOOR,
                        Some(999),
                        None,
                        0,
                        vec![],
                        EconomyGate::Free,
                        vec![],
                    )],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::UnknownState
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![variable(VAR_HAS_KEY, VariableScope::Actor, ValueType::Bool)],
                    vec![ACTION_OPEN_DOOR],
                    vec![transition(
                        1,
                        MACHINE_DOOR,
                        ACTION_OPEN_DOOR,
                        Some(STATE_CLOSED),
                        None,
                        0,
                        vec![Condition::Atom(ConditionAtom::VarGreaterOrEqual(
                            bool_ref, 1
                        ))],
                        EconomyGate::Free,
                        vec![],
                    )],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::ValueTypeMismatch
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![u64_var],
                    vec![ACTION_OPEN_DOOR],
                    vec![transition(
                        1,
                        MACHINE_DOOR,
                        ACTION_OPEN_DOOR,
                        Some(STATE_CLOSED),
                        None,
                        0,
                        vec![],
                        EconomyGate::Free,
                        vec![Effect::SetVar(u64_ref, Value::Bool(true))],
                    )],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::ValueTypeMismatch
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![transition(
                        1,
                        MACHINE_DOOR,
                        ACTION_OPEN_DOOR,
                        Some(STATE_CLOSED),
                        None,
                        0,
                        vec![Condition::All(BoundedVec::default())],
                        EconomyGate::Free,
                        vec![],
                    )],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::InvalidCondition
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![transition(
                        1,
                        MACHINE_DOOR,
                        ACTION_OPEN_DOOR,
                        Some(STATE_CLOSED),
                        None,
                        0,
                        vec![Condition::Atom(ConditionAtom::HasCredit {
                            actor_id: ACTOR,
                            credit_type: 1,
                            amount: 0,
                        })],
                        EconomyGate::Free,
                        vec![],
                    )],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::InvalidCondition
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![transition(
                        1,
                        MACHINE_DOOR,
                        ACTION_OPEN_DOOR,
                        Some(STATE_CLOSED),
                        None,
                        0,
                        vec![],
                        EconomyGate::DeveloperSponsored { amount: 0 },
                        vec![],
                    )],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::InvalidEconomyGate
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![variable(VAR_STARS, VariableScope::Actor, ValueType::U64)],
                    vec![ACTION_OPEN_DOOR],
                    vec![transition(
                        1,
                        MACHINE_DOOR,
                        ACTION_OPEN_DOOR,
                        Some(STATE_CLOSED),
                        None,
                        0,
                        vec![],
                        EconomyGate::Free,
                        vec![Effect::IncVar(u64_ref, 0)],
                    )],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::InvalidEffect
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest_with_events(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![],
                    vec![event_definition(1, vec![]), event_definition(1, vec![])],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::DuplicateEvent
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest_with_events(
                    GAME,
                    VERSION,
                    vec![valid_machine.clone()],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![],
                    vec![event_definition(
                        1,
                        vec![AttestedEffectPolicy::UpdatePassportCounter {
                            field_id: 1,
                            amount: 0,
                        }],
                    )],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::InvalidAttestedEffect
        );
        assert_noop!(
            EterraFlow::validate_manifest(
                &manifest_with_events(
                    GAME,
                    VERSION,
                    vec![valid_machine],
                    vec![],
                    vec![ACTION_OPEN_DOOR],
                    vec![],
                    vec![event_definition(
                        1,
                        vec![AttestedEffectPolicy::SetMachineState {
                            scope: Scope::Instance,
                            machine_id: MACHINE_DOOR,
                            state_id: 999,
                        }],
                    )],
                ),
                GAME,
                VERSION,
            ),
            Error::<Test>::UnknownState
        );
    });
}

#[test]
fn manifest_bounded_vectors_reject_max_plus_one_at_decode() {
    new_test_ext().execute_with(|| {
        let valid_machine = raw_machine(MACHINE_DOOR, STATE_CLOSED, vec![STATE_CLOSED, STATE_OPEN]);
        let valid_action = vec![ACTION_OPEN_DOOR];

        assert_manifest_decode_fails(raw_bounded_manifest(
            (0..=MaxMachinesPerManifest::get())
                .map(|id| raw_machine(id, STATE_CLOSED, vec![STATE_CLOSED]))
                .collect(),
            vec![],
            valid_action.clone(),
            vec![],
            vec![],
        ));
        assert_manifest_decode_fails(raw_bounded_manifest(
            vec![raw_machine(
                MACHINE_DOOR,
                0,
                (0..=MaxStatesPerMachine::get()).collect(),
            )],
            vec![],
            valid_action.clone(),
            vec![],
            vec![],
        ));
        assert_manifest_decode_fails(raw_bounded_manifest(
            vec![valid_machine],
            (0..=MaxVariablesPerManifest::get())
                .map(|id| variable(id, VariableScope::Actor, ValueType::Bool))
                .collect(),
            valid_action.clone(),
            vec![],
            vec![],
        ));
        assert_manifest_decode_fails(raw_bounded_manifest(
            vec![raw_machine(MACHINE_DOOR, STATE_CLOSED, vec![STATE_CLOSED])],
            vec![],
            (0..=MaxActionsPerManifest::get()).collect(),
            vec![],
            vec![],
        ));
        assert_manifest_decode_fails(raw_bounded_manifest(
            vec![raw_machine(
                MACHINE_DOOR,
                STATE_CLOSED,
                vec![STATE_CLOSED, STATE_OPEN],
            )],
            vec![],
            valid_action.clone(),
            (0..=MaxTransitionsPerManifest::get())
                .map(|id| raw_transition(id as TransitionId, vec![], RawEconomyGate::Free, vec![]))
                .collect(),
            vec![],
        ));
        assert_manifest_decode_fails(raw_bounded_manifest(
            vec![raw_machine(
                MACHINE_DOOR,
                STATE_CLOSED,
                vec![STATE_CLOSED, STATE_OPEN],
            )],
            vec![],
            valid_action.clone(),
            vec![raw_transition(
                1,
                (0..=MaxConditionsPerTransition::get())
                    .map(|_| {
                        RawCondition::Atom(ConditionAtom::HasEntitlement {
                            actor_id: ACTOR,
                            entitlement_id: 1,
                        })
                    })
                    .collect(),
                RawEconomyGate::Free,
                vec![],
            )],
            vec![],
        ));
        assert_manifest_decode_fails(raw_bounded_manifest(
            vec![raw_machine(
                MACHINE_DOOR,
                STATE_CLOSED,
                vec![STATE_CLOSED, STATE_OPEN],
            )],
            vec![],
            valid_action.clone(),
            vec![raw_transition(
                1,
                vec![RawCondition::All(
                    (0..=MaxConditionClauses::get())
                        .map(|_| ConditionAtom::HasEntitlement {
                            actor_id: ACTOR,
                            entitlement_id: 1,
                        })
                        .collect(),
                )],
                RawEconomyGate::Free,
                vec![],
            )],
            vec![],
        ));
        assert_manifest_decode_fails(raw_bounded_manifest(
            vec![raw_machine(
                MACHINE_DOOR,
                STATE_CLOSED,
                vec![STATE_CLOSED, STATE_OPEN],
            )],
            vec![],
            valid_action.clone(),
            vec![raw_transition(
                1,
                vec![],
                RawEconomyGate::All(
                    (0..=MaxEconomyGateClauses::get())
                        .map(|_| EconomyGateAtom::Free)
                        .collect(),
                ),
                vec![],
            )],
            vec![],
        ));
        assert_manifest_decode_fails(raw_bounded_manifest(
            vec![raw_machine(
                MACHINE_DOOR,
                STATE_CLOSED,
                vec![STATE_CLOSED, STATE_OPEN],
            )],
            vec![],
            valid_action.clone(),
            vec![raw_transition(
                1,
                vec![],
                RawEconomyGate::Free,
                (0..=MaxEffectsPerTransition::get())
                    .map(|_| Effect::GrantItem {
                        actor_id: ACTOR,
                        item_id: 1,
                        amount: 1,
                    })
                    .collect(),
            )],
            vec![],
        ));
        assert_manifest_decode_fails(raw_bounded_manifest(
            vec![raw_machine(MACHINE_DOOR, STATE_CLOSED, vec![STATE_CLOSED])],
            vec![],
            valid_action.clone(),
            vec![],
            (0..=MaxEventsPerManifest::get())
                .map(|event_type| RawEventDefinition {
                    event_type,
                    policies: vec![],
                })
                .collect(),
        ));
        assert_manifest_decode_fails(raw_bounded_manifest(
            vec![raw_machine(MACHINE_DOOR, STATE_CLOSED, vec![STATE_CLOSED])],
            vec![],
            valid_action,
            vec![],
            vec![RawEventDefinition {
                event_type: 1,
                policies: (0..=MaxEventEffectPolicies::get())
                    .map(|_| AttestedEffectPolicy::GrantPassportBadge { badge_id: 1 })
                    .collect(),
            }],
        ));

        let encoded = (0..=MaxAttestedEffectsPerEvent::get())
            .map(|_| AttestedEffect::<Test>::GrantPassportBadge {
                account: OWNER,
                badge_id: 1,
            })
            .collect::<Vec<_>>()
            .encode();
        let mut input = encoded.as_slice();
        assert!(
            BoundedVec::<AttestedEffect<Test>, MaxAttestedEffectsPerEvent>::decode(&mut input)
                .is_err()
        );
    });
}

#[test]
fn raw_call_decode_rejects_oversized_action_and_attested_payloads() {
    new_test_ext().execute_with(|| {
        let action_encoded = raw_submit_action_call_with_payload(
            <MaxActionPayloadBytes as Get<u32>>::get()
                .checked_add(1)
                .expect("test limit should not overflow") as usize,
        );
        let mut action_input = action_encoded.as_slice();
        assert!(crate::pallet::Call::<Test>::decode(&mut action_input).is_err());

        let attested_encoded = raw_submit_attested_event_call_with_payload(
            <MaxAttestedPayloadBytes as Get<u32>>::get()
                .checked_add(1)
                .expect("test limit should not overflow") as usize,
        );
        let mut attested_input = attested_encoded.as_slice();
        assert!(crate::pallet::Call::<Test>::decode(&mut attested_input).is_err());
    });
}

#[test]
fn explicit_transition_priority_selects_lowest_priority_match() {
    new_test_ext().execute_with(|| {
        let manifest = manifest(
            GAME,
            VERSION,
            vec![machine(
                MACHINE_DOOR,
                STATE_CLOSED,
                vec![STATE_CLOSED, STATE_OPEN, 3],
            )],
            vec![],
            vec![ACTION_OPEN_DOOR],
            vec![
                transition(
                    22,
                    MACHINE_DOOR,
                    ACTION_OPEN_DOOR,
                    Some(STATE_CLOSED),
                    Some(STATE_OPEN),
                    1,
                    vec![],
                    EconomyGate::Free,
                    vec![],
                ),
                transition(
                    11,
                    MACHINE_DOOR,
                    ACTION_OPEN_DOOR,
                    Some(STATE_CLOSED),
                    Some(3),
                    0,
                    vec![],
                    EconomyGate::Free,
                    vec![],
                ),
            ],
        );
        setup_active_instance(manifest);

        assert_ok!(EterraFlow::submit_action(
            RuntimeOrigin::signed(ACTOR),
            GAME,
            INSTANCE,
            ACTOR,
            MACHINE_DOOR,
            ACTION_OPEN_DOOR,
            0,
            empty_payload(),
        ));
        assert_eq!(
            MachineStates::<Test>::get((GAME, INSTANCE, Scope::Actor(ACTOR), MACHINE_DOOR)),
            Some(3)
        );
        System::assert_last_event(RuntimeEvent::EterraFlow(Event::ActionSubmitted {
            game_id: GAME,
            instance_id: INSTANCE,
            actor_id: ACTOR,
            machine_id: MACHINE_DOOR,
            action_id: ACTION_OPEN_DOOR,
            transition_id: 11,
            nonce: 1,
        }));
    });
}

#[test]
fn max_configured_manifest_shape_can_finalize_when_encoded() {
    new_test_ext().execute_with(|| {
        let machines = (0..MaxMachinesPerManifest::get())
            .map(|machine_id| machine(machine_id, 0, (0..MaxStatesPerMachine::get()).collect()))
            .collect::<Vec<_>>();
        let variables = (0..MaxVariablesPerManifest::get())
            .map(|variable_id| variable(variable_id, VariableScope::Actor, ValueType::U64))
            .collect::<Vec<_>>();
        let actions = (0..MaxActionsPerManifest::get()).collect::<Vec<_>>();
        let u64_ref = variable_ref(Scope::Actor(ACTOR), 0);
        let transitions = (0..MaxTransitionsPerManifest::get())
            .map(|index| {
                transition(
                    index as TransitionId,
                    0,
                    index,
                    Some(0),
                    Some(1),
                    0,
                    (0..MaxConditionsPerTransition::get())
                        .map(|_| Condition::Atom(ConditionAtom::VarGreaterOrEqual(u64_ref, 0)))
                        .collect(),
                    EconomyGate::Free,
                    (0..MaxEffectsPerTransition::get())
                        .map(|_| Effect::IncVar(u64_ref, 1))
                        .collect(),
                )
            })
            .collect::<Vec<_>>();

        let manifest = manifest(GAME, VERSION, machines, variables, actions, transitions);
        create_game(GAME);
        let manifest_hash = upload_manifest(GAME, VERSION, manifest);
        assert_ok!(EterraFlow::finalize_version(
            RuntimeOrigin::signed(OWNER),
            GAME,
            VERSION,
            manifest_hash,
        ));
        assert!(Manifests::<Test>::contains_key(GAME, VERSION));
    });
}

#[test]
fn invalid_manifest_finalization_does_not_activate_version() {
    new_test_ext().execute_with(|| {
        let invalid = manifest(
            GAME,
            VERSION,
            vec![machine(MACHINE_DOOR, STATE_CLOSED, vec![STATE_CLOSED])],
            vec![],
            vec![ACTION_OPEN_DOOR, ACTION_OPEN_DOOR],
            vec![],
        );
        create_game(GAME);
        let manifest_hash = upload_manifest(GAME, VERSION, invalid);
        assert_noop!(
            EterraFlow::finalize_version(
                RuntimeOrigin::signed(OWNER),
                GAME,
                VERSION,
                manifest_hash,
            ),
            Error::<Test>::DuplicateAction
        );
        assert_noop!(
            EterraFlow::activate_version(RuntimeOrigin::signed(OWNER), GAME, VERSION),
            Error::<Test>::VersionNotFinalized
        );
        assert!(!Manifests::<Test>::contains_key(GAME, VERSION));
    });
}

#[test]
fn failed_profile_effect_rolls_back_counter_and_nonce() {
    new_test_ext().execute_with(|| {
        let stars_ref = variable_ref(Scope::Actor(ACTOR), VAR_STARS);
        let manifest = manifest(
            GAME,
            VERSION,
            vec![machine(
                MACHINE_DOOR,
                STATE_CLOSED,
                vec![STATE_CLOSED, STATE_OPEN],
            )],
            vec![variable(VAR_STARS, VariableScope::Actor, ValueType::U64)],
            vec![ACTION_OPEN_DOOR],
            vec![transition(
                1,
                MACHINE_DOOR,
                ACTION_OPEN_DOOR,
                Some(STATE_CLOSED),
                Some(STATE_OPEN),
                0,
                vec![],
                EconomyGate::Free,
                vec![
                    Effect::UpdatePassportCounter {
                        actor_id: ACTOR,
                        field_id: PASSPORT_FPS_WINS,
                        amount: 1,
                    },
                    Effect::DecVar(stars_ref, 1),
                ],
            )],
        );
        setup_active_instance(manifest);

        assert_noop!(
            EterraFlow::submit_action(
                RuntimeOrigin::signed(ACTOR),
                GAME,
                INSTANCE,
                ACTOR,
                MACHINE_DOOR,
                ACTION_OPEN_DOOR,
                0,
                empty_payload(),
            ),
            Error::<Test>::Underflow
        );
        assert_eq!(EterraProfile::counter(&ACTOR, PASSPORT_FPS_WINS), 0);
        assert_eq!(ActorNonces::<Test>::get((GAME, INSTANCE, ACTOR)), 0);
        assert_eq!(
            MachineStates::<Test>::get((GAME, INSTANCE, Scope::Actor(ACTOR), MACHINE_DOOR)),
            None
        );
    });
}

#[test]
fn unauthorized_attested_event_is_rejected() {
    new_test_ext().execute_with(|| {
        setup_active_instance(fps_attested_manifest());
        assert_noop!(
            EterraFlow::submit_attested_event(
                RuntimeOrigin::signed(SERVER_ACCOUNT),
                GAME,
                INSTANCE,
                EVENT_MATCH_FINALIZED,
                0,
                empty_attested_payload(),
                None,
                empty_attested_effects(),
            ),
            Error::<Test>::UnauthorizedAuthority
        );
    });
}

#[test]
fn authorized_attested_fps_result_updates_passport_and_sequence() {
    new_test_ext().execute_with(|| {
        setup_active_instance(fps_attested_manifest());
        authorize_server(EVENT_MATCH_FINALIZED, Some(VERSION), None);
        let replay_hash = H256::repeat_byte(7);

        assert_ok!(EterraFlow::submit_attested_event(
            RuntimeOrigin::signed(SERVER_ACCOUNT),
            GAME,
            INSTANCE,
            EVENT_MATCH_FINALIZED,
            0,
            empty_attested_payload(),
            Some(replay_hash),
            attested_effects(vec![
                AttestedEffect::UpdatePassportCounter {
                    account: ACTOR,
                    field_id: PASSPORT_FPS_WINS,
                    amount: 1,
                },
                AttestedEffect::GrantPassportBadge {
                    account: ACTOR,
                    badge_id: PASSPORT_FPS_BADGE,
                },
                AttestedEffect::SetMachineState {
                    scope: Scope::Instance,
                    machine_id: MACHINE_MATCH,
                    state_id: STATE_MATCH_FINALIZED,
                },
            ]),
        ));

        assert_eq!(EterraProfile::counter(&ACTOR, PASSPORT_FPS_WINS), 1);
        assert!(EterraProfile::has_badge(&ACTOR, PASSPORT_FPS_BADGE));
        assert_eq!(
            AttestedSequences::<Test>::get((GAME, INSTANCE, FPS_AUTHORITY, EVENT_MATCH_FINALIZED)),
            1
        );
        assert_eq!(
            AttestedReplayHashes::<Test>::get((
                GAME,
                INSTANCE,
                FPS_AUTHORITY,
                EVENT_MATCH_FINALIZED,
                0
            )),
            Some(replay_hash)
        );
        let stored_manifest = Manifests::<Test>::get(GAME, VERSION).expect("manifest stored");
        assert_eq!(
            EterraFlow::machine_state(
                &stored_manifest,
                GAME,
                INSTANCE,
                Scope::Instance,
                MACHINE_MATCH,
            ),
            Some(STATE_MATCH_FINALIZED)
        );
        System::assert_last_event(RuntimeEvent::EterraFlow(Event::AttestedEventAccepted {
            game_id: GAME,
            instance_id: INSTANCE,
            authority_id: FPS_AUTHORITY,
            event_type: EVENT_MATCH_FINALIZED,
            next_sequence: 1,
            replay_hash: Some(replay_hash),
        }));

        assert_noop!(
            EterraFlow::submit_attested_event(
                RuntimeOrigin::signed(SERVER_ACCOUNT),
                GAME,
                INSTANCE,
                EVENT_MATCH_FINALIZED,
                0,
                empty_attested_payload(),
                None,
                empty_attested_effects(),
            ),
            Error::<Test>::SequenceMismatch
        );
        assert_noop!(
            EterraFlow::submit_attested_event(
                RuntimeOrigin::signed(SERVER_ACCOUNT),
                GAME,
                INSTANCE,
                EVENT_MATCH_FINALIZED,
                2,
                empty_attested_payload(),
                None,
                empty_attested_effects(),
            ),
            Error::<Test>::SequenceMismatch
        );
        assert_eq!(EterraProfile::counter(&ACTOR, PASSPORT_FPS_WINS), 1);
    });
}

#[test]
fn attested_authority_status_expiry_event_type_and_version_are_enforced() {
    new_test_ext().execute_with(|| {
        setup_active_instance(fps_attested_manifest());
        authorize_server(EVENT_MATCH_FINALIZED, Some(VERSION), None);
        assert_ok!(EterraAuthority::set_authority_status(
            RuntimeOrigin::root(),
            GAME,
            FPS_AUTHORITY,
            pallet_eterra_authority::AuthorityStatus::Revoked,
        ));
        assert_noop!(
            EterraFlow::submit_attested_event(
                RuntimeOrigin::signed(SERVER_ACCOUNT),
                GAME,
                INSTANCE,
                EVENT_MATCH_FINALIZED,
                0,
                empty_attested_payload(),
                None,
                empty_attested_effects(),
            ),
            Error::<Test>::UnauthorizedAuthority
        );
    });

    new_test_ext().execute_with(|| {
        setup_active_instance(fps_attested_manifest());
        authorize_server(EVENT_MATCH_FINALIZED, Some(VERSION), Some(1));
        assert_noop!(
            EterraFlow::submit_attested_event(
                RuntimeOrigin::signed(SERVER_ACCOUNT),
                GAME,
                INSTANCE,
                EVENT_MATCH_FINALIZED,
                0,
                empty_attested_payload(),
                None,
                empty_attested_effects(),
            ),
            Error::<Test>::UnauthorizedAuthority
        );
    });

    new_test_ext().execute_with(|| {
        setup_active_instance(fps_attested_manifest());
        authorize_server(EVENT_MATCH_FINALIZED + 1, Some(VERSION), None);
        assert_noop!(
            EterraFlow::submit_attested_event(
                RuntimeOrigin::signed(SERVER_ACCOUNT),
                GAME,
                INSTANCE,
                EVENT_MATCH_FINALIZED,
                0,
                empty_attested_payload(),
                None,
                empty_attested_effects(),
            ),
            Error::<Test>::UnauthorizedAuthority
        );
    });

    new_test_ext().execute_with(|| {
        setup_active_instance(fps_attested_manifest());
        authorize_server(EVENT_MATCH_FINALIZED, Some(VERSION + 1), None);
        assert_noop!(
            EterraFlow::submit_attested_event(
                RuntimeOrigin::signed(SERVER_ACCOUNT),
                GAME,
                INSTANCE,
                EVENT_MATCH_FINALIZED,
                0,
                empty_attested_payload(),
                None,
                empty_attested_effects(),
            ),
            Error::<Test>::UnauthorizedAuthority
        );
    });
}

#[test]
fn attested_effect_not_allowed_by_manifest_is_rejected() {
    new_test_ext().execute_with(|| {
        setup_active_instance(fps_attested_manifest());
        authorize_server(EVENT_MATCH_FINALIZED, Some(VERSION), None);

        assert_noop!(
            EterraFlow::submit_attested_event(
                RuntimeOrigin::signed(SERVER_ACCOUNT),
                GAME,
                INSTANCE,
                EVENT_MATCH_FINALIZED,
                0,
                empty_attested_payload(),
                None,
                attested_effects(vec![AttestedEffect::UpdatePassportCounter {
                    account: ACTOR,
                    field_id: PASSPORT_FPS_WINS,
                    amount: 2,
                }]),
            ),
            Error::<Test>::InvalidAttestedEffect
        );
        assert_eq!(EterraProfile::counter(&ACTOR, PASSPORT_FPS_WINS), 0);
        assert_eq!(
            AttestedSequences::<Test>::get((GAME, INSTANCE, FPS_AUTHORITY, EVENT_MATCH_FINALIZED)),
            0
        );
    });
}
