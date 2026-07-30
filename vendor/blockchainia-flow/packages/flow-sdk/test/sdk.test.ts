import assert from "node:assert/strict";
import test from "node:test";

import {
  ETERRA_FLOW_AUTHORING_ALIAS,
  FLOW_AUTHORING_LABEL,
  assertDeterministicCompilation,
  preparePublishPlan,
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
});
