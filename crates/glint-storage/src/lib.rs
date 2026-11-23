//! Glint's local state (INS-001).
//!
//! [`GlintPaths`] resolves Glint's configuration, data and cache directories
//! from the operating system (or an explicit override) and derives every
//! file location Glint writes: the state database, the package store,
//! private staging and the installer lock. Nothing in Glint hardcodes a home
//! directory or user name.
//!
//! # Errors
//!
//! Every failure is a [`StorageError`] with a stable [`ErrorCode`] whose
//! string form is `GLINT_STORAGE_*`, and an [`ErrorCategory`] that separates
//! unsupported, permission-denied, unavailable, timeout and other failures.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod paths;

pub use error::{ErrorCategory, ErrorCode, StorageError};
pub use paths::{
    BaseDirectories, GlintPaths, HOME_OVERRIDE_ENV, PathSource, PlatformLayout, is_real_dir,
};
