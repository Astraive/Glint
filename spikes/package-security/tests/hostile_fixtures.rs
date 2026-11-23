//! Hostile fixture sweep (SPK-007). Every fixture is generated at test time.
//!
//! Each case calls `stage_archive` and asserts (1) the expected typed error and
//! (2) that the sandbox tree is byte-for-byte unchanged: no staging directory,
//! no escaped file, nothing written anywhere.

mod support;

use glint_spike_package_security::{
    PackageError as E, PackageLimits, PathRejection as P, inspect_archive, stage_archive,
};
use sha2::{Digest, Sha256};
use support::*;

const MIB: usize = 1024 * 1024;

fn assert_rejected_without_writes(bytes: &[u8], limits: &PackageLimits) -> E {
    let sandbox = Sandbox::new(bytes);
    let before = sandbox.snapshot();
    let error = stage_archive(&sandbox.archive, &sandbox.staging, limits)
        .expect_err("hostile fixture must be rejected");
    assert_eq!(
        sandbox.snapshot(),
        before,
        "rejection must not write anything (error: {error:?})"
    );
    assert!(!sandbox.staging.exists());
    assert!(!sandbox.dir.path().join("out").join("escaped.txt").exists());
    error
}

macro_rules! rejects {
    ($name:ident, $bytes:expr, limits = $limits:expr, $pattern:pat) => {
        #[test]
        fn $name() {
            let bytes: Vec<u8> = $bytes;
            let error = assert_rejected_without_writes(&bytes, &$limits);
            assert!(matches!(error, $pattern), "unexpected error: {error:?}");
        }
    };
    ($name:ident, $bytes:expr, $pattern:pat) => {
        rejects!($name, $bytes, limits = PackageLimits::default(), $pattern);
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

/// Deterministic incompressible bytes (xorshift), so size limits are hit
/// without tripping the ratio check first.
fn noise(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

// ---- Path traversal, absolute, UNC and device paths -------------------------

rejects!(
    traversal_parent_dir,
    path(b"../escaped.txt"),
    E::UnsafePath {
        reason: P::ParentDir,
        ..
    }
);
rejects!(
    traversal_nested_parent_dir,
    path(b"dist/../../escaped.txt"),
    E::UnsafePath {
        reason: P::ParentDir,
        ..
    }
);
rejects!(
    traversal_backslash_parent_dir,
    path(b"..\\escaped.txt"),
    E::UnsafePath {
        reason: P::Backslash,
        ..
    }
);
rejects!(
    absolute_posix_path,
    path(b"/tmp/escaped.txt"),
    E::UnsafePath {
        reason: P::Absolute,
        ..
    }
);
rejects!(
    absolute_backslash_root,
    path(b"\\Windows\\escaped.txt"),
    E::UnsafePath {
        reason: P::Absolute,
        ..
    }
);
rejects!(
    absolute_windows_drive,
    path(b"C:\\Windows\\escaped.txt"),
    E::UnsafePath {
        reason: P::DriveOrColon,
        ..
    }
);
rejects!(
    absolute_windows_drive_forward_slash,
    path(b"C:/Windows/escaped.txt"),
    E::UnsafePath {
        reason: P::DriveOrColon,
        ..
    }
);
rejects!(
    drive_relative_path,
    path(b"C:escaped.txt"),
    E::UnsafePath {
        reason: P::DriveOrColon,
        ..
    }
);
rejects!(
    drive_prefix_inside_path,
    path(b"dist/c:escaped.txt"),
    E::UnsafePath {
        reason: P::DriveOrColon,
        ..
    }
);
rejects!(
    ntfs_alternate_data_stream,
    path(b"glint.json:hidden"),
    E::UnsafePath {
        reason: P::DriveOrColon,
        ..
    }
);
rejects!(
    unc_path_backslash,
    path(b"\\\\server\\share\\escaped.txt"),
    E::UnsafePath {
        reason: P::UncOrDevicePath,
        ..
    }
);
rejects!(
    unc_path_forward_slash,
    path(b"//server/share/escaped.txt"),
    E::UnsafePath {
        reason: P::UncOrDevicePath,
        ..
    }
);
rejects!(
    device_namespace_path,
    path(b"\\\\.\\PhysicalDrive0"),
    E::UnsafePath {
        reason: P::UncOrDevicePath,
        ..
    }
);
rejects!(
    verbatim_path,
    path(b"\\\\?\\C:\\escaped.txt"),
    E::UnsafePath {
        reason: P::UncOrDevicePath,
        ..
    }
);
rejects!(
    backslash_separator,
    path(b"dist\\panel\\x.js"),
    E::UnsafePath {
        reason: P::Backslash,
        ..
    }
);
rejects!(
    current_dir_component,
    path(b"./x.txt"),
    E::UnsafePath {
        reason: P::CurDir,
        ..
    }
);
rejects!(
    empty_component,
    path(b"dist//x.txt"),
    E::UnsafePath {
        reason: P::EmptyComponent,
        ..
    }
);
rejects!(
    nul_byte_in_name,
    path(b"x.txt\0.png"),
    E::UnsafePath {
        reason: P::ControlChar,
        ..
    }
);
rejects!(
    control_char_in_name,
    path(b"x\x07.txt"),
    E::UnsafePath {
        reason: P::ControlChar,
        ..
    }
);
rejects!(
    non_utf8_name,
    path(&[b'a', 0x81, 0xFE, b'.', b't']),
    E::UnsafePath {
        reason: P::NotUtf8,
        ..
    }
);
rejects!(
    non_ascii_name_by_default,
    path("caf\u{e9}.png".as_bytes()),
    E::UnsafePath {
        reason: P::NonAscii,
        ..
    }
);

// ---- Windows device names and namespace quirks ------------------------------

rejects!(
    device_name_con,
    path(b"CON"),
    E::UnsafePath {
        reason: P::ReservedDeviceName,
        ..
    }
);
rejects!(
    device_name_nul_with_extension,
    path(b"assets/nul.txt"),
    E::UnsafePath {
        reason: P::ReservedDeviceName,
        ..
    }
);
rejects!(
    device_name_com1_json,
    path(b"COM1.json"),
    E::UnsafePath {
        reason: P::ReservedDeviceName,
        ..
    }
);
rejects!(
    device_name_lpt9_dir,
    path(b"lpt9/x.txt"),
    E::UnsafePath {
        reason: P::ReservedDeviceName,
        ..
    }
);
rejects!(
    device_name_conin,
    path(b"CONIN$"),
    E::UnsafePath {
        reason: P::ReservedDeviceName,
        ..
    }
);
rejects!(
    device_name_superscript_com,
    path("COM\u{b9}.txt".as_bytes()),
    limits = non_ascii_limits(),
    E::UnsafePath {
        reason: P::ReservedDeviceName,
        ..
    }
);
rejects!(
    trailing_dot_component,
    path(b"assets/x.txt."),
    E::UnsafePath {
        reason: P::TrailingDotOrSpace,
        ..
    }
);
rejects!(
    trailing_space_component,
    path(b"assets /x.txt"),
    E::UnsafePath {
        reason: P::TrailingDotOrSpace,
        ..
    }
);
rejects!(
    short_name_alias,
    path(b"PROGRA~1/x.txt"),
    E::UnsafePath {
        reason: P::ForbiddenChar,
        ..
    }
);
rejects!(
    wildcard_char,
    path(b"assets/*.txt"),
    E::UnsafePath {
        reason: P::ForbiddenChar,
        ..
    }
);

// ---- Depth and length --------------------------------------------------------

rejects!(
    deep_nesting,
    path("d/".repeat(16).add_str("x.txt").as_bytes()),
    E::UnsafePath {
        reason: P::TooDeep,
        ..
    }
);
rejects!(
    path_too_long,
    path("a".repeat(241).as_bytes()),
    E::UnsafePath {
        reason: P::TooLong,
        ..
    }
);
rejects!(
    component_too_long,
    path(format!("dist/{}", "a".repeat(129)).as_bytes()),
    E::UnsafePath {
        reason: P::ComponentTooLong,
        ..
    }
);

trait AddStr {
    fn add_str(self, s: &str) -> String;
}
impl AddStr for String {
    fn add_str(mut self, s: &str) -> String {
        self.push_str(s);
        self
    }
}

// ---- Symlinks, special files and extra fields --------------------------------

rejects!(
    symlink_unix_mode,
    good_plus(vec![
        RawEntry::stored("dist/link", "../../escaped.txt").unix_mode(S_IFLNK | 0o777)
    ]),
    E::Symlink { .. }
);
rejects!(
    // The zip crate ignores the high 16 bits for MS-DOS hosts (is_symlink() is
    // false); the validator checks the raw bits regardless of host.
    symlink_bits_on_dos_host,
    good_plus(vec![
        RawEntry::stored("dist/link", "../../escaped.txt").external((S_IFLNK | 0o777) << 16)
    ]),
    E::Symlink { .. }
);
rejects!(
    symlink_bits_on_ntfs_host,
    good_plus(vec![
        RawEntry::stored("dist/link", "../../escaped.txt")
            .host(HOST_NTFS)
            .external((S_IFLNK | 0o777) << 16)
    ]),
    E::Symlink { .. }
);
rejects!(
    character_device_mode,
    good_plus(vec![
        RawEntry::stored("dist/dev", "").unix_mode(S_IFCHR | 0o666)
    ]),
    E::NotRegularFile { .. }
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
    E::Malformed(_)
);
rejects!(
    unicode_path_extra_field_case_rename,
    {
        // A case-only rename without the UTF-8 flag passes the crate's header
        // consistency rules (which tolerate case, 8.3 aliases and
        // transliteration), so only the extra-field allow-list catches it.
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
    E::ForbiddenExtraField {
        header_id: 0x7075,
        ..
    }
);
rejects!(
    pkware_unix_extra_field,
    // 0x000d carries atime/mtime/uid/gid then a link target.
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
    E::ForbiddenExtraField {
        header_id: 0x000d,
        ..
    }
);
rejects!(
    unknown_extra_field,
    good_plus(vec![
        RawEntry::stored("dist/x.txt", "x").extra_field(0xCAFE, b"payload")
    ]),
    E::ForbiddenExtraField {
        header_id: 0xCAFE,
        ..
    }
);

// ---- Duplicates and collisions -------------------------------------------------

rejects!(
    exact_duplicate_names,
    good_plus(vec![RawEntry::stored(
        "theme/tokens.json",
        r#"{"color":"red"}"#
    )]),
    E::DuplicateName
);
rejects!(
    case_fold_duplicate,
    good_plus(vec![
        RawEntry::stored("assets/A.txt", "a"),
        RawEntry::stored("assets/a.txt", "b")
    ]),
    E::PathCollision { .. }
);
rejects!(
    case_fold_manifest_shadow,
    good_plus(vec![RawEntry::stored("GLINT.JSON", "{}")]),
    E::PathCollision { .. }
);
rejects!(
    case_fold_directory_collision,
    good_plus(vec![RawEntry::stored("Dist/panel/index.html", "x")]),
    E::PathCollision { .. }
);
rejects!(
    file_and_directory_same_path,
    good_plus(vec![
        RawEntry::stored("assets", "file"),
        RawEntry::stored("assets/x.txt", "x")
    ]),
    E::PathCollision { .. }
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
    E::OverlappingEntries
);
rejects!(
    local_and_central_names_differ,
    good_plus(vec![
        RawEntry::stored("assets/safe.txt", "x").local_name("../escaped.txt")
    ]),
    E::Malformed(_)
);
rejects!(
    local_entry_hidden_from_central_directory,
    good_plus(vec![
        RawEntry::stored("../escaped.txt", "x").hidden_from_central()
    ]),
    E::Malformed(_)
);

// ---- Compression bombs and size budgets ---------------------------------------

rejects!(
    high_compression_ratio,
    // 8 MiB of zeros deflates to ~8 KiB (ratio ~1000). Generated, never committed.
    good_plus(vec![RawEntry::deflated(
        "assets/zeros.bin",
        vec![0u8; 8 * MIB]
    )]),
    E::RatioExceeded { .. }
);
rejects!(
    archive_wide_ratio_from_small_entries,
    // Each entry is below `ratio_min_bytes`; the archive-wide ratio still trips.
    good_plus(
        (0..8)
            .map(|i| RawEntry::deflated(format!("assets/z{i}.bin"), vec![0u8; MIB / 2]))
            .collect()
    ),
    E::RatioExceeded { .. }
);
rejects!(
    declared_size_understated,
    // Declares 1000 bytes (dodging the ratio check) but inflates to 4 MiB.
    good_plus(vec![
        RawEntry::deflated("assets/liar.bin", vec![0u8; 4 * MIB]).declared(1000)
    ]),
    E::Malformed(_) | E::SizeMismatch { .. }
);
rejects!(
    declared_size_overstated,
    good_plus(vec![
        RawEntry::deflated("assets/liar.bin", b"tiny").declared(5000)
    ]),
    E::Malformed(_) | E::SizeMismatch { .. }
);
rejects!(
    stored_sizes_disagree,
    good_plus(vec![
        RawEntry::stored("assets/liar.bin", "0123456789").declared(4)
    ]),
    E::SizeMismatch { .. }
);
rejects!(
    entry_too_large,
    good_plus(vec![RawEntry::stored("assets/big.bin", noise(2 * MIB))]),
    limits = PackageLimits {
        max_file_uncompressed: MIB as u64,
        ..PackageLimits::default()
    },
    E::EntryTooLarge { .. }
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
    E::TotalTooLarge { .. }
);
rejects!(
    archive_file_too_large,
    good_plus(vec![RawEntry::stored("assets/big.bin", noise(MIB))]),
    limits = PackageLimits {
        max_archive_bytes: 512 * 1024,
        ..PackageLimits::default()
    },
    E::ArchiveTooLarge { .. }
);
rejects!(
    excessive_entry_count,
    good_plus(
        (0..10_001)
            .map(|i| RawEntry::stored(format!("assets/{i}.txt"), ""))
            .collect()
    ),
    E::TooManyEntries { .. }
);

// ---- Encryption and compression methods --------------------------------------

rejects!(
    zipcrypto_encrypted_entry,
    good_plus(vec![
        RawEntry::stored("assets/secret.bin", [0u8; 24]).flags(FLAG_UTF8 | FLAG_ENCRYPTED)
    ]),
    E::Encrypted { .. }
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
    E::ForbiddenExtraField {
        header_id: 0x9901,
        ..
    } | E::Encrypted { .. }
);
rejects!(
    aes_method_without_extra_field,
    good_plus(vec![
        RawEntry::stored("assets/secret.bin", [0u8; 32]).method(99)
    ]),
    E::UnsupportedCompression { .. }
);
rejects!(
    bzip2_method,
    good_plus(vec![RawEntry::stored("assets/x.bin", [0u8; 32]).method(12)]),
    E::UnsupportedCompression { .. }
);

// ---- Envelope: magic bytes ---------------------------------------------------

rejects!(
    not_a_zip_text,
    b"this is not a zip archive at all".to_vec(),
    E::NotZip
);
rejects!(
    not_a_zip_png,
    b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec(),
    E::NotZip
);
rejects!(empty_file, Vec::<u8>::new(), E::NotZip);
rejects!(
    polyglot_prepended_executable,
    // The zip crate's default ArchiveOffset::Detect would open this.
    [b"MZ\x90\0fake-pe-stub".to_vec(), build_zip(&good_entries())].concat(),
    E::NotZip
);
rejects!(
    truncated_archive,
    build_zip(&good_entries())[..60].to_vec(),
    E::Malformed(_)
);
rejects!(
    zip64_end_of_central_directory_marker,
    {
        // EOCD total-entries fields set to 0xFFFF ("see ZIP64 record").
        let mut bytes = build_zip(&good_entries());
        let eocd = bytes.len() - 22;
        bytes[eocd + 8..eocd + 12].copy_from_slice(&[0xFF; 4]);
        bytes
    },
    E::Malformed(_)
);
rejects!(
    end_of_central_directory_claims_too_many_entries,
    {
        // The EOCD count is checked before the crate parses the directory.
        let mut bytes = build_zip(&good_entries());
        let eocd = bytes.len() - 22;
        bytes[eocd + 8..eocd + 12].copy_from_slice(&[0x50, 0xC3, 0x50, 0xC3]); // 50_000
        bytes
    },
    E::TooManyEntries { count: 50_000, .. }
);

// ---- Executables and scripts --------------------------------------------------

rejects!(
    executable_extension,
    good_plus(vec![RawEntry::stored("dist/tool.exe", "x")]),
    E::ExecutableContent { .. }
);
rejects!(
    script_extension,
    good_plus(vec![RawEntry::stored("install.ps1", "Remove-Item")]),
    E::ExecutableContent { .. }
);
rejects!(
    shell_script_extension,
    good_plus(vec![RawEntry::stored("postinstall.sh", "rm -rf")]),
    E::ExecutableContent { .. }
);
rejects!(
    pe_magic_disguised_as_png,
    good_plus(vec![RawEntry::stored("assets/image.png", b"MZ\x90\0\x03")]),
    E::ExecutableContent { .. }
);
rejects!(
    elf_magic_disguised,
    good_plus(vec![RawEntry::stored(
        "assets/data.bin",
        b"\x7fELF\x02\x01"
    )]),
    E::ExecutableContent { .. }
);

// ---- Manifest and capabilities ------------------------------------------------

rejects!(
    missing_manifest,
    build_zip(&good_entries()[1..]),
    E::MissingManifest
);
rejects!(
    manifest_not_json,
    with_manifest("{ not json"),
    E::InvalidManifest(_)
);
rejects!(
    manifest_wrong_schema_version,
    with_manifest(&GOOD_MANIFEST.replace("\"schemaVersion\": 1", "\"schemaVersion\": 99")),
    E::InvalidManifest(_)
);
rejects!(
    unknown_required_capability,
    with_manifest(&GOOD_MANIFEST.replace("\"surface.panel\"", "\"system.shell.exec\"")),
    E::UnknownCapability(_)
);
rejects!(
    unknown_optional_capability,
    with_manifest(&GOOD_MANIFEST.replace("\"system.audio.read\"", "\"filesystem.write.any\"")),
    E::UnknownCapability(_)
);
rejects!(
    manifest_references_missing_file,
    with_manifest(&GOOD_MANIFEST.replace("theme/tokens.json", "theme/missing.json")),
    E::MissingReferencedFile(_)
);
rejects!(
    manifest_remote_entrypoint,
    with_manifest(
        &GOOD_MANIFEST.replace("dist/panel/index.html", "https://evil.example/panel.html")
    ),
    E::RemoteReference { .. }
);

// ---- Remote references in UI entrypoints --------------------------------------

rejects!(
    remote_script_src,
    with_file(
        "dist/panel/index.html",
        r#"<html><head><script src="https://cdn.example.com/lib.js"></script></head></html>"#
    ),
    E::RemoteReference { .. }
);
rejects!(
    protocol_relative_script_src,
    with_file(
        "dist/panel/index.html",
        r#"<html><SCRIPT SRC='//cdn.example.com/lib.js'></SCRIPT></html>"#
    ),
    E::RemoteReference { .. }
);
rejects!(
    remote_stylesheet_link,
    with_file(
        "dist/panel/index.html",
        r#"<html><link rel="stylesheet" href="https://fonts.example.com/x.css"></html>"#
    ),
    E::RemoteReference { .. }
);
rejects!(
    remote_iframe,
    with_file(
        "dist/panel/index.html",
        r#"<html><iframe src=http://tracker.example/></iframe></html>"#
    ),
    E::RemoteReference { .. }
);
rejects!(
    meta_refresh_redirect,
    with_file(
        "dist/panel/index.html",
        r#"<html><meta http-equiv="refresh" content="0;url=https://evil.example"></html>"#
    ),
    E::RemoteReference { .. }
);
rejects!(
    css_remote_import,
    good_plus(vec![RawEntry::stored(
        "dist/panel/style.css",
        "@import url(\"https://fonts.example.com/f.css\");"
    )]),
    E::RemoteReference { .. }
);
rejects!(
    js_static_remote_import,
    with_file(
        "dist/panel/main.js",
        "import React from 'https://esm.sh/react@19';"
    ),
    E::RemoteReference { .. }
);
rejects!(
    js_dynamic_remote_import,
    with_file(
        "dist/panel/main.js",
        "const m = await import(\"https://evil.example/m.js\");"
    ),
    E::RemoteReference { .. }
);
rejects!(
    unc_reference_in_html,
    with_file(
        "dist/panel/index.html",
        r#"<html><img src="\\attacker\share\pixel.png"></html>"#
    ),
    E::RemoteReference { .. }
);

// ---- Staging discipline and the honest baseline -------------------------------

#[test]
fn staging_directory_must_not_exist() {
    let sandbox = Sandbox::new(&build_zip(&good_entries()));
    std::fs::create_dir(&sandbox.staging).unwrap();
    std::fs::write(sandbox.staging.join("planted.txt"), "x").unwrap();
    let error = stage_archive(
        &sandbox.archive,
        &sandbox.staging,
        &PackageLimits::default(),
    )
    .expect_err("existing staging directory must be refused");
    assert!(matches!(error, E::StagingExists(_)), "{error:?}");
    // The pre-existing directory is left untouched (never cleaned or reused).
    assert_eq!(
        std::fs::read_to_string(sandbox.staging.join("planted.txt")).unwrap(),
        "x"
    );
}

#[test]
fn good_package_stages_with_matching_hashes() {
    let bytes = build_zip(&good_entries());
    let sandbox = Sandbox::new(&bytes);
    let staged = stage_archive(
        &sandbox.archive,
        &sandbox.staging,
        &PackageLimits::default(),
    )
    .expect("baseline package must be accepted");
    assert_eq!(
        staged.candidate.archive_sha256,
        <[u8; 32]>::from(Sha256::digest(&bytes))
    );
    assert_eq!(staged.candidate.files.len(), 5);
    assert_eq!(staged.candidate.directories, vec!["theme".to_owned()]);
    for file in &staged.candidate.files {
        let on_disk = std::fs::read(staged.root.join(&file.path)).unwrap();
        assert_eq!(on_disk.len() as u64, file.size);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&on_disk)),
            file.sha256,
            "{}",
            file.path
        );
    }
    let mut expected = vec![
        "input",
        "input/fixture.glint",
        "out",
        "out/stage",
        "out/stage/dist",
        "out/stage/dist/panel",
        "out/stage/dist/panel/dep.js",
        "out/stage/dist/panel/index.html",
        "out/stage/dist/panel/main.js",
        "out/stage/glint.json",
        "out/stage/theme",
        "out/stage/theme/tokens.json",
    ];
    expected.sort_unstable();
    assert_eq!(sandbox.snapshot(), expected);
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
    let candidate = inspect_archive(&sandbox.archive, &PackageLimits::default())
        .expect("crate-written package must be accepted");
    assert_eq!(candidate.files.len(), 4);
}

#[test]
fn symlink_written_by_zip_crate_writer_is_rejected() {
    use zip::write::SimpleFileOptions;
    let mut entries_zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    entries_zip
        .add_symlink(
            "dist/link",
            "../../escaped.txt",
            SimpleFileOptions::default(),
        )
        .unwrap();
    let symlink_zip = entries_zip.finish().unwrap().into_inner();
    let error = assert_rejected_without_writes(&symlink_zip, &PackageLimits::default());
    assert!(matches!(error, E::Symlink { .. }), "{error:?}");
}

#[test]
fn anchor_links_to_remote_sites_are_allowed() {
    // GOOD_HTML contains <a href="https://example.org">; navigation is left to
    // the renderer's navigation policy, not treated as a remote dependency.
    assert!(GOOD_HTML.contains("<a href=\"https://"));
    let sandbox = Sandbox::new(&build_zip(&good_entries()));
    inspect_archive(&sandbox.archive, &PackageLimits::default()).expect("baseline accepted");
}

#[test]
fn unicode_normalization_collision_is_only_caught_by_ascii_policy() {
    // NFC "café.png" vs NFD "cafe\u{301}.png": distinct bytes, same file on
    // macOS (APFS/HFS+ normalization-insensitive). Default policy rejects any
    // non-ASCII name; with it disabled the spike does NOT detect the collision
    // (no Unicode normalization dependency). Documented gap for PKG-001.
    let bytes = good_plus(vec![
        RawEntry::stored("assets/caf\u{e9}.png".as_bytes(), "nfc"),
        RawEntry::stored("assets/cafe\u{301}.png".as_bytes(), "nfd"),
    ]);
    let sandbox = Sandbox::new(&bytes);
    let strict = inspect_archive(&sandbox.archive, &PackageLimits::default());
    assert!(matches!(
        strict,
        Err(E::UnsafePath {
            reason: P::NonAscii,
            ..
        })
    ));
    let relaxed = inspect_archive(&sandbox.archive, &non_ascii_limits());
    assert!(
        relaxed.is_ok(),
        "gap: NFC/NFD collision not detected without ASCII policy"
    );
}

#[test]
fn unicode_case_fold_collision_detected_when_non_ascii_allowed() {
    let bytes = good_plus(vec![
        RawEntry::stored("assets/\u{c9}t\u{e9}.png".as_bytes(), "upper"),
        RawEntry::stored("assets/\u{e9}t\u{e9}.png".as_bytes(), "lower"),
    ]);
    let error = assert_rejected_without_writes(&bytes, &non_ascii_limits());
    assert!(matches!(error, E::PathCollision { .. }), "{error:?}");
}
