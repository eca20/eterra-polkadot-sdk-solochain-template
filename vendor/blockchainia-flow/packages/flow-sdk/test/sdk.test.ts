import assert from "node:assert/strict";
import test from "node:test";

import {
  ETERRA_FLOW_AUTHORING_ALIAS,
  FLOW_AUTHORING_LABEL,
  FLOW_RUNTIME_ALIAS,
  assertDeterministicCompilation,
  decodeFlowEvent,
  flowState,
  prepareActivateVersion,
  prepareCreateInstance,
  prepareFinalizeVersion,
  preparePublishPlan,
  prepareUploadVersionChunk,
  readFlowState,
  type FlowManifest,
  type WasmCompiler,
} from "../src/index.js";

const manifest: FlowManifest = {
  manifest_version: FLOW_AUTHORING_LABEL,
  game_id: 1,
  version_id: 1,
  machines: [{ machine_id: 7, initial_state: 1, states: [1, 2] }],
  variables: [],
  actions: [9],
  transitions: [],
  event_definitions: [],
};

const compiler: WasmCompiler = {
  compileManifest(input) {
    const parsed = JSON.parse(input) as FlowManifest;
    assert.ok(
      parsed.manifest_version === FLOW_AUTHORING_LABEL ||
        parsed.manifest_version === ETERRA_FLOW_AUTHORING_ALIAS,
    );
    return JSON.stringify({
      ok: true,
      authoringLabel: FLOW_AUTHORING_LABEL,
      permanentAlias: ETERRA_FLOW_AUTHORING_ALIAS,
      canonicalAuthoringJson: JSON.stringify({
        ...parsed,
        manifest_version: FLOW_AUTHORING_LABEL,
      }),
      scaleHex: "0x00010203",
      manifestHashHex:
        "0x032251c5252f0d13230bd4a236cefcc6db32076502230fd03f70169cd402c433",
      metrics: {
        scaleBytes: 4,
        manifestChunks: 1,
        machines: 1,
        states: 2,
        variables: 0,
        actions: 1,
        transitions: 0,
        eventDefinitions: 0,
        attestedPolicies: 0,
      },
      diagnostics: [],
      graph: {},
      costEstimates: [],
    });
  },
};

test("compiler determinism and permanent alias remain typed", () => {
  const preferred = assertDeterministicCompilation(compiler, manifest);
  const aliased = assertDeterministicCompilation(compiler, {
    ...manifest,
    manifest_version: ETERRA_FLOW_AUTHORING_ALIAS,
  });
  assert.equal(preferred.scaleHex, aliased.scaleHex);
  assert.equal(preferred.manifestHashHex, aliased.manifestHashHex);
});

test("publish plan chunks bytes and never holds keys", () => {
  const compiled = assertDeterministicCompilation(compiler, manifest);
  const plan = preparePublishPlan(compiled, {
    gameId: 1,
    versionId: 1,
    maxChunkBytes: 2,
    includeActivation: true,
  });
  assert.equal(plan.keyCustody, "external");
  assert.deepEqual(
    plan.calls.map((prepared) => [
      prepared.call,
      prepared.callIndex,
    ]),
    [
      ["upload_version_chunk", 1],
      ["upload_version_chunk", 1],
      ["finalize_version", 2],
      ["activate_version", 3],
    ],
  );
  const finalize = plan.calls.find(
    (prepared) => prepared.call === "finalize_version",
  );
  assert.ok(finalize);
  assert.deepEqual(Object.keys(finalize.args).sort(), [
    "gameId",
    "manifestHash",
    "versionId",
  ]);
  assert.throws(
    () =>
      preparePublishPlan(compiled, {
        gameId: 999,
        versionId: 1,
      }),
    /publish target must match/,
  );
});

test("state reads preserve the frozen Eterra storage contract", async () => {
  const request = flowState.attestedSequence(1n, 2n, 3n, 4);
  assert.deepEqual(request, {
    pallet: FLOW_RUNTIME_ALIAS,
    storage: "AttestedSequences",
    args: [1n, 2n, 3n, 4],
  });
  const result = await readFlowState(
    {
      async read(readRequest) {
        assert.equal(readRequest.storage, "AttestedSequences");
        return 9n;
      },
    },
    request,
  );
  assert.equal(result, 9n);
});

test("instance preparation keeps the frozen call index and nullable version", () => {
  const prepared = prepareCreateInstance({
    gameId: 1n,
    instanceId: "2",
    versionId: null,
    configHash:
      "0x0000000000000000000000000000000000000000000000000000000000000000",
  });
  assert.equal(prepared.callIndex, 4);
  assert.equal(prepared.args.versionId, null);
  assert.equal(
    prepareUploadVersionChunk({
      gameId: 1,
      versionId: 2,
      chunkIndex: 0,
      chunk: new Uint8Array(),
    }).callIndex,
    1,
  );
  assert.equal(
    prepareFinalizeVersion({
      gameId: 1,
      versionId: 2,
      manifestHash:
        "0x0000000000000000000000000000000000000000000000000000000000000000",
    }).callIndex,
    2,
  );
  assert.equal(
    prepareActivateVersion({ gameId: 1, versionId: 2 }).callIndex,
    3,
  );
});

test("event decoder accepts named and ordered metadata fields", () => {
  assert.deepEqual(
    decodeFlowEvent({
      pallet: FLOW_RUNTIME_ALIAS,
      variant: "ActionSubmitted",
      fields: {
        game_id: 1n,
        instance_id: "2",
        actor_id: 3,
        machine_id: 4,
        action_id: 5,
        transition_id: 6n,
        nonce: "7",
      },
    }),
    {
      type: "ActionSubmitted",
      gameId: 1n,
      instanceId: "2",
      actorId: 3,
      machineId: 4,
      actionId: 5,
      transitionId: 6n,
      nonce: "7",
    },
  );
  assert.deepEqual(
    decodeFlowEvent({
      section: "eterraFlow",
      method: "VersionActivated",
      data: [10n, 11],
    }),
    {
      type: "VersionActivated",
      gameId: 10n,
      versionId: 11,
    },
  );
  assert.equal(
    decodeFlowEvent({
      pallet: "Balances",
      variant: "Transfer",
      fields: [],
    }),
    undefined,
  );
});
