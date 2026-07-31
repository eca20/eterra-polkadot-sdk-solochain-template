import assert from "node:assert/strict";
import test from "node:test";

import {
  addAction,
  addMachine,
  addState,
  cloneManifest,
  exportManifest,
  importManifest,
  starterManifest,
  summarizeCosts,
  validateDraft,
} from "../src/model.js";

test("import and export preserve the authoring contract", () => {
  const roundTrip = importManifest(exportManifest(starterManifest));
  assert.deepEqual(roundTrip, starterManifest);
});

test("draft validation reports ambiguous transitions and unknown states", () => {
  const draft = cloneManifest(starterManifest);
  const first = draft.transitions[0];
  assert.ok(first);
  first.to_state = 999;
  draft.transitions.push({ ...first, transition_id: 2 });
  const codes = validateDraft(draft).map((diagnostic) => diagnostic.code);
  assert.ok(codes.includes("unknown_state"));
  assert.ok(codes.includes("ambiguous_transition"));
});

test("visual edit helpers add bounded graph identities without collisions", () => {
  const draft = cloneManifest(starterManifest);
  const machineId = addMachine(draft);
  assert.equal(machineId, 1);
  assert.equal(addState(draft, machineId), 2);
  assert.equal(addAction(draft), 1);
  assert.deepEqual(draft.machines.at(-1), {
    machine_id: 1,
    initial_state: 1,
    states: [1, 2],
  });
});

test("cost summaries expose the worst runtime-facing estimate", () => {
  assert.deepEqual(
    summarizeCosts([
      {
        subject: { transition: 1 },
        storageReads: 3,
        storageWrites: 1,
        authorityProviderCalls: 0,
        economyProviderCalls: 2,
        profileProviderCalls: 0,
        conditionAtoms: 2,
        effects: 1,
      },
      {
        subject: { attestedEvent: 8 },
        storageReads: 2,
        storageWrites: 4,
        authorityProviderCalls: 1,
        economyProviderCalls: 0,
        profileProviderCalls: 2,
        conditionAtoms: 0,
        effects: 2,
      },
    ]),
    {
      subjects: 2,
      maxStorageReads: 3,
      maxStorageWrites: 4,
      maxProviderCalls: 3,
    },
  );
});
