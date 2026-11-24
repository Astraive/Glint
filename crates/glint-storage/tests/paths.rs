//! Path resolution (INS-001): expected per-OS locations, overrides honoured,
//! no literal user names, and nothing derived outside the resolved roots.
//!
//! Every layout is computed from synthetic base directories inside a
//! temporary directory, so the Windows, macOS and XDG expectations all run on
//! the Windows-only test host. The live OS answer is checked against the
//! `dirs` crate on whatever host runs the test.
//!
//! Run with `cargo test -p glint-storage --test paths`.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use glint_storage::{
    BaseDirectories, ErrorCode, GlintPaths, HOME_OVERRIDE_ENV, PathSource, PlatformLayout,
};

fn synthetic_base(root: &Path) -> BaseDirectories {
    BaseDirectories {
        config: root.join("base-config"),
        data_local: root.join("base-data"),
        cache: root.join("base-cache"),
    }
}

fn no_env(_: &str) -> Option<OsString> {
    None
}

#[test]
fn windows_layout_uses_roaming_config_and_local_data_and_cache() {
    let tmp = tempfile::tempdir().unwrap();
    let base = synthetic_base(tmp.path());
    let paths = GlintPaths::from_base(PlatformLayout::Windows, &base).unwrap();
    assert_eq!(paths.config_dir(), base.config.join("Glint"));
    assert_eq!(paths.data_dir(), base.data_local.join("Glint").join("data"));
    assert_eq!(paths.cache_dir(), base.cache.join("Glint").join("cache"));
    assert_eq!(
        paths.config_file(),
        base.config.join("Glint").join("config.toml")
    );
    assert_eq!(paths.source(), PathSource::Os);
}

#[test]
fn macos_layout_uses_application_support_and_caches() {
    let tmp = tempfile::tempdir().unwrap();
    let base = synthetic_base(tmp.path());
    let paths = GlintPaths::from_base(PlatformLayout::MacOs, &base).unwrap();
    assert_eq!(paths.config_dir(), base.config.join("Glint"));
    assert_eq!(
        paths.config_file(),
        base.config.join("Glint").join("config.toml")
    );
    assert_eq!(paths.data_dir(), base.data_local.join("Glint").join("data"));
    assert_eq!(paths.cache_dir(), base.cache.join("Glint"));
}

#[test]
fn xdg_layout_uses_lowercase_app_dir_under_each_base() {
    let tmp = tempfile::tempdir().unwrap();
    let base = synthetic_base(tmp.path());
    let paths = GlintPaths::from_base(PlatformLayout::Xdg, &base).unwrap();
    assert_eq!(paths.config_dir(), base.config.join("glint"));
    assert_eq!(
        paths.config_file(),
        base.config.join("glint").join("config.toml")
    );
    assert_eq!(paths.data_dir(), base.data_local.join("glint"));
    assert_eq!(paths.cache_dir(), base.cache.join("glint"));
}

#[test]
fn relative_base_directories_are_refused() {
    let base = BaseDirectories {
        config: PathBuf::from("relative/config"),
        data_local: PathBuf::from("relative/data"),
        cache: PathBuf::from("relative/cache"),
    };
    for layout in [
        PlatformLayout::Windows,
        PlatformLayout::MacOs,
        PlatformLayout::Xdg,
    ] {
        let error = GlintPaths::from_base(layout, &base).unwrap_err();
        assert_eq!(error.code(), ErrorCode::PathNotAbsolute);
    }
}

#[test]
fn host_layout_matches_the_platform_library() {
    let expected_layout = if cfg!(windows) {
        PlatformLayout::Windows
    } else if cfg!(target_os = "macos") {
        PlatformLayout::MacOs
    } else {
        PlatformLayout::Xdg
    };
    assert_eq!(PlatformLayout::current(), expected_layout);

    // A host without a home directory (some CI sandboxes) reports
    // PathUnavailable instead of guessing; that is the correct answer there.
    let Ok(base) = BaseDirectories::from_os() else {
        return;
    };
    let paths = GlintPaths::resolve_with_env(no_env).unwrap();
    assert_eq!(paths.source(), PathSource::Os);
    assert_eq!(
        paths,
        GlintPaths::from_base(PlatformLayout::current(), &base).unwrap()
    );
    assert!(paths.config_dir().starts_with(&base.config));
    assert!(paths.data_dir().starts_with(&base.data_local));
    assert!(paths.cache_dir().starts_with(&base.cache));
    for root in paths.roots() {
        assert!(root.is_absolute(), "{}", root.display());
    }
}

#[test]
fn env_override_puts_every_root_under_it() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("portable");
    let env = |name: &str| (name == HOME_OVERRIDE_ENV).then(|| home.clone().into_os_string());
    let paths = GlintPaths::resolve_with_env(env).unwrap();
    assert_eq!(paths.source(), PathSource::EnvOverride);
    assert_eq!(paths.config_dir(), home.join("config"));
    assert_eq!(paths.data_dir(), home.join("data"));
    assert_eq!(paths.cache_dir(), home.join("cache"));
    assert_eq!(
        paths.database_file(),
        home.join("data").join("glint.sqlite3")
    );
}

#[test]
fn relative_env_override_is_an_error_not_a_cwd_path() {
    let env = |name: &str| (name == HOME_OVERRIDE_ENV).then(|| OsString::from("relative/home"));
    let error = GlintPaths::resolve_with_env(env).unwrap_err();
    assert_eq!(error.code(), ErrorCode::PathNotAbsolute);
}

#[test]
fn empty_env_override_counts_as_unset() {
    let env = |name: &str| (name == HOME_OVERRIDE_ENV).then(OsString::new);
    match GlintPaths::resolve_with_env(env) {
        Ok(paths) => assert_eq!(paths.source(), PathSource::Os),
        Err(error) => assert_eq!(error.code(), ErrorCode::PathUnavailable),
    }
}

#[test]
fn explicit_root_must_be_absolute() {
    assert_eq!(
        GlintPaths::under_root("relative").unwrap_err().code(),
        ErrorCode::PathNotAbsolute
    );
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path()).unwrap();
    assert_eq!(paths.source(), PathSource::ExplicitRoot);
}

#[test]
fn every_derived_location_is_inside_the_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path()).unwrap();
    for derived in [
        paths.config_file(),
        paths.database_file(),
        paths.active_record_file(),
        paths.store_dir(),
        paths.staging_dir(),
        paths.trash_dir(),
        paths.installer_lock_file(),
        paths.instance_lock_file(),
        paths.instance_owner_file(),
    ] {
        assert!(paths.contains(&derived), "{}", derived.display());
        paths.check_within(&derived).unwrap();
    }
    // Store, staging and trash share the data volume so moves are renames.
    assert!(paths.store_dir().starts_with(paths.data_dir()));
    assert!(paths.staging_dir().starts_with(paths.data_dir()));
    assert!(paths.trash_dir().starts_with(paths.data_dir()));
}

#[test]
fn paths_outside_the_roots_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path().join("glint")).unwrap();
    let outside = tmp.path().join("elsewhere").join("file");
    let escaping = paths.data_dir().join("..").join("..").join("escape");
    for path in [outside, escaping, PathBuf::from("relative/data")] {
        assert!(!paths.contains(&path), "{}", path.display());
        assert_eq!(
            paths.check_within(&path).unwrap_err().code(),
            ErrorCode::PathOutsideRoots
        );
    }
}

#[test]
fn ensure_dirs_creates_only_inside_the_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("glint-home");
    let paths = GlintPaths::under_root(&root).unwrap();
    paths.ensure_dirs().unwrap();
    for dir in [
        paths.config_dir().to_path_buf(),
        paths.data_dir().to_path_buf(),
        paths.cache_dir().to_path_buf(),
        paths.store_dir(),
        paths.staging_dir(),
        paths.trash_dir(),
    ] {
        assert!(dir.is_dir(), "{}", dir.display());
    }
    // Nothing was created next to the root.
    let siblings: Vec<_> = fs::read_dir(tmp.path()).unwrap().collect();
    assert_eq!(siblings.len(), 1);
    // Idempotent.
    paths.ensure_dirs().unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(paths.staging_dir())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o077,
            0,
            "staging must not be group/world accessible"
        );
    }
}

#[test]
fn ensure_dirs_refuses_a_file_where_a_directory_belongs() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path()).unwrap();
    fs::create_dir_all(paths.data_dir()).unwrap();
    fs::write(paths.store_dir(), b"not a directory").unwrap();
    let error = paths.ensure_dirs().unwrap_err();
    assert!(
        matches!(error.code(), ErrorCode::Io | ErrorCode::PathNotDirectory),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
fn ensure_dirs_refuses_a_symlinked_staging_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path().join("home")).unwrap();
    fs::create_dir_all(paths.data_dir()).unwrap();
    let elsewhere = tmp.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, paths.staging_dir()).unwrap();
    assert_eq!(
        paths.ensure_dirs().unwrap_err().code(),
        ErrorCode::PathNotDirectory
    );
}

/// The resolver source must never embed a user name, home directory or
/// drive-letter profile path; only the platform library answers those.
#[test]
fn resolver_source_has_no_literal_user_paths() {
    let source = include_str!("../src/paths.rs");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "C:\\\\Users",
        "C:/Users",
        "/home/",
        "/Users/",
        "AppData",
        "Library/",
        "USERPROFILE",
        "\"HOME\"",
        "USERNAME",
    ] {
        assert!(
            !code.contains(forbidden),
            "paths.rs code contains {forbidden:?}"
        );
    }
}
