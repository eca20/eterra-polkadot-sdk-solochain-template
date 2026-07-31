import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const generatedRoot = resolve(
  root,
  "apps/builder/public/manifest-wasm",
);
const module = await import(
  pathToFileURL(
    resolve(generatedRoot, "blockchainia_flow_manifest_wasm.js"),
  ).href
);
const wasm = await readFile(
  resolve(generatedRoot, "blockchainia_flow_manifest_wasm_bg.wasm"),
);
await module.default({ module_or_path: wasm });

const input = JSON.parse(
  await readFile(
    resolve(root, "fixtures/wire/v0/inputs/zelda-door.flow.json"),
    "utf8",
  ),
);
const preferred = JSON.parse(module.compileManifest(JSON.stringify(input)));
const alias = JSON.parse(
  module.compileManifest(
    JSON.stringify({ ...input, manifest_version: "eterra.flow.v0" }),
  ),
);

assert.equal(module.compilerVersion(), "0.1.0-alpha.1");
assert.equal(preferred.ok, true);
assert.equal(preferred.authoringLabel, "blockchainia.flow.v0");
assert.equal(preferred.permanentAlias, "eterra.flow.v0");
assert.equal(
  preferred.manifestHashHex,
  "0x032251c5252f0d13230bd4a236cefcc6db32076502230fd03f70169cd402c433",
);
assert.equal(alias.scaleHex, preferred.scaleHex);
assert.equal(alias.manifestHashHex, preferred.manifestHashHex);

console.log(
  `verified WASM compiler ${module.compilerVersion()}: preferred/alias bytes identical`,
);
