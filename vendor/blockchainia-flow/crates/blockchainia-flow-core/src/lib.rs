//! Pure, non-authoritative preview engine for Blockchainia Flow.
//!
//! This crate helps builders explain which transition would be selected. It
//! never replaces runtime validation and never writes chain state.

use std::collections::BTreeMap;

use blockchainia_flow_manifest::{
    ActionId, ActorId, CreditTypeId, EntitlementId, GameId, InstanceId, ItemId, MachineId,
    RuntimeCondition, RuntimeConditionAtom, RuntimeEconomyGate, RuntimeEconomyGateAtom,
    RuntimeEffect, RuntimeManifestV0, RuntimeScope, RuntimeTransition, RuntimeValue,
    RuntimeVariableRef, StateId, VariableId,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VariableKey {
    pub game_id: GameId,
    pub instance_id: InstanceId,
    pub scope: RuntimeScope,
    pub variable_id: VariableId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MachineStateKey {
    pub game_id: GameId,
    pub instance_id: InstanceId,
    pub scope: RuntimeScope,
    pub machine_id: MachineId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreviewSnapshot {
    pub variables: BTreeMap<VariableKey, RuntimeValue>,
    pub machine_states: BTreeMap<MachineStateKey, StateId>,
    pub inventory: BTreeMap<(GameId, InstanceId, ActorId, ItemId), u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionContext {
    pub game_id: GameId,
    pub instance_id: InstanceId,
    pub actor_id: ActorId,
    pub machine_id: MachineId,
    pub action_id: ActionId,
}

pub trait ProviderPreview {
    fn credit_balance(&self, actor_id: ActorId, game_id: GameId, credit_type: CreditTypeId) -> u64;

    fn has_entitlement(
        &self,
        actor_id: ActorId,
        game_id: GameId,
        entitlement_id: EntitlementId,
    ) -> bool;

    fn developer_sponsorship_available(&self, game_id: GameId, amount: u128) -> bool;

    fn payment_available(&self, actor_id: ActorId, game_id: GameId, amount: u128) -> bool;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DenyExternalProviders;

impl ProviderPreview for DenyExternalProviders {
    fn credit_balance(
        &self,
        _actor_id: ActorId,
        _game_id: GameId,
        _credit_type: CreditTypeId,
    ) -> u64 {
        0
    }

    fn has_entitlement(
        &self,
        _actor_id: ActorId,
        _game_id: GameId,
        _entitlement_id: EntitlementId,
    ) -> bool {
        false
    }

    fn developer_sponsorship_available(&self, _game_id: GameId, _amount: u128) -> bool {
        false
    }

    fn payment_available(&self, _actor_id: ActorId, _game_id: GameId, _amount: u128) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedTransition {
    pub transition_id: u64,
    pub from_state: Option<StateId>,
    pub to_state: Option<StateId>,
    pub effects: Vec<RuntimeEffect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewError {
    ManifestIdentityMismatch,
    UnknownMachine,
    NoEligibleTransition,
}

/// Select the exact priority/transition-id winner used by the v0 runtime.
pub fn prepare_action<P: ProviderPreview>(
    manifest: &RuntimeManifestV0,
    snapshot: &PreviewSnapshot,
    providers: &P,
    context: ActionContext,
) -> Result<PreparedTransition, PreviewError> {
    if manifest.game_id != context.game_id {
        return Err(PreviewError::ManifestIdentityMismatch);
    }
    if !manifest
        .machines
        .iter()
        .any(|machine| machine.machine_id == context.machine_id)
    {
        return Err(PreviewError::UnknownMachine);
    }

    let current_state = machine_state(
        manifest,
        snapshot,
        context,
        RuntimeScope::Actor(context.actor_id),
        context.machine_id,
    );
    let selected = manifest
        .transitions
        .iter()
        .filter(|transition| {
            transition.machine_id == context.machine_id
                && transition.action_id == context.action_id
                && (transition.from_state.is_none() || transition.from_state == current_state)
                && transition.conditions.iter().all(|condition| {
                    condition_holds(manifest, snapshot, providers, context, condition)
                })
                && economy_gate_holds(providers, context, &transition.economy_gate)
        })
        .min_by_key(|transition| (transition.priority, transition.transition_id))
        .ok_or(PreviewError::NoEligibleTransition)?;

    Ok(prepared(selected))
}

fn prepared(transition: &RuntimeTransition) -> PreparedTransition {
    PreparedTransition {
        transition_id: transition.transition_id,
        from_state: transition.from_state,
        to_state: transition.to_state,
        effects: transition.effects.clone(),
    }
}

fn condition_holds<P: ProviderPreview>(
    manifest: &RuntimeManifestV0,
    snapshot: &PreviewSnapshot,
    providers: &P,
    context: ActionContext,
    condition: &RuntimeCondition,
) -> bool {
    match condition {
        RuntimeCondition::All(atoms) => atoms
            .iter()
            .all(|atom| condition_atom_holds(manifest, snapshot, providers, context, atom)),
        RuntimeCondition::Any(atoms) => atoms
            .iter()
            .any(|atom| condition_atom_holds(manifest, snapshot, providers, context, atom)),
        RuntimeCondition::Not(atom) => {
            !condition_atom_holds(manifest, snapshot, providers, context, atom)
        }
        RuntimeCondition::Atom(atom) => {
            condition_atom_holds(manifest, snapshot, providers, context, atom)
        }
    }
}

fn condition_atom_holds<P: ProviderPreview>(
    manifest: &RuntimeManifestV0,
    snapshot: &PreviewSnapshot,
    providers: &P,
    context: ActionContext,
    atom: &RuntimeConditionAtom,
) -> bool {
    match atom {
        RuntimeConditionAtom::VarEquals(variable, expected) => {
            variable_value(snapshot, context, variable) == Some(*expected)
        }
        RuntimeConditionAtom::VarGreaterOrEqual(variable, expected) => {
            matches!(
                variable_value(snapshot, context, variable),
                Some(RuntimeValue::U64(value)) if value >= *expected
            )
        }
        RuntimeConditionAtom::VarLessOrEqual(variable, expected) => {
            matches!(
                variable_value(snapshot, context, variable),
                Some(RuntimeValue::U64(value)) if value <= *expected
            )
        }
        RuntimeConditionAtom::HasItem {
            actor_id,
            item_id,
            amount,
        } => {
            snapshot
                .inventory
                .get(&(context.game_id, context.instance_id, *actor_id, *item_id))
                .copied()
                .unwrap_or_default()
                >= *amount
        }
        RuntimeConditionAtom::HasCredit {
            actor_id,
            credit_type,
            amount,
        } => {
            *actor_id == context.actor_id
                && providers.credit_balance(*actor_id, context.game_id, *credit_type) >= *amount
        }
        RuntimeConditionAtom::HasEntitlement {
            actor_id,
            entitlement_id,
        } => {
            *actor_id == context.actor_id
                && providers.has_entitlement(*actor_id, context.game_id, *entitlement_id)
        }
        RuntimeConditionAtom::MachineStateEquals {
            scope,
            machine_id,
            state_id,
        } => machine_state(manifest, snapshot, context, *scope, *machine_id) == Some(*state_id),
    }
}

fn economy_gate_holds<P: ProviderPreview>(
    providers: &P,
    context: ActionContext,
    gate: &RuntimeEconomyGate,
) -> bool {
    match gate {
        RuntimeEconomyGate::Free => true,
        RuntimeEconomyGate::DeveloperSponsored { amount } => {
            providers.developer_sponsorship_available(context.game_id, *amount)
        }
        RuntimeEconomyGate::RequiresPayment { amount } => {
            providers.payment_available(context.actor_id, context.game_id, *amount)
        }
        RuntimeEconomyGate::RequiresEntitlement { entitlement_id } => {
            providers.has_entitlement(context.actor_id, context.game_id, *entitlement_id)
        }
        RuntimeEconomyGate::ConsumesCredit {
            credit_type,
            amount,
        } => providers.credit_balance(context.actor_id, context.game_id, *credit_type) >= *amount,
        RuntimeEconomyGate::All(atoms) => atoms
            .iter()
            .all(|atom| economy_gate_atom_holds(providers, context, atom)),
        RuntimeEconomyGate::Any(atoms) => atoms
            .iter()
            .any(|atom| economy_gate_atom_holds(providers, context, atom)),
    }
}

fn economy_gate_atom_holds<P: ProviderPreview>(
    providers: &P,
    context: ActionContext,
    atom: &RuntimeEconomyGateAtom,
) -> bool {
    match atom {
        RuntimeEconomyGateAtom::Free => true,
        RuntimeEconomyGateAtom::DeveloperSponsored { amount } => {
            providers.developer_sponsorship_available(context.game_id, *amount)
        }
        RuntimeEconomyGateAtom::RequiresPayment { amount } => {
            providers.payment_available(context.actor_id, context.game_id, *amount)
        }
        RuntimeEconomyGateAtom::RequiresEntitlement { entitlement_id } => {
            providers.has_entitlement(context.actor_id, context.game_id, *entitlement_id)
        }
        RuntimeEconomyGateAtom::ConsumesCredit {
            credit_type,
            amount,
        } => providers.credit_balance(context.actor_id, context.game_id, *credit_type) >= *amount,
    }
}

fn variable_value(
    snapshot: &PreviewSnapshot,
    context: ActionContext,
    variable: &RuntimeVariableRef,
) -> Option<RuntimeValue> {
    snapshot
        .variables
        .get(&VariableKey {
            game_id: context.game_id,
            instance_id: storage_instance(context.instance_id, variable.scope),
            scope: variable.scope,
            variable_id: variable.variable_id,
        })
        .copied()
}

fn machine_state(
    manifest: &RuntimeManifestV0,
    snapshot: &PreviewSnapshot,
    context: ActionContext,
    scope: RuntimeScope,
    machine_id: MachineId,
) -> Option<StateId> {
    snapshot
        .machine_states
        .get(&MachineStateKey {
            game_id: context.game_id,
            instance_id: context.instance_id,
            scope,
            machine_id,
        })
        .copied()
        .or_else(|| {
            manifest
                .machines
                .iter()
                .find(|machine| machine.machine_id == machine_id)
                .map(|machine| machine.initial_state)
        })
}

fn storage_instance(instance_id: InstanceId, scope: RuntimeScope) -> InstanceId {
    match scope {
        RuntimeScope::Game | RuntimeScope::Passport(_) => 0,
        RuntimeScope::Instance | RuntimeScope::Actor(_) | RuntimeScope::Entity(_) => instance_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blockchainia_flow_manifest::{compile_manifest_json, CompilerLimits};

    #[test]
    fn preview_selects_the_runtime_priority_winner_without_mutation() {
        let compiled = compile_manifest_json(
            include_bytes!("../../../examples/zelda-door.flow.json"),
            CompilerLimits::default(),
        )
        .expect("fixture compiles");
        let mut snapshot = PreviewSnapshot::default();
        snapshot.variables.insert(
            VariableKey {
                game_id: 1,
                instance_id: 1,
                scope: RuntimeScope::Actor(42),
                variable_id: 10,
            },
            RuntimeValue::Bool(true),
        );
        let before = snapshot.clone();
        let prepared = prepare_action(
            &compiled.runtime_manifest,
            &snapshot,
            &DenyExternalProviders,
            ActionContext {
                game_id: 1,
                instance_id: 1,
                actor_id: 42,
                machine_id: 7,
                action_id: 9,
            },
        )
        .expect("transition is eligible");
        assert_eq!(prepared.transition_id, 1);
        assert_eq!(prepared.to_state, Some(2));
        assert_eq!(snapshot, before, "preview must never write state");
    }
}
