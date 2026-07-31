import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = await readFile(
  resolve(root, "crates/pallet-blockchainia-flow/src/lib.rs"),
  "utf8",
);
const contract = JSON.parse(
  await readFile(resolve(root, "fixtures/wire/v0/contract.json"), "utf8"),
);
const compatibility = contract.eterraCompatibility;

assert.equal(contract.release, "0.1.0-alpha.1");
assert.equal(contract.authoringLabels.preferred, "blockchainia.flow.v0");
assert.deepEqual(contract.authoringLabels.permanentAliases, ["eterra.flow.v0"]);
assert.equal(contract.runtimeManifestVersion, 0);
assert.equal(compatibility.runtimeAlias, "EterraFlow");
assert.equal(compatibility.palletIndex, 29);
assert.equal(compatibility.storageVersion, 2);
assert.match(
  source,
  /const STORAGE_VERSION: StorageVersion = StorageVersion::new\(2\);/,
);

for (const [call, index] of Object.entries(compatibility.callIndices)) {
  assert.match(
    source,
    new RegExp(
      String.raw`#\[pallet::call_index\(${index}\)\][\s\S]*?pub fn ${call}\(`,
    ),
    `call ${call} must remain at index ${index}`,
  );
  assert.equal(
    sha256(callSignature(source, call)),
    compatibility.callCodecSha256[call],
    `call ${call} argument codec changed`,
  );
}

for (const [storage, hashers] of Object.entries(compatibility.storage)) {
  const declaration = storageDeclaration(source, storage);
  assert.equal(
    occurrences(declaration, "Blake2_128Concat"),
    hashers.length,
    `${storage} hasher count changed`,
  );
  assert.equal(
    sha256(normalize(declaration)),
    compatibility.storageCodecSha256[storage],
    `${storage} key/value codec changed`,
  );
}

const eventBlock = boundedBlock(
  source,
  "pub enum Event<T: Config>",
  "#[pallet::error]",
);
assertOrderedDiscriminants(eventBlock, compatibility.eventDiscriminants, "event");
assert.equal(
  sha256(normalize(eventBlock)),
  compatibility.eventCodecSha256,
  "event field codec changed",
);

const errorBlock = boundedBlock(
  source,
  "pub enum Error<T>",
  "#[pallet::call]",
);
assertOrderedDiscriminants(errorBlock, compatibility.errorDiscriminants, "error");
assert.equal(
  sha256(normalize(errorBlock)),
  compatibility.errorCodecSha256,
  "error codec changed",
);

console.log(
  `verified Flow runtime contract: ${Object.keys(compatibility.storage).length} storage entries, ` +
    `${Object.keys(compatibility.callIndices).length} calls, ` +
    `${Object.keys(compatibility.eventDiscriminants).length} events, ` +
    `${Object.keys(compatibility.errorDiscriminants).length} errors`,
);

function storageDeclaration(text, name) {
  const start = text.indexOf(`pub type ${name}<`);
  assert.notEqual(start, -1, `storage ${name} is missing`);
  const nextStorage = text.indexOf("#[pallet::storage]", start + 1);
  const nextEvent = text.indexOf("#[pallet::event]", start + 1);
  const end = [nextStorage, nextEvent]
    .filter((position) => position !== -1)
    .reduce((left, right) => Math.min(left, right));
  return text.slice(start, end);
}

function boundedBlock(text, startToken, endToken) {
  const start = text.indexOf(startToken);
  assert.notEqual(start, -1, `${startToken} is missing`);
  const end = text.indexOf(endToken, start);
  assert.notEqual(end, -1, `${endToken} is missing`);
  return text.slice(start, end);
}

function callSignature(text, name) {
  const start = text.indexOf(`pub fn ${name}(`);
  assert.notEqual(start, -1, `call ${name} is missing`);
  const suffix = ") -> DispatchResult";
  const end = text.indexOf(`${suffix} {`, start);
  assert.notEqual(end, -1, `call ${name} signature is malformed`);
  return normalize(text.slice(start, end + suffix.length));
}

function assertOrderedDiscriminants(block, map, kind) {
  let cursor = -1;
  let expectedIndex = 0;
  for (const [variant, expected] of Object.entries(map)) {
    assert.equal(expected, expectedIndex, `${kind} indices must be contiguous`);
    const position = block.indexOf(variant);
    assert.ok(position > cursor, `${kind} ${variant} moved or is missing`);
    cursor = position;
    expectedIndex += 1;
  }
}

function occurrences(value, token) {
  return value.split(token).length - 1;
}

function normalize(value) {
  return value.replace(/\s+/g, " ").trim();
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}
