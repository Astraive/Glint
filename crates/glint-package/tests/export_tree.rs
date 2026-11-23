//! Canonical re-packing of extracted package trees (PKG-005):
//! `export_package_tree` keeps every file the reader accepted (no authoring
//! exclusions), strips archive metadata and is a fixed point.
//!
//! Run with `cargo test -p glint-package --test export_tree`.

mod support;

use std::fs;

use glint_package::{
    ErrorCode, PackageLimits, export_package_tree, inspect_archive_in, pack_directory,
};
use support::{RawEntry, Sandbox, good_plus};

fn names(candidate: &glint_package::PackageCandidate) -> Vec<String> {
    let mut names: Vec<String> = candidate.files.iter().map(|f| f.path.clone()).collect();
    names.sort();
    names
}

#[test]
fn keeps_every_package_file_and_strips_archive_metadata() {
    // A foreign archive: stored and deflated entries, a directory entry, a
    // DOS host and files the authoring packer would skip.
    let bytes = good_plus(vec![
        RawEntry::deflated(".well-known/notes.txt", "kept"),
        RawEntry::stored("package.json", "{}").host(support::HOST_DOS),
    ]);
    let sandbox = Sandbox::new(&bytes);
    let limits = PackageLimits::default();
    let original = inspect_archive_in(&sandbox.archive, &limits, &sandbox.staging_root).unwrap();
    let root = original.staging_dir().join("root");

    let out = sandbox.dir.path().join("out");
    fs::create_dir(&out).unwrap();
    let exported = export_package_tree(&root, &out.join("a.glint"), &limits).unwrap();
    assert_eq!(exported.entries, original.files.len());
    assert!(exported.excluded.is_empty());
    assert_ne!(
        exported.sha256, original.sha256,
        "foreign bytes are canonicalized"
    );

    let check = sandbox.dir.path().join("check");
    fs::create_dir(&check).unwrap();
    let reread = inspect_archive_in(&out.join("a.glint"), &limits, &check).unwrap();
    assert_eq!(names(&reread), names(&original));
    assert!(names(&reread).contains(&".well-known/notes.txt".to_owned()));
    assert!(names(&reread).contains(&"package.json".to_owned()));
    let content = |c: &glint_package::PackageCandidate| {
        let mut files: Vec<(String, [u8; 32])> =
            c.files.iter().map(|f| (f.path.clone(), f.sha256)).collect();
        files.sort();
        files
    };
    assert_eq!(content(&reread), content(&original));

    // The authoring packer would have dropped them.
    let authored = pack_directory(&root, &out.join("authored.glint")).unwrap();
    assert_eq!(
        authored.excluded,
        [".well-known".to_owned(), "package.json".to_owned()]
    );

    // Fixed point: exporting the re-read tree gives the same bytes.
    let again = export_package_tree(
        &reread.staging_dir().join("root"),
        &out.join("b.glint"),
        &limits,
    )
    .unwrap();
    assert_eq!(again.sha256, exported.sha256);
    assert_eq!(
        fs::read(out.join("a.glint")).unwrap(),
        fs::read(out.join("b.glint")).unwrap()
    );
}

#[test]
fn refuses_a_destination_inside_the_tree() {
    let sandbox = Sandbox::new(&good_plus(Vec::new()));
    let limits = PackageLimits::default();
    let original = inspect_archive_in(&sandbox.archive, &limits, &sandbox.staging_root).unwrap();
    let root = original.staging_dir().join("root");
    let error = export_package_tree(&root, &root.join("x.glint"), &limits).unwrap_err();
    assert_eq!(error.code(), ErrorCode::PackDestinationInvalid);
}
