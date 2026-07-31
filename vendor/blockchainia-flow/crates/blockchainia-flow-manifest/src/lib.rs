//! Blockchainia Flow v0 authoring compiler and locked SCALE wire model.
//!
//! The preferred authoring label (`blockchainia.flow.v0`) and the permanent
//! compatibility alias (`eterra.flow.v0`) both lower to runtime manifest
//! version `0`. Labels never enter the SCALE payload.

use blake2::{
    digest::{consts::U32, Digest},
    Blake2b,
};
use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
};

pub type GameId = u64;
pub type VersionId = u32;
pub type InstanceId = u64;
pub type ActorId = u64;
pub type MachineId = u32;
pub type StateId = u32;
pub type ActionId = u32;
pub type TransitionId = u64;
pub type VariableId = u32;
pub type ItemId = u32;
pub type EntitlementId = u32;
pub type CreditTypeId = u32;
pub type EventTypeId = u32;
pub type PassportFieldId = u32;
pub type PassportBadgeId = u32;

pub const AUTHORING_LABEL: &str = "blockchainia.flow.v0";
pub const ETERRA_AUTHORING_ALIAS: &str = "eterra.flow.v0";
pub const RUNTIME_MANIFEST_VERSION: u16 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompilerDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub path: String,
    pub message: String,
}

impl fmt::Display for CompilerDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} {} at {}: {}",
            self.severity, self.code, self.path, self.message
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompilerLimits {
    pub max_manifest_bytes: usize,
    pub max_manifest_chunks: usize,
    pub max_manifest_chunk_bytes: usize,
    pub max_action_payload_bytes: usize,
    pub max_attested_payload_bytes: usize,
    pub max_machines_per_manifest: usize,
    pub max_states_per_machine: usize,
    pub max_variables_per_manifest: usize,
    pub max_actions_per_manifest: usize,
    pub max_transitions_per_manifest: usize,
    pub max_conditions_per_transition: usize,
    pub max_condition_clauses: usize,
    pub max_economy_gate_clauses: usize,
    pub max_effects_per_transition: usize,
    pub max_events_per_manifest: usize,
    pub max_attested_effects_per_event: usize,
    pub max_event_effect_policies: usize,
}

impl CompilerLimits {
    pub const fn production() -> Self {
        Self {
            max_manifest_bytes: 4 * 1024 * 1024,
            max_manifest_chunks: 64,
            max_manifest_chunk_bytes: 64 * 1024,
            max_action_payload_bytes: 1024,
            max_attested_payload_bytes: 4096,
            max_machines_per_manifest: 256,
            max_states_per_machine: 1024,
            max_variables_per_manifest: 4096,
            max_actions_per_manifest: 4096,
            max_transitions_per_manifest: 20_000,
            max_conditions_per_transition: 64,
            max_condition_clauses: 64,
            max_economy_gate_clauses: 16,
            max_effects_per_transition: 64,
            max_events_per_manifest: 256,
            max_attested_effects_per_event: 32,
            max_event_effect_policies: 64,
        }
    }
}

impl Default for CompilerLimits {
    fn default() -> Self {
        Self::production()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestMetrics {
    pub scale_bytes: usize,
    pub manifest_chunks: usize,
    pub machines: usize,
    pub states: usize,
    pub variables: usize,
    pub actions: usize,
    pub transitions: usize,
    pub event_definitions: usize,
    pub attested_policies: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSummary {
    pub machines: Vec<MachineGraph>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineGraph {
    pub machine_id: MachineId,
    pub initial_state: StateId,
    pub states: Vec<StateId>,
    pub transitions: Vec<TransitionEdge>,
    pub reachable_states: Vec<StateId>,
    pub unreachable_states: Vec<StateId>,
    pub unreachable_transitions: Vec<TransitionId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionEdge {
    pub transition_id: TransitionId,
    pub action_id: ActionId,
    pub from_state: Option<StateId>,
    pub to_state: Option<StateId>,
    pub priority: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimate {
    pub subject: CostSubject,
    pub storage_reads: u32,
    pub storage_writes: u32,
    pub authority_provider_calls: u32,
    pub economy_provider_calls: u32,
    pub profile_provider_calls: u32,
    pub condition_atoms: u32,
    pub effects: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CostSubject {
    Transition(TransitionId),
    AttestedEvent(EventTypeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledManifest {
    pub canonical_authoring: AuthoringManifest,
    pub runtime_manifest: RuntimeManifestV0,
    pub scale_bytes: Vec<u8>,
    pub manifest_hash: [u8; 32],
    pub metrics: ManifestMetrics,
    pub diagnostics: Vec<CompilerDiagnostic>,
    pub graph: GraphSummary,
    pub cost_estimates: Vec<CostEstimate>,
}

impl CompiledManifest {
    pub fn scale_hex(&self) -> String {
        format!("0x{}", hex::encode(&self.scale_bytes))
    }

    pub fn manifest_hash_hex(&self) -> String {
        format!("0x{}", hex::encode(self.manifest_hash))
    }

    pub fn manifest_hash_with<Hash>(&self, hash: impl FnOnce(&[u8]) -> Hash) -> Hash {
        hash(&self.scale_bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringManifest {
    pub manifest_version: String,
    pub game_id: GameId,
    pub version_id: VersionId,
    pub machines: Vec<AuthoringMachine>,
    pub variables: Vec<AuthoringVariable>,
    pub actions: Vec<ActionId>,
    pub transitions: Vec<AuthoringTransition>,
    pub event_definitions: Vec<AuthoringEventDefinition>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringMachine {
    pub machine_id: MachineId,
    pub initial_state: StateId,
    pub states: Vec<StateId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringVariable {
    pub variable_id: VariableId,
    pub scope: VariableScopeKind,
    #[serde(rename = "type")]
    pub value_type: ValueType,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VariableScopeKind {
    Game,
    Instance,
    Actor,
    Entity,
    Passport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueType {
    Bool,
    U64,
    I64,
    Enum,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringTransition {
    pub transition_id: TransitionId,
    pub machine_id: MachineId,
    pub action_id: ActionId,
    pub from_state: Option<StateId>,
    pub to_state: Option<StateId>,
    pub priority: u16,
    pub economy_gate: AuthoringEconomyGate,
    pub conditions: Vec<AuthoringCondition>,
    pub effects: Vec<AuthoringEffect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringEventDefinition {
    pub event_type: EventTypeId,
    pub policies: Vec<AuthoringAttestedEffectPolicy>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthoringScope {
    Game { game: bool },
    Instance { instance: bool },
    Actor { actor: ActorId },
    Entity { entity: u64 },
    Passport { passport: ActorId },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringVariableRef {
    pub scope: AuthoringScope,
    pub variable_id: VariableId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthoringValue {
    Bool {
        #[serde(rename = "bool")]
        value: bool,
    },
    U64 {
        #[serde(rename = "u64")]
        value: u64,
    },
    I64 {
        #[serde(rename = "i64")]
        value: i64,
    },
    Enum {
        #[serde(rename = "enum")]
        value: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthoringCondition {
    All { all: Vec<AuthoringConditionAtom> },
    Any { any: Vec<AuthoringConditionAtom> },
    Not { not: AuthoringConditionAtom },
    Atom { atom: AuthoringConditionAtom },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthoringConditionAtom {
    VarEquals {
        var_equals: AuthoringVarEquals,
    },
    VarGreaterOrEqual {
        var_gte: AuthoringVarCompare,
    },
    VarLessOrEqual {
        var_lte: AuthoringVarCompare,
    },
    HasItem {
        has_item: AuthoringItemAmount,
    },
    HasCredit {
        has_credit: AuthoringCreditAmount,
    },
    HasEntitlement {
        has_entitlement: AuthoringEntitlement,
    },
    MachineStateEquals {
        machine_state_equals: AuthoringSetMachineState,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringVarEquals {
    pub variable: AuthoringVariableRef,
    pub value: AuthoringValue,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringVarCompare {
    pub variable: AuthoringVariableRef,
    pub value: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringItemAmount {
    pub actor_id: ActorId,
    pub item_id: ItemId,
    pub amount: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringCreditAmount {
    pub actor_id: ActorId,
    pub credit_type: CreditTypeId,
    pub amount: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringEntitlement {
    pub actor_id: ActorId,
    pub entitlement_id: EntitlementId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthoringEconomyGate {
    Free {
        free: EmptyObject,
    },
    DeveloperSponsored {
        developer_sponsored: AmountU128,
    },
    RequiresPayment {
        requires_payment: AmountU128,
    },
    RequiresEntitlement {
        requires_entitlement: EntitlementOnly,
    },
    ConsumesCredit {
        consumes_credit: CreditOnlyAmount,
    },
    All {
        all: Vec<AuthoringEconomyGateAtom>,
    },
    Any {
        any: Vec<AuthoringEconomyGateAtom>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthoringEconomyGateAtom {
    Free {
        free: EmptyObject,
    },
    DeveloperSponsored {
        developer_sponsored: AmountU128,
    },
    RequiresPayment {
        requires_payment: AmountU128,
    },
    RequiresEntitlement {
        requires_entitlement: EntitlementOnly,
    },
    ConsumesCredit {
        consumes_credit: CreditOnlyAmount,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyObject {}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmountU128 {
    pub amount: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntitlementOnly {
    pub entitlement_id: EntitlementId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditOnlyAmount {
    pub credit_type: CreditTypeId,
    pub amount: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthoringEffect {
    SetVar {
        set_var: AuthoringVarEquals,
    },
    IncVar {
        inc_var: AuthoringVarAmount,
    },
    DecVar {
        dec_var: AuthoringVarAmount,
    },
    GrantItem {
        grant_item: AuthoringItemAmount,
    },
    ConsumeItem {
        consume_item: AuthoringItemAmount,
    },
    GrantCredit {
        grant_credit: AuthoringCreditAmount,
    },
    GrantEntitlement {
        grant_entitlement: AuthoringEntitlement,
    },
    RevokeEntitlement {
        revoke_entitlement: AuthoringEntitlement,
    },
    UpdatePassportCounter {
        update_passport_counter: AuthoringPassportCounter,
    },
    GrantPassportBadge {
        grant_passport_badge: AuthoringPassportBadge,
    },
    RevokePassportBadge {
        revoke_passport_badge: AuthoringPassportBadge,
    },
    SetMachineState {
        set_machine_state: AuthoringSetMachineState,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringVarAmount {
    pub variable: AuthoringVariableRef,
    pub amount: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringPassportCounter {
    pub actor_id: ActorId,
    pub field_id: PassportFieldId,
    pub amount: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringPassportBadge {
    pub actor_id: ActorId,
    pub badge_id: PassportBadgeId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringSetMachineState {
    pub scope: AuthoringScope,
    pub machine_id: MachineId,
    pub state_id: StateId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthoringAttestedEffectPolicy {
    UpdatePassportCounter {
        update_passport_counter: AuthoringPolicyPassportCounter,
    },
    GrantPassportBadge {
        grant_passport_badge: AuthoringPolicyPassportBadge,
    },
    RevokePassportBadge {
        revoke_passport_badge: AuthoringPolicyPassportBadge,
    },
    SetMachineState {
        set_machine_state: AuthoringSetMachineState,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringPolicyPassportCounter {
    pub field_id: PassportFieldId,
    pub amount: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringPolicyPassportBadge {
    pub badge_id: PassportBadgeId,
}

// The runtime DTO order below is the locked Manifest v0 wire contract.

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct RuntimeManifestV0 {
    pub manifest_version: u16,
    pub game_id: GameId,
    pub version_id: VersionId,
    pub machines: Vec<RuntimeMachineDefinition>,
    pub variables: Vec<RuntimeVariableDefinition>,
    pub actions: Vec<ActionId>,
    pub transitions: Vec<RuntimeTransition>,
    pub event_definitions: Vec<RuntimeEventDefinition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub enum RuntimeValue {
    Bool(bool),
    U64(u64),
    I64(i64),
    Enum(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub enum RuntimeValueType {
    Bool,
    U64,
    I64,
    Enum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub enum RuntimeVariableScope {
    Game,
    Instance,
    Actor,
    Entity,
    Passport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub enum RuntimeScope {
    Game,
    Instance,
    Actor(ActorId),
    Entity(u64),
    Passport(ActorId),
}

impl RuntimeScope {
    pub const fn variable_scope(self) -> RuntimeVariableScope {
        match self {
            Self::Game => RuntimeVariableScope::Game,
            Self::Instance => RuntimeVariableScope::Instance,
            Self::Actor(_) => RuntimeVariableScope::Actor,
            Self::Entity(_) => RuntimeVariableScope::Entity,
            Self::Passport(_) => RuntimeVariableScope::Passport,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct RuntimeVariableRef {
    pub scope: RuntimeScope,
    pub variable_id: VariableId,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct RuntimeVariableDefinition {
    pub variable_id: VariableId,
    pub scope: RuntimeVariableScope,
    pub value_type: RuntimeValueType,
    pub min: Option<i64>,
    pub max: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum RuntimeConditionAtom {
    VarEquals(RuntimeVariableRef, RuntimeValue),
    VarGreaterOrEqual(RuntimeVariableRef, u64),
    VarLessOrEqual(RuntimeVariableRef, u64),
    HasItem {
        actor_id: ActorId,
        item_id: ItemId,
        amount: u64,
    },
    HasCredit {
        actor_id: ActorId,
        credit_type: CreditTypeId,
        amount: u64,
    },
    HasEntitlement {
        actor_id: ActorId,
        entitlement_id: EntitlementId,
    },
    MachineStateEquals {
        scope: RuntimeScope,
        machine_id: MachineId,
        state_id: StateId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum RuntimeCondition {
    All(Vec<RuntimeConditionAtom>),
    Any(Vec<RuntimeConditionAtom>),
    Not(RuntimeConditionAtom),
    Atom(RuntimeConditionAtom),
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum RuntimeEconomyGateAtom {
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
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum RuntimeEconomyGate {
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
    All(Vec<RuntimeEconomyGateAtom>),
    Any(Vec<RuntimeEconomyGateAtom>),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub enum RuntimeEffect {
    SetVar(RuntimeVariableRef, RuntimeValue),
    IncVar(RuntimeVariableRef, u64),
    DecVar(RuntimeVariableRef, u64),
    GrantItem {
        actor_id: ActorId,
        item_id: ItemId,
        amount: u64,
    },
    ConsumeItem {
        actor_id: ActorId,
        item_id: ItemId,
        amount: u64,
    },
    GrantCredit {
        actor_id: ActorId,
        credit_type: CreditTypeId,
        amount: u64,
    },
    GrantEntitlement {
        actor_id: ActorId,
        entitlement_id: EntitlementId,
    },
    RevokeEntitlement {
        actor_id: ActorId,
        entitlement_id: EntitlementId,
    },
    UpdatePassportCounter {
        actor_id: ActorId,
        field_id: PassportFieldId,
        amount: u64,
    },
    GrantPassportBadge {
        actor_id: ActorId,
        badge_id: PassportBadgeId,
    },
    RevokePassportBadge {
        actor_id: ActorId,
        badge_id: PassportBadgeId,
    },
    SetMachineState {
        scope: RuntimeScope,
        machine_id: MachineId,
        state_id: StateId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub enum RuntimeAttestedEffectPolicy {
    UpdatePassportCounter {
        field_id: PassportFieldId,
        amount: u64,
    },
    GrantPassportBadge {
        badge_id: PassportBadgeId,
    },
    RevokePassportBadge {
        badge_id: PassportBadgeId,
    },
    SetMachineState {
        scope: RuntimeScope,
        machine_id: MachineId,
        state_id: StateId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct RuntimeEventDefinition {
    pub event_type: EventTypeId,
    pub policies: Vec<RuntimeAttestedEffectPolicy>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct RuntimeMachineDefinition {
    pub machine_id: MachineId,
    pub initial_state: StateId,
    pub states: Vec<StateId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct RuntimeTransition {
    pub transition_id: TransitionId,
    pub machine_id: MachineId,
    pub action_id: ActionId,
    pub from_state: Option<StateId>,
    pub to_state: Option<StateId>,
    pub priority: u16,
    pub conditions: Vec<RuntimeCondition>,
    pub economy_gate: RuntimeEconomyGate,
    pub effects: Vec<RuntimeEffect>,
}

pub fn is_supported_authoring_label(label: &str) -> bool {
    label == AUTHORING_LABEL || label == ETERRA_AUTHORING_ALIAS
}

pub fn blake2_256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

pub fn compile_manifest(
    mut authoring: AuthoringManifest,
    limits: CompilerLimits,
) -> Result<CompiledManifest, Vec<CompilerDiagnostic>> {
    let mut diagnostics = Vec::new();
    if !is_supported_authoring_label(&authoring.manifest_version) {
        diagnostics.push(error(
            "unsupported_authoring_label",
            "manifest_version",
            format!("expected `{AUTHORING_LABEL}` or permanent alias `{ETERRA_AUTHORING_ALIAS}`"),
        ));
    }

    canonicalize_authoring(&mut authoring);
    let runtime_manifest = RuntimeManifestV0::from_authoring(&authoring);
    validate_manifest(&runtime_manifest, &limits, &mut diagnostics);
    let graph = build_graph(&runtime_manifest, &mut diagnostics);
    warn_duplicate_rewards(&runtime_manifest, &mut diagnostics);
    let cost_estimates = estimate_costs(&runtime_manifest);
    let scale_bytes = runtime_manifest.encode();
    let metrics = manifest_metrics(&runtime_manifest, scale_bytes.len(), &limits);

    if scale_bytes.len() > limits.max_manifest_bytes {
        diagnostics.push(error(
            "manifest_too_large",
            "manifest",
            format!(
                "encoded manifest is {} bytes; maximum is {}",
                scale_bytes.len(),
                limits.max_manifest_bytes
            ),
        ));
    }
    if metrics.manifest_chunks > limits.max_manifest_chunks {
        diagnostics.push(error(
            "too_many_manifest_chunks",
            "manifest",
            format!(
                "{} chunks are required; maximum is {}",
                metrics.manifest_chunks, limits.max_manifest_chunks
            ),
        ));
    }

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(diagnostics);
    }

    let manifest_hash = blake2_256(&scale_bytes);
    Ok(CompiledManifest {
        canonical_authoring: authoring,
        runtime_manifest,
        scale_bytes,
        manifest_hash,
        metrics,
        diagnostics,
        graph,
        cost_estimates,
    })
}

pub fn compile_manifest_json(
    bytes: &[u8],
    limits: CompilerLimits,
) -> Result<CompiledManifest, Vec<CompilerDiagnostic>> {
    match serde_json::from_slice(bytes) {
        Ok(authoring) => compile_manifest(authoring, limits),
        Err(parse_error) => Err(vec![error(
            "invalid_json",
            "manifest",
            parse_error.to_string(),
        )]),
    }
}

impl RuntimeManifestV0 {
    pub fn from_authoring(authoring: &AuthoringManifest) -> Self {
        Self {
            manifest_version: RUNTIME_MANIFEST_VERSION,
            game_id: authoring.game_id,
            version_id: authoring.version_id,
            machines: authoring
                .machines
                .iter()
                .map(|machine| RuntimeMachineDefinition {
                    machine_id: machine.machine_id,
                    initial_state: machine.initial_state,
                    states: machine.states.clone(),
                })
                .collect(),
            variables: authoring
                .variables
                .iter()
                .map(|variable| RuntimeVariableDefinition {
                    variable_id: variable.variable_id,
                    scope: runtime_variable_scope(variable.scope),
                    value_type: runtime_value_type(variable.value_type),
                    min: variable.min,
                    max: variable.max,
                })
                .collect(),
            actions: authoring.actions.clone(),
            transitions: authoring
                .transitions
                .iter()
                .map(runtime_transition)
                .collect(),
            event_definitions: authoring
                .event_definitions
                .iter()
                .map(runtime_event_definition)
                .collect(),
        }
    }
}

fn canonicalize_authoring(manifest: &mut AuthoringManifest) {
    manifest.manifest_version = AUTHORING_LABEL.to_owned();
    manifest.machines.sort_by_key(|machine| machine.machine_id);
    for machine in &mut manifest.machines {
        machine.states.sort_unstable();
    }
    manifest
        .variables
        .sort_by_key(|variable| (variable.scope, variable.variable_id));
    manifest.actions.sort_unstable();
    manifest.transitions.sort_by_key(|transition| {
        (
            transition.machine_id,
            transition.action_id,
            transition.from_state,
            transition.priority,
            transition.transition_id,
        )
    });
    manifest
        .event_definitions
        .sort_by_key(|definition| definition.event_type);
    for definition in &mut manifest.event_definitions {
        definition
            .policies
            .sort_by_key(|policy| serde_json::to_string(policy).unwrap_or_default());
    }
}

fn runtime_transition(transition: &AuthoringTransition) -> RuntimeTransition {
    RuntimeTransition {
        transition_id: transition.transition_id,
        machine_id: transition.machine_id,
        action_id: transition.action_id,
        from_state: transition.from_state,
        to_state: transition.to_state,
        priority: transition.priority,
        conditions: transition
            .conditions
            .iter()
            .map(runtime_condition)
            .collect(),
        economy_gate: runtime_economy_gate(&transition.economy_gate),
        effects: transition.effects.iter().map(runtime_effect).collect(),
    }
}

fn runtime_event_definition(definition: &AuthoringEventDefinition) -> RuntimeEventDefinition {
    RuntimeEventDefinition {
        event_type: definition.event_type,
        policies: definition.policies.iter().map(runtime_policy).collect(),
    }
}

fn runtime_scope(scope: &AuthoringScope) -> RuntimeScope {
    match scope {
        AuthoringScope::Game { .. } => RuntimeScope::Game,
        AuthoringScope::Instance { .. } => RuntimeScope::Instance,
        AuthoringScope::Actor { actor } => RuntimeScope::Actor(*actor),
        AuthoringScope::Entity { entity } => RuntimeScope::Entity(*entity),
        AuthoringScope::Passport { passport } => RuntimeScope::Passport(*passport),
    }
}

fn runtime_variable_ref(variable: &AuthoringVariableRef) -> RuntimeVariableRef {
    RuntimeVariableRef {
        scope: runtime_scope(&variable.scope),
        variable_id: variable.variable_id,
    }
}

fn runtime_value(value: &AuthoringValue) -> RuntimeValue {
    match value {
        AuthoringValue::Bool { value } => RuntimeValue::Bool(*value),
        AuthoringValue::U64 { value } => RuntimeValue::U64(*value),
        AuthoringValue::I64 { value } => RuntimeValue::I64(*value),
        AuthoringValue::Enum { value } => RuntimeValue::Enum(*value),
    }
}

fn runtime_value_type(value_type: ValueType) -> RuntimeValueType {
    match value_type {
        ValueType::Bool => RuntimeValueType::Bool,
        ValueType::U64 => RuntimeValueType::U64,
        ValueType::I64 => RuntimeValueType::I64,
        ValueType::Enum => RuntimeValueType::Enum,
    }
}

fn runtime_variable_scope(scope: VariableScopeKind) -> RuntimeVariableScope {
    match scope {
        VariableScopeKind::Game => RuntimeVariableScope::Game,
        VariableScopeKind::Instance => RuntimeVariableScope::Instance,
        VariableScopeKind::Actor => RuntimeVariableScope::Actor,
        VariableScopeKind::Entity => RuntimeVariableScope::Entity,
        VariableScopeKind::Passport => RuntimeVariableScope::Passport,
    }
}

fn runtime_condition(condition: &AuthoringCondition) -> RuntimeCondition {
    match condition {
        AuthoringCondition::All { all } => {
            RuntimeCondition::All(all.iter().map(runtime_condition_atom).collect())
        }
        AuthoringCondition::Any { any } => {
            RuntimeCondition::Any(any.iter().map(runtime_condition_atom).collect())
        }
        AuthoringCondition::Not { not } => RuntimeCondition::Not(runtime_condition_atom(not)),
        AuthoringCondition::Atom { atom } => RuntimeCondition::Atom(runtime_condition_atom(atom)),
    }
}

fn runtime_condition_atom(atom: &AuthoringConditionAtom) -> RuntimeConditionAtom {
    match atom {
        AuthoringConditionAtom::VarEquals { var_equals } => RuntimeConditionAtom::VarEquals(
            runtime_variable_ref(&var_equals.variable),
            runtime_value(&var_equals.value),
        ),
        AuthoringConditionAtom::VarGreaterOrEqual { var_gte } => {
            RuntimeConditionAtom::VarGreaterOrEqual(
                runtime_variable_ref(&var_gte.variable),
                var_gte.value,
            )
        }
        AuthoringConditionAtom::VarLessOrEqual { var_lte } => RuntimeConditionAtom::VarLessOrEqual(
            runtime_variable_ref(&var_lte.variable),
            var_lte.value,
        ),
        AuthoringConditionAtom::HasItem { has_item } => RuntimeConditionAtom::HasItem {
            actor_id: has_item.actor_id,
            item_id: has_item.item_id,
            amount: has_item.amount,
        },
        AuthoringConditionAtom::HasCredit { has_credit } => RuntimeConditionAtom::HasCredit {
            actor_id: has_credit.actor_id,
            credit_type: has_credit.credit_type,
            amount: has_credit.amount,
        },
        AuthoringConditionAtom::HasEntitlement { has_entitlement } => {
            RuntimeConditionAtom::HasEntitlement {
                actor_id: has_entitlement.actor_id,
                entitlement_id: has_entitlement.entitlement_id,
            }
        }
        AuthoringConditionAtom::MachineStateEquals {
            machine_state_equals,
        } => RuntimeConditionAtom::MachineStateEquals {
            scope: runtime_scope(&machine_state_equals.scope),
            machine_id: machine_state_equals.machine_id,
            state_id: machine_state_equals.state_id,
        },
    }
}

fn runtime_economy_gate(gate: &AuthoringEconomyGate) -> RuntimeEconomyGate {
    match gate {
        AuthoringEconomyGate::Free { .. } => RuntimeEconomyGate::Free,
        AuthoringEconomyGate::DeveloperSponsored {
            developer_sponsored,
        } => RuntimeEconomyGate::DeveloperSponsored {
            amount: developer_sponsored.amount,
        },
        AuthoringEconomyGate::RequiresPayment { requires_payment } => {
            RuntimeEconomyGate::RequiresPayment {
                amount: requires_payment.amount,
            }
        }
        AuthoringEconomyGate::RequiresEntitlement {
            requires_entitlement,
        } => RuntimeEconomyGate::RequiresEntitlement {
            entitlement_id: requires_entitlement.entitlement_id,
        },
        AuthoringEconomyGate::ConsumesCredit { consumes_credit } => {
            RuntimeEconomyGate::ConsumesCredit {
                credit_type: consumes_credit.credit_type,
                amount: consumes_credit.amount,
            }
        }
        AuthoringEconomyGate::All { all } => {
            RuntimeEconomyGate::All(all.iter().map(runtime_economy_gate_atom).collect())
        }
        AuthoringEconomyGate::Any { any } => {
            RuntimeEconomyGate::Any(any.iter().map(runtime_economy_gate_atom).collect())
        }
    }
}

fn runtime_economy_gate_atom(atom: &AuthoringEconomyGateAtom) -> RuntimeEconomyGateAtom {
    match atom {
        AuthoringEconomyGateAtom::Free { .. } => RuntimeEconomyGateAtom::Free,
        AuthoringEconomyGateAtom::DeveloperSponsored {
            developer_sponsored,
        } => RuntimeEconomyGateAtom::DeveloperSponsored {
            amount: developer_sponsored.amount,
        },
        AuthoringEconomyGateAtom::RequiresPayment { requires_payment } => {
            RuntimeEconomyGateAtom::RequiresPayment {
                amount: requires_payment.amount,
            }
        }
        AuthoringEconomyGateAtom::RequiresEntitlement {
            requires_entitlement,
        } => RuntimeEconomyGateAtom::RequiresEntitlement {
            entitlement_id: requires_entitlement.entitlement_id,
        },
        AuthoringEconomyGateAtom::ConsumesCredit { consumes_credit } => {
            RuntimeEconomyGateAtom::ConsumesCredit {
                credit_type: consumes_credit.credit_type,
                amount: consumes_credit.amount,
            }
        }
    }
}

fn runtime_effect(effect: &AuthoringEffect) -> RuntimeEffect {
    match effect {
        AuthoringEffect::SetVar { set_var } => RuntimeEffect::SetVar(
            runtime_variable_ref(&set_var.variable),
            runtime_value(&set_var.value),
        ),
        AuthoringEffect::IncVar { inc_var } => {
            RuntimeEffect::IncVar(runtime_variable_ref(&inc_var.variable), inc_var.amount)
        }
        AuthoringEffect::DecVar { dec_var } => {
            RuntimeEffect::DecVar(runtime_variable_ref(&dec_var.variable), dec_var.amount)
        }
        AuthoringEffect::GrantItem { grant_item } => RuntimeEffect::GrantItem {
            actor_id: grant_item.actor_id,
            item_id: grant_item.item_id,
            amount: grant_item.amount,
        },
        AuthoringEffect::ConsumeItem { consume_item } => RuntimeEffect::ConsumeItem {
            actor_id: consume_item.actor_id,
            item_id: consume_item.item_id,
            amount: consume_item.amount,
        },
        AuthoringEffect::GrantCredit { grant_credit } => RuntimeEffect::GrantCredit {
            actor_id: grant_credit.actor_id,
            credit_type: grant_credit.credit_type,
            amount: grant_credit.amount,
        },
        AuthoringEffect::GrantEntitlement { grant_entitlement } => {
            RuntimeEffect::GrantEntitlement {
                actor_id: grant_entitlement.actor_id,
                entitlement_id: grant_entitlement.entitlement_id,
            }
        }
        AuthoringEffect::RevokeEntitlement { revoke_entitlement } => {
            RuntimeEffect::RevokeEntitlement {
                actor_id: revoke_entitlement.actor_id,
                entitlement_id: revoke_entitlement.entitlement_id,
            }
        }
        AuthoringEffect::UpdatePassportCounter {
            update_passport_counter,
        } => RuntimeEffect::UpdatePassportCounter {
            actor_id: update_passport_counter.actor_id,
            field_id: update_passport_counter.field_id,
            amount: update_passport_counter.amount,
        },
        AuthoringEffect::GrantPassportBadge {
            grant_passport_badge,
        } => RuntimeEffect::GrantPassportBadge {
            actor_id: grant_passport_badge.actor_id,
            badge_id: grant_passport_badge.badge_id,
        },
        AuthoringEffect::RevokePassportBadge {
            revoke_passport_badge,
        } => RuntimeEffect::RevokePassportBadge {
            actor_id: revoke_passport_badge.actor_id,
            badge_id: revoke_passport_badge.badge_id,
        },
        AuthoringEffect::SetMachineState { set_machine_state } => RuntimeEffect::SetMachineState {
            scope: runtime_scope(&set_machine_state.scope),
            machine_id: set_machine_state.machine_id,
            state_id: set_machine_state.state_id,
        },
    }
}

fn runtime_policy(policy: &AuthoringAttestedEffectPolicy) -> RuntimeAttestedEffectPolicy {
    match policy {
        AuthoringAttestedEffectPolicy::UpdatePassportCounter {
            update_passport_counter,
        } => RuntimeAttestedEffectPolicy::UpdatePassportCounter {
            field_id: update_passport_counter.field_id,
            amount: update_passport_counter.amount,
        },
        AuthoringAttestedEffectPolicy::GrantPassportBadge {
            grant_passport_badge,
        } => RuntimeAttestedEffectPolicy::GrantPassportBadge {
            badge_id: grant_passport_badge.badge_id,
        },
        AuthoringAttestedEffectPolicy::RevokePassportBadge {
            revoke_passport_badge,
        } => RuntimeAttestedEffectPolicy::RevokePassportBadge {
            badge_id: revoke_passport_badge.badge_id,
        },
        AuthoringAttestedEffectPolicy::SetMachineState { set_machine_state } => {
            RuntimeAttestedEffectPolicy::SetMachineState {
                scope: runtime_scope(&set_machine_state.scope),
                machine_id: set_machine_state.machine_id,
                state_id: set_machine_state.state_id,
            }
        }
    }
}

fn validate_manifest(
    manifest: &RuntimeManifestV0,
    limits: &CompilerLimits,
    diagnostics: &mut Vec<CompilerDiagnostic>,
) {
    if manifest.manifest_version != RUNTIME_MANIFEST_VERSION {
        diagnostics.push(error(
            "unsupported_runtime_manifest",
            "manifest_version",
            format!(
                "runtime manifest version must be {RUNTIME_MANIFEST_VERSION}, got {}",
                manifest.manifest_version
            ),
        ));
    }
    if manifest.machines.is_empty() {
        diagnostics.push(error(
            "empty_machines",
            "machines",
            "at least one state machine is required",
        ));
    }
    if manifest.actions.is_empty() {
        diagnostics.push(error(
            "empty_actions",
            "actions",
            "at least one action is required",
        ));
    }

    check_limit(
        manifest.machines.len(),
        limits.max_machines_per_manifest,
        "too_many_machines",
        "machines",
        diagnostics,
    );
    check_limit(
        manifest.variables.len(),
        limits.max_variables_per_manifest,
        "too_many_variables",
        "variables",
        diagnostics,
    );
    check_limit(
        manifest.actions.len(),
        limits.max_actions_per_manifest,
        "too_many_actions",
        "actions",
        diagnostics,
    );
    check_limit(
        manifest.transitions.len(),
        limits.max_transitions_per_manifest,
        "too_many_transitions",
        "transitions",
        diagnostics,
    );
    check_limit(
        manifest.event_definitions.len(),
        limits.max_events_per_manifest,
        "too_many_events",
        "event_definitions",
        diagnostics,
    );

    let mut machine_ids = BTreeSet::new();
    let mut state_sets = BTreeMap::<MachineId, BTreeSet<StateId>>::new();
    for (machine_index, machine) in manifest.machines.iter().enumerate() {
        let path = format!("machines[{machine_index}]");
        if !machine_ids.insert(machine.machine_id) {
            diagnostics.push(error(
                "duplicate_machine",
                format!("{path}.machine_id"),
                format!("machine {} appears more than once", machine.machine_id),
            ));
        }
        check_limit(
            machine.states.len(),
            limits.max_states_per_machine,
            "too_many_states",
            format!("{path}.states"),
            diagnostics,
        );
        let mut states = BTreeSet::new();
        for (state_index, state_id) in machine.states.iter().copied().enumerate() {
            if !states.insert(state_id) {
                diagnostics.push(error(
                    "duplicate_state",
                    format!("{path}.states[{state_index}]"),
                    format!(
                        "state {state_id} appears more than once in machine {}",
                        machine.machine_id
                    ),
                ));
            }
        }
        if !states.contains(&machine.initial_state) {
            diagnostics.push(error(
                "unknown_initial_state",
                format!("{path}.initial_state"),
                format!(
                    "initial state {} is not declared by machine {}",
                    machine.initial_state, machine.machine_id
                ),
            ));
        }
        state_sets.insert(machine.machine_id, states);
    }

    let mut variables =
        BTreeMap::<(RuntimeVariableScope, VariableId), &RuntimeVariableDefinition>::new();
    for (variable_index, variable) in manifest.variables.iter().enumerate() {
        let path = format!("variables[{variable_index}]");
        if variables
            .insert((variable.scope, variable.variable_id), variable)
            .is_some()
        {
            diagnostics.push(error(
                "duplicate_variable",
                format!("{path}.variable_id"),
                format!(
                    "variable {} is duplicated in {:?} scope",
                    variable.variable_id, variable.scope
                ),
            ));
        }
        if variable
            .min
            .zip(variable.max)
            .is_some_and(|(min, max)| min > max)
        {
            diagnostics.push(error(
                "invalid_variable_bounds",
                &path,
                "variable min must be less than or equal to max",
            ));
        }
        if variable.value_type == RuntimeValueType::Bool
            && (variable.min.is_some() || variable.max.is_some())
        {
            diagnostics.push(warning(
                "ignored_boolean_bounds",
                path,
                "boolean bounds are encoded but do not constrain runtime values",
            ));
        }
    }

    let mut actions = BTreeSet::new();
    for (action_index, action_id) in manifest.actions.iter().copied().enumerate() {
        if !actions.insert(action_id) {
            diagnostics.push(error(
                "duplicate_action",
                format!("actions[{action_index}]"),
                format!("action {action_id} appears more than once"),
            ));
        }
    }

    let mut transition_ids = BTreeSet::new();
    let mut transition_keys = BTreeSet::new();
    for (transition_index, transition) in manifest.transitions.iter().enumerate() {
        let path = format!("transitions[{transition_index}]");
        if !transition_ids.insert(transition.transition_id) {
            diagnostics.push(error(
                "duplicate_transition",
                format!("{path}.transition_id"),
                format!(
                    "transition {} appears more than once",
                    transition.transition_id
                ),
            ));
        }
        let key = (
            transition.machine_id,
            transition.action_id,
            transition.from_state,
            transition.priority,
        );
        if !transition_keys.insert(key) {
            diagnostics.push(error(
                "ambiguous_transition",
                path.clone(),
                "machine, action, from_state, and priority must identify one transition",
            ));
        }
        if !machine_ids.contains(&transition.machine_id) {
            diagnostics.push(error(
                "unknown_machine",
                format!("{path}.machine_id"),
                format!("machine {} is not declared", transition.machine_id),
            ));
        }
        if !actions.contains(&transition.action_id) {
            diagnostics.push(error(
                "unknown_action",
                format!("{path}.action_id"),
                format!("action {} is not declared", transition.action_id),
            ));
        }
        if let Some(state_id) = transition.from_state {
            validate_state_ref(
                &state_sets,
                transition.machine_id,
                state_id,
                format!("{path}.from_state"),
                diagnostics,
            );
        }
        if let Some(state_id) = transition.to_state {
            validate_state_ref(
                &state_sets,
                transition.machine_id,
                state_id,
                format!("{path}.to_state"),
                diagnostics,
            );
        }
        check_limit(
            transition.conditions.len(),
            limits.max_conditions_per_transition,
            "too_many_conditions",
            format!("{path}.conditions"),
            diagnostics,
        );
        check_limit(
            transition.effects.len(),
            limits.max_effects_per_transition,
            "too_many_effects",
            format!("{path}.effects"),
            diagnostics,
        );
        for (condition_index, condition) in transition.conditions.iter().enumerate() {
            validate_condition(
                condition,
                &variables,
                &state_sets,
                limits,
                format!("{path}.conditions[{condition_index}]"),
                diagnostics,
            );
        }
        validate_economy_gate(
            &transition.economy_gate,
            limits,
            format!("{path}.economy_gate"),
            diagnostics,
        );
        for (effect_index, effect) in transition.effects.iter().enumerate() {
            validate_effect(
                effect,
                &variables,
                &state_sets,
                format!("{path}.effects[{effect_index}]"),
                diagnostics,
            );
        }
    }

    let mut event_types = BTreeSet::new();
    for (event_index, event) in manifest.event_definitions.iter().enumerate() {
        let path = format!("event_definitions[{event_index}]");
        if !event_types.insert(event.event_type) {
            diagnostics.push(error(
                "duplicate_event",
                format!("{path}.event_type"),
                format!("event type {} appears more than once", event.event_type),
            ));
        }
        check_limit(
            event.policies.len(),
            limits.max_event_effect_policies,
            "too_many_event_policies",
            format!("{path}.policies"),
            diagnostics,
        );
        for (policy_index, policy) in event.policies.iter().enumerate() {
            validate_policy(
                policy,
                &state_sets,
                format!("{path}.policies[{policy_index}]"),
                diagnostics,
            );
        }
    }
}

fn validate_condition(
    condition: &RuntimeCondition,
    variables: &BTreeMap<(RuntimeVariableScope, VariableId), &RuntimeVariableDefinition>,
    states: &BTreeMap<MachineId, BTreeSet<StateId>>,
    limits: &CompilerLimits,
    path: String,
    diagnostics: &mut Vec<CompilerDiagnostic>,
) {
    match condition {
        RuntimeCondition::All(atoms) | RuntimeCondition::Any(atoms) => {
            if atoms.is_empty() {
                diagnostics.push(error(
                    "empty_condition_group",
                    path.clone(),
                    "all/any condition groups must contain at least one atom",
                ));
            }
            check_limit(
                atoms.len(),
                limits.max_condition_clauses,
                "too_many_condition_clauses",
                path.clone(),
                diagnostics,
            );
            for (index, atom) in atoms.iter().enumerate() {
                validate_condition_atom(
                    atom,
                    variables,
                    states,
                    format!("{path}[{index}]"),
                    diagnostics,
                );
            }
        }
        RuntimeCondition::Not(atom) | RuntimeCondition::Atom(atom) => {
            validate_condition_atom(atom, variables, states, path, diagnostics);
        }
    }
}

fn validate_condition_atom(
    atom: &RuntimeConditionAtom,
    variables: &BTreeMap<(RuntimeVariableScope, VariableId), &RuntimeVariableDefinition>,
    states: &BTreeMap<MachineId, BTreeSet<StateId>>,
    path: String,
    diagnostics: &mut Vec<CompilerDiagnostic>,
) {
    match atom {
        RuntimeConditionAtom::VarEquals(variable, value) => {
            if let Some(definition) = validate_variable_ref(variable, variables, &path, diagnostics)
            {
                if !value_matches_type(*value, definition.value_type)
                    || !value_within_bounds(*value, definition)
                {
                    diagnostics.push(error(
                        "value_type_or_bounds_mismatch",
                        path,
                        "value must match the variable type and declared bounds",
                    ));
                }
            }
        }
        RuntimeConditionAtom::VarGreaterOrEqual(variable, _)
        | RuntimeConditionAtom::VarLessOrEqual(variable, _) => {
            if let Some(definition) = validate_variable_ref(variable, variables, &path, diagnostics)
            {
                if definition.value_type != RuntimeValueType::U64 {
                    diagnostics.push(error(
                        "value_type_mismatch",
                        path,
                        "var_gte and var_lte require a u64 variable",
                    ));
                }
            }
        }
        RuntimeConditionAtom::HasItem { amount, .. }
        | RuntimeConditionAtom::HasCredit { amount, .. } => {
            require_nonzero(*amount, "invalid_condition_amount", path, diagnostics);
        }
        RuntimeConditionAtom::HasEntitlement { .. } => {}
        RuntimeConditionAtom::MachineStateEquals {
            machine_id,
            state_id,
            ..
        } => validate_state_ref(states, *machine_id, *state_id, path, diagnostics),
    }
}

fn validate_economy_gate(
    gate: &RuntimeEconomyGate,
    limits: &CompilerLimits,
    path: String,
    diagnostics: &mut Vec<CompilerDiagnostic>,
) {
    match gate {
        RuntimeEconomyGate::Free | RuntimeEconomyGate::RequiresEntitlement { .. } => {}
        RuntimeEconomyGate::DeveloperSponsored { amount }
        | RuntimeEconomyGate::RequiresPayment { amount } => {
            require_nonzero(*amount, "invalid_economy_amount", path, diagnostics);
        }
        RuntimeEconomyGate::ConsumesCredit { amount, .. } => {
            require_nonzero(*amount, "invalid_economy_amount", path, diagnostics);
        }
        RuntimeEconomyGate::All(atoms) | RuntimeEconomyGate::Any(atoms) => {
            if atoms.is_empty() {
                diagnostics.push(error(
                    "empty_economy_gate",
                    path.clone(),
                    "all/any economy gates must contain at least one atom",
                ));
            }
            check_limit(
                atoms.len(),
                limits.max_economy_gate_clauses,
                "too_many_economy_gate_clauses",
                path.clone(),
                diagnostics,
            );
            for (index, atom) in atoms.iter().enumerate() {
                validate_economy_gate_atom(atom, format!("{path}[{index}]"), diagnostics);
            }
        }
    }
}

fn validate_economy_gate_atom(
    atom: &RuntimeEconomyGateAtom,
    path: String,
    diagnostics: &mut Vec<CompilerDiagnostic>,
) {
    match atom {
        RuntimeEconomyGateAtom::Free | RuntimeEconomyGateAtom::RequiresEntitlement { .. } => {}
        RuntimeEconomyGateAtom::DeveloperSponsored { amount }
        | RuntimeEconomyGateAtom::RequiresPayment { amount } => {
            require_nonzero(*amount, "invalid_economy_amount", path, diagnostics);
        }
        RuntimeEconomyGateAtom::ConsumesCredit { amount, .. } => {
            require_nonzero(*amount, "invalid_economy_amount", path, diagnostics);
        }
    }
}

fn validate_effect(
    effect: &RuntimeEffect,
    variables: &BTreeMap<(RuntimeVariableScope, VariableId), &RuntimeVariableDefinition>,
    states: &BTreeMap<MachineId, BTreeSet<StateId>>,
    path: String,
    diagnostics: &mut Vec<CompilerDiagnostic>,
) {
    match effect {
        RuntimeEffect::SetVar(variable, value) => {
            if let Some(definition) = validate_variable_ref(variable, variables, &path, diagnostics)
            {
                if !value_matches_type(*value, definition.value_type)
                    || !value_within_bounds(*value, definition)
                {
                    diagnostics.push(error(
                        "value_type_or_bounds_mismatch",
                        path,
                        "value must match the variable type and declared bounds",
                    ));
                }
            }
        }
        RuntimeEffect::IncVar(variable, amount) | RuntimeEffect::DecVar(variable, amount) => {
            if let Some(definition) = validate_variable_ref(variable, variables, &path, diagnostics)
            {
                if definition.value_type != RuntimeValueType::U64 {
                    diagnostics.push(error(
                        "value_type_mismatch",
                        path.clone(),
                        "increment/decrement effects require a u64 variable",
                    ));
                }
            }
            require_nonzero(*amount, "invalid_effect_amount", path, diagnostics);
        }
        RuntimeEffect::GrantItem { amount, .. }
        | RuntimeEffect::ConsumeItem { amount, .. }
        | RuntimeEffect::GrantCredit { amount, .. }
        | RuntimeEffect::UpdatePassportCounter { amount, .. } => {
            require_nonzero(*amount, "invalid_effect_amount", path, diagnostics);
        }
        RuntimeEffect::GrantEntitlement { .. }
        | RuntimeEffect::RevokeEntitlement { .. }
        | RuntimeEffect::GrantPassportBadge { .. }
        | RuntimeEffect::RevokePassportBadge { .. } => {}
        RuntimeEffect::SetMachineState {
            machine_id,
            state_id,
            ..
        } => validate_state_ref(states, *machine_id, *state_id, path, diagnostics),
    }
}

fn validate_policy(
    policy: &RuntimeAttestedEffectPolicy,
    states: &BTreeMap<MachineId, BTreeSet<StateId>>,
    path: String,
    diagnostics: &mut Vec<CompilerDiagnostic>,
) {
    match policy {
        RuntimeAttestedEffectPolicy::UpdatePassportCounter { amount, .. } => {
            require_nonzero(*amount, "invalid_attested_amount", path, diagnostics);
        }
        RuntimeAttestedEffectPolicy::GrantPassportBadge { .. }
        | RuntimeAttestedEffectPolicy::RevokePassportBadge { .. } => {}
        RuntimeAttestedEffectPolicy::SetMachineState {
            machine_id,
            state_id,
            ..
        } => validate_state_ref(states, *machine_id, *state_id, path, diagnostics),
    }
}

fn validate_variable_ref<'a>(
    variable: &RuntimeVariableRef,
    variables: &'a BTreeMap<(RuntimeVariableScope, VariableId), &RuntimeVariableDefinition>,
    path: &str,
    diagnostics: &mut Vec<CompilerDiagnostic>,
) -> Option<&'a RuntimeVariableDefinition> {
    let key = (variable.scope.variable_scope(), variable.variable_id);
    let definition = variables.get(&key).copied();
    if definition.is_none() {
        diagnostics.push(error(
            "unknown_variable",
            path,
            format!(
                "variable {} is not declared in {:?} scope",
                variable.variable_id, key.0
            ),
        ));
    }
    definition
}

fn validate_state_ref(
    states: &BTreeMap<MachineId, BTreeSet<StateId>>,
    machine_id: MachineId,
    state_id: StateId,
    path: impl Into<String>,
    diagnostics: &mut Vec<CompilerDiagnostic>,
) {
    if !states
        .get(&machine_id)
        .is_some_and(|machine_states| machine_states.contains(&state_id))
    {
        diagnostics.push(error(
            "unknown_state",
            path,
            format!("state {state_id} is not declared by machine {machine_id}"),
        ));
    }
}

fn value_matches_type(value: RuntimeValue, value_type: RuntimeValueType) -> bool {
    matches!(
        (value, value_type),
        (RuntimeValue::Bool(_), RuntimeValueType::Bool)
            | (RuntimeValue::U64(_), RuntimeValueType::U64)
            | (RuntimeValue::I64(_), RuntimeValueType::I64)
            | (RuntimeValue::Enum(_), RuntimeValueType::Enum)
    )
}

fn value_within_bounds(value: RuntimeValue, variable: &RuntimeVariableDefinition) -> bool {
    match value {
        RuntimeValue::Bool(_) => true,
        RuntimeValue::U64(value) => {
            let min_ok = variable
                .min
                .map_or(true, |min| min <= 0 || value >= min as u64);
            let max_ok = variable
                .max
                .map_or(true, |max| max >= 0 && value <= max as u64);
            min_ok && max_ok
        }
        RuntimeValue::I64(value) => {
            let min_ok = variable.min.map_or(true, |min| value >= min);
            let max_ok = variable.max.map_or(true, |max| value <= max);
            min_ok && max_ok
        }
        RuntimeValue::Enum(value) => {
            let min_ok = variable
                .min
                .map_or(true, |min| min <= 0 || value >= min as u32);
            let max_ok = variable
                .max
                .map_or(true, |max| max >= 0 && value <= max as u32);
            min_ok && max_ok
        }
    }
}

fn require_nonzero<T>(value: T, code: &str, path: String, diagnostics: &mut Vec<CompilerDiagnostic>)
where
    T: Default + PartialEq,
{
    if value == T::default() {
        diagnostics.push(error(code, path, "amount must be greater than zero"));
    }
}

fn check_limit(
    actual: usize,
    maximum: usize,
    code: &str,
    path: impl Into<String>,
    diagnostics: &mut Vec<CompilerDiagnostic>,
) {
    if actual > maximum {
        diagnostics.push(error(
            code,
            path,
            format!("{actual} entries exceed the maximum of {maximum}"),
        ));
    }
}

fn build_graph(
    manifest: &RuntimeManifestV0,
    diagnostics: &mut Vec<CompilerDiagnostic>,
) -> GraphSummary {
    let mut machines = Vec::with_capacity(manifest.machines.len());
    for machine in &manifest.machines {
        let state_set: BTreeSet<_> = machine.states.iter().copied().collect();
        let mut reachable = BTreeSet::from([machine.initial_state]);
        let mut queue = VecDeque::from([machine.initial_state]);
        let transitions: Vec<_> = manifest
            .transitions
            .iter()
            .filter(|transition| transition.machine_id == machine.machine_id)
            .collect();

        while let Some(state) = queue.pop_front() {
            for transition in &transitions {
                let applicable =
                    transition.from_state.is_none() || transition.from_state == Some(state);
                if applicable {
                    if let Some(to_state) = transition.to_state {
                        if state_set.contains(&to_state) && reachable.insert(to_state) {
                            queue.push_back(to_state);
                        }
                    }
                }
            }
        }

        let unreachable_states: Vec<_> = state_set.difference(&reachable).copied().collect();
        let unreachable_transitions: Vec<_> = transitions
            .iter()
            .filter(|transition| {
                transition
                    .from_state
                    .is_some_and(|from| !reachable.contains(&from))
            })
            .map(|transition| transition.transition_id)
            .collect();

        for state_id in &unreachable_states {
            diagnostics.push(warning(
                "unreachable_state",
                format!("machines[{}].states", machine.machine_id),
                format!(
                    "state {state_id} is not reachable from initial state {}",
                    machine.initial_state
                ),
            ));
        }
        for transition_id in &unreachable_transitions {
            diagnostics.push(warning(
                "unreachable_transition",
                "transitions",
                format!("transition {transition_id} starts from an unreachable state"),
            ));
        }

        machines.push(MachineGraph {
            machine_id: machine.machine_id,
            initial_state: machine.initial_state,
            states: machine.states.clone(),
            transitions: transitions
                .iter()
                .map(|transition| TransitionEdge {
                    transition_id: transition.transition_id,
                    action_id: transition.action_id,
                    from_state: transition.from_state,
                    to_state: transition.to_state,
                    priority: transition.priority,
                })
                .collect(),
            reachable_states: reachable.into_iter().collect(),
            unreachable_states,
            unreachable_transitions,
        });
    }
    GraphSummary { machines }
}

fn warn_duplicate_rewards(manifest: &RuntimeManifestV0, diagnostics: &mut Vec<CompilerDiagnostic>) {
    for (transition_index, transition) in manifest.transitions.iter().enumerate() {
        let mut seen = BTreeSet::new();
        for (effect_index, effect) in transition.effects.iter().enumerate() {
            if !seen.insert(effect) {
                diagnostics.push(warning(
                    "duplicate_effect",
                    format!("transitions[{transition_index}].effects[{effect_index}]"),
                    "identical effects execute independently; confirm the duplicate is intentional",
                ));
            }
        }
    }
    for (event_index, event) in manifest.event_definitions.iter().enumerate() {
        let mut seen = BTreeSet::new();
        for (policy_index, policy) in event.policies.iter().enumerate() {
            if !seen.insert(policy) {
                diagnostics.push(warning(
                    "duplicate_attested_policy",
                    format!("event_definitions[{event_index}].policies[{policy_index}]"),
                    "identical attested policies are redundant",
                ));
            }
        }
    }
}

fn estimate_costs(manifest: &RuntimeManifestV0) -> Vec<CostEstimate> {
    let mut estimates =
        Vec::with_capacity(manifest.transitions.len() + manifest.event_definitions.len());
    for transition in &manifest.transitions {
        let condition_atoms = transition.conditions.iter().map(condition_atom_count).sum();
        let mut authority_calls = 0;
        let mut economy_calls = economy_gate_provider_calls(&transition.economy_gate);
        let mut profile_calls = 0;
        let mut writes = 1;
        for condition in &transition.conditions {
            count_condition_providers(
                condition,
                &mut authority_calls,
                &mut economy_calls,
                &mut profile_calls,
            );
        }
        for effect in &transition.effects {
            count_effect_providers(
                effect,
                &mut authority_calls,
                &mut economy_calls,
                &mut profile_calls,
            );
            writes += 1;
        }
        estimates.push(CostEstimate {
            subject: CostSubject::Transition(transition.transition_id),
            storage_reads: 4 + condition_atoms,
            storage_writes: writes,
            authority_provider_calls: authority_calls,
            economy_provider_calls: economy_calls,
            profile_provider_calls: profile_calls,
            condition_atoms,
            effects: transition.effects.len() as u32,
        });
    }
    for event in &manifest.event_definitions {
        let policies = event.policies.len() as u32;
        estimates.push(CostEstimate {
            subject: CostSubject::AttestedEvent(event.event_type),
            storage_reads: 5 + policies,
            storage_writes: 2 + policies,
            authority_provider_calls: 1,
            economy_provider_calls: 0,
            profile_provider_calls: event
                .policies
                .iter()
                .filter(|policy| {
                    matches!(
                        policy,
                        RuntimeAttestedEffectPolicy::UpdatePassportCounter { .. }
                            | RuntimeAttestedEffectPolicy::GrantPassportBadge { .. }
                            | RuntimeAttestedEffectPolicy::RevokePassportBadge { .. }
                    )
                })
                .count() as u32,
            condition_atoms: 0,
            effects: policies,
        });
    }
    estimates
}

fn condition_atom_count(condition: &RuntimeCondition) -> u32 {
    match condition {
        RuntimeCondition::All(atoms) | RuntimeCondition::Any(atoms) => atoms.len() as u32,
        RuntimeCondition::Not(_) | RuntimeCondition::Atom(_) => 1,
    }
}

fn count_condition_providers(
    condition: &RuntimeCondition,
    authority: &mut u32,
    economy: &mut u32,
    profile: &mut u32,
) {
    let mut count_atom = |atom: &RuntimeConditionAtom| match atom {
        RuntimeConditionAtom::HasItem { .. } | RuntimeConditionAtom::HasCredit { .. } => {
            *economy += 1
        }
        RuntimeConditionAtom::HasEntitlement { .. } => *authority += 1,
        RuntimeConditionAtom::VarEquals(variable, _)
        | RuntimeConditionAtom::VarGreaterOrEqual(variable, _)
        | RuntimeConditionAtom::VarLessOrEqual(variable, _) => {
            if variable.scope.variable_scope() == RuntimeVariableScope::Passport {
                *profile += 1;
            }
        }
        RuntimeConditionAtom::MachineStateEquals { .. } => {}
    };
    match condition {
        RuntimeCondition::All(atoms) | RuntimeCondition::Any(atoms) => {
            for atom in atoms {
                count_atom(atom);
            }
        }
        RuntimeCondition::Not(atom) | RuntimeCondition::Atom(atom) => count_atom(atom),
    }
}

fn economy_gate_provider_calls(gate: &RuntimeEconomyGate) -> u32 {
    match gate {
        RuntimeEconomyGate::Free => 0,
        RuntimeEconomyGate::DeveloperSponsored { .. }
        | RuntimeEconomyGate::RequiresPayment { .. }
        | RuntimeEconomyGate::RequiresEntitlement { .. }
        | RuntimeEconomyGate::ConsumesCredit { .. } => 1,
        RuntimeEconomyGate::All(atoms) | RuntimeEconomyGate::Any(atoms) => atoms.len() as u32,
    }
}

fn count_effect_providers(
    effect: &RuntimeEffect,
    authority: &mut u32,
    economy: &mut u32,
    profile: &mut u32,
) {
    match effect {
        RuntimeEffect::GrantItem { .. }
        | RuntimeEffect::ConsumeItem { .. }
        | RuntimeEffect::GrantCredit { .. } => *economy += 1,
        RuntimeEffect::GrantEntitlement { .. } | RuntimeEffect::RevokeEntitlement { .. } => {
            *authority += 1
        }
        RuntimeEffect::UpdatePassportCounter { .. }
        | RuntimeEffect::GrantPassportBadge { .. }
        | RuntimeEffect::RevokePassportBadge { .. } => *profile += 1,
        RuntimeEffect::SetVar(variable, _)
        | RuntimeEffect::IncVar(variable, _)
        | RuntimeEffect::DecVar(variable, _) => {
            if variable.scope.variable_scope() == RuntimeVariableScope::Passport {
                *profile += 1;
            }
        }
        RuntimeEffect::SetMachineState { .. } => {}
    }
}

fn manifest_metrics(
    manifest: &RuntimeManifestV0,
    scale_bytes: usize,
    limits: &CompilerLimits,
) -> ManifestMetrics {
    let manifest_chunks = if scale_bytes == 0 {
        0
    } else if limits.max_manifest_chunk_bytes == 0 {
        usize::MAX
    } else {
        scale_bytes.div_ceil(limits.max_manifest_chunk_bytes)
    };
    ManifestMetrics {
        scale_bytes,
        manifest_chunks,
        machines: manifest.machines.len(),
        states: manifest
            .machines
            .iter()
            .map(|machine| machine.states.len())
            .sum(),
        variables: manifest.variables.len(),
        actions: manifest.actions.len(),
        transitions: manifest.transitions.len(),
        event_definitions: manifest.event_definitions.len(),
        attested_policies: manifest
            .event_definitions
            .iter()
            .map(|event| event.policies.len())
            .sum(),
    }
}

fn error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> CompilerDiagnostic {
    CompilerDiagnostic {
        severity: DiagnosticSeverity::Error,
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn warning(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> CompilerDiagnostic {
    CompilerDiagnostic {
        severity: DiagnosticSeverity::Warning,
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

pub mod templates {
    use super::AuthoringManifest;

    pub fn zelda_door() -> AuthoringManifest {
        parse(include_str!("../../../examples/zelda-door.flow.json"))
    }

    pub fn arcade_credit_run() -> AuthoringManifest {
        parse(include_str!(
            "../../../examples/arcade-credit-run.flow.json"
        ))
    }

    pub fn season_pass_reward() -> AuthoringManifest {
        parse(include_str!(
            "../../../examples/season-pass-reward.flow.json"
        ))
    }

    pub fn dungeon_run() -> AuthoringManifest {
        parse(include_str!("../../../examples/dungeon-run.flow.json"))
    }

    pub fn fps_attested_result() -> AuthoringManifest {
        parse(include_str!(
            "../../../examples/fps-attested-result.flow.json"
        ))
    }

    fn parse(value: &str) -> AuthoringManifest {
        serde_json::from_str(value).expect("checked-in Flow example must parse")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        authoring: &'static str,
        scale_hex: &'static str,
        hash: &'static str,
    }

    const FIXTURES: &[Fixture] = &[
        Fixture {
            authoring: include_str!("../../../fixtures/wire/v0/inputs/zelda-door.flow.json"),
            scale_hex: include_str!("../../../fixtures/wire/v0/zelda-door.scale.hex"),
            hash: "032251c5252f0d13230bd4a236cefcc6db32076502230fd03f70169cd402c433",
        },
        Fixture {
            authoring: include_str!("../../../fixtures/wire/v0/inputs/arcade-credit-run.flow.json"),
            scale_hex: include_str!("../../../fixtures/wire/v0/arcade-credit-run.scale.hex"),
            hash: "ff8a969873aea32afeeba948773c2b4b5fdc977f4eb7265420875ef75c7fee47",
        },
        Fixture {
            authoring: include_str!(
                "../../../fixtures/wire/v0/inputs/season-pass-reward.flow.json"
            ),
            scale_hex: include_str!("../../../fixtures/wire/v0/season-pass-reward.scale.hex"),
            hash: "a5868eba91236d0f3985ae961c4d8adec3518c0eb4142d91035dc43932e32c18",
        },
        Fixture {
            authoring: include_str!("../../../fixtures/wire/v0/inputs/dungeon-run.flow.json"),
            scale_hex: include_str!("../../../fixtures/wire/v0/dungeon-run.scale.hex"),
            hash: "4e9b0c6ef600274220e831291865746c53e772fad1d2456e961bbf7c46732e25",
        },
        Fixture {
            authoring: include_str!(
                "../../../fixtures/wire/v0/inputs/fps-attested-result.flow.json"
            ),
            scale_hex: include_str!("../../../fixtures/wire/v0/fps-attested-result.scale.hex"),
            hash: "99e1aff3fe84ca5e93e18310deada9edaa008fa375539ea291a0ddcf24de23f2",
        },
    ];

    #[test]
    fn locked_wire_fixtures_compile_byte_for_byte() {
        for fixture in FIXTURES {
            let compiled =
                compile_manifest_json(fixture.authoring.as_bytes(), CompilerLimits::default())
                    .unwrap_or_else(|diagnostics| panic!("{diagnostics:#?}"));
            assert_eq!(
                compiled.scale_bytes,
                hex::decode(
                    fixture
                        .scale_hex
                        .trim()
                        .strip_prefix("0x")
                        .unwrap_or(fixture.scale_hex.trim())
                )
                .expect("valid fixture hex")
            );
            assert_eq!(hex::encode(compiled.manifest_hash), fixture.hash);

            let mut input = &compiled.scale_bytes[..];
            let decoded = RuntimeManifestV0::decode(&mut input).expect("decode fixture");
            assert!(input.is_empty(), "locked payload rejects trailing bytes");
            assert_eq!(decoded.encode(), compiled.scale_bytes);
        }
    }

    #[test]
    fn permanent_eterra_alias_compiles_identically() {
        for fixture in FIXTURES {
            let preferred =
                compile_manifest_json(fixture.authoring.as_bytes(), CompilerLimits::default())
                    .expect("preferred label compiles");
            let mut aliased: AuthoringManifest =
                serde_json::from_str(fixture.authoring).expect("fixture parses");
            aliased.manifest_version = ETERRA_AUTHORING_ALIAS.to_owned();
            let aliased =
                compile_manifest(aliased, CompilerLimits::default()).expect("alias compiles");
            assert_eq!(aliased.scale_bytes, preferred.scale_bytes);
            assert_eq!(aliased.manifest_hash, preferred.manifest_hash);
            assert_eq!(
                aliased.canonical_authoring.manifest_version,
                AUTHORING_LABEL
            );
        }
    }

    #[test]
    fn validation_is_actionable_and_write_free() {
        let mut manifest: AuthoringManifest =
            serde_json::from_str(FIXTURES[0].authoring).expect("fixture parses");
        manifest.transitions[0].to_state = Some(999);
        manifest.transitions[0]
            .effects
            .push(AuthoringEffect::IncVar {
                inc_var: AuthoringVarAmount {
                    variable: AuthoringVariableRef {
                        scope: AuthoringScope::Actor { actor: 42 },
                        variable_id: 10,
                    },
                    amount: 0,
                },
            });
        let diagnostics =
            compile_manifest(manifest, CompilerLimits::default()).expect_err("must fail");
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "unknown_state" && diagnostic.path.contains("to_state")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "value_type_mismatch" || diagnostic.code == "invalid_effect_amount"
        }));
    }

    #[test]
    fn graph_reports_unreachable_state_as_warning() {
        let mut manifest: AuthoringManifest =
            serde_json::from_str(FIXTURES[0].authoring).expect("fixture parses");
        manifest.machines[0].states.push(77);
        let compiled =
            compile_manifest(manifest, CompilerLimits::default()).expect("warnings are allowed");
        assert!(compiled
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "unreachable_state"));
        assert_eq!(compiled.graph.machines[0].unreachable_states, vec![77]);
    }
}
