import type { CompileSuccess } from "./types.js";

export const FLOW_RUNTIME_ALIAS = "EterraFlow" as const;
export const FLOW_PALLET_INDEX = 29 as const;
export type FlowRuntimeInteger = number | bigint | string;

export const FLOW_CALL_INDEX = {
  create_game: 0,
  upload_version_chunk: 1,
  finalize_version: 2,
  activate_version: 3,
  create_instance: 4,
  submit_action: 5,
  submit_attested_event: 6,
} as const;

export interface PreparedCall<
  Name extends keyof typeof FLOW_CALL_INDEX,
  Args,
> {
  pallet: typeof FLOW_RUNTIME_ALIAS;
  palletIndex: typeof FLOW_PALLET_INDEX;
  call: Name;
  callIndex: (typeof FLOW_CALL_INDEX)[Name];
  args: Args;
}

export type PublishCall =
  | PreparedCall<
      "create_game",
      {
        gameId: FlowRuntimeInteger;
        metadataHash: `0x${string}`;
        metadataUri: Uint8Array;
      }
    >
  | PreparedCall<
      "upload_version_chunk",
      {
        gameId: FlowRuntimeInteger;
        versionId: FlowRuntimeInteger;
        chunkIndex: number;
        chunk: Uint8Array;
      }
    >
  | PreparedCall<
      "finalize_version",
      {
        gameId: FlowRuntimeInteger;
        versionId: FlowRuntimeInteger;
        manifestHash: `0x${string}`;
      }
    >
  | PreparedCall<
      "activate_version",
      { gameId: FlowRuntimeInteger; versionId: FlowRuntimeInteger }
    >;

export interface PublishPlanOptions {
  gameId: FlowRuntimeInteger;
  versionId: FlowRuntimeInteger;
  includeCreateGame?: {
    metadataHash: `0x${string}`;
    metadataUri: Uint8Array;
  };
  includeActivation?: boolean;
  maxChunkBytes?: number;
}

export interface UnsignedPublishPlan {
  runtimeAlias: typeof FLOW_RUNTIME_ALIAS;
  keyCustody: "external";
  calls: PublishCall[];
  scaleBytes: number;
  manifestHash: `0x${string}`;
}

export function preparePublishPlan(
  compiled: CompileSuccess,
  options: PublishPlanOptions,
): UnsignedPublishPlan {
  assertManifestIdentity(compiled, options);
  const maxChunkBytes = options.maxChunkBytes ?? 64 * 1024;
  if (!Number.isInteger(maxChunkBytes) || maxChunkBytes <= 0) {
    throw new RangeError("maxChunkBytes must be a positive integer");
  }
  const bytes = scaleHexToBytes(compiled.scaleHex);
  const chunks = chunkBytes(bytes, maxChunkBytes);
  const calls: PublishCall[] = [];

  if (options.includeCreateGame !== undefined) {
    calls.push(
      prepareCreateGame({
        gameId: options.gameId,
        metadataHash: options.includeCreateGame.metadataHash,
        metadataUri: options.includeCreateGame.metadataUri,
      }),
    );
  }
  chunks.forEach((chunk, chunkIndex) => {
    calls.push(
      prepareUploadVersionChunk({
        gameId: options.gameId,
        versionId: options.versionId,
        chunkIndex,
        chunk,
      }),
    );
  });
  calls.push(
    prepareFinalizeVersion({
      gameId: options.gameId,
      versionId: options.versionId,
      manifestHash: compiled.manifestHashHex,
    }),
  );
  if (options.includeActivation === true) {
    calls.push(
      prepareActivateVersion({
        gameId: options.gameId,
        versionId: options.versionId,
      }),
    );
  }
  return {
    runtimeAlias: FLOW_RUNTIME_ALIAS,
    keyCustody: "external",
    calls,
    scaleBytes: bytes.length,
    manifestHash: compiled.manifestHashHex,
  };
}

export function prepareCreateGame(args: {
  gameId: FlowRuntimeInteger;
  metadataHash: `0x${string}`;
  metadataUri: Uint8Array;
}): PreparedCall<"create_game", typeof args> {
  return call("create_game", args);
}

export function prepareUploadVersionChunk(args: {
  gameId: FlowRuntimeInteger;
  versionId: FlowRuntimeInteger;
  chunkIndex: number;
  chunk: Uint8Array;
}): PreparedCall<"upload_version_chunk", typeof args> {
  return call("upload_version_chunk", args);
}

export function prepareFinalizeVersion(args: {
  gameId: FlowRuntimeInteger;
  versionId: FlowRuntimeInteger;
  manifestHash: `0x${string}`;
}): PreparedCall<"finalize_version", typeof args> {
  return call("finalize_version", args);
}

export function prepareActivateVersion(args: {
  gameId: FlowRuntimeInteger;
  versionId: FlowRuntimeInteger;
}): PreparedCall<"activate_version", typeof args> {
  return call("activate_version", args);
}

export function prepareAction(args: {
  gameId: FlowRuntimeInteger;
  instanceId: FlowRuntimeInteger;
  actorId: FlowRuntimeInteger;
  machineId: FlowRuntimeInteger;
  actionId: FlowRuntimeInteger;
  nonce: FlowRuntimeInteger;
  payload: Uint8Array;
}): PreparedCall<"submit_action", typeof args> {
  return call("submit_action", args);
}

export function prepareCreateInstance(args: {
  gameId: FlowRuntimeInteger;
  instanceId: FlowRuntimeInteger;
  versionId: FlowRuntimeInteger | null;
  configHash: `0x${string}`;
}): PreparedCall<"create_instance", typeof args> {
  return call("create_instance", args);
}

export function prepareAttestedEvent(args: {
  gameId: FlowRuntimeInteger;
  instanceId: FlowRuntimeInteger;
  eventType: FlowRuntimeInteger;
  sequence: FlowRuntimeInteger;
  payload: Uint8Array;
  replayHash: `0x${string}` | null;
  effects: unknown[];
}): PreparedCall<"submit_attested_event", typeof args> {
  return call("submit_attested_event", args);
}

export function scaleHexToBytes(hex: `0x${string}`): Uint8Array {
  const payload = hex.slice(2);
  if (payload.length % 2 !== 0 || !/^[0-9a-f]*$/i.test(payload)) {
    throw new TypeError("scaleHex must be even-length hexadecimal");
  }
  return Uint8Array.from(
    payload.match(/.{2}/g)?.map((byte) => Number.parseInt(byte, 16)) ?? [],
  );
}

export function chunkBytes(
  bytes: Uint8Array,
  maxChunkBytes: number,
): Uint8Array[] {
  if (!Number.isInteger(maxChunkBytes) || maxChunkBytes <= 0) {
    throw new RangeError("maxChunkBytes must be a positive integer");
  }
  const chunks: Uint8Array[] = [];
  for (let offset = 0; offset < bytes.length; offset += maxChunkBytes) {
    chunks.push(bytes.slice(offset, offset + maxChunkBytes));
  }
  return chunks;
}

function call<Name extends keyof typeof FLOW_CALL_INDEX, Args>(
  name: Name,
  args: Args,
): PreparedCall<Name, Args> {
  return {
    pallet: FLOW_RUNTIME_ALIAS,
    palletIndex: FLOW_PALLET_INDEX,
    call: name,
    callIndex: FLOW_CALL_INDEX[name],
    args,
  };
}

function assertManifestIdentity(
  compiled: CompileSuccess,
  options: Pick<PublishPlanOptions, "gameId" | "versionId">,
): void {
  const canonical: unknown = JSON.parse(compiled.canonicalAuthoringJson);
  if (
    typeof canonical !== "object" ||
    canonical === null ||
    !("game_id" in canonical) ||
    !("version_id" in canonical)
  ) {
    throw new TypeError("compiler report is missing manifest identity");
  }
  const identity = canonical as {
    game_id: unknown;
    version_id: unknown;
  };
  if (
    decimal(identity.game_id) !== decimal(options.gameId) ||
    decimal(identity.version_id) !== decimal(options.versionId)
  ) {
    throw new RangeError(
      "publish target must match the compiled game_id and version_id",
    );
  }
}

function decimal(value: unknown): string {
  if (
    typeof value === "bigint" ||
    (typeof value === "number" && Number.isSafeInteger(value)) ||
    (typeof value === "string" && /^(0|[1-9][0-9]*)$/.test(value))
  ) {
    return String(value);
  }
  throw new TypeError("Flow runtime identity must be an unsigned integer");
}
