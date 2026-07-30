export const FLOW_AUTHORING_LABEL = "blockchainia.flow.v0" as const;
export const ETERRA_FLOW_AUTHORING_ALIAS = "eterra.flow.v0" as const;

export type FlowAuthoringLabel =
  | typeof FLOW_AUTHORING_LABEL
  | typeof ETERRA_FLOW_AUTHORING_ALIAS;

export type FlowScope =
  | { game: boolean }
  | { instance: boolean }
  | { actor: number }
  | { entity: number }
  | { passport: number };

export interface VariableRef {
  scope: FlowScope;
  variable_id: number;
}

export type FlowValue =
  | { bool: boolean }
  | { u64: number }
  | { i64: number }
  | { enum: number };

export type ConditionAtom =
  | { var_equals: { variable: VariableRef; value: FlowValue } }
  | { var_gte: { variable: VariableRef; value: number } }
  | { var_lte: { variable: VariableRef; value: number } }
  | { has_item: { actor_id: number; item_id: number; amount: number } }
  | {
      has_credit: {
        actor_id: number;
        credit_type: number;
        amount: number;
      };
    }
  | {
      has_entitlement: {
        actor_id: number;
        entitlement_id: number;
      };
    }
  | {
      machine_state_equals: {
        scope: FlowScope;
        machine_id: number;
        state_id: number;
      };
    };

export type Condition =
  | { all: ConditionAtom[] }
  | { any: ConditionAtom[] }
  | { not: ConditionAtom }
  | { atom: ConditionAtom };

export type EconomyGateAtom =
  | { free: Record<string, never> }
  | { developer_sponsored: { amount: number | string } }
  | { requires_payment: { amount: number | string } }
  | { requires_entitlement: { entitlement_id: number } }
  | { consumes_credit: { credit_type: number; amount: number } };

export type EconomyGate =
  | EconomyGateAtom
  | { all: EconomyGateAtom[] }
  | { any: EconomyGateAtom[] };

export type Effect =
  | { set_var: { variable: VariableRef; value: FlowValue } }
  | { inc_var: { variable: VariableRef; amount: number } }
  | { dec_var: { variable: VariableRef; amount: number } }
  | { grant_item: { actor_id: number; item_id: number; amount: number } }
  | { consume_item: { actor_id: number; item_id: number; amount: number } }
  | {
      grant_credit: {
        actor_id: number;
        credit_type: number;
        amount: number;
      };
    }
  | {
      grant_entitlement: {
        actor_id: number;
        entitlement_id: number;
      };
    }
  | {
      revoke_entitlement: {
        actor_id: number;
        entitlement_id: number;
      };
    }
  | {
      update_passport_counter: {
        actor_id: number;
        field_id: number;
        amount: number;
      };
    }
  | { grant_passport_badge: { actor_id: number; badge_id: number } }
  | { revoke_passport_badge: { actor_id: number; badge_id: number } }
  | {
      set_machine_state: {
        scope: FlowScope;
        machine_id: number;
        state_id: number;
      };
    };

export type AttestedEffectPolicy =
  | { update_passport_counter: { field_id: number; amount: number } }
  | { grant_passport_badge: { badge_id: number } }
  | { revoke_passport_badge: { badge_id: number } }
  | {
      set_machine_state: {
        scope: FlowScope;
        machine_id: number;
        state_id: number;
      };
    };

export interface FlowManifest {
  manifest_version: FlowAuthoringLabel;
  game_id: number;
  version_id: number;
  machines: Array<{
    machine_id: number;
    initial_state: number;
    states: number[];
  }>;
  variables: Array<{
    variable_id: number;
    scope: "game" | "instance" | "actor" | "entity" | "passport";
    type: "bool" | "u64" | "i64" | "enum";
    min?: number;
    max?: number;
  }>;
  actions: number[];
  transitions: Array<{
    transition_id: number;
    machine_id: number;
    action_id: number;
    from_state: number | null;
    to_state: number | null;
    priority: number;
    economy_gate: EconomyGate;
    conditions: Condition[];
    effects: Effect[];
  }>;
  event_definitions: Array<{
    event_type: number;
    policies: AttestedEffectPolicy[];
  }>;
}

export interface CompilerDiagnostic {
  severity: "error" | "warning";
  code: string;
  path: string;
  message: string;
}

export interface ManifestMetrics {
  scaleBytes: number;
  manifestChunks: number;
  machines: number;
  states: number;
  variables: number;
  actions: number;
  transitions: number;
  eventDefinitions: number;
  attestedPolicies: number;
}

export interface CompileSuccess {
  ok: true;
  authoringLabel: typeof FLOW_AUTHORING_LABEL;
  permanentAlias: typeof ETERRA_FLOW_AUTHORING_ALIAS;
  canonicalAuthoringJson: string;
  scaleHex: `0x${string}`;
  manifestHashHex: `0x${string}`;
  metrics: ManifestMetrics;
  diagnostics: CompilerDiagnostic[];
  graph: unknown;
  costEstimates: unknown[];
}

export interface CompileFailure {
  ok: false;
  diagnostics: CompilerDiagnostic[];
}

export type CompileReport = CompileSuccess | CompileFailure;
