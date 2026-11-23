// CAP-005: the generated capability registry types. `cargo test -p
// glint-platform --test registry` keeps src/capabilities.ts in step with the
// Rust registry; this file checks the TypeScript side (run under
// `tsc --noEmit` and `node --test`).

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  CAPABILITY_ACCESS,
  CAPABILITY_IDS,
  CAPABILITY_NAMESPACES,
  CAPABILITY_REGISTRY,
  CAPABILITY_REGISTRY_VERSION,
  CAPABILITY_SENSITIVITIES,
  type CapabilityId,
  CUSTOM_SURFACE_CAPABILITY,
  SURFACE_CATALOG,
} from "../src/index.ts";

const registered = new Set<string>(CAPABILITY_IDS);

test("the registry is version 1 and lists each ID once", () => {
  assert.equal(CAPABILITY_REGISTRY_VERSION, 1);
  assert.equal(registered.size, CAPABILITY_IDS.length);
  assert.deepEqual(Object.keys(CAPABILITY_REGISTRY), [...CAPABILITY_IDS]);
});

test("every entry is consistent with its ID", () => {
  for (const id of CAPABILITY_IDS) {
    const spec = CAPABILITY_REGISTRY[id];
    assert.equal(spec.namespace, id.split(".")[0], id);
    assert.ok(CAPABILITY_NAMESPACES.includes(spec.namespace), id);
    assert.ok(CAPABILITY_ACCESS.includes(spec.access), id);
    assert.ok(CAPABILITY_SENSITIVITIES.includes(spec.sensitivity), id);
    if (spec.access === "read") assert.equal(spec.effect, "glint-owned-ui", id);
    if (spec.access === "write") assert.equal(spec.effect, "os-managed-setting", id);
  }
});

test("every surface catalog capability is registered", () => {
  assert.ok(registered.has(CUSTOM_SURFACE_CAPABILITY));
  for (const entry of Object.values(SURFACE_CATALOG)) {
    assert.ok(registered.has(entry.capability), entry.capability);
    for (const id of entry.optionalCapabilities) assert.ok(registered.has(id), id);
  }
});

test("unknown IDs are not capability IDs", () => {
  for (const id of ["system.shell.exec", "surface.taskbar", "surface.notification-overlay", ""]) {
    assert.equal(registered.has(id), false, id);
  }
  // @ts-expect-error: an unregistered ID is not a CapabilityId.
  const unknown: CapabilityId = "system.shell.exec";
  assert.equal(registered.has(unknown), false);
});
