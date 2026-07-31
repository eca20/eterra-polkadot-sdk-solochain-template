import {
  FLOW_RUNTIME_ALIAS,
  type FlowRuntimeInteger,
} from "./transactions.js";

export const FLOW_STORAGE = {
  games: "Games",
  versions: "Versions",
  versionChunks: "VersionChunks",
  manifests: "Manifests",
  instances: "Instances",
  actorNonces: "ActorNonces",
  attestedSequences: "AttestedSequences",
  attestedReplayHashes: "AttestedReplayHashes",
  variableValues: "VariableValues",
  machineStates: "MachineStates",
  inventory: "Inventory",
} as const;

export type FlowStorageName =
  (typeof FLOW_STORAGE)[keyof typeof FLOW_STORAGE];

export interface FlowStateRead<
  Name extends FlowStorageName = FlowStorageName,
  Value = unknown,
> {
  pallet: typeof FLOW_RUNTIME_ALIAS;
  storage: Name;
  args: readonly unknown[];
  readonly __value?: Value;
}

export interface FlowStateReader {
  read(request: FlowStateRead): Promise<unknown>;
}

export const flowState = {
  game: (gameId: FlowRuntimeInteger) =>
    read(FLOW_STORAGE.games, [gameId]),
  version: (
    gameId: FlowRuntimeInteger,
    versionId: FlowRuntimeInteger,
  ) => read(FLOW_STORAGE.versions, [gameId, versionId]),
  versionChunk: (
    gameId: FlowRuntimeInteger,
    versionId: FlowRuntimeInteger,
    chunkIndex: FlowRuntimeInteger,
  ) => read(FLOW_STORAGE.versionChunks, [gameId, versionId, chunkIndex]),
  manifest: (
    gameId: FlowRuntimeInteger,
    versionId: FlowRuntimeInteger,
  ) => read(FLOW_STORAGE.manifests, [gameId, versionId]),
  instance: (instanceId: FlowRuntimeInteger) =>
    read(FLOW_STORAGE.instances, [instanceId]),
  actorNonce: (
    gameId: FlowRuntimeInteger,
    instanceId: FlowRuntimeInteger,
    actorId: FlowRuntimeInteger,
  ) => read(FLOW_STORAGE.actorNonces, [gameId, instanceId, actorId]),
  attestedSequence: (
    gameId: FlowRuntimeInteger,
    instanceId: FlowRuntimeInteger,
    authorityId: FlowRuntimeInteger,
    eventType: FlowRuntimeInteger,
  ) =>
    read(FLOW_STORAGE.attestedSequences, [
      gameId,
      instanceId,
      authorityId,
      eventType,
    ]),
  attestedReplayHash: (
    gameId: FlowRuntimeInteger,
    instanceId: FlowRuntimeInteger,
    authorityId: FlowRuntimeInteger,
    eventType: FlowRuntimeInteger,
    sequence: FlowRuntimeInteger,
  ) =>
    read(FLOW_STORAGE.attestedReplayHashes, [
      gameId,
      instanceId,
      authorityId,
      eventType,
      sequence,
    ]),
  variableValue: (
    gameId: FlowRuntimeInteger,
    instanceId: FlowRuntimeInteger,
    scope: unknown,
    variableId: FlowRuntimeInteger,
  ) =>
    read(FLOW_STORAGE.variableValues, [
      gameId,
      instanceId,
      scope,
      variableId,
    ]),
  machineState: (
    gameId: FlowRuntimeInteger,
    instanceId: FlowRuntimeInteger,
    scope: unknown,
    machineId: FlowRuntimeInteger,
  ) =>
    read(FLOW_STORAGE.machineStates, [
      gameId,
      instanceId,
      scope,
      machineId,
    ]),
  inventory: (
    gameId: FlowRuntimeInteger,
    instanceId: FlowRuntimeInteger,
    actorId: FlowRuntimeInteger,
    itemId: FlowRuntimeInteger,
  ) => read(FLOW_STORAGE.inventory, [gameId, instanceId, actorId, itemId]),
} as const;

export async function readFlowState<Value>(
  reader: FlowStateReader,
  request: FlowStateRead<FlowStorageName, Value>,
): Promise<Value> {
  return (await reader.read(request)) as Value;
}

export type FlowHash = `0x${string}`;

export type FlowEvent =
  | {
      type: "GameCreated";
      gameId: FlowRuntimeInteger;
      owner: unknown;
    }
  | {
      type: "VersionChunkUploaded";
      gameId: FlowRuntimeInteger;
      versionId: FlowRuntimeInteger;
      chunkIndex: FlowRuntimeInteger;
    }
  | {
      type: "VersionFinalized";
      gameId: FlowRuntimeInteger;
      versionId: FlowRuntimeInteger;
      manifestHash: FlowHash;
    }
  | {
      type: "VersionActivated";
      gameId: FlowRuntimeInteger;
      versionId: FlowRuntimeInteger;
    }
  | {
      type: "InstanceCreated";
      gameId: FlowRuntimeInteger;
      instanceId: FlowRuntimeInteger;
      versionId: FlowRuntimeInteger;
    }
  | {
      type: "ActionSubmitted";
      gameId: FlowRuntimeInteger;
      instanceId: FlowRuntimeInteger;
      actorId: FlowRuntimeInteger;
      machineId: FlowRuntimeInteger;
      actionId: FlowRuntimeInteger;
      transitionId: FlowRuntimeInteger;
      nonce: FlowRuntimeInteger;
    }
  | {
      type: "AttestedEventAccepted";
      gameId: FlowRuntimeInteger;
      instanceId: FlowRuntimeInteger;
      authorityId: FlowRuntimeInteger;
      eventType: FlowRuntimeInteger;
      nextSequence: FlowRuntimeInteger;
      replayHash: FlowHash | null;
    };

export interface FlowEventEnvelope {
  pallet?: string;
  section?: string;
  variant?: string;
  method?: string;
  fields?: Record<string, unknown> | readonly unknown[];
  data?: Record<string, unknown> | readonly unknown[];
}

/**
 * Normalize a metadata-decoded Substrate event into the stable Flow union.
 *
 * The caller's exact-metadata client remains responsible for SCALE decoding.
 * Named fields and metadata-ordered arrays are accepted; malformed Flow events
 * are rejected rather than silently coerced.
 */
export function decodeFlowEvent(
  envelope: FlowEventEnvelope,
): FlowEvent | undefined {
  const pallet = envelope.pallet ?? envelope.section;
  if (pallet !== FLOW_RUNTIME_ALIAS && pallet !== "eterraFlow") {
    return undefined;
  }
  const variant = envelope.variant ?? envelope.method;
  const data = envelope.fields ?? envelope.data;
  if (variant === undefined || data === undefined) {
    throw new TypeError("Flow event envelope is missing variant or fields");
  }

  switch (variant) {
    case "GameCreated": {
      const fields = eventFields(data, ["game_id", "owner"]);
      return {
        type: variant,
        gameId: integer(fields.game_id, "game_id"),
        owner: fields.owner,
      };
    }
    case "VersionChunkUploaded": {
      const fields = eventFields(data, [
        "game_id",
        "version_id",
        "chunk_index",
      ]);
      return {
        type: variant,
        gameId: integer(fields.game_id, "game_id"),
        versionId: integer(fields.version_id, "version_id"),
        chunkIndex: integer(fields.chunk_index, "chunk_index"),
      };
    }
    case "VersionFinalized": {
      const fields = eventFields(data, [
        "game_id",
        "version_id",
        "manifest_hash",
      ]);
      return {
        type: variant,
        gameId: integer(fields.game_id, "game_id"),
        versionId: integer(fields.version_id, "version_id"),
        manifestHash: hash(fields.manifest_hash, "manifest_hash"),
      };
    }
    case "VersionActivated": {
      const fields = eventFields(data, ["game_id", "version_id"]);
      return {
        type: variant,
        gameId: integer(fields.game_id, "game_id"),
        versionId: integer(fields.version_id, "version_id"),
      };
    }
    case "InstanceCreated": {
      const fields = eventFields(data, [
        "game_id",
        "instance_id",
        "version_id",
      ]);
      return {
        type: variant,
        gameId: integer(fields.game_id, "game_id"),
        instanceId: integer(fields.instance_id, "instance_id"),
        versionId: integer(fields.version_id, "version_id"),
      };
    }
    case "ActionSubmitted": {
      const fields = eventFields(data, [
        "game_id",
        "instance_id",
        "actor_id",
        "machine_id",
        "action_id",
        "transition_id",
        "nonce",
      ]);
      return {
        type: variant,
        gameId: integer(fields.game_id, "game_id"),
        instanceId: integer(fields.instance_id, "instance_id"),
        actorId: integer(fields.actor_id, "actor_id"),
        machineId: integer(fields.machine_id, "machine_id"),
        actionId: integer(fields.action_id, "action_id"),
        transitionId: integer(fields.transition_id, "transition_id"),
        nonce: integer(fields.nonce, "nonce"),
      };
    }
    case "AttestedEventAccepted": {
      const fields = eventFields(data, [
        "game_id",
        "instance_id",
        "authority_id",
        "event_type",
        "next_sequence",
        "replay_hash",
      ]);
      return {
        type: variant,
        gameId: integer(fields.game_id, "game_id"),
        instanceId: integer(fields.instance_id, "instance_id"),
        authorityId: integer(fields.authority_id, "authority_id"),
        eventType: integer(fields.event_type, "event_type"),
        nextSequence: integer(fields.next_sequence, "next_sequence"),
        replayHash:
          fields.replay_hash === null || fields.replay_hash === undefined
            ? null
            : hash(fields.replay_hash, "replay_hash"),
      };
    }
    default:
      return undefined;
  }
}

function read<Name extends FlowStorageName>(
  storage: Name,
  args: readonly unknown[],
): FlowStateRead<Name> {
  return {
    pallet: FLOW_RUNTIME_ALIAS,
    storage,
    args,
  };
}

function eventFields(
  data: Record<string, unknown> | readonly unknown[],
  names: readonly string[],
): Record<string, unknown> {
  if (Array.isArray(data)) {
    if (data.length !== names.length) {
      throw new TypeError(
        `Flow event expected ${names.length} fields, received ${data.length}`,
      );
    }
    return Object.fromEntries(names.map((name, index) => [name, data[index]]));
  }
  const record = data as Record<string, unknown>;
  return Object.fromEntries(
    names.map((name) => {
      const camel = name.replace(/_([a-z])/g, (_, letter: string) =>
        letter.toUpperCase(),
      );
      if (!(name in record) && !(camel in record)) {
        throw new TypeError(`Flow event is missing field ${name}`);
      }
      return [name, record[name] ?? record[camel]];
    }),
  );
}

function integer(value: unknown, field: string): FlowRuntimeInteger {
  if (
    typeof value === "bigint" ||
    (typeof value === "number" && Number.isSafeInteger(value)) ||
    (typeof value === "string" && /^(0|[1-9][0-9]*)$/.test(value))
  ) {
    return value;
  }
  throw new TypeError(`Flow event field ${field} is not an unsigned integer`);
}

function hash(value: unknown, field: string): FlowHash {
  if (typeof value === "string" && /^0x[0-9a-fA-F]{64}$/.test(value)) {
    return value as FlowHash;
  }
  throw new TypeError(`Flow event field ${field} is not a 32-byte hash`);
}
