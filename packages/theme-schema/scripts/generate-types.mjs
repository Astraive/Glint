// Generate src/generated/manifest.ts from schemas/glint-theme-v1.schema.json,
// the single source of truth for the manifest's TypeScript types (SCH-002).
//
//   node scripts/generate-types.mjs          write the generated file
//   node scripts/generate-types.mjs --check  exit 1 if the committed file differs
//
// The generator is pinned (json-schema-to-typescript in package.json and
// pnpm-lock.yaml), runs offline and never dereferences remote $refs.

import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { compile } from "json-schema-to-typescript";

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, "..");
const repoRoot = resolve(packageRoot, "../..");
const schemaPath = resolve(repoRoot, "schemas/glint-theme-v1.schema.json");
const outputPath = resolve(packageRoot, "src/generated/manifest.ts");

const BANNER = [
  "// GENERATED FILE - DO NOT EDIT.",
  "// Source: schemas/glint-theme-v1.schema.json (PROPOSED format).",
  "// Regenerate: corepack pnpm@10.18.0 --filter @glint/theme-schema generate",
  "// CI fails when this file differs from the generator output.",
].join("\n");

// Keywords that only constrain values, never the shape of a type. They are
// enforced by the validator in src/validate.ts, so dropping them here keeps
// the generated types readable (`not` would otherwise widen string to
// `string | undefined`).
const VALIDATION_ONLY = ["not", "if", "then", "else", "pattern", "propertyNames"];

// Objects exported as named interfaces (JSON Pointer in the schema -> name).
const NAMED = {
  "/properties/author": "Author",
  "/properties/compatibility": "Compatibility",
  "/properties/renderer": "Renderer",
  "/$defs/surface": "Surface",
};

/**
 * Return a type-shaping copy of the schema; the schema file is not changed.
 * Local `$ref`s are inlined (sibling keywords such as `description` win) so
 * every property keeps its documentation and no duplicate aliases appear.
 */
function shapeForTypes(node, defs, pointer) {
  if (Array.isArray(node)) return node.map((item, i) => shapeForTypes(item, defs, `${pointer}/${i}`));
  if (node === null || typeof node !== "object") return node;
  let source = node;
  let at = pointer;
  if (typeof node.$ref === "string") {
    const name = node.$ref.replace(/^#\/\$defs\//, "");
    if (name === node.$ref || !(name in defs)) throw new Error(`unsupported $ref ${node.$ref}`);
    const { $ref, ...siblings } = node;
    source = { ...defs[name], ...siblings };
    at = `/$defs/${name}`;
  }
  const out = {};
  for (const [key, value] of Object.entries(source)) {
    if (VALIDATION_ONLY.includes(key) || key === "$defs") continue;
    out[key] = shapeForTypes(value, defs, `${at}/${key}`);
  }
  // `allOf: [{ $ref }, { pattern }]` narrows a value, not its type.
  if (Array.isArray(out.allOf)) {
    const members = out.allOf.filter((member) => Object.keys(member).length > 0);
    delete out.allOf;
    if (members.length === 1) Object.assign(out, { ...members[0], ...out });
    else if (members.length > 1) out.allOf = members;
  }
  if (NAMED[at]) out.title = NAMED[at];
  return out;
}

async function generate() {
  const raw = JSON.parse(await readFile(schemaPath, "utf8"));
  const schema = shapeForTypes(raw, raw.$defs ?? {}, "");
  // The root type is named explicitly instead of from $id or the title.
  delete schema.$id;
  delete schema.title;
  const body = await compile(schema, "ThemeManifest", {
    bannerComment: BANNER,
    unreachableDefinitions: false,
    strictIndexSignatures: true,
    format: true,
    $refOptions: { resolve: { external: false, file: false, http: false } },
    style: { endOfLine: "lf", printWidth: 100, singleQuote: false, trailingComma: "all" },
  });
  return body.replace(/\r\n/g, "\n");
}

const check = process.argv.includes("--check");
const generated = await generate();
const shown = relative(repoRoot, outputPath).replaceAll("\\", "/");

if (check) {
  let committed = null;
  try {
    committed = (await readFile(outputPath, "utf8")).replace(/\r\n/g, "\n");
  } catch {
    // Missing file is drift too.
  }
  if (committed !== generated) {
    const a = (committed ?? "").split("\n");
    const b = generated.split("\n");
    const line = a.findIndex((text, index) => text !== b[index]);
    const at = line === -1 ? Math.min(a.length, b.length) : line;
    console.error(`DRIFT: ${shown} is out of date with schemas/glint-theme-v1.schema.json.`);
    console.error(`First difference at line ${at + 1}:`);
    console.error(`  committed: ${JSON.stringify(a[at] ?? "<end of file>")}`);
    console.error(`  generated: ${JSON.stringify(b[at] ?? "<end of file>")}`);
    console.error("Run: corepack pnpm@10.18.0 --filter @glint/theme-schema generate");
    process.exit(1);
  }
  console.log(`PASS: ${shown} matches the generator output.`);
} else {
  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, generated, "utf8");
  console.log(`Wrote ${shown}.`);
}
