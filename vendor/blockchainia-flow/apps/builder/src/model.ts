import {
  FLOW_AUTHORING_LABEL,
  type CompilerDiagnostic,
  type FlowManifest,
} from "@blockchainia/flow-sdk";

export const starterManifest: FlowManifest = {
  manifest_version: FLOW_AUTHORING_LABEL,
  game_id: 1,
  version_id: 1,
  machines: [{ machine_id: 7, initial_state: 1, states: [1, 2] }],
  variables: [
    { variable_id: 10, scope: "actor", type: "bool" },
    { variable_id: 12, scope: "instance", type: "bool" },
  ],
  actions: [9],
  transitions: [
    {
      transition_id: 1,
      machine_id: 7,
      action_id: 9,
      from_state: 1,
      to_state: 2,
      priority: 0,
      economy_gate: { free: {} },
      conditions: [
        {
          atom: {
            var_equals: {
              variable: {
                scope: { actor: 42 },
                variable_id: 10,
              },
              value: { bool: true },
            },
          },
        },
      ],
      effects: [
        {
          set_var: {
            variable: {
              scope: { instance: true },
              variable_id: 12,
            },
            value: { bool: true },
          },
        },
      ],
    },
  ],
  event_definitions: [],
};

export function cloneManifest(manifest: FlowManifest): FlowManifest {
  return JSON.parse(JSON.stringify(manifest)) as FlowManifest;
}

export function exportManifest(manifest: FlowManifest): string {
  return `${JSON.stringify(manifest, null, 2)}\n`;
}

export function importManifest(value: string): FlowManifest {
  const parsed: unknown = JSON.parse(value);
  if (!isRecord(parsed)) {
    throw new TypeError("Manifest JSON must contain an object");
  }
  if (
    parsed.manifest_version !== "blockchainia.flow.v0" &&
    parsed.manifest_version !== "eterra.flow.v0"
  ) {
    throw new TypeError(
      "manifest_version must be blockchainia.flow.v0 or eterra.flow.v0",
    );
  }
  if (
    !Array.isArray(parsed.machines) ||
    !Array.isArray(parsed.variables) ||
    !Array.isArray(parsed.actions) ||
    !Array.isArray(parsed.transitions) ||
    !Array.isArray(parsed.event_definitions)
  ) {
    throw new TypeError("Manifest is missing one or more required arrays");
  }
  return parsed as unknown as FlowManifest;
}

export function validateDraft(
  manifest: FlowManifest,
): CompilerDiagnostic[] {
  const diagnostics: CompilerDiagnostic[] = [];
  if (manifest.machines.length === 0) {
    diagnostics.push(error("empty_machines", "machines", "Add a machine."));
  }
  if (manifest.actions.length === 0) {
    diagnostics.push(error("empty_actions", "actions", "Add an action."));
  }
  const machineIds = new Set<number>();
  const stateMap = new Map<number, Set<number>>();
  manifest.machines.forEach((machine, index) => {
    if (machineIds.has(machine.machine_id)) {
      diagnostics.push(
        error(
          "duplicate_machine",
          `machines[${index}].machine_id`,
          `Machine ${machine.machine_id} is duplicated.`,
        ),
      );
    }
    machineIds.add(machine.machine_id);
    const states = new Set(machine.states);
    stateMap.set(machine.machine_id, states);
    if (!states.has(machine.initial_state)) {
      diagnostics.push(
        error(
          "unknown_initial_state",
          `machines[${index}].initial_state`,
          "Initial state must be listed in states.",
        ),
      );
    }
    if (states.size !== machine.states.length) {
      diagnostics.push(
        error(
          "duplicate_state",
          `machines[${index}].states`,
          "State IDs must be unique within a machine.",
        ),
      );
    }
  });
  const actions = new Set(manifest.actions);
  if (actions.size !== manifest.actions.length) {
    diagnostics.push(
      error("duplicate_action", "actions", "Action IDs must be unique."),
    );
  }
  const transitionIds = new Set<number>();
  const transitionKeys = new Set<string>();
  manifest.transitions.forEach((transition, index) => {
    if (transitionIds.has(transition.transition_id)) {
      diagnostics.push(
        error(
          "duplicate_transition",
          `transitions[${index}].transition_id`,
          "Transition IDs must be unique.",
        ),
      );
    }
    transitionIds.add(transition.transition_id);
    const key = [
      transition.machine_id,
      transition.action_id,
      transition.from_state,
      transition.priority,
    ].join(":");
    if (transitionKeys.has(key)) {
      diagnostics.push(
        error(
          "ambiguous_transition",
          `transitions[${index}]`,
          "Machine, action, source state, and priority must be unique.",
        ),
      );
    }
    transitionKeys.add(key);
    if (!machineIds.has(transition.machine_id)) {
      diagnostics.push(
        error(
          "unknown_machine",
          `transitions[${index}].machine_id`,
          "Transition references an unknown machine.",
        ),
      );
    }
    if (!actions.has(transition.action_id)) {
      diagnostics.push(
        error(
          "unknown_action",
          `transitions[${index}].action_id`,
          "Transition references an unknown action.",
        ),
      );
    }
    for (const [field, state] of [
      ["from_state", transition.from_state],
      ["to_state", transition.to_state],
    ] as const) {
      if (
        state !== null &&
        !stateMap.get(transition.machine_id)?.has(state)
      ) {
        diagnostics.push(
          error(
            "unknown_state",
            `transitions[${index}].${field}`,
            `State ${state} is not declared by this machine.`,
          ),
        );
      }
    }
    if (transition.effects.length === 0) {
      diagnostics.push({
        severity: "warning",
        code: "no_effects",
        path: `transitions[${index}].effects`,
        message: "This transition changes only machine state.",
      });
    }
  });
  return diagnostics;
}

function error(
  code: string,
  path: string,
  message: string,
): CompilerDiagnostic {
  return { severity: "error", code, path, message };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
