# Package security fixture suite

Status: **implemented** (PKG-001, PKG-002, PKG-003, PKG-004). This page
lists every hostile `.glint` fixture in
[`crates/glint-package/tests/security.rs`](../../crates/glint-package/tests/security.rs)
and
[`crates/glint-package/tests/references.rs`](../../crates/glint-package/tests/references.rs)
(manifest references and static content restrictions, PKG-004) and the
stable error code each must produce. The rules behind the codes come
from the [package security spike](../research/spikes/package-security.md).

## How the suite works

- **Generated, never committed.** Every archive is built in memory at test
  time by the raw ZIP builder in
  [`tests/support/mod.rs`](../../crates/glint-package/tests/support/mod.rs),
  ported from the SPK-007 spike. The largest fixture is 8 MiB of zeros,
  deflated in memory to about 8 KiB. Provenance: all bytes are synthesized by
  that file; license MIT (repository license).
- **Specific code.** Each hostile case asserts one `GLINT_PACKAGE_*` code,
  plus the `PathRejection` rule for unsafe names and the `GLINT_MANIFEST_*`
  code from `glint-schema` for invalid manifests and the `GLINT_SURFACE_*`
  code plus JSON Pointer from `glint-surfaces` for invalid surfaces. "Some
  error" is never accepted.
- **No writes.** Each case runs `inspect_archive_in` inside a fresh temporary
  sandbox (`input/fixture.glint` plus an empty `staging/` root) and snapshots
  the whole sandbox before and after. The tree must be identical afterwards
  and the staging root must be empty: no private copy, no partial
  extraction and no escaped file is left behind.
- **Executables are never run.** The reader has no code path that executes
  package content. Executables and scripts are rejected by extension or magic
  bytes; the fixtures assert the rejection and that no file remains.
- **Invalid manifests never reach the live profile.** No profile store exists
  yet (PRF/INS). At the reader level, an invalid or corrupt manifest is
  rejected before extraction and leaves nothing in staging, so there is
  nothing an installer could activate.

## Running it

```sh
cargo test -p glint-package --test security
cargo test -p glint-package --test references
cargo test -p glint-package --test roundtrip
```

CI runs both in the `Rust workspace` job of
[`repository-checks.yml`](../../.github/workflows/repository-checks.yml) on
`ubuntu-latest`, `windows-latest` and `macos-latest`: once inside
`cargo test --workspace`, and again as the named step "Run package security
fixture suite" with `--nocapture`, so any skip message is visible in the log.

Results recorded so far:

| Host | Result |
|---|---|
| Windows 11 (hands-on host, Rust 1.95.0) | `security`: 135 passed; `references`: 37 passed; `roundtrip`: 24 passed; no skips (the file-symlink test created a symlink, so it ran) |
| Linux, macOS | **Not run locally.** CI only; record the CI run before claiming a pass |

## Platform-specific and skippable tests

Nothing is silently skipped. Tests that cannot exist on a platform are
compiled out with `#[cfg]`, and the run-time skip prints `SKIPPED: ...`:

| Test | Platforms | Notes |
|---|---|---|
| `symlinked_staging_root_is_refused` | Unix | Creating symlinks on Windows needs Developer Mode |
| `world_writable_staging_root_is_refused` | Unix | Windows has no mode bits; the staging root's ACL is the installer's job |
| `case_folded_collision_is_refused` (roundtrip) | Unix | Prints `SKIPPED` on a case-insensitive file system (default macOS APFS), where the collision cannot be created |
| `symlink_to_outside_the_theme_is_refused` (roundtrip) | Unix | |
| `junction_to_outside_the_theme_is_refused` (roundtrip) | Windows | Uses `mklink /J`, which needs no privilege |
| `file_symlink_is_refused_when_the_host_allows_creating_one` (roundtrip) | Windows | Prints `SKIPPED` when symlink creation is not permitted |

## Fixtures and expected codes

Unless noted, each fixture is the honest baseline package plus or minus one
change. The baseline itself is accepted (`good_package_extracts_read_only_with_matching_hashes`).

### Path traversal, absolute, UNC and device paths

All produce `GLINT_PACKAGE_UNSAFE_PATH` with the listed rule.

| Fixture | Entry name | `PathRejection` |
|---|---|---|
| `traversal_parent_dir` | `../escaped.txt` | `ParentDir` |
| `traversal_nested_parent_dir` | `dist/../../escaped.txt` | `ParentDir` |
| `traversal_backslash_parent_dir` | `..\escaped.txt` | `Backslash` |
| `absolute_posix_path` | `/tmp/escaped.txt` | `Absolute` |
| `absolute_backslash_root` | `\Windows\escaped.txt` | `Absolute` |
| `absolute_windows_drive` | `C:\Windows\escaped.txt` | `DriveOrColon` |
| `absolute_windows_drive_forward_slash` | `C:/Windows/escaped.txt` | `DriveOrColon` |
| `drive_relative_path` | `C:escaped.txt` | `DriveOrColon` |
| `drive_prefix_inside_path` | `dist/c:escaped.txt` | `DriveOrColon` |
| `ntfs_alternate_data_stream` | `glint.json:hidden` | `DriveOrColon` |
| `unc_path_backslash` | `\\server\share\escaped.txt` | `UncOrDevicePath` |
| `unc_path_forward_slash` | `//server/share/escaped.txt` | `UncOrDevicePath` |
| `device_namespace_path` | `\\.\PhysicalDrive0` | `UncOrDevicePath` |
| `verbatim_path` | `\\?\C:\escaped.txt` | `UncOrDevicePath` |
| `backslash_separator` | `dist\panel\x.js` | `Backslash` |
| `current_dir_component` | `./x.txt` | `CurDir` |
| `empty_component` | `dist//x.txt` | `EmptyComponent` |
| `empty_name` | (zero-length name) | `Empty` |
| `nul_byte_in_name` | `x.txt<NUL>.png` | `ControlChar` |
| `control_char_in_name` | `x<BEL>.txt` | `ControlChar` |
| `non_utf8_name` | bytes `61 81 FE 2E 74` | `NotUtf8` |
| `non_ascii_name_by_default` | `café.png` | `NonAscii` |

### Windows device names and namespace quirks

All produce `GLINT_PACKAGE_UNSAFE_PATH`. The rules are the same on every
host, unlike the `zip` crate's own helpers.

| Fixture | Entry name | `PathRejection` |
|---|---|---|
| `device_name_con` | `CON` | `ReservedDeviceName` |
| `device_name_nul_with_extension` | `assets/nul.txt` | `ReservedDeviceName` |
| `device_name_com1_json` | `COM1.json` | `ReservedDeviceName` |
| `device_name_lpt9_dir` | `lpt9/x.txt` | `ReservedDeviceName` |
| `device_name_conin` | `CONIN$` | `ReservedDeviceName` |
| `device_name_superscript_com` | `COM¹.txt` (non-ASCII allowed) | `ReservedDeviceName` |
| `trailing_dot_component` | `assets/x.txt.` | `TrailingDotOrSpace` |
| `trailing_space_component` | `assets /x.txt` | `TrailingDotOrSpace` |
| `leading_space_component` | `assets/ x.txt` | `TrailingDotOrSpace` |
| `short_name_alias` | `PROGRA~1/x.txt` | `ForbiddenChar` |
| `wildcard_char` | `assets/*.txt` | `ForbiddenChar` |

### Depth and length

| Fixture | Change | Expected |
|---|---|---|
| `deep_nesting` | 17 components | `GLINT_PACKAGE_UNSAFE_PATH` / `TooDeep` |
| `path_too_long` | 241-byte name | `GLINT_PACKAGE_UNSAFE_PATH` / `TooLong` |
| `component_too_long` | 129-byte component | `GLINT_PACKAGE_UNSAFE_PATH` / `ComponentTooLong` |

### Symlinks, hardlinks, special files and extra fields

| Fixture | Change | Expected |
|---|---|---|
| `symlink_unix_mode` | Unix host, `S_IFLNK` mode | `GLINT_PACKAGE_SYMLINK` |
| `symlink_bits_on_dos_host` | `S_IFLNK` bits with an MS-DOS host (the crate ignores them) | `GLINT_PACKAGE_SYMLINK` |
| `symlink_bits_on_ntfs_host` | `S_IFLNK` bits with an NTFS host | `GLINT_PACKAGE_SYMLINK` |
| `symlink_directory_entry` | directory entry with `S_IFLNK` | `GLINT_PACKAGE_SYMLINK` |
| `symlink_written_by_zip_crate_writer_is_rejected` | `ZipWriter::add_symlink` output | `GLINT_PACKAGE_SYMLINK` |
| `character_device_mode` | `S_IFCHR` | `GLINT_PACKAGE_NOT_REGULAR_FILE` |
| `fifo_mode` | `S_IFIFO` | `GLINT_PACKAGE_NOT_REGULAR_FILE` |
| `unicode_path_extra_field` | Info-ZIP Unicode Path `0x7075` renaming to `../escaped.txt` | `GLINT_PACKAGE_MALFORMED` (crate header-consistency check fires first) |
| `unicode_path_extra_field_case_rename` | `0x7075` case-only rename, no UTF-8 flag | `GLINT_PACKAGE_FORBIDDEN_EXTRA_FIELD` |
| `pkware_unix_extra_field_link_target` | PKWARE Unix `0x000d` carrying a link target (symlink or hardlink) | `GLINT_PACKAGE_FORBIDDEN_EXTRA_FIELD` |
| `unknown_extra_field` | extra field `0xCAFE` | `GLINT_PACKAGE_FORBIDDEN_EXTRA_FIELD` |

### Duplicates and collisions

| Fixture | Change | Expected |
|---|---|---|
| `exact_duplicate_names` | two `theme/tokens.json` records | `GLINT_PACKAGE_DUPLICATE_NAME` |
| `case_fold_duplicate` | `assets/A.txt` and `assets/a.txt` | `GLINT_PACKAGE_PATH_COLLISION` |
| `case_fold_manifest_shadow` | `GLINT.JSON` next to `glint.json` | `GLINT_PACKAGE_PATH_COLLISION` |
| `case_fold_directory_collision` | `Dist/panel/index.html` next to `dist/...` | `GLINT_PACKAGE_PATH_COLLISION` |
| `file_and_directory_same_path` | file `assets` and `assets/x.txt` | `GLINT_PACKAGE_PATH_COLLISION` |
| `file_shadows_directory_case_folded` | file `Assets` and `assets/x.txt` | `GLINT_PACKAGE_PATH_COLLISION` |
| `unicode_case_fold_collision_detected_when_non_ascii_allowed` | `Été.png` and `été.png`, non-ASCII allowed | `GLINT_PACKAGE_PATH_COLLISION` |
| `overlapping_entries_share_data` | two records share one compressed stream | `GLINT_PACKAGE_OVERLAPPING_ENTRIES` |
| `local_and_central_names_differ` | local header says `../escaped.txt` | `GLINT_PACKAGE_MALFORMED` |
| `local_entry_hidden_from_central_directory` | local entry missing from the central directory | `GLINT_PACKAGE_MALFORMED` |

### Compression bombs and size budgets

| Fixture | Change | Expected |
|---|---|---|
| `high_compression_ratio` | 8 MiB of zeros (ratio about 1000) | `GLINT_PACKAGE_RATIO_EXCEEDED` |
| `archive_wide_ratio_from_small_entries` | 8 × 0.5 MiB of zeros, each below `ratio_min_bytes` | `GLINT_PACKAGE_RATIO_EXCEEDED` |
| `declared_size_understated` | declares 1000 bytes, inflates to 4 MiB | `GLINT_PACKAGE_MALFORMED` (the crate stops at the declared size) |
| `declared_size_overstated` | declares 5000 bytes, inflates to 4 | `GLINT_PACKAGE_SIZE_MISMATCH` |
| `stored_sizes_disagree` | Stored entry, compressed size ≠ declared size | `GLINT_PACKAGE_SIZE_MISMATCH` |
| `entry_too_large` | 2 MiB file, `max_file_uncompressed` = 1 MiB | `GLINT_PACKAGE_ENTRY_TOO_LARGE` |
| `total_too_large` | 4 × 0.5 MiB, `max_total_uncompressed` = 1 MiB | `GLINT_PACKAGE_TOTAL_TOO_LARGE` |
| `archive_file_too_large` | 1 MiB archive, `max_archive_bytes` = 512 KiB | `GLINT_PACKAGE_ARCHIVE_TOO_LARGE` |
| `excessive_entry_count` | 10 007 entries | `GLINT_PACKAGE_TOO_MANY_ENTRIES` |
| `entry_count_above_custom_limit` | baseline, `max_entries` = 3 | `GLINT_PACKAGE_TOO_MANY_ENTRIES` |
| `end_of_central_directory_claims_too_many_entries` | end record claims 50 000 entries | `GLINT_PACKAGE_TOO_MANY_ENTRIES` (before the crate parses the directory) |
| `scanned_text_too_large` | 4 KiB JS, `max_scanned_text_bytes` = 1 KiB | `GLINT_PACKAGE_TEXT_TOO_LARGE` |

### Encryption and compression methods

| Fixture | Change | Expected |
|---|---|---|
| `zipcrypto_encrypted_entry` | encrypted flag | `GLINT_PACKAGE_ENCRYPTED` |
| `strong_encryption_flag` | strong-encryption flag (bit 6) | `GLINT_PACKAGE_ENCRYPTED` |
| `aes_encrypted_entry` | method 99 with AES extra field `0x9901` | `GLINT_PACKAGE_FORBIDDEN_EXTRA_FIELD` |
| `aes_method_without_extra_field` | method 99 | `GLINT_PACKAGE_UNSUPPORTED_COMPRESSION` |
| `bzip2_method` | method 12 | `GLINT_PACKAGE_UNSUPPORTED_COMPRESSION` |
| `lzma_method` | method 14 | `GLINT_PACKAGE_UNSUPPORTED_COMPRESSION` |

### Envelope: magic bytes and directory records

| Fixture | Change | Expected |
|---|---|---|
| `not_a_zip_text` | plain text | `GLINT_PACKAGE_NOT_ZIP` |
| `not_a_zip_png` | PNG header | `GLINT_PACKAGE_NOT_ZIP` |
| `empty_file` | zero bytes | `GLINT_PACKAGE_NOT_ZIP` |
| `polyglot_prepended_executable` | `MZ` stub before a valid ZIP | `GLINT_PACKAGE_NOT_ZIP` |
| `truncated_archive` | first 60 bytes only | `GLINT_PACKAGE_MALFORMED` |
| `zip64_end_of_central_directory_marker` | entry counts `0xFFFF` | `GLINT_PACKAGE_UNSUPPORTED_ARCHIVE_FEATURE` |
| `zip64_locator_before_end_record` | ZIP64 locator before the end record | `GLINT_PACKAGE_UNSUPPORTED_ARCHIVE_FEATURE` |
| `multi_disk_archive` | disk number 1 | `GLINT_PACKAGE_UNSUPPORTED_ARCHIVE_FEATURE` |
| `archive_comment` | 7-byte archive comment | `GLINT_PACKAGE_UNSUPPORTED_ARCHIVE_FEATURE` |
| `trailing_data_after_end_record` | junk after the end record | `GLINT_PACKAGE_MALFORMED` |
| `planted_end_record_in_comment` | a second, fake end record inside the comment | `GLINT_PACKAGE_MALFORMED` |
| `data_descriptor_flag` | streamed entry (bit 3) | `GLINT_PACKAGE_UNSUPPORTED_ARCHIVE_FEATURE` |
| `masked_local_headers_flag` | masked local headers (bit 13) | `GLINT_PACKAGE_UNSUPPORTED_ARCHIVE_FEATURE` |
| `source_that_is_a_directory_is_refused` | the source path is a directory | `GLINT_PACKAGE_SOURCE_NOT_FILE` (or `GLINT_PACKAGE_IO` on Windows, which cannot open a directory as a file) |

### Executables and scripts (rejected, never run)

All produce `GLINT_PACKAGE_EXECUTABLE_CONTENT`.

| Fixture | Change |
|---|---|
| `executable_extension` | `dist/tool.exe` |
| `script_extension` | `install.ps1` |
| `shell_script_extension` | `postinstall.sh` |
| `wasm_module_extension` | `dist/panel/app.wasm` |
| `pe_magic_disguised_as_png` | `assets/image.png` starting with `MZ` |
| `elf_magic_disguised` | `assets/data.bin` starting with `\x7fELF` |
| `macho_magic_disguised` | Mach-O 64-bit magic |
| `wasm_magic_disguised` | `\0asm` magic |
| `shebang_script_disguised_as_text` | `assets/readme.txt` starting with `#!` |

### Manifest and capabilities

| Fixture | Change | Expected |
|---|---|---|
| `missing_manifest` | no `glint.json` | `GLINT_PACKAGE_MISSING_MANIFEST` |
| `manifest_only_in_subdirectory` | `theme/glint.json` only | `GLINT_PACKAGE_MISSING_MANIFEST` |
| `manifest_not_utf8` | invalid UTF-8 bytes | `GLINT_PACKAGE_MANIFEST_NOT_UTF8` |
| `manifest_not_json` | `{ not json` | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_INVALID_JSON` |
| `manifest_wrong_schema_version` | `schemaVersion: 99` | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_UNSUPPORTED_SCHEMA_VERSION` |
| `manifest_wrong_kind` | `kind: "plugin"` | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_WRONG_KIND` |
| `manifest_missing_id` | no `id` | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_MISSING_ID` |
| `manifest_duplicate_key` | `name` twice | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_DUPLICATE_KEY` |
| `manifest_remote_entrypoint` | surface entry `https://...` | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_INVALID_PATH` |
| `manifest_traversal_reference` | `tokens: "../tokens.json"` | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_INVALID_PATH` |
| `capability_outside_known_namespace` | `filesystem.write.any` | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_INVALID_CAPABILITY` |
| `unknown_required_capability` | `system.shell.exec` (valid namespace, unknown ID) | `GLINT_PACKAGE_UNKNOWN_CAPABILITY` |
| `unknown_optional_capability` | `system.files.write` | `GLINT_PACKAGE_UNKNOWN_CAPABILITY` |
| `manifest_references_missing_file` | `tokens` points to a missing file | `GLINT_PACKAGE_MISSING_REFERENCED_FILE` |
| `manifest_references_directory` | `tokens: "theme"` (a directory) | `GLINT_PACKAGE_MISSING_REFERENCED_FILE` |
| `manifest_references_missing_asset` | `assets.wallpaper` missing | `GLINT_PACKAGE_MISSING_REFERENCED_FILE` |
| `manifest_reference_differs_in_case` | `theme/Tokens.json` vs `theme/tokens.json` | `GLINT_PACKAGE_MISSING_REFERENCED_FILE` |

### Surface entrypoints and page entries (`references.rs`, PKG-004)

Every token, asset and layout path, then every surface HTML entrypoint, must
be a regular file in the archive (exact, case-sensitive match). A theme
missing a declared surface entrypoint is refused.

| Fixture | Change | Expected |
|---|---|---|
| `missing_surface_entrypoint` | extra `launcher` surface whose entry is not in the archive | `GLINT_PACKAGE_MISSING_REFERENCED_FILE` |
| `surface_entrypoint_differs_in_case` | `dist/panel/Index.html` vs `index.html` | `GLINT_PACKAGE_MISSING_REFERENCED_FILE` |
| `surface_entrypoint_is_a_directory` | entry `dist/shell.html` is a directory | `GLINT_PACKAGE_MISSING_REFERENCED_FILE` |
| `missing_entrypoint_remote_script_and_executables_have_distinct_codes` | missing entry; remote `<script src>`; `bin/tool.exe`; `install.sh` | `MISSING_REFERENCED_FILE`, `REMOTE_REFERENCE`, `EXECUTABLE_CONTENT` (three distinct codes) |
| `file_url_entrypoint` | entry `file:///C:/...` | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_INVALID_PATH` |
| `data_url_entrypoint` | entry `data:text/html,...` | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_INVALID_PATH` |
| `absolute_entrypoint` | entry `/srv/panel/index.html` | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_INVALID_PATH` |
| `install_script_field` | top-level `scripts.postinstall` | `GLINT_PACKAGE_INVALID_MANIFEST` / `GLINT_MANIFEST_UNKNOWN_FIELD` |

### Surface definitions (`references.rs`, PKG-004)

The reader and the packer run `glint_surfaces::surface_definitions` on the
parsed manifest. Failures produce `GLINT_PACKAGE_INVALID_SURFACE`;
`PackageError::surface_error()` carries the code and pointer below.

| Fixture | Change | `GLINT_SURFACE_*` | Pointer |
|---|---|---|---|
| `unknown_surface_kind` | surface `taskbar` | `UNKNOWN_KIND` | `/surfaces/taskbar` |
| `surface_placement_out_of_range` | panel `height: 99999` | `OUT_OF_RANGE` | `/surfaces/panel/placement/height` |
| `surface_placement_unknown_field` | panel `placement.zIndex` | `UNKNOWN_FIELD` | `/surfaces/panel/placement/zIndex` |
| `surface_placement_kind_not_allowed` | panel `placement.kind: "desktop"` | `UNSUPPORTED_PLACEMENT` | `/surfaces/panel/placement/kind` |
| `surface_behavior_conflict` | panel `behavior.clickThrough: true` | `CONFLICT` | `/surfaces/panel/behavior/clickThrough` |
| `surface_check_runs_before_the_entrypoint_existence_check` | `height: 0` and a missing entry | `OUT_OF_RANGE` | |

Accepted: `valid_placement_behavior_and_custom_surface_are_accepted` (a
bottom-edge panel reserving the work area plus a floating
`custom:dev.example.clock`).

### Capability IDs and the surface catalog (`references.rs`, PKG-004)

The accepted set is `known_capabilities()`: the frozen v1
[capability registry](../spec/capabilities-v1.md) owned by `glint-platform`
(CAP-005), which contains every surface catalog capability and
`surface.custom`. Anything else is refused (fail closed).

| Fixture | Change | Expected |
|---|---|---|
| `every_known_capability_is_accepted_by_schema_and_reader` | every known ID as required, then as optional | accepted |
| `surface_catalog_capabilities_are_accepted` | each catalog row's capabilities, and `surface.custom` | accepted |
| `unknown_surface_capability` | `surface.taskbar` | `GLINT_PACKAGE_UNKNOWN_CAPABILITY` |
| `unregistered_plan_alias_capability` | optional `surface.notification-overlay` (plan spelling, not registered) | `GLINT_PACKAGE_UNKNOWN_CAPABILITY` |
| `every_registered_capability_is_known` | `known_capabilities()` against the registry | equal |
| `unknown_provider_capability_next_to_catalog_ones` | `desktop.windows.close` next to `surface.dock` | `GLINT_PACKAGE_UNKNOWN_CAPABILITY` |

### Remote references in UI files

All produce `GLINT_PACKAGE_REMOTE_REFERENCE`. The scan is a heuristic; the
renderer CSP (RND) is the real control.

| Fixture | Change |
|---|---|
| `remote_script_src` | `<script src="https://...">` |
| `protocol_relative_script_src` | `<SCRIPT SRC='//...'>` |
| `remote_stylesheet_link` | `<link rel="stylesheet" href="https://...">` |
| `remote_iframe` | `<iframe src=http://...>` |
| `meta_refresh_redirect` | `<meta http-equiv="refresh" content="0;url=https://...">` |
| `css_remote_import` | CSS `@import url("https://...")` |
| `js_static_remote_import` | `import React from 'https://...'` |
| `js_dynamic_remote_import` | `await import("https://...")` |
| `unc_reference_in_html` | `<img src="\\attacker\share\pixel.png">` |
| `script_src_after_slash` (`references.rs`) | `<script/src="https://...">` |
| `file_url_script_src` (`references.rs`) | `<script src="file:///C:/...">` |
| `remote_stylesheet_after_quoted_attribute` (`references.rs`) | `<link rel="stylesheet"href="https://...">` |
| `anchor_ping_beacon` (`references.rs`) | `<a href="#top" ping="https://...">` |
| `css_remote_image_set` (`references.rs`) | CSS `image-set("https://..." 1x)` |
| `css_remote_import_without_url` (`references.rs`) | CSS `@import "https://..."` |
| `js_remote_websocket` (`references.rs`) | `new WebSocket('wss://...')` |
| `svg_remote_image` (`references.rs`) | SVG `<image href="https://...">` |
| `svg_remote_stylesheet_instruction` (`references.rs`) | `<?xml-stylesheet href="https://..."?>` |

### Active content in SVG images (`references.rs`, PKG-004)

SVG files are images, so they must be inert data. All produce
`GLINT_PACKAGE_ACTIVE_CONTENT` (reader and packer). HTML entrypoints are
sandboxed UI governed by the renderer CSP and are not checked for inline
script.

| Fixture | Change |
|---|---|
| `svg_script` | `<script>` |
| `svg_foreign_object` | `<foreignObject>` |
| `svg_event_handler` | `onload="..."` |
| `svg_javascript_url` | `href="javascript:..."` |
| `svg_entity_declaration` | `<!ENTITY ...>` in the internal DTD |

Accepted: `inert_svg_is_accepted`, and the default theme's `preview.svg`.

### Staging discipline and accepted controls

| Test | Asserts |
|---|---|
| `missing_staging_root_is_refused_without_creating_it` | `GLINT_PACKAGE_STAGING_ROOT_INVALID`; nothing created |
| `staging_root_that_is_a_file_is_refused` | `GLINT_PACKAGE_STAGING_ROOT_INVALID` |
| `symlinked_staging_root_is_refused` (Unix) | `GLINT_PACKAGE_STAGING_ROOT_INVALID` |
| `world_writable_staging_root_is_refused` (Unix) | `GLINT_PACKAGE_STAGING_ROOT_INVALID` |
| `good_package_extracts_read_only_with_matching_hashes` | archive SHA-256, per-file SHA-256 on disk, read-only files, nothing outside the staging root, staging removed on drop |
| `keep_staging_leaves_the_directory_for_the_installer` | `keep_staging` hands the directory over |
| `changing_the_source_after_inspection_does_not_affect_the_candidate` | the private copy is what was validated |
| `each_inspection_gets_its_own_staging_directory` | no staging directory is reused |
| `identification_uses_magic_bytes_not_the_extension` | a ZIP named `.txt` is accepted |
| `package_written_by_zip_crate_writer_is_accepted` | interop with `ZipWriter` output |
| `anchor_links_to_remote_sites_are_allowed` | `<a href="https://...">` is navigation, not a dependency |
| `unicode_normalization_collision_is_only_caught_by_ascii_policy` | NFC/NFD names are refused only by the ASCII default (gap below) |
| `every_error_code_is_unique_and_prefixed` | stable code table |

The packer round-trip suite
([`tests/roundtrip.rs`](../../crates/glint-package/tests/roundtrip.rs))
additionally checks that packed output always passes these rules.

## Known gaps

- **Unicode normalization.** With `ascii_paths_only = false`, NFC and NFD
  spellings of one name are not detected as a collision. The ASCII-only
  default is the verified policy.
- **Handle-relative staging.** Staging creates files component by component
  with `create_new` and re-checks every directory for links, but does not use
  handle-relative opens (`openat`, `cap-std`); see the `staging` module docs.
- **Remote-reference and active-content scans** are heuristics (no `eval`,
  string-built URLs, `fetch(variable)`, HTML character references or CSS
  escapes); the renderer CSP is the control. Gzipped `.svgz` files are not
  scanned.
- **References inside UI files.** Only paths in `glint.json` are checked for
  existence; a local `<script src="./missing.js">` inside an entrypoint is
  not (CRT-003 build output checks).
- **"Untrusted renderer cannot call unauthorized daemon commands"** is out of
  scope for an archive reader (RND/CAP).
