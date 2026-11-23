// @glint/theme-schema: TypeScript types generated from
// schemas/glint-theme-v1.schema.json and a validator kept in parity with the
// Rust parser in crates/glint-schema (SCH-002). The format is PROPOSED.

export {
  ERROR_CODES,
  WARNING_CODES,
  type Diagnostic,
  type ErrorCode,
  type ManifestError,
  type ManifestWarning,
  type WarningCode,
} from "./codes.ts";
export type {
  Author,
  Compatibility,
  Renderer,
  Surface,
  ThemeManifest,
} from "./generated/manifest.ts";
export { isValidVersion, isValidVersionRequirement } from "./semver.ts";
export {
  MAX_MANIFEST_BYTES,
  SUPPORTED_ENTRYPOINT_VERSION,
  SUPPORTED_SCHEMA_VERSION,
  validateThemeManifest,
  type ValidationResult,
} from "./validate.ts";
