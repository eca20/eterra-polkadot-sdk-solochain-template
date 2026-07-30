import assert from "node:assert/strict";
import test from "node:test";

import {
  cloneManifest,
  exportManifest,
  importManifest,
  starterManifest,
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
