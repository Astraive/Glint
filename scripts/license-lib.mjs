// Shared helpers for the npm license check and the third-party inventory
// generator (REL-002). Policy rationale: docs/legal/license-policy.md.
// Uses only Node built-ins; it runs `pnpm licenses list` and `cargo metadata`.

import { spawnSync } from "node:child_process";

// SPDX identifiers allowed for dependencies that ship in or build Glint.
// Keep in sync with deny.toml (Rust) and docs/legal/license-policy.md.
// GPL, AGPL and LGPL are intentionally absent.
export const ALLOWED_NPM_LICENSES = new Set([
  "MIT",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "Unicode-3.0",
  "MPL-2.0",
  // Extra npm-only permissive licenses that appear in common tooling trees.
  "0BSD",
  "BlueOak-1.0.0",
  "CC0-1.0",
  "Python-2.0",
]);

// Evaluate an SPDX expression: `A OR B` passes if either side passes,
// `A AND B` needs both, parentheses group. Anything unparseable fails closed.
export function licenseAllowed(expression, allowed = ALLOWED_NPM_LICENSES) {
  const tokens = String(expression ?? "")
    .replace(/\(/g, " ( ")
    .replace(/\)/g, " ) ")
    .split(/\s+/)
    .filter(Boolean);
  let pos = 0;

  const parseOr = () => {
    let ok = parseAnd();
    while (tokens[pos] === "OR") {
      pos += 1;
      const rhs = parseAnd();
      ok = ok || rhs;
    }
    return ok;
  };
  const parseAnd = () => {
    let ok = parseAtom();
    while (tokens[pos] === "AND") {
      pos += 1;
      const rhs = parseAtom();
      ok = ok && rhs;
    }
    return ok;
  };
  const parseAtom = () => {
    const token = tokens[pos];
    pos += 1;
    if (token === undefined) throw new Error("unexpected end of expression");
    if (token === "(") {
      const ok = parseOr();
      if (tokens[pos] !== ")") throw new Error("missing closing parenthesis");
      pos += 1;
      return ok;
    }
    if (token === "OR" || token === "AND" || token === ")") {
      throw new Error(`unexpected token ${token}`);
    }
    return allowed.has(token.replace(/\+$/, ""));
  };

  try {
    const ok = parseOr();
    return ok && pos === tokens.length;
  } catch {
    return false;
  }
}

function run(command, args) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
    shell: false,
  });
  if (result.error) throw result.error;
  return result;
}

// Run pnpm the way the invoking pnpm did when possible (npm_execpath), so the
// scripts work both under `pnpm <script>` and with a plain `pnpm` on PATH.
function pnpm(args) {
  const execPath = process.env.npm_execpath;
  if (execPath && /pnpm/i.test(execPath)) {
    return run(process.execPath, [execPath, ...args]);
  }
  return run("pnpm", args);
}

// Returns [{ name, version, license }] for installed npm packages, sorted.
// `prodOnly` limits the list to production dependencies.
export function listNpmPackages({ prodOnly }) {
  const args = ["licenses", "list", "--json"];
  if (prodOnly) args.push("--prod");
  const result = pnpm(args);
  const text = result.stdout.trim();
  if (result.status !== 0 || !text.startsWith("{")) {
    // pnpm prints "No licenses in packages found" and exits 0 for an empty set.
    if (/No licenses in packages found/.test(result.stdout + result.stderr)) {
      return [];
    }
    throw new Error(
      `pnpm licenses list failed (exit ${result.status}): ${result.stderr || result.stdout}`,
    );
  }
  const byLicense = JSON.parse(text);
  const rows = [];
  for (const entries of Object.values(byLicense)) {
    for (const entry of entries) {
      for (const version of entry.versions) {
        rows.push({ name: entry.name, version, license: entry.license });
      }
    }
  }
  return collapsePlatformBinaries(rows).sort(
    (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
  );
}

// Biome ships one optional binary package per OS and pnpm installs only the
// host's. Collapse them so the inventory is identical on every OS.
function collapsePlatformBinaries(rows) {
  const out = [];
  let biome;
  for (const row of rows) {
    if (row.name.startsWith("@biomejs/cli-")) {
      biome ??= {
        name: "@biomejs/cli-<platform>",
        version: row.version,
        license: row.license,
      };
    } else {
      out.push(row);
    }
  }
  if (biome) out.push(biome);
  return out;
}

// Returns [{ name, version, license, source }] for non-workspace crates.
export function listCratePackages() {
  const result = run("cargo", ["metadata", "--locked", "--format-version", "1"]);
  if (result.status !== 0) {
    throw new Error(`cargo metadata failed (exit ${result.status}): ${result.stderr}`);
  }
  const metadata = JSON.parse(result.stdout);
  const workspace = new Set(metadata.workspace_members);
  return metadata.packages
    .filter((pkg) => !workspace.has(pkg.id))
    .map((pkg) => ({
      name: pkg.name,
      version: pkg.version,
      license: pkg.license ?? "UNKNOWN",
      source: pkg.source ?? "path",
    }))
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}
