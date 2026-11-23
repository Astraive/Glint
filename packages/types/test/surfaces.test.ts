// SRF-001: the generated surface types agree with the shared corpus that
// crates/glint-surfaces/tests/surface_manifest.rs checks against the Rust
// parser. `cargo test -p glint-surfaces --test typegen` keeps
// src/surfaces.ts in step with the Rust catalog; this file checks the
// TypeScript side (run under `tsc --noEmit` and `node --test`).

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  ANCHORS,
  DISMISS_TRIGGERS,
  FOCUS_POLICIES,
  MONITOR_TARGETS,
  PLACEMENT_KINDS,
  STANDARD_SURFACE_KINDS,
  SURFACE_CATALOG,
  SURFACE_ERROR_CODES,
  type SurfaceDefinition,
  type SurfaceEntryInput,
  type SurfaceKind,
} from "../src/index.ts";

interface Case {
  name: string;
  id: string;
  surface: unknown;
  expect?: Record<string, unknown>;
  code?: string;
  pointer?: string;
}

const corpus = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../../tests/package-fixtures/surfaces/cases.json",
);
const cases = JSON.parse(readFileSync(corpus, "utf8")) as { valid: Case[]; invalid: Case[] };

test("catalog keys are exactly the standard kinds, in order", () => {
  assert.deepEqual(Object.keys(SURFACE_CATALOG), [...STANDARD_SURFACE_KINDS]);
  for (const kind of STANDARD_SURFACE_KINDS) {
    const entry = SURFACE_CATALOG[kind];
    assert.equal(entry.capability, `surface.${kind}`);
    assert.ok(entry.placements.length > 0);
    for (const placement of entry.placements) assert.ok(PLACEMENT_KINDS.includes(placement));
  }
});

test("every corpus error code is a generated SurfaceErrorCode, and every code is used", () => {
  const codes: readonly string[] = SURFACE_ERROR_CODES;
  const used = new Set(cases.invalid.map((c) => c.code));
  for (const code of used) assert.ok(code !== undefined && codes.includes(code), String(code));
  for (const code of codes) assert.ok(used.has(code), code);
});

test("valid corpus expectations only use generated vocabulary", () => {
  const words: Record<string, readonly string[]> = {
    kind: PLACEMENT_KINDS,
    anchor: ANCHORS,
    monitor: MONITOR_TARGETS,
    focus: FOCUS_POLICIES,
  };
  for (const c of cases.valid) {
    const placement = c.expect?.placement as Record<string, unknown> | undefined;
    const behavior = c.expect?.behavior as Record<string, unknown> | undefined;
    for (const [field, value] of Object.entries({ ...placement, ...behavior })) {
      const allowed = words[field];
      if (allowed !== undefined) assert.ok(allowed.includes(String(value)), `${c.name}: ${field}`);
    }
    for (const trigger of (behavior?.dismissOn as string[] | undefined) ?? []) {
      assert.ok((DISMISS_TRIGGERS as readonly string[]).includes(trigger), c.name);
    }
  }
});

test("typed surface values compile against the generated types", () => {
  const custom: SurfaceKind = "custom:dev.example.clock";
  const input: SurfaceEntryInput = {
    entry: "dist/panel/index.html",
    placement: { kind: "edge", anchor: "top", height: 44, monitor: "all" },
    behavior: { focus: "non-activating", reserveWorkArea: true },
  };
  const resolved: SurfaceDefinition = {
    id: "panel",
    kind: "panel",
    entrypoint: input.entry,
    placement: { kind: "edge", anchor: "top", monitor: "all", height: 44, margin: 0 },
    behavior: { focus: "non-activating", reserveWorkArea: true, dismissOn: [], clickThrough: false },
    capabilities: { required: ["surface.panel"], optional: [] },
  };
  assert.equal(custom.startsWith("custom:"), true);
  assert.equal(resolved.entrypoint, "dist/panel/index.html");
});
