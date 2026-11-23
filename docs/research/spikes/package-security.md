# Spike: package security (SPK-007)

Status: **spike complete; findings feed PKG-001**. The code under
[`spikes/package-security`](../../../spikes/package-security/src/lib.rs) is
throwaway feasibility code. It is never shipped and never imported by product
code. Everything below was measured on the Windows 11 host with Rust 1.95.0.
Linux and macOS were **not run** (CI compile-only), so the Windows-only crate
behaviours listed below are recorded from the source, not from a run on those
hosts.

Source obligations:

- [Spike 4: package security](../../plan/08-research/feasibility-spikes.md#spike-4-package-security)
- [Non-negotiable package security tests](../../plan/06-engineering/testing-and-quality-gates.md#non-negotiable-package-security-tests)
- [Manifest validation rules](../../plan/04-themes/manifest-and-glint-package-spec.md#manifest-validation-rules)
  and [import behavior](../../plan/04-themes/manifest-and-glint-package-spec.md#import-behavior)
- [Installer transaction steps](../../plan/05-distribution/installer-lifecycle-and-updates.md#transaction-steps)
- [Risk register](../../plan/06-engineering/risks-feasibility-and-tradeoffs.md#risk-register)
  ("Package supply chain")

## Outcome

- All 91 hostile fixtures are rejected with a typed error. For each one, the
  test also confirms that nothing was written anywhere in the sandbox (no
  staging directory and no escaped file). The test helper
  `assert_rejected_without_writes` in
  [`tests/hostile_fixtures.rs`](../../../spikes/package-security/tests/hostile_fixtures.rs)
  snapshots the whole temp tree before and after each call.
- Every fixture is generated in memory at test time by a hand-written ZIP
  builder
  ([`tests/support/mod.rs`](../../../spikes/package-security/tests/support/mod.rs))
  and written only to a per-test temp directory. No archive is committed. The
  largest fixture is 8 MiB of zeros, deflated in memory to about 8 KiB.
- The `zip` crate is usable for PKG-001 **only behind our own validator**. Its
  convenience APIs (`enclosed_name`, `mangled_name`, `is_symlink`, `len`,
  `extract`) are not safe on their own. The 27 tests in
  [`tests/zip_crate_behaviour.rs`](../../../spikes/package-security/tests/zip_crate_behaviour.rs)
  pin the evidence.

Verification on this host: `cargo test -p glint-spike-package-security` →
98 hostile/staging tests + 27 crate-behaviour tests passed (125 total).
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`
and `python scripts/check-repository.py` all pass.

## Chosen crate, version and features

| Item | Choice |
|---|---|
| Crate | [`zip`](https://crates.io/crates/zip) (zip-rs/zip2) **`=9.8.1`** (latest on crates.io at spike time) |
| Features | `default-features = false`, `features = ["deflate-flate2-zlib-rs"]` |
| Methods compiled | Stored (0) and Deflate (8) only, using the pure-Rust `zlib-rs` 0.6.8 backend via `flate2` 1.1.11 |
| Excluded | `aes-crypto`, `bzip2`, `deflate64`, `lzma`, `xz`, `ppmd`, `zstd`, `time`, `deflate-zopfli` (the default set pulls all of them) |
| Still compiled regardless | `crc32fast`, `indexmap`, `memchr`, `typed-path` 0.13, `getrandom` 0.4, `zeroize`, `constant_time_eq` (legacy ZipCrypto support is not feature-gated), `windows-sys` |
| Hashing | `sha2` 0.11.1 (streaming SHA-256 per entry and per archive) |
| Manifest parsing | `serde_json` 1.0.154 |
| Test-only | `flate2` (fixture deflate), `crc32fast` (fixture CRCs), `tempfile` 3.27.0 |

Excluded methods are reported as `CompressionMethod::Unsupported(n)` in
metadata, and the validator rejects them before reading
(`disabled_methods_surface_as_unsupported`,
`aes_method_without_aes_feature_is_unsupported`).

**Alternatives.** Only `zip` was exercised. `rc-zip` (sans-I/O) and
`async_zip` were not evaluated hands-on. The validator owns every security
decision and uses the crate only to parse records and inflate data, so
swapping the parser later is cheap. PKG-001 should pin the exact version
(`=9.8.1`) and re-run `zip_crate_behaviour.rs` on every upgrade.

## Recommended `PackageLimits` defaults

| Limit | Default | Rationale |
|---|---|---|
| `max_archive_bytes` | 256 MiB | Checked on file size before parsing. It also bounds central-directory parse cost: each record is at least 46 bytes |
| `max_entries` | 10 000 | Compiled themes have tens to hundreds of files. Checked against the EOCD **before** `ZipArchive` parses records, and again after parsing |
| `max_total_uncompressed` | 512 MiB | Enforced on declared sizes (pass 1) and on actual inflated bytes (pass 2) |
| `max_file_uncompressed` | 128 MiB | Allows an 8K PNG/WebP wallpaper. Video wallpapers are out of scope for v1 |
| `max_ratio` | 100 : 1 | Zeros deflate at about 1000:1. Real text assets compress about 3–20:1. Applies per entry and archive-wide |
| `ratio_min_bytes` | 1 MiB | The ratio check is skipped below this size, so a small, highly compressible JSON never trips it. The archive-wide ratio still catches many small entries |
| `max_depth` | 16 components | `dist/<surface>/assets/...` needs fewer than 6 |
| `max_path_len` | 240 bytes | Relative to the package root. See the MAX_PATH gap below |
| `max_component_len` | 128 bytes | Below the NTFS limit of 255 UTF-16 units |
| `ascii_paths_only` | `true` | Removes every Unicode case-folding and normalization collision class (see gaps) |
| `max_scanned_text_bytes` | 4 MiB | Text kept in memory for manifest parsing and the remote-reference scan |

## Rejection rules

The rules run in this order. Each one maps to a typed `PackageError` variant
in [`src/lib.rs`](../../../spikes/package-security/src/lib.rs).

**Envelope (before parsing):**

1. File larger than `max_archive_bytes` → `ArchiveTooLarge`.
2. First 4 bytes are not `PK\x03\x04` → `NotZip`. This covers non-ZIP data
   with a `.glint` extension, empty files, and polyglots with data prepended,
   such as an EXE stub before the ZIP.
3. The EOCD (end of central directory) record is parsed by hand. ZIP64
   markers or multiple disks → `Malformed`. An entry count above the budget →
   `TooManyEntries`.
4. `ZipArchive::with_config` is called with `ArchiveOffset::Known(0)`, so the
   crate cannot "detect" a shifted archive.

**Archive structure:**

5. Duplicate exact names in the central directory (`has_duplicate_names()`) → `DuplicateName`.
6. Central-directory count above the budget → `TooManyEntries`.
7. Entries that share compressed data (`has_overlapping_files()`) → `OverlappingEntries`.
8. A local header that disagrees with the central directory, or a local
   entry that the central directory omits (`validate_header_consistency()`) →
   `Malformed`.

**Per entry (metadata pass, nothing inflated):**

9. Any extra field outside this allow-list → `ForbiddenExtraField`. Allowed:
   ZIP64 `0x0001`, NTFS `0x000a`, extended timestamp `0x5455`, Info-ZIP Unix
   `0x7875`, and data-stream alignment. Rejected examples: Unicode Path
   `0x7075`, PKWARE Unix `0x000d` (can carry link targets), AES `0x9901`, and
   unknown IDs.
10. Name rules → `UnsafePath { reason }`:
    - empty, too long, or too deep
    - not UTF-8, non-ASCII (default policy), or containing control characters or NUL
    - `\\` or `//` prefix: UNC, `\\.\` device namespace, `\\?\` verbatim
    - leading `/` or `\`
    - any `:` (drive letters, `C:x`, NTFS alternate data streams)
    - any `\`
    - `.`, `..` or empty components
    - a component longer than `max_component_len`
    - `< > " | ? * ~` (`~` blocks 8.3 short-name aliases such as `PROGRA~1`)
    - leading space, trailing `.` or trailing space
    - Windows device names with any extension: `CON`, `PRN`, `AUX`, `NUL`,
      `COM0-9`, `LPT0-9`, `COM¹²³`, `CONIN$`, `CONOUT$`, `CLOCK$`, `CONFIG$`

    The rules are identical on every host.
11. File type taken from the **raw** high 16 bits of the external attributes,
    and also from the crate's `unix_mode()`. Symlink → `Symlink`. Any type
    other than a regular file or directory → `NotRegularFile`.
12. Encrypted flag → `Encrypted`.
13. Method other than Stored or Deflate → `UnsupportedCompression`.
14. Directory entries must declare size 0.
15. File entries:
    - declared size above `max_file_uncompressed` → `EntryTooLarge`
    - Stored entries where the compressed size differs from the declared size → `SizeMismatch`
    - per-entry ratio above the limit → `RatioExceeded`
    - an executable or script extension (`exe`, `dll`, `ps1`, `bat`, `sh`, `so`, `dylib`, ...) → `ExecutableContent`
16. Running declared total above the budget → `TotalTooLarge`. Archive-wide ratio → `RatioExceeded`.
17. Paths that collide after ASCII case folding (or after `to_lowercase`
    when non-ASCII is allowed), and paths that are both a file and a parent
    directory → `PathCollision`.

**Content pass (bounded inflate, still nothing written):**

18. Each entry is read through `take(min(declared, max_file) + 1)`. Producing
    more or fewer bytes than declared → `SizeMismatch` or `Malformed` (the
    crate reports "larger than declared" and CRC failures as `InvalidData`).
19. The actual running total is checked against the budget → `TotalTooLarge`.
20. Content that starts with executable magic (`MZ`, `\x7fELF`, Mach-O,
    fat binary, `#!`) → `ExecutableContent`.

**Manifest and UI content:**

21. `glint.json` must be present at the root (`MissingManifest`) and must parse
    as an object with `schemaVersion: 1` and string `kind`, `id`, `name` and
    `version` fields (`InvalidManifest`).
22. Every entry in `requiredCapabilities` **and** `optionalCapabilities` must
    be a known capability ID → `UnknownCapability`.
23. Every path the manifest references (`tokens`, `surfaces.*.entry`,
    `assets.*`, `layouts.*`) must exist in the archive
    (`MissingReferencedFile`) and must not be remote (`RemoteReference`).
24. HTML, SVG, CSS and JS files are scanned for remote loads → `RemoteReference`:
    - `src`, `href`, `data`, `action`, `poster`, `srcset` and `xlink:href`
      attributes on any tag except `<a>`
    - `<meta http-equiv=refresh>`
    - CSS `url()` and `@import`
    - JS `import ... from`, `import()`, `importScripts`, `new Worker` and `fetch` with literal URLs

    "Remote" means one of these prefixes: `http:`, `https:`, `//`, `\\`,
    `ftp:`, `ws:`, `wss:` or `file:`.

**Staging:**

25. `stage_archive` refuses a staging directory that already exists
    (`StagingExists`) and leaves it untouched. It creates the directory
    itself, writes files only with `create_new` under paths built from
    validated components (never from crate path helpers), re-hashes every
    file as it writes, and removes the directory on any error
    (`StagingHashMismatch` and others).
26. On Windows, the archive is opened with `FILE_SHARE_READ` only, so no other
    process can modify it between validation and staging. The same open handle
    is reused for both passes.

## Observed `zip` 9.8.1 behaviours

| Behaviour | Safe to rely on? | Evidence (test name) |
|---|---|---|
| `enclosed_name()` returns `None` for escapes such as `../x` and `a/../../x`, but silently resolves `a/../b` to `b` | Partly. It hides aliasing | `enclosed_name_rejects_parent_escape_but_resolves_inner_parent` |
| `enclosed_name()` **accepts absolute names** by dropping the root, drive or UNC prefix: `/etc/passwd` → `etc/passwd`, `C:\Windows\x` → `Windows/x`, `\\server\share\x` → `x` | **No.** A validator must reject, not rewrite | `enclosed_name_strips_leading_root_instead_of_rejecting`, `enclosed_name_strips_drive_and_unc_prefixes` |
| Names are parsed with Windows rules on every host, so `a\b` is two components even on Linux | Consistent, but means a POSIX tool and Glint disagree. We reject `\` outright | `enclosed_name_splits_on_backslash_on_every_host` |
| Device names, ADS `:` and trailing dot/space are rejected by `enclosed_name()` **only when compiled for Windows** (`#[cfg(windows)]`) | **No.** The same archive gets different answers on different hosts | `enclosed_name_rejects_windows_device_names_and_ads_on_windows_only`, `enclosed_name_accepts_windows_device_names_off_windows` |
| `enclosed_name()` rejects NUL bytes | Yes | `enclosed_name_rejects_nul_byte` |
| `mangled_name()` rewrites hostile names into extractable ones (`../../x` → `x`, `CON` → `_CON`), and two different names can mangle to the same path | **No.** It hides the attack | `mangled_name_rewrites_hostile_names_into_something_extractable`, `mangled_name_prefixes_device_names_on_windows` |
| Exact duplicate names collapse into one map slot: `len()` counts unique names, the **last** record wins, and `has_duplicate_names()` is the only signal | Only if you check `has_duplicate_names()` | `duplicate_names_are_collapsed_and_only_flagged` |
| Case-fold duplicates (`A.txt` and `a.txt`) are not flagged | No. Must be checked ourselves | `case_fold_duplicates_are_not_flagged` |
| `is_symlink()` and `unix_mode()` use the high 16 bits for Unix and most other hosts, but **ignore them for MS-DOS hosts** (they synthesize a mode `0o100664` instead) | **No.** Read the raw `external_attributes() >> 16` | `symlink_on_unix_host_is_surfaced`, `symlink_bits_on_dos_host_are_ignored_by_the_crate`, `symlink_bits_on_other_hosts_are_surfaced` |
| Extra fields are exposed through `extra_data_fields()`. Unknown ones appear as `ExtraField::Custom { header_id, .. }` | Yes. Enough for an allow-list | `extra_fields_are_exposed_with_header_ids` |
| An Info-ZIP Unicode Path field (`0x7075`) whose CRC matches **replaces `name_raw()`** before callers see it | Dangerous. Tools that ignore `0x7075` extract another name. `validate_header_consistency()` rejects most renames (only case changes, 8.3 aliases and transliterations pass). The allow-list rejects the rest | `unicode_path_extra_field_renames_the_entry`, hostile `unicode_path_extra_field`, `unicode_path_extra_field_case_rename` |
| `size()` and `decompressed_size()` trust the central directory | Only as a pre-check | `size_reports_central_directory_value_without_checking` |
| The reader fails with `InvalidData` once output exceeds the declared size, buffering at most one extra byte | Yes, as one bound. We still apply our own `take()` | `reader_fails_once_output_exceeds_declared_size` |
| Output **shorter** than declared, with a matching CRC, reads successfully | **No.** Compare actual with declared | `reader_accepts_output_shorter_than_declared` |
| CRC mismatch is reported at end of stream | Yes. A partial file is already written by then, so it must be staged | `crc_mismatch_is_detected_at_end_of_stream` |
| The default `ArchiveOffset::Detect` opens archives with data prepended (polyglots). `Known(0)` refuses them | Use `Known(0)` plus a magic check | `prepended_data_is_accepted_by_default_offset_detection` |
| `by_index()` does **not** check that local and central headers match. Hidden local entries are invisible to it | Call `validate_header_consistency()` | `local_header_mismatch_is_not_checked_by_default`, `hidden_local_entries_are_invisible_but_detectable` |
| Overlapping entries (several names sharing one compressed stream) all read successfully | Call `has_overlapping_files()` | `overlapping_entries_are_readable_and_detectable` |
| ZipCrypto entries: `by_index()` fails with "password required" and `by_index_raw()` still works | Yes, with an explicit `encrypted()` check | `zipcrypto_entry_requires_password_and_raw_access_still_works` |
| An unsupported method opens without error in `by_index()` and fails only on the first `read` | Check `compression()` up front | `aes_method_without_aes_feature_is_unsupported`, `disabled_methods_surface_as_unsupported` |
| `ZipArchive::new` materializes **every** central record before any caller-side count check | Pre-check the EOCD and bound the file size | `central_directory_is_fully_parsed_before_any_count_check`, hostile `end_of_central_directory_claims_too_many_entries` |
| `ZipWriter` output, including directory entries, passes the validator. `add_symlink` output is rejected | Interop is fine | `package_written_by_zip_crate_writer_is_accepted`, `symlink_written_by_zip_crate_writer_is_rejected` |

The crate's own `ZipArchive::extract*` helpers were deliberately **not** used.
PKG-001 must not use them either: they follow the `enclosed_name()` rules
above, which depend on the host.

## Fixture coverage

Each row maps one plan obligation to the hostile fixtures in
`tests/hostile_fixtures.rs`. Every fixture runs through `stage_archive`.

| Obligation | Fixtures |
|---|---|
| `../outside` | `traversal_parent_dir`, `traversal_nested_parent_dir`, `traversal_backslash_parent_dir` |
| Absolute POSIX/Windows, UNC and device paths | `absolute_posix_path`, `absolute_backslash_root`, `absolute_windows_drive`, `absolute_windows_drive_forward_slash`, `drive_relative_path`, `drive_prefix_inside_path`, `unc_path_backslash`, `unc_path_forward_slash`, `device_namespace_path`, `verbatim_path`, `device_name_*` (6), `ntfs_alternate_data_stream`, `trailing_dot_component`, `trailing_space_component`, `short_name_alias` |
| Backslash separators and other name syntax | `backslash_separator`, `current_dir_component`, `empty_component`, `nul_byte_in_name`, `control_char_in_name`, `non_utf8_name`, `non_ascii_name_by_default`, `wildcard_char`, `deep_nesting`, `path_too_long`, `component_too_long` |
| Symlinks and ZIP extra attributes | `symlink_unix_mode`, `symlink_bits_on_dos_host`, `symlink_bits_on_ntfs_host`, `character_device_mode`, `unicode_path_extra_field`, `unicode_path_extra_field_case_rename`, `pkware_unix_extra_field`, `unknown_extra_field`, `symlink_written_by_zip_crate_writer_is_rejected` |
| Duplicates and case folding | `exact_duplicate_names`, `case_fold_duplicate`, `case_fold_manifest_shadow`, `case_fold_directory_collision`, `file_and_directory_same_path`, `overlapping_entries_share_data`, `local_and_central_names_differ`, `local_entry_hidden_from_central_directory`, `unicode_case_fold_collision_detected_when_non_ascii_allowed` |
| Compression bombs and oversized assets | `high_compression_ratio`, `archive_wide_ratio_from_small_entries`, `declared_size_understated`, `declared_size_overstated`, `stored_sizes_disagree`, `entry_too_large`, `total_too_large`, `archive_file_too_large`, `excessive_entry_count`, `end_of_central_directory_claims_too_many_entries` |
| Encryption and methods | `zipcrypto_encrypted_entry`, `aes_encrypted_entry`, `aes_method_without_extra_field`, `bzip2_method` |
| Magic bytes and envelope | `not_a_zip_text`, `not_a_zip_png`, `empty_file`, `polyglot_prepended_executable`, `truncated_archive`, `zip64_end_of_central_directory_marker` |
| No remote scripts | `remote_script_src`, `protocol_relative_script_src`, `remote_stylesheet_link`, `remote_iframe`, `meta_refresh_redirect`, `css_remote_import`, `js_static_remote_import`, `js_dynamic_remote_import`, `unc_reference_in_html`, `manifest_remote_entrypoint` |
| Executables are never run (rejected) | `executable_extension`, `script_extension`, `shell_script_extension`, `pe_magic_disguised_as_png`, `elf_magic_disguised` |
| Invalid manifest and unknown capability | `missing_manifest`, `manifest_not_json`, `manifest_wrong_schema_version`, `unknown_required_capability`, `unknown_optional_capability`, `manifest_references_missing_file` |
| Staging discipline and controls | `staging_directory_must_not_exist`, `good_package_stages_with_matching_hashes`, `package_written_by_zip_crate_writer_is_accepted`, `anchor_links_to_remote_sites_are_allowed` |

"Untrusted renderer cannot call unauthorized daemon commands" (spike 4, last
sentence) is out of scope for an archive reader. It belongs to the
renderer/IPC spikes and the CAP/RND epics.

## Gaps and recommendations for PKG-001

PKG-001 builds `crates/glint-package` with
`inspect_archive(path, &PackageLimits) -> Result<PackageCandidate, PackageError>`.

1. **Port the validator, not the crate's helpers.** Keep the two-pass design:
   a metadata pass, then a bounded inflate with SHA-256. Write nothing before
   both passes succeed. Keep the typed error enum and map it to the installer
   taxonomy (`invalid-archive`, `invalid-manifest`, `missing-capability`,
   `filesystem-full`).
2. **Copy into private staging first.** Plan step 2 says to stream the
   download or local file into a Glint-owned staging file while hashing it,
   then validate **that copy**. The spike's `FILE_SHARE_READ` lock works only
   on Windows. Unix has no mandatory lock, so validating the user's original
   file leaves a TOCTOU window.
3. **Use handle-relative creation in staging.** The spike uses `create_dir`
   and `create_new` with joined paths. A local attacker who can write to the
   staging parent could swap in a junction or symlink between the two. Put
   staging in a Glint-owned directory with a restrictive ACL or mode, and use
   handle-relative opens (`openat` / `NtCreateFile` with a root handle, or
   `cap-std`).
4. **MAX_PATH.** `max_path_len = 240` is relative to the package root.
   Staging root plus 240 can exceed 260. Either use long-path-aware APIs
   (`\\?\`) everywhere in `glint-package`, or cap
   `staging_root.len() + max_path_len` and fail with a typed error.
5. **Unicode.** The ASCII-only default is the only policy the spike verified.
   If non-ASCII names are needed, add NFC normalization plus full Unicode case
   folding before collision checks. Without them, NFC and NFD forms such as
   `café.png` collide on macOS (`unicode_normalization_collision_is_only_caught_by_ascii_policy`).
6. **Remote-reference scan is a heuristic.** It cannot see string-built URLs,
   `eval`, `fetch(variable)` or CSS custom properties. The real control is a
   strict renderer CSP (`default-src 'self'`, no remote `connect-src`), which
   is RND epic work. Keep the scan for early, clear import errors.
7. **Not exercised:**
   - streamed entries with data descriptors (flag bit 3)
   - ZIP64 archives (refused outright; no package within the limits needs them)
   - archive comments and trailing data after the EOCD
   - multiple EOCD signatures planted in the comment
   - hardlink encodings beyond PKWARE `0x000d`

   PKG-001 should add fixtures for these, or refuse them explicitly.
8. **Manifest strictness belongs to SCH.** The spike checks the required
   fields, capabilities and referenced files only. Unknown top-level fields,
   permission fields and `compatibility` are not validated. Optional unknown
   capabilities are rejected here. CAP may decide to warn instead, but must
   never silently grant.
9. **Executable policy.** Rejection uses extension plus magic bytes. Decide in
   PKG-001 whether `.wasm`, `.node` (already rejected) and fonts with embedded
   hinting programs need explicit rules. Themes stay data plus sandboxed UI.
10. **Cross-platform runs.** Linux and macOS were not run. The validator's
    rules do not depend on the host, but the crate's `enclosed_name` and
    `mangled_name` behaviour does (see the table above). The
    `#[cfg(not(windows))]` test records the expected difference until CI or
    real hardware runs it.
11. **Crate pinning.** Pin `zip = "=9.8.1"` with the reduced feature set, and
    gate upgrades on `zip_crate_behaviour.rs`. Run `cargo deny` or advisory
    checks against the remaining transitive dependencies.
