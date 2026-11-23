//! Observed behaviour of `zip` 9.8.1 (default-features = false,
//! features = ["deflate-flate2-zlib-rs"]) on hostile input.
//!
//! These tests exercise the crate directly, not the validator, and pin what
//! the research doc claims. If a `zip` upgrade changes a behaviour, the test
//! that fails names the claim that must be re-checked.

mod support;

use std::io::{Cursor, Read};

use support::*;
use zip::read::{ArchiveOffset, Config};
use zip::{ExtraField, ZipArchive};

fn open(bytes: Vec<u8>) -> ZipArchive<Cursor<Vec<u8>>> {
    ZipArchive::new(Cursor::new(bytes)).expect("archive parses")
}

fn single(name: &[u8]) -> ZipArchive<Cursor<Vec<u8>>> {
    open(build_zip(&[RawEntry::stored(name, "x")]))
}

fn enclosed(name: &[u8]) -> Option<std::path::PathBuf> {
    single(name).by_index_raw(0).unwrap().enclosed_name()
}

fn mangled(name: &[u8]) -> std::path::PathBuf {
    single(name)
        .by_index_raw(0)
        .unwrap()
        .mangled_name()
        .unwrap()
}

fn components(path: &std::path::Path) -> Vec<String> {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect()
}

// ---- enclosed_name / mangled_name ----------------------------------------------

#[test]
fn enclosed_name_rejects_parent_escape_but_resolves_inner_parent() {
    assert_eq!(enclosed(b"../escaped.txt"), None);
    assert_eq!(enclosed(b"a/../../escaped.txt"), None);
    // `a/../b` is silently resolved to `b`: two archive names can map to one path.
    assert_eq!(
        enclosed(b"a/../b.txt").map(|p| components(&p)),
        Some(vec!["b.txt".into()])
    );
}

#[test]
fn enclosed_name_strips_leading_root_instead_of_rejecting() {
    // An absolute name is NOT rejected: the root is dropped and the rest kept.
    let path = enclosed(b"/etc/passwd").expect("absolute path is accepted");
    assert_eq!(components(&path), vec!["etc", "passwd"]);
}

#[test]
fn enclosed_name_strips_drive_and_unc_prefixes() {
    let drive = enclosed(b"C:\\Windows\\escaped.txt").expect("drive path is accepted");
    assert_eq!(components(&drive), vec!["Windows", "escaped.txt"]);
    let unc = enclosed(b"\\\\server\\share\\escaped.txt").expect("UNC path is accepted");
    assert_eq!(components(&unc), vec!["escaped.txt"]);
}

#[test]
fn enclosed_name_splits_on_backslash_on_every_host() {
    // Names are parsed with Windows rules everywhere (typed-path), so `a\b`
    // becomes two components even on Linux/macOS.
    let path = enclosed(b"dist\\panel\\x.js").expect("backslash path is accepted");
    assert_eq!(components(&path), vec!["dist", "panel", "x.js"]);
}

#[test]
fn enclosed_name_rejects_nul_byte() {
    assert_eq!(enclosed(b"x.txt\0.png"), None);
}

#[cfg(windows)]
#[test]
fn enclosed_name_rejects_windows_device_names_and_ads_on_windows_only() {
    assert_eq!(enclosed(b"CON"), None);
    assert_eq!(enclosed(b"assets/nul.txt"), None);
    assert_eq!(enclosed(b"glint.json:hidden"), None);
    assert_eq!(enclosed(b"assets/x.txt."), None);
    // Only compiled in on Windows (`#[cfg(windows)]` in zip::path): the same
    // archive gives different answers on Linux/macOS CI.
}

#[cfg(not(windows))]
#[test]
fn enclosed_name_accepts_windows_device_names_off_windows() {
    assert!(enclosed(b"CON").is_some());
    assert!(enclosed(b"assets/nul.txt").is_some());
}

#[test]
fn mangled_name_rewrites_hostile_names_into_something_extractable() {
    assert_eq!(
        components(&mangled(b"../../escaped.txt")),
        vec!["escaped.txt"]
    );
    assert_eq!(components(&mangled(b"/etc/passwd")), vec!["etc", "passwd"]);
    // Two different archive names collapse to the same output: mangled_name is
    // unsuitable for a validator (it hides the attack instead of reporting it).
    assert_eq!(mangled(b"a/../b.txt"), mangled(b"a/b.txt"));
}

#[cfg(windows)]
#[test]
fn mangled_name_prefixes_device_names_on_windows() {
    assert_eq!(components(&mangled(b"CON")), vec!["_CON"]);
}

// ---- Duplicates ----------------------------------------------------------------

#[test]
fn duplicate_names_are_collapsed_and_only_flagged() {
    let mut archive = open(build_zip(&[
        RawEntry::stored("a.txt", "first"),
        RawEntry::stored("a.txt", "second"),
    ]));
    // `len()` counts unique names, so an entry-count limit based on it is
    // blind to duplicates; `has_duplicate_names()` is the only signal.
    assert_eq!(archive.len(), 1);
    assert!(archive.has_duplicate_names());
    let mut content = String::new();
    archive
        .by_name("a.txt")
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    assert_eq!(content, "second", "last central record wins");
}

#[test]
fn case_fold_duplicates_are_not_flagged() {
    let archive = open(build_zip(&[
        RawEntry::stored("A.txt", "a"),
        RawEntry::stored("a.txt", "b"),
    ]));
    assert_eq!(archive.len(), 2);
    assert!(!archive.has_duplicate_names());
}

// ---- Symlinks, unix mode and external attributes --------------------------------

#[test]
fn symlink_on_unix_host_is_surfaced() {
    let mut archive = open(build_zip(&[
        RawEntry::stored("link", "../escaped.txt").unix_mode(S_IFLNK | 0o777)
    ]));
    let entry = archive.by_index_raw(0).unwrap();
    assert!(entry.is_symlink());
    assert!(!entry.is_file());
    assert_eq!(entry.unix_mode(), Some(S_IFLNK | 0o777));
}

#[test]
fn symlink_bits_on_dos_host_are_ignored_by_the_crate() {
    let mut archive = open(build_zip(&[
        RawEntry::stored("link", "../escaped.txt").external((S_IFLNK | 0o777) << 16)
    ]));
    let entry = archive.by_index_raw(0).unwrap();
    // The crate synthesizes a regular-file mode from the DOS byte and ignores
    // the high 16 bits, so `is_symlink()` alone is not a complete check.
    assert!(!entry.is_symlink());
    assert_eq!(entry.unix_mode(), Some(S_IFREG | 0o664));
    assert_eq!(entry.external_attributes() >> 16, S_IFLNK | 0o777);
}

#[test]
fn symlink_bits_on_other_hosts_are_surfaced() {
    let mut archive = open(build_zip(&[RawEntry::stored("link", "../escaped.txt")
        .host(HOST_NTFS)
        .external((S_IFLNK | 0o777) << 16)]));
    assert!(archive.by_index_raw(0).unwrap().is_symlink());
}

#[test]
fn extra_fields_are_exposed_with_header_ids() {
    let mut archive = open(build_zip(&[
        RawEntry::stored("x.txt", "x").extra_field(0xCAFE, b"payload")
    ]));
    let entry = archive.by_index_raw(0).unwrap();
    let ids: Vec<u16> = entry
        .extra_data_fields()
        .filter_map(|field| match field {
            ExtraField::Custom(custom) => Some(custom.header_id),
            _ => None,
        })
        .collect();
    assert_eq!(ids, vec![0xCAFE]);
}

#[test]
fn unicode_path_extra_field_renames_the_entry() {
    let header_name = b"safe.txt";
    let mut payload = vec![1u8];
    payload.extend_from_slice(&crc32fast::hash(header_name).to_le_bytes());
    payload.extend_from_slice(b"../escaped.txt");
    let mut archive = open(build_zip(&[RawEntry::stored(header_name, "x")
        .flags(0)
        .extra_field(0x7075, &payload)]));
    let entry = archive.by_index_raw(0).unwrap();
    // `name_raw()` already returns the extra-field name, not the header bytes:
    // tools that ignore 0x7075 extract a different file name.
    assert_eq!(entry.name_raw(), b"../escaped.txt");
    assert_eq!(entry.enclosed_name(), None);
}

// ---- Declared vs actual sizes --------------------------------------------------

#[test]
fn size_reports_central_directory_value_without_checking() {
    let mut archive = open(build_zip(&[RawEntry::deflated(
        "liar.bin",
        vec![0u8; 1024 * 1024],
    )
    .declared(1000)]));
    assert_eq!(archive.by_index_raw(0).unwrap().size(), 1000);
    assert!(
        archive
            .decompressed_size()
            .is_some_and(|total| total == 1000)
    );
}

#[test]
fn reader_fails_once_output_exceeds_declared_size() {
    let mut archive = open(build_zip(&[RawEntry::deflated(
        "liar.bin",
        vec![0u8; 1024 * 1024],
    )
    .declared(1000)]));
    let mut sink = Vec::new();
    let error = archive
        .by_index(0)
        .unwrap()
        .read_to_end(&mut sink)
        .expect_err("over-long entry must fail");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        error.to_string().contains("larger than its declared"),
        "{error}"
    );
    // read_to_end stops one byte past the declared size.
    assert!(sink.len() <= 1001, "buffered {} bytes", sink.len());
}

#[test]
fn reader_accepts_output_shorter_than_declared() {
    // The size bound is one-sided: a deflate stream that ends early with a
    // matching CRC reads successfully. Callers must compare actual vs declared.
    let mut archive = open(build_zip(&[
        RawEntry::deflated("liar.bin", b"tiny").declared(5000)
    ]));
    let mut sink = Vec::new();
    archive.by_index(0).unwrap().read_to_end(&mut sink).unwrap();
    assert_eq!(sink, b"tiny");
}

#[test]
fn crc_mismatch_is_detected_at_end_of_stream() {
    let mut bytes = build_zip(&[RawEntry::stored("x.txt", "hello world")]);
    // Flip one data byte inside the local entry (header is 30 + 5 bytes).
    bytes[30 + 5] ^= 0xFF;
    let mut archive = open(bytes);
    let mut sink = Vec::new();
    let error = archive
        .by_index(0)
        .unwrap()
        .read_to_end(&mut sink)
        .unwrap_err();
    assert!(error.to_string().contains("checksum"), "{error}");
}

// ---- Archive envelope ----------------------------------------------------------

#[test]
fn prepended_data_is_accepted_by_default_offset_detection() {
    let junk = b"MZ\x90\0fake-pe-stub".to_vec();
    let bytes = [junk.clone(), build_zip(&[RawEntry::stored("x.txt", "x")])].concat();
    let archive = ZipArchive::new(Cursor::new(bytes.clone())).expect("polyglot opens");
    assert_eq!(archive.offset(), junk.len() as u64);
    let config = Config {
        archive_offset: ArchiveOffset::Known(0),
    };
    // With Known(0) the central directory offsets no longer line up and the
    // archive is refused at open time.
    let strict = ZipArchive::with_config(config, Cursor::new(bytes));
    assert!(strict.is_err());
}

#[test]
fn local_header_mismatch_is_not_checked_by_default() {
    let bytes = build_zip(&[RawEntry::stored("safe.txt", "x").local_name("../escaped.txt")]);
    let mut archive = open(bytes);
    let mut content = String::new();
    archive
        .by_index(0)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    assert_eq!(content, "x", "default by_index ignores the local name");
    assert!(archive.validate_header_consistency().is_err());
}

#[test]
fn hidden_local_entries_are_invisible_but_detectable() {
    let bytes = build_zip(&[
        RawEntry::stored("../escaped.txt", "x").hidden_from_central(),
        RawEntry::stored("ok.txt", "y"),
    ]);
    let mut archive = open(bytes);
    assert_eq!(archive.len(), 1);
    assert!(archive.validate_header_consistency().is_err());
}

#[test]
fn overlapping_entries_are_readable_and_detectable() {
    let bytes = build_zip(&[
        RawEntry::deflated("one.bin", vec![b'a'; 4096]),
        RawEntry::stored("two.bin", "").reuse_local_of(0),
    ]);
    let mut archive = open(bytes);
    assert!(archive.has_overlapping_files().unwrap());
    let mut sink = Vec::new();
    archive.by_index(1).unwrap().read_to_end(&mut sink).unwrap();
    assert_eq!(sink.len(), 4096, "second name decompresses the shared data");
}

// ---- Encryption and compression methods --------------------------------------

#[test]
fn zipcrypto_entry_requires_password_and_raw_access_still_works() {
    let mut archive = open(build_zip(&[
        RawEntry::stored("secret.bin", [0u8; 24]).flags(FLAG_UTF8 | FLAG_ENCRYPTED)
    ]));
    assert!(archive.by_index_raw(0).unwrap().encrypted());
    let error = archive.by_index(0).map(|_| ()).unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("password"),
        "{error}"
    );
}

#[test]
fn aes_method_without_aes_feature_is_unsupported() {
    let mut archive = open(build_zip(&[
        RawEntry::stored("secret.bin", [0u8; 32]).method(99)
    ]));
    let compression = archive.by_index_raw(0).unwrap().compression();
    assert_eq!(format!("{compression:?}"), "Unsupported(99)");
    assert_unsupported_fails_only_on_read(&mut archive);
}

#[test]
fn disabled_methods_surface_as_unsupported() {
    let mut archive = open(build_zip(
        &[RawEntry::stored("x.bin", [0u8; 32]).method(12)],
    ));
    let compression = archive.by_index_raw(0).unwrap().compression();
    assert_eq!(format!("{compression:?}"), "Unsupported(12)");
    assert_unsupported_fails_only_on_read(&mut archive);
}

/// `by_index` succeeds for an unsupported method; the error is deferred to the
/// first `read`. Validators must check `compression()` up front.
fn assert_unsupported_fails_only_on_read(archive: &mut ZipArchive<Cursor<Vec<u8>>>) {
    let mut entry = archive
        .by_index(0)
        .expect("by_index does not check the method");
    let mut sink = Vec::new();
    let error = entry.read_to_end(&mut sink).unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("not supported")
            || error.to_string().to_lowercase().contains("unsupported"),
        "{error}"
    );
}

#[test]
fn central_directory_is_fully_parsed_before_any_count_check() {
    // ZipArchive::new materializes every central record; an entry-count limit
    // can only be applied afterwards, so the archive byte budget is what bounds
    // the parse cost (each record costs >= 46 bytes of input).
    let entries: Vec<RawEntry> = (0..20_000)
        .map(|i| RawEntry::stored(format!("{i}"), ""))
        .collect();
    let archive = open(build_zip(&entries));
    assert_eq!(archive.len(), 20_000);
}
