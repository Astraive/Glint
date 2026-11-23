// Hand-written mirror of crates/glint-schema/src/parse.rs (the authority).
// Every check runs in the same order with the same code and JSON Pointer, so
// both validators stop at the same first problem. Object keys are visited in
// UTF-8 byte order because serde_json's map is sorted that way.

import {
  ManifestRejection,
  reject,
  type ManifestError,
  type ManifestWarning,
} from "./codes.ts";
import type { ThemeManifest } from "./generated/manifest.ts";
import { isValidVersion, isValidVersionRequirement } from "./semver.ts";
import { parseStrictJson, toPlain, type JsonObject, type JsonValue } from "./strict-json.ts";

/** Largest accepted manifest, in UTF-8 bytes (1 MiB). */
export const MAX_MANIFEST_BYTES = 1024 * 1024;
/** The only `schemaVersion` understood. */
export const SUPPORTED_SCHEMA_VERSION = 1;
/** The only `renderer.entrypointVersion` understood. */
export const SUPPORTED_ENTRYPOINT_VERSION = 1;

export type ValidationResult =
  | { readonly ok: true; readonly manifest: ThemeManifest; readonly warnings: ManifestWarning[] }
  | { readonly ok: false; readonly error: ManifestError };

const TOP_LEVEL_FIELDS = [
  "$schema",
  "schemaVersion",
  "kind",
  "id",
  "version",
  "name",
  "description",
  "author",
  "license",
  "compatibility",
  "renderer",
  "requiredCapabilities",
  "optionalCapabilities",
  "tokens",
  "assets",
  "layouts",
  "surfaces",
];
const CAPABILITY_NAMESPACES = ["surface", "system", "desktop", "native", "host"];
const PLATFORMS = ["windows", "linux", "macos"];
const RESERVED_DEVICE_NAMES = [
  "con", "prn", "aux", "nul",
  "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
  "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

const MAX_ID_LEN = 128;
const MAX_NAME_LEN = 128;
const MAX_DESCRIPTION_LEN = 2048;
const MAX_SHORT_TEXT_LEN = 256;
const MAX_URL_LEN = 2048;
const MAX_VERSION_LEN = 64;
const MAX_PATH_LEN = 512;
const MAX_KEY_LEN = 64;
const MAX_CAPABILITY_LEN = 128;

const encoder = new TextEncoder();
const byteLength = (text: string): number => encoder.encode(text).length;
const charCount = (text: string): number => [...text].length;
/** Rust `str::trim().is_empty()`: Unicode White_Space only (U+FEFF is not). */
const isBlank = (text: string): boolean => /^\p{White_Space}*$/u.test(text);
/** Rust `char::is_control`: general category Cc. */
const hasControl = (text: string): boolean => /\p{Cc}/u.test(text);
const hasWhitespace = (text: string): boolean => /\p{White_Space}/u.test(text);
const asciiLower = (text: string): string => text.replace(/[A-Z]/g, (c) => c.toLowerCase());

/** Order of Rust `String` keys: byte-wise UTF-8, i.e. by code point. */
function compareUtf8(a: string, b: string): number {
  const x = [...a];
  const y = [...b];
  for (let i = 0; i < Math.min(x.length, y.length); i += 1) {
    const d = (x[i]?.codePointAt(0) ?? 0) - (y[i]?.codePointAt(0) ?? 0);
    if (d !== 0) return d;
  }
  return x.length - y.length;
}

/** RFC 6901 child pointer: `~` becomes `~0` and `/` becomes `~1`. */
function child(parent: string, key: string): string {
  return `${parent}/${key.replaceAll("~", "~0").replaceAll("/", "~1")}`;
}

function typeName(value: JsonValue): string {
  return value.type;
}

function wrongType(value: JsonValue, pointer: string, expected: string): never {
  return reject("GLINT_MANIFEST_INVALID_TYPE", pointer, `expected ${expected}, found ${typeName(value)}`);
}

/** A JSON object together with its pointer (Rust `Obj`). */
class Obj {
  readonly node: JsonObject;
  readonly ptr: string;

  constructor(node: JsonObject, ptr: string) {
    this.node = node;
    this.ptr = ptr;
  }

  child(key: string): string {
    return child(this.ptr, key);
  }

  get(key: string): JsonValue | undefined {
    return this.node.entries.get(key);
  }

  sortedKeys(): string[] {
    return [...this.node.entries.keys()].sort(compareUtf8);
  }

  required(key: string): JsonValue {
    const value = this.get(key);
    if (value === undefined) {
      reject("GLINT_MANIFEST_MISSING_FIELD", this.child(key), `required field \`${key}\` is missing`);
    }
    return value;
  }

  requiredObject(key: string): Obj {
    return asObject(this.required(key), this.child(key));
  }

  optionalObject(key: string): Obj | undefined {
    const value = this.get(key);
    return value === undefined ? undefined : asObject(value, this.child(key));
  }

  denyUnknown(allowed: readonly string[]): void {
    const key = this.sortedKeys().find((k) => !allowed.includes(k));
    if (key !== undefined) {
      reject(
        "GLINT_MANIFEST_UNKNOWN_FIELD",
        this.child(key),
        `unknown field \`${key}\`; allowed fields here are: ${allowed.join(", ")}`,
      );
    }
  }

  warnUnknown(allowed: readonly string[], warnings: ManifestWarning[]): void {
    for (const key of this.sortedKeys().filter((k) => !allowed.includes(k))) {
      warnings.push({
        code: "GLINT_MANIFEST_WARN_UNKNOWN_FIELD",
        pointer: this.child(key),
        message: `unknown field \`${key}\` was ignored; known fields here are: ${allowed.join(", ")}`,
      });
    }
  }
}

function asObject(value: JsonValue, pointer: string): Obj {
  if (value.type !== "object") wrongType(value, pointer, "an object");
  return new Obj(value, pointer);
}

function asString(value: JsonValue, pointer: string): string {
  if (value.type !== "string") wrongType(value, pointer, "a string");
  return value.value;
}

function asArray(value: JsonValue, pointer: string): readonly JsonValue[] {
  if (value.type !== "array") wrongType(value, pointer, "an array");
  return value.items;
}

function textValue(value: JsonValue, pointer: string, max: number): string {
  const text = asString(value, pointer);
  if (isBlank(text)) reject("GLINT_MANIFEST_INVALID_VALUE", pointer, "must not be blank");
  if (charCount(text) > max) {
    reject("GLINT_MANIFEST_INVALID_VALUE", pointer, `must be at most ${max} characters`);
  }
  if (hasControl(text)) {
    reject("GLINT_MANIFEST_INVALID_VALUE", pointer, "must not contain control characters");
  }
  return text;
}

// --------------------------------------------------------------------- fields

function schemaVersion(root: Obj): void {
  const pointer = "/schemaVersion";
  const value = root.get("schemaVersion");
  if (value === undefined) {
    reject("GLINT_MANIFEST_MISSING_SCHEMA_VERSION", pointer, "required field `schemaVersion` is missing");
  }
  if (value.type !== "number") wrongType(value, pointer, "an integer");
  if (value.u64 !== BigInt(SUPPORTED_SCHEMA_VERSION)) {
    reject(
      "GLINT_MANIFEST_UNSUPPORTED_SCHEMA_VERSION",
      pointer,
      `schemaVersion ${value.raw} is not supported; this parser supports ${SUPPORTED_SCHEMA_VERSION}`,
    );
  }
}

function kind(root: Obj): void {
  const pointer = "/kind";
  const value = root.get("kind");
  if (value === undefined) reject("GLINT_MANIFEST_MISSING_KIND", pointer, "required field `kind` is missing");
  const text = asString(value, pointer);
  if (text !== "theme") {
    reject("GLINT_MANIFEST_WRONG_KIND", pointer, `expected kind "theme", found ${JSON.stringify(text)}`);
  }
}

/** `[a-z0-9]+(-[a-z0-9]+)*` */
function isIdSegment(segment: string): boolean {
  return /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(segment);
}

function isValidId(id: string): boolean {
  const segments = id.split(".");
  return byteLength(id) <= MAX_ID_LEN && segments.length >= 2 && segments.every(isIdSegment);
}

function packageId(value: JsonValue, pointer: string): void {
  const id = asString(value, pointer);
  if (!isValidId(id)) {
    reject(
      "GLINT_MANIFEST_INVALID_ID",
      pointer,
      `${JSON.stringify(id)} is not a package id: use at least two dot-separated lowercase segments`,
    );
  }
}

function version(value: JsonValue, pointer: string): void {
  const text = asString(value, pointer);
  if (byteLength(text) > MAX_VERSION_LEN) reject("GLINT_MANIFEST_INVALID_VERSION", pointer, "version is too long");
  if (!isValidVersion(text)) {
    reject("GLINT_MANIFEST_INVALID_VERSION", pointer, `${JSON.stringify(text)} is not a semantic version`);
  }
}

function versionReq(value: JsonValue, pointer: string): void {
  const text = asString(value, pointer);
  if (isBlank(text) || byteLength(text) > MAX_VERSION_LEN) {
    reject(
      "GLINT_MANIFEST_INVALID_VERSION_REQUIREMENT",
      pointer,
      "version requirement must be non-blank and short",
    );
  }
  if (!isValidVersionRequirement(text)) {
    reject(
      "GLINT_MANIFEST_INVALID_VERSION_REQUIREMENT",
      pointer,
      `${JSON.stringify(text)} is not a semver requirement`,
    );
  }
}

function author(obj: Obj, warnings: ManifestWarning[]): void {
  // Non-sensitive display metadata: unknown keys warn (manifest-compatibility.md).
  obj.warnUnknown(["name", "email", "url"], warnings);
  textValue(obj.required("name"), obj.child("name"), MAX_NAME_LEN);
  const email = obj.get("email");
  if (email !== undefined) {
    const pointer = obj.child("email");
    const text = textValue(email, pointer, MAX_SHORT_TEXT_LEN);
    if (!text.includes("@") || hasWhitespace(text)) {
      reject("GLINT_MANIFEST_INVALID_VALUE", pointer, "not an e-mail address");
    }
  }
  const url = obj.get("url");
  if (url !== undefined) {
    const pointer = obj.child("url");
    const text = textValue(url, pointer, MAX_URL_LEN);
    if (!text.startsWith("https://") || hasWhitespace(text)) {
      reject("GLINT_MANIFEST_INVALID_VALUE", pointer, "author url must be an https:// URL");
    }
  }
}

function compatibility(obj: Obj): void {
  obj.denyUnknown(["glint", "sdk", "platforms"]);
  versionReq(obj.required("glint"), obj.child("glint"));
  const sdk = obj.get("sdk");
  if (sdk !== undefined) versionReq(sdk, obj.child("sdk"));
  const platformsPtr = obj.child("platforms");
  const items = asArray(obj.required("platforms"), platformsPtr);
  if (items.length === 0) reject("GLINT_MANIFEST_INVALID_VALUE", platformsPtr, "at least one platform is required");
  const seen: string[] = [];
  items.forEach((item, index) => {
    const pointer = `${platformsPtr}/${index}`;
    const name = asString(item, pointer);
    if (!PLATFORMS.includes(name)) {
      reject("GLINT_MANIFEST_INVALID_VALUE", pointer, `unknown platform ${JSON.stringify(name)}`);
    }
    if (seen.includes(name)) {
      reject("GLINT_MANIFEST_INVALID_VALUE", pointer, `platform ${JSON.stringify(name)} is listed twice`);
    }
    seen.push(name);
  });
}

function renderer(obj: Obj): void {
  obj.denyUnknown(["kind", "entrypointVersion"]);
  const kindPtr = obj.child("kind");
  const kindName = asString(obj.required("kind"), kindPtr);
  if (kindName !== "glint-web") {
    reject(
      "GLINT_MANIFEST_UNSUPPORTED_RENDERER",
      kindPtr,
      `renderer kind ${JSON.stringify(kindName)} is not supported; expected "glint-web"`,
    );
  }
  const versionPtr = obj.child("entrypointVersion");
  const value = obj.required("entrypointVersion");
  if (value.type !== "number") wrongType(value, versionPtr, "an integer");
  if (value.u64 !== BigInt(SUPPORTED_ENTRYPOINT_VERSION)) {
    reject(
      "GLINT_MANIFEST_UNSUPPORTED_RENDERER",
      versionPtr,
      `entrypointVersion ${value.raw} is not supported; expected ${SUPPORTED_ENTRYPOINT_VERSION}`,
    );
  }
}

/** `[a-z][a-z0-9]*(-[a-z0-9]+)*` */
function isNameSegment(segment: string): boolean {
  return /^[a-z]/.test(segment) && isIdSegment(segment);
}

function isValidCapability(capability: string): boolean {
  const [namespace, ...rest] = capability.split(".");
  return (
    byteLength(capability) <= MAX_CAPABILITY_LEN &&
    CAPABILITY_NAMESPACES.includes(namespace ?? "") &&
    rest.length >= 1 &&
    rest.every(isNameSegment)
  );
}

function capabilities(root: Obj, key: string, exclude: readonly string[]): string[] {
  const value = root.get(key);
  if (value === undefined) return [];
  const listPtr = root.child(key);
  const out: string[] = [];
  asArray(value, listPtr).forEach((item, index) => {
    const pointer = `${listPtr}/${index}`;
    const capability = asString(item, pointer);
    if (!isValidCapability(capability)) {
      reject(
        "GLINT_MANIFEST_INVALID_CAPABILITY",
        pointer,
        `${JSON.stringify(capability)} is not a capability id`,
      );
    }
    if (out.includes(capability)) {
      reject("GLINT_MANIFEST_INVALID_CAPABILITY", pointer, `capability ${JSON.stringify(capability)} is listed twice`);
    }
    if (exclude.includes(capability)) {
      reject(
        "GLINT_MANIFEST_INVALID_CAPABILITY",
        pointer,
        `capability ${JSON.stringify(capability)} is already required; it cannot also be optional`,
      );
    }
    out.push(capability);
  });
  return out;
}

function pathProblem(path: string): string | null {
  if (path === "") return "path must not be empty";
  if (byteLength(path) > MAX_PATH_LEN) return "path is too long";
  if (path.startsWith("/")) return "path must be relative to the package root";
  if (path.includes("\\")) return "use `/` as the separator; `\\` is not allowed";
  if (path.includes(":")) return "`:` is not allowed (drive letters, URLs and alternate data streams)";
  if (/[\p{Cc}*?"<>|]/u.test(path)) return "path contains a character that is not portable";
  for (const segment of path.split("/")) {
    if (segment === "") return "path must not contain empty segments or a trailing `/`";
    if (segment === "." || segment === "..") return "`.` and `..` segments are not allowed";
    if (segment.endsWith(".") || segment.endsWith(" ")) return "segments must not end with `.` or a space";
    const stem = segment.split(".")[0] ?? segment;
    if (RESERVED_DEVICE_NAMES.includes(asciiLower(stem))) return "segment is a reserved device name";
  }
  return null;
}

function packagePath(value: JsonValue, pointer: string): string {
  const path = asString(value, pointer);
  const problem = pathProblem(path);
  if (problem !== null) {
    reject(
      "GLINT_MANIFEST_INVALID_PATH",
      pointer,
      `${JSON.stringify(path)} is not a safe package-relative path: ${problem}`,
    );
  }
  return path;
}

function checkKey(key: string, pointer: string, valid: boolean): void {
  if (!valid) {
    reject(
      "GLINT_MANIFEST_INVALID_VALUE",
      pointer,
      `${JSON.stringify(key)} is not a valid name: use lowercase a-z, 0-9 and inner hyphens`,
    );
  }
}

function isValidKey(key: string): boolean {
  return byteLength(key) <= MAX_KEY_LEN && isNameSegment(key);
}

function pathMap(root: Obj, key: string): void {
  const obj = root.optionalObject(key);
  if (obj === undefined) return;
  for (const name of obj.sortedKeys()) {
    const pointer = obj.child(name);
    checkKey(name, pointer, isValidKey(name));
    packagePath(obj.required(name), pointer);
  }
}

function isValidSurfaceId(id: string): boolean {
  return id.startsWith("custom:") ? isValidId(id.slice("custom:".length)) : isValidKey(id);
}

function surfaces(root: Obj): number {
  const obj = root.optionalObject("surfaces");
  if (obj === undefined) return 0;
  const ids = obj.sortedKeys();
  for (const id of ids) {
    const pointer = obj.child(id);
    checkKey(id, pointer, isValidSurfaceId(id));
    const surface = asObject(obj.required(id), pointer);
    surface.denyUnknown(["entry", "placement"]);
    const entryPtr = surface.child("entry");
    const entry = packagePath(surface.required("entry"), entryPtr);
    const lower = asciiLower(entry);
    if (!(lower.endsWith(".html") || lower.endsWith(".htm"))) {
      reject("GLINT_MANIFEST_INVALID_PATH", entryPtr, "surface entry must be an .html document inside the package");
    }
    surface.optionalObject("placement");
  }
  return ids.length;
}

function check(text: string): ValidationResult {
  const size = byteLength(text);
  if (size > MAX_MANIFEST_BYTES) {
    reject("GLINT_MANIFEST_TOO_LARGE", "", `manifest is ${size} bytes; the limit is ${MAX_MANIFEST_BYTES}`);
  }
  const document = parseStrictJson(text.startsWith("﻿") ? text.slice(1) : text);
  if (document.type !== "object") {
    reject("GLINT_MANIFEST_NOT_AN_OBJECT", "", "manifest root must be a JSON object");
  }
  const root = new Obj(document, "");
  const warnings: ManifestWarning[] = [];

  // Version and kind first: without them no other field can be interpreted.
  schemaVersion(root);
  kind(root);
  root.denyUnknown(TOP_LEVEL_FIELDS);

  const hint = root.get("$schema");
  if (hint !== undefined) textValue(hint, root.child("$schema"), MAX_URL_LEN);

  const id = root.get("id");
  if (id === undefined) reject("GLINT_MANIFEST_MISSING_ID", "/id", "required field `id` is missing");
  packageId(id, "/id");
  textValue(root.required("name"), "/name", MAX_NAME_LEN);
  version(root.required("version"), "/version");
  const description = root.get("description");
  if (description !== undefined) textValue(description, "/description", MAX_DESCRIPTION_LEN);
  author(root.requiredObject("author"), warnings);
  textValue(root.required("license"), "/license", MAX_SHORT_TEXT_LEN);
  compatibility(root.requiredObject("compatibility"));
  const rendererObj = root.optionalObject("renderer");
  if (rendererObj !== undefined) renderer(rendererObj);

  const required = capabilities(root, "requiredCapabilities", []);
  capabilities(root, "optionalCapabilities", required);

  const tokens = root.get("tokens");
  if (tokens !== undefined) packagePath(tokens, "/tokens");
  pathMap(root, "assets");
  pathMap(root, "layouts");
  const surfaceCount = surfaces(root);

  if (surfaceCount > 0 && rendererObj === undefined) {
    reject("GLINT_MANIFEST_MISSING_FIELD", "/renderer", "`renderer` is required when `surfaces` declares any surface");
  }

  return { ok: true, manifest: toPlain(document) as ThemeManifest, warnings };
}

/**
 * Validate a `glint.json` theme manifest exactly like
 * `glint_schema::parse_theme_manifest_with_diagnostics`: the same verdict,
 * the same first error code and pointer, and the same warnings.
 */
export function validateThemeManifest(text: string): ValidationResult {
  try {
    return check(text);
  } catch (error) {
    if (error instanceof ManifestRejection) return { ok: false, error: error.diagnostic };
    throw error;
  }
}
