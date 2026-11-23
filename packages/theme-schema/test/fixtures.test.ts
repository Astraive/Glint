// Golden corpus parity (SCH-002): the same files and expectations that
// crates/glint-schema/tests/fixtures.rs checks against the Rust parser.

import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { ERROR_CODES, WARNING_CODES, validateThemeManifest } from "../src/index.ts";

const corpus = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../../tests/package-fixtures/manifests",
);

interface Expectations {
  errorCodes: string[];
  warningCodes: string[];
  notCoveredByFixtures: Record<string, string>;
  valid: Record<string, { warnings: { code: string; pointer: string }[] }>;
  invalid: Record<string, { code: string; pointer: string }>;
}

interface VersionStrings {
  versions: { valid: string[]; invalid: string[] };
  requirements: { valid: string[]; invalid: string[] };
}

const readJson = <T>(name: string): T => JSON.parse(readFileSync(join(corpus, name), "utf8")) as T;
const expectations = readJson<Expectations>("expectations.json");
// Read as UTF-8 text like Rust's read_to_string; a leading BOM is kept.
const fixture = (dir: string, name: string): string => readFileSync(join(corpus, dir, name), "utf8");

test("every fixture has exactly one expectation", () => {
  for (const dir of ["valid", "invalid"] as const) {
    assert.deepEqual(readdirSync(join(corpus, dir)).sort(), Object.keys(expectations[dir]).sort(), dir);
  }
});

test("code lists match the expectations file (and so the Rust enums)", () => {
  assert.deepEqual([...ERROR_CODES], expectations.errorCodes);
  assert.deepEqual([...WARNING_CODES], expectations.warningCodes);
});

for (const [name, expected] of Object.entries(expectations.valid)) {
  test(`valid/${name} is accepted with the expected warnings`, () => {
    const result = validateThemeManifest(fixture("valid", name));
    assert.ok(result.ok, `rejected: ${JSON.stringify(result.ok ? null : result.error)}`);
    assert.deepEqual(
      result.warnings.map(({ code, pointer }) => ({ code, pointer })),
      expected.warnings,
    );
  });
}

for (const [name, expected] of Object.entries(expectations.invalid)) {
  test(`invalid/${name} is rejected with ${expected.code} at "${expected.pointer}"`, () => {
    const result = validateThemeManifest(fixture("invalid", name));
    assert.ok(!result.ok, "accepted");
    assert.deepEqual(
      { code: result.error.code, pointer: result.error.pointer },
      { code: expected.code, pointer: expected.pointer },
      result.error.message,
    );
  });
}

function minimalWith(pointer: string, value: string): string {
  const doc = {
    schemaVersion: 1,
    kind: "theme",
    id: "dev.example.minimal",
    name: "Minimal",
    version: "0.1.0",
    author: { name: "Glint Test Fixtures" },
    license: "MIT",
    compatibility: { glint: ">=0.1.0", platforms: ["windows"] },
  };
  if (pointer === "/version") doc.version = value;
  else doc.compatibility.glint = value;
  return JSON.stringify(doc);
}

test("version-string cases match the Rust semver crate", () => {
  const cases = readJson<VersionStrings>("version-strings.json");
  const axes = [
    ["versions", "/version", "GLINT_MANIFEST_INVALID_VERSION"],
    ["requirements", "/compatibility/glint", "GLINT_MANIFEST_INVALID_VERSION_REQUIREMENT"],
  ] as const;
  for (const [key, pointer, code] of axes) {
    for (const text of cases[key].valid) {
      const result = validateThemeManifest(minimalWith(pointer, text));
      assert.ok(result.ok, `${key} ${JSON.stringify(text)}: ${JSON.stringify(result)}`);
    }
    for (const text of cases[key].invalid) {
      const result = validateThemeManifest(minimalWith(pointer, text));
      assert.ok(!result.ok, `${key} ${JSON.stringify(text)} was accepted`);
      assert.deepEqual([result.error.code, result.error.pointer], [code, pointer], `${key} ${JSON.stringify(text)}`);
    }
  }
});
