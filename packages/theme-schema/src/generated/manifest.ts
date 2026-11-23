// GENERATED FILE - DO NOT EDIT.
// Source: schemas/glint-theme-v1.schema.json (PROPOSED format).
// Regenerate: corepack pnpm@10.18.0 --filter @glint/theme-schema generate
// CI fails when this file differs from the generator output.

/**
 * Root manifest of a Glint theme package. All paths are relative to the package root and must stay inside it.
 */
export interface ThemeManifest {
  /**
   * Editor hint only; Glint never dereferences it.
   */
  $schema?: string;
  /**
   * Package format version, independent of the SDK and renderer entrypoint versions.
   */
  schemaVersion: 1;
  /**
   * Package kind. Only "theme" exists in v1.
   */
  kind: "theme";
  /**
   * Package identifier: two or more dot-separated lowercase segments, e.g. dev.example.midnight.
   */
  id: string;
  /**
   * Semantic version (SemVer 2.0) of this package release.
   */
  version: string;
  /**
   * Display name.
   */
  name: string;
  /**
   * Optional short description.
   */
  description?: string;
  author: Author;
  /**
   * License expression; an SPDX identifier such as MIT is expected.
   */
  license: string;
  compatibility: Compatibility;
  renderer?: Renderer;
  /**
   * Capabilities the theme cannot run without.
   */
  requiredCapabilities?: string[];
  /**
   * Capabilities used when available. Must not repeat a required capability (enforced by the parser).
   */
  optionalCapabilities?: string[];
  /**
   * Path to the design-token file.
   */
  tokens?: string;
  /**
   * Named static assets, e.g. wallpaper and preview.
   */
  assets?: {
    /**
     * Relative, /-separated path inside the package: no leading /, no empty, . or .. segments, no backslash or colon, no Windows-reserved characters or device names, no segment ending in . or space.
     */
    [k: string]: string | undefined;
  };
  /**
   * Named layout files.
   */
  layouts?: {
    /**
     * Relative, /-separated path inside the package: no leading /, no empty, . or .. segments, no backslash or colon, no Windows-reserved characters or device names, no segment ending in . or space.
     */
    [k: string]: string | undefined;
  };
  /**
   * Surfaces the theme provides, keyed by surface ID (e.g. panel, launcher, control-center or custom:<package-id>).
   */
  surfaces?: {
    [k: string]: Surface | undefined;
  };
}
/**
 * Package author. Metadata only; never contacted by Glint. Unknown keys are not security-sensitive: they are ignored and reported as warnings.
 */
export interface Author {
  name: string;
  email?: string;
  url?: string;
  [k: string]: unknown | undefined;
}
/**
 * Supported Glint and SDK version ranges and operating systems.
 */
export interface Compatibility {
  /**
   * Cargo-style semver requirement, e.g. >=0.1.0 or ^0.1.0.
   */
  glint: string;
  /**
   * Cargo-style semver requirement, e.g. >=0.1.0 or ^0.1.0.
   */
  sdk?: string;
  /**
   * @minItems 1
   */
  platforms: ["windows" | "linux" | "macos", ...("windows" | "linux" | "macos")[]];
}
/**
 * Renderer contract for programmable surfaces. Required when any surface is declared.
 */
export interface Renderer {
  kind: "glint-web";
  entrypointVersion: 1;
}
export interface Surface {
  /**
   * Package-relative .html entry document.
   */
  entry: string;
  /**
   * Placement (kind, anchor, size, monitor); shape owned and validated by the surface engine (crates/glint-surfaces, SRF-001). See docs/architecture/surfaces.md.
   */
  placement?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Focus, dismissal, auto-hide and work-area behaviour; shape owned and validated by the surface engine (crates/glint-surfaces, SRF-001). See docs/architecture/surfaces.md.
   */
  behavior?: {
    [k: string]: unknown | undefined;
  };
}
