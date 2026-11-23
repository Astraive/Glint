// Stable diagnostic codes, mirroring `ErrorCode` and `WarningCode` in
// crates/glint-schema/src/error.rs. The fixture tests check both lists
// against tests/package-fixtures/manifests/expectations.json, which the Rust
// tests check against the Rust enums.

/** Every manifest error code, in the Rust declaration order. */
export const ERROR_CODES = [
  "GLINT_MANIFEST_INVALID_JSON",
  "GLINT_MANIFEST_TOO_LARGE",
  "GLINT_MANIFEST_DUPLICATE_KEY",
  "GLINT_MANIFEST_NOT_AN_OBJECT",
  "GLINT_MANIFEST_MISSING_SCHEMA_VERSION",
  "GLINT_MANIFEST_UNSUPPORTED_SCHEMA_VERSION",
  "GLINT_MANIFEST_MISSING_KIND",
  "GLINT_MANIFEST_WRONG_KIND",
  "GLINT_MANIFEST_MISSING_ID",
  "GLINT_MANIFEST_INVALID_ID",
  "GLINT_MANIFEST_MISSING_FIELD",
  "GLINT_MANIFEST_UNKNOWN_FIELD",
  "GLINT_MANIFEST_INVALID_TYPE",
  "GLINT_MANIFEST_INVALID_VALUE",
  "GLINT_MANIFEST_INVALID_VERSION",
  "GLINT_MANIFEST_INVALID_VERSION_REQUIREMENT",
  "GLINT_MANIFEST_INVALID_PATH",
  "GLINT_MANIFEST_INVALID_CAPABILITY",
  "GLINT_MANIFEST_UNSUPPORTED_RENDERER",
] as const;

/** Every manifest warning code. */
export const WARNING_CODES = ["GLINT_MANIFEST_WARN_UNKNOWN_FIELD"] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];
export type WarningCode = (typeof WARNING_CODES)[number];

/** A diagnostic: only `code` and `pointer` (RFC 6901) are stable. */
export interface Diagnostic<C extends string> {
  readonly code: C;
  readonly pointer: string;
  readonly message: string;
}

export type ManifestError = Diagnostic<ErrorCode>;
export type ManifestWarning = Diagnostic<WarningCode>;

/** Thrown internally to stop at the first problem, like the Rust parser. */
export class ManifestRejection extends Error {
  readonly diagnostic: ManifestError;

  constructor(code: ErrorCode, pointer: string, message: string) {
    super(`${code} at ${pointer === "" ? "(root)" : pointer}: ${message}`);
    this.diagnostic = { code, pointer, message };
  }
}

export function reject(code: ErrorCode, pointer: string, message: string): never {
  throw new ManifestRejection(code, pointer, message);
}
