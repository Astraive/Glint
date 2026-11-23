// Unit tests for behaviour the shared corpus cannot hold as files.

import assert from "node:assert/strict";
import { test } from "node:test";

import { MAX_MANIFEST_BYTES, validateThemeManifest, type ThemeManifest } from "../src/index.ts";

const minimal = {
  schemaVersion: 1,
  kind: "theme",
  id: "dev.example.minimal",
  name: "Minimal",
  version: "0.1.0",
  author: { name: "Glint Test Fixtures" },
  license: "MIT",
  compatibility: { glint: ">=0.1.0", platforms: ["windows"] },
};

test("oversized manifest is rejected before parsing", () => {
  const text = JSON.stringify({ ...minimal, description: "a".repeat(MAX_MANIFEST_BYTES) });
  const result = validateThemeManifest(text);
  assert.ok(!result.ok);
  assert.equal(result.error.code, "GLINT_MANIFEST_TOO_LARGE");
  assert.equal(result.error.pointer, "");
});

test("size limit counts UTF-8 bytes, not UTF-16 units", () => {
  // 300 000 three-byte characters: under 1 MiB in UTF-16 units, over in bytes.
  const text = JSON.stringify({ ...minimal, description: "日".repeat(350_000) });
  assert.ok(text.length < MAX_MANIFEST_BYTES);
  const result = validateThemeManifest(text);
  assert.ok(!result.ok);
  assert.equal(result.error.code, "GLINT_MANIFEST_TOO_LARGE");
});

test("nesting limit is 127 containers, like serde_json", () => {
  const nest = (depth: number): string => "[".repeat(depth) + "]".repeat(depth);
  const ok = validateThemeManifest(nest(127));
  assert.ok(!ok.ok);
  assert.equal(ok.error.code, "GLINT_MANIFEST_NOT_AN_OBJECT");
  const deep = validateThemeManifest(nest(128));
  assert.ok(!deep.ok);
  assert.equal(deep.error.code, "GLINT_MANIFEST_INVALID_JSON");
});

test("accepted manifest is returned as plain typed data", () => {
  const result = validateThemeManifest(JSON.stringify(minimal));
  assert.ok(result.ok);
  const manifest: ThemeManifest = result.manifest;
  assert.equal(manifest.id, "dev.example.minimal");
  assert.deepEqual(manifest.compatibility.platforms, ["windows"]);
  assert.deepEqual(result.warnings, []);
});

test("__proto__ keys stay data and never touch prototypes", () => {
  const text = JSON.stringify(minimal).replace(
    '"author":{',
    '"author":{"__proto__":{"polluted":true},',
  );
  const result = validateThemeManifest(text);
  assert.ok(result.ok);
  assert.deepEqual(
    result.warnings.map((w) => w.pointer),
    ["/author/__proto__"],
  );
  assert.equal(({} as Record<string, unknown>).polluted, undefined);
  assert.equal(Object.getPrototypeOf(result.manifest.author), Object.prototype);
});

test("errors carry a human-readable message", () => {
  const result = validateThemeManifest("[]");
  assert.ok(!result.ok);
  assert.equal(result.error.code, "GLINT_MANIFEST_NOT_AN_OBJECT");
  assert.ok(result.error.message.length > 0);
});
