import type { CompileSuccess } from "./types.js";

export const FLOW_RUNTIME_ALIAS = "EterraFlow" as const;
export const FLOW_PALLET_INDEX = 29 as const;

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
        gameId: number;
        metadataHash: `0x${string}`;
        metadataUri: Uint8Array;
      }
    >
  | PreparedCall<
      "upload_version_chunk",
      {
        gameId: number;
        versionId: number;
        chunkIndex: number;
        chunk: Uint8Array;
      }
    >
  | PreparedCall<
      "finalize_version",
      {
        gameId: number;
        versionId: number;
        chunkCount: number;
        manifestHash: `0x${string}`;
      }
    >
  | PreparedCall<
      "activate_version",
      { gameId: number; versionId: number }
    >;

export interface PublishPlanOptions {
  gameId: number;
  versionId: number;
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
  const maxChunkBytes = options.maxChunkBytes ?? 64 * 1024;
  if (!Number.isInteger(maxChunkBytes) || maxChunkBytes <= 0) {
    throw new RangeError("maxChunkBytes must be a positive integer");
  }
  const bytes = scaleHexToBytes(compiled.scaleHex);
  const chunks = chunkBytes(bytes, maxChunkBytes);
  const calls: PublishCall[] = [];

  if (options.includeCreateGame !== undefined) {
    calls.push(
      call("create_game", {
        gameId: options.gameId,
        metadataHash: options.includeCreateGame.metadataHash,
        metadataUri: options.includeCreateGame.metadataUri,
      }),
    );
  }
  chunks.forEach((chunk, chunkIndex) => {
    calls.push(
      call("upload_version_chunk", {
        gameId: options.gameId,
        versionId: options.versionId,
        chunkIndex,
        chunk,
      }),
    );
  });
  calls.push(
    call("finalize_version", {
      gameId: options.gameId,
      versionId: options.versionId,
      chunkCount: chunks.length,
      manifestHash: compiled.manifestHashHex,
    }),
  );
  if (options.includeActivation === true) {
    calls.push(
      call("activate_version", {
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

export function prepareAction(args: {
  gameId: number;
  instanceId: number;
  actorId: number;
  machineId: number;
  actionId: number;
  nonce: number;
  payload: Uint8Array;
}): PreparedCall<"submit_action", typeof args> {
  return call("submit_action", args);
}

export function prepareAttestedEvent(args: {
  gameId: number;
  instanceId: number;
  eventType: number;
  sequence: number;
  payload: Uint8Array;
  replayHash: `0x${string}`;
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
